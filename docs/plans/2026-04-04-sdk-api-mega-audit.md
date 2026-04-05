# SDK & API Mega Audit — Handoff Prompt

**Date**: 2026-04-04 | **Version**: v0.68.1 | **Scope**: Full SDK ecosystem review
**Goal**: Verify the entire SDK surface is logical, consistent, well-architected, and production-ready

---

## What We're Auditing

```
┌─────────────────────────────────────────────────────────────┐
│  5 Components — 1 API server + 4 SDK implementations       │
│                                                             │
│  nika-serve (Rust)     HTTP API — 13 endpoints, SSE, auth  │
│  nika-sdk (Rust)       Core SDK — 3 transports, 56 tests   │
│  nika-napi (Node.js)   napi-rs binding — 14 tests          │
│  nika-py (Python)      PyO3 binding — 26 tests             │
│  nika-client (TS)      Standalone client — 56 tests, 0 dep │
└─────────────────────────────────────────────────────────────┘
```

## Key Files

| Component | Root | Main Source | Tests | Types/API |
|-----------|------|-------------|-------|-----------|
| nika-serve | `tools/nika-serve/` | `src/lib.rs` (850L) | 74 tests | `src/routes/workflows.rs` |
| nika-sdk | `tools/nika-sdk/` | `src/client.rs` (201L) | 56 tests | `src/types.rs` (384L) |
| nika-napi | `tools/nika-napi/` | `src/lib.rs` (595L) | 14 tests | `index.d.ts` (214L) |
| nika-py | `tools/nika-py/` | `src/lib.rs` (764L) | 26 tests | `__init__.pyi` (72L) |
| nika-client | `../nika-client/` | `src/index.ts` | 56 tests | `src/types.ts` |

## API Contract (source of truth: nika-serve)

| Endpoint | Method | Auth | SDK Method |
|----------|--------|------|------------|
| `/health` | GET | NO | `client.health()` |
| `/v1/run` | POST | YES | `client.submit(workflow, options)` |
| `/v1/status/{id}` | GET | YES | `job.status()` |
| `/v1/cancel/{id}` | POST | YES | `job.cancel()` |
| `/v1/events/{id}` | GET/SSE | YES | `job.events()` / `job.stream_events()` |
| `/v1/jobs/{id}/artifacts` | GET | YES | `job.artifacts()` |
| `/v1/jobs/{id}/artifacts/{name}` | GET | YES | `artifact.download()` |
| `/v1/workflows` | GET | YES | `client.workflows.list()` (TS only) |
| `/v1/workflows/{name}/source` | GET | YES | `client.workflows.source()` (TS only) |
| `/v1/reload` | POST | YES | (not exposed in SDKs) |
| `/v1/openapi.json` | GET | NO | (not exposed) |
| `/metrics` | GET | YES | (not exposed) |

## Event Types (wire format — SSE JSON)

```json
{ "type": "started",          "job_id": "..." }
{ "type": "task_start",       "job_id": "...", "task_id": "...", "verb": "infer" }
{ "type": "task_complete",    "job_id": "...", "task_id": "...", "duration_ms": 1234 }
{ "type": "task_failed",      "job_id": "...", "task_id": "...", "error": "...", "duration_ms": 0 }
{ "type": "artifact_written", "job_id": "...", "task_id": "...", "path": "...", "size": 456 }
{ "type": "completed",        "job_id": "...", "output": "..." }
{ "type": "failed",           "job_id": "...", "error": "..." }
{ "type": "cancelled",        "job_id": "..." }
```

---

## Audit Angles (7 agents)

### Agent 1: API Contract Consistency

**Question**: Do all 4 SDK implementations expose the SAME API contract?

