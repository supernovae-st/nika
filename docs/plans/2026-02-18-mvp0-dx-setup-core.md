# MVP 0: DX Setup Core

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Establish minimal DX foundation for Nika v0.2 development - Claude Code integration, testing stack, and project structure.

**Architecture:** Single-crate approach (no workspace split yet), feature-gated TUI, comprehensive test infrastructure.

**Tech Stack:** Rust 2021, cargo-deny, proptest, insta, cargo-nextest

**Estimated Time:** 2-3 hours

**Prerequisites:** None (this is the first plan to execute)

---

## Task 1: Update Cargo.toml with v0.2 Dependencies

**Files:**
- Modify: `tools/nika/Cargo.toml`

### Step 1: Add new dependencies

```toml
[package]
name = "nika"
version = "0.2.0"
edition = "2021"
description = "DAG workflow runner for AI tasks with MCP integration"
license = "AGPL-3.0-or-later"
rust-version = "1.75"

[[bin]]
name = "nika"
path = "src/main.rs"

[features]
default = ["tui"]
tui = ["dep:ratatui", "dep:crossterm"]

[dependencies]
# CLI
clap = { version = "4.5", features = ["derive"] }

# Async
tokio = { version = "1.48", features = ["rt-multi-thread", "macros", "process", "sync", "time", "fs"] }
async-trait = "0.1"

# Serialization
serde = { version = "1.0", features = ["derive", "rc"] }
serde_yaml = "0.9"
serde_json = "1.0"

# JSON Schema validation
jsonschema = "0.26"

# Errors
anyhow = "1.0"
thiserror = "1.0"

# HTTP
reqwest = { version = "0.12", features = ["json"] }

# Utilities
regex = "1.11"
colored = "2.1"
dotenvy = "0.15"
dashmap = "6.1"
parking_lot = "0.12"
smallvec = "1.13"
rustc-hash = "2.1"
uuid = { version = "1.0", features = ["v4"] }
xxhash-rust = { version = "0.8", features = ["xxh3"] }

# MCP (NEW v0.2)
rmcp = { version = "0.1", features = ["client", "transport-io"], optional = true }

# TUI (feature-gated)
ratatui = { version = "0.29", optional = true }
crossterm = { version = "0.28", optional = true }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tempfile = "3.14"
proptest = "1.4"
insta = { version = "1.34", features = ["yaml"] }
pretty_assertions = "1.4"
tokio-test = "0.4"
```

### Step 2: Run cargo check to verify dependencies resolve

Run: `cd tools/nika && cargo check`
Expected: Compiles without errors (warnings OK)

### Step 3: Commit

```bash
git add tools/nika/Cargo.toml
git commit -m "chore(deps): add v0.2 dependencies (rmcp, ratatui, proptest, insta)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 2: Create CLAUDE.md for Nika

**Files:**
- Create: `tools/nika/CLAUDE.md`

### Step 1: Write CLAUDE.md

```markdown
# Nika CLI — Claude Code Context

## Overview

Nika is a DAG workflow runner for AI tasks with MCP integration. It's the "body" of the supernovae-agi architecture, executing workflows that leverage NovaNet's knowledge graph "brain".

## Architecture

```
tools/nika/src/
├── main.rs           # CLI entry point
├── lib.rs            # Public API
├── error.rs          # NikaError with codes
├── ast/              # YAML → Rust structs
│   ├── workflow.rs   # Workflow, Task
│   ├── action.rs     # TaskAction (5 variants)
│   └── output.rs     # OutputSpec
├── dag/              # DAG validation
├── runtime/          # Execution engine
│   ├── executor.rs   # Task dispatch
│   ├── runner.rs     # Workflow orchestration
│   └── agent_loop.rs # Agentic execution (v0.2)
├── mcp/              # MCP client (v0.2)
├── event/            # Event sourcing
│   ├── log.rs        # EventLog
│   └── trace.rs      # NDJSON writer
├── tui/              # Terminal UI (feature-gated)
├── binding/          # Data flow ({{use.alias}})
├── provider/         # LLM providers
└── store/            # DataStore
```

