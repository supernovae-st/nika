#!/usr/bin/env bash
# Master Migration Runner - Orchestrates all migration steps
# Usage: ./run-migration.sh [--dry-run] [--step N]

set -Eeuo pipefail
shopt -s inherit_errexit

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
readonly MIGRATION_LOG="$PROJECT_ROOT/migration/migration.log"

readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly BLUE='\033[0;34m'
readonly YELLOW='\033[1;33m'
readonly NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*" | tee -a "$MIGRATION_LOG"; }

# === Configuration ===
declare -a MIGRATION_STEPS=(
  "01-setup.sh"
  "02-move-core.sh"
  "03-move-runtime.sh"
  "04-move-provider.sh"
  "05-move-mcp.sh"
  "06-move-tui.sh"
  "07-build-cli.sh"
  "08-final-verification.sh"
)

readonly TOTAL_STEPS=${#MIGRATION_STEPS[@]}

# === Trap for cleanup ===
cleanup() {
  local exit_code=$?
  if [[ $exit_code -ne 0 ]]; then
    log_error "Migration failed with exit code $exit_code"
    log_info "To rollback: ./migration/rollback.sh"
    log_info "To retry from step N: ./migration/run-migration.sh --step N"
  fi
}
trap cleanup EXIT

# === Display banner ===
show_banner() {
  cat <<'EOF'
╔═══════════════════════════════════════════════════════════════════════════════╗
║                                                                               ║
║    🦋  N I K A   v 0 . 2 8 . 0                                                ║
║                                                                               ║
║    Workspace Restructure Migration                                           ║
║                                                                               ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║    This migration will transform Nika from a monolithic crate                ║
║    into a 6-crate workspace with proper separation of concerns.              ║
║                                                                               ║
║    Steps:                                                                     ║
║    1. Setup workspace structure                                              ║
║    2. Move nika-core (AST, DAG, error)                                       ║
║    3. Move nika-runtime (executor, binding, event)                           ║
║    4. Move nika-provider (rig, native)                                       ║
║    5. Move nika-mcp (MCP client)                                             ║
║    6. Move nika-tui (TUI views)                                              ║
║    7. Build nika-cli (binary)                                                ║
║    8. Final verification                                                      ║
║                                                                               ║
║    Each step includes:                                                        ║
║    • Checkpoint creation                                                      ║
║    • Import rewriting                                                         ║
║    • Build verification                                                       ║
║    • Test execution                                                           ║
║    • Git commit                                                               ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
EOF
}

# === Pre-flight checks ===
preflight_checks() {
  log_info "Running pre-flight checks..."

  # Check we're in the right directory
  if [[ ! -f "$PROJECT_ROOT/Cargo.toml" ]]; then
    log_error "Not in Nika project root (no Cargo.toml found)"
    exit 1
  fi

  # Check git status is clean
  if [[ -n "$(git status --porcelain)" ]]; then
    log_error "Git working directory is not clean"
    log_error "Commit or stash changes before running migration"
    exit 1
  fi

  # Check required tools
  local required_tools=("cargo" "git" "sed" "grep" "find")
  for tool in "${required_tools[@]}"; do
    if ! command -v "$tool" &>/dev/null; then
      log_error "Required tool not found: $tool"
      exit 1
    fi
  done

  # Check GNU sed (required for -i flag)
  if ! sed --version 2>&1 | grep -q "GNU sed"; then
    log_error "GNU sed required (found BSD sed)"
    log_error "On macOS: brew install gnu-sed"
    exit 1
  fi

  log_success "Pre-flight checks passed"
}

# === Run a single migration step ===
run_step() {
  local step_number="$1"
  local step_script="${MIGRATION_STEPS[$((step_number - 1))]}"
  local step_path="$SCRIPT_DIR/$step_script"

  log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  log_info "  Step $step_number/$TOTAL_STEPS: $step_script"
  log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  if [[ ! -f "$step_path" ]]; then
    log_error "Migration script not found: $step_path"
    return 1
  fi

  # Make script executable
  chmod +x "$step_path"

  # Run the step
  if ! bash "$step_path"; then
    log_error "Step $step_number failed: $step_script"
    return 1
  fi

  log_success "Step $step_number complete: $step_script"
}

# === Dry run mode ===
dry_run() {
  log_info "Dry run mode - showing migration steps:"

  for i in "${!MIGRATION_STEPS[@]}"; do
    local step_number=$((i + 1))
    local step_script="${MIGRATION_STEPS[$i]}"
    echo "  $step_number. $step_script"
  done

  log_info ""
  log_info "To run migration: ./migration/run-migration.sh"
  log_info "To run from step N: ./migration/run-migration.sh --step N"
}

# === Confirm with user ===
confirm_migration() {
  log_warn "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  log_warn "  WARNING: This will restructure the entire workspace"
  log_warn "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  log_warn ""
  log_warn "This migration will:"
  log_warn "  • Create 6 new crates"
  log_warn "  • Move all source files"
  log_warn "  • Rewrite all imports"
  log_warn "  • Make 8 git commits"
  log_warn ""
  log_warn "Estimated time: 5-10 minutes"
  log_warn ""

  read -rp "Continue? (yes/no): " confirm

  if [[ "$confirm" != "yes" ]]; then
    log_info "Migration cancelled"
    exit 0
  fi
}

# === Run all steps ===
run_all_steps() {
  local start_step="${1:-1}"

  log_info "Starting migration from step $start_step..."
  log_info "Started: $(date)"

  for i in $(seq "$start_step" "$TOTAL_STEPS"); do
    if ! run_step "$i"; then
      log_error "Migration stopped at step $i"
      log_info "To retry: ./migration/run-migration.sh --step $i"
      log_info "To rollback: ./migration/rollback.sh"
      exit 1
    fi

    # Pause between steps for review (optional)
    if [[ $i -lt $TOTAL_STEPS ]]; then
      log_info ""
      log_info "Press Enter to continue to next step (or Ctrl+C to stop)..."
      read -r
    fi
  done

  log_success "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  log_success "  ALL STEPS COMPLETE! 🦋"
  log_success "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  log_info ""
  log_info "Migration completed successfully!"
  log_info "Finished: $(date)"
  log_info ""
  log_info "Report: migration/MIGRATION_REPORT.md"
  log_info ""
  log_info "Next steps:"
  log_info "  1. Review migration report"
  log_info "  2. Update CHANGELOG.md"
  log_info "  3. Update documentation"
  log_info "  4. git tag v0.28.0"
  log_info "  5. git push origin main --tags"
  log_info ""
}

# === Usage ===
usage() {
  cat <<EOF
Usage: $0 [OPTIONS]

Run Nika v0.28 workspace restructure migration.

OPTIONS:
  --dry-run          Show migration steps without executing
  --step N           Start from step N (1-8)
  --no-confirm       Skip confirmation prompt
  -h, --help         Show this help message

EXAMPLES:
  $0                   # Run full migration
  $0 --dry-run         # Show steps only
  $0 --step 3          # Start from step 3
  $0 --no-confirm      # Run without confirmation

SAFETY:
  • All steps create checkpoints
  • Each step is atomic (builds + tests)
  • Rollback available: ./migration/rollback.sh
  • Git commits are granular (1 per step)

EOF
}

# === Main execution ===
main() {
  local dry_run_mode=false
  local start_step=1
  local no_confirm=false

  # Parse arguments
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dry-run)
        dry_run_mode=true
        shift
        ;;
      --step)
        start_step="$2"
        shift 2
        ;;
      --no-confirm)
        no_confirm=true
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        log_error "Unknown option: $1"
        usage
        exit 1
        ;;
    esac
  done

  # Validate step number
  if [[ $start_step -lt 1 ]] || [[ $start_step -gt $TOTAL_STEPS ]]; then
    log_error "Invalid step number: $start_step (must be 1-$TOTAL_STEPS)"
    exit 1
  fi

  # Show banner
  show_banner

  # Dry run mode
  if [[ "$dry_run_mode" == true ]]; then
    dry_run
    exit 0
  fi

  # Pre-flight checks
  preflight_checks

  # Confirm with user
  if [[ "$no_confirm" == false ]]; then
    confirm_migration
  fi

  # Run migration
  run_all_steps "$start_step"
}

main "$@"
