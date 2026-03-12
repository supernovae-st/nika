#!/usr/bin/env bash
# Migration Step 1: Setup and Preparation
# Creates workspace structure and checkpoints

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
readonly NC='\033[0m' # No Color

# === Logging ===
log_info() {
  echo -e "${BLUE}[INFO]${NC} $*" | tee -a "$MIGRATION_LOG"
}

log_success() {
  echo -e "${GREEN}[SUCCESS]${NC} $*" | tee -a "$MIGRATION_LOG"
}

log_warn() {
  echo -e "${YELLOW}[WARN]${NC} $*" | tee -a "$MIGRATION_LOG"
}

log_error() {
  echo -e "${RED}[ERROR]${NC} $*" | tee -a "$MIGRATION_LOG"
}

# === Trap for cleanup ===
cleanup() {
  local exit_code=$?
  if [[ $exit_code -ne 0 ]]; then
    log_error "Migration failed with exit code $exit_code"
    log_info "Run './migration/rollback.sh' to restore previous state"
  fi
}
trap cleanup EXIT

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

  # Check current branch
  local current_branch
  current_branch="$(git branch --show-current)"
  log_info "Current branch: $current_branch"

  # Verify cargo works
  if ! command -v cargo &>/dev/null; then
    log_error "cargo command not found"
    exit 1
  fi

  # Verify sed works (GNU sed required)
  if ! sed --version 2>&1 | grep -q "GNU sed"; then
    log_error "GNU sed required (found BSD sed)"
    log_error "On macOS: brew install gnu-sed"
    exit 1
  fi

  # Run initial tests to establish baseline
  log_info "Running baseline tests..."
  if ! cargo test --quiet --lib 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "Baseline tests failed - fix before migrating"
    exit 1
  fi

  log_success "Pre-flight checks passed"
}

# === Create checkpoint ===
create_checkpoint() {
  local checkpoint_name="$1"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"

  log_info "Creating checkpoint: $checkpoint_name"

  mkdir -p "$checkpoint_path"

  # Save git state
  git rev-parse HEAD > "$checkpoint_path/git-sha.txt"
  git diff > "$checkpoint_path/git-diff.patch" || true
  git status --porcelain > "$checkpoint_path/git-status.txt"

  # Save workspace structure
  find src -type f -name "*.rs" | sort > "$checkpoint_path/file-list.txt"

  # Save Cargo.toml state
  cp Cargo.toml "$checkpoint_path/Cargo.toml.bak"
  cp Cargo.lock "$checkpoint_path/Cargo.lock.bak"

  # Save test count
  cargo test --lib -- --list 2>&1 | grep "test result:" > "$checkpoint_path/test-count.txt" || true

  log_success "Checkpoint created: $checkpoint_path"
}

