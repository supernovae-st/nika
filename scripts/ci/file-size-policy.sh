#!/usr/bin/env bash
# The reviewed file-size policy (ADR-154's values, ADR-023's tiers), shared by the CI file ratchet
# (scripts/ci/check-loc-limits.sh) and the hygiene dashboard (vector 12,
# scripts/hygiene/check-file-loc.sh). ONE marker parser and ONE verdict, so both gates read the
# ADR's table the same way and state the same sentence for the same file.
# Sourcing this file does not change the caller's shell options.

# Each level is crossed only by MORE lines than it.
FILE_LOC_WARN=1500
FILE_LOC_LIMIT=2500
FILE_LOC_EXEMPT_CEILING=5000
# A `// LOC-EXEMPT: <reason> [<owner>]` marker carries a file above the limit only with one of
# these reasons, and never above the ceiling.
FILE_LOC_EXEMPT_REASONS="codegen lookup-table enum-mega"

# loc_exempt_marker <file> prints the reason of the first LOC-EXEMPT marker on the file's first 5
# lines, or nothing when there is none. A marker whose reason cannot be read prints its whole line,
# which no reason matches.
loc_exempt_marker() {
  head -5 "$1" | grep -E '^//[[:space:]]*LOC-EXEMPT:' | head -1 \
    | sed -E 's|^//[[:space:]]*LOC-EXEMPT:[[:space:]]*([a-z-]+).*|\1|'
}

# loc_exempt_allowed <reason> succeeds when the reason is on the whitelist.
loc_exempt_allowed() {
  local reason
  for reason in $FILE_LOC_EXEMPT_REASONS; do
    [ "$1" = "$reason" ] && return 0
  done
  return 1
}

# file_loc_verdict <file> <loc> prints the file's verdict:
#   green            at most FILE_LOC_WARN lines
#   warn             above the warning, at most the limit (never blocks)
#   exempt:<reason>  above the limit, carried by a whitelisted marker up to the ceiling
#   block:limit      above the limit without a marker
#   block:reason:<r> above the limit with a marker whose reason is not whitelisted
#   block:ceiling    above the ceiling, whatever the marker
file_loc_verdict() {
  local file="$1" loc="$2" reason
  if [ "$loc" -le "$FILE_LOC_WARN" ]; then
    echo green
  elif [ "$loc" -le "$FILE_LOC_LIMIT" ]; then
    echo warn
  elif [ "$loc" -gt "$FILE_LOC_EXEMPT_CEILING" ]; then
    echo block:ceiling
  else
    reason=$(loc_exempt_marker "$file")
    if [ -z "$reason" ]; then
      echo block:limit
    elif loc_exempt_allowed "$reason"; then
      echo "exempt:$reason"
    else
      echo "block:reason:$reason"
    fi
  fi
}

# file_loc_explain <verdict> prints the sentence every gate states for that verdict.
file_loc_explain() {
  case "$1" in
    green) echo "within the ${FILE_LOC_WARN}-line warning" ;;
    warn) echo "above the ${FILE_LOC_WARN}-line warning, within the ${FILE_LOC_LIMIT}-line limit" ;;
    exempt:*) echo "LOC-EXEMPT ${1#exempt:}, within the ${FILE_LOC_EXEMPT_CEILING}-line ceiling" ;;
    block:limit) echo "over the ${FILE_LOC_LIMIT}-line limit without a LOC-EXEMPT marker" ;;
    block:reason:*)
      echo "over the ${FILE_LOC_LIMIT}-line limit; LOC-EXEMPT reason '${1#block:reason:}'" \
        "is not one of: ${FILE_LOC_EXEMPT_REASONS}"
      ;;
    block:ceiling) echo "over the ${FILE_LOC_EXEMPT_CEILING}-line ceiling, which no marker lifts" ;;
    *) echo "unknown verdict '$1'" ;;
  esac
}
