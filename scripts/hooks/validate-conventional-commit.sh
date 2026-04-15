#!/usr/bin/env bash
# validate-conventional-commit.sh — Tier 1 commit-msg gate
#
# Validates:
#   1. Conventional Commits format: type(scope): description
#   2. Type in allowed whitelist
#   3. Subject in lowercase (first char)
#   4. Header ≤100 chars
#   5. Body lines ≤72 chars (after blank separator)
#   6. No trailing whitespace on any line
#   7. Co-Authored-By: Nika 🦋 <nika@supernovae.studio> in footer
#
# Usage: validate-conventional-commit.sh <commit-msg-file>
# Exit:  0 = valid | 1 = invalid (prints reason to stderr)
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -Eeuo pipefail

readonly VALID_TYPES='feat|fix|chore|docs|perf|test|refactor|build|ci|revert|security'
readonly VALID_SCOPES='workspace|kernel|catalog|error|kernel-mock|ci|hygiene|docs|adr|hq|deps|release|dx|rules|skills|hooks|claude|submodules|templates|nika|jungo|novanet|qrcodeai|engine|website|client-sdk|sdk|design-skill|homebrew|audit-workflow|brew|skill|runtime|provider|dag|event|resilience|core|db|mcp|cli|tui|schema'
readonly COAUTHOR_PATTERN='^Co-Authored-By: Nika 🦋 <nika@supernovae\.studio>$'
readonly MAX_HEADER=100
readonly MAX_BODY_LINE=72

MSG_FILE="${1:-}"

# ---------------------------------------------------------------------------
die() {
  printf '\n[commit-msg] REJECTED: %s\n\n' "$1" >&2
  printf 'See: https://www.conventionalcommits.org/\n' >&2
  exit 1
}

warn() {
  printf '[commit-msg] WARNING: %s\n' "$1" >&2
}
# ---------------------------------------------------------------------------

[[ -n "$MSG_FILE" && -f "$MSG_FILE" ]] || die "no commit message file passed (got: '${MSG_FILE:-}')"

# Read the commit message, stripping comment lines (# ...) git injects
mapfile -t LINES < <(grep -v '^#' "$MSG_FILE" || true)

# Reconstruct cleaned message
FULL_MSG="$(printf '%s\n' "${LINES[@]}")"

# Skip automated commit patterns (merge commits, fixup!, squash!, dependabot, release-plz)
HEADER="${LINES[0]:-}"
if [[ "$HEADER" =~ ^(Merge\ |fixup\!|squash\!|Initial\ commit|Revert\ \"Revert) ]] \
  || printf '%s\n' "$FULL_MSG" | grep -q '\[skip ci\]'; then
  exit 0
fi

# ---------------------------------------------------------------------------
# 1. Header format
# ---------------------------------------------------------------------------
# Pattern: type(scope): description  OR  type: description
HEADER_RE="^(${VALID_TYPES})(\([^)]+\))?: .+"
if ! [[ "$HEADER" =~ $HEADER_RE ]]; then
  die "Header must match 'type(scope): description'. Got: '${HEADER}'"
fi

COMMIT_SCOPE="${BASH_REMATCH[2]:-}" # includes parens, may be empty
DESCRIPTION="${HEADER#*: }"

# ---------------------------------------------------------------------------
# 2. Scope validation (warning only — scope-enum is advisory per commitlint config)
# ---------------------------------------------------------------------------
if [[ -n "$COMMIT_SCOPE" ]]; then
  SCOPE_INNER="${COMMIT_SCOPE:1:${#COMMIT_SCOPE}-2}" # strip parens
  if ! printf '%s' "$SCOPE_INNER" | grep -qE "^(${VALID_SCOPES})$"; then
    warn "Scope '${SCOPE_INNER}' is not in the known list. Add it to commitlint.config.js if intentional."
  fi
fi

# ---------------------------------------------------------------------------
# 3. Subject case — first character of description must be lowercase
# ---------------------------------------------------------------------------
FIRST_CHAR="${DESCRIPTION:0:1}"
if [[ "$FIRST_CHAR" =~ [A-Z] ]]; then
  die "Description must start lowercase. Got: '${DESCRIPTION}'"
fi

# ---------------------------------------------------------------------------
# 4. No trailing period on subject
# ---------------------------------------------------------------------------
if [[ "$DESCRIPTION" == *. ]]; then
  die "Description must not end with a period. Got: '${DESCRIPTION}'"
fi

# ---------------------------------------------------------------------------
# 5. Header length
# ---------------------------------------------------------------------------
HEADER_LEN="${#HEADER}"
if ((HEADER_LEN > MAX_HEADER)); then
  die "Header is ${HEADER_LEN} chars (max ${MAX_HEADER}). Shorten: '${HEADER}'"
fi

# ---------------------------------------------------------------------------
# 6. Body validation (lines after blank separator)
# ---------------------------------------------------------------------------
for ((i = 1; i < ${#LINES[@]}; i++)); do
  line="${LINES[$i]}"

  # First non-header line must be blank (body-leading-blank)
  if ((i == 1)) && [[ -n "$line" ]]; then
    die "Missing blank line between header and body. Add an empty line after '${HEADER}'"
  fi

  # Trailing whitespace
  if [[ "$line" =~ [[:space:]]$ ]]; then
    die "Trailing whitespace on line $((i + 1)): '${line}'"
  fi

  # Body line length (skip footer lines that contain URLs — common exception)
  LINE_LEN="${#line}"
  if ((LINE_LEN > MAX_BODY_LINE)) && ! [[ "$line" =~ ^(https?://|Co-Authored-By:) ]]; then
    warn "Body line $((i + 1)) is ${LINE_LEN} chars (target ≤${MAX_BODY_LINE}): '${line}'"
  fi
done

# ---------------------------------------------------------------------------
# 7. Co-Authored-By: Nika 🦋 must be present in footer (as a REAL trailer)
# ---------------------------------------------------------------------------
# P0-2 Batch H+: both patterns anchored with ^...$ to reject commits that
# only MENTION the trailer in prose (docs, comments, code fences) but lack
# an actual trailer line. The previous `grep -qP` path relied on Perl regex
# which BSD grep (default macOS) silently rejects; `2>/dev/null` hid the
# failure and the non-anchored fallback made this gate substring-permissive.
# Both patterns now use POSIX `grep -E`, portable across BSD + GNU.
#
# Only enforce on non-automated commits (already returned early for merge/fixup above)
if ! printf '%s\n' "${LINES[@]}" | grep -qE "$COAUTHOR_PATTERN" \
  && ! printf '%s\n' "${LINES[@]}" | grep -qE '^Co-Authored-By: Nika( |$)'; then
  die "Missing footer: Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
  Add it to your commit template or use: git commit --trailer 'Co-Authored-By: Nika 🦋 <nika@supernovae.studio>'"
fi

printf '[commit-msg] OK — %s\n' "$HEADER" >&2
exit 0
