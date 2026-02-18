# Nika Quality Gaps Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close all quality gaps identified in gap analysis - CI/CD, FxHash consistency, naming, tests, cleanup.

**Architecture:** Systematic improvement across 9 areas, prioritized by impact.

**Tech Stack:** Rust, GitHub Actions, rustc-hash (FxHash)

---

## Status Update

| Gap | Planned | Actual Status |
|-----|---------|---------------|
| #1 CI/CD | Not done | **TO DO** |
| #2 Unified use: syntax | 3 forms → 1 | **ALREADY DONE** ✅ (entry.rs has ?? operator) |
| #3 FxHash consistency | Mix of std/Fx | **PARTIAL** (entry.rs done, 6 files need update) |
| #4 Rename UseWiring | Confusing | **TO DO** |
| #5 NovaNet integration | No tests | **TO DO** |
| #6 EventEmitter trait | Scattered | **TO DO** |
| #7 Token counting | TODO | **DEFER** (v0.4 feature) |
| #8 Future improvements | Arc<Value> | **DEFER** (v0.4 feature) |
| #9 dead_code cleanup | 25 annotations | **TO DO** |

---

## Task 1: CI/CD Pipeline

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/workflows/release.yml`

**Step 1: Create CI workflow**

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -Dwarnings

jobs:
  check:
    name: Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --all-features

  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --all-features -- -D warnings

  test:
    name: Test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --all-features

  build:
    name: Build Release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --release
```

**Step 2: Create release workflow**

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: aarch64-apple-darwin
            os: macos-latest
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest

    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@v2

      - name: Build
        run: cargo build --release --target ${{ matrix.target }}

      - name: Package
        run: |
          cd target/${{ matrix.target }}/release
          tar czvf nika-${{ matrix.target }}.tar.gz nika
          mv nika-${{ matrix.target }}.tar.gz ../../../

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: nika-${{ matrix.target }}
          path: nika-${{ matrix.target }}.tar.gz

  release:
    name: Create Release
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          path: artifacts

      - name: Create Release
        uses: softprops/action-gh-release@v1
        with:
          files: artifacts/**/*.tar.gz
          generate_release_notes: true
```

**Step 3: Commit**

```bash
git add .github/workflows/
git commit -m "ci: add CI/CD pipeline with GitHub Actions"
```

---

## Task 2: FxHash Consistency

**Files to modify:**
- `src/runtime/runner.rs` - HashMap → FxHashMap
- `src/runtime/executor.rs` - HashMap → FxHashMap
- `src/runtime/agent_loop.rs` - HashMap → FxHashMap
- `src/mcp/types.rs` - HashMap → FxHashMap
- `src/mcp/transport.rs` - HashMap → FxHashMap
- `src/ast/workflow.rs` - HashMap → FxHashMap

**Step 1: Update src/runtime/runner.rs**

```rust
// Change:
use std::collections::HashMap;
// To:
use rustc_hash::FxHashMap;

// Update all HashMap<K, V> to FxHashMap<K, V>
```

**Step 2: Update src/runtime/executor.rs**

Same pattern as Step 1.

**Step 3: Update src/runtime/agent_loop.rs**

Same pattern as Step 1.

**Step 4: Update src/mcp/types.rs**

Same pattern as Step 1.

**Step 5: Update src/mcp/transport.rs**

Same pattern as Step 1.

**Step 6: Update src/ast/workflow.rs**

Same pattern as Step 1.

**Step 7: Run tests**

```bash
cargo test
```
Expected: All 513 tests pass

**Step 8: Commit**

```bash
git add src/
git commit -m "perf: migrate remaining HashMap/HashSet to FxHash"
```

---

## Task 3: Rename UseWiring → WiringSpec, UseBindings → ResolvedBindings

**Files to modify:**
- `src/binding/entry.rs` - UseWiring → WiringSpec
- `src/binding/resolve.rs` - UseBindings → ResolvedBindings
- `src/binding/mod.rs` - Update exports
- All files importing these types

**Step 1: Rename in entry.rs**

```rust
// Change:
pub type UseWiring = FxHashMap<String, UseEntry>;
// To:
pub type WiringSpec = FxHashMap<String, UseEntry>;

