#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-seam-discipline.sh
# Mutation proof for the seam-discipline vector's FILE scope.
#
# The vector promised to mirror `rs_prod_files` (clippy's `#[cfg(test)]`
# exclusion) and delivered only the `tests.rs` basename half: a test module
# carved out of `lib.rs` into its own file — `#[cfg(test)] mod probe;` beside
# `probe.rs` — was read as production, and its real-filesystem fixtures came
# back as 24 unmarked `std::fs::` bypasses. A RED on a file the compiler never
# builds outside a test profile (measured 2026-09-06 on a carve of the
# builtin dispatcher tests).
#
# BOTH directions are pinned: the same file declared as a production module
# must still turn the vector RED, or the fix would be a filter that drops
# everything. Every case is a throwaway repo with the vector and its library
# copied in at the same relative layout, because the vector resolves its repo
# root from its own location and lists files through `git ls-files`.
set -uo pipefail

# Vector 49 runs this file from inside the pre-push gate, where git exports
# GIT_DIR and friends; inherited, they would point the throwaway repos at the
# real index. Clear the inheritance once, here.
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# scaffold <dir> — a one-crate workspace at layer L2 with the vector copied in.
scaffold() {
  local dir="$1"
  mkdir -p "$dir/scripts/ci" "$dir/scripts/hygiene" "$dir/crates/fixture/src"
  cp "$ROOT/scripts/hygiene/check-seam-discipline.sh" "$dir/scripts/hygiene/"
  # The library fails CLOSED without its own self-tests beside it, so every
  # `scripts/ci/test-*.sh` travels with it.
  cp "$ROOT/scripts/ci/_lib.sh" "$ROOT"/scripts/ci/test-*.sh "$dir/scripts/ci/"
  cat >"$dir/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/fixture"]

[workspace.metadata.diamond]
layers.fixture = "L2"
EOF
  cat >"$dir/crates/fixture/Cargo.toml" <<'EOF'
[package]
name = "fixture"
version = "0.0.0"
edition = "2021"
EOF
}

# expect <GREEN|RED> <label> <lib.rs body> <probe file name> <probe body>
expect() {
  local want="$1" label="$2" lib="$3" probe_name="$4" probe="$5"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  scaffold "$dir"
  printf '%s\n' "$lib" >"$dir/crates/fixture/src/lib.rs"
  printf '%s\n' "$probe" >"$dir/crates/fixture/src/$probe_name"
  git -C "$dir" init -q
  git -C "$dir" add -A
  local out rc
  out="$(cd "$dir" && bash scripts/hygiene/check-seam-discipline.sh 2>&1)"
  rc=$?
  local got="RED"
  [ "$rc" -eq 0 ] && got="GREEN"
  if [ "$got" = "$want" ]; then
    printf '  ok   %s\n' "$label"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted %s, vector exited %d:\n%s\n' "$label" "$want" "$rc" "$out"
  fi
}

PROBE_BYPASS='pub fn touch() { let _ = std::fs::write("x", b"y"); }'

# The rule in the direction that bit: a module declared under `#[cfg(test)]`
# is test-only whatever its basename, so its direct-OS fixtures are not
# bypasses. Reverting the fix turns this case RED.
expect GREEN "a #[cfg(test)]-declared module file is not production" \
  $'#[cfg(test)]\nmod probe;' probe.rs "$PROBE_BYPASS"

# The same file declared as a PRODUCTION module is a bypass — the half that
# proves the exclusion is a rule, not a filter that drops everything (an
# empty test-only list must drop nothing).
expect RED "the same file declared as a production module is a bypass" \
  'mod probe;' probe.rs "$PROBE_BYPASS"

# An attribute or a comment between `#[cfg(test)]` and the declaration keeps
# the pairing (the library's rule); an unrelated line breaks it.
expect GREEN "a comment between the attribute and the declaration keeps the pairing" \
  $'#[cfg(test)]\n// the real dispatcher route\nmod probe;' probe.rs "$PROBE_BYPASS"
expect RED "a statement between the attribute and the declaration breaks the pairing" \
  $'#[cfg(test)]\npub struct Marker;\nmod probe;' probe.rs "$PROBE_BYPASS"

# The other half, unchanged: the `tests.rs` basename is excluded, and a
# production bypass carrying the sign-off marker is exempt.
expect GREEN "the tests.rs basename stays excluded" \
  $'#[cfg(test)]\nmod tests;' tests.rs "$PROBE_BYPASS"
expect GREEN "a signed-off production bypass stays exempt" \
  'mod probe;' probe.rs \
  'pub fn touch() { let _ = std::fs::write("x", b"y"); } // seam-bypass-ok: fixture of the marker rule'

if [ "$fails" -gt 0 ]; then
  printf 'RED: %d of %d seam-discipline scope case(s) failed\n' "$fails" "$cases"
  exit 2
fi
printf 'OK: %d seam-discipline scope case(s)\n' "$cases"
