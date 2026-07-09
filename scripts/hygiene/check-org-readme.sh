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
# redirects the old name); nika-agents + nika-registry joined the profile with
# the 2026-07-09 storefront-truth pass (.github#1).
for repo in nika nika.sh nika-client nika-spec nika-docs nika-vscode nika-agents nika-registry homebrew-tap nika-site-audit; do
  echo "$content" | grep -q "$repo" || missing="${missing}${repo} "
done

if [ -n "$missing" ]; then
  echo "missing from profile: ${missing}"
  exit 1
fi
echo "OK (all canonical repos listed)"
exit 0
