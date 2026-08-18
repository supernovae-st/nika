#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-crate-size.sh
#
# Mutation proof for vector 24's DESCENT WINDOW (the [12000, 15000) band).
#
# The band exists so the red is not discovered AT the push, mid-merge-train.
# But the hygiene dashboard (check-all.sh) renders ONE line per vector — the
# FIRST one — and that line said only:
#
#     YELLOW: crate(s) in [12000, 15000) LOC range (the descent window):
#
# which is the SAME sentence at 12001 LOC and at 14999 LOC. The band was
# 3000 wide and reported as a single bit. Measured 2026-08-18 · nika-check
# sat at 14995/15000 — five lines of headroom — and read exactly like a
# crate with three thousand to spare. A warning that cannot say how close
# the wall is has already failed at the one job the band was added for.
#
# The header now names the TIGHTEST crate and its remaining headroom. These
# cases pin that, and the first one is the case that matters: the tightest
# crate is NOT the first row the counter emits, so a naive `head -1` on
# unsorted input names the wrong crate and still looks plausible.
#
# shellcheck disable=SC2329
set -uo pipefail

unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
VECTOR="$ROOT/scripts/hygiene/check-crate-size.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# Builds a fake repo whose scripts/ci/check-crate-size.sh is a stub:
#   · called plain          → replays $red_rows,  rc = $red_rc   (the 15000 probe)
#   · called CRATE_SIZE_MAX → replays $band_rows, rc = $band_rc  (the 12000 probe)
# The stub emits the real counter's line shape: `FAIL  <dir>  <n> LOC (max <m>)`.
scaffold() {
  local dir="$1" red_rows="$2" red_rc="$3" band_rows="$4" band_rc="$5"
  mkdir -p "$dir/scripts/hygiene" "$dir/scripts/ci"
  cp "$VECTOR" "$dir/scripts/hygiene/"
  printf '%s' "$red_rows" >"$dir/red.rows"
  printf '%s' "$band_rows" >"$dir/band.rows"
  printf '%s\n' "$red_rc" >"$dir/red.rc"
  printf '%s\n' "$band_rc" >"$dir/band.rc"
  cat >"$dir/scripts/ci/check-crate-size.sh" <<'STUB'
#!/usr/bin/env bash
# Stub of the ONE prod-LOC counter. Replays fixed rows so the wrapper's
# band arithmetic is what is under test, never the counting itself.
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [ -n "${CRATE_SIZE_MAX:-}" ]; then
  cat "$here/band.rows"
  exit "$(cat "$here/band.rc")"
fi
cat "$here/red.rows"
exit "$(cat "$here/red.rc")"
STUB
  chmod +x "$dir/scripts/ci/check-crate-size.sh"
}

# expect_rc <want-rc> <label> <red_rows> <red_rc> <band_rows> <band_rc>
expect_rc() {
  local want="$1" label="$2"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  scaffold "$dir" "$3" "$4" "$5" "$6"
  local rc
  bash "$dir/scripts/hygiene/check-crate-size.sh" >/dev/null 2>&1
  rc=$?
  if [ "$rc" = "$want" ]; then
    printf '  ok   %s (rc=%d)\n' "$label" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted rc=%s, got rc=%d\n' "$label" "$want" "$rc" >&2
  fi
}

# expect_says <needle> <label> <red_rows> <red_rc> <band_rows> <band_rc>
# Asserts the needle appears on the FIRST line — the only line the dashboard
# renders. A needle found further down would not reach the operator.
expect_says() {
  local needle="$1" label="$2"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  scaffold "$dir" "$3" "$4" "$5" "$6"
  local head_line
  head_line=$(bash "$dir/scripts/hygiene/check-crate-size.sh" 2>/dev/null | head -1)
  case "$head_line" in
    *"$needle"*)
      printf '  ok   %s\n' "$label"
      ;;
    *)
      fails=$((fails + 1))
      printf '  FAIL %s — first line lacks %s\n       got: %s\n' \
        "$label" "$needle" "$head_line" >&2
      ;;
  esac
}

# The counter emits in workspace order, NOT sorted by size: the tightest crate
# (nika-check) is the LAST row here, and a comfortable one is first.
UNSORTED='FAIL  crates/nika-builtin  13302 LOC (max 12000)
FAIL  crates/nika-runtime  14929 LOC (max 12000)
FAIL  crates/nika-check  14995 LOC (max 12000)
'
ONE='FAIL  crates/nika-solo  12500 LOC (max 12000)
'
OVER='FAIL  crates/nika-over  15400 LOC (max 15000)
'

echo "vector 24 · descent-window mutation proof"

# THE case · the header must name the TIGHTEST crate, not the first row.
expect_says 'crates/nika-check' 'header names the tightest crate' \
  '' 0 "$UNSORTED" 1

# ...and it must say HOW CLOSE. 15000 - 14995 = 5. This is the number whose
# absence let a crate sit five lines from a blocking push, unseen.
expect_says '5 LOC of headroom' 'header states the remaining headroom' \
  '' 0 "$UNSORTED" 1

# Control · the comfortable crate must NOT be the one promoted to the header.
expect_says '14995/15000' 'header carries the tightest figure, not a band label' \
  '' 0 "$UNSORTED" 1

# Control · a single crate in the band still names itself.
expect_says 'crates/nika-solo' 'a lone crate in the band is named' \
  '' 0 "$ONE" 1

# The verdict semantics are untouched by the message change.
expect_rc 1 'band is YELLOW, never blocking' '' 0 "$UNSORTED" 1
expect_rc 2 'over budget is RED' "$OVER" 1 '' 0
expect_rc 0 'all crates under the warn line is GREEN' '' 0 '' 0

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — a band that cannot say how close the wall is.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
