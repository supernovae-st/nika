#!/bin/bash
# Verify nuke candidates before deletion — pre-flight check for M1
# Usage: ./scripts/nuke-verify.sh

set -e

cd "$(dirname "$0")/.."

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BOLD='\033[1m'
NC='\033[0m'

check() {
    local name="$1"
    local cmd="$2"
    local expected="$3"

    local actual
    actual=$(eval "$cmd" 2>/dev/null | tr -d ' \n')

    if [ "$actual" = "$expected" ]; then
        echo -e "  ${GREEN}✅${NC} ${name}: ${actual} (expected ${expected})"
        return 0
    else
        echo -e "  ${RED}❌${NC} ${name}: ${actual} (expected ${expected}) — DO NOT DELETE"
        return 1
    fi
}

echo -e "${BOLD}🔪 NUKE PRE-FLIGHT CHECK${NC}"
echo ""
echo "Verifying all Tier 1 deletion candidates are SAFE to remove..."
echo ""

echo -e "${BOLD}NUKE-1: nika-runtime crate${NC}"
check "Production callers" "grep -rn 'use nika_runtime::\|nika_runtime::' tools --include='*.rs' | grep -v 'tools/nika-runtime/' | wc -l" "0"
check "Cargo deps" "grep -l '^nika-runtime' tools/*/Cargo.toml 2>/dev/null | grep -v 'tools/nika-runtime/' | wc -l" "0"
echo ""

echo -e "${BOLD}NUKE-2: verb-fetch retry infra${NC}"
check "External callers of run_with_retry" "grep -rn 'run_with_retry' tools --include='*.rs' | grep -v 'verb-fetch/' | wc -l" "0"
check "External callers of RetryPolicy" "grep -rn 'RetryPolicy' tools --include='*.rs' | grep -v 'verb-fetch/' | wc -l" "0"
echo ""

echo -e "${BOLD}NUKE-3: SetupResult::message${NC}"
check "Reads of .message field" "grep -rn 'result\\.message\|\\.message\\b' tools/nika-cli/src/machine/ --include='*.rs' | grep -v 'message:' | wc -l" "0"
echo ""

echo -e "${BOLD}NUKE-4: DetectionSource::Extension${NC}"
check "Usages of ::Extension variant" "grep -rn 'DetectionSource::Extension' tools --include='*.rs' | grep -v 'detect.rs' | wc -l" "0"
echo ""

echo -e "${BOLD}NUKE-5: LSP ExecutionEventNotification${NC}"
check "External callers" "grep -rn 'ExecutionEventNotification\|ExecutionEventParams' tools --include='*.rs' | grep -v 'nika-lsp/src/backend.rs' | wc -l" "0"
echo ""

echo -e "${YELLOW}If any ❌ appeared above, DO NOT DELETE the affected target.${NC}"
echo -e "${GREEN}All ✅ means safe to proceed with M1 Nuke Week.${NC}"