## Key Concepts

- **Workflow:** YAML file with tasks and flows
- **Task:** Single unit of work (infer, exec, fetch, invoke, agent)
- **Flow:** Dependency edge between tasks
- **Verb:** Action type (infer:, exec:, fetch:, invoke:, agent:)
- **Binding:** Data passing via `use:` block and `{{use.alias}}`

## Schema Versions

- `nika/workflow@0.1`: infer, exec, fetch verbs
- `nika/workflow@0.2`: +invoke, +agent verbs, +mcp config

## Commands

```bash
# Run workflow
cargo run -- run workflow.yaml

# Validate without executing
cargo run -- validate workflow.yaml

# Run with TUI (default feature)
cargo run -- tui workflow.yaml

# Run tests
cargo nextest run

# Run with coverage
cargo llvm-cov nextest
```

## Testing Strategy

- **Unit tests:** In-file `#[cfg(test)]` modules
- **Integration tests:** `tests/` directory
- **Snapshot tests:** insta for YAML/JSON outputs
- **Property tests:** proptest for parser fuzzing

## Error Codes

| Range | Category |
|-------|----------|
| NIKA-000-009 | Workflow errors |
| NIKA-010-019 | Task errors |
| NIKA-020-029 | DAG errors |
| NIKA-030-039 | Provider errors |
| NIKA-040-049 | Binding errors |
| NIKA-100-109 | MCP errors |
| NIKA-110-119 | Agent errors |

## Conventions

- **Imports:** Group by std, external, internal
- **Error handling:** Use `NikaError` with codes, not `anyhow`
- **Logging:** Use `tracing` macros (debug!, info!, warn!, error!)
- **Tests:** TDD - write failing test first
- **Commits:** Conventional commits with scope
```

### Step 2: Commit

```bash
git add tools/nika/CLAUDE.md
git commit -m "docs: add CLAUDE.md for Nika CLI context

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 3: Create deny.toml for Security Auditing

**Files:**
- Create: `tools/nika/deny.toml`

### Step 1: Write deny.toml

```toml
# cargo-deny configuration
# Run: cargo deny check

[advisories]
db-path = "~/.cargo/advisory-db"
db-urls = ["https://github.com/rustsec/advisory-db"]
vulnerability = "deny"
unmaintained = "warn"
yanked = "warn"
notice = "warn"

[licenses]
unlicensed = "deny"
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
    "MPL-2.0",
    "Unicode-DFS-2016",
]
copyleft = "warn"
confidence-threshold = 0.8

[bans]
multiple-versions = "warn"
wildcards = "allow"
highlight = "all"
workspace-default-features = "allow"
external-default-features = "allow"

# Deny specific crates
deny = []

# Skip specific versions (temporary allowances)
skip = []

[sources]
unknown-registry = "deny"
unknown-git = "warn"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

### Step 2: Run cargo deny check

Run: `cd tools/nika && cargo deny check 2>&1 | head -20`
Expected: No critical errors (warnings OK)

### Step 3: Commit

```bash
git add tools/nika/deny.toml
git commit -m "chore: add cargo-deny configuration for security auditing

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 4: Create .claude Directory Structure

**Files:**
- Create: `tools/nika/.claude/settings.json`
- Create: `tools/nika/.claude/rules/testing.md`
- Create: `tools/nika/.claude/rules/error-handling.md`

### Step 1: Create settings.json

```json
{
  "project": "nika",
  "version": "0.2.0",
  "primaryLanguage": "rust",
  "testCommand": "cargo nextest run",
  "buildCommand": "cargo build --release",
  "lintCommand": "cargo clippy -- -D warnings",
  "formatCommand": "cargo fmt --check"
}
```

### Step 2: Create testing.md rule

