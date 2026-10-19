#!/bin/bash
# Builds the DMG of a release. The maintainer's closed-beta builds run it, and so does
# tools/publish-github.sh for a public release (signed with Developer ID and notarized there).
# People building SpecBoard from source do not need it.
#
#   bash tools/build-dmg.sh <output folder>
#
# Environment:
#   BUILD_NUMBER                 the build number (twelve digits, year to minute, Japan time); becomes the
#                                app's CFBundleVersion. Required
#   SPECBOARD_UPDATE_FEED_URL    the URL of the built-in update feed (public releases)
#   SPECBOARD_UPDATE_FEED_REPO   the owner/repo of the built-in update feed
#   Give both feed values or neither. Without them the app has no built-in feed
#   (specboard://SpecBoard/Docs/Design:Release).
#   SPECBOARD_SIGN_IDENTITY      the signing certificate. Apple Development by default
#                                (tools/publish-github.sh passes Developer ID Application)
#   SPECBOARD_TEAM               the signing team. Without it, the project's setting is used
#   SPECBOARD_SIGN_TIMESTAMP     1 adds a secure timestamp to the signatures (needed for notarization;
#                                reaches Apple's timestamp server)
# The version is VERSION (CFBundleShortVersionString). It makes one file, <output folder>/SpecBoard-<version>.dmg.
#
# Signing is manual (no Xcode account), with the team's certificate in the keychain, and with the
# Hardened Runtime (needed for notarization). The Linux agent is required
# (SPECBOARD_REQUIRE_LINUX_AGENT=YES; the build stops without zig).
# The debugger entitlement (get-task-allow) that Xcode adds even to Release builds is turned off with
# CODE_SIGN_INJECT_BASE_ENTITLEMENTS=NO. Xcode strips the debug symbol table (it holds the source and
# object file paths of the build machine) before signing (DEPLOYMENT_POSTPROCESSING, STRIP_STYLE=debugging).
# The scheme turns code coverage on, so it is turned off for a release (ENABLE_CODE_COVERAGE,
# CLANG_COVERAGE_MAPPING; it would leave counters and function names with source paths in the binary).
# The build stops if either is left.
# It builds in a temporary folder and leaves nothing in the source tree. Written for macOS's bash 3.2.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:?give the output folder}"
: "${BUILD_NUMBER:?BUILD_NUMBER is not set}"
SPECBOARD_UPDATE_FEED_URL="${SPECBOARD_UPDATE_FEED_URL:-}"
SPECBOARD_UPDATE_FEED_REPO="${SPECBOARD_UPDATE_FEED_REPO:-}"
[[ -z $SPECBOARD_UPDATE_FEED_URL && -z $SPECBOARD_UPDATE_FEED_REPO || -n $SPECBOARD_UPDATE_FEED_URL && -n $SPECBOARD_UPDATE_FEED_REPO ]] \
  || { echo "give both SPECBOARD_UPDATE_FEED_URL and SPECBOARD_UPDATE_FEED_REPO, or neither" >&2; exit 1; }
[[ $BUILD_NUMBER =~ ^[0-9]{12}$ ]] || { echo "BUILD_NUMBER must be twelve digits, year to minute: ${BUILD_NUMBER}" >&2; exit 1; }
identity="${SPECBOARD_SIGN_IDENTITY:-Apple Development}"
team=()
[[ -z ${SPECBOARD_TEAM:-} ]] || team=(DEVELOPMENT_TEAM="$SPECBOARD_TEAM")
timestamp=()
case ${SPECBOARD_SIGN_TIMESTAMP:-} in
  "" | 0) ;;
  1) timestamp=(OTHER_CODE_SIGN_FLAGS=--timestamp) ;;
  *) echo "SPECBOARD_SIGN_TIMESTAMP must be 1 or empty: ${SPECBOARD_SIGN_TIMESTAMP}" >&2; exit 1 ;;
esac
version=$(tr -d ' \t\r\n' < "$root/VERSION")
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "VERSION must be X.Y.Z: ${version}" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"
log="$work/xcodebuild.log"
if ! xcodebuild -project "$root/sources/SpecBoard.xcodeproj" -scheme SpecBoard \
    -configuration Release -destination 'platform=macOS' -derivedDataPath "$work/build" \
    MARKETING_VERSION="$version" CURRENT_PROJECT_VERSION="$BUILD_NUMBER" \
    CODE_SIGN_STYLE=Manual CODE_SIGN_IDENTITY="$identity" PROVISIONING_PROFILE_SPECIFIER= ${team[@]+"${team[@]}"} \
    ${timestamp[@]+"${timestamp[@]}"} ENABLE_HARDENED_RUNTIME=YES SPECBOARD_REQUIRE_LINUX_AGENT=YES \
    CODE_SIGN_INJECT_BASE_ENTITLEMENTS=NO \
    DEPLOYMENT_POSTPROCESSING=YES STRIP_INSTALLED_PRODUCT=YES STRIP_STYLE=debugging \
    ENABLE_CODE_COVERAGE=NO CLANG_COVERAGE_MAPPING=NO \
    SPECBOARD_UPDATE_FEED_URL="$SPECBOARD_UPDATE_FEED_URL" SPECBOARD_UPDATE_FEED_REPO="$SPECBOARD_UPDATE_FEED_REPO" \
    build > "$log" 2>&1; then
  grep -E "error:" "$log" | head -20 >&2 || true
  echo "the build failed" >&2
  exit 1
