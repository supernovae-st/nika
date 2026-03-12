# Nika v0.28 Migration Design Document

## Executive Summary

This document describes the automated migration system for restructuring Nika from a monolithic crate into a 6-crate workspace.

**Goal:** Zero downtime, fully automated, verifiable, and rollbackable workspace restructure.

## Design Principles

1. **Atomic Steps** - Each step is independent and verifiable
2. **Checkpoint-Based** - Every step creates a rollback point
3. **Import Automation** - All import rewrites are automated via sed
4. **Test-Driven** - Every step runs full test suite
5. **Granular Commits** - One commit per step for clear history

## Architecture Overview

### File Movement Strategy

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  FILE MOVEMENT MATRIX                                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Source                           Destination                   Import Rewrite  │
│  ──────────────────────────────────────────────────────────────────────────     │
│  src/ast/                    →    crates/nika-core/src/ast/     crate::ast →   │
│  src/dag/                    →    crates/nika-core/src/dag/     nika_core::ast │
│  src/core/                   →    crates/nika-core/src/core/                   │
│  src/error.rs                →    crates/nika-core/src/error.rs                │
│                                                                                 │
│  src/runtime/                →    crates/nika-runtime/src/runtime/  crate::    │
│  src/binding/                →    crates/nika-runtime/src/binding/  runtime → │
│  src/event/                  →    crates/nika-runtime/src/event/    nika_     │
│                                                                      runtime    │
│                                                                                 │
│  src/provider/               →    crates/nika-provider/src/         crate::    │
│                                                                      provider → │
│                                                                      nika_      │
│                                                                      provider   │
│                                                                                 │
│  src/mcp/                    →    crates/nika-mcp/src/              crate::    │
│                                                                      mcp →      │
│                                                                      nika_mcp   │
│                                                                                 │
│  src/tui/                    →    crates/nika-tui/src/              crate::    │
│                                                                      tui →      │
│                                                                      nika_tui   │
│                                                                                 │
│  src/main.rs                 →    crates/nika-cli/src/main.rs       All crate::│
│                                                                      → nika_*   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Import Rewriting Patterns

Each script applies these transformations:

```bash
# Step 2: nika-core
sed -i \
  -e 's|use crate::ast|use nika_core::ast|g' \
  -e 's|use crate::dag|use nika_core::dag|g' \
  -e 's|use crate::error|use nika_core::error|g' \
  -e 's|use super::ast|use crate::ast|g' \
  "$file"

# Step 3: nika-runtime
sed -i \
  -e 's|use crate::runtime|use nika_runtime::runtime|g' \
  -e 's|use crate::binding|use nika_runtime::binding|g' \
  -e 's|use crate::event|use nika_runtime::event|g' \
  -e 's|use crate::ast|use nika_core::ast|g' \
  -e 's|use crate::dag|use nika_core::dag|g' \
  -e 's|use crate::error|use nika_core::error|g' \
  "$file"

# Step 4: nika-provider
sed -i \
  -e 's|use crate::provider|use nika_provider|g' \
  -e 's|use crate::ast|use nika_core::ast|g' \
  -e 's|use crate::error|use nika_core::error|g' \
  "$file"

# Step 5: nika-mcp
sed -i \
  -e 's|use crate::mcp|use nika_mcp|g' \
  -e 's|use crate::ast|use nika_core::ast|g' \
  -e 's|use crate::error|use nika_core::error|g' \
  "$file"

# Step 6: nika-tui
sed -i \
  -e 's|use crate::tui|use nika_tui|g' \
  -e 's|use crate::ast|use nika_core::ast|g' \
  -e 's|use crate::runtime|use nika_runtime::runtime|g' \
  -e 's|use crate::event|use nika_runtime::event|g' \
  "$file"

# Step 7: nika-cli
sed -i \
  -e 's|use crate::ast|use nika_core::ast|g' \
  -e 's|use crate::runtime|use nika_runtime::runtime|g' \
  -e 's|use crate::provider|use nika_provider|g' \
  -e 's|use crate::mcp|use nika_mcp|g' \
  -e 's|use crate::tui|use nika_tui|g' \
  "$file"
```

## Checkpoint System

### Checkpoint Structure

