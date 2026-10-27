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
#   (specboard://SpecBoard/Docs/Design:Release). The built-in feed is GitHub Releases and goes into every
#   copy of the app, so the URL must be https://github.com as it is (no user name, port or path), and the
#   repo owner/name.
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
# The build stops if either is left in any Mach-O file of the app (the app, the core, the agent), if a
# build path is left in any of them, or if a Mach-O file has its LINKEDIT string table off an 8-byte
# boundary (tools/check-linkedit.sh): the dyld of macOS 27 would not load it. A check whose tool fails
# stops the build too (tools/check-app.sh).
# It builds in a temporary folder and leaves nothing in the source tree. When the build fails, the log of
# xcodebuild is kept in the output folder (xcodebuild.log), and the errors of the compilers, rustc's
# error[E…] among them, are shown. Written for macOS's bash 3.2.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:?give the output folder}"
: "${BUILD_NUMBER:?BUILD_NUMBER is not set}"
SPECBOARD_UPDATE_FEED_URL="${SPECBOARD_UPDATE_FEED_URL:-}"
SPECBOARD_UPDATE_FEED_REPO="${SPECBOARD_UPDATE_FEED_REPO:-}"
[[ -z $SPECBOARD_UPDATE_FEED_URL && -z $SPECBOARD_UPDATE_FEED_REPO || -n $SPECBOARD_UPDATE_FEED_URL && -n $SPECBOARD_UPDATE_FEED_REPO ]] \
  || { echo "give both SPECBOARD_UPDATE_FEED_URL and SPECBOARD_UPDATE_FEED_REPO, or neither" >&2; exit 1; }
# The built-in feed is GitHub Releases: nothing else is let into the app
if [[ -n $SPECBOARD_UPDATE_FEED_URL ]]; then
  [[ $SPECBOARD_UPDATE_FEED_URL == "https://github.com" ]] \
    || { echo "SPECBOARD_UPDATE_FEED_URL must be https://github.com (the built-in feed is GitHub Releases): ${SPECBOARD_UPDATE_FEED_URL}" >&2; exit 1; }
  [[ $SPECBOARD_UPDATE_FEED_REPO =~ ^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$ && $SPECBOARD_UPDATE_FEED_REPO != *..* ]] \
    || { echo "SPECBOARD_UPDATE_FEED_REPO must be owner/name: ${SPECBOARD_UPDATE_FEED_REPO}" >&2; exit 1; }
fi
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
  # The temporary folder goes away on exit, so the log is copied out first. rustc names the error on its
  # first line (error[E0463]: ...) and what it is about on the next ones; cargo then says only that the
  # crate could not be compiled
  cp -f "$log" "$out/xcodebuild.log"
  grep -E -A2 "error(\[E[0-9]+\])?:" "$log" | head -40 >&2 || true
  echo "the build failed (the whole log: ${out}/xcodebuild.log)" >&2
  exit 1
fi
rm -f "$out/xcodebuild.log"
app="$work/build/Build/Products/Release/SpecBoard.app"

# Checks before it goes out: the signature, the entitlements and every binary in the app
# (tools/check-app.sh), then the version, build number and feed
bash "$root/tools/check-app.sh" "$app" "$root" "$HOME" "$work"
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
