#!/bin/bash
# Checks a built app before it goes out (tools/build-dmg.sh runs it on the app it built):
# - the signature is intact and has no debugger entitlement (get-task-allow);
# - every Mach-O file in it (the app, the core and the macOS agent) has its LINKEDIT string table on an
#   8-byte boundary (tools/check-linkedit.sh), no debug symbol table (nm's SO and OSO entries hold the
#   source and object file paths of the build machine), no code coverage instrumentation (the __llvm_prf
#   and __llvm_cov sections hold counters and function names with source paths), and none of the build
#   paths given;
# - the macOS agent is arm64 only and signed by the same team as the app; the Linux agent is a statically
#   linked x86_64 ELF file without the build paths.
# A tool that cannot read a file (nm, otool, codesign, lipo) stops the check: what it could not read is
# not taken as clean.
#
#   bash tools/check-app.sh <SpecBoard.app> [<build path>...]
#
# Written for macOS's bash 3.2.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
app="${1:?give the app to check}"
shift
places=("$@")
fail() {
  echo "$1" >&2
  exit 1
}

codesign --verify --deep --strict "$app" || fail "the signature of the app does not verify"
entitlements=$(codesign -d --entitlements - --xml "$app" 2>/dev/null) \
  || fail "codesign cannot read the entitlements of the app"
if grep "get-task-allow" > /dev/null <<< "$entitlements"; then
  fail "get-task-allow is set"
fi

# Every Mach-O file in the app: the app, the core and the macOS agent (the Linux agent is an ELF file in
# Resources)
machos=()
while IFS= read -r file; do
  kind=$(file -b "$file") || fail "file cannot read ${file}"
  if [[ $kind == Mach-O* ]]; then machos+=("$file"); fi
done < <(find "$app/Contents/MacOS" "$app/Contents/Frameworks" -type f)
[[ ${#machos[@]} -ge 3 ]] || fail "expected the app, the core and the agent in the app, found: ${machos[*]-none}"
bash "$root/tools/check-linkedit.sh" "${machos[@]}"

# A file holding a build path, any kind of file
no_paths() {  # <what> <file>
  local place
  for place in ${places[@]+"${places[@]}"}; do
    if LC_ALL=C grep -aqF "$place" "$2"; then
      fail "${1} holds a build path: ${place}"
    fi
  done
}

for file in "${machos[@]}"; do
  what=${file#"$app/"}
  symbols=$(nm -ap "$file") || fail "nm cannot read ${what}"
  if grep -E " (SO|OSO) " > /dev/null <<< "$symbols"; then
    fail "the debug symbol table is left in ${what} (it holds the build paths)"
  fi
  commands=$(otool -l "$file") || fail "otool cannot read ${what}"
  [[ -n $commands ]] || fail "otool says nothing of ${what}"
  if grep -E "sectname __llvm_(prf|cov)" > /dev/null <<< "$commands"; then
    fail "code coverage instrumentation is left in ${what}"
  fi
  no_paths "$what" "$file"
done

# The agent the app installs on hosts over SSH (tools/build-agent.sh): arm64 only, signed with the app's
# certificate
agent="$app/Contents/MacOS/specboard-agent"
[[ -f $agent ]] || fail "the agent is missing"
archs=$(lipo -archs "$agent") || fail "lipo cannot read the agent"
[[ $archs == arm64 ]] || fail "the agent is not arm64 only: ${archs}"
codesign --verify --strict "$agent" || fail "the signature of the agent does not verify"
team() {  # <file>: its TeamIdentifier line, "not set" for an ad-hoc signature
  local said
  said=$(codesign -dv "$1" 2>&1) || fail "codesign cannot read the signature of ${1#"$app/"}"
  said=$(grep -E "^TeamIdentifier=" <<< "$said") || fail "codesign says no team of ${1#"$app/"}"
  echo "$said"
}
agent_team=$(team "$agent")
app_team=$(team "$app")
[[ $agent_team == "$app_team" ]] || fail "the agent is not signed like the app"

# The Linux agent (tools/build-agent.sh): a statically linked x86_64 ELF, no build paths
linux="$app/Contents/Resources/specboard-agent-linux-x86_64"
[[ -f $linux ]] || fail "the Linux agent is missing"
kind=$(file -b "$linux") || fail "file cannot read the Linux agent"
[[ $kind == "ELF 64-bit LSB executable, x86-64"*"statically linked"* ]] \
  || fail "the Linux agent is not a static x86_64 executable: ${kind}"
no_paths "the Linux agent" "$linux"
