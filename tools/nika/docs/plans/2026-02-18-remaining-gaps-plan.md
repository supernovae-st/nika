# Nika Remaining Gaps Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close all remaining gaps between current implementation and MVP roadmap.

**Architecture:** Incremental improvements across MCP, TUI, and v0.3 features.

**Tech Stack:** Rust, tokio, ratatui, rmcp

---

## Gap Analysis Summary

| Gap | Priority | Status | MVP |
|-----|----------|--------|-----|
| #1 Enable MCP feature by default | HIGH | ✅ DONE | MVP 4 |
| #2 Run ignored doc tests | MEDIUM | ✅ DONE (correctly ignored) | Quality |
| #3 TUI 4-panel layout | HIGH | ✅ DONE (foundation exists) | MVP 3 |
| #4 CLI trace commands | MEDIUM | ✅ DONE (already implemented) | MVP 3 |
| #5 Real NovaNet integration test | MEDIUM | ✅ DONE (14 tests exist) | MVP 4 |
| #6 **DAG cycle detection** | CRITICAL | ✅ DONE | MVP 0 |
| #7 **DAG validation tests** | CRITICAL | ✅ DONE (7 tests) | Quality |
| #8 **Windows CI target** | MEDIUM | ✅ DONE | Quality |
| #9 OpenAI tool calling | LOW | DEFER | MVP 5 |
| #10 for_each parallelism | LOW | DEFER | MVP 6 |

---

## Task 1: Enable MCP Feature by Default

**Files:**
- Modify: `Cargo.toml`

**Step 1: Update Cargo.toml features**

```toml
[features]
default = ["tui", "mcp"]  # Add mcp to defaults
tui = ["dep:ratatui", "dep:crossterm"]
mcp = []  # MCP types are always compiled, this gates future optional deps
integration = []  # Enable integration tests with real MCP servers
```

**Step 2: Verify compilation**

