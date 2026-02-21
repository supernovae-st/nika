# Implementation Plan: Token Counts, MCP Caching, Provider Overrides

**Date:** 2026-02-21
**Author:** Claude + Thibaut
**Status:** In Progress

## Overview

Three features to implement from existing TODOs:

1. **Token counts in executor** - Extract actual token usage from rig-core streaming
2. **MCP response caching** - Cache MCP tool responses with TTL
3. **Provider/model overrides in ChatAgent** - CLI args for provider selection

## Feature 1: Token Counts in Executor

### Current State

```rust
// src/runtime/executor.rs:502-510
self.event_log.emit(EventKind::ProviderResponded {
    input_tokens: 0,  // TODO: Get from provider response
    output_tokens: 0, // TODO: Get from provider response
    // ...
});
```

### Solution

rig-core's `StreamedAssistantContent::Final(R)` contains the response with token usage.
The response type `R` implements `GetTokenUsage` trait:

```rust
// rig-core trait
pub trait GetTokenUsage {
    fn token_usage(&self) -> Option<Usage>;
}
```

### Changes Required

1. **`src/provider/rig.rs`** - Modify `infer_stream()` to return token counts:
   - Capture `Final(response)` chunk
   - Extract via `response.token_usage()`
   - Return `InferResult` with actual token counts

2. **`src/runtime/executor.rs`** - Use actual token counts from `InferResult`

### Files

| File | Change |
|------|--------|
| `src/provider/rig.rs` | Extract tokens from `Final` chunk |
| `src/provider/mod.rs` | Add token fields to `InferResult` |
| `src/runtime/executor.rs` | Use `InferResult.tokens` |

---

## Feature 2: MCP Response Caching

### Current State

```rust
// src/runtime/executor.rs:714
cached: false, // TODO: Implement MCP response caching
```

### Solution

Add caching layer in `McpClient` with:
- DashMap for thread-safe cache
- TTL-based expiration
- Cache key = `server:tool:params_hash`

### Changes Required

1. **`src/mcp/client.rs`** - Add cache to `McpClient`:
   ```rust
   struct CacheEntry {
       result: JsonValue,
       created_at: Instant,
   }

   cache: DashMap<String, CacheEntry>,
   cache_ttl: Duration, // default 5 minutes
   ```

2. **`src/mcp/client.rs`** - Check cache before calling tool:
   - Hash params for cache key
   - Return cached if valid
   - Store result after call

3. **`src/runtime/executor.rs`** - Set `cached: true` when cache hit

### Files

| File | Change |
|------|--------|
| `src/mcp/client.rs` | Add cache layer |
| `src/runtime/executor.rs` | Set cached flag |

---

## Feature 3: Provider/Model Overrides in ChatAgent

### Current State

```rust
// src/tui/mod.rs:189
// TODO: Apply provider/model overrides to ChatAgent when implemented
// For now, ChatAgent uses environment variables
```

### Solution

Add CLI arguments and propagate to ChatAgent:

```bash
nika chat --provider claude --model claude-sonnet-4-20250514
nika chat --provider openai --model gpt-4o
```

### Changes Required

1. **`src/main.rs`** - Add CLI args:
   ```rust
   #[derive(Args)]
   struct ChatArgs {
       #[arg(long)]
       provider: Option<String>,
       #[arg(long)]
       model: Option<String>,
   }
   ```

2. **`src/tui/chat_agent.rs`** - Accept provider/model overrides:
   ```rust
   impl ChatAgent {
       pub fn with_provider(mut self, provider: &str) -> Self { ... }
       pub fn with_model(mut self, model: &str) -> Self { ... }
   }
   ```

3. **`src/tui/mod.rs`** - Pass overrides to ChatAgent

### Files

| File | Change |
|------|--------|
| `src/main.rs` | Add CLI args |
| `src/tui/chat_agent.rs` | Accept overrides |
| `src/tui/mod.rs` | Pass overrides |

---

## Implementation Order

1. **Token counts** (most value, touches core provider)
2. **Provider overrides** (user-facing DX improvement)
3. **MCP caching** (optimization, can defer)

## Testing Strategy

- Unit tests for each feature
- Integration tests with mock provider
- Manual verification with real API calls
