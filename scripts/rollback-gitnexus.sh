#!/usr/bin/env bash
# rollback-gitnexus.sh — fully remove GitNexus from Nika
#
# Usage: bash scripts/rollback-gitnexus.sh <BACKUP_DIR>
# Or for latest backup: bash scripts/rollback-gitnexus.sh $(ls -1dt ~/.nika-backups/gitnexus-* | head -1)
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "Usage: $0 <BACKUP_DIR>"
  echo "Latest backups:"
  ls -1dt ~/.nika-backups/gitnexus-* 2>/dev/null | head -5
  exit 1
fi

BK="$1"
REPO="/Users/thibaut/dev/supernovae/nika"
cd "$REPO"

[ -d "$BK" ] || { echo "❌ Backup dir not found: $BK"; exit 1; }

echo "═══ Rolling back from $BK ═══"

# Remove gitnexus artifacts
rm -rf .gitnexus .gitnexus.db node_modules

# Remove MCP entry (if claude CLI available)
if command -v claude >/dev/null 2>&1; then
  claude mcp remove gitnexus 2>/dev/null || true
fi

# Restore backed up files
[ -f "$BK/claude.json" ]           && cp "$BK/claude.json"           ~/.claude.json
[ -f "$BK/user-settings.json" ]    && cp "$BK/user-settings.json"    ~/.claude/settings.json
[ -f "$BK/project-settings.json" ] && cp "$BK/project-settings.json" .claude/settings.json
[ -f "$BK/mcp.json" ]              && cp "$BK/mcp.json"              .mcp.json
[ -f "$BK/gitignore" ]             && cp "$BK/gitignore"             .gitignore
[ -f "$BK/project-CLAUDE.md" ]     && cp "$BK/project-CLAUDE.md"     .claude/CLAUDE.md

# AGENTS.md / root CLAUDE.md (either restore or remove)
if [ -f "$BK/AGENTS.md" ]; then
  cp "$BK/AGENTS.md" AGENTS.md
else
  rm -f AGENTS.md
fi
if [ -f "$BK/root-CLAUDE.md" ]; then
  cp "$BK/root-CLAUDE.md" CLAUDE.md
else
  rm -f CLAUDE.md
fi

echo "✅ Rolled back from $BK"
bash scripts/verify-gitnexus-safe.sh
