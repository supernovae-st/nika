#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Nika Quality Overhaul — Autonomous Orchestrator
# ═══════════════════════════════════════════════════════════════════
#
# Runs 6 sessions (A-F) sequentially via Claude Code headless mode.
# Each session reads its plan, executes fixes, runs tests, commits.
# Logs everything to logs/ directory.
#
# Usage:
#   tmux new-session -s nika-quality
#   ./scripts/orchestrator.sh
#
# Monitor from phone:
#   git log --oneline -20  (check commits)
#   tail -f logs/session-*.log  (check progress)
#
# ═══════════════════════════════════════════════════════════════════

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TOOLS_DIR="$REPO_ROOT/tools"
PLANS_DIR="$REPO_ROOT/docs/plans/sessions"
LOGS_DIR="$REPO_ROOT/logs"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)

mkdir -p "$LOGS_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log() { echo -e "${CYAN}[$(date +%H:%M:%S)]${NC} $1"; }
success() { echo -e "${GREEN}[$(date +%H:%M:%S)] ✓${NC} $1"; }
error() { echo -e "${RED}[$(date +%H:%M:%S)] ✗${NC} $1"; }
warn() { echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠${NC} $1"; }

# ── Pre-flight checks ──────────────────────────────────────────────

log "Pre-flight checks..."

cd "$TOOLS_DIR"

# Verify tests pass before starting
log "Running baseline tests..."
if ! cargo test --workspace --lib 2>&1 | tail -1 | grep -q "0 failed"; then
    error "Baseline tests FAILED. Fix before running orchestrator."
    exit 1
fi
success "Baseline tests pass"

# Verify clippy
if ! cargo clippy --workspace -- -D warnings 2>&1 | tail -1 | grep -q "Finished"; then
    error "Clippy has warnings. Fix before running orchestrator."
    exit 1
fi
success "Clippy clean"

# Count initial tests
INITIAL_TESTS=$(cargo test --workspace --lib 2>&1 | grep "^test result" | awk '{sum += $4} END {print sum}')
log "Initial test count: $INITIAL_TESTS"

cd "$REPO_ROOT"

# ── Session runner ──────────────────────────────────────────────────

run_session() {
    local session_name="$1"
    local plan_file="$2"
    local log_file="$LOGS_DIR/session-${session_name}-${TIMESTAMP}.log"

    log "═══════════════════════════════════════════════════"
    log "Starting Session $session_name"
    log "Plan: $plan_file"
    log "Log: $log_file"
    log "═══════════════════════════════════════════════════"

    if [ ! -f "$plan_file" ]; then
        error "Plan file not found: $plan_file"
        return 1
    fi

    local plan_content
    plan_content=$(cat "$plan_file")

    local prompt="You are executing a quality improvement session on the Nika workflow engine.
Workspace: $HOME/.nika (12 Rust crates)

## CRITICAL RULES
- ALWAYS use \`cargo test --workspace --lib\` (--lib to avoid keychain popups)
- 1 fix = 1 commit with co-authors
- Run clippy after EVERY fix: \`cargo clippy --workspace -- -D warnings\`
- Push after every 3-4 commits: \`git push\`
- If a fix breaks tests: REVERT immediately, move to next bug
- Write failing test FIRST (TDD), then implement fix
- NEVER mark a bug as done without code + passing test

## SESSION PLAN
$plan_content

## AFTER ALL FIXES
1. Run: cd $HOME/.nika && cargo test --workspace --lib
2. Run: cargo clippy --workspace -- -D warnings
3. Push: cd $HOME/.nika && git push
4. Report: count of bugs fixed, tests added, total test count"

    # Run Claude in headless mode
    cd "$REPO_ROOT"
    claude -p "$prompt" \
        --dangerously-skip-permissions \
        --model opus \
        --max-turns 200 \
        --max-budget-usd 15.00 \
        --verbose \
        2>&1 | tee "$log_file"

    local exit_code=$?

    if [ $exit_code -eq 0 ]; then
        success "Session $session_name completed successfully"
    else
        warn "Session $session_name exited with code $exit_code"
    fi

    # Post-session verification
    cd "$TOOLS_DIR"
    log "Post-session verification for $session_name..."

    if cargo test --workspace --lib 2>&1 | tail -1 | grep -q "0 failed"; then
        success "Tests pass after Session $session_name"
    else
        error "Tests FAILED after Session $session_name!"
        warn "Continuing to next session anyway..."
    fi

    # Push any remaining commits
    cd "$REPO_ROOT"
    git push 2>/dev/null || true

    # Count tests
    cd "$TOOLS_DIR"
    local current_tests
    current_tests=$(cargo test --workspace --lib 2>&1 | grep "^test result" | awk '{sum += $4} END {print sum}')
    log "Test count after Session $session_name: $current_tests (was $INITIAL_TESTS)"

    return 0
}

# ── Main execution ──────────────────────────────────────────────────

log "═══════════════════════════════════════════════════════════════"
log "  Nika Quality Overhaul — Starting 6 Sessions"
log "  Estimated time: 20-25 hours"
log "  Started: $(date)"
log "═══════════════════════════════════════════════════════════════"

SESSIONS=(
    "A:$PLANS_DIR/session-A-security.md"
    "B:$PLANS_DIR/session-B-agent-refactor.md"
    "C:$PLANS_DIR/session-C-silent-failures.md"
    "D:$PLANS_DIR/session-D-quality-infra.md"
    "E:$PLANS_DIR/session-E-test-hardening.md"
    "F:$PLANS_DIR/session-F-stringly-typed.md"
)

COMPLETED=0
FAILED=0

for session_entry in "${SESSIONS[@]}"; do
    IFS=':' read -r name plan <<< "$session_entry"

    if run_session "$name" "$plan"; then
        ((COMPLETED++))
    else
        ((FAILED++))
        warn "Session $name had issues, continuing..."
    fi

    # Brief pause between sessions
    sleep 5
done

# ── Final summary ───────────────────────────────────────────────────

cd "$TOOLS_DIR"
FINAL_TESTS=$(cargo test --workspace --lib 2>&1 | grep "^test result" | awk '{sum += $4} END {print sum}')

log ""
log "═══════════════════════════════════════════════════════════════"
log "  ORCHESTRATOR COMPLETE"
log "  Sessions: $COMPLETED completed, $FAILED failed"
log "  Tests: $INITIAL_TESTS → $FINAL_TESTS (+$((FINAL_TESTS - INITIAL_TESTS)))"
log "  Commits: $(git log --oneline --since='12 hours ago' | wc -l | tr -d ' ')"
log "  Finished: $(date)"
log "═══════════════════════════════════════════════════════════════"

# Final push
cd "$REPO_ROOT"
git push 2>/dev/null || true

success "All done. Check git log for details."
