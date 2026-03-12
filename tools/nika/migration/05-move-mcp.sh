#!/usr/bin/env bash
# Migration Step 5: Move nika-mcp modules
# Moves src/mcp/ to nika-mcp

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
    -e 's|use crate::mcp|use nika_mcp|g' \
    -e 's|use crate::ast|use nika_core::ast|g' \
    -e 's|use crate::error|use nika_core::error|g' \
    -e 's|use crate::runtime|use nika_runtime::runtime|g' \
    -e 's|use super::mcp|use crate|g' \
    "$file"
}

rewrite_imports_in_dir() {
  local dir="$1"
  find "$dir" -type f -name "*.rs" | while IFS= read -r file; do
    rewrite_imports_in_file "$file"
  done
}

build_mcp_lib() {
  local lib_path="$PROJECT_ROOT/crates/nika-mcp/src/lib.rs"

  cat > "$lib_path" <<'EOF'
//! Nika MCP - Model Context Protocol client
//!
//! This crate contains the MCP client implementation for
//! connecting to MCP servers.

#![warn(clippy::all)]
#![warn(missing_docs)]

pub mod client;
pub mod types;

// Re-export commonly used types
pub use client::McpClient;
EOF

  log_success "nika-mcp lib.rs created"
}

move_mcp_modules() {
  log_info "Moving MCP modules..."

  cd "$PROJECT_ROOT" || exit 1

  local source_dir="src/mcp"
  local dest_dir="crates/nika-mcp/src"

  # Move entire mcp directory
  move_module "$source_dir" "$dest_dir/temp"
  mv "$dest_dir/temp"/* "$dest_dir/"
  rmdir "$dest_dir/temp"

  rewrite_imports_in_dir "$dest_dir"
  build_mcp_lib

  log_success "MCP modules moved"
}

update_mcp_deps() {
  log_info "Updating nika-mcp dependencies..."

  local cargo_toml="$PROJECT_ROOT/crates/nika-mcp/Cargo.toml"

  cat >> "$cargo_toml" <<'EOF'

# MCP dependencies
rmcp = "0.16"
async-trait = "0.1"
EOF

  log_success "nika-mcp dependencies updated"
}

verify_mcp_builds() {
  log_info "Verifying nika-mcp builds..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo build -p nika-mcp 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-mcp build failed"
    return 1
  fi

  log_success "nika-mcp builds successfully"
}

run_mcp_tests() {
  log_info "Running nika-mcp tests..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo test -p nika-mcp --lib 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-mcp tests failed"
    return 1
  fi

  log_success "nika-mcp tests pass"
}

create_checkpoint() {
  local checkpoint_name="$1"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"

  mkdir -p "$checkpoint_path"
  git status --porcelain > "$checkpoint_path/git-status.txt"
  find crates/nika-mcp/src -type f -name "*.rs" | sort > "$checkpoint_path/file-list.txt"
  cargo test -p nika-mcp --lib -- --list 2>&1 | grep "test result:" > "$checkpoint_path/test-count.txt" || true

  log_success "Checkpoint created: $checkpoint_path"
}

main() {
  log_info "Starting Step 5: Move nika-mcp modules..."

  move_mcp_modules
  update_mcp_deps
  verify_mcp_builds
  run_mcp_tests

  create_checkpoint "05-mcp-moved"

  log_info "Committing changes..."
  git add -A
  git commit -m "refactor(v0.28): move MCP modules to nika-mcp crate

- Move src/mcp/ → crates/nika-mcp/src/
- Rewrite imports to use nika_core
- All nika-mcp tests passing

Part of Nika v0.28 workspace restructure.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"

  log_success "Step 5 complete!"
  log_info "Next step: Run ./migration/06-move-tui.sh"
}

main "$@"
