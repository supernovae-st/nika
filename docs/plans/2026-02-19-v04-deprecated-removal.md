# Plan: v0.4 Deprecated Code Removal

**Date:** 2026-02-19
**Status:** In Progress
**Goal:** Remove deprecated providers and AgentLoop, complete RigAgentLoop migration

---

## Current State Analysis

### Files to Modify
| File | Change |
|------|--------|
| `src/runtime/rig_agent_loop.rs` | Complete `run_claude()` - add tools, implement chat |
| `src/runtime/executor.rs` | Update `execute_agent()` to use `RigAgentLoop` |
| `src/runtime/mod.rs` | Remove `AgentLoop` exports |
| `src/provider/mod.rs` | Remove deprecated exports, clean up `Provider` trait |

### Files to Delete
| File | Lines | Reason |
|------|-------|--------|
| `src/provider/claude.rs` | 335 | Deprecated - use `RigProvider::claude()` |
| `src/provider/openai.rs` | 339 | Deprecated - use `RigProvider::openai()` |
| `src/provider/types.rs` | ~150 | Deprecated - use rig-core types |
| `src/runtime/agent_loop.rs` | 717 | Deprecated - use `RigAgentLoop` |

### Tests to Update
| Test File | Issue | Solution |
|-----------|-------|----------|
| `tests/agent_loop_test.rs` | Uses `AgentLoop` | Update to use `RigAgentLoop` |
| `tests/agent_edge_cases_test.rs` | Uses `is_retryable_provider_error` | Move helper or remove tests |

---

## Execution Plan

### Phase 1: Complete RigAgentLoop.run_claude()

**Problem:** Current `run_claude()` returns mock data, doesn't actually:
1. Add tools to `AgentBuilder`
2. Call `agent.chat()` for execution
3. Parse response and handle multi-turn

**Solution:** Use rig's `AgentBuilder.tool()` to add each `NikaMcpTool`, then call `agent.chat()`.

```rust
// Target implementation
pub async fn run_claude(&mut self) -> Result<RigAgentLoopResult, NikaError> {
    let client = anthropic::Client::from_env();
    let model = client.completion_model(CLAUDE_3_5_SONNET);

    let mut builder = AgentBuilder::new(model)
        .preamble(&self.params.prompt);

    // Add tools (drain ownership since we consume them)
    for tool in self.tools.drain(..) {
        builder = builder.tool(tool);
    }

    let agent = builder.build();

    // Run chat
    let response = agent.chat(&self.params.prompt, vec![]).await
        .map_err(|e| NikaError::AgentLoopError { ... })?;

    // Parse response...
}
```

### Phase 2: Update execute_agent()

Change from:
```rust
let agent_loop = AgentLoop::new(...)?;
let result = agent_loop.run(provider).await?;
```

To:
```rust
let mut agent_loop = RigAgentLoop::new(...)?;
let result = agent_loop.run_claude().await?;
```

### Phase 3: Remove Deprecated Code

1. Delete `src/provider/claude.rs`
2. Delete `src/provider/openai.rs`
3. Delete `src/provider/types.rs`
4. Delete `src/runtime/agent_loop.rs`
5. Update `mod.rs` files to remove exports
6. Remove `Provider` trait if no longer needed (keep `MockProvider` for testing)

### Phase 4: Update Tests

- Migrate `agent_loop_test.rs` tests to use `RigAgentLoop`
- Move or inline `is_retryable_provider_error` if needed for tests
- Run full test suite

---

## Critical Insight

The `tools` field is `Vec<Box<dyn rig::tool::ToolDyn>>` - these are trait objects that can only be moved once.
`RigAgentLoop` must take ownership of tools to pass them to `AgentBuilder.tool()`.

Options:
1. Make `run_claude(&mut self)` and drain tools (one-time use)
2. Clone tools (if `ToolDyn: Clone`)
3. Use `Arc<dyn ToolDyn>` for shared ownership

**Decision:** Option 1 - `&mut self` with `drain()`. Agent loops are single-use anyway.

---

## Verification

After each phase:
```bash
cargo check   # Compilation
cargo test    # All tests
```

Final:
```bash
cargo nextest run  # Full test suite
```