```
migration/checkpoints/<step-name>/
├── git-sha.txt          # Git commit SHA before step
├── git-diff.patch       # Uncommitted changes (if any)
├── git-status.txt       # Git status output
├── Cargo.toml.bak       # Backup of Cargo.toml
├── Cargo.lock.bak       # Backup of Cargo.lock
├── file-list.txt        # List of all .rs files
└── test-count.txt       # Test count output
```

### Checkpoint Creation

```bash
create_checkpoint() {
  local checkpoint_name="$1"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"

  mkdir -p "$checkpoint_path"

  # Save git state
  git rev-parse HEAD > "$checkpoint_path/git-sha.txt"
  git diff > "$checkpoint_path/git-diff.patch" || true
  git status --porcelain > "$checkpoint_path/git-status.txt"

  # Save workspace structure
  find src -type f -name "*.rs" | sort > "$checkpoint_path/file-list.txt"

  # Save Cargo state
  cp Cargo.toml "$checkpoint_path/Cargo.toml.bak"
  cp Cargo.lock "$checkpoint_path/Cargo.lock.bak"

  # Save test count
  cargo test --lib -- --list 2>&1 | grep "test result:" > "$checkpoint_path/test-count.txt" || true
}
```

### Rollback Process

```bash
rollback_to_checkpoint() {
  local checkpoint_name="$1"
  local checkpoint_path="$CHECKPOINT_DIR/$checkpoint_name"
  local checkpoint_sha=$(cat "$checkpoint_path/git-sha.txt")

  # Stash uncommitted changes
  git stash push -u -m "Pre-rollback stash"

  # Reset to checkpoint SHA
  git reset --hard "$checkpoint_sha"

  # Apply saved diff
  git apply "$checkpoint_path/git-diff.patch" || true

  # Restore Cargo files
  cp "$checkpoint_path/Cargo.toml.bak" Cargo.toml
  cp "$checkpoint_path/Cargo.lock.bak" Cargo.lock
}
```

## Step-by-Step Execution Plan

### Step 1: Setup (01-setup.sh)

**Purpose:** Create workspace structure and stub crates

**Actions:**
1. Pre-flight checks (git status, cargo, sed)
2. Create workspace root `Cargo.toml`
3. Create 6 crate directories
4. Create initial `Cargo.toml` for each crate
5. Create stub `lib.rs` files
6. Verify workspace builds

**Output:**
- `Cargo.toml` (workspace)
- `crates/nika-{core,runtime,provider,mcp,tui,cli}/`
- Checkpoint: `00-initial-state`, `01-workspace-created`

**Verification:**
```bash
cargo build --workspace
```

**Commit Message:**
```
refactor(v0.28): initialize 6-crate workspace structure

- Create workspace Cargo.toml
- Create nika-core, nika-runtime, nika-provider, nika-mcp, nika-tui, nika-cli
- Add stub lib.rs files
- Workspace builds successfully

Part of Nika v0.28 workspace restructure.
```

---

### Step 2: Move Core (02-move-core.sh)

**Purpose:** Move foundational modules to nika-core

**Modules Moved:**
- `src/ast/` → `crates/nika-core/src/ast/`
- `src/dag/` → `crates/nika-core/src/dag/`
- `src/core/` → `crates/nika-core/src/core/`
- `src/error.rs` → `crates/nika-core/src/error.rs`

**Import Rewrites:**
```bash
# Within nika-core files
crate::ast → nika_core::ast
crate::dag → nika_core::dag
crate::error → nika_core::error
```

**Output:**
- nika-core crate populated
- Checkpoint: `02-core-moved`

**Verification:**
```bash
cargo build -p nika-core
cargo test -p nika-core --lib
```

**Commit Message:**
```
refactor(v0.28): move core modules to nika-core crate

- Move src/ast/ → crates/nika-core/src/ast/
- Move src/dag/ → crates/nika-core/src/dag/
- Move src/core/ → crates/nika-core/src/core/
- Move src/error.rs → crates/nika-core/src/error.rs
- Rewrite imports: crate::ast → nika_core::ast
- All nika-core tests passing

Part of Nika v0.28 workspace restructure.
```

---

### Step 3: Move Runtime (03-move-runtime.sh)

**Purpose:** Move execution engine to nika-runtime

**Modules Moved:**
- `src/runtime/` → `crates/nika-runtime/src/runtime/`
- `src/binding/` → `crates/nika-runtime/src/binding/`
- `src/event/` → `crates/nika-runtime/src/event/`

