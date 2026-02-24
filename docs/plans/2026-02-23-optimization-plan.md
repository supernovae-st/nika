# Nika v0.8.1 Optimization Plan

**Date:** 2026-02-23
**Status:** In Progress
**Found By:** Rust Performance + Code Review Agents

## Summary

11 optimization opportunities identified across TUI, runtime, and MCP modules.
Prioritized by impact and effort.

---

## HIGH Priority (Performance/Reliability Impact)

### Fix #1: state.rs Cache Clone Optimization

**File:** `src/tui/state.rs:1513-1515`
**Issue:** `cached.clone()` called on every cache hit in `get_or_format()`.
**Impact:** Unnecessary string allocations in hot render path.

```rust
// BEFORE
pub fn get_or_format<T: serde::Serialize>(&mut self, key: &str, value: &T) -> String {
    if let Some(cached) = self.cache.get(key) {
        return cached.clone();  // ❌ Clone on every hit
    }
    // ...
}

// AFTER - Return &str and let caller clone if needed
pub fn get_or_format<T: serde::Serialize>(&mut self, key: &str, value: &T) -> &str {
    if !self.cache.contains_key(key) {
        // ... format and insert
    }
    self.cache.get(key).map(|s| s.as_str()).unwrap_or("")
}
```

**Alternative:** Keep String return but use `entry()` API to avoid double lookup.

---

### Fix #2: rig_agent_loop.rs Vec Reuse in Streaming

**File:** `src/runtime/rig_agent_loop.rs:785-786`
**Issue:** `Vec::new()` allocated every streaming turn for `response_parts` and `thinking_parts`.
**Impact:** Memory churn during streaming, ~150µs per turn.

```rust
// BEFORE (in stream_completion_with_tokens)
let mut response_parts: Vec<String> = Vec::new();  // ❌ New allocation
let mut thinking_parts: Vec<String> = Vec::new();  // ❌ New allocation

// AFTER - Store buffers in struct
pub struct RigAgentLoop {
    // ... existing fields
    response_buffer: Vec<String>,  // Reusable buffer
    thinking_buffer: Vec<String>,  // Reusable buffer
}

// Then in method:
self.response_buffer.clear();
self.thinking_buffer.clear();
```

---

### Fix #3: provider/rig.rs Streaming Timeout

**File:** `src/runtime/rig_agent_loop.rs:790`
**Issue:** `stream.next().await` has no timeout protection.
**Impact:** Potential infinite hang if LLM provider stalls.

```rust
// BEFORE
while let Some(chunk_result) = stream.next().await {  // ❌ No timeout
    // ...
}

// AFTER - Use tokio::time::timeout per chunk
use tokio::time::{timeout, Duration};

const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(60);

while let Some(chunk_result) = timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await? {
    // ...
}
```

---

### Fix #4: executor.rs Token Tracking Metadata

**File:** `src/runtime/executor.rs:~380`
**Issue:** Token tracking returns 0 in some execution paths.
**Impact:** Incomplete observability for token usage.

**Research needed:** Trace all paths where `AgentTurnMetadata.input_tokens` / `output_tokens` are set.
Use streaming API consistently when no tools present.

---

## MEDIUM Priority (Code Quality)

### Fix #5: app.rs Vec Reuse in poll_runtime_events

**File:** `src/tui/app.rs:631`
**Issue:** `Vec::new()` allocated on every frame (60 FPS = 60 allocations/sec).
**Impact:** Minor but measurable allocation pressure.

```rust
// BEFORE
fn poll_runtime_events(&mut self) {
    let mut events: Vec<crate::event::Event> = Vec::new();  // ❌ New allocation
    // ...
}

// AFTER - Store in struct
pub struct App {
    // ... existing fields
    event_buffer: Vec<crate::event::Event>,  // Reusable
}

fn poll_runtime_events(&mut self) {
    self.event_buffer.clear();
    // ... use self.event_buffer
}
```