# === Create new crate structure ===
create_workspace_structure() {
  log_info "Creating workspace structure..."

  cd "$PROJECT_ROOT" || exit 1

  # Create workspace root Cargo.toml
  log_info "Creating workspace Cargo.toml..."
  cat > Cargo.toml <<'EOF'
[workspace]
resolver = "2"
members = [
    "crates/nika-core",
    "crates/nika-runtime",
    "crates/nika-provider",
    "crates/nika-mcp",
    "crates/nika-tui",
    "crates/nika-cli",
]

[workspace.package]
version = "0.28.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/SuperNovae-studio/nika"

[workspace.dependencies]
# Workspace crates
nika-core = { path = "crates/nika-core" }
nika-runtime = { path = "crates/nika-runtime" }
nika-provider = { path = "crates/nika-provider" }
nika-mcp = { path = "crates/nika-mcp" }
nika-tui = { path = "crates/nika-tui" }

# External dependencies (versions aligned)
tokio = { version = "1.42", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
thiserror = "2.0"
tracing = "0.1"
anyhow = "1.0"

# Testing
rstest = "0.23"
insta = "1.41"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
EOF

  # Create crate directories
  mkdir -p crates/{nika-core,nika-runtime,nika-provider,nika-mcp,nika-tui,nika-cli}/src

  log_success "Workspace structure created"
}

# === Create initial Cargo.toml files ===
create_crate_manifests() {
  log_info "Creating crate manifests..."

  # nika-core
  cat > crates/nika-core/Cargo.toml <<'EOF'
[package]
name = "nika-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
serde_yaml.workspace = true
thiserror.workspace = true
tracing.workspace = true

[dev-dependencies]
rstest.workspace = true
insta.workspace = true
EOF

  # nika-runtime
  cat > crates/nika-runtime/Cargo.toml <<'EOF'
[package]
name = "nika-runtime"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
nika-core.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
anyhow.workspace = true

[dev-dependencies]
rstest.workspace = true
EOF

  # nika-provider
  cat > crates/nika-provider/Cargo.toml <<'EOF'
[package]
name = "nika-provider"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
nika-core.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true

[dev-dependencies]
rstest.workspace = true
EOF

  # nika-mcp
  cat > crates/nika-mcp/Cargo.toml <<'EOF'
[package]
name = "nika-mcp"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
nika-core.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true

[dev-dependencies]
rstest.workspace = true
EOF

  # nika-tui
  cat > crates/nika-tui/Cargo.toml <<'EOF'
[package]
name = "nika-tui"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
nika-core.workspace = true
nika-runtime.workspace = true
tokio.workspace = true
serde.workspace = true
thiserror.workspace = true
tracing.workspace = true

[dev-dependencies]
rstest.workspace = true
EOF

  # nika-cli
  cat > crates/nika-cli/Cargo.toml <<'EOF'
[package]
name = "nika-cli"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "nika"
path = "src/main.rs"

[dependencies]
nika-core.workspace = true
nika-runtime.workspace = true
nika-provider.workspace = true
nika-mcp.workspace = true
nika-tui.workspace = true
tokio.workspace = true
anyhow.workspace = true
tracing.workspace = true

[dev-dependencies]
rstest.workspace = true
EOF

  log_success "Crate manifests created"
}

# === Create stub lib.rs files ===
create_stub_files() {
  log_info "Creating stub lib.rs files..."

  # nika-core stub
  cat > crates/nika-core/src/lib.rs <<'EOF'
// Temporary stub - will be replaced by moved code
#![allow(unused)]

pub mod ast {
    pub fn placeholder() {}
}

pub mod dag {
    pub fn placeholder() {}
}

pub mod error {
    pub fn placeholder() {}
}
EOF

  # nika-runtime stub
  cat > crates/nika-runtime/src/lib.rs <<'EOF'
// Temporary stub - will be replaced by moved code
#![allow(unused)]

pub mod executor {
    pub fn placeholder() {}
}
EOF

  # nika-provider stub
  cat > crates/nika-provider/src/lib.rs <<'EOF'
// Temporary stub - will be replaced by moved code
#![allow(unused)]

pub mod rig {
    pub fn placeholder() {}
}
EOF

  # nika-mcp stub
  cat > crates/nika-mcp/src/lib.rs <<'EOF'
// Temporary stub - will be replaced by moved code
#![allow(unused)]

pub mod client {
    pub fn placeholder() {}
}
EOF

  # nika-tui stub
  cat > crates/nika-tui/src/lib.rs <<'EOF'
// Temporary stub - will be replaced by moved code
#![allow(unused)]

pub mod app {
    pub fn placeholder() {}
}
EOF

  # nika-cli stub
  cat > crates/nika-cli/src/main.rs <<'EOF'
// Temporary stub - will be replaced by moved code
fn main() {
    println!("Nika v0.28.0 (migration in progress)");
}
EOF

  log_success "Stub files created"
}

# === Verify workspace builds ===
verify_workspace_builds() {
  log_info "Verifying workspace builds..."

  cd "$PROJECT_ROOT" || exit 1

  if ! cargo build --workspace 2>&1 | tee -a "$MIGRATION_LOG"; then
    log_error "Workspace build failed"
    return 1
  fi

  log_success "Workspace builds successfully"
}

# === Main execution ===
main() {
  log_info "Starting Nika v0.28 migration setup..."
  log_info "Project root: $PROJECT_ROOT"

  # Create migration directory structure
  mkdir -p "$PROJECT_ROOT/migration"/{checkpoints,logs,scripts}
  mkdir -p "$CHECKPOINT_DIR"

  # Initialize log file
  echo "=== Nika v0.28 Migration Log ===" > "$MIGRATION_LOG"
  echo "Started: $(date)" >> "$MIGRATION_LOG"

  # Run setup steps
  preflight_checks
  create_checkpoint "00-initial-state"
  create_workspace_structure
  create_crate_manifests
  create_stub_files
  verify_workspace_builds

  # Final checkpoint
  create_checkpoint "01-workspace-created"

  log_success "Migration setup complete!"
  log_info "Next step: Run ./migration/02-move-core.sh"
}

main "$@"
