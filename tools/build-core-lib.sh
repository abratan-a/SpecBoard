#!/bin/bash
# Builds the core that goes into the app (specboard_ffi in sources/core/ffi): a dynamic library for
# arm64 only. The Xcode build runs it (the app target's "Build the core" phase) and puts it in the app's
# Contents/Frameworks.
#
#   bash tools/build-core-lib.sh <folder>
#
# Not a static library, so that the symbols of the SQLite inside the core (bundled by rusqlite) do not
# clash with the app's SQLite (the system's, used by the Swift side): a dynamic library exports only what
# Rust exports.
# When the app is signed, the library is signed with the same certificate and options
# (OTHER_CODE_SIGN_FLAGS). The build paths are left out and the local symbols stripped (Apple's strip -x,
# which keeps the exported functions the app calls).
# Cargo does not strip: it strips with the rust-objcopy of the Rust version in rust-toolchain.toml, which
# can leave the LINKEDIT string table on a 4-byte boundary, and the dyld of macOS 27 refuses such a file
# linked against SDK 27. That goes for the proc-macros and build scripts too (rustc loads them during the
# build), so they are not stripped at all. tools/check-linkedit.sh stops the build if the table is
# misaligned anyway.
# Cargo builds under Xcode's build folder, so nothing is left in the source tree. Written for macOS's
# bash 3.2.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dest="${1:?give the folder to put the library in}"
# tools/build-env.sh finds cargo and rustup
source "$root/tools/build-env.sh"
specboard_require_rust
targets=(aarch64-apple-darwin)
name=libspecboard_ffi.dylib

export CARGO_TARGET_DIR="${OBJROOT:-$root/build}/specboard-core"
export RUSTFLAGS="--remap-path-prefix=$root=. --remap-path-prefix=$HOME=~"
export CARGO_PROFILE_RELEASE_STRIP=none
export CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP=none
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-14.0}

cd "$root/sources/core"
"$RUSTUP" target add "${targets[@]}" >/dev/null
built=()
for target in "${targets[@]}"; do
  "$CARGO" build --quiet --release --locked -p specboard-ffi --target "$target"
  built+=("$CARGO_TARGET_DIR/$target/release/$name")
done

mkdir -p "$dest"
cp -f "${built[0]}" "$dest/$name.incoming"
# /usr/bin/strip is Apple's (a strip from binutils or Homebrew may come first on PATH)
/usr/bin/strip -x "$dest/$name.incoming"
install_name_tool -id "@rpath/$name" "$dest/$name.incoming"
if [[ ${CODE_SIGNING_ALLOWED:-NO} == YES && -n ${EXPANDED_CODE_SIGN_IDENTITY:-} ]]; then
  # OTHER_CODE_SIGN_FLAGS is what the app itself is signed with (--timestamp for a notarized build)
  codesign --force --sign "$EXPANDED_CODE_SIGN_IDENTITY" --options runtime ${OTHER_CODE_SIGN_FLAGS:-} "$dest/$name.incoming"
fi
bash "$root/tools/check-linkedit.sh" "$dest/$name.incoming"
mv -f "$dest/$name.incoming" "$dest/$name"