```markdown
# Testing Rules for Nika

## TDD Workflow

1. **Write failing test first** - Always
2. **Run test to see it fail** - Verify error message makes sense
3. **Write minimal code** - Only what's needed to pass
4. **Run test to see it pass** - Verify green
5. **Refactor** - Only if needed
6. **Commit** - Atomic commits per feature

## Test File Location

- Unit tests: Same file as implementation in `#[cfg(test)]` module
- Integration tests: `tests/` directory
- Snapshot tests: Use insta with `.snap` files in `tests/snapshots/`

## Test Naming

```rust
#[test]
fn test_<function>_<scenario>_<expected_outcome>() {
    // arrange
    // act
    // assert
}

// Examples:
fn test_parse_workflow_valid_yaml_returns_workflow()
fn test_parse_workflow_missing_schema_returns_error()
fn test_execute_task_infer_calls_provider()
```

## Assertions

- Use `pretty_assertions` for struct comparisons
- Use `insta::assert_yaml_snapshot!` for complex outputs
- Use `proptest` for parser fuzzing

## Mocking

- Prefer real implementations over mocks
- If mocking needed, use `mockall` or manual test doubles
- Never mock what you don't own
```

### Step 3: Create error-handling.md rule

```markdown
# Error Handling Rules for Nika

## Use NikaError, Not anyhow

```rust
// GOOD
fn parse_workflow(yaml: &str) -> Result<Workflow, NikaError> {
    serde_yaml::from_str(yaml)
        .map_err(|e| NikaError::ParseError {
            source: e.to_string(),
            line: e.location().map(|l| l.line())
        })
}

// BAD
fn parse_workflow(yaml: &str) -> anyhow::Result<Workflow> {
    Ok(serde_yaml::from_str(yaml)?)
}
```

## Error Code Assignment

Each error variant MUST have a unique code:

```rust
#[derive(Debug, thiserror::Error)]
pub enum NikaError {
    #[error("[NIKA-001] Failed to parse workflow: {source}")]
    ParseError { source: String, line: Option<usize> },

    #[error("[NIKA-100] MCP server '{name}' not connected")]
    McpNotConnected { name: String },
}
```

## Error Context

Always provide actionable context:

```rust
// GOOD
NikaError::TaskFailed {
    task_id: "generate".to_string(),
    reason: "Provider returned empty response".to_string(),
    suggestion: "Check API key and model availability".to_string(),
}

// BAD
NikaError::TaskFailed("generate failed".to_string())
```
```

### Step 4: Commit

```bash
mkdir -p tools/nika/.claude/rules
git add tools/nika/.claude/
git commit -m "chore: add .claude directory with settings and rules

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 5: Add Error Codes to NikaError

**Files:**
- Modify: `tools/nika/src/error.rs`

### Step 1: Read current error.rs

Run: `cat tools/nika/src/error.rs`

### Step 2: Update error.rs with codes

