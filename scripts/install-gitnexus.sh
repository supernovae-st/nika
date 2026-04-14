#!/usr/bin/env bash
# install-gitnexus.sh — paranoid install for Nika Diamond
#
# MCP-only install. Skips `gitnexus setup` (touches globals aggressively).
# Runs `gitnexus analyze --skip-agents-md` to preserve our authority-chain
# CLAUDE.md.
#
# Safety:
#   - Backups to ~/.nika-backups/gitnexus-YYYYMMDD-HHMMSS/
#   - .gitnexus/ pre-added to .gitignore
#   - Integrity verification after install
#   - Rollback script created
set -euo pipefail

REPO="/Users/thibaut/dev/supernovae/nika"
BACKUP_DIR="$HOME/.nika-backups/gitnexus-$(date +%Y%m%d-%H%M%S)"
cd "$REPO"

echo "═══ PHASE 0 — Pre-flight ═══"
node --version | grep -qE 'v(2[0-9]|[3-9][0-9])\.' || { echo "❌ Need Node 20+"; exit 1; }
jq empty .claude/settings.json || { echo "❌ settings.json invalid JSON"; exit 1; }
HOOK_COUNT=$(jq '[.hooks | .. | objects | select(.command)] | length' .claude/settings.json)
echo "✅ Pre-flight OK: Node $(node --version), $HOOK_COUNT hooks"

echo "═══ PHASE 1 — Backups → $BACKUP_DIR ═══"
mkdir -p "$BACKUP_DIR"
cp ~/.claude.json                    "$BACKUP_DIR/claude.json"           2>/dev/null || true
cp ~/.claude/settings.json           "$BACKUP_DIR/user-settings.json"    2>/dev/null || true
cp "$REPO/.claude/settings.json"     "$BACKUP_DIR/project-settings.json"
cp "$REPO/.mcp.json"                 "$BACKUP_DIR/mcp.json"              2>/dev/null || true
cp "$REPO/.gitignore"                "$BACKUP_DIR/gitignore"
cp "$REPO/.claude/CLAUDE.md"         "$BACKUP_DIR/project-CLAUDE.md"
[ -f AGENTS.md ] && cp AGENTS.md "$BACKUP_DIR/AGENTS.md" || true
[ -f CLAUDE.md ] && cp CLAUDE.md "$BACKUP_DIR/root-CLAUDE.md" || true
echo "✅ Backups created"

echo "═══ PHASE 2 — Preventive hardening ═══"
if ! grep -qE '^\.gitnexus/?$' .gitignore; then
  {
    echo ""
    echo "# GitNexus analyzer artifacts (PolyForm Noncommercial, MCP-only install)"
    echo ".gitnexus/"
    echo ".gitnexus.db"
    echo "node_modules/"
    echo "AGENTS.md"
  } >> .gitignore
  echo "✅ .gitignore hardened"
else
  echo "ℹ️  .gitnexus/ already ignored"
fi

echo "═══ PHASE 3 — MCP-only install (project scope) ═══"
if command -v claude >/dev/null 2>&1; then
  claude mcp add gitnexus --scope local -- npx -y gitnexus@latest mcp 2>&1 | tail -3 || {
    echo "⚠️  claude mcp add failed, trying direct JSON edit"
    jq '.mcpServers.gitnexus = {"command":"npx","args":["-y","gitnexus@latest","mcp"]}' .mcp.json > /tmp/mcp-new.json
    mv /tmp/mcp-new.json .mcp.json
  }
else
  echo "⚠️  claude CLI not in PATH, editing .mcp.json directly"
  jq '.mcpServers.gitnexus = {"command":"npx","args":["-y","gitnexus@latest","mcp"]}' .mcp.json > /tmp/mcp-new.json
  mv /tmp/mcp-new.json .mcp.json
fi
jq -e '.mcpServers.gitnexus' .mcp.json >/dev/null && echo "✅ gitnexus MCP registered"

echo "═══ PHASE 4 — Analyze (skip-agents-md, skip-embeddings) ═══"
NODE_OPTIONS="--max-old-space-size=8192" \
  npx -y gitnexus@latest analyze --skip-agents-md --skip-embeddings --verbose \
  2>&1 | tee "$BACKUP_DIR/analyze.log" | tail -20
if [ ! -d .gitnexus ]; then
  echo "❌ .gitnexus/ not created"; exit 1
fi
du -sh .gitnexus/ | awk '{print "✅ .gitnexus/ size: "$1}'

echo "═══ PHASE 5 — Integrity verification ═══"
bash "$REPO/scripts/verify-gitnexus-safe.sh"

echo ""
echo "🎉 DONE. Backup: $BACKUP_DIR"
echo "   Rollback: bash $REPO/scripts/rollback-gitnexus.sh \"$BACKUP_DIR\""
echo "   Re-verify anytime: bash $REPO/scripts/verify-gitnexus-safe.sh"
