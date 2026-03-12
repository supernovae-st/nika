#!/usr/bin/env bash
# Migration Step 3: Move nika-runtime modules
# Moves src/runtime/, src/binding/, src/event/ to nika-runtime

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
readonly BLUE='\033[0;34m'
readonly NC='\033[0m'

# === Logging ===
log_info() { echo -e "${BLUE}[INFO]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $*" | tee -a "$MIGRATION_LOG"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" | tee -a "$MIGRATION_LOG"; }

# === Move and rewrite ===
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
    -e 's|use crate::runtime|use nika_runtime::runtime|g' \
    -e 's|use crate::binding|use nika_runtime::binding|g' \
    -e 's|use crate::event|use nika_runtime::event|g' \
    -e 's|use crate::ast|use nika_core::ast|g' \
    -e 's|use crate::dag|use nika_core::dag|g' \
    -e 's|use crate::error|use nika_core::error|g' \
    -e 's|use super::runtime|use crate::runtime|g' \
    -e 's|use super::binding|use crate::binding|g' \
    -e 's|use super::event|use crate::event|g' \
    "$file"
}

rewrite_imports_in_dir() {
  local dir="$1"
  find "$dir" -type f -name "*.rs" | while IFS= read -r file; do
    rewrite_imports_in_file "$file"
  done
}

# === Build nika-runtime lib.rs ===
build_runtime_lib() {
  local lib_path="$PROJECT_ROOT/crates/nika-runtime/src/lib.rs"

  cat > "$lib_path" <<'EOF'
//! Nika Runtime - Workflow execution engine
//!
//! This crate contains the runtime executor, event system,
//! and data binding logic for Nika workflows.

#![warn(clippy::all)]
#![warn(missing_docs)]

pub mod runtime;
pub mod binding;
pub mod event;

// Re-export commonly used types
pub use runtime::executor::Executor;
pub use event::log::EventLog;
pub use binding::resolve::Resolver;
EOF

  log_success "nika-runtime lib.rs created"
}

# === Move runtime modules ===
move_runtime_modules() {
  log_info "Moving runtime modules..."

  cd "$PROJECT_ROOT" || exit 1

  local source_dir="src"
  local dest_dir="crates/nika-runtime/src"

  move_module "$source_dir/runtime" "$dest_dir/runtime"
  move_module "$source_dir/binding" "$dest_dir/binding"
  move_module "$source_dir/event" "$dest_dir/event"

  rewrite_imports_in_dir "$dest_dir"
  build_runtime_lib

  log_success "Runtime modules moved"
}

# === Update nika-runtime dependencies ===
update_runtime_deps() {
  log_info "Updating nika-runtime dependencies..."

  local cargo_toml="$PROJECT_ROOT/crates/nika-runtime/Cargo.toml"

  cat >> "$cargo_toml" <<'EOF'

# Additional dependencies for runtime
futures = "0.3"
async-trait = "0.1"
uuid = { version = "1.11", features = ["v4", "serde"] }
EOF

  log_success "nika-runtime dependencies updated"
}

# === Verify nika-runtime builds ===
verify_runtime_builds() {
  log_info "Verifying nika-runtime builds..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo build -p nika-runtime 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-runtime build failed"
    return 1
  fi

  log_success "nika-runtime builds successfully"
}

# === Run nika-runtime tests ===
run_runtime_tests() {
  log_info "Running nika-runtime tests..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo test -p nika-runtime --lib 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-runtime tests failed"
    return 1
  fi

  log_success "nika-runtime tests pass"
}

# === Create checkpoint ===
create_checkpoint() {
  local checkpoint_name="$1"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"

  mkdir -p "$checkpoint_path"
  git status --porcelain > "$checkpoint_path/git-status.txt"
  find crates/nika-runtime/src -type f -name "*.rs" | sort > "$checkpoint_path/file-list.txt"
  cargo test -p nika-runtime --lib -- --list 2>&1 | grep "test result:" > "$checkpoint_path/test-count.txt" || true

  log_success "Checkpoint created: $checkpoint_path"
}

# === Main execution ===
main() {
  log_info "Starting Step 3: Move nika-runtime modules..."

  move_runtime_modules
  update_runtime_deps
  verify_runtime_builds
  run_runtime_tests

  create_checkpoint "03-runtime-moved"

  log_info "Committing changes..."
  git add -A
  git commit -m "refactor(v0.28): move runtime modules to nika-runtime crate

- Move src/runtime/ → crates/nika-runtime/src/runtime/
- Move src/binding/ → crates/nika-runtime/src/binding/
- Move src/event/ → crates/nika-runtime/src/event/
- Rewrite imports to use nika_core and nika_runtime
- All nika-runtime tests passing

Part of Nika v0.28 workspace restructure.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"

  log_success "Step 3 complete!"
  log_info "Next step: Run ./migration/04-move-provider.sh"
}

main "$@"