```rust
//! Nika Error Types with Error Codes
//!
//! Error code ranges:
//! - NIKA-000-009: Workflow errors
//! - NIKA-010-019: Task errors
//! - NIKA-020-029: DAG errors
//! - NIKA-030-039: Provider errors
//! - NIKA-040-049: Binding errors
//! - NIKA-100-109: MCP errors
//! - NIKA-110-119: Agent errors

use thiserror::Error;

pub type Result<T> = std::result::Result<T, NikaError>;

#[derive(Debug, Error)]
pub enum NikaError {
    // ═══════════════════════════════════════════
    // WORKFLOW ERRORS (000-009)
    // ═══════════════════════════════════════════
    #[error("[NIKA-001] Failed to parse workflow: {source}")]
    ParseError { source: String },

    #[error("[NIKA-002] Invalid schema version: {version}")]
    InvalidSchemaVersion { version: String },

    #[error("[NIKA-003] Workflow file not found: {path}")]
    WorkflowNotFound { path: String },

    #[error("[NIKA-004] Workflow validation failed: {reason}")]
    ValidationError { reason: String },

    // ═══════════════════════════════════════════
    // TASK ERRORS (010-019)
    // ═══════════════════════════════════════════
    #[error("[NIKA-010] Task '{task_id}' not found")]
    TaskNotFound { task_id: String },

    #[error("[NIKA-011] Task '{task_id}' failed: {reason}")]
    TaskFailed { task_id: String, reason: String },

    #[error("[NIKA-012] Task '{task_id}' timed out after {timeout_ms}ms")]
    TaskTimeout { task_id: String, timeout_ms: u64 },

    // ═══════════════════════════════════════════
    // DAG ERRORS (020-029)
    // ═══════════════════════════════════════════
    #[error("[NIKA-020] Cycle detected in DAG: {cycle}")]
    CycleDetected { cycle: String },

    #[error("[NIKA-021] Missing dependency: task '{task_id}' depends on unknown '{dep_id}'")]
    MissingDependency { task_id: String, dep_id: String },

    // ═══════════════════════════════════════════
    // PROVIDER ERRORS (030-039)
    // ═══════════════════════════════════════════
    #[error("[NIKA-030] Provider '{provider}' not configured")]
    ProviderNotConfigured { provider: String },

    #[error("[NIKA-031] Provider API error: {message}")]
    ProviderApiError { message: String },

    #[error("[NIKA-032] Missing API key for provider '{provider}'")]
    MissingApiKey { provider: String },

    // ═══════════════════════════════════════════
    // BINDING ERRORS (040-049)
    // ═══════════════════════════════════════════
    #[error("[NIKA-040] Binding resolution failed: {reason}")]
    BindingError { reason: String },

    #[error("[NIKA-041] Template error in '{template}': {reason}")]
    TemplateError { template: String, reason: String },

    // ═══════════════════════════════════════════
    // MCP ERRORS (100-109) - NEW v0.2
    // ═══════════════════════════════════════════
    #[error("[NIKA-100] MCP server '{name}' not connected")]
    McpNotConnected { name: String },

    #[error("[NIKA-101] MCP server '{name}' failed to start: {reason}")]
    McpStartError { name: String, reason: String },

    #[error("[NIKA-102] MCP tool '{tool}' call failed: {reason}")]
    McpToolError { tool: String, reason: String },

    #[error("[NIKA-103] MCP resource '{uri}' not found")]
    McpResourceNotFound { uri: String },

    #[error("[NIKA-104] MCP protocol error: {reason}")]
    McpProtocolError { reason: String },

    // ═══════════════════════════════════════════
    // AGENT ERRORS (110-119) - NEW v0.2
    // ═══════════════════════════════════════════
    #[error("[NIKA-110] Agent loop exceeded max turns ({max_turns})")]
    AgentMaxTurns { max_turns: u32 },

    #[error("[NIKA-111] Agent stop condition not met: {condition}")]
    AgentStopConditionFailed { condition: String },

    #[error("[NIKA-112] Invalid tool name format: {name}")]
    InvalidToolName { name: String },

    // ═══════════════════════════════════════════
    // IO ERRORS (generic)
    // ═══════════════════════════════════════════
    #[error("[NIKA-090] IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("[NIKA-091] JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

impl NikaError {
    /// Get the error code (e.g., "NIKA-001")
    pub fn code(&self) -> &'static str {
        match self {
            Self::ParseError { .. } => "NIKA-001",
            Self::InvalidSchemaVersion { .. } => "NIKA-002",
            Self::WorkflowNotFound { .. } => "NIKA-003",
            Self::ValidationError { .. } => "NIKA-004",
            Self::TaskNotFound { .. } => "NIKA-010",
            Self::TaskFailed { .. } => "NIKA-011",
            Self::TaskTimeout { .. } => "NIKA-012",
            Self::CycleDetected { .. } => "NIKA-020",
            Self::MissingDependency { .. } => "NIKA-021",
            Self::ProviderNotConfigured { .. } => "NIKA-030",
            Self::ProviderApiError { .. } => "NIKA-031",
            Self::MissingApiKey { .. } => "NIKA-032",
            Self::BindingError { .. } => "NIKA-040",
            Self::TemplateError { .. } => "NIKA-041",
            Self::McpNotConnected { .. } => "NIKA-100",
            Self::McpStartError { .. } => "NIKA-101",
            Self::McpToolError { .. } => "NIKA-102",
            Self::McpResourceNotFound { .. } => "NIKA-103",
            Self::McpProtocolError { .. } => "NIKA-104",
            Self::AgentMaxTurns { .. } => "NIKA-110",
            Self::AgentStopConditionFailed { .. } => "NIKA-111",
            Self::InvalidToolName { .. } => "NIKA-112",
            Self::IoError(_) => "NIKA-090",
            Self::JsonError(_) => "NIKA-091",
        }
    }

    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::TaskTimeout { .. } | Self::McpNotConnected { .. } | Self::ProviderApiError { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_extraction() {
        let err = NikaError::McpNotConnected { name: "novanet".to_string() };
        assert_eq!(err.code(), "NIKA-100");
    }

    #[test]
    fn test_error_display_includes_code() {
        let err = NikaError::TaskFailed {
            task_id: "gen".to_string(),
            reason: "timeout".to_string()
        };
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-011]"));
        assert!(msg.contains("gen"));
    }
}
```

