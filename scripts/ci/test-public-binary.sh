#!/usr/bin/env bash
# test-public-binary.sh — the mutants of the public-binary contract (ADR-135).
#
# check-public-binary.sh judges a tree; this builds the smallest trees that
# carry the contract and every way of breaking it, and expects the gate to
# say GREEN once and RED for each mutant. A gate that passes a mutant is
# decoration (gate-honesty) — this file is what makes the verdict mean
# something. Run alone or through `check-public-binary.sh --self-test`.
# shellcheck disable=SC2016  # the fixtures carry literal ${TARGET} / $out on purpose
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
GATE="$HERE/check-public-binary.sh"
export PUBLIC_BINARY_SELFTESTED=1
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
failures=0

# fixture <root> <bin name> <default-run line> <build-line bin> <flake mv 0|1> <second nika crate 0|1> <auto nika crate 0|1>
fixture() {
  local root="$1" bin="$2" run_line="$3" build_bin="$4" flake_mv="$5" second="$6" auto="$7"
  mkdir -p "$root/crates/nika-cli/src" "$root/.github/workflows" "$root/scripts"
  printf '[package]\nname = "nika-cli"\n%s\n\n[[bin]]\nname = "%s"\npath = "src/main.rs"\n' "$run_line" "$bin" >"$root/crates/nika-cli/Cargo.toml"
  : >"$root/crates/nika-cli/src/main.rs"
  printf 'jobs:\n  build:\n    steps:\n      - run: cargo build --release --locked --bin %s --target x\n      - run: cp "target/${TARGET}/release/%s" "${stage}/nika"\n' "$build_bin" "$build_bin" >"$root/.github/workflows/release.yml"
  # The real workflow is long: pad the fixture far past a pipe buffer so a
  # reader that quits early (grep -q under pipefail) would kill its writer
  # with SIGPIPE and read RED on a true line — the clean case must catch it.
  yes '      # padding: the workflow continues for hundreds of lines' | head -4000 >>"$root/.github/workflows/release.yml"
  if [ "$flake_mv" = 1 ]; then
    printf 'postInstall = mv $out/bin/nika-cli $out/bin/nika\n' >"$root/flake.nix"
  else
    printf '# the executable is born nika\n' >"$root/flake.nix"
  fi
  if [ "$second" = 1 ]; then
    mkdir -p "$root/crates/nika-other/src"
    printf '[package]\nname = "nika-other"\n\n[[bin]]\nname = "nika"\npath = "src/main.rs"\n' >"$root/crates/nika-other/Cargo.toml"
    : >"$root/crates/nika-other/src/main.rs"
  fi
  if [ "$auto" = 1 ]; then
    # a crate named `nika` with src/main.rs and no [[bin]]: cargo auto-discovers
    # a second executable named nika — the gate must see what cargo sees.
    mkdir -p "$root/crates/nika/src"
    printf '[package]\nname = "nika"\n' >"$root/crates/nika/Cargo.toml"
    : >"$root/crates/nika/src/main.rs"
  fi
}

expect() {
  local label="$1" want="$2" root="$3" got
  bash "$GATE" "$root" >"$root/verdict.txt" 2>&1
  got=$?
  if [ "$got" -eq "$want" ]; then
    printf '  ok   %s (rc=%s)\n' "$label" "$got"
  else
    printf '  FAIL %s · expected rc=%s got rc=%s\n' "$label" "$want" "$got"
    sed 's/^/       /' "$root/verdict.txt"
    failures=$((failures + 1))
  fi
}

DEFAULT='default-run = "nika"'
fixture "$work/clean" nika "$DEFAULT" nika 0 0 0
expect "clean tree is GREEN" 0 "$work/clean"
fixture "$work/m1" nika-cli "$DEFAULT" nika 0 0 0
expect "mutant · the bin target is named nika-cli" 1 "$work/m1"
fixture "$work/m2" nika "" nika 0 0 0
expect "mutant · default-run missing" 1 "$work/m2"
fixture "$work/m3" nika "$DEFAULT" nika-cli 0 0 0
expect "mutant · the release line builds --bin nika-cli and renames" 1 "$work/m3"
fixture "$work/m4" nika "$DEFAULT" nika 1 0 0
expect "mutant · the flake renames nika-cli into nika" 1 "$work/m4"
fixture "$work/m5" nika "$DEFAULT" nika 0 1 0
expect "mutant · a second crate declares a bin named nika" 1 "$work/m5"
fixture "$work/m6" nika "$DEFAULT" nika 0 0 1
expect "mutant · a crate named nika auto-discovers a second executable" 1 "$work/m6"
fixture "$work/m7" nika "$DEFAULT" nika 0 0 0
printf 'fn main() {}\n// %s\n' 'env!("CARGO_BIN_EXE_nika-cli")' >"$work/m7/crates/nika-cli/src/main.rs"
expect "mutant · a test still reads the nika-cli target" 1 "$work/m7"
mkdir -p "$work/m8"
expect "no crates/ under the root cannot be judged" 2 "$work/m8"

if [ "$failures" -eq 0 ]; then
  printf '[public-binary self-test] GREEN · 1 clean + 7 mutants + 1 unjudgeable behave\n'
  exit 0
fi
printf '[public-binary self-test] RED · %s case(s) misjudged\n' "$failures"
exit 1
