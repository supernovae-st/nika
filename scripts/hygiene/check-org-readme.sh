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
# real work outside the map, and lost nika-site-audit and nika-starter, which
# were archived: the first because its syntax drifted from the released grammar,
# the second because it shared nine of twelve files with nika-actions-starter.
# An archived repo must NOT be advertised on the front page, so this vector
# tracks the living city rather than a frozen snapshot of it.
for repo in nika nika.sh nika-client nika-spec nika-docs nika-vscode nika-plugins \
  gh-nika nika-registry homebrew-tap nika-action nika-actions-starter \
  nika-estate; do
  echo "$content" | grep -q "$repo" || missing="${missing}${repo} "
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
  echo "warn: canon.yaml unreachable — counts parity skipped"
fi

echo "OK (all canonical repos listed · counts match canon)"
exit 0
