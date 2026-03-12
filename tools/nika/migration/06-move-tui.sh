#!/usr/bin/env bash
# Migration Step 6: Move nika-tui modules
# Moves src/tui/ to nika-tui

set -Eeuo pipefail
shopt -s inherit_errexit

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
readonly MIGRATION_LOG="$PROJECT_ROOT/migration/migration.log"
readonly CHECKPOINT_DIR="$PROJECT_ROOT/migration/checkpoints"

readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" | tee -a "$MIGRATION_LOG"; }

move_module() {
  local source="$1"
  local dest="$2"
  log_info "Moving: $source → $dest"
  mkdir -p "$(dirname "$dest")"
  cp -r "$source" "$dest"
  log_success "Moved: $source"
}

rewrite_imports_in_file() {
  local file="$1"
  cp "$file" "${file}.bak"

  sed -i \
    -e 's|use crate::tui|use nika_tui|g' \
    -e 's|use crate::ast|use nika_core::ast|g' \
    -e 's|use crate::error|use nika_core::error|g' \
    -e 's|use crate::runtime|use nika_runtime::runtime|g' \
    -e 's|use crate::event|use nika_runtime::event|g' \
    -e 's|use super::tui|use crate|g' \
    "$file"
}

rewrite_imports_in_dir() {
  local dir="$1"
  find "$dir" -type f -name "*.rs" | while IFS= read -r file; do
    rewrite_imports_in_file "$file"
  done
}

build_tui_lib() {
  local lib_path="$PROJECT_ROOT/crates/nika-tui/src/lib.rs"

  cat > "$lib_path" <<'EOF'
//! Nika TUI - Terminal User Interface
//!
//! This crate contains the TUI implementation for Nika,
//! including the Studio, Runner, Chat, and Settings views.

#![warn(clippy::all)]
#![warn(missing_docs)]

pub mod app;
pub mod views;
pub mod widgets;
pub mod theme;

// Re-export main app type
pub use app::App;
EOF

  log_success "nika-tui lib.rs created"
}

move_tui_modules() {
  log_info "Moving TUI modules..."

  cd "$PROJECT_ROOT" || exit 1

  local source_dir="src/tui"
  local dest_dir="crates/nika-tui/src"

  # Move entire tui directory
  move_module "$source_dir" "$dest_dir/temp"
  mv "$dest_dir/temp"/* "$dest_dir/"
  rmdir "$dest_dir/temp"

  rewrite_imports_in_dir "$dest_dir"
  build_tui_lib

  log_success "TUI modules moved"
}

update_tui_deps() {
  log_info "Updating nika-tui dependencies..."

  local cargo_toml="$PROJECT_ROOT/crates/nika-tui/Cargo.toml"

  cat >> "$cargo_toml" <<'EOF'

# TUI dependencies
ratatui = "0.29"
crossterm = "0.28"
tui-tree-widget = "0.24"
tui-textarea = "0.7"

# Additional utilities
chrono = "0.4"
EOF

  log_success "nika-tui dependencies updated"
}

verify_tui_builds() {
  log_info "Verifying nika-tui builds..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo build -p nika-tui 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-tui build failed"
    return 1
  fi

  log_success "nika-tui builds successfully"
}

run_tui_tests() {
  log_info "Running nika-tui tests..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo test -p nika-tui --lib 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-tui tests failed"
    return 1
  fi

  log_success "nika-tui tests pass"
}

create_checkpoint() {
  local checkpoint_name="$1"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"

  mkdir -p "$checkpoint_path"
  git status --porcelain > "$checkpoint_path/git-status.txt"
  find crates/nika-tui/src -type f -name "*.rs" | sort > "$checkpoint_path/file-list.txt"
  cargo test -p nika-tui --lib -- --list 2>&1 | grep "test result:" > "$checkpoint_path/test-count.txt" || true

  log_success "Checkpoint created: $checkpoint_path"
}

main() {
  log_info "Starting Step 6: Move nika-tui modules..."

  move_tui_modules
  update_tui_deps
  verify_tui_builds
  run_tui_tests

  create_checkpoint "06-tui-moved"

  log_info "Committing changes..."
  git add -A
  git commit -m "refactor(v0.28): move TUI modules to nika-tui crate

- Move src/tui/ → crates/nika-tui/src/
- Rewrite imports to use nika_core and nika_runtime
- All nika-tui tests passing

Part of Nika v0.28 workspace restructure.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"

  log_success "Step 6 complete!"
  log_info "Next step: Run ./migration/07-build-cli.sh"
}

main "$@"