fi
app="$work/build/Build/Products/Release/SpecBoard.app"

# Checks before it goes out: the signature is intact, no get-task-allow, no build paths, the version,
# build number and feed are in, and the agents are in
codesign --verify --deep --strict "$app"
bin="$app/Contents/MacOS/SpecBoard"
if nm -ap "$bin" | grep -E " (SO|OSO) " > /dev/null; then
  echo "the debug symbol table is left (it holds the build paths)" >&2
  exit 1
fi
if otool -l "$bin" | grep -E "sectname __llvm_(prf|cov)" > /dev/null; then
  echo "code coverage instrumentation is left in" >&2
  exit 1
fi
for place in "$root" "$HOME" "$work"; do
  if LC_ALL=C grep -aqF "$place" "$bin"; then
    echo "the executable holds a build path: ${place}" >&2
    exit 1
  fi
done
if codesign -d --entitlements - --xml "$app" 2>/dev/null | grep "get-task-allow" > /dev/null; then
  echo "get-task-allow is set" >&2
  exit 1
fi
plist="$app/Contents/Info.plist"
check() {  # <key> <expected value>
  local found
  found=$(/usr/libexec/PlistBuddy -c "Print :${1}" "$plist" 2>/dev/null || true)
  [[ $found == "${2}" ]] || { echo "Info.plist ${1} is ${found}, expected ${2}" >&2; exit 1; }
}
check CFBundleShortVersionString "$version"
check CFBundleVersion "$BUILD_NUMBER"
check SpecBoardUpdateFeedURL "$SPECBOARD_UPDATE_FEED_URL"
check SpecBoardUpdateFeedRepo "$SPECBOARD_UPDATE_FEED_REPO"

# The agent the app installs on hosts over SSH (tools/build-agent.sh): arm64 only, signed with the app's
# certificate, no build paths
agent="$app/Contents/MacOS/specboard-agent"
[[ -f $agent ]] || { echo "the agent is missing" >&2; exit 1; }
archs=$(lipo -archs "$agent")
[[ $archs == arm64 ]] || { echo "the agent is not arm64 only: ${archs}" >&2; exit 1; }
codesign --verify --strict "$agent"
[[ $(codesign -dv "$agent" 2>&1 | grep -E "^TeamIdentifier") == "$(codesign -dv "$app" 2>&1 | grep -E "^TeamIdentifier")" ]] \
  || { echo "the agent is not signed like the app" >&2; exit 1; }
for place in "$root" "$HOME" "$work"; do
  if LC_ALL=C grep -aqF "$place" "$agent"; then
    echo "the agent holds a build path: ${place}" >&2
    exit 1
  fi
done

# The Linux agent (tools/build-agent.sh): a statically linked x86_64 ELF, no build paths
linux="$app/Contents/Resources/specboard-agent-linux-x86_64"
[[ -f $linux ]] || { echo "the Linux agent is missing" >&2; exit 1; }
kind=$(file -b "$linux")
[[ $kind == "ELF 64-bit LSB executable, x86-64"*"statically linked"* ]] \
  || { echo "the Linux agent is not a static x86_64 executable: ${kind}" >&2; exit 1; }
for place in "$root" "$HOME" "$work"; do
  if LC_ALL=C grep -aqF "$place" "$linux"; then
    echo "the Linux agent holds a build path: ${place}" >&2
    exit 1
  fi
done

# A compressed DMG (UDZO) with the app and a link to /Applications
stage="$work/dmg"
mkdir -p "$stage"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
dmg="$out/SpecBoard-${version}.dmg"
rm -f "$dmg"
hdiutil create -quiet -volname "SpecBoard ${version}" -srcfolder "$stage" -ov -format UDZO "$dmg"
echo "built: ${dmg} (version ${version}, build ${BUILD_NUMBER})"
codesign -dv "$app" 2>&1 | grep -E "^TeamIdentifier" || true
