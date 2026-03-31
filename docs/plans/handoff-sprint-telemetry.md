# Handoff: Telemetry Sprint (~4h)

> Copy this file as first message in a new Claude Code session.

## Context
5 new events added in overnight session. 2 more planned events remain, plus event infrastructure improvements.

## Codebase
```
cd /Users/thibaut/dev/supernovae/nika
cargo test --workspace --lib  # 9057 pass
```

## Already Done (overnight session)
- ForEachItemStarted/Completed/Failed — wired in runner.rs for_each loop
- TaskCancelled — distinct from TaskFailed
- FallbackChainExhausted — emitted before error return

## Items

### 1. StructuredOutputTimeout event (~1h)
**File:** `nika-engine/src/runtime/structured_output.rs:315-329`
**Bug:** When 600s aggregate timeout fires, only `StructuredOutputAllLayersFailed` error is returned. No dedicated event.
**Fix:** Before returning the timeout error, emit `EventKind::StructuredOutputTimeout { task_id, timeout_secs: 600, current_layer: ... }`.

### 2. MCP reconnection event (~1h)
**File:** `nika-engine/src/runtime/executor/invoke.rs`
**Bug:** When MCP connection fails and retries succeed, there's no event trail.
**Fix:** Add `EventKind::McpReconnected { server, attempt, duration_ms }` and emit after successful retry.

### 3. 429 Retry-After header support (~2h)
**File:** `nika-engine/src/runtime/executor/fetch.rs:417`
**Bug:** When a provider returns 429 with `Retry-After: 30`, nika uses exponential backoff instead of the server's requested delay.
**Fix:** Parse `Retry-After` header (seconds or HTTP-date format). Use max(retry_after, calculated_backoff) for the delay.

### 4. FetchFailed event on 5xx exhaustion (~15min)
**File:** `nika-engine/src/runtime/executor/fetch.rs:466`
**Bug:** When all retry attempts fail on 5xx, only `Err` is returned. No `FetchFailed` event for observability.
**Fix:** Emit `EventKind::FetchExhausted` before returning the error (check if it already exists — it may have been added).

## Verification
```bash
cargo test --workspace --lib
# Run a workflow with structured output to verify events:
./tools/target/debug/nika run tests/e2e-overnight/A01-basic-structured.nika.yaml --no-live
```