---

### Fix #6: state.rs Format Indicator Caching

**File:** `src/tui/state.rs` (various)
**Issue:** Provider name / model string formatted on every render.
**Impact:** String allocations in hot render path.

```rust
// BEFORE
fn render_provider(&self) -> String {
    format!("🧠 {} | {}", self.provider_name, self.model_name)  // ❌ Every frame
}

// AFTER - Cache the formatted string
pub struct TuiState {
    cached_provider_indicator: Option<String>,
}

fn get_provider_indicator(&mut self) -> &str {
    if self.cached_provider_indicator.is_none() || self.provider_changed {
        self.cached_provider_indicator = Some(format!("🧠 {} | {}", ...));
        self.provider_changed = false;
    }
    self.cached_provider_indicator.as_ref().unwrap()
}
```

---

### Fix #7: executor.rs Decompose Error Handling

**File:** `src/runtime/executor.rs:370-402`
**Issue:** Multiple `Ok(nodes.clone())` calls - cloning arrays unnecessarily.
**Impact:** Memory copies for large node arrays.

```rust
// BEFORE
fn extract_decompose_nodes(&self, result: &Value) -> Result<Vec<Value>, NikaError> {
    if let Some(nodes) = result.get("nodes").and_then(|v| v.as_array()) {
        return Ok(nodes.clone());  // ❌ Clone entire array
    }
    // ... similar patterns
}

// AFTER - Take ownership when possible, or use references
fn extract_decompose_nodes(&self, result: Value) -> Result<Vec<Value>, NikaError> {
    // Take ownership of the result, extract array directly
    if let Value::Object(mut map) = result {
        if let Some(Value::Array(nodes)) = map.remove("nodes") {
            return Ok(nodes);  // ✅ No clone
        }
    }
    // ...
}
```

---

### Fix #8: mcp/types.rs From to TryFrom

**File:** `src/mcp/types.rs:130-134`
**Issue:** `From<i32>` impl for `McpErrorCode` cannot express failure.
**Impact:** Silent handling of unknown error codes.

**Analysis:** Current implementation is actually correct - `from_code()` handles all cases
including `Unknown(i32)`. The `serde(try_from = "i32")` works with `From` via blanket impl.

**Recommendation:** LOW priority - current code is correct, but could add explicit `TryFrom`
for documentation clarity.

---

## LOW Priority (Cleanup)

### Fix #9: Remove Dead Code

**Files:** Various
**Issue:** Some `#[allow(dead_code)]` markers on unused functions.
**Action:** Audit and either use or remove.

---

### Fix #10: Optimize String Formatting

**Files:** Various TUI widgets
**Issue:** `format!()` in render loops.
**Action:** Use `write!()` to pre-allocated buffers where possible.

---

### Fix #11: Use Cow<str> for Mixed Ownership

**Files:** `src/tui/state.rs`, `src/tui/views/*.rs`
**Issue:** Mixed owned/borrowed strings use `String` everywhere.
**Action:** Use `Cow<'static, str>` for static strings, `Cow<'_, str>` for borrowed.

---

## Execution Plan

| Phase | Fixes | Effort | Impact |
|-------|-------|--------|--------|
| 1 | #2, #3, #5 | 2h | HIGH (streaming stability) |
| 2 | #1, #6 | 1h | MEDIUM (render perf) |
| 3 | #4, #7 | 2h | MEDIUM (observability) |
| 4 | #8, #9, #10, #11 | 1h | LOW (cleanup) |

**Total Effort:** ~6 hours

---

## Verification

After each fix:
1. `cargo test` - All 1,902 tests pass
2. `cargo clippy -- -D warnings` - Zero warnings
3. `cargo bench` (if benchmark exists) - No regression

---

## References

- Context7 Tokio docs: `timeout()` and `select!` patterns
- Context7 rig-core docs: `StreamedAssistantContent` handling
- PERFORMANCE.md rules: Render path constraints
