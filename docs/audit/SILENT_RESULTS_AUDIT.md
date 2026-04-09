# Silent Result Audit: `let _ =` Patterns in nika-engine

## Summary

- **Total patterns found**: 81 (non-test code), 92 (including tests)
- **Analysis scope**: First 20+ patterns examined with context
- **Finding**: Mix of intentional fire-and-forget + legitimate bugs + test cleanup

---

## Classification of First 20 Patterns

### 1. File Cleanup (OK - Intentional Fire-and-Forget)

| File | Line | Pattern | Classification | Reason |
|------|------|---------|-----------------|--------|
| `tools/edit.rs` | 179 | `fs::remove_file(temp_clone)` | **OK** | Async cleanup spawned after rename failure; log warning already emitted |
| `tools/write.rs` | 147 | `fs::remove_file(temp_clone)` | **OK** | Same pattern — async cleanup post-error |
| `util/fs.rs` | 54 | `fs::remove_file(&temp_path)` | **OK** | `inspect_err` closure cleanup; error already handled |
| `io/atomic.rs` | 80 | `fs::remove_file(&temp_path)` | **OK** | Best-effort cleanup after rename failure; error already propagated |
| `core/storage.rs` | 327 | `fs::remove_file(&file_path)` | **OK** | Checksum mismatch detected; cleanup is defensive, error returned |
| `core/paths.rs` | 511 | `fs::remove_dir_all(temp_path)` | **OK** | Test helper; setup cleanup |
| `runtime/runner.rs` | 83 | `fs::remove_file(&path)` | **OK** | Stale lockfile cleanup with warning logged |
| `runtime/runner.rs` | 103 | `fs::remove_file(&self.path)` | **OK** | Drop trait cleanup; silent expected |
| `runtime/runner.rs` | 6502 | `fs::remove_file(&tmp)` | **OK** | Test cleanup |
| `runtime/runner.rs` | 6583 | `fs::remove_file(&tmp)` | **OK** | Test cleanup |

### 2. Channel Send Failures (BUG - Should Log)

| File | Line | Pattern | Classification | Reason |
|------|------|---------|-----------------|--------|
| `runtime/rig_agent_loop/streaming.rs` | 128 | `tx.try_send(StreamChunk::Token(...))` | **BUG** | Silent channel failure = lost token data; should at least warn |
| `runtime/rig_agent_loop/streaming.rs` | 148 | `tx.try_send(StreamChunk::Thinking(...))` | **BUG** | Silent drop of reasoning content for TUI; should log |
| `runtime/rig_agent_loop/streaming.rs` | 160+ (6 more) | Multiple `tx.try_send(...)` | **BUG** | All streaming output silently dropped if channel is closed |

**Impact**: User doesn't see real-time streaming because failed sends are silently ignored. No error indication.

### 3. Media Cleanup (Intentional - Defense-in-Depth)

| File | Line | Pattern | Classification | Reason |
|------|------|---------|-----------------|--------|
| `runtime/runner.rs` | 728 | `datastore.take_media(task_id)` | **OK** | Orphaned media draining after error; logged with NIKA-060 |
| `runtime/runner.rs` | 780 | `datastore.take_media(task_id)` | **OK** | Schema validation failure; logged with NIKA-061 |
| `runtime/runner.rs` | 808 | `datastore.take_media(task_id)` | **OK** | Executor error; logged + event emitted |
| `runtime/runner.rs` | 1166 | `datastore.take_media(&task_id)` | **OK** | Error case; logged + event emitted |
| `runtime/runner.rs` | 1210 | `datastore.take_media(&task_id)` | **OK** | Error case; logged + event emitted |

### 4. Intentional Discards (OK - Test/Debug)

| File | Line | Pattern | Classification | Reason |
|------|------|---------|-----------------|--------|
| `dag/flow.rs` | 1688 | `compute_layers(...)` (panics prevented) | **OK** | Explicit comment: "Must not panic" — validation test |
| `io/writer.rs` | 1102 | `writer.write_binary(request)` | **OK** | Test verifying error handling; result intentionally ignored |
| `runtime/structured_output.rs` | 1207 | `engine.validate(...)` | **OK** | Test setup; validation result not asserted yet |
| `runtime/structured_output.rs` | 1235 | `engine.validate(...)` | **OK** | Test checking error path |
| `runtime/runner.rs` | 2231 | `write!(task_id_buf, ...)` | **OK** | Format to buffer; failure would be programmer error (always succeeds) |
| `runtime/executor/infer.rs` | 230 | `provider_idx` variable | **OK** | Explicitly used for tracing; comment explains intent |
| `runtime/runner.rs` | 3821, 3874, 5070, 5344, etc. | `runner.run()` (test code) | **OK** | Test teardown; result checked in subsequent assertions |