**Import Rewrites:**
```bash
# Within nika-runtime files
crate::runtime → nika_runtime::runtime
crate::binding → nika_runtime::binding
crate::event → nika_runtime::event
crate::ast → nika_core::ast
crate::dag → nika_core::dag
crate::error → nika_core::error
```

**Dependencies Added:**
```toml
nika-core.workspace = true
futures = "0.3"
async-trait = "0.1"
uuid = { version = "1.11", features = ["v4", "serde"] }
```

**Output:**
- nika-runtime crate populated
- Checkpoint: `03-runtime-moved`

**Verification:**
```bash
cargo build -p nika-runtime
cargo test -p nika-runtime --lib
```

**Commit Message:**
```
refactor(v0.28): move runtime modules to nika-runtime crate

- Move src/runtime/ → crates/nika-runtime/src/runtime/
- Move src/binding/ → crates/nika-runtime/src/binding/
- Move src/event/ → crates/nika-runtime/src/event/
- Rewrite imports to use nika_core and nika_runtime
- All nika-runtime tests passing

Part of Nika v0.28 workspace restructure.
```

---

### Step 4: Move Provider (04-move-provider.sh)

**Purpose:** Move LLM providers to nika-provider

**Modules Moved:**
- `src/provider/` → `crates/nika-provider/src/`

**Import Rewrites:**
```bash
# Within nika-provider files
crate::provider → nika_provider
crate::ast → nika_core::ast
crate::error → nika_core::error
```

**Dependencies Added:**
```toml
nika-core.workspace = true
rig-core = "0.32"
mistralrs = "0.4"
async-trait = "0.1"
```

**Output:**
- nika-provider crate populated
- Checkpoint: `04-provider-moved`

**Verification:**
```bash
cargo build -p nika-provider
cargo test -p nika-provider --lib
```

**Commit Message:**
```
refactor(v0.28): move provider modules to nika-provider crate

- Move src/provider/ → crates/nika-provider/src/
- Rewrite imports to use nika_core
- All nika-provider tests passing

Part of Nika v0.28 workspace restructure.
```

---

### Step 5: Move MCP (05-move-mcp.sh)

**Purpose:** Move MCP client to nika-mcp

**Modules Moved:**
- `src/mcp/` → `crates/nika-mcp/src/`

**Import Rewrites:**
```bash
# Within nika-mcp files
crate::mcp → nika_mcp
crate::ast → nika_core::ast
crate::error → nika_core::error
```

**Dependencies Added:**
```toml
nika-core.workspace = true
rmcp = "0.16"
async-trait = "0.1"
```

**Output:**
- nika-mcp crate populated
- Checkpoint: `05-mcp-moved`

**Verification:**
```bash
cargo build -p nika-mcp
cargo test -p nika-mcp --lib
```

**Commit Message:**
```
refactor(v0.28): move MCP modules to nika-mcp crate

- Move src/mcp/ → crates/nika-mcp/src/
- Rewrite imports to use nika_core
- All nika-mcp tests passing

Part of Nika v0.28 workspace restructure.
```

---

### Step 6: Move TUI (06-move-tui.sh)

**Purpose:** Move terminal UI to nika-tui

**Modules Moved:**
- `src/tui/` → `crates/nika-tui/src/`

**Import Rewrites:**
```bash
# Within nika-tui files
crate::tui → nika_tui
crate::ast → nika_core::ast
crate::runtime → nika_runtime::runtime
crate::event → nika_runtime::event
```

**Dependencies Added:**
```toml
nika-core.workspace = true
nika-runtime.workspace = true
ratatui = "0.29"
crossterm = "0.28"
tui-tree-widget = "0.24"
tui-textarea = "0.7"
chrono = "0.4"
```

**Output:**
- nika-tui crate populated
- Checkpoint: `06-tui-moved`

**Verification:**
```bash
cargo build -p nika-tui
cargo test -p nika-tui --lib
```

**Commit Message:**
```
refactor(v0.28): move TUI modules to nika-tui crate

- Move src/tui/ → crates/nika-tui/src/
- Rewrite imports to use nika_core and nika_runtime
- All nika-tui tests passing

Part of Nika v0.28 workspace restructure.
```

---

### Step 7: Build CLI (07-build-cli.sh)

**Purpose:** Create CLI binary crate

**Modules Moved:**
- `src/main.rs` → `crates/nika-cli/src/main.rs`

