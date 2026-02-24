# Nika v0.8.5 Timeout Fixes - Implementation Plan

> **Date:** 2026-02-24
> **Status:** Ready for Implementation
> **Priority:** HIGH - Prevents hanging operations

---

## Executive Summary

This plan addresses **5 unprotected async operations** that can hang indefinitely, causing infinite timers in the Activity panel and stuck TaskBox states.

```
+===============================================================================+
|  TIMEOUT FIXES ROADMAP                                                        |
+===============================================================================+
|                                                                               |
|  Phase 1: Provider Streaming (6 methods)           ~2 hours                   |
|  ├── Add per-chunk timeout to rig.rs streaming loops                          |
|  └── All 6 providers: Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama         |
|                                                                               |
|  Phase 2: MCP Connection Init (2 locations)        ~1 hour                    |
|  ├── Wrap get_or_try_init() with timeout in app.rs                            |
|  └── Add MCP_INIT_TIMEOUT constant (45s)                                      |
|                                                                               |
|  Phase 3: Agent Loop Streaming (2 methods)         ~1 hour                    |
|  ├── stream_with_tools_streaming() line 1025                                  |
|  └── run_claude_with_thinking() line 1358                                     |
|                                                                               |
|  Phase 4: Event Channel Protection                 ~30 min                    |
|  └── event_rx.recv() in agent relay task (line 2852)                          |
|                                                                               |
+===============================================================================+
```

---

## Phase 1: Provider Streaming Per-Chunk Timeout

### Problem

All 6 providers in `rig.rs` use `while let Some(chunk) = stream.next().await` without timeout. If the LLM API stalls, the stream hangs forever.

### Files to Modify

| File | Lines | Method |
|------|-------|--------|
| `src/provider/rig.rs` | 597-637 | Claude streaming |
| `src/provider/rig.rs` | 638-671 | OpenAI streaming |
| `src/provider/rig.rs` | 673-705 | Mistral streaming |
| `src/provider/rig.rs` | 706-738 | Groq streaming |
| `src/provider/rig.rs` | 739-771 | DeepSeek streaming |
| `src/provider/rig.rs` | 772-804 | Ollama streaming |

### Reference Pattern (from rig_agent_loop.rs:810-821)

```rust
use tokio::time::timeout;
use crate::util::STREAM_CHUNK_TIMEOUT;

loop {
    let chunk_result = match timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
        Ok(Some(result)) => result,
        Ok(None) => break, // Stream ended normally
        Err(_elapsed) => {
            let _ = tx.try_send(StreamChunk::Error(format!(
                "Stream timeout: no chunk received for {}s",
                STREAM_CHUNK_TIMEOUT.as_secs()
            )));
            return Err(RigInferError::Timeout {
                duration_ms: STREAM_CHUNK_TIMEOUT.as_millis() as u64,
            });
        }
    };

    match chunk_result {
        Ok(content) => { /* existing handling */ },
        Err(e) => {
            let _ = tx.try_send(StreamChunk::Error(e.to_string()));
            return Err(RigInferError::PromptError(e.to_string()));
        }
    }
}
```

### Implementation Steps

#### Step 1.1: Add Timeout Variant to RigInferError

**File:** `src/provider/rig.rs` (line ~50)

```rust
#[derive(Debug, thiserror::Error)]
pub enum RigInferError {
    #[error("Completion error: {0}")]
    PromptError(String),

    // NEW: Add timeout variant
    #[error("Stream timeout: no chunk received for {duration_ms}ms")]
    Timeout { duration_ms: u64 },
}
```

#### Step 1.2: Add Imports

**File:** `src/provider/rig.rs` (top of file)

```rust
use tokio::time::timeout;
use crate::util::STREAM_CHUNK_TIMEOUT;
```

#### Step 1.3: Update Each Provider Streaming Loop

**Pattern to replace:**
```rust
// BEFORE (vulnerable)
while let Some(chunk_result) = stream.next().await {
    match chunk_result { ... }
}
```

**Replace with:**
```rust
// AFTER (protected)
loop {
    let chunk_result = match timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
        Ok(Some(result)) => result,
        Ok(None) => break,
        Err(_elapsed) => {
            let _ = tx.try_send(StreamChunk::Error(format!(
                "Stream timeout: no chunk received for {}s",
                STREAM_CHUNK_TIMEOUT.as_secs()
            )));
            return Err(RigInferError::Timeout {
                duration_ms: STREAM_CHUNK_TIMEOUT.as_millis() as u64,
            });
        }
    };

    match chunk_result {
        // ... existing match arms unchanged ...
    }
}
```

