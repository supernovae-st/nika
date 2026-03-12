#!/usr/bin/env bash
# Rollback Script - Restore previous state from checkpoint
# Usage: ./rollback.sh [checkpoint-name]

set -Eeuo pipefail
shopt -s inherit_errexit

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
readonly MIGRATION_LOG="$PROJECT_ROOT/migration/migration.log"
readonly CHECKPOINT_DIR="$PROJECT_ROOT/migration/checkpoints"

readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly BLUE='\033[0;34m'
readonly YELLOW='\033[1;33m'
readonly NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*" | tee -a "$MIGRATION_LOG"; }

# === List available checkpoints ===
list_checkpoints() {
  log_info "Available checkpoints:"

  if [[ ! -d "$CHECKPOINT_DIR" ]]; then
    log_error "No checkpoints directory found"
    return 1
  fi

  local checkpoints
  checkpoints=$(find "$CHECKPOINT_DIR" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort)

  if [[ -z "$checkpoints" ]]; then
    log_error "No checkpoints found"
    return 1
  fi

  echo "$checkpoints" | nl
}

# === Find latest checkpoint ===
find_latest_checkpoint() {
  local latest
  latest=$(find "$CHECKPOINT_DIR" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort -r | head -n1)

  if [[ -z "$latest" ]]; then
    log_error "No checkpoints found"
    return 1
  fi

  echo "$latest"
}

# === Rollback to checkpoint ===
rollback_to_checkpoint() {
  local checkpoint_name="$1"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"

  log_warn "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  log_warn "  ROLLBACK TO: $checkpoint_name"
  log_warn "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  if [[ ! -d "$checkpoint_path" ]]; then
    log_error "Checkpoint does not exist: $checkpoint_name"
    return 1
  fi

  cd "$PROJECT_ROOT" || exit 1

  # Show current state
  log_info "Current state:"
  git status --short | head -n 10

  # Verify checkpoint has required files
  if [[ ! -f "$checkpoint_path/git-sha.txt" ]]; then
    log_error "Checkpoint missing git-sha.txt"
    return 1
  fi

  # Read checkpoint git SHA
  local checkpoint_sha
  checkpoint_sha=$(cat "$checkpoint_path/git-sha.txt")

  log_info "Checkpoint SHA: $checkpoint_sha"
  log_info "Current SHA: $(git rev-parse HEAD)"

  # Confirm rollback
  log_warn "This will reset to checkpoint: $checkpoint_name"
  log_warn "All uncommitted changes will be lost!"
  read -rp "Continue? (yes/no): " confirm

  if [[ "$confirm" != "yes" ]]; then
    log_info "Rollback cancelled"
    return 0
  fi

  # Stash any uncommitted changes
  log_info "Stashing uncommitted changes..."
  git stash push -u -m "Pre-rollback stash $(date)"

  # Reset to checkpoint SHA
  log_info "Resetting to checkpoint SHA..."
  if ! git reset --hard "$checkpoint_sha"; then
    log_error "Failed to reset to checkpoint SHA"
    return 1
  fi

  # Apply any diff that was saved
  if [[ -f "$checkpoint_path/git-diff.patch" ]] && [[ -s "$checkpoint_path/git-diff.patch" ]]; then
    log_info "Applying checkpoint diff..."
    if ! git apply "$checkpoint_path/git-diff.patch"; then
      log_warn "Failed to apply diff patch (may be empty or already applied)"
    fi
  fi

  # Restore Cargo.toml if backed up
  if [[ -f "$checkpoint_path/Cargo.toml.bak" ]]; then
    log_info "Restoring Cargo.toml..."
    cp "$checkpoint_path/Cargo.toml.bak" "$PROJECT_ROOT/Cargo.toml"
  fi

  # Restore Cargo.lock if backed up
  if [[ -f "$checkpoint_path/Cargo.lock.bak" ]]; then
    log_info "Restoring Cargo.lock..."
    cp "$checkpoint_path/Cargo.lock.bak" "$PROJECT_ROOT/Cargo.lock"
  fi

  log_success "Rollback complete!"
  log_info "Current state:"
  git status --short
  log_info "To undo rollback: git reflog"
}

