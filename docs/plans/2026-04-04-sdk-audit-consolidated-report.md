# SDK & API Mega Audit — Consolidated Report

**Date**: 2026-04-04 | **Version**: v0.68.1 | **Agents**: 4 parallel reviewers
**Scope**: nika-sdk, nika-serve, nika-napi, nika-py, nika-client

---

## Executive Summary

4 agents audited the full SDK ecosystem across 7 angles. Results:

| Severity | Count | Launch-blocking? |
|----------|-------|-----------------|
| CRITICAL | 1 | YES |
| HIGH | 15 | 5 must-fix before May 5 |
| MEDIUM | 11 | Nice to have |
| LOW | 7 | Polish |

**Key themes:**
1. **Wire format drift** — cancel() returns wrong type, JobStatus missing Pending, ArtifactInfo.format nullability mismatch
2. **napi binding should be deprecated** — .d.ts lies about runtime types, TS client is strictly superior
3. **Embedded mode is a silent capability trap** — no artifacts, no resume, no tests
4. **Test coverage has structural holes** — EmbeddedTransport: 0 tests, cross-crate event schema: 0 tests

---

## CRITICAL (1)

### C-1: `cancel()` deserializes CancelResponse as JobInfo — silent field corruption
**Source**: Agent 1 | **Confidence**: 95%

Server returns `{ job_id, status, message }` from `POST /v1/cancel/{id}`.
SDK deserializes as `JobInfo` which expects `workflow, created_at, started_at, ...`.
Result: callers get a `JobInfo` with empty workflow, null timestamps.

**Files**: `nika-sdk/src/remote.rs:103`, `nika-serve/src/routes/workflows.rs:273`
**Fix**: Make server return full `StatusResponse` from cancel endpoint (simplest), or add `CancelInfo` type to SDK.

---

## HIGH — Must-Fix Before Launch (5)

### H-1: napi .d.ts discriminated unions contradict flat runtime struct
**Source**: Agent 1 + Agent 4 | **Confidence**: 96%

`index.d.ts` declares `NikaEvent = StartedEvent | TaskStartEvent | ...` with type narrowing.
But `lib.rs:42` exports a flat struct where ALL fields are optional — no discriminated union at runtime.
TypeScript `event.type === 'task_start'` type-narrows to `{ verb: string }` but `verb` is `undefined` for non-task events.

**Files**: `nika-napi/index.d.ts:16`, `nika-napi/src/lib.rs:42`
**Fix**: Either (a) emit proper per-variant objects in Rust, or (b) deprecate napi in favor of nika-client.

### H-2: napi .d.ts uses camelCase but napi-rs exports snake_case at runtime
**Source**: Agent 1 | **Confidence**: 96%

`index.d.ts:16`: `jobId`, `taskId`, `durationMs`.
Runtime object: `job_id`, `task_id`, `duration_ms` (napi-rs default = Rust field name).
No `#[napi(js_name = "...")]` attributes. TypeScript code type-checks but crashes at runtime.

**Files**: `nika-napi/index.d.ts:16-130`, `nika-napi/src/lib.rs:42-167`
**Fix**: Add `#[napi(js_name)]` on every field, or deprecate napi.

### H-3: JobStatus missing `Pending` — initial state maps to `Unknown`
**Source**: Agent 1 | **Confidence**: 92%

Server returns `"pending"` on `POST /v1/run`. SDK `JobStatus` has no `Pending` variant.
`#[serde(other)]` catches it as `Unknown`. Users comparing `== JobStatus::Queued` miss the real initial state.

**File**: `nika-sdk/src/types.rs:98`
**Fix**: Add `Pending` variant with `#[serde(rename = "pending")]`.

### H-4: NIKA-XXX error codes never populated in `SdkError::Engine`
**Source**: Agent 4 | **Confidence**: 90%

`SdkError::Engine { message, code }` has `code: Option<String>`.
But `Job::wait()` at `client.rs:147` always sets `code: None`.
Display impl drops `code`. Users never see NIKA-XXX codes through the SDK.

