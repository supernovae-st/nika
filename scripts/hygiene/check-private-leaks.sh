#!/usr/bin/env bash
# Vector 14: no private memory paths referenced in PUBLIC tracked files.
# .claude/projects/.../memory/ is private; public code must never reference it.
# `.claude/CLAUDE.md` is allowed to reference `~/.claude/...` as pointer —
# it's the local-dev onboarding doc, user path is expected context.
set -uo pipefail
leaks=$(git grep -l -E '\.claude/projects/-Users-thibaut' \
        -- ':!.gitignore' ':!*.lock' \
           ':!.claude/CLAUDE.md' ':!.claude/rules/*.md' \
           ':!scripts/hygiene/*.sh' \
        2>/dev/null || true)

if [ -n "$leaks" ]; then
  echo "private path leak in: ${leaks}"; exit 2
fi
echo "OK (no private paths in tracked code)"; exit 0
