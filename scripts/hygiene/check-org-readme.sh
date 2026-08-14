#!/usr/bin/env bash
# Vector 9: Org profile README mentions every canonical public repo.
set -u
if ! command -v gh >/dev/null; then
  echo "gh not installed"
  exit 1
fi

content="$(gh api repos/supernovae-st/.github/contents/profile/README.md --jq .content 2>/dev/null | base64 -d 2>/dev/null)"
[ -z "$content" ] && {
  echo "cannot read profile README"
  exit 1
}

missing=""
# homebrew-nika was RENAMED homebrew-tap (canonical per api .full_name · GitHub
# redirects the old name); nika-plugins + nika-registry joined the profile with
# the 2026-07-09 storefront-truth pass (.github#1).
#
# The list below IS the city (D-2026-07-29-N10 · thirteen buildings). It gained
# nika-action, nika-actions-starter, gh-nika and nika-estate, which were doing
# real work outside the map, and lost nika-site-audit and nika-starter.
#
# 2026-08-14 · that exclusion was justified here as "both archived". The API
# says otherwise for one of them: nika-site-audit IS archived, but
# nika-starter is archived=false, private=false, and was updated 2026-08-04.
# Whether a live public repo belongs on the front page is a curation call and
# this vector does not make it — but the stated reason was checkable and
# wrong, so it no longer claims it.
#
# Matching is WHOLE-WORD. `grep -q "$repo"` was a bare substring test, so one
# mention of `nika-actions-starter` satisfied `nika`, `nika-action` AND
# `nika-actions-starter` at once, and `nika.sh`'s unescaped `.` matched any
# character. All thirteen appear as whole words on the profile today, so this
# changes no verdict — it stops a future removal from being covered by a
# longer sibling's name.
REPOS=(
  nika nika.sh nika-client nika-spec nika-docs nika-vscode nika-plugins
  gh-nika nika-registry homebrew-tap nika-action nika-actions-starter
  nika-estate
)
for repo in "${REPOS[@]}"; do
  escaped="${repo//./\\.}"
  printf '%s' "$content" \
    | grep -qE "(^|[^A-Za-z0-9._-])${escaped}([^A-Za-z0-9._-]|$)" \
    || missing="${missing}${repo} "
done

if [ -n "$missing" ]; then
  echo "missing from profile: ${missing}"
  exit 1
fi

# Counts parity · the profile quotes canon.yaml and MUST track it (the
# 2026-07-09 storefront drift: « 14 providers, 23 builtin tools » lived on
# the page for weeks while canon said 16/25 · stress-to-ratchet graduation,
# ≥3 same-class incidents that day). canon.yaml is the SSOT — when a count
# moves there, this goes RED until the vitrine follows.
canon="$(curl -fsSL --max-time 15 https://raw.githubusercontent.com/supernovae-st/nika-spec/main/canon.yaml 2>/dev/null)"
if [ -n "$canon" ]; then
  verbs="$(echo "$canon" | awk '/^counts:/{f=1} f && /^  verbs:/{print $2; exit}')"
  providers="$(echo "$canon" | awk '/^counts:/{f=1} f && /^  providers:/{print $2; exit}')"
  builtins="$(echo "$canon" | awk '/^counts:/{f=1} f && /^  builtins:/{print $2; exit}')"
  expected="${verbs} verbs, ${providers} providers, ${builtins} builtin tools"
  if ! echo "$content" | grep -q "$expected"; then
    echo "counts drift: profile lacks \"$expected\" (canon.yaml is the SSOT)"
    exit 1
  fi
else
  # YELLOW, not green. This branch used to print "counts parity skipped" and
  # then fall through to a verdict that says "counts match canon" — the
  # script announced the measurement it had just declined to make, and
  # returned 0 while doing it. A verdict must never claim a measurement that
  # did not happen. Tier 1 (yellow) is right here: an unreachable network
  # source is not a drift finding, so it must not block, but it is not a
  # clean bill of health either.
  echo "YELLOW (all ${#REPOS[@]} canonical repos listed · counts NOT verified:"
  echo "        canon.yaml unreachable, so the profile's quoted counts were"
  echo "        never compared against the SSOT)"
  exit 1
fi

echo "OK (all ${#REPOS[@]} canonical repos listed · counts match canon)"
exit 0