# === Rollback by steps ===
rollback_n_steps() {
  local steps="$1"

  log_info "Rolling back $steps steps..."

  # Find checkpoints ordered by number
  local checkpoints
  checkpoints=$(find "$CHECKPOINT_DIR" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort -r)

  # Get the Nth checkpoint
  local target_checkpoint
  target_checkpoint=$(echo "$checkpoints" | sed -n "${steps}p")

  if [[ -z "$target_checkpoint" ]]; then
    log_error "Cannot rollback $steps steps (not enough checkpoints)"
    return 1
  fi

  rollback_to_checkpoint "$target_checkpoint"
}

# === Show checkpoint info ===
show_checkpoint_info() {
  local checkpoint_name="$1"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"

  if [[ ! -d "$checkpoint_path" ]]; then
    log_error "Checkpoint does not exist: $checkpoint_name"
    return 1
  fi

  log_info "Checkpoint: $checkpoint_name"
  log_info "Path: $checkpoint_path"

  if [[ -f "$checkpoint_path/git-sha.txt" ]]; then
    log_info "Git SHA: $(cat "$checkpoint_path/git-sha.txt")"
  fi

  if [[ -f "$checkpoint_path/test-count.txt" ]]; then
    log_info "Tests: $(cat "$checkpoint_path/test-count.txt")"
  fi

  if [[ -f "$checkpoint_path/file-list.txt" ]]; then
    local file_count
    file_count=$(wc -l < "$checkpoint_path/file-list.txt")
    log_info "Files: $file_count"
  fi

  if [[ -f "$checkpoint_path/git-status.txt" ]]; then
    log_info "Git status:"
    cat "$checkpoint_path/git-status.txt" | head -n 10
  fi
}

# === Usage ===
usage() {
  cat <<EOF
Usage: $0 [OPTIONS]

Rollback Nika migration to a previous checkpoint.

OPTIONS:
  -l, --list              List all available checkpoints
  -n, --steps N           Rollback N steps
  -c, --checkpoint NAME   Rollback to specific checkpoint
  -i, --info NAME         Show checkpoint information
  -h, --help              Show this help message

EXAMPLES:
  $0 --list                     # List checkpoints
  $0 --steps 1                  # Rollback 1 step
  $0 --checkpoint 02-core-moved # Rollback to specific checkpoint
  $0                            # Rollback to latest checkpoint

EOF
}

# === Main execution ===
main() {
  local checkpoint_name=""
  local steps=""
  local show_info=""

  # Parse arguments
  while [[ $# -gt 0 ]]; do
    case "$1" in
      -l|--list)
        list_checkpoints
        exit 0
        ;;
      -n|--steps)
        steps="$2"
        shift 2
        ;;
      -c|--checkpoint)
        checkpoint_name="$2"
        shift 2
        ;;
      -i|--info)
        show_info="$2"
        shift 2
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

  # Show checkpoint info
  if [[ -n "$show_info" ]]; then
    show_checkpoint_info "$show_info"
    exit 0
  fi

  # Rollback by steps
  if [[ -n "$steps" ]]; then
    rollback_n_steps "$steps"
    exit 0
  fi

  # Rollback to specific checkpoint
  if [[ -n "$checkpoint_name" ]]; then
    rollback_to_checkpoint "$checkpoint_name"
    exit 0
  fi

  # Default: rollback to latest checkpoint
  log_info "No checkpoint specified, rolling back to latest..."
  local latest
  latest=$(find_latest_checkpoint)

  if [[ -z "$latest" ]]; then
    log_error "No checkpoints found"
    exit 1
  fi

  rollback_to_checkpoint "$latest"
}

main "$@"