**Files**: `nika-sdk/src/client.rs:147`, `nika-sdk/src/error.rs:19`
**Fix**: Parse NIKA-XXX prefix from error strings, or pass structured code from server JSON.

### H-5: 429 → QueueFull, 503 → generic Http (inverted semantics)
**Source**: Agent 1 | **Confidence**: 93%

SDK maps 429 → `SdkError::QueueFull`. Server sends 503 for queue full, 429 for rate limiting.
Rate limit hits are misidentified as "queue full". Queue-full errors become generic Http.

**Files**: `nika-sdk/src/remote.rs:312`, `nika-serve/src/error.rs:42`
**Fix**: Map 503 → QueueFull, 429 → new RateLimited variant (or just Http).

---

## HIGH — Should Fix (10)

### H-6: Python Unauthorized → PyConnectionError (wrong exception type)
**Source**: Agent 1 | **File**: `nika-py/src/lib.rs:42`
401 auth failure maps to `ConnectionError`. Should be `PermissionError` or `NikaError`.

### H-7: Python 2-worker Tokio runtime deadlocks under concurrent load
**Source**: Agent 2 | **File**: `nika-py/src/lib.rs:20`
Global runtime has 2 workers. 100 concurrent Python threads each call `block_on` → deadlock.
Fix: `num_cpus::get().max(4)` or document concurrency limit.

### H-8: Node EventStream leaks SSE connection for up to 3600s on abandon
**Source**: Agent 2 | **File**: `nika-napi/src/lib.rs:356-378`
JS `break` from `for await` drops rx but upstream SSE connection lives until TCP timeout.
Fix: Drop stream explicitly in forwarding task when channel closes.

### H-9: `ArtifactInfo.format` is `String` on wire but `Option<String>` in SDK
**Source**: Agent 1 | **Files**: `nika-serve/src/routes/artifacts.rs:24`, `nika-sdk/src/types.rs:155`
Nullability mismatch. Fix: Align to required String in SDK.

### H-10: `SdkError::Http.body` leaks raw server response in Debug output
**Source**: Agent 2 | **File**: `nika-sdk/src/error.rs:16`
Raw response may contain provider error details, account info. Fix: Strip or make `body` private.

### H-11: Embedded transport artifacts always empty — no warning to caller
**Source**: Agent 1 | **File**: `nika-sdk/src/embedded.rs:342`
`list_artifacts()` returns `Ok(Vec::new())`. User assumes no artifacts exist.
Fix: Return `Err(SdkError::Engine { message: "..." })` or `tracing::warn!`.

### H-12: Embedded resume_from rejected after expensive file parse
**Source**: Agent 1 | **File**: `nika-sdk/src/embedded.rs:100`
Check should be first line of `submit()`, before any I/O.

### H-13: `workflows.list/source/reload` only in TS client
**Source**: Agent 1 | **File**: `nika-sdk/src/transport.rs` (absent)
3 server endpoints with no Rust SDK support. Affects all non-TS consumers.

### H-14: Two npm packages with no documented distinction
**Source**: Agent 4 | `@supernovae-st/nika-sdk` (napi) vs `@supernovae-st/nika-client` (TS)
No guidance for which to use. TS client is strictly superior.
Fix: Deprecate napi before launch, document in both READMEs.

### H-15: EmbeddedTransport has ZERO tests — and it's the prod default
**Source**: Agent 3 | **File**: `nika-sdk/src/embedded.rs`
360 lines, polling loop, broadcast channel, job reaping — all untested.
Also: `Executor::Embedded` never tested in nika-serve integration tests (`lib.rs:478`).

---

## MEDIUM (11)

