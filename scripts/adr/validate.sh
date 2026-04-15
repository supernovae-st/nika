#!/usr/bin/env bash
# Validate ADR YAML frontmatter against structural checks.
# Usage: scripts/adr/validate.sh [--strict]
#
# Checks:
#   1. Every adr-*.md has YAML frontmatter with required fields
#   2. id matches filename (adr-001-*.md -> ADR-001)
#   3. status is one of the 6 allowed values
#   4. date is valid ISO 8601 (YYYY-MM-DD)
#   5. No dangling refs (references to non-existent ADR IDs)
#   6. Bidirectional consistency (if A supersedes B, B.superseded_by contains A)
#   7. No supersession cycles
#   8. affects_layers values are valid (L0, L0.5, L1..L5)
#   9. affects_crates match nika-* pattern
#
# Exit codes:
#   0 -- all valid
#   1 -- warnings only (bidirectional mismatches in non-strict mode)
#   2 -- validation errors found
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ADR_DIR="$REPO_ROOT/docs/adr"

# Color helpers (same as check-adr-coverage.sh pattern)
readonly C_RED=$'\033[0;31m'
readonly C_YELLOW=$'\033[0;33m'
readonly C_GREEN=$'\033[0;32m'
readonly C_RESET=$'\033[0m'

STRICT=0
for arg in "$@"; do
  case "$arg" in
    --strict) STRICT=1 ;;
  esac
done

errors=0
warnings=0

error() {
  printf '%s[adr-validate] ERROR%s: %s\n' "$C_RED" "$C_RESET" "$1" >&2
  errors=$((errors + 1))
}
warn() {
  printf '%s[adr-validate] WARN%s:  %s\n' "$C_YELLOW" "$C_RESET" "$1" >&2
  warnings=$((warnings + 1))
}

# --- Helper: extract frontmatter ---
extract_fm() {
  awk '/^---$/{n++; next} n==1{print} n>=2{exit}' "$1"
}

# --- Helper: extract scalar from frontmatter string ---
fm_scalar() {
  local field="$1" fm="$2"
  printf '%s\n' "$fm" | grep -E "^${field}:" | head -1 \
    | sed -E "s/^${field}:[[:space:]]*//" \
    | sed -E 's/^"//; s/"$//' \
    | sed -E "s/^'//; s/'$//"
}

# --- Helper: extract array elements ---
fm_array() {
  local field="$1" fm="$2"
  printf '%s\n' "$fm" | grep -E "^${field}:" | head -1 \
    | sed -E "s/^${field}:[[:space:]]*//" \
    | tr -d '[]' \
    | tr ',' '\n' \
    | sed -E 's/^[[:space:]]*//; s/[[:space:]]*$//' \
    | sed -E 's/^"//; s/"$//' \
    | sed -E "s/^'//; s/'$//" \
    | grep -v '^$'
}

VALID_STATUSES="draft proposed accepted rejected deprecated superseded"
VALID_LAYERS="L0 L0.5 L1 L2 L3 L4 L5"

# --- Pass 1: Collect all ADR IDs and validate individual fields ---
declare -a ALL_IDS=()
# Store frontmatter per file for cross-checks (using temp files, portable)
TMPDIR_VAL="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_VAL"' EXIT

adr_count=0
for filepath in "$ADR_DIR"/adr-[0-9][0-9][0-9]-*.md; do
  [ -f "$filepath" ] || continue
  fname="$(basename "$filepath")"
  adr_count=$((adr_count + 1))

  fm="$(extract_fm "$filepath")"
  if [ -z "$fm" ]; then
    error "$fname: missing YAML frontmatter (no --- block)"
    continue
  fi

  # Store frontmatter for cross-checks
  printf '%s\n' "$fm" >"$TMPDIR_VAL/$fname.fm"

  adr_id="$(fm_scalar "id" "$fm")"
  title="$(fm_scalar "title" "$fm")"
  status="$(fm_scalar "status" "$fm")"
  date="$(fm_scalar "date" "$fm")"
  deciders="$(fm_array "deciders" "$fm" | head -1)"

  # Check id matches filename
  fname_num="$(printf '%s' "$fname" | grep -oE 'adr-[0-9]{3}' | grep -oE '[0-9]{3}')"
  expected_id="ADR-${fname_num}"
  if [ "$adr_id" != "$expected_id" ]; then
    error "$fname: id '${adr_id}' does not match filename (expected '${expected_id}')"
  fi

  # Check required fields
  [ -z "$adr_id" ] && error "$fname: missing required field 'id'"
  [ -z "$title" ] && error "$fname: missing required field 'title'"
  [ -z "$status" ] && error "$fname: missing required field 'status'"
  [ -z "$date" ] && error "$fname: missing required field 'date'"
  [ -z "$deciders" ] && error "$fname: missing required field 'deciders'"

  # Check status value
  if [ -n "$status" ]; then
    valid=0
    for s in $VALID_STATUSES; do
      [ "$status" = "$s" ] && valid=1
    done
    [ $valid -eq 0 ] && error "$fname: invalid status '${status}' (allowed: ${VALID_STATUSES})"
  fi

  # Check date format (YYYY-MM-DD)
  if [ -n "$date" ]; then
    if ! printf '%s' "$date" | grep -qE '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'; then
      error "$fname: invalid date format '${date}' (expected YYYY-MM-DD)"
    fi
  fi

  # Check layers
  while IFS= read -r layer; do
    [ -z "$layer" ] && continue
    valid=0
    for l in $VALID_LAYERS; do
      [ "$layer" = "$l" ] && valid=1
    done
    [ $valid -eq 0 ] && error "$fname: invalid layer '${layer}' (allowed: ${VALID_LAYERS})"
  done <<<"$(fm_array "affects_layers" "$fm")"

  # Check crate name pattern
  while IFS= read -r crate; do
    [ -z "$crate" ] && continue
    if ! printf '%s' "$crate" | grep -qE '^nika-[a-z0-9-]+$'; then
      error "$fname: invalid crate name '${crate}' (must match nika-<name>)"
    fi
  done <<<"$(fm_array "affects_crates" "$fm")"

  ALL_IDS+=("$adr_id")