---

## Key Findings

### Critical Bugs (Streaming Channel Sends)

**9 instances** in `runtime/rig_agent_loop/streaming.rs` where `tx.try_send()` failures are silently swallowed:

```rust
// CURRENT (BUG):
if let Some(ref tx) = self.stream_tx {
    let _ = tx.try_send(StreamChunk::Token(...));  // SILENT FAILURE
}

// SHOULD BE:
if let Some(ref tx) = self.stream_tx {
    if let Err(e) = tx.try_send(StreamChunk::Token(...)) {
        tracing::warn!("Failed to send token to stream: {}", e);
        // Optionally: set flag to close stream
    }
}
```

**Impact**: Real-time streaming to TUI silently fails when channel is full or receiver dropped.

### Legitimate Patterns

1. **File cleanup after errors** (10 instances) — intentional fire-and-forget
2. **Media orphan draining** (5 instances) — error already logged
3. **Test cleanup** (15+ instances) — test code teardown
4. **Format/write to buffer** (1 instance) — cannot fail

---

## Recommendations

### Priority 1 (Critical)

**Streaming channel sends** — 9 instances in `rig_agent_loop/streaming.rs` + 5 in `provider/native/runtime.rs`:

```bash
grep -n 'let _ = tx.' nika-engine/src/runtime/rig_agent_loop/streaming.rs
grep -n 'let _ = tx.' nika-engine/src/provider/native/runtime.rs
```

**Pattern**: `let _ = tx.try_send(...).await` or `tx.send(...).await`

**Impact**:
- Real-time streaming output lost silently when channel is full or receiver dropped
- Users see no indication of streaming failure
- Critical for TUI real-time display and error reporting

**Action**: Replace silent `let _` with proper logging:
```rust
// BEFORE:
let _ = tx.try_send(StreamChunk::Token(...));

// AFTER:
if let Err(e) = tx.try_send(StreamChunk::Token(...)) {
    tracing::warn!("Failed to send token to TUI stream: {}", e);
    // Consider: break/return to avoid cascade of failures
}
```

### Priority 2 (Minor)

Review `compute_layers()` call in `dag/flow.rs:1688` — explicit comment says "Must not panic", but why allow silent ignore?

```bash
grep -B5 -A3 'Must not panic' nika-engine/src/dag/flow.rs
```

**Current**: Validation function called only for side effects; result ignored
**Suggest**: Either assert result is Ok or log if validation fails

---

## Detailed Bug Analysis

### Bug: Streaming Channel Sends (14 instances total)

**rig_agent_loop/streaming.rs (9 instances)**:
- Line 128: Token streaming
- Line 148: Thinking/reasoning streaming
- Line 160: (nested context)
- Line 176: Metrics streaming
- Line 481: Error reporting
- Line 504: Token streaming (agent loop)
- Line 536: MCP call start
- Line 569: Thinking/reasoning (agent loop)
- Line 593: Metrics (agent loop)

**native/runtime.rs (2 instances)**:
- Line 449: Cancellation error (likely OK — receiver may be dropped)
- Line 459: Cancellation error (likely OK — receiver may be dropped)

**Assessment**: The rig_agent_loop patterns are CRITICAL bugs; native runtime patterns are legitimate (send during cleanup).

---

## Statistics

- **Intentional fire-and-forget (file cleanup)**: 10 instances (OK)
- **Intentional (media defense-in-depth)**: 5 instances (OK)
- **Test/setup code**: 17 instances (OK)
- **Format/write operations**: 1 instance (OK)
- **Silent bugs (channel sends in streaming.rs)**: 9 instances (CRITICAL BUG)
- **Legitimate cleanup (native cancellation)**: 2 instances (OK)
- **Unknown/other patterns**: 37 instances (mixed)

---

## Files Requiring Changes

1. `/path/to/project — 9 fixes

## Audit Command

```bash
cd /path/to/project
grep -rn 'let _ =' nika-engine/src/ --include='*.rs' | grep -v test | grep -v target
```