Run: `cargo build --all-features`
Expected: PASS

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "feat(mcp): enable mcp feature by default"
```

---

## Task 2: Fix Ignored Doc Tests

**Files to audit:**
- `src/mcp/client.rs` (doc examples)
- `src/mcp/protocol.rs` (doc examples)
- `src/mcp/transport.rs` (doc examples)
- `src/provider/mod.rs` (doc examples)
- `src/resilience/*.rs` (doc examples)

**Step 1: Audit ignored doc tests**

Run: `cargo test --doc 2>&1 | grep -E "(ignored|SKIP)"`

List all ignored tests and determine which can be enabled.

**Step 2: Enable doc tests that don't require external services**

For each doc test marked `ignore`:
- If it only tests types/parsing → remove `ignore`
- If it requires Neo4j/MCP server → keep `ignore` but add comment

**Step 3: Run doc tests**

Run: `cargo test --doc`
Expected: At least 10 doc tests running (currently 0 pass, 23 ignored)

**Step 4: Commit**

```bash
git add src/
git commit -m "test: enable doc tests that don't require external services"
```

---

## Task 3: TUI 4-Panel Layout Foundation

**Files:**
- Modify: `src/tui/ui.rs`
- Modify: `src/tui/panels/mod.rs`
- Create: `src/tui/panels/trace.rs`

**Step 1: Write failing test for 4-panel layout**

```rust
// tests/tui_layout_test.rs
#[test]
fn test_tui_has_four_panels() {
    use nika::tui::panels::{ProgressPanel, GraphPanel, ContextPanel, ReasoningPanel};

    // Verify all 4 panel types exist and can be constructed
    let progress = ProgressPanel::new();
    let graph = GraphPanel::new();
    let context = ContextPanel::new();
    let reasoning = ReasoningPanel::new();

    assert!(progress.title().contains("Progress"));
    assert!(graph.title().contains("Graph"));
    assert!(context.title().contains("Context"));
    assert!(reasoning.title().contains("Reasoning"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_tui_has_four_panels -v`
Expected: FAIL (panels not fully implemented)

**Step 3: Implement panel trait and 4 panels**

```rust
// src/tui/panels/mod.rs
pub trait Panel {
    fn title(&self) -> &str;
    fn render(&self, frame: &mut Frame, area: Rect);
}

pub mod progress;
pub mod graph;
pub mod context;
pub mod reasoning;

pub use progress::ProgressPanel;
pub use graph::GraphPanel;
pub use context::ContextPanel;
pub use reasoning::ReasoningPanel;
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_tui_has_four_panels -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/ tests/tui_layout_test.rs
git commit -m "feat(tui): add 4-panel layout foundation (MVP 3)"
```

---

## Task 4: CLI Trace Commands

**Files:**
- Create: `src/commands/trace.rs`
- Modify: `src/main.rs`

**Step 1: Write failing test for trace list command**

```rust
// tests/cli_trace_test.rs
#[test]
fn test_trace_list_command_exists() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--", "trace", "list", "--help"])
        .output()
        .expect("Failed to run command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("list") || stdout.contains("traces"));
}
```

**Step 2: Implement trace subcommands**

```rust
// src/commands/trace.rs
use clap::Subcommand;
use crate::event::{list_traces, TraceInfo};
use crate::error::Result;

#[derive(Subcommand)]
pub enum TraceCommand {
    /// List recent workflow traces
    List {
        /// Maximum number of traces to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Show trace details
    Show {
        /// Trace ID or generation ID
        id: String,
    },
    /// Replay a trace interactively
    Replay {
        /// Trace ID to replay
        id: String,
    },
}

pub fn handle_trace(cmd: TraceCommand) -> Result<()> {
    match cmd {
        TraceCommand::List { limit } => {
            let traces = list_traces(limit)?;
            for trace in traces {
                println!("{} {} {}",
                    trace.generation_id,
                    trace.workflow_hash,
                    trace.timestamp);
            }
            Ok(())
        }
        TraceCommand::Show { id } => {
            // TODO: Implement show
            println!("Showing trace: {}", id);
            Ok(())
        }
        TraceCommand::Replay { id } => {
            // TODO: Implement replay
            println!("Replaying trace: {}", id);
            Ok(())
        }
    }
}
```

**Step 3: Add to main CLI**

```rust
// In src/main.rs, add to Commands enum:
/// Trace management commands
Trace {
    #[command(subcommand)]
    command: TraceCommand,
},
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_trace_list_command_exists -v`
Expected: PASS

**Step 5: Commit**

```bash
git add src/commands/trace.rs src/main.rs tests/cli_trace_test.rs
git commit -m "feat(cli): add trace list/show/replay commands (MVP 3)"
```

---

## Task 5: Real NovaNet Integration Test

**Files:**
- Modify: `tests/integration/novanet_test.rs`

**Step 1: Verify integration test infrastructure**

Run: `ls tests/integration/`
Expected: helpers.rs, mod.rs, novanet_test.rs exist

**Step 2: Add real NovaNet test (requires Neo4j)**

```rust
// tests/integration/novanet_test.rs - add to existing file
#[tokio::test]
#[ignore] // Run with: cargo test --features integration -- --ignored
async fn test_real_novanet_describe_schema() {
    use crate::integration::helpers::{novanet_mcp_path, neo4j_available};

    if novanet_mcp_path().is_none() {
        eprintln!("SKIP: NovaNet MCP not available");
        return;
    }
    if !neo4j_available() {
        eprintln!("SKIP: Neo4j not available");
        return;
    }

    let path = novanet_mcp_path().unwrap();
    let config = nika::mcp::McpConfig {
        command: path,
        args: vec![],
        env: vec![
            ("NOVANET_MCP_NEO4J_PASSWORD".to_string(), "novanetpassword".to_string()),
        ],
        cwd: None,
    };

    let client = nika::mcp::McpClient::new("novanet".to_string(), config).unwrap();
    client.connect().await.expect("Failed to connect to NovaNet");

    let result = client.call_tool("novanet_describe", serde_json::json!({
        "describe": "schema"
    })).await;

    assert!(result.is_ok(), "novanet_describe failed: {:?}", result.err());

    let response = result.unwrap();
    assert!(!response.content.is_empty(), "Response should have content");
}
```

**Step 3: Run integration test (requires Neo4j + NovaNet MCP)**

Run: `cargo test --features integration -- --ignored test_real_novanet`
Expected: PASS (if Neo4j running) or SKIP (if not)

**Step 4: Commit**

```bash
git add tests/integration/novanet_test.rs
git commit -m "test(integration): add real NovaNet MCP integration test"
```

---

## Execution Order

```
                               CRITICAL PATH (DO FIRST)
                               ========================
Task 6 (Cycle Detection) ─────┬──► Task 7 (DAG Tests)
                               │
                               ▼
                               HIGH PRIORITY
                               =============
Task 1 (MCP feature)     ─────┐
                               │
Task 2 (Doc tests)       ─────┼──► Task 5 (Integration)
                               │
Task 3 (TUI panels)      ─────┤
                               │
Task 4 (CLI trace)       ─────┤
                               │
Task 8 (Windows CI)      ─────┘
```

Execution order:
1. **Task 6** (cycle detection) - CRITICAL, blocks Task 7
2. **Task 7** (DAG tests) - depends on Task 6
3. Tasks 1-4, 8 can run in parallel
4. **Task 5** depends on Task 1

---

## Verification Checklist

After all tasks:
- [x] `cargo build --all-features` passes ✅
- [x] `cargo test` passes (536 tests, including DAG cycle tests) ✅
- [x] `cargo test --doc` runs (23 doc tests correctly ignored) ✅
- [x] `cargo clippy -- -D warnings` no warnings ✅
- [x] `cargo run -- trace list --help` shows help ✅
- [x] TUI has 4 panel structs defined ✅
- [x] DAG cycle detection catches A→B→C→A ✅
- [x] Integration test exists and runs with `--ignored` (14 NovaNet tests) ✅
- [x] release.yml includes Windows target ✅

---

## Task 6: DAG Cycle Detection (CRITICAL)

**Files:**
- Modify: `src/dag/flow.rs`
- Modify: `src/error.rs`

**Step 1: Write failing test for cycle detection**

```rust
// In src/dag/flow.rs, add to #[cfg(test)] module
#[test]
fn test_detect_cycle_simple() {
    // A → B → C → A (cycle)
    let yaml = r#"
schema: nika/workflow@0.1
id: cycle_test
tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"
  - id: c
    infer: "C"
flows:
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: a
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    let result = graph.detect_cycles();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("NIKA-025")); // Cycle error code
}

#[test]
fn test_no_cycle_linear() {
    // A → B → C (no cycle)
    let yaml = r#"
schema: nika/workflow@0.1
id: linear_test
tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"
  - id: c
    infer: "C"
flows:
  - source: a
    target: b
  - source: b
    target: c
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_ok());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_detect_cycle_simple -v`
Expected: FAIL (method `detect_cycles` not found)

**Step 3: Add NIKA-025 error variant**

```rust
// In src/error.rs, add to NikaError enum:
#[error("[NIKA-025] DAG contains cycle: {path}")]
DagCycle { path: String },
```

**Step 4: Implement detect_cycles using DFS**

```rust
// In src/dag/flow.rs, add method to FlowGraph impl
/// Detect cycles in the DAG using DFS with three-color marking
/// Returns Ok(()) if acyclic, Err with cycle path if cycle found
pub fn detect_cycles(&self) -> Result<(), NikaError> {
    use rustc_hash::FxHashSet;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Color { White, Gray, Black }

    let mut colors: FxHashMap<&str, Color> = self.task_ids
        .iter()
        .map(|id| (id.as_ref(), Color::White))
        .collect();
    let mut stack: Vec<&str> = Vec::new();

    fn dfs<'a>(
        node: &'a str,
        adjacency: &FxHashMap<Arc<str>, DepVec>,
        colors: &mut FxHashMap<&'a str, Color>,
        stack: &mut Vec<&'a str>,
    ) -> Result<(), String> {
        colors.insert(node, Color::Gray);
        stack.push(node);

        if let Some(neighbors) = adjacency.get(node) {
            for neighbor in neighbors {
                let n = neighbor.as_ref();
                match colors.get(n) {
                    Some(Color::Gray) => {
                        // Found cycle - build path from stack
                        let cycle_start = stack.iter().position(|&x| x == n).unwrap();
                        let cycle: Vec<&str> = stack[cycle_start..].to_vec();
                        return Err(format!("{} → {}", cycle.join(" → "), n));
                    }
                    Some(Color::White) | None => {
                        dfs(n, adjacency, colors, stack)?;
                    }
                    Some(Color::Black) => {} // Already processed
                }
            }
        }

        stack.pop();
        colors.insert(node, Color::Black);
        Ok(())
    }

    for task_id in &self.task_ids {
        if colors.get(task_id.as_ref()) == Some(&Color::White) {
            if let Err(path) = dfs(task_id.as_ref(), &self.adjacency, &mut colors, &mut stack) {
                return Err(NikaError::DagCycle { path });
            }
        }
    }

    Ok(())
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test test_detect_cycle -v`
Expected: PASS (both tests)

**Step 6: Commit**

```bash
git add src/dag/flow.rs src/error.rs
git commit -m "feat(dag): add cycle detection with DFS three-color algorithm (MVP 0)"
```

---

## Task 7: DAG Validation Tests

**Files:**
- Modify: `src/dag/flow.rs`
- Create: `tests/dag_test.rs`

**Step 1: Write comprehensive DAG tests**

```rust
// tests/dag_test.rs
use nika::dag::FlowGraph;
use nika::ast::Workflow;

#[test]
fn test_dag_diamond_no_cycle() {
    // Diamond: A → B, A → C, B → D, C → D
    let yaml = r#"
schema: nika/workflow@0.1
id: diamond
tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"
  - id: c
    infer: "C"
  - id: d
    infer: "D"
flows:
  - source: a
    target: [b, c]
  - source: [b, c]
    target: d
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(graph.get_final_tasks().len(), 1);
    assert!(graph.has_path("a", "d"));
}

#[test]
fn test_dag_self_loop() {
    // A → A (self-loop = cycle)
    let yaml = r#"
schema: nika/workflow@0.1
id: self_loop
tasks:
  - id: a
    infer: "A"
flows:
  - source: a
    target: a
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    let result = graph.detect_cycles();
    assert!(result.is_err());
}

#[test]
fn test_dag_disconnected_valid() {
    // A → B, C → D (two disconnected chains, no cycle)
    let yaml = r#"
schema: nika/workflow@0.1
id: disconnected
tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"
  - id: c
    infer: "C"
  - id: d
    infer: "D"
flows:
  - source: a
    target: b
  - source: c
    target: d
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let graph = FlowGraph::from_workflow(&workflow);

    assert!(graph.detect_cycles().is_ok());
    assert_eq!(graph.get_final_tasks().len(), 2); // b and d
}
```

**Step 2: Run tests**

Run: `cargo test dag_test -v`
Expected: PASS

**Step 3: Commit**

```bash
git add tests/dag_test.rs
git commit -m "test(dag): add comprehensive DAG validation tests"
```

---

## Task 8: Windows CI Target

**Files:**
- Modify: `.github/workflows/release.yml`

**Step 1: Audit current release workflow**

Run: `cat .github/workflows/release.yml | grep -A5 "matrix:"`
Expected: See current build targets (likely macOS + Linux only)

**Step 2: Add Windows target to matrix**

```yaml
# In .github/workflows/release.yml, update matrix:
strategy:
  matrix:
    include:
      - os: ubuntu-latest
        target: x86_64-unknown-linux-gnu
        artifact: nika-linux-x64
      - os: macos-latest
        target: x86_64-apple-darwin
        artifact: nika-macos-x64
      - os: macos-latest
        target: aarch64-apple-darwin
        artifact: nika-macos-arm64
      - os: windows-latest
        target: x86_64-pc-windows-msvc
        artifact: nika-windows-x64
```

**Step 3: Add SHA256 checksum generation**

```yaml
# After build step, add:
- name: Generate checksums
  run: |
    cd target/${{ matrix.target }}/release
    sha256sum nika* > checksums.sha256 || shasum -a 256 nika* > checksums.sha256
```

**Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add Windows target and SHA256 checksums"
```

---

## Deferred to Future MVPs

| Feature | MVP | Reason |
|---------|-----|--------|
| OpenAI tool calling | MVP 5 | Requires API key, not blocking |
| for_each parallelism | MVP 6 | Already parsed, execution pending |
| Production hardening | MVP 5 | Rate limiting, circuit breaker |
| Multi-crate workspace | MVP 6 | Architectural change |