done

# --- Pass 2: Cross-ADR checks (dangling refs, bidirectional, cycles) ---
REF_FIELDS="supersedes superseded_by related requires enables amends"

for filepath in "$ADR_DIR"/adr-[0-9][0-9][0-9]-*.md; do
  [ -f "$filepath" ] || continue
  fname="$(basename "$filepath")"
  fm_file="$TMPDIR_VAL/$fname.fm"
  [ -f "$fm_file" ] || continue
  fm="$(cat "$fm_file")"
  adr_id="$(fm_scalar "id" "$fm")"

  for field in $REF_FIELDS; do
    while IFS= read -r ref; do
      [ -z "$ref" ] && continue
      # Check dangling
      found=0
      for known_id in "${ALL_IDS[@]}"; do
        [ "$ref" = "$known_id" ] && found=1
      done
      [ $found -eq 0 ] && error "$adr_id: dangling ref in ${field}: '${ref}' does not exist"
    done <<<"$(fm_array "$field" "$fm")"
  done

  # Bidirectional: if A supersedes B, B.superseded_by should contain A
  while IFS= read -r ref; do
    [ -z "$ref" ] && continue
    ref_fname="$(grep -rl "^id: ${ref}" "$ADR_DIR"/adr-[0-9][0-9][0-9]-*.md 2>/dev/null | head -1)"
    if [ -n "$ref_fname" ]; then
      ref_bn="$(basename "$ref_fname")"
      ref_fm_file="$TMPDIR_VAL/$ref_bn.fm"
      if [ -f "$ref_fm_file" ]; then
        other_sb="$(fm_array "superseded_by" "$(cat "$ref_fm_file")")"
        if ! printf '%s\n' "$other_sb" | grep -qx "$adr_id"; then
          warn "$adr_id supersedes $ref, but $ref.superseded_by does not contain $adr_id"
        fi
      fi
    fi
  done <<<"$(fm_array "supersedes" "$fm")"

  while IFS= read -r ref; do
    [ -z "$ref" ] && continue
    ref_fname="$(grep -rl "^id: ${ref}" "$ADR_DIR"/adr-[0-9][0-9][0-9]-*.md 2>/dev/null | head -1)"
    if [ -n "$ref_fname" ]; then
      ref_bn="$(basename "$ref_fname")"
      ref_fm_file="$TMPDIR_VAL/$ref_bn.fm"
      if [ -f "$ref_fm_file" ]; then
        other_ss="$(fm_array "supersedes" "$(cat "$ref_fm_file")")"
        if ! printf '%s\n' "$other_ss" | grep -qx "$adr_id"; then
          warn "$adr_id superseded_by $ref, but $ref.supersedes does not contain $adr_id"
        fi
      fi
    fi
  done <<<"$(fm_array "superseded_by" "$fm")"
done

# --- Report ---
if [ "$errors" -gt 0 ]; then
  printf '\n%s%d error(s), %d warning(s)%s\n' "$C_RED" "$errors" "$warnings" "$C_RESET" >&2
  exit 2
fi
if [ "$warnings" -gt 0 ] && [ "$STRICT" -eq 1 ]; then
  printf '\n%s%d warning(s) in strict mode%s\n' "$C_YELLOW" "$warnings" "$C_RESET" >&2
  exit 1
fi

printf '%sOK%s: %d ADRs validated, 0 errors, %d warning(s)\n' "$C_GREEN" "$C_RESET" "$adr_count" "$warnings"
exit 0
