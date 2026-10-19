#!/bin/bash
# Publishes a public release of SpecBoard on GitHub: builds it from the published source tree,
# signs it with Developer ID, notarizes and staples the DMG, pushes main and the version tag
# to GitHub, and makes a GitHub Release with the DMG and its .sha256.
#
#   bash tools/publish-github.sh <version> <commit>
#   bash tools/publish-github.sh --check <version> <commit>    prerequisites and source only
#
# Only the maintainer runs this, on their own Mac, inside a clone of the published source tree
# (its remote `origin`), after the version has been published there. <commit> is the commit
# the version was published as; the tag `v<version>` fetched from origin must point at it.
# People building SpecBoard from source do not need this script. Its messages are in Japanese
# because the maintainer reads them; the steps and what to do when one stops are described in
# specboard://SpecBoard/Docs/Design:Release.
#
# Steps, each stopping with what to do when something is missing or does not match:
#   1. prerequisites: Apple Silicon, Xcode 26 or later, rustup, zig and cargo-zigbuild, gh signed
#      in with push access to the GitHub repository, one Developer ID Application identity in the
#      keychain (or SPECBOARD_SIGN_IDENTITY), a notarytool keychain profile (SPECBOARD_NOTARY_PROFILE,
#      default specboard-notary), and a clone without changes. All missing ones are reported at once.
#   2. source: the tag v<version> from origin points at <commit>, origin/main contains it, VERSION is
#      <version> and CHANGELOG.md has a "## <version>" section. --check ends here.
#   3. GitHub: a published Release v<version> with its DMG means the work is done.
#   4. build and notarize in ~/Library/Caches/SpecBoard-publish/<version>/, outside the clone.
#   5. push <commit> to GitHub's main (fast-forward only) and the tag as it is.
#   6. a draft Release with the DMG and .sha256, published as the latest; then the DMG is
#      downloaded again and its SHA-256 compared.
#
# A re-run after a stop picks up where it stopped: a notarized DMG of the same commit is not
# built again, pushes already made are not made again, and assets already on a draft are kept.
#
# Never: force-push, delete tags or releases, replace an asset of a published release, push to
# origin, change files of the clone, change the global git configuration, or print a password or
# token. Credentials for the push come from gh, for that push only.
# Written for macOS's bash 3.2.
set -euo pipefail

github_repo=abratan-a/SpecBoard
github_url=https://github.com/$github_repo.git
profile=${SPECBOARD_NOTARY_PROFILE:-specboard-notary}

usage() {
  echo "使い方: bash tools/publish-github.sh [--check] <版> <コミット>" >&2
  exit 2
}

check_only=""
args=()
for arg in "$@"; do
  case $arg in
    --check) check_only=1 ;;
    -*) usage ;;
    *) args+=("$arg") ;;
  esac