### Step 3: Run tests

Run: `cd tools/nika && cargo test error`
Expected: All tests pass

### Step 4: Commit

```bash
git add tools/nika/src/error.rs
git commit -m "feat(error): add error codes NIKA-000 to NIKA-112

- Workflow errors: 000-009
- Task errors: 010-019
- DAG errors: 020-029
- Provider errors: 030-039
- Binding errors: 040-049
- MCP errors: 100-109
- Agent errors: 110-119

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Task 6: Create Test Utilities Module

**Files:**
- Create: `tools/nika/tests/common/mod.rs`
- Create: `tools/nika/tests/common/fixtures.rs`

### Step 1: Create tests/common/mod.rs

```rust
//! Shared test utilities for Nika integration tests

pub mod fixtures;

pub use fixtures::*;
```

### Step 2: Create tests/common/fixtures.rs

```rust
//! Test fixtures and helpers

use std::path::PathBuf;

/// Get path to test fixtures directory
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Get path to a specific fixture file
pub fn fixture(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

/// Create a minimal valid workflow YAML
pub fn minimal_workflow_yaml() -> &'static str {
    r#"
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: hello
    infer:
      prompt: "Say hello"
"#
}

/// Create a workflow with invoke verb (v0.2)
pub fn invoke_workflow_yaml() -> &'static str {
    r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: "echo"
    args: ["mock"]

tasks:
  - id: get_context
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        mode: block
        entity: qr-code
        locale: fr-FR
"#
}

/// Create a workflow with agent verb (v0.2)
pub fn agent_workflow_yaml() -> &'static str {
    r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: "echo"
    args: ["mock"]

tasks:
  - id: generate
    agent:
      prompt: "Generate content using novanet context"
      mcp:
        - novanet
      max_turns: 5
      stop_conditions:
        - "GENERATION_COMPLETE"
"#
}
```

### Step 3: Create fixtures directory

```bash
mkdir -p tools/nika/tests/fixtures
```

### Step 4: Commit

```bash
git add tools/nika/tests/common/
git commit -m "test: add common test utilities and fixtures

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Summary

After completing MVP 0, you will have:

1. **Updated Cargo.toml** with all v0.2 dependencies
2. **CLAUDE.md** for AI-assisted development context
3. **deny.toml** for security auditing
4. **.claude/** directory with settings and rules
5. **NikaError** with comprehensive error codes
6. **Test utilities** for integration testing

**Next:** Proceed to MVP 1 (Invoke Verb) plan.
