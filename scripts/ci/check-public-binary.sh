#!/usr/bin/env bash
# check-public-binary.sh — the public executable is born `nika` (ADR-135).
#
# One identity from `cargo build --bin nika` through target/release/nika, the
# release tarball, the flake, the tap and the user's prompt. This half reads
# the manifests and the packaging surfaces with no cargo on the runner;
# cargo's own reading (cargo metadata · exactly one bin target named `nika` ·
# the package's default-run) is crates/nika-cli/tests/public_binary_identity.rs,
# which cannot even COMPILE against a target named otherwise.
#
# The gate proves itself before it judges: every mutant of the contract must
# read RED here (scripts/ci/test-public-binary.sh) or the verdict is decoration.
#
#   bash scripts/ci/check-public-binary.sh              # judge this tree
#   bash scripts/ci/check-public-binary.sh <root>       # judge another tree
#   bash scripts/ci/check-public-binary.sh --self-test  # the mutants
#
# Exit: 0 green · 1 red · 2 cannot judge
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
if [ "${1:-}" = "--self-test" ]; then
  exec bash "$HERE/test-public-binary.sh"
fi
ROOT="${1:-$(cd "$HERE/../.." && pwd)}"
if [ ! -d "$ROOT/crates" ]; then
  printf '[public-binary] cannot judge: no crates/ under %s\n' "$ROOT"
  exit 2
fi
if [ "${PUBLIC_BINARY_SELFTESTED:-}" != "1" ]; then
  # The self-test runs BEFORE the verdict: a gate that cannot see its own
  # mutants has no standing to call a tree green.
  if ! PUBLIC_BINARY_SELFTESTED=1 bash "$HERE/test-public-binary.sh" >/dev/null 2>&1; then
    printf '[public-binary] RED · the self-test fails — the gate is not honest, its verdict is void:\n'
    PUBLIC_BINARY_SELFTESTED=1 bash "$HERE/test-public-binary.sh" 2>&1 | tail -12
    exit 1
  fi
fi

fail=0
red() {
  printf '[public-binary] RED · %s\n' "$*"
  fail=1
}

# 1 · every bin target a manifest declares ([[bin]] name), plus the one cargo
#     auto-discovers when a crate has src/main.rs, no [[bin]] and autobins on
#     (named after the package). Exactly one is `nika`, it belongs to
#     crates/nika-cli, and none is still named `nika-cli`.
targets="$(mktemp)"
trap 'rm -f "$targets"' EXIT
for manifest in "$ROOT"/crates/*/Cargo.toml; do
  [ -f "$manifest" ] || continue
  dir="$(dirname "$manifest")"
  crate="$(basename "$dir")"
  names="$(awk '/^\[\[bin\]\]/{inbin=1; next} /^\[/{inbin=0} inbin && /^name[[:space:]]*=/{sub(/^name[[:space:]]*=[[:space:]]*"/,""); sub(/".*$/,""); print}' "$manifest")"
  if [ -z "$names" ] && [ -f "$dir/src/main.rs" ] && ! grep -qE '^autobins[[:space:]]*=[[:space:]]*false' "$manifest"; then
    names="$(awk '/^\[package\]/{inpkg=1; next} /^\[/{inpkg=0} inpkg && /^name[[:space:]]*=/{sub(/^name[[:space:]]*=[[:space:]]*"/,""); sub(/".*$/,""); print; exit}' "$manifest")"
  fi
  printf '%s\n' "$names" | sed '/^$/d' | sed "s/^/$crate:/" >>"$targets"
done
nika_owners="$(grep -E ':nika$' "$targets" | sed 's/:nika$//' | tr '\n' ' ' | sed 's/ $//')"
nika_count="$(grep -cE ':nika$' "$targets" || true)"
if [ "$nika_count" -ne 1 ]; then
  red "expected exactly one bin target named nika, found $nika_count (owners: ${nika_owners:-none})"
elif [ "$nika_owners" != "nika-cli" ]; then
  red "the nika bin target must belong to crates/nika-cli (found in $nika_owners)"
fi
if grep -qE ':nika-cli$' "$targets"; then
  red "a bin target is still named nika-cli ($(grep -E ':nika-cli$' "$targets" | tr '\n' ' '))"
fi

# 2 · the package runs its one executable by default.
if ! grep -qE '^default-run[[:space:]]*=[[:space:]]*"nika"' "$ROOT/crates/nika-cli/Cargo.toml" 2>/dev/null; then
  red 'crates/nika-cli/Cargo.toml: default-run = "nika" is missing'
fi

# 3 · the release builds and stages the same name — no rename at packaging.
#     The comment-stripped workflow is read WHOLE (no `grep -q`): under
#     pipefail an early-exiting reader kills the writer with SIGPIPE on a
#     long file and turns a true line into a RED — the self-test pads its
#     fixture past that cliff so the class stays caught.
release="$ROOT/.github/workflows/release.yml"
if [ -f "$release" ]; then
  stripped="$(sed -E 's/#.*$//' "$release")"
  if ! printf '%s\n' "$stripped" | grep -E -- 'cargo build .*--bin nika( |$)' >/dev/null; then
    red "release.yml: the release build line does not say --bin nika"
  fi
  if ! printf '%s\n' "$stripped" | grep -E 'release/nika"' >/dev/null; then
    red "release.yml: packaging does not stage target/<triple>/release/nika"
  fi
else
  red "no .github/workflows/release.yml to judge"
fi

# 4 · no build, package, install or test path still spells the old name
#     (the README's install lines included: a teaching surface that tells a
#     stranger to symlink nika-cli is the same split identity in prose).
#     Both spellings of a build line are read: the shell form `--bin nika-cli`
#     and the argv-array form `"--bin", "nika-cli"` a Rust test hands to
#     `cargo run` (the wasm differential test carried the second one).
stale="$(grep -rnE -- '--bin nika-cli|"--bin", *"nika-cli"|/(release|debug)/nika-cli|bin/nika-cli|CARGO_BIN_EXE_nika-cli' \
  "$ROOT/.github" "$ROOT/scripts" "$ROOT/crates" "$ROOT/.agents" "$ROOT/flake.nix" "$ROOT/Dockerfile" "$ROOT/README.md" 2>/dev/null \
  | grep -vE '/target/|check-public-binary\.sh:|test-public-binary\.sh:' || true)"
if [ -n "$stale" ]; then
  red "a nika-cli binary path survives (the executable is born nika):"$'\n'"$stale"
fi

if [ "$fail" -eq 0 ]; then
  printf '[public-binary] GREEN · one executable identity: nika (crates/nika-cli · default-run · the release line · no packaging rename · the tests read the nika target)\n'
fi
exit "$fail"