// Add deprecation alias for backward compat:
#[deprecated(note = "use WiringSpec instead")]
pub type UseWiring = WiringSpec;
```

**Step 2: Rename in resolve.rs**

```rust
// Change struct name and update all references
pub struct ResolvedBindings { ... }

// Add deprecation alias:
#[deprecated(note = "use ResolvedBindings instead")]
pub type UseBindings = ResolvedBindings;
```

**Step 3: Update mod.rs exports**

```rust
pub use entry::{UseEntry, WiringSpec, parse_use_entry};
pub use resolve::ResolvedBindings;

// Deprecated aliases
#[allow(deprecated)]
pub use entry::UseWiring;
#[allow(deprecated)]
pub use resolve::UseBindings;
```

**Step 4: Update all imports across codebase**

Use find-replace across all .rs files.

**Step 5: Run tests**

```bash
cargo test
```

**Step 6: Commit**

```bash
git add src/
git commit -m "refactor(binding): rename UseWiring→WiringSpec, UseBindings→ResolvedBindings"
```

---

## Task 4: NovaNet Integration Test Infrastructure

**Files:**
- Create: `tests/integration/mod.rs`
- Create: `tests/integration/helpers.rs`
- Create: `tests/integration/novanet_test.rs`

**Step 1: Create integration helpers**

```rust
// tests/integration/helpers.rs
use std::path::Path;

/// Check if NovaNet MCP server binary exists
pub fn novanet_mcp_path() -> Option<String> {
    let paths = [
        std::env::var("NOVANET_MCP_PATH").ok(),
        Some("/Users/thibaut/supernovae-st/supernovae-agi/novanet-dev/tools/novanet-mcp/target/release/novanet-mcp".to_string()),
    ];

    paths.into_iter()
        .flatten()
        .find(|p| Path::new(p).exists())
}

