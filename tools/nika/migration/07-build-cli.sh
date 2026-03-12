#!/usr/bin/env bash
# Migration Step 7: Build nika-cli
# Moves remaining CLI code to nika-cli

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

move_file() {
  local source="$1"
  local dest="$2"
  log_info "Moving: $source → $dest"
  mkdir -p "$(dirname "$dest")"
  cp "$source" "$dest"
  log_success "Moved: $source"
}

rewrite_imports_in_file() {
  local file="$1"
  cp "$file" "${file}.bak"

  sed -i \
    -e 's|use crate::ast|use nika_core::ast|g' \
    -e 's|use crate::error|use nika_core::error|g' \
    -e 's|use crate::runtime|use nika_runtime::runtime|g' \
    -e 's|use crate::provider|use nika_provider|g' \
    -e 's|use crate::mcp|use nika_mcp|g' \
    -e 's|use crate::tui|use nika_tui|g' \
    "$file"
}

build_cli_main() {
  local main_path="$PROJECT_ROOT/crates/nika-cli/src/main.rs"

  cat > "$main_path" <<'EOF'
//! Nika CLI - Command-line interface
//!
//! Main entry point for the Nika workflow runner.

use anyhow::Result;
use nika_core::ast::raw::parse;
use nika_runtime::executor::Executor;
use nika_tui::App;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("chat") => {
            // Launch TUI in chat mode
            let mut app = App::new();
            app.run_chat().await?;
        }
        Some("studio") => {
            // Launch TUI in studio mode
            let mut app = App::new();
            app.run_studio().await?;
        }
        Some(workflow_path) => {
            // Run workflow directly
            let content = std::fs::read_to_string(workflow_path)?;
            let workflow = parse(&content, nika_core::source::FileId(0))?;
            let executor = Executor::new(workflow);
            executor.run().await?;
        }
        None => {
            // Launch TUI home
            let mut app = App::new();
            app.run().await?;
        }
    }

    Ok(())
}
EOF

  log_success "nika-cli main.rs created"
}

move_cli_modules() {
  log_info "Moving CLI modules..."

  cd "$PROJECT_ROOT" || exit 1

  local source_dir="src"
  local dest_dir="crates/nika-cli/src"

  # Move main.rs if it exists
  if [[ -f "$source_dir/main.rs" ]]; then
    move_file "$source_dir/main.rs" "$dest_dir/main.rs.old"
    rewrite_imports_in_file "$dest_dir/main.rs.old"
  fi

  # Build new main.rs
  build_cli_main

  log_success "CLI modules moved"
}

update_cli_deps() {
  log_info "Updating nika-cli dependencies..."

  local cargo_toml="$PROJECT_ROOT/crates/nika-cli/Cargo.toml"

  cat >> "$cargo_toml" <<'EOF'

# CLI dependencies
clap = { version = "4.5", features = ["derive"] }
tracing-subscriber = "0.3"
dotenvy = "0.15"
EOF

  log_success "nika-cli dependencies updated"
}

verify_cli_builds() {
  log_info "Verifying nika-cli builds..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo build -p nika-cli 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-cli build failed"
    return 1
  fi

  log_success "nika-cli builds successfully"
}

test_cli_binary() {
  log_info "Testing nika-cli binary..."

  cd "$PROJECT_ROOT" || exit 1

  # Run help command
  if ! cargo run -p nika-cli -- --help 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "nika-cli --help failed"
    return 1
  fi

  log_success "nika-cli binary works"
}

create_checkpoint() {
  local checkpoint_name="$1"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"

  mkdir -p "$checkpoint_path"
  git status --porcelain > "$checkpoint_path/git-status.txt"
  find crates/nika-cli/src -type f -name "*.rs" | sort > "$checkpoint_path/file-list.txt"

  log_success "Checkpoint created: $checkpoint_path"
}

main() {
  log_info "Starting Step 7: Build nika-cli..."

  move_cli_modules
  update_cli_deps
  verify_cli_builds
  test_cli_binary

  create_checkpoint "07-cli-built"

  log_info "Committing changes..."
  git add -A
  git commit -m "refactor(v0.28): build nika-cli binary crate

- Move src/main.rs → crates/nika-cli/src/main.rs
- Rewrite imports to use workspace crates
- Binary builds and runs successfully

Part of Nika v0.28 workspace restructure.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"

  log_success "Step 7 complete!"
  log_info "Next step: Run ./migration/08-final-verification.sh"
}

main "$@"
