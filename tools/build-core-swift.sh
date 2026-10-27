#!/bin/bash
# Builds the core in the form Swift calls: an XCFramework of the arm64 static library, and the Swift
# bindings (.swift) that UniFFI writes. The output goes to Frameworks/ and Generated/ in
# sources/core/swift-check/ (neither is committed), and the bindings the app uses are copied into the
# app's sources (below). Runs on macOS.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
CORE=$ROOT/sources/core
CHECK=$CORE/swift-check
WORK=$ROOT/build/core
# tools/build-env.sh finds cargo and rustup
source "$ROOT/tools/build-env.sh"
specboard_require_rust
TARGET_DIR=${CARGO_TARGET_DIR:-$CORE/target}
# Proc-macros and build scripts are not stripped: the rust-objcopy that strips them can leave the LINKEDIT
# string table on a 4-byte boundary, and the dyld of macOS 27 then refuses to load them into rustc
# (tools/build-core-lib.sh says more).
export CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP=none
TARGETS=(aarch64-apple-darwin)

cd "$CORE"
"$RUSTUP" target add "${TARGETS[@]}" >/dev/null
for target in "${TARGETS[@]}"; do
  "$CARGO" build --release -p specboard-ffi --target "$target"
done

rm -rf "$WORK"
mkdir -p "$WORK/headers" "$WORK/swift"
LIBS=()
for target in "${TARGETS[@]}"; do LIBS+=("$TARGET_DIR/$target/release/libspecboard_ffi.a"); done
cp -f "${LIBS[0]}" "$WORK/libspecboard_ffi.a"

# The bindings are written from the interface description inside the library
"$CARGO" run --quiet --release -p uniffi-bindgen -- generate \
  --library "$TARGET_DIR/aarch64-apple-darwin/release/libspecboard_ffi.dylib" \
  --language swift --out-dir "$WORK/swift"
# The bindings can be called from any thread (the core locks inside): the outermost declarations are
# marked nonisolated so that they do not fall under the app's default isolation (MainActor)
python3 - "$WORK/swift/specboard_ffi.swift" <<'PY'
import re, sys
path = sys.argv[1]
text = open(path).read()
decl = re.compile(r'^((?:public |private |fileprivate |internal |open )?(?:final )?(?:class|struct|enum|protocol|extension|func|let|var) )', re.M)
open(path, 'w').write(decl.sub(r'nonisolated \1', text))
PY
cp "$WORK/swift/specboard_ffiFFI.h" "$WORK/headers/"
cp "$WORK/swift/specboard_ffiFFI.modulemap" "$WORK/headers/module.modulemap"

rm -rf "$CHECK/Frameworks" "$CHECK/Generated"
mkdir -p "$CHECK/Frameworks" "$CHECK/Generated"
xcodebuild -create-xcframework \
  -library "$WORK/libspecboard_ffi.a" -headers "$WORK/headers" \
  -output "$CHECK/Frameworks/SpecBoardCoreFFI.xcframework" >/dev/null
cp "$WORK/swift/specboard_ffi.swift" "$CHECK/Generated/"

# The bindings the app builds with: the Swift side goes into the app's sources and the C side (header and
# modulemap) into sources/CoreFFI, both committed. They change only when the core's interface changes;
# tools/swift-check.sh checks that they match what is written here.
APP_SWIFT=$ROOT/sources/SpecBoard/Core/specboard_ffi.swift
APP_C=$ROOT/sources/CoreFFI
mkdir -p "$(dirname "$APP_SWIFT")" "$APP_C"
cp "$WORK/swift/specboard_ffi.swift" "$APP_SWIFT"
cp "$WORK/headers/specboard_ffiFFI.h" "$APP_C/"
cp "$WORK/headers/module.modulemap" "$APP_C/"

echo "built: $CHECK/Frameworks/SpecBoardCoreFFI.xcframework ($(lipo -archs "$WORK/libspecboard_ffi.a"))"
