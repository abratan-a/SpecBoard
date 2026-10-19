# Finds the tools the app's build phases need. Sourced by tools/build-core-lib.sh and
# tools/build-agent.sh; it is the one place that knows where they live.
#
# An Xcode build does not inherit the user's shell PATH, so the usual places are looked at
# before PATH. Each tool is looked up in this order:
#
#   cargo                CARGO, then /usr/local/bin, ~/.cargo/bin (rustup's default), PATH
#   zig, cargo-zigbuild  ZIG / ZIGBUILD, then /opt/homebrew/bin, ~/.cargo/bin, PATH
#
# A variable that is set is used as it is, even when nothing is there: setting ZIG to a path
# that does not exist hides zig. rustup is looked for next to cargo (the real file, then the
# link), because rust-toolchain.toml picks the Rust version and only rustup reads it.
#
# After sourcing: CARGO and RUSTUP (empty when not found; call specboard_require_rust to stop
# with what to install), ZIG and ZIGBUILD (empty when not found).
# SPECBOARD_CARGO_PLACES and SPECBOARD_ZIG_PLACES (colon-separated) replace the fixed places,
# for checking the lookup itself. Written for macOS's bash 3.2.

specboard_find_tool() {  # <value of the variable> <name> <places, colon-separated>
  local given=$1 name=$2 places=$3 place
  if [[ -n $given ]]; then
    [[ -x $given ]] && echo "$given"
    return 0
  fi
  local IFS=:
  for place in $places; do
    [[ -n $place && -x $place/$name ]] && { echo "$place/$name"; return 0; }
  done
  command -v "$name" 2>/dev/null || true
}

CARGO=$(specboard_find_tool "${CARGO:-}" cargo "${SPECBOARD_CARGO_PLACES-/usr/local/bin:$HOME/.cargo/bin}")
RUSTUP=""
if [[ -n $CARGO ]]; then
  for specboard_dir in "$(dirname "$(readlink -f "$CARGO" 2>/dev/null || echo "$CARGO")")" "$(dirname "$CARGO")"; do
    if [[ -x $specboard_dir/rustup ]]; then RUSTUP=$specboard_dir/rustup; break; fi
  done
  unset specboard_dir
fi
ZIG=$(specboard_find_tool "${ZIG:-}" zig "${SPECBOARD_ZIG_PLACES-/opt/homebrew/bin:$HOME/.cargo/bin}")
ZIGBUILD=$(specboard_find_tool "${ZIGBUILD:-}" cargo-zigbuild "${SPECBOARD_ZIG_PLACES-/opt/homebrew/bin:$HOME/.cargo/bin}")

specboard_require_rust() {
  if [[ -z $CARGO ]]; then
    echo "error: cargo was not found (looked at \$CARGO, /usr/local/bin, ~/.cargo/bin and PATH). Install Rust with rustup: https://rustup.rs" >&2
    exit 1
  fi
  if [[ -z $RUSTUP ]]; then
    echo "error: rustup is required next to ${CARGO}: rust-toolchain.toml picks the Rust version, and only rustup reads it. Install Rust with rustup: https://rustup.rs" >&2
    exit 1
  fi
}
