#!/bin/bash
set -e

# Builds it.
build() {
  local out="$1"
  helper "$out"
}

function helper {
  echo "$@"
}

# A plain comment, then a blank line: not the function's.

main() {
  build target
}

main "$@"
