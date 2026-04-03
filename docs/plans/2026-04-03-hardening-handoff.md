# Hardening Session Handoff — 2026-04-03

## Summary

Multi-round deep audit and fix session. 6 commits, ~30 files, 25+ bugs found, 23 fixed.

## Commits (chronological)

| Commit | Description | Files |
|--------|-------------|-------|
| `01f8468` | 8-bug hardening: M8 unsafe set_var, BUG-018/020, ANSI strip, stale refs, MCP_JSON | 15 |
| `19d3f9f` | 5 runtime: backoff clamp, checkpoint log, webhook timeout, guardrail log, infer retry | 5 |
| `9f4873b` | 3 serve/agent: resume_from validation, chat history cap, CLI stream timeout | 3 |
| `486b589` | TUI saturating_sub, storage LIMIT 10K, clippy drain_collect | 3 |
| `f172ab0` | Key redaction, turn_count, error codes NIKA-501+, #[must_use] | 2 |
| (pending) | Rate limiter GC, SlotGuard, daemon GetSecret auth | 5 |

## Fixes by Category

### CRITICAL (5)
1. **M8: unsafe set_var** — 9 production sites centralized into `inject_secret_to_env()` with safety contract
2. **BUG-018: for_each artifact** — Skip per-iteration write, rely on post-aggregation
3. **BUG-1: ANSI in API** — `strip_ansi_escapes()` in embedded executor
4. **BUG-020: NIKA-053 false positive** — 4 KiB scan limit for blocklist
5. **Backoff saturation** — Clamp [1.0, 10.0], cap delay at 5 min

### HIGH (7)
6. **resume_from injection** — UUID/alphanum validation in POST /v1/run
7. **CLI stream timeout** — Per-chunk STREAM_CHUNK_TIMEOUT added to CLI path
8. **Checkpoint save silent** — tracing::warn on storage error
9. **Webhook no timeout** — 10s connect + 30s total on both clients
10. **active_jobs leak** — SlotGuard RAII in handler scope
11. **Rate limiter growth** — `retain_recent()` in GC loop
12. **Daemon GetSecret auth** — auth_token required (defense in depth)

### MEDIUM (8)
13. AcceptEdits doc comment aligned with code
14. nika-napi version synced to 0.63.0
15. Stale TODO(v0.58) removed
16. MCP_JSON constant added, 4 literals replaced
17. API key redaction in provider errors
18. chat turn_count off-by-one (div_ceil)
19. nika-init error codes NIKA-501/502/503 (was reusing engine codes)
20. #[must_use] on SdkError
21. TUI McpCallBox saturating_sub
22. Storage history LIMIT 10K
23. Infer retry unwrap hardened

### Verified Already Fixed (7)
- BUG-019 (PartialSuccess `is_usable()`)
- events.rs unwrap (test-only)
- SSE CRLF (`find_frame_boundary`)
- SSE buffer cap (1 MiB)
- Python pythonize
- Python `__eq__`
- `SdkError::Cancelled`

### Verified False Positives (3)
- NIKA-034 exists at error.rs:1500
- WorkflowTimeout emitted at runner.rs:2979
- JoinSet abort_all already wired at runner.rs:2934

## Pending / Blocked

### Daemon GetSecret auth (code done, blocked by jaq dep)
Files modified but can't compile due to WIP jaq-syn dependency in nika-core from another session:
- `tools/nika-daemon/src/protocol.rs` — added `auth_token: Option<String>` to `GetSecret`
- `tools/nika-daemon/src/server.rs` — added `validate_auth_token` check
- `tools/nika-daemon/src/client.rs` — passes auth token from `read_auth_token()`

**To unblock:** Fix nika-core's `Cargo.toml` to use `jaq-parse`/`jaq-interpret` (workspace deps) instead of `jaq-syn`/`jaq-std` (not in workspace). The transform.rs code uses `jaq_interpret` which matches the workspace dep.

### Rate limiter / SlotGuard (code done, same compile block)
- `tools/nika-serve/src/lib.rs` — `gc_limiter.retain_recent()` in GC loop
- `tools/nika-serve/src/routes/workflows.rs` — `SlotGuard` RAII struct

## Verification Commands

```bash
# Full workspace check (after fixing jaq dep)
cd tools/nika && cargo check --workspace

# Run all tests (excluding PyO3 link issues)
cargo test --workspace --lib --exclude nika-py --exclude nika-napi

# Specific test suites for fixed areas
cargo test -p nika-engine --lib -- secrets       # 48 tests
cargo test -p nika-serve --lib                   # 67 tests
cargo test -p nika-cli --lib -- doctor           # 17 tests
cargo test -p nika-engine --lib -- rig_agent     # 59 tests
cargo test -p nika-storage --lib                 # 24 tests
cargo test -p nika-tui --lib                     # 2154 tests
cargo test -p nika-sdk --lib                     # 56 tests
cargo test -p nika-init --lib                    # 230 tests
```

## Architecture Notes

### inject_secret_to_env pattern
All 9 production `unsafe set_var` sites now go through `nika_engine::secrets::inject_secret_to_env()`. The safety contract: call only during sequential boot or single-threaded CLI. Test-only set_var remains in test modules (acceptable — `#[serial]` tests hold ENV_LOCK).

### SlotGuard pattern
The `SlotGuard` in `routes/workflows.rs` mirrors the `WorkerGuard` in `worker.rs`. The handler guard covers the gap between counter increment and WorkerGuard creation in the spawned task. `disarm()` transfers responsibility.

### Error code ranges
- NIKA-001 to NIKA-499: nika-engine (authoritative)
- NIKA-500 to NIKA-599: nika-init
- NIKA-100 to NIKA-110: shared MCP codes (nika-engine + nika-mcp, same semantics)