Check:
- [ ] Method names: are they idiomatic per language? (snake_case Python, camelCase TS/Node)
- [ ] Do all SDKs cover all endpoints? What's missing where?
- [ ] Request/response types: are field names consistent across wire format → Rust SDK → Node → Python → TS?
- [ ] Error types: do error variants map 1:1 across all SDKs?
- [ ] Event types: same 8 events, same field names (accounting for naming conventions)?
- [ ] RunOptions: same fields across all SDKs? (inputs, resume_from)
- [ ] JobStatus enum: same variants everywhere? Forward-compat (Unknown)?

**Critical gap to check**: TS client has `workflows.list()` and `workflows.source()` but Rust/Node/Python SDKs don't. Is this intentional?

### Agent 2: Architecture & Transport Layer

**Question**: Is the transport abstraction clean, correct, and complete?

Check:
- [ ] Sealed trait pattern: is it correctly preventing external implementations?
- [ ] Arc<dyn Transport>: is this the right abstraction? (vs generics, vs enum dispatch)
- [ ] RemoteTransport: does the URL validation match nika-serve's path validation?
- [ ] EmbeddedTransport: why are artifacts not tracked? Is this a known limitation or a bug?
- [ ] EmbeddedTransport: resume_from returns error — should it be supported?
- [ ] MockTransport: is it sufficient for consumer testing?
- [ ] SSE parser: does it handle all edge cases? (reconnect, keepalive, multiline data)
- [ ] Buffer limit: 1 MiB for SSE frames — is this aligned with serve's output limits?

**Critical question**: The embedded transport doesn't track artifacts and can't resume. This means embedded mode is a SUBSET of remote mode. Is this documented? Should Client expose a `capabilities()` method?

### Agent 3: Error Handling & Edge Cases

**Question**: Are errors handled correctly, consistently, and without information leaks?

Check:
- [ ] HTTP status code mapping: 401→Unauthorized, 404→NotFound, 429→QueueFull — complete?
- [ ] What happens on 500? On 502/503 (proxy errors)?
- [ ] Timeout behavior: 30s for API calls, 3600s for SSE — configurable by SDK consumer?
- [ ] Connection errors: retry strategy? Exponential backoff?
- [ ] SSE reconnect: does Last-Event-Id work correctly? What if events were dropped?
- [ ] Job.wait() on already-completed job: does it work or hang?
- [ ] Double cancel: what happens? Error or idempotent?
- [ ] Submit with invalid workflow path: clear error message?
- [ ] Token validation: what if empty string? What if None/null?
- [ ] Path traversal: validated in RemoteTransport AND nika-serve — redundant? (defense in depth = OK)

**Critical question**: The TS client has automatic retry on 429/5xx with backoff. The Rust SDK does NOT. Should it?

### Agent 4: Security Review

**Question**: Are there any security vulnerabilities in the SDK chain?

Check:
- [ ] Token handling: is the Bearer token stored securely? (not logged, not in URLs)
- [ ] SSE stream: could a malicious server inject events that bypass type checking?
- [ ] Path traversal: job_id and artifact name validated — what about workflow name in submit?
- [ ] SSRF: the SDK connects to user-specified URLs — any SSRF risk in the SDK itself?
- [ ] Deserialization: serde(tag = "type") with unknown variants — DoS via large payloads?
- [ ] Python GIL: is py.allow_threads() used correctly everywhere? Race conditions?
- [ ] Node napi: are there any unsafe operations? Memory leaks in EventStream?
- [ ] Webhook HMAC verification: timing-safe comparison? Replay protection?
- [ ] Error messages: do they leak internal paths, tokens, or sensitive data?

### Agent 5: Type Safety & Correctness

**Question**: Are the types correct, complete, and aligned with the wire format?

