#!/usr/bin/env bash
# verify-gitnexus-safe.sh — idempotent integrity check for Nika Diamond
#
# Run anytime to confirm:
#   - our 10 custom Claude Code hooks are intact
#   - .mcp.json still has nika + linear (+ gitnexus if installed)
#   - CLAUDE.md authority chain untouched
#   - .gitnexus/ gitignored
#   - hygiene dashboard green
set -u
REPO="/Users/thibaut/dev/supernovae/nika"
cd "$REPO"

FAIL=0
check() {
  local name="$1"
  local cmd="$2"
  if eval "$cmd" >/dev/null 2>&1; then
    echo "✅ $name"
  else
    echo "❌ $name"
    FAIL=1
  fi
}

echo "═══ GitNexus integrity verification ═══"

check "settings.json valid JSON" \
  "jq empty .claude/settings.json"

check "10+ hooks present" \
  "[ \$(jq '[.hooks|..|objects|select(.command)]|length' .claude/settings.json) -ge 10 ]"

check "nika MCP present" \
  "jq -e '.mcpServers.nika' .mcp.json"

check "linear MCP present" \
  "jq -e '.mcpServers.linear' .mcp.json"

check "CLAUDE.md authority chain intact" \
  "grep -q 'Authority hierarchy' .claude/CLAUDE.md"

check "CLAUDE.md free of gitnexus HTML markers" \
  "! grep -qE 'gitnexus-start|<!-- GITNEXUS' .claude/CLAUDE.md"

check ".gitnexus/ in .gitignore" \
  "grep -qE '^\.gitnexus/?\$' .gitignore"

check "no .gitnexus staged in git" \
  "! git diff --cached --name-only | grep -q '^.gitnexus'"

if [ -x scripts/hygiene/check-all.sh ]; then
  check "hygiene dashboard passes (no RED)" \
    "scripts/hygiene/check-all.sh --quiet"
fi

echo ""
if [ $FAIL -eq 0 ]; then
  echo "🎉 All checks passed — setup integrity OK"
else
  echo "⚠️  Some checks failed — review above"
  exit 1
fi
