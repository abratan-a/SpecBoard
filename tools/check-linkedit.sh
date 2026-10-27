#!/bin/bash
# Stops when the LINKEDIT string table of a Mach-O file does not start on an 8-byte boundary.
# The dyld of macOS 27 refuses such a file when it was linked against SDK 27 or later ("mis-aligned
# LINKEDIT string pool"): the app, the core or the agent would not load. Some strip tools (the
# rust-objcopy of the Rust version in rust-toolchain.toml) leave the table on a 4-byte boundary; Apple's
# ld and strip do not. The check reads the offset itself (otool), so a build on an older macOS or SDK,
# whose dyld still loads the file, catches it too.
#
#   bash tools/check-linkedit.sh <Mach-O file>...
#
# Written for macOS's bash 3.2.
set -euo pipefail

[[ $# -gt 0 ]] || { echo "give the Mach-O files to check" >&2; exit 2; }
status=0
for file in "$@"; do
  # Every architecture of a universal file; stroff is counted from the start of its slice
  offsets=$(otool -arch all -l "$file" \
    | awk '$1 == "cmd" { symtab = ($2 == "LC_SYMTAB"); next } symtab && $1 == "stroff" { print $2 }')
  if [[ -z $offsets ]]; then
    echo "error: no symbol table (LC_SYMTAB) in ${file}" >&2
    status=1
    continue
  fi
  for offset in $offsets; do
    if (( offset % 8 != 0 )); then
      echo "error: the LINKEDIT string table of ${file} starts at ${offset}, not on an 8-byte boundary (the dyld of macOS 27 refuses it)" >&2
      status=1
    fi
  done
done
exit "$status"
