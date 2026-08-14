#!/usr/bin/env bash
# Mutation proof for vector 9 (check-org-readme.sh).
#
# Two defects, both of them a verdict outrunning its evidence.
#
# The final `echo "OK (... counts match canon)"` sat OUTSIDE the if/else, so
# on an unreachable canon.yaml the script printed "counts parity skipped"
# and then asserted the parity anyway, rc=0. It announced the measurement it
# had just declined to make.
#
# And membership was a bare substring test: one mention of
# `nika-actions-starter` satisfied `nika`, `nika-action` and
# `nika-actions-starter` at once, so a repo dropped from the profile stayed
# "present" behind a longer sibling's name.
#
# Nothing here touches the network: gh and curl are shims.
#
# shellcheck disable=SC2329
set -uo pipefail

unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
VECTOR="$ROOT/scripts/hygiene/check-org-readme.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

ALL_REPOS=(
  nika nika.sh nika-client nika-spec nika-docs nika-vscode nika-plugins
  gh-nika nika-registry homebrew-tap nika-action nika-actions-starter
  nika-estate
)
COUNTS='4 verbs, 17 providers, 25 builtin tools'

# seed <dir> <profile-body> <canon-reachable(y|n)> [canon-counts]
seed() {
  local dir="$1" profile="$2" reachable="$3" counts="${4:-4 17 25}"
  mkdir -p "$dir/bin"
  printf '%s\n' "$profile" >"$dir/profile.md"
  # gh shim: only the one API call this vector makes.
  cat >"$dir/bin/gh" <<SHIM
#!/usr/bin/env bash
base64 <"$dir/profile.md"
SHIM
  # shellcheck disable=SC2086  # splitting "4 17 25" into $1 $2 $3 is the point
  set -- $counts
  if [ "$reachable" = y ]; then
    cat >"$dir/bin/curl" <<SHIM
#!/usr/bin/env bash
printf 'counts:\n  verbs: $1\n  providers: $2\n  builtins: $3\n'
SHIM
  else
    printf '#!/usr/bin/env bash\nexit 7\n' >"$dir/bin/curl"
  fi
  chmod +x "$dir/bin/gh" "$dir/bin/curl"
}

# expect <label> <want-rc> <needle-in-output> <must-NOT-appear> <profile> <reachable> [counts]
expect() {
  local label="$1" wantrc="$2" needle="$3" forbidden="$4"
  local profile="$5" reachable="$6" counts="${7:-4 17 25}"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  seed "$dir" "$profile" "$reachable" "$counts"
  local out rc
  out="$(PATH="$dir/bin:/usr/bin:/bin" bash "$VECTOR" 2>&1)"
  rc=$?
  local ok=1
  [ "$rc" = "$wantrc" ] || ok=0
  [ -n "$needle" ] && ! printf '%s' "$out" | grep -qF -- "$needle" && ok=0
  [ -n "$forbidden" ] && printf '%s' "$out" | grep -qF -- "$forbidden" && ok=0
  if [ "$ok" = 1 ]; then
    printf '  ok   %s (rc=%d)\n' "$label" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted rc=%s with "%s" and without "%s"; got rc=%d: %s\n' \
      "$label" "$wantrc" "$needle" "$forbidden" "$rc" \
      "$(printf '%s' "$out" | tr '\n' ' ')" >&2
  fi
}

FULL="$(printf '%s\n' "${ALL_REPOS[@]}") $COUNTS"
# Every repo EXCEPT a standalone nika-action — only the longer sibling.
# A substring test cannot tell this apart from the full profile.
NO_ACTION="$(printf '%s\n' "${ALL_REPOS[@]}" | grep -vx 'nika-action') $COUNTS"

echo "vector 9 · mutation proof"

expect "everything present, canon reachable" 0 'counts match canon' '' "$FULL" y
expect "canon UNREACHABLE — must not claim parity" 1 'counts NOT verified' 'counts match canon' "$FULL" n
expect "a repo missing from the profile" 1 'missing from profile' '' \
  "$(printf '%s\n' "${ALL_REPOS[@]}" | grep -vx 'nika-docs') $COUNTS" y
expect "counts drift against canon" 1 'counts drift' '' "$FULL" y "9 99 999"
expect "only the longer sibling — nika-action is gone" 1 'missing from profile' '' "$NO_ACTION" y

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — the vector claims what it did not measure.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
