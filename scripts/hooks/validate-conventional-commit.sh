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

# Scopes that nothing on disk can derive: sibling repos, cross-cutting concerns
# and named surfaces. Everything the tree DOES describe — workspace crates and
# script surfaces — is derived below, because a hand-kept list goes stale the
# moment either grows. Measured 2026-07-30: `dap`, `proof`, `check` and `pack`
# warned as unknown while `runtime` and `cli` passed, purely by order of
# addition; then `media` warned on the very commit that fixed the crate half.
readonly UNDERIVABLE_SCOPES='workspace|docs|api|hq|deps|dx|rules|skills|claude|submodules|templates|nika|jungo|novanet|qrcodeai|engine|website|client-sdk|sdk|design-skill|homebrew|audit-workflow|brew|skill|provider|resilience|db|tui|diamond|coherence|typos'

# Every workspace crate, as both its full name and its `nika-`-stripped short
# form (`fix(dap)` and `fix(nika-dap)` are the same intent, and history carries
# both), plus every script surface (`scripts/media` → `media`, which is how
# `ci`, `hooks`, `hygiene`, `adr`, `release` and `test` earn their place too).
# Falls back to the pattern alone if the tree is unreachable, since this check
# only ever warns and must never block a commit over its own plumbing.
derive_tree_scopes() {
  local root
  root="$(git rev-parse --show-toplevel 2>/dev/null)" || return 1
  [[ -d "$root/crates" ]] || return 1
  {
    # shellcheck disable=SC2012  # names only, no metadata needed
    ls -1 "$root/crates" 2>/dev/null \
      | sed -e 's/$/|/' -e 's/^nika-\(.*\)|$/nika-\1|\1|/'
    find "$root/scripts" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; 2>/dev/null \
      | sed 's/$/|/'
  } | tr -d '\n' | sed 's/|$//'
}

TREE_SCOPES="$(derive_tree_scopes || true)"
if [[ -n "$TREE_SCOPES" ]]; then
  readonly VALID_SCOPES="${UNDERIVABLE_SCOPES}|${TREE_SCOPES}"
else
  readonly VALID_SCOPES="${UNDERIVABLE_SCOPES}|ci|hooks|hygiene|adr|release|test|nika-[a-z0-9-]+"
fi
readonly COAUTHOR_PATTERN='^Co-Authored-By: Nika 🦋 <nika@supernovae\.studio>$'
# The tolerant form, for the shapes that really occur (`nika-bot 🦋`,
# `nika-release[bot]`). Anchored at BOTH ends, and the identity it pins is
# the EMAIL — a display name is decoration, an address is a claim.
readonly COAUTHOR_FALLBACK='^Co-Authored-By: [Nn]ika[^<]* <nika@supernovae\.studio>$'
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
    warn "Scope '${SCOPE_INNER}' is neither a workspace crate nor a known surface. If it is intentional, add it to UNDERIVABLE_SCOPES in scripts/hooks/validate-conventional-commit.sh (crate scopes need no edit — they are derived from crates/)."
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

  # Body line length. Trailers are exempt: they are not prose and their
  # shape is fixed by whoever consumes them. `Signed-off-by:` was missing
  # from this list, and the DCO trailer this repo REQUIRES is 77 chars —
  # so every correctly signed commit earned a warning it could do nothing
  # about, which is how a gate teaches people to skim past its output.
  LINE_LEN="${#line}"
  if ((LINE_LEN > MAX_BODY_LINE)) \
    && ! [[ "$line" =~ ^(https?://|[Cc]o-[Aa]uthored-[Bb]y:|[Ss]igned-off-by:) ]]; then
    warn "Body line $((i + 1)) is ${LINE_LEN} chars (target ≤${MAX_BODY_LINE}): '${line}'"
  fi
done

# ---------------------------------------------------------------------------
# 7. Co-Authored-By: Nika 🦋 must be present in footer (as a REAL trailer)
# ---------------------------------------------------------------------------
# P0-2 Batch H+: both patterns use POSIX `grep -E`, portable across BSD +
# GNU. The previous `grep -qP` path relied on Perl regex, which BSD grep
# (default macOS) silently rejects, and `2>/dev/null` hid the failure.
#
# 2026-08-14: the comment here used to claim "both patterns anchored with
# ^...$". The first was. The second was `^Co-Authored-By: Nika( |$)`, where
# `( |$)` is an ALTERNATION INSIDE the pattern, not a terminator for it —
# match `Nika` followed by a space and everything to the right is
# unconstrained. So this passed, rc=0:
#
#     Co-Authored-By: Nika Impostor <evil@example.com>
#
# and so did `Co-Authored-By: Nika Claude <claude@anthropic.com>`, which is
# the one attribution the alignment doctrine exists to keep out — reachable
# by prefixing the word `Nika`. A bare `Claude` trailer was correctly
# rejected the whole time, which is what made the hole invisible.
#
# The fallback is now anchored at both ends and pins the EMAIL rather than
# the display name.
#
# Only enforce on non-automated commits (already returned early for merge/fixup above)
if ! printf '%s\n' "${LINES[@]}" | grep -qE "$COAUTHOR_PATTERN" \
  && ! printf '%s\n' "${LINES[@]}" | grep -qE "$COAUTHOR_FALLBACK"; then
  die "Missing footer: Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
  Add it to your commit template or use: git commit --trailer 'Co-Authored-By: Nika 🦋 <nika@supernovae.studio>'"
fi

printf '[commit-msg] OK — %s\n' "$HEADER" >&2
exit 0