### Verification

```bash
# Run streaming tests
cargo test --test streaming_test -- --ignored

# Test with real API (Claude)
ANTHROPIC_API_KEY=sk-... cargo run -- chat
# Type a message and verify timeout behavior
```

---

## Phase 2: MCP Connection Init Timeout

### Problem

`OnceCell::get_or_try_init()` has no overall timeout. Individual operations have timeouts, but the entire init can take 40s+ with no user feedback.

### Files to Modify

| File | Lines | Location |
|------|-------|----------|
| `src/util/constants.rs` | 24 | Add MCP_INIT_TIMEOUT |
| `src/tui/app.rs` | 2574-2614 | handle_chat_invoke() |
| `src/tui/app.rs` | 2786-2826 | handle_chat_agent() |

### Implementation Steps

#### Step 2.1: Add Constant

**File:** `src/util/constants.rs` (after MCP_CALL_TIMEOUT)

```rust
/// Timeout for MCP tool calls (invoke: verb)
pub const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for complete MCP server initialization (connect + list_tools + overhead)
/// Prevents hanging on slow/unresponsive MCP servers during startup.
/// Should be > CONNECT_TIMEOUT + MCP_CALL_TIMEOUT to allow sequential operations.
pub const MCP_INIT_TIMEOUT: Duration = Duration::from_secs(45);
```

#### Step 2.2: Update Import in app.rs

```rust
use crate::util::constants::{
    EXEC_TIMEOUT, FETCH_TIMEOUT, INFER_TIMEOUT, WORKFLOW_TIMEOUT,
    MCP_INIT_TIMEOUT,  // NEW
};
```

#### Step 2.3: Wrap get_or_try_init() - Location 1

**File:** `src/tui/app.rs` (lines 2574-2614)

```rust
// BEFORE
match cell.get_or_try_init(|| async { ... }).await { ... }

// AFTER
match timeout(MCP_INIT_TIMEOUT, cell.get_or_try_init(|| async { ... })).await {
    Ok(Ok(c)) => {
        let _ = status_tx.send(StreamChunk::McpConnected(server_name_clone.clone())).await;
        Arc::clone(c)
    }
    Ok(Err(e)) => {
        // Init error (connection failed, config issue, etc.)
        let _ = status_tx.send(StreamChunk::McpError {
            server_name: server_name_clone.clone(),
            error: e.to_string(),
        }).await;
        let _ = tx.send(format!("❌ MCP init failed: {}", e)).await;
        return;
    }
    Err(_elapsed) => {
        // Timeout error
        let error_msg = format!(
            "MCP server '{}' initialization timed out after {}s. Server may be slow or unresponsive.",
            server_name_clone, MCP_INIT_TIMEOUT.as_secs()
        );
        let _ = status_tx.send(StreamChunk::McpError {
            server_name: server_name_clone.clone(),
            error: error_msg.clone(),
        }).await;
        let _ = tx.send(format!("❌ {}", error_msg)).await;
        return;
    }
}
```

#### Step 2.4: Wrap get_or_try_init() - Location 2

Apply the **same pattern** to `handle_chat_agent()` (lines 2786-2826).

### Verification

```bash
# Test with slow/non-existent MCP server
cargo run -- chat
# Type: /invoke slow_server some_tool
# Should timeout after 45s with clear error message
```

---

## Phase 3: Agent Loop Streaming Timeout

### Problem

Two methods in `rig_agent_loop.rs` have unprotected `stream.next().await` that can hang forever.

### Files to Modify

| File | Lines | Method |
|------|-------|--------|
| `src/runtime/rig_agent_loop.rs` | 1025 | stream_with_tools_streaming() |
| `src/runtime/rig_agent_loop.rs` | 1358 | run_claude_with_thinking() |

### Implementation Steps

#### Step 3.1: Fix stream_with_tools_streaming()

**File:** `src/runtime/rig_agent_loop.rs` (line 1025)

```rust
// BEFORE
while let Some(chunk) = stream.next().await {
    match chunk { ... }
}

// AFTER
loop {
    let chunk = match timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
        Ok(Some(result)) => result,
        Ok(None) => break, // Stream ended normally
        Err(_elapsed) => {
            tracing::warn!("Agent stream timed out after {}s", STREAM_CHUNK_TIMEOUT.as_secs());
            return Err(NikaError::Timeout {
                operation: "agent streaming".to_string(),
                duration_ms: STREAM_CHUNK_TIMEOUT.as_millis() as u64,
            });
        }
    };

    match chunk {
        // ... existing match arms unchanged ...
    }
}
```