done
[[ ${#args[@]} -eq 2 ]] || usage
ver=${args[0]}
commit_arg=${args[1]}
tag=v$ver
dmg_name=SpecBoard-$ver.dmg

say() { printf '%s\n' "$*"; }
step() { printf '\n== %s\n' "$*"; }
# stop <what happened> [<what to do>...]
stop() {
  printf '\n止まりました: %s\n' "$1" >&2
  shift
  local line
  for line in "$@"; do printf '  → %s\n' "$line" >&2; done
  printf '直したら、同じコマンドをもう一度流してください（済んだ段は飛ばします）。\n' >&2
  exit 1
}

[[ $ver =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || stop "版は X.Y.Z の形で渡してください（渡されたのは ${ver}）"
[[ $commit_arg =~ ^[0-9a-f]{7,40}$ ]] || stop "コミットは 16 進のハッシュで渡してください（渡されたのは ${commit_arg}）"

root=$(cd "$(dirname "$0")/.." && pwd -P)
top=$(git -C "$root" rev-parse --show-toplevel 2>/dev/null || true)
[[ -n $top && $(cd "$top" && pwd -P) == "$root" ]] \
  || stop "この道具は、公開する木の clone の中の tools/publish-github.sh として流してください（${root} は git の clone ではありません）"

# --- 1. prerequisites: collect every missing one, then report them together ---
problems=()
need() { problems+=("$1"$'\n'"    → $2"); }

step "1. 準備"
if [[ $(uname -s) != Darwin || $(uname -m) != arm64 ]]; then
  need "Apple Silicon の Mac（arm64）で流してください（今: $(uname -s) $(uname -m)）" "Apple Silicon の Mac で流す"
fi

developer=$(xcode-select -p 2>/dev/null || true)
if [[ $developer != *.app/Contents/Developer ]]; then
  need "xcode-select が Xcode.app を指していません（今: ${developer:-無し}）" "sudo xcode-select -s /Applications/Xcode.app"
else
  xcode=$(xcodebuild -version 2>/dev/null | awk 'NR == 1 { print $2 }' || true)
  major=${xcode%%.*}
  if [[ ! $major =~ ^[0-9]+$ ]] || (( major < 26 )); then
    need "Xcode 26 以上が要ります（今: ${xcode:-読めない}。ライセンスに同意していないときも読めません）" \
      "App Store で Xcode を更新し、sudo xcodebuild -license accept"
  fi
fi

# The same lookup as the build phases (cargo, rustup, zig, cargo-zigbuild).
source "$root/tools/build-env.sh"
if [[ -z $CARGO ]]; then
  need "Rust がありません（rustup で入れます）" "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
elif [[ -z $RUSTUP ]]; then
  need "cargo（${CARGO}）の隣に rustup がありません（Rust の版は rust-toolchain.toml で決まり、rustup が入れます。Homebrew の rust では組めません）" \
    "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi
if [[ -z $ZIG || -z $ZIGBUILD ]]; then
  need "zig と cargo-zigbuild がありません（Linux 用のエージェントを組む）" "brew install zig cargo-zigbuild"
fi

gh=$(command -v gh 2>/dev/null || true)
[[ -n $gh || ! -x /opt/homebrew/bin/gh ]] || gh=/opt/homebrew/bin/gh
if [[ -z $gh ]]; then
  need "gh（GitHub CLI）がありません" "brew install gh のあと gh auth login --hostname github.com --git-protocol https --web"
elif ! "$gh" auth status --hostname github.com >/dev/null 2>&1; then
  need "gh が github.com にログインしていません" "gh auth login --hostname github.com --git-protocol https --web"
else
  push=$("$gh" api "repos/$github_repo" --jq .permissions.push 2>/dev/null || true)
  if [[ $push != true ]]; then
    need "gh のアカウントから ${github_repo} に push できません（${push:-リポジトリが読めない}）" \
      "gh auth status で使っているアカウントを見て、${github_repo} に push できるアカウントで gh auth login をやり直す"
  fi
fi

identities=$(security find-identity -v -p codesigning 2>/dev/null \
  | sed -n 's/^ *[0-9][0-9]*) [0-9A-F]* "\(Developer ID Application: .*\)"$/\1/p' | sort -u || true)
if [[ -n ${SPECBOARD_SIGN_IDENTITY:-} ]]; then
  identity=$(printf '%s\n' "$identities" | grep -Fx -- "$SPECBOARD_SIGN_IDENTITY" || true)
  [[ -n $identity ]] || need "SPECBOARD_SIGN_IDENTITY の証明書（${SPECBOARD_SIGN_IDENTITY}）がキーチェーンにありません" \
    "security find-identity -v -p codesigning で名前を見て、\"Developer ID Application: …\" の全体を渡す"
else
  count=$(printf '%s' "$identities" | grep -c . || true)
  identity=""
  if (( count == 0 )); then
    need "キーチェーンに Developer ID Application の証明書がありません" \
      "Xcode → Settings → Accounts → チームを選んで Manage Certificates… → 左下の + → Developer ID Application（確かめ: security find-identity -v -p codesigning）"
  elif (( count > 1 )); then
    need "Developer ID Application の証明書がいくつもあります: $(printf '%s' "$identities" | tr '\n' '|' | sed 's/|/ ・ /g')" \
      "SPECBOARD_SIGN_IDENTITY='<使う証明書の名前の全体>' を付けて流す"
  else
    identity=$identities
  fi
fi
team=""
if [[ -n $identity ]]; then
  team=$(printf '%s\n' "$identity" | sed -n 's/.*(\([A-Z0-9]\{10\}\))$/\1/p')
  [[ -n $team ]] || need "証明書の名前からチーム ID が読めません（${identity}）" "security find-identity -v -p codesigning で名前を見る"
fi

if ! xcrun notarytool history --keychain-profile "$profile" >/dev/null 2>&1; then
  need "公証の資格（notarytool のキーチェーンのプロファイル ${profile}）が使えません（ネットに繋がっていないときもここで止まります）" \
    "xcrun notarytool store-credentials ${profile} --apple-id <Apple ID のメール> --team-id ${team:-<チーム ID>}（パスワードを聞かれたら App 用パスワードを貼る。確かめ: xcrun notarytool history --keychain-profile ${profile}）"
fi

changes=$(git -C "$root" status --porcelain 2>/dev/null || echo "?")
if [[ -n $changes ]]; then
  need "clone に変更があります（${root}）" "git -C ${root} status で見て、変更を片付ける（この道具は clone のファイルを書き換えません）"
fi

# --- 2. source: the published tag points at the commit the maintainer was given ---
step "2. 公開する木（origin）"
before=${#problems[@]}
commit=""
if ! git -C "$root" remote get-url origin >/dev/null 2>&1; then
  need "clone にリモート origin がありません" "公開する木を clone し直す（手順は Docs/Rules:PublishGithub）"
elif ! git -C "$root" fetch --quiet --tags origin 2>/dev/null; then
  need "origin からタグを取れません（ネットか、手元のタグが origin と違う）" "git -C ${root} fetch --tags origin を流して、出た理由を見る"
else
  commit=$(git -C "$root" rev-parse -q --verify "${commit_arg}^{commit}" 2>/dev/null || true)
  tagged=$(git -C "$root" rev-parse -q --verify "refs/tags/${tag}^{commit}" 2>/dev/null || true)
  if [[ -z $tagged ]]; then
    need "origin にタグ ${tag} がありません" "版を確かめる（公開する木に出す前なら、セッションが出し終えるのを待つ）"
  fi
  if [[ -z $commit ]]; then
    need "コミット ${commit_arg} が origin にありません" "セッションが伝えたハッシュを確かめる"
  elif [[ -n $tagged && $tagged != "$commit" ]]; then
    need "タグ ${tag} は ${tagged:0:12} を指していて、渡されたコミット ${commit:0:12} と違います" \
      "セッションに版とハッシュを確かめてもらう（違うものを公開しないために止めています）"
  fi
  if [[ -n $commit ]]; then
    if ! git -C "$root" merge-base --is-ancestor "$commit" refs/remotes/origin/main 2>/dev/null; then
      need "コミット ${commit:0:12} が origin/main に含まれていません" "セッションに伝えたハッシュを確かめてもらう"
    fi
    version=$(git -C "$root" show "$commit:VERSION" 2>/dev/null | tr -d ' \t\r\n' || true)
    [[ $version == "$ver" ]] || need "コミットの VERSION が ${version:-無し} で、版 ${ver} と違います" "セッションに版を確かめてもらう"
    changelog=$(git -C "$root" show "$commit:CHANGELOG.md" 2>/dev/null || true)
    if ! grep -qx "## $ver" <<<"$changelog"; then
      need "コミットの CHANGELOG.md に「## ${ver}」の節がありません" "セッションに伝える（GitHub の Release のノートにする節です）"
    fi
  fi
fi

if (( ${#problems[@]} == before )); then
  say "${tag} は ${commit:0:12} を指し、origin/main に含まれ、VERSION と CHANGELOG.md も ${ver} です"
fi

if (( ${#problems[@]} > 0 )); then
  printf '\n足りないもの・合わないもの（%d）:\n' "${#problems[@]}" >&2
  for problem in "${problems[@]}"; do printf -- '- %s\n' "$problem" >&2; done
  printf '\n直したら、同じコマンドをもう一度流してください。\n' >&2
  exit 1
fi
say "準備は揃っています。署名: ${identity}（チーム ${team}）・公証: ${profile}"
if [[ -n $check_only ]]; then
  say ""
  say "--check なので、ここで終わります（組まず、push もしません）。"
  exit 0
fi

cache="$HOME/Library/Caches/SpecBoard-publish/$ver"
out="$cache/out"
dmg="$out/$dmg_name"
mkdir -p "$cache"
# Credentials for GitHub from gh, for these git commands only; other credential helpers are
# not asked.
cred=(-c credential.helper= -c "credential.helper=!\"$gh\" auth git-credential")

remote_ref() {  # <ref> → its object on GitHub, or nothing
  git -C "$root" "${cred[@]}" ls-remote "$github_url" "$1" | awk '{ print $1 }'
}

# Whether GitHub already has <commit> on main and the same tag object; stops when it has
# something else.
pushed=""
check_pushes() {
  local main tag_object local_tag
  pushed=1
  main=$(remote_ref refs/heads/main) || stop "GitHub の ${github_repo} を読めません" "gh auth status を見る"
  if [[ -z $main ]]; then
    pushed=""
  elif [[ $main != "$commit" ]] && ! git -C "$root" merge-base --is-ancestor "$commit" "$main" 2>/dev/null; then
    if git -C "$root" cat-file -e "${main}^{commit}" 2>/dev/null \
        && git -C "$root" merge-base --is-ancestor "$main" "$commit"; then
      pushed=""
    else
      stop "GitHub の main（${main:0:12}）から ${commit:0:12} へ進めません（公開する木に無いコミットがあります）" \
        "GitHub の ${github_repo} の main を見る。force push はしません"
    fi
  fi
  local_tag=$(git -C "$root" rev-parse "refs/tags/$tag")
  tag_object=$(remote_ref "refs/tags/$tag")
  if [[ -z $tag_object ]]; then
    pushed=""
  elif [[ $tag_object != "$local_tag" ]]; then
    stop "GitHub のタグ ${tag} が、公開する木のタグと違うものを指しています" \
      "GitHub の ${github_repo} のタグ ${tag} を見る。タグは消しも付け替えもしません"
  fi
}

# --- 3. GitHub: a published release with its DMG means there is nothing left to do ---
step "3. GitHub の今"
release_state() {  # → none, draft or published
  local err="$cache/gh-view.err" draft
  if draft=$("$gh" release view "$tag" -R "$github_repo" --json isDraft --jq .isDraft 2>"$err"); then
    [[ $draft == true ]] && echo draft || echo published
  elif grep -qi "not found" "$err"; then
    echo none
  else
    stop "GitHub の Release ${tag} を読めません: $(head -1 "$err")" "gh auth status を見る"
  fi
}
release_assets() {  # → "<name>\t<size>" per asset
  "$gh" release view "$tag" -R "$github_repo" --json assets --jq '.assets[] | "\(.name)\t\(.size)"'
}
release_url() { "$gh" release view "$tag" -R "$github_repo" --json url --jq .url; }

state=$(release_state)
if [[ $state == published ]] && grep -qx "$dmg_name" <<<"$(release_assets | cut -f1)"; then
  check_pushes
  [[ -n $pushed ]] || stop "Release ${tag} は公開済みなのに、GitHub に main かタグがありません" "GitHub の ${github_repo} を見る"
  say "済んでいます: $(release_url)"
  exit 0
fi
say "Release ${tag}: $([[ $state == none ]] && echo 無し || echo "$state")"

# --- 4. build, sign, notarize and staple, in the cache, never in the clone ---
step "4. 組む・公証する（${cache}）"
sha_of() { shasum -a 256 "$1" | awk '{ print $1 }'; }
stamp_built="$cache/built"      # <commit>: the DMG in out/ is built from it and signed
stamp_stapled="$cache/stapled"  # <commit> <sha256>: that DMG is notarized and stapled

check_signature() {  # <file> <what it is> [code]: code is also held to Hardened Runtime
  local info
  info=$(codesign -dvv "$1" 2>&1) || stop "${2} の署名が読めません"
  grep -q "^Authority=Developer ID Application: " <<<"$info" || stop "${2} の署名が Developer ID Application ではありません"
  grep -q "^TeamIdentifier=${team}$" <<<"$info" || stop "${2} の署名のチームが ${team} ではありません"
  grep -q "^Timestamp=" <<<"$info" || stop "${2} の署名に安全なタイムスタンプがありません（公証で断られます）"
  if [[ ${3:-} == code ]]; then
    grep -Eq "^CodeDirectory .*flags=0x[0-9a-f]+\([^)]*runtime" <<<"$info" || stop "${2} の署名が Hardened Runtime ではありません"
  fi
}

check_app() {  # the app inside the DMG: Developer ID, secure timestamp and Hardened Runtime throughout
  local mount="$cache/mount"
  rm -rf "$mount"
  mkdir -p "$mount"
  hdiutil attach -quiet -nobrowse -readonly -noautoopen -mountpoint "$mount" "$dmg" || stop "DMG を開けません（${dmg}）"
  local app="$mount/SpecBoard.app" rc=0
  (
    set -e
    codesign --verify --deep --strict "$app" || stop "アプリの署名が壊れています"
    check_signature "$app" "アプリ" code
    check_signature "$app/Contents/Frameworks/libspecboard_ffi.dylib" "コアの dylib" code
    check_signature "$app/Contents/MacOS/specboard-agent" "エージェント" code
    [[ -f $app/Contents/Resources/specboard-agent-linux-x86_64 ]] || stop "Linux 用のエージェントが入っていません"
  ) || rc=$?
  hdiutil detach -quiet "$mount" || true
  (( rc == 0 )) || exit "$rc"
}

stapled=""
if [[ -f $stamp_stapled && -f $dmg ]] && [[ $(cat "$stamp_stapled") == "$commit $(sha_of "$dmg")" ]]; then
  stapled=1
  say "公証まで済んだ DMG があるので、組み直しません"
fi

if [[ -z $stapled ]]; then
  if [[ -f $stamp_built && -f $dmg && $(cat "$stamp_built") == "$commit" ]]; then
    say "署名した DMG があるので、組み直さずに公証へ進みます"
  else
    rm -f "$stamp_built" "$stamp_stapled"
    rm -rf "$cache/src" "$out"
    mkdir -p "$cache/src" "$out"
    git -C "$root" archive --format=tar "$commit" | tar -x -C "$cache/src"
    build=$(TZ=Asia/Tokyo date +%Y%m%d%H%M)
    say "組みます（版 ${ver}・ビルド番号 ${build}。10〜20 分かかります）"
    BUILD_NUMBER=$build \
      SPECBOARD_SIGN_IDENTITY="$identity" SPECBOARD_TEAM="$team" SPECBOARD_SIGN_TIMESTAMP=1 \
      SPECBOARD_UPDATE_FEED_URL=https://github.com SPECBOARD_UPDATE_FEED_REPO="$github_repo" \
      bash "$cache/src/tools/build-dmg.sh" "$out" \
      || stop "ビルドに失敗しました" "上に出たエラーを見る（直せなければ、セッションに渡す）"
    [[ -f $dmg ]] || stop "DMG ができていません（${dmg}）"
    check_app
    codesign --force --sign "$identity" --timestamp "$dmg" || stop "DMG に署名できません"
    check_signature "$dmg" "DMG"
    echo "$commit" > "$stamp_built"
  fi

  say "公証に出します（数分〜十数分かかります）"
  result="$cache/notary.json"
  xcrun notarytool submit "$dmg" --keychain-profile "$profile" --wait --output-format json > "$result" || true
  status=$(plutil -extract status raw -o - "$result" 2>/dev/null || true)
  submission=$(plutil -extract id raw -o - "$result" 2>/dev/null || true)
  if [[ $status != Accepted ]]; then
    if [[ -n $submission ]]; then
      say "公証の記録（${submission}）:"
      xcrun notarytool log "$submission" --keychain-profile "$profile" || true
    else
      cat "$result" >&2 || true
    fi
    stop "公証で通りませんでした（${status:-答えが読めない}）" "上の記録の issues を見る（セッションに渡せば直し方を調べます）"
  fi
  xcrun stapler staple -q "$dmg" || stop "DMG に公証の票を付けられません"
  xcrun stapler validate -q "$dmg" || stop "DMG の公証の票を確かめられません"
  spctl -a -t open --context context:primary-signature "$dmg" 2>/dev/null \
    || stop "Gatekeeper が DMG を通しません（spctl -a -t open --context context:primary-signature -vv ${dmg}）"
  echo "$commit $(sha_of "$dmg")" > "$stamp_stapled"
fi
sha=$(sha_of "$dmg")
(cd "$out" && printf '%s  %s\n' "$sha" "$dmg_name" > "$dmg_name.sha256")
say "DMG: ${dmg}（SHA-256 ${sha}）"

# --- 5. push main and the tag to GitHub ---
step "5. GitHub へ push"
remote=$(git -C "$root" config --get remote.github.url 2>/dev/null || true)
if [[ -z $remote ]]; then
  git -C "$root" remote add github "$github_url"
elif [[ $remote != "$github_url" ]]; then
  stop "リモート github が ${remote} を指しています（期待は ${github_url}）" "git -C ${root} remote set-url github ${github_url}"
fi
check_pushes
main=$(remote_ref refs/heads/main)
if [[ -z $main ]] || { [[ $main != "$commit" ]] && ! git -C "$root" merge-base --is-ancestor "$commit" "$main" 2>/dev/null; }; then
  git -C "$root" "${cred[@]}" push --quiet github "$commit:refs/heads/main" || stop "main を push できません"
  say "main を ${commit:0:12} にしました"
else
  say "main は済んでいます"
fi
if [[ -z $(remote_ref "refs/tags/$tag") ]]; then
  git -C "$root" "${cred[@]}" push --quiet github "refs/tags/$tag:refs/tags/$tag" || stop "タグ ${tag} を push できません"
  say "タグ ${tag} を push しました"
else
  say "タグ ${tag} は済んでいます"
fi

# --- 6. the GitHub Release ---
step "6. GitHub の Release"
notes="$cache/notes.md"
# Read whole before awk stops at the next section: an early stop would break the pipe.
changelog=$(git -C "$root" show "$commit:CHANGELOG.md")
awk -v heading="## $ver" '$0 == heading { on = 1; next } on && /^## / { exit } on { print }' <<<"$changelog" > "$notes"
grep -q '[^[:space:]]' "$notes" || stop "CHANGELOG.md の「## ${ver}」の節が空です" "セッションに伝える"

state=$(release_state)
if [[ $state == none ]]; then
  "$gh" release create "$tag" -R "$github_repo" --draft --verify-tag --title "SpecBoard $ver" --notes-file "$notes" >/dev/null \
    || stop "Release ${tag} を作れません"
  state=draft
  say "Release ${tag} を下書きで作りました"
fi

asset_sha() {  # <name> → SHA-256 of that asset, downloaded from the release
  local dir
  dir=$(mktemp -d "$cache/download.XXXXXX")
  "$gh" release download "$tag" -R "$github_repo" -p "$1" -D "$dir" >/dev/null || { rm -rf "$dir"; return 1; }
  sha_of "$dir/$1"
  rm -rf "$dir"
}

for file in "$dmg" "$dmg.sha256"; do
  name=$(basename "$file")
  if ! grep -qx "$name" <<<"$(release_assets | cut -f1)"; then
    "$gh" release upload "$tag" -R "$github_repo" "$file" >/dev/null || stop "${name} を載せられません"
    say "${name} を載せました"
    continue
  fi
  there=$(asset_sha "$name") || stop "Release の ${name} を取れません"
  if [[ $there == "$(sha_of "$file")" ]]; then
    say "${name} は載っています"
  elif [[ $state == draft ]]; then
    # A draft is not visible to anyone yet: what it holds from an earlier run is replaced.
    "$gh" release upload "$tag" -R "$github_repo" "$file" --clobber >/dev/null || stop "${name} を載せ直せません"
    say "下書きの ${name} を、今の DMG のものに載せ直しました"
  else
    stop "公開済みの Release ${tag} の ${name} が、今の DMG と違います" \
      "公開済みの添付は差し替えません。GitHub の ${github_repo} の Release ${tag} を見て、セッションに伝える"
  fi
done

if [[ $state == draft ]]; then
  "$gh" release edit "$tag" -R "$github_repo" --draft=false --latest >/dev/null || stop "Release ${tag} を公開できません"
  say "Release ${tag} を公開しました（最新）"
fi

there=$(asset_sha "$dmg_name") || stop "公開した Release から ${dmg_name} を取れません"
[[ $there == "$sha" ]] || stop "公開した Release の ${dmg_name} が、組んだ DMG と違います（${there}）" "GitHub の Release を見て、セッションに伝える"

say ""
say "済みました: $(release_url)"
