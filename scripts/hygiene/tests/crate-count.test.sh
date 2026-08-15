#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-crate-count.sh
#
# Mutation proof for vector 2 (every workspace member is layer-classified).
#
# It compared COUNTS. Each fault alone reddened, so it looked enforced —
# but the realistic refactor breaks both halves at once: you add a crate
# without a layer AND drop another whose layer stays behind. Two members,
# two layers, parity satisfied, invariant broken twice.
#
# Measured before the fix · `OK (2 workspace members, all layer-classified)`,
# rc=0, with one crate unclassified and one layer naming nothing.
#
# A parity that two errors can satisfy is arithmetic, not enforcement — the
# same family as a runner announcing "all 8 ratchets passed" after skipping
# the ninth. The swap case below is the one that matters; the two solo cases
# are the controls that prove the probe discriminates rather than reddening
# on everything.
#
# shellcheck disable=SC2329
set -uo pipefail

unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
VECTOR="$ROOT/scripts/hygiene/check-crate-count.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# expect <want-rc> <label> <members> <layers>
expect() {
  local want="$1" label="$2" members="$3" layers="$4"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  mkdir -p "$dir/scripts/hygiene"
  cp "$VECTOR" "$dir/scripts/hygiene/"
  {
    printf '[workspace]\nmembers = [%s]\n\n[workspace.metadata.diamond]\n' "$members"
    printf '%s\n' "$layers"
  } >"$dir/Cargo.toml"
  local rc
  bash "$dir/scripts/hygiene/check-crate-count.sh" >/dev/null 2>&1
  rc=$?
  if [ "$rc" = "$want" ]; then
    printf '  ok   %s (rc=%d)\n' "$label" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted rc=%s, got rc=%d\n' "$label" "$want" "$rc" >&2
  fi
}

BOTH='"crates/nika-a", "crates/nika-b"'

echo "vector 2 · mutation proof"

expect 0 "every member classified" "$BOTH" 'layers.nika-a = "L0"
layers.nika-b = "L1"'

# THE case: one member unclassified AND one layer naming no member. The
# counts match; both halves of the invariant are broken.
expect 2 "a member with no layer AND a zombie layer" "$BOTH" 'layers.nika-a = "L0"
layers.nika-zombie = "L1"'

# Controls — each fault alone. A count check caught these, which is exactly
# why it looked like it worked.
expect 2 "a member with no layer, alone" "$BOTH" 'layers.nika-a = "L0"'
expect 2 "a zombie layer, alone" '"crates/nika-a"' 'layers.nika-a = "L0"
layers.nika-zombie = "L1"'

# The empty harvest was already guarded; keep it pinned.
expect 2 "no members parsed at all" '' 'layers.nika-a = "L0"'

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — a parity two errors can satisfy.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
