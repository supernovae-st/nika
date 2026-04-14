#!/usr/bin/env bash
# Vector 9: Org profile README mentions all 6 canonical repos.
set -u
if ! command -v gh >/dev/null; then echo "gh not installed"; exit 1; fi

content="$(gh api repos/supernovae-st/.github/contents/profile/README.md --jq .content 2>/dev/null | base64 -d 2>/dev/null)"
[ -z "$content" ] && { echo "cannot read profile README"; exit 1; }

missing=""
for repo in nika nika.sh nika-client nika-registry nika-design-skill homebrew; do
  echo "$content" | grep -q "$repo" || missing="${missing}${repo} "
done

if [ -n "$missing" ]; then
  echo "missing from profile: ${missing}"; exit 1
fi
echo "OK (all 6 repos listed)"; exit 0
