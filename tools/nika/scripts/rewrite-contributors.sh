#!/usr/bin/env bash
# Rewrite git history to:
# 1. Remove Happy co-author from all commits
# 2. Add Nika co-author to commits that have Claude co-author
#
# ⚠️  This rewrites history - requires force push!
# Run from nika/ directory (not tools/nika)

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "╔═══════════════════════════════════════════════════════════════════════════════╗"
echo "║  🔄 GIT HISTORY REWRITE                                                       ║"
echo "╠═══════════════════════════════════════════════════════════════════════════════╣"
echo "║                                                                               ║"
echo "║  ACTIONS:                                                                     ║"
echo "║  • Remove: Co-Authored-By: Happy <yesreply@happy.engineering>                ║"
echo "║  • Remove: via [Happy](https://happy.engineering)                            ║"
echo "║  • Add: Co-Authored-By: Nika 🦋 <nika@supernovae.studio>                     ║"
echo "║                                                                               ║"
echo "╚═══════════════════════════════════════════════════════════════════════════════╝"
echo ""

# Check we're in the right repo
if [[ ! -f "tools/nika/Cargo.toml" ]]; then
  echo "❌ Run this from the nika/ repo root"
  exit 1
fi

# Create backup
BACKUP_TAG="backup-$(date +%Y%m%d-%H%M%S)"
git tag "$BACKUP_TAG"
echo "💾 Backup tag: $BACKUP_TAG"
echo ""

# Count before
HAPPY_BEFORE=$(git log --all --pretty=full | grep -c "yesreply@happy.engineering" || echo 0)
NIKA_BEFORE=$(git log --all --pretty=full | grep -c "nika@supernovae.studio" || echo 0)
echo "📊 Before: Happy=$HAPPY_BEFORE, Nika=$NIKA_BEFORE"

# Run git-filter-repo with inline Python
git-filter-repo --force --commit-callback '
import re

NIKA = b"Co-Authored-By: Nika \xf0\x9f\xa6\x8b <nika@supernovae.studio>"
CLAUDE = b"Co-Authored-By: Claude <noreply@anthropic.com>"

msg = commit.message

# Remove Happy
msg = re.sub(rb"Co-Authored-By: Happy <yesreply@happy\.engineering>\n?", b"", msg)
msg = re.sub(rb"via \[Happy\]\(https://happy\.engineering\)\n?", b"", msg)

# Add Nika if has Claude but not Nika
if CLAUDE in msg and b"nika@supernovae.studio" not in msg:
    msg = msg.replace(CLAUDE, CLAUDE + b"\n" + NIKA)

# Clean extra newlines
msg = re.sub(rb"\n{3,}", b"\n\n", msg)

commit.message = msg
'

echo ""

# Count after
HAPPY_AFTER=$(git log --all --pretty=full | grep -c "yesreply@happy.engineering" || echo 0)
NIKA_AFTER=$(git log --all --pretty=full | grep -c "nika@supernovae.studio" || echo 0)
echo "📊 After: Happy=$HAPPY_AFTER, Nika=$NIKA_AFTER"
echo ""

echo "╔═══════════════════════════════════════════════════════════════════════════════╗"
echo "║  ✅ REWRITE COMPLETE                                                          ║"
echo "╠═══════════════════════════════════════════════════════════════════════════════╣"
echo "║                                                                               ║"
echo "║  Happy: $HAPPY_BEFORE → $HAPPY_AFTER                                                           ║"
echo "║  Nika:  $NIKA_BEFORE → $NIKA_AFTER                                                            ║"
echo "║                                                                               ║"
echo "║  ⚠️  Next: git push --force origin main                                       ║"
echo "║  🗑️  After verify: git tag -d $BACKUP_TAG                                     ║"
echo "║                                                                               ║"
echo "╚═══════════════════════════════════════════════════════════════════════════════╝"