| ID | Description | Source | File |
|----|-------------|--------|------|
| M-1 | No compile-time bridge between ServeEvent and sdk::Event — drift risk | A1 | types.rs:24, events.rs:33 |
| M-2 | Embedded stream race: job eviction during terminal state check | A1 | embedded.rs:309-336 |
| M-3 | Embedded `cancel()` leaks forwarder task until job reap at 1024 | A2 | embedded.rs:250-267 |
| M-4 | `validate_path_segment` misses percent-encoded traversal (`%2F`) | A2 | remote.rs:57-61 |
| M-5 | Webhook `verify()` is dead code — no caller in serve | A2 | webhook.rs:184 |
| M-6 | Python `EventStream.__next__` race: concurrent threads get premature StopIteration | A2 | nika-py/lib.rs:338 |
| M-7 | SSE 1 MiB buffer vs uncapped embedded output — large completions abort stream | A2 | remote.rs:143 |
| M-8 | Python error hierarchy flat — no NikaJobCancelledError subclass | A1 | nika-py/lib.rs:43 |
| M-9 | napi version frozen at v0.63.0, workspace at v0.68.1 | A4 | nika-napi/package.json |
| M-10 | Cross-SDK event schema roundtrip: 0 tests exist | A3 | — |
| M-11 | `/metrics` endpoint behind auth but `build_metrics_router` has none built-in | A1 | routes/mod.rs:80 |

---

## LOW (7)

| ID | Description | Source |
|----|-------------|--------|
| L-1 | Python `run_sync` suffix unnecessary (entire SDK is sync) | A4 |
| L-2 | Python `base_url` param vs `url` everywhere else | A4 |
| L-3 | `RunOptions` mixes public fields and builder methods | A4 |
| L-4 | `Job.wait()` name implies blocking (consider `complete()` or `join()`) | A4 |
| L-5 | Connection errors don't hint "is nika serve running?" | A4 |
| L-6 | TS client README doesn't explain nika serve prerequisite | A4 |
| L-7 | Reap strategy DoS: 1024+ Running jobs → map grows unbounded | A2 |

---

## Socratic Answers (from Agent 4)

**Q: Why 4 SDKs?**
→ napi binding should be deprecated. TS client is strictly superior (zero deps, runs on Workers/Deno/Bun, has auto-retry, streaming, webhook verify, proper types). Final count: 3 SDKs (Rust, Python, TypeScript).

**Q: Why is the TS client in a separate repo?**
→ Makes sense for independent npm publishing. But version alignment must be documented.

**Q: Why embedded mode has no artifacts?**
→ Probably "not yet implemented" but silently returning empty is dangerous. Should error explicitly.

**Q: Why events untyped in Python?**
→ Acceptable for Python (dicts are idiomatic). But should have TypedDict definitions in `.pyi` stubs.

**Q: Should Python SDK be async?**
→ No as primary. Blocking is correct for scripts/notebooks/data pipelines. Optional async layer (pyo3-asyncio) would be additive.

---

## Recommended Action Plan

### Sprint 1 — Launch Blockers (before May 5)

| # | Fix | Effort |
|---|-----|--------|
| 1 | C-1: Fix cancel() return type (server → StatusResponse) | 30 min |
| 2 | H-3: Add `Pending` to JobStatus | 15 min |
| 3 | H-5: Fix 429/503 mapping | 15 min |
| 4 | H-4: Populate NIKA-XXX code in SdkError::Engine | 1h |
| 5 | H-14: Deprecate napi, document TS client as canonical | 30 min |

### Sprint 2 — High Priority

| # | Fix | Effort |
|---|-----|--------|
| 6 | H-7: Python runtime workers → num_cpus | 5 min |
| 7 | H-6: Python Unauthorized → PermissionError | 5 min |
| 8 | H-9: ArtifactInfo.format → required String | 10 min |
| 9 | H-11: Embedded artifacts → explicit error | 10 min |
| 10 | H-12: Move resume check before file parse | 5 min |
| 11 | H-10: Strip/privatize SdkError::Http.body | 15 min |
| 12 | H-15: Write EmbeddedTransport tests | 2-3h |

### Sprint 3 — Polish

| # | Fix | Effort |
|---|-----|--------|
| 13 | H-13: Add workflow list/source/reload to Transport trait | 1h |
| 14 | M-1: Cross-crate ServeEvent ↔ Event roundtrip test | 30 min |
| 15 | M-3: Cancel() abort forwarder task | 15 min |
| 16 | M-6: Python EventStream thread-safety docs/fix | 15 min |
| 17 | L-1 to L-6: Naming/DX polish | 1h |
