#!/bin/bash
# Quick state dashboard — prints current Nika state in terminal
# Usage: ./scripts/nika-stats.sh

set -e

cd "$(dirname "$0")/.."

CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

ENGINE_LOC=$(find tools/nika-engine/src -name '*.rs' -exec cat {} + | wc -l | tr -d ' ')
WORKSPACE_LOC=$(find tools/*/src -name '*.rs' -exec cat {} + | wc -l | tr -d ' ')
CRATES=$(find tools -maxdepth 2 -name Cargo.toml | wc -l | tr -d ' ')
HEAD=$(git log -1 --format='%h')
HEAD_MSG=$(git log -1 --format='%s')
COMMITS_TODAY=$(git log --since="midnight" --oneline | wc -l | tr -d ' ')
FILES_OVER_1500=$(find tools/*/src -name '*.rs' ! -name 'tests*.rs' -exec wc -l {} + | awk '$1 > 1500' | wc -l | tr -d ' ')

# Dispatch arm count
DISPATCH_NOT_IMPL=$(grep -c '=> Err(RuntimeError::NotImplemented' tools/nika-runtime/src/dispatch.rs 2>/dev/null || echo "5")
DISPATCH_LIVE=$((5 - DISPATCH_NOT_IMPL))

echo -e "${BOLD}╔══════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}║          🦋 NIKA CONSTELLATION — LIVE STATE                  ║${NC}"
echo -e "${BOLD}╚══════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "  ${CYAN}HEAD:${NC}        ${HEAD}  ${HEAD_MSG}"
echo -e "  ${CYAN}Today:${NC}       ${COMMITS_TODAY} commits"
echo ""
echo -e "  ${BOLD}📏 SIZE${NC}"
echo -e "  Engine:      ${BOLD}${ENGINE_LOC}${NC} LOC (target ≤100k, $(( ENGINE_LOC - 100000 )) to go)"
echo -e "  Workspace:   ${WORKSPACE_LOC} LOC"
echo -e "  Crates:      ${CRATES}"
echo -e "  Files >1500: ${FILES_OVER_1500}"
echo ""
echo -e "  ${BOLD}⚡ DISPATCH${NC}"
if [ "$DISPATCH_LIVE" -eq 0 ]; then
    echo -e "  Arms live:   ${RED}0/5${NC} — all NotImplemented"
elif [ "$DISPATCH_LIVE" -lt 5 ]; then
    echo -e "  Arms live:   ${YELLOW}${DISPATCH_LIVE}/5${NC} — partial (compile only, 0 production callers)"
else
    echo -e "  Arms live:   ${GREEN}5/5${NC} — all active"
fi
echo ""
echo -e "  ${BOLD}📊 DASHBOARD${NC}"
if [ -f docs/constellation/index.html ]; then
    DASH_AGE=$(($(date +%s) - $(stat -f %m docs/constellation/index.html 2>/dev/null || stat -c %Y docs/constellation/index.html)))
    DASH_AGE_MIN=$((DASH_AGE / 60))
    if [ $DASH_AGE_MIN -lt 60 ]; then
        echo -e "  Last update: ${GREEN}${DASH_AGE_MIN} min ago${NC}"
    elif [ $DASH_AGE_MIN -lt 1440 ]; then
        echo -e "  Last update: ${YELLOW}$((DASH_AGE_MIN / 60)) hours ago${NC}"
    else
        echo -e "  Last update: ${RED}$((DASH_AGE_MIN / 1440)) days ago${NC} — refresh with: python3 scripts/update-constellation-dashboard.py"
    fi
else
    echo -e "  Status:      ${RED}not generated${NC} — run: python3 scripts/update-constellation-dashboard.py"
fi
echo ""
echo -e "  ${BOLD}🎯 NEXT${NC}"
echo -e "  Read:        project_roadmap_v5_honest.md"
echo -e "  Next sess:   M1 Nuke Week OR S23 follow-up"
echo -e "  Prompt:      project_m1_copy_paste_prompt.md"
echo ""
