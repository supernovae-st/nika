# v0.9.x Implementation Checklist

Quick reference for ensuring spec-code alignment during development.

---

## Pre-Implementation (MUST DO FIRST)

- [ ] Add dependencies to `Cargo.toml`:
  - [ ] `petgraph = "0.6"`
  - [ ] `humantime = "2.1"`
  - [ ] `evalexpr = "11.0"`

- [ ] Verify rig-core v0.31 API:
  - [ ] Check `rig::tool::ToolDyn` trait signature
  - [ ] Confirm `ToolDefinition` struct exists
  - [ ] Test `call()` method signature

- [ ] Thread safety review:
  - [ ] Confirm `EventLog` design for concurrent access
  - [ ] Confirm `DataStore` supports Arc<Mutex<>>
  - [ ] Document ChatWorkflow locking strategy

- [ ] Architecture decision:
  - [ ] Decide: Replace FlowGraph (breaking) vs create parallel impl
  - [ ] If replacing: Update all imports in dag/mod.rs
  - [ ] If parallel: Plan migration strategy for v0.10

- [ ] Update error codes in `src/error.rs`:
  - [ ] Add NIKA-260..265 (mention errors)
  - [ ] Add NIKA-200..299 (builtin tool errors)

---

## v0.9.0: StableGraph Foundation

**Files to Create:**
- [ ] `src/dag/stable.rs` OR refactor `src/dag/flow.rs`

**Files to Modify:**
- [ ] `src/dag/mod.rs` - Update exports
- [ ] `src/dag/validate.rs` - Verify compatibility

**Tests Required:**
- [ ] `cargo test dag::` - All DAG tests pass
- [ ] Cycle detection works with StableGraph
- [ ] Topological sort uses petgraph::algo::toposort()

**Live Test:**
- [ ] `cargo test dag::` passes
- [ ] No clippy warnings

---

## v0.9.1: ChatWorkflow Struct

**Files to Create:**
- [ ] `src/runtime/chat_workflow.rs` (300-400 LOC)

**Files to Modify:**
- [ ] `src/runtime/mod.rs` - Add `mod chat_workflow` and export
- [ ] `src/tui/views/chat.rs` - Update to use ChatWorkflow

**Tests Required:**
- [ ] `ChatWorkflow::new()` creates empty DAG
- [ ] `add_message()` creates node + returns NodeIndex
- [ ] Sequential edge creation works
- [ ] Message counter increments correctly
- [ ] Mutex protection verified

**Live Test:**
- [ ] `cargo run -- chat` creates ChatWorkflow
- [ ] Messages appear in DAG (verify with debug output)
- [ ] No panic on concurrent message creation

---

## v0.9.2: @mention Binding System

**Files to Create:**
- [ ] `src/binding/mention.rs` (400-500 LOC)

**Files to Modify:**
- [ ] `src/binding/mod.rs` - Add `mod mention` and export `MentionParser`, `Mention`
- [ ] `src/binding/entry.rs` - Verify WiringSpec structure
- [ ] `src/error.rs` - Verify NIKA-260..265 added

**Tests Required (Min 35):**
- [ ] `parse_mentions("@1")` → Some(vec![Mention::Number(1)])
- [ ] `parse_mentions("@last")` → Some(vec![Mention::Last])
- [ ] `parse_mentions("@all")` → Some(vec![Mention::All])
- [ ] `parse_mentions("@1..3")` → Some(vec![Mention::Range(1,3)])
- [ ] `parse_mentions("// text")` → parallel marker detected
- [ ] Regex doesn't match `user@example.com`
- [ ] @0 is valid (first message)
- [ ] Out of bounds → Error NIKA-264
- [ ] Deleted node → Error NIKA-261
- [ ] Self-reference → Error NIKA-263
- [ ] Invalid range (start > end) → Error NIKA-265
- [ ] Empty chat + @last → Error NIKA-260
- [ ] Empty chat + @all → Ok(vec![])
- [ ] mentions_to_wiring() produces valid WiringSpec
- [ ] ChatWorkflow integration: mentions create edges

