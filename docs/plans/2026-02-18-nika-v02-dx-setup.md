# Nika v0.2 DX Setup Implementation Plan

> **PREREQUISITE:** Execute `spn-agi/docs/plans/2026-02-18-spn-agi-restructuration.md` first!
>
> **Note:** All paths in this plan are relative to `nika-dev/tools/nika/` (e.g., `src/` means `tools/nika/src/`).

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Set up comprehensive Developer Experience for Nika v0.2 matching NovaNet's DX quality.

**Architecture:** Hybrid integration (novanet-types crate + MCP), full Claude Code DX, complete TUI, production-grade testing stack.

**Tech Stack:** Rust, ratatui, rmcp, proptest, insta, cargo-deny, cargo-nextest

---

## Phase 1: Project Structure & Crates

### Task 1.1: Create novanet-types Crate

**Files:**
- Create: `crates/novanet-types/Cargo.toml`
- Create: `crates/novanet-types/src/lib.rs`
- Create: `crates/novanet-types/src/taxonomy.rs`
- Create: `crates/novanet-types/src/entities.rs`
- Create: `crates/novanet-types/src/denomination.rs`

**Step 1: Create Cargo.toml**

```toml
[package]
name = "novanet-types"
version = "0.1.0"
edition = "2024"
description = "Shared types for NovaNet knowledge graph integration"
license = "MIT"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

**Step 2: Create lib.rs with module exports**

```rust
//! NovaNet shared types for Nika integration.
//!
//! This crate provides type-safe structs for NovaNet MCP responses.

pub mod taxonomy;
pub mod entities;
pub mod denomination;

pub use taxonomy::*;
pub use entities::*;
pub use denomination::*;
```

**Step 3: Create denomination.rs (ADR-033)**

```rust
use serde::{Deserialize, Serialize};

/// Form type for entity denomination (ADR-033)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormType {
    /// Prose, body content
    Text,
    /// H1, H2, meta_title
    Title,
    /// After first mention, short text
    Abbrev,
    /// URL-safe slug (post-SEO pipeline)
    Url,
    /// Native script + latin hybrid (CJK locales)
    Mixed,
    /// International reference form
    Base,
}

/// A single denomination form for an entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenominationForm {
    #[serde(rename = "type")]
    pub form_type: FormType,
    pub value: String,
    #[serde(default = "default_priority")]
    pub priority: u8,
}

fn default_priority() -> u8 {
    1
}
```

**Step 4: Create entities.rs**

```rust
use serde::{Deserialize, Serialize};
use crate::DenominationForm;

/// EntityNative - locale-specific entity content (org/semantic, authored)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityNative {
    /// Composite key: "entity:{key}@{locale}"
    pub key: String,
    /// Denormalized entity key
    pub entity_key: String,
    /// Denormalized locale key (BCP-47)
    pub locale_key: String,
    /// Display name for UI
    pub display_name: String,
    /// Entity description
    pub description: Option<String>,
    /// Canonical forms for LLM (ADR-033)
    pub denomination_forms: Vec<DenominationForm>,
}

/// Term - knowledge atom (shared/knowledge, imported)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Term {
    pub key: String,
    pub value: String,
    pub domain: Option<String>,
    pub register: Option<String>,
}

/// Expression - phrasal knowledge atom
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expression {
    pub key: String,
    pub value: String,
    pub domain: Option<String>,
    pub usage_context: Option<String>,
}

/// Response from novanet_generate MCP tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateResponse {
    pub entity: Option<EntityNative>,
    pub block: Option<BlockContext>,
    pub terms: Vec<Term>,
    pub expressions: Vec<Expression>,
    pub token_count: usize,
}

/// Block generation context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockContext {
    pub block_key: String,
    pub block_type: String,
    pub page_key: String,
    pub instruction: Option<String>,
}
```

**Step 5: Create taxonomy.rs**

```rust
use serde::{Deserialize, Serialize};

/// Node realm (WHERE)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRealm {
    Shared,
    Org,
}

/// Node trait (Data Origin - ADR-024)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeTrait {
    /// Human creates ONCE
    Defined,
    /// Human writes PER locale
    Authored,
    /// External data brought in
    Imported,
    /// Our LLM produces
    Generated,
    /// Fetched from external APIs
    Retrieved,
}

/// Node layer (WHAT functional category)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeLayer {
    // Shared layers
    Config,
    Locale,
    Geography,
    Knowledge,
    // Org layers
    Foundation,
    Structure,
    Semantic,
    Instruction,
    Output,
}
```

**Step 6: Run tests**

```bash
cd crates/novanet-types && cargo test
```

**Step 7: Commit**

```bash
git add crates/novanet-types/
git commit -m "feat(novanet-types): create shared types crate for NovaNet integration

- Add DenominationForm with FormType enum (ADR-033)
- Add EntityNative, Term, Expression structs
- Add GenerateResponse for MCP novanet_generate
- Add taxonomy types (NodeRealm, NodeTrait, NodeLayer)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 1.2: Update Nika Cargo.toml

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`

**Step 1: Add novanet-types dependency**

```toml
[dependencies]
# ... existing deps ...
novanet-types = { path = "crates/novanet-types" }
```

**Step 2: Re-export types in lib.rs**

```rust
pub use novanet_types;
```

**Step 3: Verify build**

```bash
cargo build
```

**Step 4: Commit**

```bash
git add Cargo.toml src/lib.rs
git commit -m "feat(nika): add novanet-types dependency

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Phase 2: Claude Code DX Setup

### Task 2.1: Create CLAUDE.md

**Files:**
- Create: `CLAUDE.md`

**Content:**

```markdown
# CLAUDE.md

This file provides guidance to Claude Code when working in the Nika project.

## Overview

**Nika** = Native Intelligence Kernel Agent — A DAG workflow runner for AI tasks.

**Version**: v0.2 (invoke: + agent: verbs for MCP integration)

## Commands

\`\`\`bash
# Build
cargo build                          # Debug build
cargo build --features tui           # Build with TUI

# Run workflows
cargo run -- run workflow.yaml       # Execute workflow
cargo run -- validate workflow.yaml  # Validate without running
cargo run -- tui                     # Interactive TUI

# Testing
cargo nextest run                    # Fast parallel tests
cargo test -- --ignored              # Integration tests
cargo deny check                     # Security/license check

# Quality
cargo clippy -- -D warnings          # Zero warnings policy
cargo fmt --check                    # Format check
\`\`\`

## Architecture

\`\`\`
src/
├── main.rs           # CLI entry point
├── lib.rs            # Library exports
├── error.rs          # NikaError enum
│
├── ast/              # Domain model
│   ├── workflow.rs   # Workflow, Task
│   ├── action.rs     # TaskAction (5 variants)
│   ├── infer.rs      # InferParams
│   ├── exec.rs       # ExecParams
│   ├── fetch.rs      # FetchParams
│   ├── invoke.rs     # InvokeParams (MCP)
│   └── agent.rs      # AgentParams (agentic loop)
│
├── dag/              # DAG validation
├── runtime/          # Execution engine
├── binding/          # Data flow ({{use.alias}})
├── mcp/              # MCP client (rmcp)
├── store/            # DataStore
└── provider/         # LLM providers
\`\`\`

## 5 Semantic Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| \`infer:\` | LLM inference | Generate text |
| \`exec:\` | Shell command | Run npm build |
| \`fetch:\` | HTTP request | Call API |
| \`invoke:\` | MCP tool/resource | NovaNet query |
| \`agent:\` | Agentic loop | Multi-turn with tools |

## NovaNet Integration

Nika uses **Hybrid** integration:
- \`novanet-types\` crate for compile-time type safety
- MCP protocol for runtime queries

\`\`\`yaml
mcp:
  novanet:
    command: "cargo"
    args: ["run", "-p", "novanet-mcp"]

tasks:
  - id: context
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        mode: block
        page_key: homepage
\`\`\`

## Conventions

| Aspect | Convention |
|--------|------------|
| Formatting | \`cargo fmt\`, 100 chars |
| Linting | \`cargo clippy -- -D warnings\` |
| Tests | TDD, proptest for parsing |
| Commits | Conventional: \`type(scope): description\` |
| Errors | \`thiserror\` for NikaError |
```

**Step 1: Write CLAUDE.md**

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude): add CLAUDE.md for Claude Code DX

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2.2: Create ADR Structure

**Files:**
- Create: `.claude/rules/adr/README.md`
- Create: `.claude/rules/adr/core/adr-001-5-semantic-verbs.md`
- Create: `.claude/rules/adr/core/adr-002-yaml-first.md`
- Create: `.claude/rules/adr/mcp/adr-003-hybrid-integration.md`

