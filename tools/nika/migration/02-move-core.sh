#!/usr/bin/env bash
# Migration Step 2: Move nika-core modules
# Moves src/ast/, src/dag/, src/core/, src/error.rs to nika-core

set -Eeuo pipefail
shopt -s inherit_errexit

# === Configuration ===
readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
readonly MIGRATION_LOG="$PROJECT_ROOT/migration/migration.log"
readonly CHECKPOINT_DIR="$PROJECT_ROOT/migration/checkpoints"

# === Colors ===
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m'

# === Logging ===
log_info() {
  echo -e "${BLUE}[INFO]${NC} $*" | tee -a "$MIGRATION_LOG"
}

log_success() {
  echo -e "${GREEN}[SUCCESS]${NC} $*" | tee -a "$MIGRATION_LOG"
}

log_error() {
  echo -e "${RED}[ERROR]${NC} $*" | tee -a "$MIGRATION_LOG"
}

# === Move module with structure preservation ===
move_module() {
  local source_path="$1"
  local dest_path="$2"

  log_info "Moving: $source_path → $dest_path"

  if [[ ! -e "$source_path" ]]; then
    log_error "Source path does not exist: $source_path"
    return 1
  fi

  # Create destination directory
  mkdir -p "$(dirname "$dest_path")"

  # Move preserving structure
  if [[ -d "$source_path" ]]; then
    cp -r "$source_path" "$dest_path"
    log_info "Copied directory: $source_path"
  else
    cp "$source_path" "$dest_path"
    log_info "Copied file: $source_path"
  fi

  log_success "Moved: $source_path"
}

# === Rewrite imports in a file ===
rewrite_imports_in_file() {
  local file_path="$1"

  log_info "Rewriting imports in: $file_path"

  # Backup original
  cp "$file_path" "${file_path}.bak"

  # Rewrite patterns:
  # use crate::ast → use nika_core::ast
  # use crate::dag → use nika_core::dag
  # use crate::error → use nika_core::error
  # use crate::core → use nika_core::core (this stays as core in nika-core)

  sed -i \
    -e 's|use crate::ast|use nika_core::ast|g' \
    -e 's|use crate::dag|use nika_core::dag|g' \
    -e 's|use crate::error|use nika_core::error|g' \
    -e 's|use super::ast|use crate::ast|g' \
    -e 's|use super::dag|use crate::dag|g' \
    -e 's|use super::error|use crate::error|g' \
    "$file_path"

  log_success "Imports rewritten in: $file_path"
}

# === Rewrite imports in all Rust files in a directory ===
rewrite_imports_in_dir() {
  local dir_path="$1"

  log_info "Rewriting imports in directory: $dir_path"

  find "$dir_path" -type f -name "*.rs" | while IFS= read -r file; do
    rewrite_imports_in_file "$file"
  done

  log_success "All imports rewritten in: $dir_path"
}

# === Build nika-core lib.rs ===
build_core_lib() {
  local lib_path="$PROJECT_ROOT/crates/nika-core/src/lib.rs"

  log_info "Building nika-core lib.rs..."

  cat > "$lib_path" <<'EOF'
//! Nika Core - AST, DAG, and foundational types
//!
//! This crate contains the core data structures and parsing logic
//! for Nika workflows.

#![warn(clippy::all)]
#![warn(missing_docs)]

pub mod ast;
pub mod dag;
pub mod error;

// Re-export commonly used types
pub use error::{NikaError, Result};
EOF

  log_success "nika-core lib.rs created"
}

# === Move core modules ===
move_core_modules() {
  log_info "Moving core modules..."

  cd "$PROJECT_ROOT" || exit 1

  local source_dir="src"
  local dest_dir="crates/nika-core/src"

  # Move modules
  move_module "$source_dir/ast" "$dest_dir/ast"
  move_module "$source_dir/dag" "$dest_dir/dag"
  move_module "$source_dir/core" "$dest_dir/core"
  move_module "$source_dir/error.rs" "$dest_dir/error.rs"

  # Rewrite imports in moved files
  rewrite_imports_in_dir "$dest_dir"

  # Build lib.rs
  build_core_lib

  log_success "Core modules moved"
}

# === Verify nika-core builds ===
verify_core_builds() {
  log_info "Verifying nika-core builds..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo build -p nika-core 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-core build failed"
    return 1
  fi

  log_success "nika-core builds successfully"
}

# === Run nika-core tests ===
run_core_tests() {
  log_info "Running nika-core tests..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo test -p nika-core --lib 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-core tests failed"
    return 1
  fi

  log_success "nika-core tests pass"
}

# === Create checkpoint ===
create_checkpoint() {
  local checkpoint_name="$1"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"

  log_info "Creating checkpoint: $checkpoint_name"

  mkdir -p "$checkpoint_path"

  # Save git state
  git status --porcelain > "$checkpoint_path/git-status.txt"

  # Save file list
  find crates/nika-core/src -type f -name "*.rs" | sort > "$checkpoint_path/file-list.txt"

  # Save test count
  cargo test -p nika-core --lib -- --list 2>&1 | grep "test result:" > "$checkpoint_path/test-count.txt" || true

  log_success "Checkpoint created: $checkpoint_path"
}

# === Main execution ===
main() {
  log_info "Starting Step 2: Move nika-core modules..."

  move_core_modules
  verify_core_builds
  run_core_tests

  # Create checkpoint
  create_checkpoint "02-core-moved"

  # Commit changes
  log_info "Committing changes..."
  git add -A
  git commit -m "refactor(v0.28): move core modules to nika-core crate

- Move src/ast/ → crates/nika-core/src/ast/
- Move src/dag/ → crates/nika-core/src/dag/
- Move src/core/ → crates/nika-core/src/core/
- Move src/error.rs → crates/nika-core/src/error.rs
- Rewrite imports: crate::ast → nika_core::ast
- All nika-core tests passing

Part of Nika v0.28 workspace restructure.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"

  log_success "Step 2 complete!"
  log_info "Next step: Run ./migration/03-move-runtime.sh"
}

main "$@"