/// Check if Neo4j is available
pub fn neo4j_available() -> bool {
    std::process::Command::new("nc")
        .args(["-z", "localhost", "7687"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Skip test macro
#[macro_export]
macro_rules! skip_without_novanet {
    () => {
        if $crate::integration::helpers::novanet_mcp_path().is_none() {
            eprintln!("SKIP: NovaNet MCP not available");
            return;
        }
    };
}

#[macro_export]
macro_rules! skip_without_neo4j {
    () => {
        if !$crate::integration::helpers::neo4j_available() {
            eprintln!("SKIP: Neo4j not available");
            return;
        }
    };
}
```

**Step 2: Create integration test**

```rust
// tests/integration/novanet_test.rs
#![cfg(feature = "integration")]

use nika::mcp::{McpClient, McpConfig};
use serde_json::json;

mod helpers;

#[tokio::test]
#[ignore] // Run with: cargo test --features integration -- --ignored
async fn test_novanet_describe_schema() {
    skip_without_novanet!();
    skip_without_neo4j!();

    let path = helpers::novanet_mcp_path().unwrap();
    let config = McpConfig {
        command: path,
        args: vec![],
        env: vec![("NOVANET_NEO4J_PASSWORD".into(), "novanetpassword".into())],
    };

    let client = McpClient::connect("novanet", config).await.unwrap();
    let result = client.call_tool("novanet_describe", json!({"describe": "schema"})).await;

    assert!(result.is_ok(), "novanet_describe failed: {:?}", result.err());
}
```

**Step 3: Update Cargo.toml**

```toml
[features]
integration = []
```

**Step 4: Commit**

```bash
git add tests/integration/ Cargo.toml
git commit -m "test: add NovaNet integration test infrastructure"
```

---

## Task 5: EventEmitter Trait

**Files:**
- Create: `src/event/emitter.rs`
- Modify: `src/event/mod.rs`
- Modify: `src/runtime/runner.rs`
- Modify: `src/runtime/executor.rs`

**Step 1: Create EventEmitter trait**

```rust
// src/event/emitter.rs
use crate::event::{EventKind, EventLog};
use std::sync::Arc;

/// Trait for emitting events
pub trait EventEmitter: Send + Sync {
    fn emit(&self, kind: EventKind) -> u64;
    fn emit_with_data(&self, kind: EventKind, data: Option<serde_json::Value>) -> u64;
}

impl EventEmitter for Arc<EventLog> {
    fn emit(&self, kind: EventKind) -> u64 {
        EventLog::emit(self, kind)
    }

    fn emit_with_data(&self, kind: EventKind, data: Option<serde_json::Value>) -> u64 {
        EventLog::emit_with_data(self, kind, data)
    }
}

/// No-op emitter for testing
pub struct NoopEmitter;

impl EventEmitter for NoopEmitter {
    fn emit(&self, _kind: EventKind) -> u64 { 0 }
    fn emit_with_data(&self, _kind: EventKind, _data: Option<serde_json::Value>) -> u64 { 0 }
}
```

**Step 2: Update mod.rs**

```rust
mod emitter;
pub use emitter::{EventEmitter, NoopEmitter};
```

**Step 3: Update runner/executor to use trait**

Replace direct EventLog calls with trait methods.

**Step 4: Run tests**

```bash
cargo test
```

**Step 5: Commit**

```bash
git add src/event/
git commit -m "refactor(event): add EventEmitter trait for cleaner emission"
```

---

## Task 6: Dead Code Cleanup

**25 #[allow(dead_code)] annotations to audit:**

| File | Line | Item | Decision |
|------|------|------|----------|
| interner.rs | 57,68,74,94 | len, is_empty, intern_arc | KEEP - used in tests |
| tui/ui.rs | 10 | struct | KEEP - feature-gated |
| tui/panels/reasoning.rs | 12 | struct | KEEP - feature-gated |
| dag/flow.rs | 31,100,125 | task_set, get_successors, contains | AUDIT - may be dead |
| runner.rs | 70 | event_log | KEEP - used in tests |
| binding/resolve.rs | 68 | is_empty | KEEP - used in tests |
| binding/template.rs | 206,222 | extract_refs, validate_refs | AUDIT - may be dead |
| ast/workflow.rs | 25,29 | SCHEMA constants | KEEP - public API |
| event/log.rs | 195-323 | multiple methods | KEEP - used in tests/future |

**Step 1: Audit dag/flow.rs**

Check if get_successors and contains are actually used anywhere.

**Step 2: Audit binding/template.rs**

Check if extract_refs and validate_refs are used.

**Step 3: For items used in tests, change annotation**

```rust
// From:
#[allow(dead_code)]
// To:
#[cfg(test)]  // if only used in tests
// Or remove if actually used
```

**Step 4: Remove truly dead code**

Delete functions that are never used anywhere.

**Step 5: Run tests**

```bash
cargo test
```

**Step 6: Commit**

```bash
git add src/
git commit -m "chore: clean up dead_code annotations"
```

---

## Execution Order

```
Task 1 (CI/CD)        ─────┐
                           ├──► Task 6 (dead_code)
Task 2 (FxHash)       ─────┤
                           │
Task 3 (Rename)       ─────┤
                           │
Task 4 (Integration)  ─────┤
                           │
Task 5 (EventEmitter) ─────┘
```

Tasks 1-5 can run in parallel. Task 6 is final cleanup.

---

## Verification Checklist

After all tasks:
- [ ] `cargo build --all-features` passes
- [ ] `cargo test` passes (513+ tests)
- [ ] `cargo clippy -- -D warnings` no warnings
- [ ] `cargo fmt --check` passes
- [ ] CI workflow runs successfully
- [ ] No std::collections::HashMap in src/ (except comments)
- [ ] 0 unnecessary #[allow(dead_code)] annotations