**Step 1: Create ADR README**

```markdown
# Nika Architecture Decision Records

## ADR Index

| ADR | Domain | Title | Status |
|-----|--------|-------|--------|
| 001 | core | 5 Semantic Verbs | approved |
| 002 | core | YAML-First Workflows | approved |
| 003 | mcp | Hybrid NovaNet Integration | approved |
```

**Step 2: Create ADR-001**

```markdown
# ADR-001: 5 Semantic Verbs

**Status**: Approved (v0.2)

**Decision**: Nika supports exactly 5 semantic verbs for task actions.

| Verb | Purpose | v0.1 | v0.2 |
|------|---------|------|------|
| \`infer:\` | LLM inference | ✓ | ✓ |
| \`exec:\` | Shell command | ✓ | ✓ |
| \`fetch:\` | HTTP request | ✓ | ✓ |
| \`invoke:\` | MCP tool/resource | - | NEW |
| \`agent:\` | Agentic loop | - | NEW |

**Rationale**:
- 3 verbs cover basic automation (infer, exec, fetch)
- 2 new verbs enable knowledge graph integration (invoke, agent)
- Verbs are mutually exclusive per task
```

**Step 3: Create ADR-003**

```markdown
# ADR-003: Hybrid NovaNet Integration

**Status**: Approved (v0.2)

**Decision**: Nika integrates NovaNet via Hybrid approach:
- \`novanet-types\` crate for compile-time type safety
- MCP protocol for runtime queries

**Architecture**:
\`\`\`
novanet-types (crate) ─────┐
  DenominationForm         │
  EntityNative             ├──► Nika
  GenerateResponse         │
                           │
novanet-mcp (MCP server) ──┘
  novanet_generate
  novanet_traverse
\`\`\`

**Rationale**:
- Type safety: IDE autocomplete, compile-time validation
- Decoupling: MCP allows NovaNet to evolve independently
- Zero Cypher: Workflow authors don't write graph queries
```

**Step 4: Commit**

```bash
git add .claude/
git commit -m "docs(adr): add ADR structure with core decisions

- ADR-001: 5 Semantic Verbs
- ADR-002: YAML-First Workflows
- ADR-003: Hybrid NovaNet Integration

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2.3: Create Hooks

**Files:**
- Create: `.claude/hooks/workflow-lint.sh`
- Create: `.claude/hooks/pre-commit.sh`
- Create: `.claude/hooks/keybindings-reminder.sh`

**Step 1: Create workflow-lint hook**

```bash
#!/bin/bash
# .claude/hooks/workflow-lint.sh
# Triggered when editing workflow YAML files

set -e

if [[ "$1" == *.nika.yaml ]] || [[ "$1" == *workflow*.yaml ]]; then
    echo "🔍 Validating workflow syntax..."
    cargo run --quiet -- validate "$1" 2>/dev/null || true
fi
```

**Step 2: Create pre-commit hook**

```bash
#!/bin/bash
# .claude/hooks/pre-commit.sh

set -e

echo "🔒 Running pre-commit checks..."

# Format check
cargo fmt --check

# Clippy
cargo clippy -- -D warnings

# Tests
cargo nextest run --no-fail-fast

# Security
cargo deny check

echo "✅ All checks passed!"
```

**Step 3: Create keybindings reminder**

```bash
#!/bin/bash
# .claude/hooks/keybindings-reminder.sh
# Triggered when editing TUI files