**Import Rewrites:**
```bash
# Within nika-cli files
crate::ast → nika_core::ast
crate::runtime → nika_runtime::runtime
crate::provider → nika_provider
crate::mcp → nika_mcp
crate::tui → nika_tui
```

**Dependencies Added:**
```toml
nika-core.workspace = true
nika-runtime.workspace = true
nika-provider.workspace = true
nika-mcp.workspace = true
nika-tui.workspace = true
clap = { version = "4.5", features = ["derive"] }
tracing-subscriber = "0.3"
dotenvy = "0.15"
```

**Output:**
- nika-cli binary crate
- Checkpoint: `07-cli-built`

**Verification:**
```bash
cargo build -p nika-cli
cargo run -p nika-cli -- --version
cargo run -p nika-cli -- --help
```

**Commit Message:**
```
refactor(v0.28): build nika-cli binary crate

- Move src/main.rs → crates/nika-cli/src/main.rs
- Rewrite imports to use workspace crates
- Binary builds and runs successfully

Part of Nika v0.28 workspace restructure.
```

---

### Step 8: Final Verification (08-final-verification.sh)

**Purpose:** Comprehensive workspace validation

**Checks:**
1. Workspace structure verification
2. All crates build
3. All tests pass (4,433 expected)
4. Zero clippy warnings
5. Documentation builds
6. Binary functionality
7. Import correctness
8. Test count comparison

**Output:**
- `migration/MIGRATION_REPORT.md`
- Checkpoint: `08-final-state`

**Verification:**
```bash
cargo build --workspace --all-targets
cargo test --workspace --lib
cargo clippy --workspace -- -D warnings
cargo doc --workspace --no-deps
cargo run -p nika-cli -- --version
```

**Report Generated:**
```markdown
# Nika v0.28 Migration Report

- ✅ All 6 crates build
- ✅ 4,433 tests pass
- ✅ Zero clippy warnings
- ✅ Documentation builds
- ✅ Binary runs correctly
```

---

## Dependency Graph

Final workspace dependency structure:

```
nika-core (no workspace deps)
    ↑
    ├── nika-runtime
    │       ↑
    │       └── nika-tui
    │
    ├── nika-provider
    │
    └── nika-mcp
        ↑
        └── nika-cli
                ↑
                ├── nika-tui
                ├── nika-runtime
                ├── nika-provider
                └── nika-mcp
```

## Workspace Cargo.toml

```toml
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
thiserror = "2.0"
tracing = "0.1"
```

## Error Recovery

### Common Failures

| Failure | Cause | Recovery |
|---------|-------|----------|
| Import rewrite incomplete | Sed pattern mismatch | Manual fix + re-run step |
| Build failure | Missing dependency | Add to Cargo.toml + re-run |
| Test failure | Broken import path | Fix import + re-run |
| Checkpoint restore fails | Corrupted checkpoint | Use earlier checkpoint |

### Recovery Commands

```bash
# View recent checkpoints
ls -lt migration/checkpoints/

# Rollback to specific checkpoint
./migration/rollback.sh --checkpoint <name>

# Retry from specific step
./migration/run-migration.sh --step N

# View error logs
tail -n 100 migration/migration.log
```

## Success Criteria

Migration is successful when:

1. ✅ All 6 crates exist and have proper structure
2. ✅ Workspace builds without errors
3. ✅ All 4,433 tests pass
4. ✅ Zero clippy warnings
5. ✅ Documentation builds
6. ✅ Binary runs: `cargo run -p nika-cli -- --version`
7. ✅ No remaining `use crate::` imports in wrong locations
8. ✅ Git history shows 8 granular commits
9. ✅ Migration report generated

## Timeline

| Phase | Duration | Cumulative |
|-------|----------|------------|
| Setup | 30s | 30s |
| Core | 1m | 1m 30s |
| Runtime | 1m | 2m 30s |
| Provider | 1m | 3m 30s |
| MCP | 1m | 4m 30s |
| TUI | 1m | 5m 30s |
| CLI | 30s | 6m |
| Verification | 2m | 8m |

**Total:** ~8 minutes (automated)

## Future Improvements

1. **Parallel Testing** - Run crate tests in parallel
2. **Incremental Build Cache** - Use sccache for faster builds
3. **Import Analysis** - Pre-flight scan for import complexity
4. **Automatic Dependency Resolution** - Infer missing deps
5. **Web UI** - Real-time migration progress dashboard
