#!/bin/bash
# Builds the agent that goes into the app (sources/core/agent): for macOS (an arm64-only executable) and
# for Linux (x86_64, statically linked with musl). The Xcode build runs it (the app target's "Build the
# agent" phase); the macOS one goes to the app's Contents/MacOS/specboard-agent, the Linux one to
# Contents/Resources/specboard-agent-linux-x86_64 (an ELF file cannot be code-signed, so it does not go
# in MacOS; the app's signature still seals it).
#
#   bash tools/build-agent.sh <place for the macOS agent> <place for the Linux agent>
#
# cargo-zigbuild builds the Linux agent with zig as the linker and C compiler (install both with
# Homebrew). Without them, a build for a release (build setting SPECBOARD_REQUIRE_LINUX_AGENT=YES, passed
# by tools/build-dmg.sh) stops, so that no app goes out without the Linux agent; any other build (from
# source, for example) warns and goes on without it.
# When the app is signed, the agent is signed with the same certificate and options
# (OTHER_CODE_SIGN_FLAGS); otherwise it keeps the ad-hoc signature the linker gives it.
# The build paths are left out (--remap-path-prefix) and the symbols stripped. It is built for macOS 11.0,
# not for the app's deployment target, so that it runs on hosts with an older macOS than the app's.
# The macOS agent is stripped with Apple's strip, not by cargo: cargo strips with the rust-objcopy of the
# Rust version in rust-toolchain.toml, which can leave the LINKEDIT string table on a 4-byte boundary, and
# the dyld of macOS 27 refuses such a file linked against SDK 27. The proc-macros and build scripts of both
# builds run on this Mac (rustc loads them), so they are not stripped at all. The Linux agent (ELF) keeps
# cargo's strip. tools/check-linkedit.sh stops the build if the table of the macOS agent is misaligned
# anyway.
# The app's build number (CURRENT_PROJECT_VERSION from Xcode; twelve digits, year to minute, in a release
# build) goes in: an app with a newer build restarts the agent running on a host, and an app with an
# older build does not connect and says so (sources/core/agent/src/attach.rs).
# Cargo builds under Xcode's build folder, so nothing is left in the source tree. Written for macOS's
# bash 3.2.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dest="${1:?give the place for the macOS agent}"
linux_dest="${2:?give the place for the Linux agent}"
# tools/build-env.sh finds cargo, rustup, zig and cargo-zigbuild
source "$root/tools/build-env.sh"
specboard_require_rust
targets=(aarch64-apple-darwin)
linux_target=x86_64-unknown-linux-musl

linux=1
if [[ -z $ZIG || -z $ZIGBUILD ]]; then
  if [[ ${SPECBOARD_REQUIRE_LINUX_AGENT:-NO} == YES ]]; then
    echo "error: building the Linux agent needs zig and cargo-zigbuild (brew install zig cargo-zigbuild)" >&2
    exit 1
  fi
  # Xcode shows this as a build warning. The app says the Linux agent is missing when it would install one
  echo "warning: zig or cargo-zigbuild was not found (brew install zig cargo-zigbuild): skipping the Linux agent" >&2
  linux=""
fi

export CARGO_TARGET_DIR="${OBJROOT:-$root/build}/specboard-agent"
export RUSTFLAGS="--remap-path-prefix=$root=. --remap-path-prefix=$HOME=~"
export CARGO_PROFILE_RELEASE_STRIP=none
export CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP=none
export MACOSX_DEPLOYMENT_TARGET=11.0
export SPECBOARD_BUILD="${CURRENT_PROJECT_VERSION:-}"

cd "$root/sources/core"
"$RUSTUP" target add "${targets[@]}" ${linux:+"$linux_target"} >/dev/null
built=()
for target in "${targets[@]}"; do
  "$CARGO" build --quiet --release --locked -p specboard-agent --target "$target"
  built+=("$CARGO_TARGET_DIR/$target/release/specboard-agent")
done

mkdir -p "$(dirname "$dest")"
cp -f "${built[0]}" "$dest.incoming"
# /usr/bin/strip is Apple's (a strip from binutils or Homebrew may come first on PATH). Without options it
# removes every symbol an executable does not need to run
/usr/bin/strip "$dest.incoming"
if [[ ${CODE_SIGNING_ALLOWED:-NO} == YES && -n ${EXPANDED_CODE_SIGN_IDENTITY:-} ]]; then
  # OTHER_CODE_SIGN_FLAGS is what the app itself is signed with (--timestamp for a notarized build)
  codesign --force --sign "$EXPANDED_CODE_SIGN_IDENTITY" --options runtime ${OTHER_CODE_SIGN_FLAGS:-} "$dest.incoming"
fi
bash "$root/tools/check-linkedit.sh" "$dest.incoming"
mv -f "$dest.incoming" "$dest"

# The Linux agent. zig and cargo are looked up on PATH, so their places go first. The macOS SDK and
# deployment target that Xcode passes must not reach the Linux build.
# When it is not built, what an earlier build left is removed, so that the app does not hold a stale one
if [[ -z $linux ]]; then
  rm -f "$linux_dest"
  exit 0
fi
env -u SDKROOT -u MACOSX_DEPLOYMENT_TARGET CARGO_PROFILE_RELEASE_STRIP=symbols PATH="$(dirname "$ZIG"):$(dirname "$(readlink -f "$CARGO")"):$PATH" \
  "$ZIGBUILD" build --quiet --release --locked -p specboard-agent --target "$linux_target"
mkdir -p "$(dirname "$linux_dest")"
cp -f "$CARGO_TARGET_DIR/$linux_target/release/specboard-agent" "$linux_dest.incoming"
mv -f "$linux_dest.incoming" "$linux_dest"