**Live Test:**
- [ ] `cargo run -- chat`, type `@1 test` → no panic
- [ ] Mention resolution in prompts works
- [ ] Range @1..3 creates correct dependencies

---

## v0.9.3: Builtin Tools (6 nika:*)

**Files to Create:**
- [ ] `src/runtime/builtin/mod.rs` (50 LOC)
- [ ] `src/runtime/builtin/router.rs` (150 LOC)
- [ ] `src/runtime/builtin/sleep.rs` (80 LOC)
- [ ] `src/runtime/builtin/log.rs` (100 LOC)
- [ ] `src/runtime/builtin/assert.rs` (120 LOC)
- [ ] `src/runtime/builtin/emit.rs` (70 LOC)
- [ ] `src/runtime/builtin/prompt.rs` (200 LOC)
- [ ] `src/runtime/builtin/run.rs` (150 LOC)

**Files to Modify:**
- [ ] `src/runtime/mod.rs` - Add `pub mod builtin` and exports
- [ ] `src/runtime/executor.rs` - Integrate router into task dispatch
- [ ] `src/runtime/rig_agent_loop.rs` - Add builtin tools to agent
- [ ] `src/error.rs` - Verify NIKA-200..299 added

**BuiltinTool Trait:**
```rust
pub trait BuiltinTool: ToolDyn {
    fn event_log(&self) -> &EventLog;
    fn data_store(&self) -> &DataStore;
    fn category(&self) -> BuiltinCategory { BuiltinCategory::ControlFlow }
}
```

**Router Behavior:**
- `invoke: { tool: "nika:sleep", ... }` → BuiltinToolRouter::dispatch()
- `invoke: { tool: "novanet:describe", ... }` → McpClient::call_tool()
- No prefix → Error (ambiguous)

**Tests Required (Min 45):**
- [ ] BuiltinToolRouter can dispatch all 6 tools
- [ ] nika:sleep parses duration ("5s", "1m" via humantime)
- [ ] nika:log writes to EventLog
- [ ] nika:emit creates custom events
- [ ] nika:assert evaluates expressions (via evalexpr)
- [ ] nika:prompt pauses execution (TUI integration)
- [ ] nika:run executes nested workflow
- [ ] Prefix detection works (@1 vs nika:sleep vs novanet:tool)
- [ ] Error handling for invalid prefix
- [ ] Tool definitions are correct (for rig::ToolDyn)

**Live Test:**
- [ ] `cargo run -- run examples/test-builtin-sleep.nika.yaml`
- [ ] `cargo run -- run examples/test-builtin-assert.nika.yaml`
- [ ] `/nika:sleep 1s` command in chat works (if integrated)

---

## v0.9.4: ChatDagPanel Widget

**Files to Create:**
- [ ] `src/tui/widgets/chat_dag_panel.rs` (500-600 LOC)
- [ ] `src/tui/widgets/node_box.rs` (200-250 LOC)
- [ ] `src/tui/widgets/edge_line.rs` (150-200 LOC)

**Files to Modify:**
- [ ] `src/tui/widgets/mod.rs` - Add exports
- [ ] `src/tui/views/chat.rs` - Add ChatDagPanel to layout
- [ ] `src/event/log.rs` - Verify subscription API exists

**Tests Required (Min 25):**
- [ ] ChatDagPanel renders nodes
- [ ] Edges drawn between connected nodes
- [ ] NodeBox shows task IDs correctly
- [ ] Layout algorithm positions nodes vertically
- [ ] Scroll sync: click node → scroll chat to message
- [ ] EventLog subscription triggers updates
- [ ] Real-time updates on new events

**Live Test:**
- [ ] `cargo run -- chat`, Ctrl+D toggles DAG panel
- [ ] DAG panel visible with 2+ messages
- [ ] Click node in DAG → message highlighted in chat
- [ ] New message creates new node (animation optional)

