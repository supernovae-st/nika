#!/usr/bin/env bash
# Migration Step 4: Move nika-provider modules
# Moves src/provider/ to nika-provider

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
    -e 's|use crate::provider|use nika_provider|g' \
    -e 's|use crate::ast|use nika_core::ast|g' \
    -e 's|use crate::error|use nika_core::error|g' \
    -e 's|use crate::runtime|use nika_runtime::runtime|g' \
    -e 's|use super::provider|use crate|g' \
    "$file"
}

rewrite_imports_in_dir() {
  local dir="$1"
  find "$dir" -type f -name "*.rs" | while IFS= read -r file; do
    rewrite_imports_in_file "$file"
  done
}

build_provider_lib() {
  local lib_path="$PROJECT_ROOT/crates/nika-provider/src/lib.rs"

  cat > "$lib_path" <<'EOF'
//! Nika Provider - LLM provider integrations
//!
//! This crate contains the LLM provider abstractions and
//! implementations for Nika workflows.

#![warn(clippy::all)]
#![warn(missing_docs)]

pub mod rig;
pub mod native;

// Re-export commonly used types
pub use rig::RigProvider;
pub use native::NativeRuntime;
EOF

  log_success "nika-provider lib.rs created"
}

move_provider_modules() {
  log_info "Moving provider modules..."

  cd "$PROJECT_ROOT" || exit 1

  local source_dir="src/provider"
  local dest_dir="crates/nika-provider/src"

  # Move entire provider directory
  move_module "$source_dir" "$dest_dir/temp"
  # Flatten: move contents up one level
  mv "$dest_dir/temp"/* "$dest_dir/"
  rmdir "$dest_dir/temp"

  rewrite_imports_in_dir "$dest_dir"
  build_provider_lib

  log_success "Provider modules moved"
}

update_provider_deps() {
  log_info "Updating nika-provider dependencies..."

  local cargo_toml="$PROJECT_ROOT/crates/nika-provider/Cargo.toml"

  cat >> "$cargo_toml" <<'EOF'

# LLM provider dependencies
rig-core = "0.32"
mistralrs = "0.4"
async-trait = "0.1"
EOF

  log_success "nika-provider dependencies updated"
}

verify_provider_builds() {
  log_info "Verifying nika-provider builds..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo build -p nika-provider 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-provider build failed"
    return 1
  fi

  log_success "nika-provider builds successfully"
}

run_provider_tests() {
  log_info "Running nika-provider tests..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo test -p nika-provider --lib 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-provider tests failed"
    return 1
  fi

  log_success "nika-provider tests pass"
}

create_checkpoint() {
  local checkpoint_name="$1"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"

  mkdir -p "$checkpoint_path"
  git status --porcelain > "$checkpoint_path/git-status.txt"
  find crates/nika-provider/src -type f -name "*.rs" | sort > "$checkpoint_path/file-list.txt"
  cargo test -p nika-provider --lib -- --list 2>&1 | grep "test result:" > "$checkpoint_path/test-count.txt" || true

  log_success "Checkpoint created: $checkpoint_path"
}

main() {
  log_info "Starting Step 4: Move nika-provider modules..."

  move_provider_modules
  update_provider_deps
  verify_provider_builds
  run_provider_tests

  create_checkpoint "04-provider-moved"

  log_info "Committing changes..."
  git add -A
  git commit -m "refactor(v0.28): move provider modules to nika-provider crate

- Move src/provider/ → crates/nika-provider/src/
- Rewrite imports to use nika_core
- All nika-provider tests passing

Part of Nika v0.28 workspace restructure.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"

  log_success "Step 4 complete!"
  log_info "Next step: Run ./migration/05-move-mcp.sh"
}

main "$@"