Check:
- [ ] Event serde: roundtrip serialize/deserialize all 8 types — any field drift?
- [ ] JobStatus: server returns "pending"|"running"|"completed"|"failed"|"cancelled" — all mapped?
- [ ] ArtifactInfo: `format` is Option<String> in Rust SDK but `String` in serve — mismatch?
- [ ] JobInfo.exit_code: i32 in Rust, number in TS, int in Python — overflow risk?
- [ ] RunOptions.inputs: Value in Rust, Record<string, unknown> in TS, dict in Python — any limitations?
- [ ] TypeScript: are the discriminated union types (`type NikaEvent = ...`) correct for exhaustive pattern matching?
- [ ] Python: events returned as `dict[str, object]` instead of typed dataclass — why?

**Critical question**: Python returns events as raw dicts, not typed objects. The TS client also uses plain objects. Only the Rust SDK has typed Event enum. Is this the right design for each language?

### Agent 6: Test Coverage & Quality

**Question**: Are the tests meaningful, comprehensive, and not superficial?

Check:
- [ ] nika-sdk: 56 tests — but 0 tests for Client directly and 0 for EmbeddedTransport
- [ ] nika-napi: 14 tests — only event conversion, no integration
- [ ] nika-py: 26 tests — event conversion + repr/eq, no integration
- [ ] nika-serve: 74 tests — good coverage but no E2E with real workflow execution
- [ ] nika-client: 56 tests — includes E2E with mock HTTP server, best coverage
- [ ] Missing: cross-SDK consistency tests (same workflow → same events across all SDKs)
- [ ] Missing: error path tests (what if server returns garbage? Connection drops mid-stream?)
- [ ] Mock transport: tests the mock, not the real behavior — is this sufficient?

### Agent 7: DX & Documentation

**Question**: Would a developer have a good experience using these SDKs?

Check:
- [ ] README quality: does each SDK have clear quick-start examples?
- [ ] Error messages: actionable? (e.g., "invalid URL" → what format is expected?)
- [ ] Type stubs: Python __init__.pyi and Node index.d.ts — complete and accurate?
- [ ] Changelog: does nika-client have one? Versioning strategy?
- [ ] Version alignment: nika-sdk is 0.68.1, nika-client is 0.63.0 — confusing?
- [ ] Package naming: @supernovae-st/nika-sdk (npm native) vs @supernovae-st/nika-client (TS) — TWO npm packages?
- [ ] Publication status: nika-client not yet published. npm publish blocker?
- [ ] OpenAPI spec: auto-generated by aide — is it accurate? Can TS client be generated from it?

---

## Socratic Questions (answer before coding)

1. **Why 4 SDKs?** Rust SDK + TS client + Node binding + Python binding. Is this the right granularity? Should the TS client replace the Node binding? Should there be a unified approach?

2. **Why is the TS client a separate repo?** (`../nika-client/` vs `tools/nika-napi/`). The Rust/Node/Python SDKs are in the nika monorepo. The TS client is outside. Why?

3. **Why does embedded mode not support artifacts?** This is a fundamental capability gap. A user choosing embedded mode loses artifact download. Is this acceptable for a production SDK?

4. **Why are events untyped in Python/TS?** The Rust SDK has `Event` enum. Python returns `dict`. TS client returns plain objects. This loses the main benefit of typed SDKs.

5. **Who is the primary SDK consumer?** If it's Nicolas's nk-jungo (Node.js), why does the TS client exist separately from nika-napi? Does nk-jungo use the napi binding or the TS client?

6. **Is the API stable?** v0.68.1 with no backward compat guarantee (per project rules). But SDKs are published. What's the contract with consumers?

---

## Invariants

```bash
# Rust SDK
cd tools && cargo test -p nika-sdk --lib
# Node binding  
cd tools && cargo test -p nika-napi --lib
# Python binding (needs Python)
cd tools && cargo test -p nika-py --lib
# Serve
cd tools && cargo test -p nika-serve --lib
# TS client
cd ../nika-client && npm test
```

## Output Expected

Each agent produces a structured report with:
1. **Findings** (CRITICAL / HIGH / MEDIUM / LOW)
2. **Recommendations** (with specific file:line references)
3. **Questions for the maintainer** (things that need human judgment)