#### Step 3.2: Fix run_claude_with_thinking()

**File:** `src/runtime/rig_agent_loop.rs` (line 1358)

Apply the **same pattern** as Step 3.1.

### Verification

```bash
# Run unit tests
cargo test rig_agent_loop

# Run agent with slow provider (simulate timeout)
cargo run -- chat
# Type: /agent "Complex research task"
# Verify timeout behavior after STREAM_CHUNK_TIMEOUT (60s)
```

---

## Phase 4: Event Channel Protection

### Problem

`event_rx.recv().await` in the agent relay task (line 2852) can hang if the agent crashes without dropping the channel.

### Files to Modify

| File | Lines | Location |
|------|-------|----------|
| `src/tui/app.rs` | 2852 | Agent event relay task |

### Implementation

**Option A: Use timeout per receive (Recommended)**

```rust
// BEFORE
while let Ok(event) = event_rx.recv().await {
    // Process event
}

// AFTER
loop {
    match timeout(Duration::from_secs(5), event_rx.recv()).await {
        Ok(Ok(event)) => {
            // Process event
        }
        Ok(Err(_)) => {
            // Channel closed (agent completed)
            break;
        }
        Err(_elapsed) => {
            // Timeout - check if parent task is still alive
            // If agent task is done, break. Otherwise continue waiting.
            // This prevents infinite hang on orphaned channels.
            if agent_handle.is_finished() {
                tracing::debug!("Agent task finished, closing event relay");
                break;
            }
        }
    }
}
```

**Option B: Use select! with cancellation token**

```rust
use tokio::select;
use tokio_util::sync::CancellationToken;

let token = CancellationToken::new();

loop {
    select! {
        event = event_rx.recv() => {
            match event {
                Ok(e) => { /* process */ }
                Err(_) => break,
            }
        }
        _ = token.cancelled() => {
            break;
        }
    }
}
```

### Verification

```bash
# Run agent tests
cargo test handle_chat_agent

# Manual test: Start agent, kill it mid-execution
# Verify event relay task terminates cleanly
```

---

## Testing Checklist

### Unit Tests

- [ ] `RigInferError::Timeout` variant serialization
- [ ] `timeout()` wrapper returns correct error type
- [ ] Streaming loop breaks correctly on timeout
- [ ] MCP init timeout error message is correct

### Integration Tests

- [ ] `cargo test --test streaming_test -- --ignored` (with API keys)
- [ ] MCP timeout with non-existent server
- [ ] Agent timeout with slow provider

### Manual Tests

- [ ] Start Chat, trigger long inference, verify timeout after 60s
- [ ] Configure slow MCP server, verify timeout after 45s
- [ ] Run /agent, kill network, verify timeout and clean recovery

---

## Constants Summary

| Constant | Value | Purpose | File |
|----------|-------|---------|------|
| `STREAM_CHUNK_TIMEOUT` | 60s | Per-chunk streaming timeout | constants.rs |
| `MCP_INIT_TIMEOUT` | 45s | MCP server init timeout | constants.rs (NEW) |
| `MCP_CALL_TIMEOUT` | 30s | MCP tool call timeout | constants.rs |
| `INFER_TIMEOUT` | 120s | Single inference timeout | constants.rs |
| `WORKFLOW_TIMEOUT` | 300s | Total workflow timeout | constants.rs |

---

## Risk Assessment

| Phase | Risk | Mitigation |
|-------|------|------------|
| 1 | Partial response on timeout | Send accumulated text before error |
| 2 | False timeout on slow servers | 45s is generous; add retry option later |
| 3 | Lost agent state | Already tracked via EventLog; timeout triggers cleanup |
| 4 | Orphaned relay tasks | Timeout + handle check ensures cleanup |

---

## Rollback Plan

If issues arise:
1. Revert to commit `622c093` (current fixes)
2. Increase timeout values if false positives
3. Add config option for custom timeouts

---

## Estimated Effort

| Phase | Effort | Complexity |
|-------|--------|------------|
| Phase 1 | 2 hours | Medium (6 similar changes) |
| Phase 2 | 1 hour | Low (2 locations, clear pattern) |
| Phase 3 | 1 hour | Medium (need to trace control flow) |
| Phase 4 | 30 min | Low (single location) |
| **Total** | **4.5 hours** | |

---

## Next Steps

1. Review this plan
2. Implement Phase 1 (most impactful)
3. Test with real API keys
4. Implement remaining phases
5. Run full test suite
6. Commit with detailed message
