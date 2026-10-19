#!/bin/bash
# Checks that Swift can call the core: builds the XCFramework, checks that it holds arm64 only, and runs
# the small Swift check in sources/core/swift-check.
set -euo pipefail

cd "$(dirname "$0")/.."
bash tools/build-core-swift.sh

archs=$(lipo -archs build/core/libspecboard_ffi.a)
[[ $archs == arm64 ]] || { echo "failed: the core is not arm64 only ($archs)"; exit 1; }

swift run --package-path sources/core/swift-check core-check "$PWD/sources/core/tests/fixtures/projects/sample.json"

# The bindings in the app's sources match the core's interface as it is now (after changing it, run
# build-core-swift.sh and commit what it writes)
if ! git diff --quiet -- sources/SpecBoard/Core/specboard_ffi.swift sources/CoreFFI; then
  echo "failed: the core bindings in the app are out of date (run bash tools/build-core-swift.sh and commit them)"
  git diff --stat -- sources/SpecBoard/Core/specboard_ffi.swift sources/CoreFFI
  exit 1
fi