---

## v0.9.5: Polish & Export

**Files to Modify:**
- [ ] `src/tui/widgets/chat_dag_panel.rs` - Add animations
- [ ] `src/tui/views/chat.rs` - Add Ctrl+E export
- [ ] `src/event/log.rs` - Verify serialization
- [ ] Session persistence (existing)

**Tests Required (Min 18):**
- [ ] Node pulse animation
- [ ] Edge flow animation
- [ ] Ctrl+E triggers export
- [ ] DAG serializes to JSON
- [ ] DAG serializes to Mermaid
- [ ] Export produces valid YAML workflow
- [ ] Session persistence restores DAG state
- [ ] Restart chat → DAG restored

**Live Test:**
- [ ] `cargo run -- chat`, type messages, Ctrl+E → exports
- [ ] Exit/restart → DAG structure preserved
- [ ] Exported YAML can be run with `nika run exported.nika.yaml`

---

## WIRING Checkpoints (After Each Phase)

Run these integration tests:

**After v0.9.0:**
```bash
cargo test wiring_checkpoint_0  # FlowGraph → DAG validation
```

**After v0.9.1:**
```bash
cargo test wiring_checkpoint_1  # ChatWorkflow → FlowGraph
```

**After v0.9.2:**
```bash
cargo test wiring_checkpoint_2  # MentionParser → WiringSpec
```

**After v0.9.3:**
```bash
cargo test wiring_checkpoint_3  # BuiltinRouter → Executor
```

**After v0.9.4:**
```bash
cargo test wiring_checkpoint_4  # ChatDagPanel → EventLog
```

**After v0.9.5:**
```bash
cargo test wiring_checkpoint_5  # Session → DAG restore
```

---

## Quality Gates (Every Phase)

**Before committing:**
```bash
cargo test                              # All tests pass
cargo clippy -- -D warnings             # Zero warnings
cargo fmt -- --check                    # Format check
cargo build --release                   # Release build
```

**Before marking phase complete:**
```bash
cargo test dag::
cargo test mention_
cargo test builtin_
# (phase-specific)
```

---

## Import Verification Checklist

For each new module, verify imports are correct:

- [ ] No circular imports (new → existing only)
- [ ] All types imported from correct modules
- [ ] Arc<str>, FxHashMap, SmallVec used correctly
- [ ] async/await syntax where needed
- [ ] Error types use NikaError enum

**Common Import Pattern:**
```rust
// src/runtime/chat_workflow.rs
use crate::dag::FlowGraph;
use crate::event::EventLog;
use crate::store::DataStore;
use parking_lot::Mutex;
use petgraph::stable_graph::{StableGraph, NodeIndex};
```

---

## Export Verification Checklist

For each new module, update parent `mod.rs`:

```rust
// src/runtime/mod.rs (example)
pub mod builtin;
pub use builtin::{BuiltinToolRouter, BuiltinTool};
```

Verify exports in lib.rs if needed for public API.

---

## Git Workflow (Per Phase)

```bash
# Start phase
git checkout -b feature/v0.9.X-<name>

# Work (write tests first, then implementation)
# Commit frequently

# Quality gate
cargo test
cargo clippy -- -D warnings

# Merge to main
git checkout main
git merge feature/v0.9.X-<name>
git tag v0.9.X

# Next phase
git checkout -b feature/v0.9.<X+1>-<name>
```

---

## Critical Reminders

1. **TDD First** - Write failing test before implementation
2. **WIRING Checkpoints** - Don't skip integration tests
3. **Zero Warnings** - `cargo clippy -- -D warnings` must pass
4. **Thread Safety** - Use Mutex for shared state
5. **Error Codes** - Add to error.rs BEFORE using
6. **Module Exports** - Update mod.rs IMMEDIATELY after creating files
7. **Dependencies** - Verify petgraph/humantime/evalexpr added

---

**Last Updated:** 2026-02-25
**For:** Claude Code implementation team