if [[ "$1" == *tui/*.rs ]]; then
    echo "📋 Remember to update KEYBINDINGS.md if you change keybindings!"
fi
```

**Step 4: Make executable and commit**

```bash
chmod +x .claude/hooks/*.sh
git add .claude/hooks/
git commit -m "feat(hooks): add Claude Code hooks

- workflow-lint: validate workflow YAML on edit
- pre-commit: full quality checks
- keybindings-reminder: TUI keybinding docs

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 2.4: Create Skills

**Files:**
- Create: `.claude/skills/nika-run.md`
- Create: `.claude/skills/nika-validate.md`
- Create: `.claude/skills/nika-arch.md`
- Create: `.claude/skills/nika-debug.md`

**Step 1: Create nika-run skill**

```markdown
---
name: nika-run
description: Run a Nika workflow with live output
---

# /nika-run

Run a Nika workflow file with live execution output.

## Usage

\`\`\`
/nika-run <workflow.yaml>
/nika-run examples/hello-world.nika.yaml
\`\`\`

## What it does

1. Validates the workflow YAML
2. Builds the DAG
3. Connects to MCP servers (if any)
4. Executes tasks in topological order
5. Shows live progress and results

## Example

\`\`\`bash
cargo run -- run examples/hello-world.nika.yaml
\`\`\`
```

**Step 2: Create nika-arch skill**

```markdown
---
name: nika-arch
description: Display Nika architecture diagram
---

# /nika-arch

Display the Nika architecture as ASCII diagram.

## Output

\`\`\`
┌─────────────────────────────────────────────────────────────────┐
│                         NIKA v0.2                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  YAML Workflow                                                  │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                            │
│  │   DAG Builder   │  ← Validate dependencies                   │
│  └─────────────────┘                                            │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                            │
│  │    Executor     │  ← Parallel task execution                 │
│  └─────────────────┘                                            │
│       │                                                         │
│       ├── infer  → LLM Provider                                 │
│       ├── exec   → Shell                                        │
│       ├── fetch  → HTTP Client                                  │
│       ├── invoke → MCP Client                                   │
│       └── agent  → Agent Loop + MCP                             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
\`\`\`
```

**Step 3: Commit**

```bash
git add .claude/skills/
git commit -m "feat(skills): add Nika-specific Claude Code skills

- /nika-run: execute workflows
- /nika-validate: validate YAML
- /nika-arch: architecture diagram
- /nika-debug: debug specific tasks

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Phase 3: Verification Stack

### Task 3.1: Setup cargo-deny

**Files:**
- Create: `deny.toml`

**Step 1: Create deny.toml**

```toml
[advisories]
db-path = "~/.cargo/advisory-db"
vulnerability = "deny"
unmaintained = "warn"
yanked = "warn"
notice = "warn"
ignore = []

[licenses]
unlicensed = "deny"
allow = [
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
    "MPL-2.0",
]
copyleft = "warn"
confidence-threshold = 0.8

[bans]
multiple-versions = "warn"
wildcards = "deny"
highlight = "all"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

**Step 2: Test**

```bash
cargo deny check
```

**Step 3: Commit**

```bash
git add deny.toml
git commit -m "feat(security): add cargo-deny configuration

- License policy: MIT, Apache-2.0, BSD
- Advisory database checking
- Ban wildcards in dependencies

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3.2: Setup Testing Stack

**Files:**
- Modify: `Cargo.toml`
- Create: `tests/snapshots/.gitkeep`

**Step 1: Add dev-dependencies**

```toml
[dev-dependencies]
proptest = "1.4"
insta = { version = "1.34", features = ["yaml"] }
pretty_assertions = "1.4"
tokio-test = "0.4"
```

**Step 2: Create snapshot directory**

```bash
mkdir -p tests/snapshots
touch tests/snapshots/.gitkeep
```

**Step 3: Commit**

```bash
git add Cargo.toml tests/
git commit -m "feat(testing): add proptest, insta, pretty_assertions

- proptest for property-based testing
- insta for snapshot testing
- pretty_assertions for readable diffs

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 3.3: Add Example Snapshot Test

**Files:**
- Create: `tests/workflow_parsing_test.rs`

**Step 1: Create test file**

```rust
use insta::assert_yaml_snapshot;
use nika::ast::Workflow;

#[test]
fn test_parse_minimal_workflow() {
    let yaml = r#"
schema: "nika/workflow@0.2"
provider: claude

tasks:
  - id: hello
    infer:
      prompt: "Say hello"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    assert_yaml_snapshot!(workflow);
}

#[test]
fn test_parse_invoke_task() {
    let yaml = r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: "cargo"
    args: ["run", "-p", "novanet-mcp"]

tasks:
  - id: context
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        mode: block
        page_key: homepage
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    assert_yaml_snapshot!(workflow);
}
```

**Step 2: Run tests to create snapshots**

```bash
cargo test
cargo insta review  # Accept snapshots
```

**Step 3: Commit**

```bash
git add tests/ src/
git commit -m "test(parsing): add snapshot tests for workflow parsing

- Minimal workflow snapshot
- Invoke task with MCP config snapshot

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Phase 4: TUI Foundation

### Task 4.1: Add TUI Feature Flag

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add TUI dependencies and feature**

```toml
[features]
default = ["tui"]
tui = ["ratatui", "crossterm"]

[dependencies]
ratatui = { version = "0.26", optional = true }
crossterm = { version = "0.27", optional = true }
```

**Step 2: Commit**

```bash
git add Cargo.toml
git commit -m "feat(tui): add ratatui feature flag

- TUI enabled by default
- Can build without: --no-default-features

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 4.2: Create TUI Module Structure

**Files:**
- Create: `src/tui/mod.rs`
- Create: `src/tui/app.rs`
- Create: `src/tui/ui.rs`
- Create: `src/tui/dag_view.rs`

**Step 1: Create mod.rs**

```rust
//! Nika Terminal User Interface
//!
//! Features:
//! - DAG View: Visualize workflow graph
//! - Execution Monitor: Live task progress
//! - DataStore Explorer: Browse results

#[cfg(feature = "tui")]
mod app;
#[cfg(feature = "tui")]
mod ui;
#[cfg(feature = "tui")]
mod dag_view;

#[cfg(feature = "tui")]
pub use app::App;
```

**Step 2: Create app.rs skeleton**

```rust
use std::io;
use ratatui::prelude::*;

/// TUI Application state
pub struct App {
    /// Current view mode
    pub mode: ViewMode,
    /// Should exit
    pub should_quit: bool,
}

#[derive(Default, Clone, Copy)]
pub enum ViewMode {
    #[default]
    Dag,
    Execution,
    DataStore,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: ViewMode::Dag,
            should_quit: false,
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        // TUI event loop - to be implemented
        Ok(())
    }
}
```

**Step 3: Commit**

```bash
git add src/tui/
git commit -m "feat(tui): create TUI module structure

- App state machine with ViewMode
- DAG, Execution, DataStore views planned
- Feature-gated behind 'tui' flag

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Phase 5: CI/CD Setup

### Task 5.1: Create GitHub Actions Workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Step 1: Create CI workflow**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: Format check
        run: cargo fmt --check

      - name: Clippy
        run: cargo clippy -- -D warnings

      - name: Install cargo-deny
        run: cargo install cargo-deny

      - name: Security check
        run: cargo deny check

  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: Install nextest
        run: cargo install cargo-nextest

      - name: Run tests
        run: cargo nextest run

      - name: Run ignored tests
        run: cargo test -- --ignored
        continue-on-error: true

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: llvm-tools-preview

      - name: Install cargo-llvm-cov
        run: cargo install cargo-llvm-cov

      - name: Generate coverage
        run: cargo llvm-cov --lcov --output-path lcov.info

      - name: Upload coverage
        uses: codecov/codecov-action@v3
        with:
          files: lcov.info
```

**Step 2: Commit**

```bash
git add .github/
git commit -m "ci: add GitHub Actions workflow

- Format, clippy, cargo-deny checks
- Parallel test execution with nextest
- Coverage reporting to Codecov

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

---

## Phase 6: Advanced Patterns (Agent Review Additions)

> **Note:** These tasks were added after specialized agent reviews covering Rust architecture, NovaNet deep dive, LLM patterns, TUI best practices, and testing strategies.

### Task 6.1: Enhanced Error System (NIKA Error Codes)

**Files:**
- Modify: `src/error.rs`

**Step 1: Create comprehensive error enum with codes**

```rust
use thiserror::Error;

/// NikaError with error codes for debugging and documentation
///
/// Error Code Ranges:
/// - NIKA-000 to NIKA-049: Workflow parsing errors
/// - NIKA-050 to NIKA-099: DAG validation errors
/// - NIKA-100 to NIKA-109: MCP connection errors
/// - NIKA-110 to NIKA-119: Agent loop errors
/// - NIKA-120 to NIKA-129: Provider errors
/// - NIKA-130 to NIKA-139: Template binding errors
#[derive(Error, Debug)]
pub enum NikaError {
    // ═══════════════════════════════════════════════════════════════════════
    // WORKFLOW PARSING (NIKA-000 to NIKA-049)
    // ═══════════════════════════════════════════════════════════════════════

    #[error("NIKA-001: Invalid schema version '{version}', expected 'nika/workflow@0.2'")]
    InvalidSchema { version: String },

    #[error("NIKA-002: Task '{task_id}' has no action (must have infer:, exec:, fetch:, invoke:, or agent:)")]
    MissingAction { task_id: String },

    #[error("NIKA-003: Task '{task_id}' has multiple actions (only one allowed per task)")]
    MultipleActions { task_id: String },

    #[error("NIKA-004: Invalid YAML at line {line}: {message}")]
    YamlParse { line: usize, message: String },

    #[error("NIKA-005: Task ID '{task_id}' is not a valid identifier (use lowercase alphanumeric with underscores)")]
    InvalidTaskId { task_id: String },

    // ═══════════════════════════════════════════════════════════════════════
    // DAG VALIDATION (NIKA-050 to NIKA-099)
    // ═══════════════════════════════════════════════════════════════════════

    #[error("NIKA-050: Cyclic dependency detected: {cycle}")]
    CyclicDependency { cycle: String },

    #[error("NIKA-051: Task '{task_id}' depends on unknown task '{dependency}'")]
    UnknownDependency { task_id: String, dependency: String },

    #[error("NIKA-052: Flow references unknown source '{source}'")]
    UnknownFlowSource { source: String },

    #[error("NIKA-053: Flow references unknown target '{target}'")]
    UnknownFlowTarget { target: String },

    // ═══════════════════════════════════════════════════════════════════════
    // MCP ERRORS (NIKA-100 to NIKA-109)
    // ═══════════════════════════════════════════════════════════════════════

    #[error("NIKA-100: MCP connection failed for server '{server}': {message}")]
    McpConnection { server: String, message: String },

    #[error("NIKA-101: MCP tool '{tool}' not found on server '{server}'")]
    McpToolNotFound { tool: String, server: String },

    #[error("NIKA-102: MCP resource '{uri}' not found on server '{server}'")]
    McpResourceNotFound { uri: String, server: String },

    #[error("NIKA-103: MCP tool call failed: {message}")]
    McpToolCallFailed { tool: String, message: String },

    #[error("NIKA-104: MCP server '{server}' timed out after {timeout_ms}ms")]
    McpTimeout { server: String, timeout_ms: u64 },

    #[error("NIKA-105: MCP server '{server}' process exited unexpectedly with code {code}")]
    McpProcessExit { server: String, code: i32 },

    // ═══════════════════════════════════════════════════════════════════════
    // AGENT ERRORS (NIKA-110 to NIKA-119)
    // ═══════════════════════════════════════════════════════════════════════

    #[error("NIKA-110: Agent loop exceeded max iterations ({max}) for task '{task_id}'")]
    AgentMaxIterations { max: u32, task_id: String },

    #[error("NIKA-111: Agent token budget exceeded ({used}/{budget}) for task '{task_id}'")]
    AgentTokenBudget { used: u64, budget: u64, task_id: String },

    #[error("NIKA-112: Agent stop condition failed to parse: {condition}")]
    AgentStopConditionInvalid { condition: String },

    #[error("NIKA-113: Agent scope '{scope}' is not valid (use: full, minimal, focused, debug, default)")]
    AgentScopeInvalid { scope: String },

    // ═══════════════════════════════════════════════════════════════════════
    // PROVIDER ERRORS (NIKA-120 to NIKA-129)
    // ═══════════════════════════════════════════════════════════════════════

    #[error("NIKA-120: Provider '{provider}' not found")]
    ProviderNotFound { provider: String },

    #[error("NIKA-121: API key not set for provider '{provider}' (set {env_var} environment variable)")]
    ApiKeyMissing { provider: String, env_var: String },

    #[error("NIKA-122: Provider rate limited, retry after {retry_after_secs}s")]
    ProviderRateLimited { retry_after_secs: u64 },

    #[error("NIKA-123: Provider returned invalid response: {message}")]
    ProviderInvalidResponse { message: String },

    // ═══════════════════════════════════════════════════════════════════════
    // BINDING ERRORS (NIKA-130 to NIKA-139)
    // ═══════════════════════════════════════════════════════════════════════

    #[error("NIKA-130: Template binding '{{{{use.{alias}}}}}' references undefined alias")]
    BindingAliasNotFound { alias: String },

    #[error("NIKA-131: Path '{path}' not found in task result")]
    BindingPathNotFound { path: String },

    #[error("NIKA-132: Default value parse error for '{{{{use.{alias} ?? {default}}}}}': {message}")]
    BindingDefaultInvalid { alias: String, default: String, message: String },
}

/// Result type alias for Nika operations
pub type NikaResult<T> = Result<T, NikaError>;
```

**Step 2: Run tests**

```bash
cargo test error
```

**Step 3: Commit**

```bash
git add src/error.rs
git commit -m "feat(error): add comprehensive NIKA error codes

- NIKA-000 to NIKA-049: Workflow parsing
- NIKA-050 to NIKA-099: DAG validation
- NIKA-100 to NIKA-109: MCP connection
- NIKA-110 to NIKA-119: Agent loop
- NIKA-120 to NIKA-129: Provider
- NIKA-130 to NIKA-139: Template binding

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6.2: Complete NovaNet Types (Full EntityNative Schema)

**Files:**
- Modify: `crates/novanet-types/src/entities.rs`
- Create: `crates/novanet-types/src/seo.rs`
- Create: `crates/novanet-types/src/blocks.rs`
- Create: `crates/novanet-types/src/icons.rs`

**Step 1: Expand EntityNative to full 16+ fields**

```rust
use serde::{Deserialize, Serialize};
use crate::DenominationForm;

/// EntityNative - locale-specific entity content (org/semantic, authored)
/// Full schema matching NovaNet v0.13.1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityNative {
    /// Composite key: "entity:{key}@{locale}"
    pub key: String,

    /// Denormalized entity key
    pub entity_key: String,

    /// Denormalized locale key (BCP-47)
    pub locale_key: String,

    /// Display name for UI
    pub display_name: String,

    /// Entity description
    pub description: Option<String>,

    /// Canonical forms for LLM (ADR-033)
    pub denomination_forms: Vec<DenominationForm>,

    /// LLM context for generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_context: Option<String>,

    /// Curation status
    #[serde(default = "default_curation_status")]
    pub curation_status: CurationStatus,

    /// Content status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ContentStatus>,

    /// Detailed definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,

    /// Entity purpose
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,

    /// Benefits list
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benefits: Option<Vec<String>>,

    /// Usage examples
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_examples: Option<Vec<String>>,

    /// Target audience segment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience_segment: Option<String>,

    /// Cultural adaptation notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cultural_notes: Option<String>,

    /// Content version
    #[serde(default = "default_version")]
    pub version: i32,

    /// Embedding vector for semantic search
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,

    /// Creation timestamp
    pub created_at: String,

    /// Last update timestamp
    pub updated_at: String,
}

/// Curation status enum
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CurationStatus {
    #[default]
    HumanAuthored,
    MachineTranslated,
    AiGenerated,
    HybridCurated,
}

/// Content status enum
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentStatus {
    Draft,
    Reviewed,
    Published,
    Archived,
}

fn default_curation_status() -> CurationStatus {
    CurationStatus::HumanAuthored
}

fn default_version() -> i32 {
    1
}
```

**Step 2: Create seo.rs for SEO types**

```rust
use serde::{Deserialize, Serialize};

/// SEOKeyword - imported keyword data (shared/knowledge)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SEOKeyword {
    /// Unique key: "{keyword}@{locale}"
    pub key: String,

    /// Keyword text
    pub keyword: String,

    /// Locale (BCP-47)
    pub locale_key: String,

    /// Search volume (monthly)
    pub volume: Option<u32>,

    /// Keyword difficulty (0-100)
    pub difficulty: Option<u8>,

    /// CPC in cents
    pub cpc_cents: Option<u32>,

    /// URL-safe slug form
    pub slug_form: Option<String>,

    /// Data source
    pub source: Option<String>,

    /// Last updated
    pub updated_at: Option<String>,
}

/// SEOKeywordMetrics - retrieved metrics (shared/knowledge)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SEOKeywordMetrics {
    pub keyword_key: String,
    pub position: Option<u8>,
    pub impressions: Option<u32>,
    pub clicks: Option<u32>,
    pub ctr: Option<f32>,
    pub retrieved_at: String,
}

/// Knowledge tier for context assembly
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeTier {
    /// Terms, expressions, patterns
    Atoms,
    /// Entities, categories
    Entities,
    /// Full page/block context
    Full,
    /// Minimal context for simple tasks
    Minimal,
}
```

**Step 3: Create blocks.rs for block types**

```rust
use serde::{Deserialize, Serialize};

/// BlockNative - generated block content (org/output)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockNative {
    /// Composite key: "block:{block_key}@{locale}"
    pub key: String,

    /// Denormalized block key
    pub block_key: String,

    /// Denormalized locale key
    pub locale_key: String,

    /// Block type
    pub block_type: String,

    /// Generated content (JSON)
    pub content: serde_json::Value,

    /// Generation metadata
    pub generation_metadata: Option<GenerationMetadata>,

    /// Content version
    pub version: i32,

    /// Timestamps
    pub created_at: String,
    pub updated_at: String,
}

/// Metadata about content generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationMetadata {
    pub model: String,
    pub provider: String,
    pub tokens_used: u32,
    pub generation_time_ms: u64,
    pub prompt_hash: Option<String>,
}

/// OutputArtifact - bundled output (org/output)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputArtifact {
    pub key: String,
    pub artifact_type: String,
    pub pages: Vec<String>,
    pub locale_key: String,
    pub bundle_hash: String,
    pub created_at: String,
}
```

**Step 4: Create icons.rs**

```rust
use serde::{Deserialize, Serialize};

/// Dual-format icon (web + terminal)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Icon {
    /// Lucide icon name for web
    pub web: String,
    /// Unicode symbol for terminal
    pub terminal: String,
}

impl Icon {
    pub fn new(web: impl Into<String>, terminal: impl Into<String>) -> Self {
        Self {
            web: web.into(),
            terminal: terminal.into(),
        }
    }
}

/// Standard icons for NovaNet node types
pub mod icons {
    use super::Icon;

    pub fn realm_shared() -> Icon {
        Icon::new("share-2", "◎")
    }

    pub fn realm_org() -> Icon {
        Icon::new("building", "●")
    }

    pub fn layer_semantic() -> Icon {
        Icon::new("brain", "◆")
    }

    pub fn layer_output() -> Icon {
        Icon::new("file-output", "▣")
    }

    pub fn trait_defined() -> Icon {
        Icon::new("lock", "■")
    }

    pub fn trait_generated() -> Icon {
        Icon::new("sparkles", "★")
    }
}
```

**Step 5: Update lib.rs exports**

```rust
pub mod taxonomy;
pub mod entities;
pub mod denomination;
pub mod seo;
pub mod blocks;
pub mod icons;

pub use taxonomy::*;
pub use entities::*;
pub use denomination::*;
pub use seo::*;
pub use blocks::*;
pub use icons::*;
```

**Step 6: Commit**

```bash
git add crates/novanet-types/
git commit -m "feat(novanet-types): add complete NovaNet type coverage

- EntityNative: full 16+ fields matching v0.13.1
- SEOKeyword, SEOKeywordMetrics, KnowledgeTier
- BlockNative, OutputArtifact, GenerationMetadata
- Icon dual-format (web + terminal)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6.3: Enhanced AgentParams and Agent Loop

**Files:**
- Modify: `src/ast/agent.rs`
- Create: `src/runtime/agent_loop.rs`

**Step 1: Create enhanced AgentParams**

```rust
use serde::{Deserialize, Serialize};

/// Agent task parameters with full configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentParams {
    /// Initial prompt for the agent
    pub prompt: String,

    /// LLM provider (claude, openai)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Model override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// MCP servers to connect
    #[serde(default)]
    pub mcp: Vec<String>,

    /// Maximum turns before stopping
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,

    /// Stop conditions
    #[serde(default)]
    pub stop_conditions: Vec<StopCondition>,

    /// Scope preset
    #[serde(default = "default_scope")]
    pub scope: String,

    /// Token budget (total allowed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<u32>,

    /// Enable streaming responses
    #[serde(default)]
    pub streaming: bool,

    /// System prompt override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Tools to exclude from scope
    #[serde(default)]
    pub tool_blacklist: Vec<String>,

    /// Temperature override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

fn default_max_turns() -> u32 {
    10
}

fn default_scope() -> String {
    "default".to_string()
}

/// Stop condition for agent loop
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StopCondition {
    /// Stop when output contains literal string
    Literal { text: String },

    /// Stop when output matches regex
    Regex {
        pattern: String,
        #[serde(default)]
        case_insensitive: bool,
    },

    /// Stop when JSON output matches path
    JsonPath {
        path: String,
        equals: serde_json::Value,
    },

    /// Stop when specific tool is called
    ToolCall { tool: String },

    /// Stop when token count exceeded (soft stop)
    TokenLimit { max_tokens: u32 },
}

/// Scope presets
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// All tools + MCP
    Full,
    /// Text-only, no tools
    Minimal,
    /// Specific tool subset
    Focused,
    /// Full + verbose logging
    Debug,
    /// Balanced subset (default)
    Default,
}

impl Scope {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "full" => Some(Self::Full),
            "minimal" => Some(Self::Minimal),
            "focused" => Some(Self::Focused),
            "debug" => Some(Self::Debug),
            "default" => Some(Self::Default),
            _ => None,
        }
    }

    pub fn allows_tools(&self) -> bool {
        !matches!(self, Self::Minimal)
    }

    pub fn is_verbose(&self) -> bool {
        matches!(self, Self::Debug)
    }
}
```

**Step 2: Create agent_loop.rs**

```rust
use crate::error::{NikaError, NikaResult};
use crate::ast::agent::{AgentParams, StopCondition, Scope};
use crate::mcp::McpClient;
use crate::provider::{Provider, Message, ToolCall};
use crate::store::DataStore;
use tracing::{debug, info, warn, instrument};

/// Agent execution state
pub struct AgentState {
    /// Current turn number
    pub turn: u32,
    /// Total tokens used
    pub tokens_used: u64,
    /// Conversation history
    pub messages: Vec<Message>,
    /// Whether to continue
    pub should_continue: bool,
    /// Final result
    pub result: Option<String>,
}

impl AgentState {
    pub fn new() -> Self {
        Self {
            turn: 0,
            tokens_used: 0,
            messages: Vec::new(),
            should_continue: true,
            result: None,
        }
    }
}

/// Execute an agentic loop
#[instrument(skip(provider, mcp_clients, store))]
pub async fn execute_agent_loop(
    params: &AgentParams,
    provider: &dyn Provider,
    mcp_clients: &[McpClient],
    store: &DataStore,
) -> NikaResult<AgentState> {
    let scope = Scope::from_str(&params.scope)
        .ok_or_else(|| NikaError::AgentScopeInvalid {
            scope: params.scope.clone()
        })?;

    let mut state = AgentState::new();

    // Add system prompt
    if let Some(system) = &params.system_prompt {
        state.messages.push(Message::system(system.clone()));
    }

    // Add initial user prompt
    state.messages.push(Message::user(params.prompt.clone()));

    // Collect available tools from MCP servers
    let tools = if scope.allows_tools() {
        collect_tools(mcp_clients, &params.tool_blacklist).await?
    } else {
        vec![]
    };

    info!(
        max_turns = params.max_turns,
        tools_count = tools.len(),
        scope = ?scope,
        "Starting agent loop"
    );

    // Main agent loop
    while state.should_continue && state.turn < params.max_turns {
        state.turn += 1;

        debug!(turn = state.turn, "Agent turn");

        // Call LLM
        let response = provider.chat(&state.messages, &tools).await?;

        // Track token usage
        state.tokens_used += response.usage.total_tokens as u64;

        // Check token budget
        if let Some(budget) = params.token_budget {
            if state.tokens_used > budget as u64 {
                warn!(
                    used = state.tokens_used,
                    budget = budget,
                    "Token budget exceeded"
                );
                return Err(NikaError::AgentTokenBudget {
                    used: state.tokens_used,
                    budget: budget as u64,
                    task_id: "agent".to_string(),
                });
            }
        }

        // Add assistant message
        state.messages.push(Message::assistant(response.content.clone()));

        // Process tool calls
        if let Some(tool_calls) = response.tool_calls {
            for tool_call in tool_calls {
                let result = execute_tool_call(&tool_call, mcp_clients).await?;
                state.messages.push(Message::tool_result(tool_call.id, result));

                // Check stop condition: tool call
                if check_stop_conditions(&params.stop_conditions, &tool_call, &response.content) {
                    state.should_continue = false;
                    state.result = Some(response.content.clone());
                    break;
                }
            }
        } else {
            // No tool calls = final answer
            if check_stop_conditions(&params.stop_conditions, &ToolCall::none(), &response.content) {
                state.should_continue = false;
                state.result = Some(response.content);
            }
        }
    }

    // Check if we hit max iterations
    if state.turn >= params.max_turns && state.result.is_none() {
        return Err(NikaError::AgentMaxIterations {
            max: params.max_turns,
            task_id: "agent".to_string(),
        });
    }

    info!(
        turns = state.turn,
        tokens = state.tokens_used,
        "Agent loop completed"
    );

    Ok(state)
}

async fn collect_tools(
    mcp_clients: &[McpClient],
    blacklist: &[String],
) -> NikaResult<Vec<crate::provider::ToolDefinition>> {
    let mut tools = Vec::new();
    for client in mcp_clients {
        let server_tools = client.list_tools().await?;
        tools.extend(
            server_tools
                .into_iter()
                .filter(|t| !blacklist.contains(&t.name))
        );
    }
    Ok(tools)
}

async fn execute_tool_call(
    tool_call: &ToolCall,
    mcp_clients: &[McpClient],
) -> NikaResult<String> {
    for client in mcp_clients {
        if client.has_tool(&tool_call.name) {
            return client.call_tool(&tool_call.name, tool_call.arguments.clone()).await;
        }
    }
    Err(NikaError::McpToolNotFound {
        tool: tool_call.name.clone(),
        server: "any".to_string(),
    })
}

fn check_stop_conditions(
    conditions: &[StopCondition],
    tool_call: &ToolCall,
    content: &str,
) -> bool {
    for condition in conditions {
        match condition {
            StopCondition::Literal { text } => {
                if content.contains(text) {
                    return true;
                }
            }
            StopCondition::Regex { pattern, case_insensitive } => {
                let flags = if *case_insensitive { "(?i)" } else { "" };
                if let Ok(re) = regex::Regex::new(&format!("{}{}", flags, pattern)) {
                    if re.is_match(content) {
                        return true;
                    }
                }
            }
            StopCondition::JsonPath { path, equals } => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(content) {
                    if let Some(value) = json.pointer(path) {
                        if value == equals {
                            return true;
                        }
                    }
                }
            }
            StopCondition::ToolCall { tool } => {
                if &tool_call.name == tool {
                    return true;
                }
            }
            StopCondition::TokenLimit { .. } => {
                // Handled separately in main loop
            }
        }
    }

    // Default: stop if no tool calls (final answer)
    conditions.is_empty() && tool_call.is_none()
}
```

**Step 3: Commit**

```bash
git add src/ast/agent.rs src/runtime/agent_loop.rs
git commit -m "feat(agent): implement full agent loop with stop conditions

- AgentParams: token_budget, streaming, temperature, tool_blacklist
- StopCondition: literal, regex, json_path, tool_call, token_limit
- Scope presets: full, minimal, focused, debug, default
- Complete agent_loop.rs with state machine

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6.4: TUI Event Loop and State Machine

**Files:**
- Modify: `src/tui/app.rs`
- Create: `src/tui/event.rs`

**Step 1: Create event.rs with 2-phase loading**

```rust
use std::time::Duration;
use crossterm::event::{Event, EventStream, KeyCode, KeyModifiers};
use futures::StreamExt;
use tokio::sync::mpsc;

/// Event timeout for smooth animations
pub const EVENT_TIMEOUT_MS: u64 = 100;

/// Application event
pub enum AppEvent {
    /// Keyboard input
    Key(KeyCode, KeyModifiers),
    /// Terminal resize
    Resize(u16, u16),
    /// Animation tick
    Tick,
    /// Data loaded (deferred)
    DataLoaded(DataPayload),
    /// Error occurred
    Error(String),
}

/// Deferred data payload
pub enum DataPayload {
    Instances(String, Vec<String>),  // (class_name, instance_keys)
    Details(String, serde_json::Value),  // (key, full_data)
}

/// Run the event loop with 2-phase loading
pub async fn run_event_loop(
    terminal: &mut crate::tui::terminal::Terminal,
    app: &mut crate::tui::app::App,
) -> std::io::Result<()> {
    let mut event_stream = EventStream::new();
    let (tx, mut rx) = mpsc::channel::<AppEvent>(100);

    loop {
        // PHASE 1: Handle immediate events
        let event = tokio::time::timeout(
            Duration::from_millis(EVENT_TIMEOUT_MS),
            event_stream.next()
        ).await;

        match event {
            Ok(Some(Ok(Event::Key(key)))) => {
                // Handle key immediately for responsive UI
                let action = app.handle_key(key.code, key.modifiers);

                // Dispatch async loading if needed
                if let Some(load_request) = action.deferred_load {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        match load_data(load_request).await {
                            Ok(payload) => { let _ = tx.send(AppEvent::DataLoaded(payload)).await; }
                            Err(e) => { let _ = tx.send(AppEvent::Error(e.to_string())).await; }
                        }
                    });
                }

                if action.should_quit {
                    return Ok(());
                }
            }
            Ok(Some(Ok(Event::Resize(w, h)))) => {
                app.resize(w, h);
            }
            Err(_) => {
                // PHASE 2: Timeout = animation tick
                app.tick();
            }
            _ => {}
        }

        // Process deferred data
        while let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::DataLoaded(payload) => {
                    app.apply_data(payload);
                }
                AppEvent::Error(msg) => {
                    app.show_error(&msg);
                }
                _ => {}
            }
        }

        // Render
        terminal.draw(|f| crate::tui::ui::render(f, app))?;
    }
}

async fn load_data(request: LoadRequest) -> Result<DataPayload, Box<dyn std::error::Error + Send + Sync>> {
    // Async data loading - implement based on request type
    todo!()
}

pub struct LoadRequest {
    pub request_type: LoadRequestType,
}

pub enum LoadRequestType {
    Instances { class_name: String },
    Details { key: String },
}
```

**Step 2: Update app.rs with state machine**

```rust
use std::collections::HashSet;
use ratatui::prelude::*;
use crate::tui::event::DataPayload;

/// View mode for TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Dag,
    Execution,
    DataStore,
    Help,
    Search,
}

/// TUI Application state
pub struct App {
    /// Current view mode
    pub mode: ViewMode,
    /// Previous mode (for overlays)
    pub previous_mode: Option<ViewMode>,
    /// Should exit
    pub should_quit: bool,
    /// Terminal size
    pub size: (u16, u16),
    /// Collapsed tree nodes (using FxHashSet for performance)
    pub collapsed: rustc_hash::FxHashSet<String>,
    /// Selected index
    pub selected: usize,
    /// Scroll offset
    pub scroll: usize,
    /// Loading state
    pub loading: HashSet<String>,
    /// Error message
    pub error: Option<String>,
    /// Search query
    pub search_query: String,
    /// Animation frame
    pub frame: u64,
}

impl App {
    pub fn new() -> Self {
        Self {
            mode: ViewMode::Dag,
            previous_mode: None,
            should_quit: false,
            size: (80, 24),
            collapsed: rustc_hash::FxHashSet::default(),
            selected: 0,
            scroll: 0,
            loading: HashSet::new(),
            error: None,
            search_query: String::new(),
            frame: 0,
        }
    }

    /// Handle key input
    pub fn handle_key(&mut self, code: crossterm::event::KeyCode, modifiers: crossterm::event::KeyModifiers) -> KeyAction {
        use crossterm::event::KeyCode::*;

        let mut action = KeyAction::default();

        // Global keys
        match code {
            Char('q') | Esc if self.mode == ViewMode::Help || self.mode == ViewMode::Search => {
                self.mode = self.previous_mode.take().unwrap_or(ViewMode::Dag);
                return action;
            }
            Char('q') => {
                action.should_quit = true;
                return action;
            }
            Char('/') => {
                self.previous_mode = Some(self.mode);
                self.mode = ViewMode::Help;
                return action;
            }
            Char('f') if modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                self.previous_mode = Some(self.mode);
                self.mode = ViewMode::Search;
                return action;
            }
            _ => {}
        }

        // Mode-specific keys
        match self.mode {
            ViewMode::Dag => self.handle_dag_key(code, &mut action),
            ViewMode::Execution => self.handle_execution_key(code, &mut action),
            ViewMode::DataStore => self.handle_datastore_key(code, &mut action),
            ViewMode::Search => self.handle_search_key(code, &mut action),
            _ => {}
        }

        action
    }

    fn handle_dag_key(&mut self, code: crossterm::event::KeyCode, action: &mut KeyAction) {
        use crossterm::event::KeyCode::*;

        match code {
            Char('j') | Down => self.selected = self.selected.saturating_add(1),
            Char('k') | Up => self.selected = self.selected.saturating_sub(1),
            Char('h') | Left => {
                // Collapse current node
                if let Some(key) = self.get_selected_key() {
                    self.collapsed.insert(key);
                }
            }
            Char('l') | Right | Enter => {
                // Expand or trigger lazy load
                if let Some(key) = self.get_selected_key() {
                    if self.collapsed.remove(&key) {
                        // Already expanded
                    } else {
                        // Request lazy load
                        action.deferred_load = Some(crate::tui::event::LoadRequest {
                            request_type: crate::tui::event::LoadRequestType::Instances {
                                class_name: key,
                            },
                        });
                        self.loading.insert(key);
                    }
                }
            }
            Char('1') => self.mode = ViewMode::Dag,
            Char('2') => self.mode = ViewMode::Execution,
            Char('3') => self.mode = ViewMode::DataStore,
            _ => {}
        }
    }

    fn handle_execution_key(&mut self, _code: crossterm::event::KeyCode, _action: &mut KeyAction) {
        // Execution view keys
    }

    fn handle_datastore_key(&mut self, _code: crossterm::event::KeyCode, _action: &mut KeyAction) {
        // DataStore view keys
    }

    fn handle_search_key(&mut self, code: crossterm::event::KeyCode, _action: &mut KeyAction) {
        use crossterm::event::KeyCode::*;

        match code {
            Char(c) => self.search_query.push(c),
            Backspace => { self.search_query.pop(); }
            _ => {}
        }
    }

    fn get_selected_key(&self) -> Option<String> {
        // Return key of selected tree item
        None // Implement based on tree structure
    }

    /// Apply deferred data
    pub fn apply_data(&mut self, payload: DataPayload) {
        match payload {
            DataPayload::Instances(class_name, _instances) => {
                self.loading.remove(&class_name);
                // Store instances in tree
            }
            DataPayload::Details(key, _data) => {
                self.loading.remove(&key);
                // Store details
            }
        }
    }

    /// Show error
    pub fn show_error(&mut self, msg: &str) {
        self.error = Some(msg.to_string());
    }

    /// Clear error
    pub fn clear_error(&mut self) {
        self.error = None;
    }

    /// Resize handler
    pub fn resize(&mut self, w: u16, h: u16) {
        self.size = (w, h);
    }

    /// Animation tick
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        // Clear old errors after 3 seconds (30 ticks at 100ms)
        if self.frame % 30 == 0 {
            self.error = None;
        }
    }
}

/// Action result from key handling
#[derive(Default)]
pub struct KeyAction {
    pub should_quit: bool,
    pub deferred_load: Option<crate::tui::event::LoadRequest>,
}
```

**Step 3: Commit**

```bash
git add src/tui/
git commit -m "feat(tui): implement event loop with 2-phase lazy loading

- EVENT_TIMEOUT_MS = 100 for smooth animations
- 2-phase: immediate key handling + deferred data loading
- State machine with ViewMode enum
- FxHashSet for fast collapsed state lookup
- Async data loading via mpsc channel

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6.5: Auto-Fix System for Workflows

**Files:**
- Create: `src/validation/mod.rs`
- Create: `src/validation/autofix.rs`

**Step 1: Create validation module**

```rust
//! Workflow validation and auto-fix system

pub mod autofix;

use crate::ast::Workflow;
use crate::error::NikaResult;

/// Validation issue severity
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A validation issue
#[derive(Debug, Clone)]
pub struct WorkflowIssue {
    pub rule: String,
    pub message: String,
    pub severity: Severity,
    pub task_id: Option<String>,
    pub line: Option<usize>,
}

/// Validate a workflow
pub fn validate(workflow: &Workflow) -> Vec<WorkflowIssue> {
    let mut issues = Vec::new();

    // Rule: schema version
    if !workflow.schema.starts_with("nika/workflow@0.") {
        issues.push(WorkflowIssue {
            rule: "SCHEMA_VERSION".to_string(),
            message: format!("Invalid schema: {}", workflow.schema),
            severity: Severity::Error,
            task_id: None,
            line: Some(1),
        });
    }

    // Rule: task IDs unique
    let mut seen_ids = std::collections::HashSet::new();
    for task in &workflow.tasks {
        if !seen_ids.insert(&task.id) {
            issues.push(WorkflowIssue {
                rule: "TASK_ID_UNIQUE".to_string(),
                message: format!("Duplicate task ID: {}", task.id),
                severity: Severity::Error,
                task_id: Some(task.id.clone()),
                line: None,
            });
        }
    }

    // Rule: task has action
    for task in &workflow.tasks {
        if task.action.is_none() {
            issues.push(WorkflowIssue {
                rule: "TASK_ACTION_REQUIRED".to_string(),
                message: format!("Task '{}' has no action", task.id),
                severity: Severity::Error,
                task_id: Some(task.id.clone()),
                line: None,
            });
        }
    }

    // Rule: flow references valid tasks
    for flow in &workflow.flows {
        if !workflow.tasks.iter().any(|t| t.id == flow.source) {
            issues.push(WorkflowIssue {
                rule: "FLOW_SOURCE_EXISTS".to_string(),
                message: format!("Flow source '{}' not found", flow.source),
                severity: Severity::Error,
                task_id: None,
                line: None,
            });
        }
    }

    issues
}
```

**Step 2: Create autofix.rs**

```rust
use crate::ast::Workflow;
use crate::validation::WorkflowIssue;
use crate::error::NikaResult;

/// Result of applying a fix
#[derive(Debug)]
pub enum FixAction {
    /// Fix was applied
    Modified { changes: Vec<Change> },
    /// Fix was skipped
    Skipped { reason: String },
}

/// A single change
#[derive(Debug)]
pub struct Change {
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: String,
}

/// Auto-fix trait for workflow issues
pub trait WorkflowAutoFix: Send + Sync {
    /// Check if this fixer can handle the issue
    fn can_fix(&self, issue: &WorkflowIssue) -> bool;

    /// Apply the fix
    fn fix(&self, workflow: &mut Workflow, issue: &WorkflowIssue) -> NikaResult<FixAction>;

    /// Human-readable description
    fn description(&self) -> &str;
}

/// Fix engine with registered fixers
pub struct WorkflowFixEngine {
    fixers: Vec<Box<dyn WorkflowAutoFix>>,
}

impl Default for WorkflowFixEngine {
    fn default() -> Self {
        let mut engine = Self::new();
        engine.register(Box::new(SchemaMissingFixer));
        engine.register(Box::new(TaskIdFixer));
        engine.register(Box::new(TaskActionFixer));
        engine
    }
}

impl WorkflowFixEngine {
    pub fn new() -> Self {
        Self { fixers: Vec::new() }
    }

    pub fn register(&mut self, fixer: Box<dyn WorkflowAutoFix>) {
        self.fixers.push(fixer);
    }

    pub fn apply_fix(&self, workflow: &mut Workflow, issue: &WorkflowIssue) -> NikaResult<FixAction> {
        for fixer in &self.fixers {
            if fixer.can_fix(issue) {
                return fixer.fix(workflow, issue);
            }
        }
        Ok(FixAction::Skipped {
            reason: format!("No fixer for rule: {}", issue.rule)
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// FIXERS
// ═══════════════════════════════════════════════════════════════════════════

/// Fix missing schema version
pub struct SchemaMissingFixer;

impl WorkflowAutoFix for SchemaMissingFixer {
    fn can_fix(&self, issue: &WorkflowIssue) -> bool {
        issue.rule == "SCHEMA_VERSION" || issue.rule == "SCHEMA_MISSING"
    }

    fn fix(&self, workflow: &mut Workflow, _issue: &WorkflowIssue) -> NikaResult<FixAction> {
        let old = workflow.schema.clone();
        workflow.schema = "nika/workflow@0.2".to_string();
        Ok(FixAction::Modified {
            changes: vec![Change {
                field: "schema".to_string(),
                old_value: Some(old),
                new_value: workflow.schema.clone(),
            }]
        })
    }

    fn description(&self) -> &str {
        "Set schema to nika/workflow@0.2"
    }
}

/// Fix invalid task IDs
pub struct TaskIdFixer;

impl WorkflowAutoFix for TaskIdFixer {
    fn can_fix(&self, issue: &WorkflowIssue) -> bool {
        issue.rule == "TASK_ID_INVALID"
    }

    fn fix(&self, workflow: &mut Workflow, issue: &WorkflowIssue) -> NikaResult<FixAction> {
        if let Some(task_id) = &issue.task_id {
            if let Some(task) = workflow.tasks.iter_mut().find(|t| &t.id == task_id) {
                let old = task.id.clone();
                // Convert to snake_case
                let new_id = old
                    .chars()
                    .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
                    .collect::<String>();
                task.id = new_id.clone();
                return Ok(FixAction::Modified {
                    changes: vec![Change {
                        field: format!("tasks[{}].id", task_id),
                        old_value: Some(old),
                        new_value: new_id,
                    }]
                });
            }
        }
        Ok(FixAction::Skipped { reason: "Task not found".to_string() })
    }

    fn description(&self) -> &str {
        "Convert task ID to lowercase snake_case"
    }
}

/// Add missing action to task
pub struct TaskActionFixer;

impl WorkflowAutoFix for TaskActionFixer {
    fn can_fix(&self, issue: &WorkflowIssue) -> bool {
        issue.rule == "TASK_ACTION_REQUIRED"
    }

    fn fix(&self, workflow: &mut Workflow, issue: &WorkflowIssue) -> NikaResult<FixAction> {
        if let Some(task_id) = &issue.task_id {
            if let Some(task) = workflow.tasks.iter_mut().find(|t| &t.id == task_id) {
                // Default to infer action
                task.action = Some(crate::ast::TaskAction::Infer {
                    infer: crate::ast::InferParams {
                        prompt: format!("TODO: Add prompt for task '{}'", task_id),
                        provider: None,
                        model: None,
                    }
                });
                return Ok(FixAction::Modified {
                    changes: vec![Change {
                        field: format!("tasks[{}].infer", task_id),
                        old_value: None,
                        new_value: "{ prompt: TODO }".to_string(),
                    }]
                });
            }
        }
        Ok(FixAction::Skipped { reason: "Task not found".to_string() })
    }

    fn description(&self) -> &str {
        "Add default infer action with TODO prompt"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_schema_fixer_idempotent(schema in "[a-z/0-9@.]*") {
            let mut workflow = Workflow {
                schema: schema.clone(),
                ..Default::default()
            };
            let issue = WorkflowIssue {
                rule: "SCHEMA_VERSION".to_string(),
                message: String::new(),
                severity: crate::validation::Severity::Error,
                task_id: None,
                line: None,
            };

            let fixer = SchemaMissingFixer;
            let _ = fixer.fix(&mut workflow, &issue);
            let result2 = fixer.fix(&mut workflow, &issue).unwrap();

            // Second fix should still work but not change anything
            prop_assert_eq!(workflow.schema, "nika/workflow@0.2");
        }
    }
}
```

**Step 3: Commit**

```bash
git add src/validation/
git commit -m "feat(validation): add workflow auto-fix system

- WorkflowAutoFix trait with can_fix/fix/description
- WorkflowFixEngine registry pattern
- Fixers: SchemaMissingFixer, TaskIdFixer, TaskActionFixer
- Property-based tests with proptest

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6.6: Proptest Strategies for Workflow Parsing

**Files:**
- Create: `tests/proptest_workflows.rs`

**Step 1: Create property-based tests**

```rust
use proptest::prelude::*;
use nika::ast::{Workflow, Task, TaskAction, InferParams};

/// Strategy for valid task IDs
fn task_id_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,30}".prop_map(|s| s)
}

/// Strategy for valid prompts
fn prompt_strategy() -> impl Strategy<Value = String> {
    "[A-Za-z0-9 .,!?]{10,200}".prop_map(|s| s)
}

/// Strategy for valid InferParams
fn infer_params_strategy() -> impl Strategy<Value = InferParams> {
    (prompt_strategy(), prop::option::of(Just("claude".to_string())))
        .prop_map(|(prompt, provider)| InferParams {
            prompt,
            provider,
            model: None,
        })
}

/// Strategy for valid Task
fn task_strategy() -> impl Strategy<Value = Task> {
    (task_id_strategy(), infer_params_strategy())
        .prop_map(|(id, infer)| Task {
            id,
            action: Some(TaskAction::Infer { infer }),
            use_bindings: Default::default(),
            output: None,
        })
}

/// Strategy for workflows with 1-10 tasks
fn workflow_strategy() -> impl Strategy<Value = Workflow> {
    prop::collection::vec(task_strategy(), 1..10)
        .prop_map(|tasks| Workflow {
            schema: "nika/workflow@0.2".to_string(),
            provider: Some("claude".to_string()),
            mcp: Default::default(),
            tasks,
            flows: vec![],
        })
}

proptest! {
    /// Workflow serializes and deserializes without loss
    #[test]
    fn prop_workflow_roundtrip(workflow in workflow_strategy()) {
        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let parsed: Workflow = serde_yaml::from_str(&yaml).unwrap();

        prop_assert_eq!(workflow.schema, parsed.schema);
        prop_assert_eq!(workflow.tasks.len(), parsed.tasks.len());

        for (orig, parsed) in workflow.tasks.iter().zip(parsed.tasks.iter()) {
            prop_assert_eq!(orig.id, parsed.id);
        }
    }

    /// Task IDs are always valid identifiers
    #[test]
    fn prop_task_id_valid(id in task_id_strategy()) {
        prop_assert!(id.chars().next().unwrap().is_ascii_lowercase());
        prop_assert!(id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
    }

    /// Validation returns no errors for valid workflows
    #[test]
    fn prop_valid_workflow_no_errors(workflow in workflow_strategy()) {
        let issues = nika::validation::validate(&workflow);
        let errors: Vec<_> = issues.iter()
            .filter(|i| i.severity == nika::validation::Severity::Error)
            .collect();

        prop_assert!(errors.is_empty(), "Unexpected errors: {:?}", errors);
    }

    /// Auto-fix is idempotent
    #[test]
    fn prop_autofix_idempotent(workflow in workflow_strategy()) {
        let engine = nika::validation::autofix::WorkflowFixEngine::default();

        let mut workflow1 = workflow.clone();
        let issues1 = nika::validation::validate(&workflow1);
        for issue in &issues1 {
            let _ = engine.apply_fix(&mut workflow1, issue);
        }

        let mut workflow2 = workflow1.clone();
        let issues2 = nika::validation::validate(&workflow2);
        for issue in &issues2 {
            let _ = engine.apply_fix(&mut workflow2, issue);
        }

        // Should be identical after second pass
        prop_assert_eq!(workflow1.schema, workflow2.schema);
        prop_assert_eq!(workflow1.tasks.len(), workflow2.tasks.len());
    }
}
```

**Step 2: Commit**

```bash
git add tests/proptest_workflows.rs
git commit -m "test(proptest): add property-based tests for workflow parsing

- Strategies for task_id, prompt, InferParams, Task, Workflow
- prop_workflow_roundtrip: serialize/deserialize without loss
- prop_task_id_valid: IDs are valid identifiers
- prop_valid_workflow_no_errors: generated workflows validate
- prop_autofix_idempotent: fixes are stable

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6.7: Additional Skills and Hooks

**Files:**
- Create: `.claude/skills/nika-diagnose.md`
- Create: `.claude/skills/nika-binding.md`
- Create: `.claude/hooks/semantic-check.sh`

**Step 1: Create nika-diagnose skill**

```markdown
---
name: nika-diagnose
description: Diagnose workflow execution issues
---

# /nika-diagnose

Diagnose issues with a Nika workflow execution.

## Usage

\`\`\`
/nika-diagnose <workflow.yaml> [task_id]
\`\`\`

## Checks

1. **Parse Check**: Valid YAML syntax
2. **Schema Check**: Valid nika/workflow@0.2 schema
3. **DAG Check**: No cycles, all dependencies exist
4. **MCP Check**: All MCP servers reachable
5. **Provider Check**: API keys configured
6. **Binding Check**: All {{use.alias}} references valid

## Output

\`\`\`
📋 Workflow Diagnosis: example.nika.yaml
─────────────────────────────────────────

✅ Parse Check     PASS
✅ Schema Check    PASS (nika/workflow@0.2)
✅ DAG Check       PASS (5 tasks, 4 edges)
⚠️  MCP Check      WARN (novanet: not running)
✅ Provider Check  PASS (claude: API key set)
✅ Binding Check   PASS (3 bindings resolved)

Recommendations:
1. Start novanet MCP server: cargo run -p novanet-mcp
\`\`\`
```

**Step 2: Create nika-binding skill**

```markdown
---
name: nika-binding
description: Explain and debug data binding expressions
---

# /nika-binding

Explain and debug {{use.alias}} binding expressions.

## Syntax

\`\`\`yaml
use:
  # Simple: entire result
  result: other_task

  # Path: nested value
  price: api_call.data.price

  # Default: fallback value
  score: game.score ?? 0
\`\`\`

## Template Usage

\`\`\`yaml
infer:
  prompt: "Weather: {{use.forecast}}. Suggest activity."
\`\`\`

## Debugging

Run with trace:

\`\`\`bash
RUST_LOG=nika::binding=trace cargo run -- run workflow.yaml
\`\`\`

## Common Errors

| Error | Cause | Fix |
|-------|-------|-----|
| NIKA-130 | Alias not defined | Check `use:` section |
| NIKA-131 | Path not in result | Verify task output structure |
| NIKA-132 | Default parse error | Quote strings: `?? "default"` |
```

**Step 3: Create semantic-check hook**

```bash
#!/bin/bash
# .claude/hooks/semantic-check.sh
# Triggered when editing AST or validation files

set -e

# Check if we're editing semantic files
if [[ "$1" == *ast/*.rs ]] || [[ "$1" == *validation/*.rs ]]; then
    echo "🧠 Running semantic validation..."

    # Run proptest with minimal cases for fast feedback
    PROPTEST_CASES=10 cargo test proptest --quiet || true

    # Run snapshot tests
    cargo test --lib -- snapshot --quiet || true
fi
```

**Step 4: Commit**

```bash
chmod +x .claude/hooks/semantic-check.sh
git add .claude/skills/ .claude/hooks/
git commit -m "feat(dx): add diagnostic skills and semantic check hook

- /nika-diagnose: comprehensive workflow diagnosis
- /nika-binding: binding syntax reference and debugging
- semantic-check hook: runs proptest on AST/validation edits

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

### Task 6.8: Workspace Organization (5 Crates)

**Files:**
- Create: `crates/nika-core/Cargo.toml`
- Create: `crates/nika-mcp/Cargo.toml`
- Create: `crates/nika-tui/Cargo.toml`
- Modify: `Cargo.toml` (workspace)

**Step 1: Create workspace Cargo.toml**

```toml
[workspace]
members = [
    "crates/novanet-types",
    "crates/nika-core",
    "crates/nika-mcp",
    "crates/nika-tui",
    "crates/nika-cli",
]
resolver = "2"

[workspace.package]
version = "0.2.0"
edition = "2024"
license = "MIT"
repository = "https://github.com/supernovae-st/nika"

[workspace.dependencies]
# Shared across crates
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
thiserror = "1.0"
tokio = { version = "1.0", features = ["full"] }
tracing = "0.1"

# Testing
proptest = "1.4"
insta = { version = "1.34", features = ["yaml"] }
pretty_assertions = "1.4"

# TUI
ratatui = "0.26"
crossterm = "0.27"

# MCP
rmcp = "0.1"
```

**Step 2: Create nika-core Cargo.toml**

```toml
[package]
name = "nika-core"
description = "Nika core library - AST, DAG, runtime, binding"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
novanet-types = { path = "../novanet-types" }
serde.workspace = true
serde_json.workspace = true
serde_yaml.workspace = true
thiserror.workspace = true
tokio.workspace = true
tracing.workspace = true
dashmap = "5.5"
regex = "1.10"

[dev-dependencies]
proptest.workspace = true
insta.workspace = true
pretty_assertions.workspace = true
```

**Step 3: Create nika-mcp Cargo.toml**

```toml
[package]
name = "nika-mcp"
description = "Nika MCP client - Model Context Protocol integration"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
nika-core = { path = "../nika-core" }
rmcp.workspace = true
tokio.workspace = true
tracing.workspace = true
serde_json.workspace = true

[dev-dependencies]
tokio-test = "0.4"
```

**Step 4: Create nika-tui Cargo.toml**

```toml
[package]
name = "nika-tui"
description = "Nika TUI - Terminal User Interface"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
nika-core = { path = "../nika-core" }
ratatui.workspace = true
crossterm.workspace = true
tokio.workspace = true
rustc-hash = "1.1"
futures = "0.3"
```

**Step 5: Commit**

```bash
git add Cargo.toml crates/*/Cargo.toml
git commit -m "feat(workspace): organize into 5 crates

- novanet-types: NovaNet shared types
- nika-core: AST, DAG, runtime, binding
- nika-mcp: MCP client integration
- nika-tui: Terminal UI
- nika-cli: CLI binary (main)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Summary

### Created Files

```
crates/
└── novanet-types/
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── taxonomy.rs
        ├── entities.rs
        └── denomination.rs

.claude/
├── rules/
│   └── adr/
│       ├── README.md
│       └── core/
│           ├── adr-001-5-semantic-verbs.md
│           ├── adr-002-yaml-first.md
│           └── adr-003-hybrid-integration.md
├── hooks/
│   ├── workflow-lint.sh
│   ├── pre-commit.sh
│   └── keybindings-reminder.sh
└── skills/
    ├── nika-run.md
    ├── nika-validate.md
    ├── nika-arch.md
    └── nika-debug.md

src/tui/
├── mod.rs
├── app.rs
├── ui.rs
└── dag_view.rs

.github/workflows/ci.yml
CLAUDE.md
deny.toml
```

### Verification Commands

```bash
# Build everything
cargo build --all-features

# Run all checks
cargo fmt --check && cargo clippy -- -D warnings && cargo nextest run && cargo deny check

# TUI
cargo run -- tui
```
