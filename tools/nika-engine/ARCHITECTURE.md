# nika-engine Architecture

The embeddable workflow engine: parses YAML, builds a DAG, resolves bindings,
dispatches tasks to providers, and streams results. ~146k LOC (incl. tests).

Post-Session 13 (Constellation v2.3). Last updated 2026-04-10.

## Crate Dependencies (downward only)

```
nika-engine
├── nika-kernel      Effect traits + 5 per-verb Caps structs
├── nika-core        AST types, catalogs, policy config, trust
├── nika-event       EventLog, TraceWriter
├── nika-builtin     37 builtin tools (core, file, data, introspection) — Phase 12
├── nika-policy      PolicyEnforcer + SSRF helpers (L1, S12-F5)
├── nika-extract     Fetch post-processing — 9 extract modes (L2, S12-F7)
├── nika-verb-exec   exec: verb crate (L2, NEW S13-B1) — ShellExecutor trait
├── nika-verb-invoke invoke: verb crate (L2, NEW S13-C1) — BuiltinRouter trait
├── nika-verb-fetch  fetch: verb crate (L2, NEW S13-D1) — HttpClient trait
├── nika-exec-runner TokioShell L1 impl (bridge uses directly for engine→verb handoff)
├── nika-clock       SystemClock L1 impl (bridge uses directly)
├── nika-fs          TokioFs L1 impl (bridge uses directly)
├── nika-media       CAS store, image/document processing
├── nika-mcp         MCP client (rmcp)
├── nika-vault       Encrypted secrets (XChaCha20 + Argon2i)
├── nika-daemon      Background daemon IPC
├── nika-display     CLI renderers (Renderer trait)
└── nika-lsp-core    LSP intelligence (opt-in feature "lsp")
```

`nika-runtime` (L3) sits **above** nika-engine in the diamond — it depends
on the verb crates + kernel + event + core, and provides `VerbCapabilities`
+ `dispatch()`. The engine bridges verb execution to `nika-verb-*::run()`
directly during S13; in S14 the Runner will call `nika-runtime::dispatch`
as the live path.

nika-kernel (L0.5) was added in Session 4 (Phase 11). It defines pure traits
with zero I/O — nika-engine implements them via concrete types (RigProvider,
TokioFs, etc.). The 5 L1 effect crates implement the kernel traits:

```
nika-kernel (L0.5, ~900 LOC — traits only, zero deps)
├── nika-clock          SystemClock impl
├── nika-fs             TokioFs impl (FsRead + FsWrite splinters, S12-F4)
├── nika-blob           DiskBlobStore impl
├── nika-http           ReqwestClient impl (HttpClient::send_streaming, S12-F2)
├── nika-exec-runner    TokioShell impl (CancellationToken support, S12-F3)
├── nika-policy         PolicyEnforcer impl (PolicyChecker trait, S12-F5)
└── nika-extract        L2 pure extraction pipeline (no kernel trait, S12-F7)
     (+ nika-kernel-mock — test doubles for all traits)
```

## Constellation Session 12 Foundation — 2026-04-10

S12 Foundation (11 commits) extended the kernel trait surface and extracted
two pure crates to make Session 13 verb extraction mechanical:

| Commit | Purpose |
|--------|---------|
| S12-F1 | nika-kernel: PolicyChecker trait (object-safe, 4 methods) |
| S12-F2 | nika-kernel: HttpClient::send_streaming + HttpError::TooLarge/Unsupported |
| S12-F3 | nika-kernel: ShellCommand::cancel + ShellError::Cancelled + TokioShell integration |
| S12-F4 | nika-kernel: split Filesystem → FsRead + FsWrite splinters |
| S12-F5 | **New nika-policy L1 crate** (PolicyEnforcer moved from engine, ~1263 LOC) |
| S12-F6 | engine: delete duplicated runtime/policy.rs (`pub use nika_policy as policy`) |
| S12-F7 | **New nika-extract L2 crate** (extract.rs moved from engine, ~1327 LOC) |
| S12-F8 | engine: delete runtime/executor/extract.rs wrapper, rewire callers |
| S12-F9 | nika-kernel: 5 per-verb Caps structs (ExecCaps/FetchCaps/...) — types only |
| S12-F10 | docs: this update |
| S12-F11 | test: golden e2e regression suite for all 5 verbs |

**Engine LOC:** 148,792 → ~146,200 (−2,590)
**Crate count:** 28 → 30 (+nika-policy, +nika-extract)
**Diamond verified:** `cargo tree -p nika-policy --no-default-features` and
`cargo tree -p nika-extract --no-default-features` both have zero nika-engine
dependency. Both crates compile without nika-engine in their dep graph.

## Constellation Session 13 — 2026-04-10

S13 created `nika-runtime` (L3) and extracted 3 verb crates (exec, invoke,
fetch). The verb crates consume kernel traits only; engine bridges delegate
verb execution to `nika_verb_*::run()` after doing template resolution +
security validation.

| Commit | Purpose |
|--------|---------|
| S13-A0 | nika-kernel: expand Caps structs (+ cancel, workflow_base_dir, working_dir_mode, project_root); add BuiltinRouter + McpPool traits; MockPolicyChecker in nika-kernel-mock |
| S13-A1 | **New nika-runtime L3 crate** — VerbCapabilities bundle + dispatch() match + RuntimeError |
| S13-B1 | **New nika-verb-exec crate** — run() via ShellExecutor trait + 11 tests |
| S13-B2 | engine bridge: run_exec delegates to nika_verb_exec::run via TokioShell; ShellCommand gains env_remove + pre_validated fields |
| S13-B3 | GATE-S13-1 regression: >1MB subprocess deadlock test through full bridge |
| S13-B4 | nika-runtime::verb_exec::run_exec adapter + RuntimeError::Exec variant |
| S13-C1 | **New nika-verb-invoke crate** — builtin routing via BuiltinRouter trait + 6 tests |
| S13-C2 | engine bridge: builtin path delegates to nika_verb_invoke::run via BuiltinRouterAdapter (MCP path stays inline for S13) |
| S13-C3 | nika-runtime::verb_invoke::run_invoke adapter + RuntimeError::Invoke variant |
| S13-D1 | **New nika-verb-fetch crate** — HTTP fetch via HttpClient trait + nika-extract pipeline + 4 tests |
| S13-D2 | nika-runtime::verb_fetch::run_fetch adapter + RuntimeError::Fetch variant |
| S13-E1 | docs: this update + session memory |

**Engine LOC:** 146,473 → 146,557 (+84 — exec bridge -322, invoke adapter +271 for BuiltinRouterAdapter/NullBlobStore/NullHttpClient shims, expected to drop in S14 when MCP+media extract)
**Crate count:** 28 → **32** (+nika-runtime L3, +3 verb crates L2)
**Tests:** 10,805 → **10,840** (+35 across the new crates)
**Diamond verified:** all 4 new crates have zero `nika-engine` in their dep
graph (`cargo tree -p nika-runtime | grep nika-engine` → empty for each).
**Golden oracle:** 5/5 green throughout the session — `golden_exec_hello`,
`golden_invoke_builtin_log`, and `golden_fetch_placeholder` all exercise
the bridge paths and prove observable output is preserved.

## Constellation Session 14 — 2026-04-11

S14 split into **wave A–B (pre-launch infer extraction prep)** and a
**post-review hotfix wave (S14.5)** that fixed one P1 correctness bug
and codified three new sacred invariants the post-S14 4-agent review
caught.

### Wave A–B commits (5)

| Commit | Hash | Purpose |
|--------|------|---------|
| S14-α | `c96dec861` | **kernel: enrich `InferEvent::Done` as struct variant.** Mark enum `#[non_exhaustive]`, convert `Done(StopReason)` → `Done { stop_reason, request_id, finish_reason_raw }`. Updates 5 sites atomically: definition, `nika-kernel-mock` MockProvider stream, `nika-engine/.../rig/kernel_bridge.rs` adapter + 2 test matches. New test in kernel-mock asserts request_id threads through synthesized streams. |
| S14-β | `9f384e07a` | **verb-fetch: migrate pure retry/hreflang helpers from engine.** New modules `retry.rs` (4 helpers + 16 tests: `safe_backoff_delay`, `parse_retry_after`, `is_html_content_type`, `MAX_BACKOFF_MS`) and `hreflang.rs` (3 functions + 4 tests, `merge_link_hreflang` made generic over error type). Engine `fetch.rs` strips 277 LOC and re-imports the helpers via `use nika_verb_fetch::{retry,hreflang}::*`. New `nika-engine` dep on `nika-verb-fetch`. |
| S14-γ | `935658eae` | **verb-fetch: add `RetryExhausted` + `DeadlineExceeded` error variants.** Prep for S15 retry loop orchestration extraction. `VerbFetchError` marked `#[non_exhaustive]`. 3 Display tests verify message formatting. |
| S14-δ | `aebea1cd9` | **verb-infer: golden oracle asserts all `ProviderResponded` fields.** Existing test asserted only 3 of 8 fields. New test `infer_emits_provider_responded_with_all_fields` enqueues an `InferResponse` with known values in every optional field (`request_id`, `ttft_ms`, `cost_usd`, `cache_read_tokens`) and asserts each on the emitted event. S12-G2 compliance — never lifecycle-only. |
| S14-ε | `acf9d1784` | **verb-exec: pre-spawn cancellation check.** Add `caps.cancel.is_cancelled()` short-circuit before `caps.shell.run(cmd).await`. Micro-optimisation that mirrors verb-invoke's pattern: skip subprocess fork on already-cancelled task under `fail_fast: true` fan-out. New test uses an empty `MockShell` queue (panics if called) + pre-cancelled token to prove zero subprocess fork happens. |

### Wave 14.5 hotfix commits (2 — post-review)

The post-S14 4-agent review (`code-reviewer` + `rust-architect` ×2 +
`code-explorer`) caught one P1 bug, three architectural violations, and
one symmetry gap. All fixed in two follow-up commits.

| Commit | Hash | Purpose |
|--------|------|---------|
| S14.5-A | `53513e5ee` | **fix: post-S14 review findings hotfix A.** (1) `f64::EPSILON` assertion in S14-δ was mathematically wrong — at magnitude 0.0042 the ULP is ~9.3e-19 not EPSILON ≈ 2.22e-16, accepting up to ~5.3e-14 silent drift. Replaced with `assert_eq!` exact (pure pass-through, no arithmetic). (2) `#[non_exhaustive]` retrofit to `VerbExecError`, `VerbInvokeError`, `VerbInferError` for symmetry with `VerbFetchError`. Engine match sites in `exec.rs:267` and `invoke.rs:183` gain wildcard arms with `format!("unmapped … {other:?}")` for triageability. (3) Retrofit `# TEMP` markers on all 3 `nika-verb-*` deps in `nika-engine/Cargo.toml` per invariant #22. |
| S14.5-B | `144f5abeb` | **docs: codify invariants #23/#24/#25 + correct crate count.** New invariants in `.claude/rules/architecture.md`: **#23** kernel-adjacent helpers stay primitive-typed (no `reqwest::*` / `tokio::*` leaks — triggered by `parse_retry_after(&HeaderMap)` shipped in S14-β); **#24** event emission singletons — exactly one `EventKind::*` emit site per file (currently violated by 7 `ProviderResponded` sites in `infer.rs:621/1156/1330/1388/1527/1592/1898` — W14-B2 must collapse); **#25** all verb-crate errors `#[non_exhaustive]` from day one with wildcard arms in mapping fns. Crate count corrected from "33" to **35** (32 diamond + 3 outside: napi, py, macros). |

### Post-S14 measurements

**Engine LOC:** 146,473 (S13 baseline) → **~146,600** (S14 net: −277 fetch.rs from S14-β, +84 hotfix wildcard arms + TEMP comments, +others = roughly wash). The bulk of engine reduction lives further out in S15+ when bridge surgery starts.

**Crate count:** 32 → **35** (the prior count was off by 2; no new crates landed in S14 itself — S14 only enriched existing crates).

**Tests:** 10,840 → **~10,900** (+1 in `nika-kernel-mock`, +20 in `nika-verb-fetch` via retry+hreflang migration, +3 in `nika-verb-fetch` Display tests, +1 in `nika-verb-infer` golden oracle, +1 in `nika-verb-exec` pre-cancel test, ≈+26 net; rest from W14-A0/A1/A2/B0/B1/B3 pre-S14 commits).

**Verb crate matrix post-S14:**

| Crate | LOC | Tests | Bridge live? | dispatch arm | Errors `#[non_exhaustive]`? | Pre-spawn cancel? |
|-------|-----|-------|--------------|---------------|------------------------------|--------------------|
| `nika-verb-exec` | 446 | 13 | YES (S13-B2) | NotImpl | YES (S14.5) | YES (S14-ε) |
| `nika-verb-fetch` | 297 + retry/hreflang | 28 | partial (helpers only) | NotImpl | YES (S14-γ) | n/a (`tokio::select!` inside `run`) |
| `nika-verb-invoke` | 387 | 6 | partial (builtin only, MCP inline) | NotImpl | YES (S14.5) | n/a (`tokio::select!` inside `run`) |
| `nika-verb-infer` | 499 | 10 | **NO** (W14-B2 deferred) | NotImpl | YES (S14.5) | n/a (`tokio::select!` inside `run`) |

### Sacred invariants added in S14 / S14.5

- **#17** No `infer_vision` / `infer_with_tools` trait methods — unify into `InferRequest` (S14 W14-A0)
- **#18** Capability queries belong on the Provider trait, not on `ModelCapabilities`
- **#19** Per-crate `new()` constructors on every `#[non_exhaustive]` struct in same commit
- **#20** Verb-crate minimum extraction is valid architecture
- **#21** StopReason ↔ FinishReason mapping lives at the verb-crate/event boundary
- **#22** TEMP engine deps must carry `# TEMP` comments with clearance condition
- **#23** Kernel-adjacent helpers use std/primitive/Bytes types only — no `reqwest::*` / `tokio::*` leaks (S14.5)
- **#24** Event emission for a given `EventKind::*` variant happens at exactly one call site per file (S14.5)
- **#25** All verb-crate error enums are `#[non_exhaustive]` from day one (S14.5)

### Known debts entering S15

1. **`McpPool` trait too thin** — 88 LOC, 3 methods. Missing `call_tool_with_retry_events` surface, `read_resource → ResourceContent` (currently returns `String`, drops blob), 50 MB result cap, cancel tokens, `list_tools`. Blocks `McpPoolAdapter`, blocks `NoopMcpPool` removal. **S15-A0/A1/A2/A3 target.**
2. **`infer.rs` has 7 `ProviderResponded` emit sites** (lines 621/1156/1330/1388/1527/1592/1898). Must collapse before W14-B2 flips bridge to `nika_verb_infer::run()` or golden tests see double-emit. **S15/S16 W14-B2 target.**
3. **`parse_retry_after(&reqwest::header::HeaderMap)` leaks L1 type into L2 verb crate signature** (invariant #23 violation shipped in S14-β). **S15-A0 fix:** change signature to `Option<&str>`, move reqwest to `[dev-dependencies]`.
4. **`safe_backoff_delay` silently truncates** for fractional multipliers `0.0 < m < 1.0`. `safe_backoff_delay(1000, 0.8, 2)` returns 1ms instead of ~640ms. **S15 cleanup:** document explicitly OR fix with `factor.round() as u64`.
5. **`finish_reason_raw` plumbed but never consumed** — `stop_reason_to_finish_reason()` at `nika-verb-infer/src/lib.rs:175-184` hardcodes `"content_filter"` instead of using the field. **W14-B2 fix.**
6. **9 TEMP engine deps in `rig_agent_loop/`** (5,363 LOC across 8 files) — Wave C territory, dedicated S15+/S16 plan in `docs/plans/22-agent-v2-design.md`.
7. **Wave D dispatch() activation blocked** — all 5 arms in `nika-runtime::dispatch::dispatch()` return `NotImplemented` because template resolution + binding + skills + spotlight live in `nika-engine`.

### GATE-S13 resolutions

- **S13-1** (deadlock test): added `subprocess_does_not_deadlock_on_large_output`
  in nika-verb-exec end-to-end through TokioShell (B3).
- **S13-2** (Runner move): deferred to S14 — Runner stays in nika-engine
  during S13 per AMEND-4 (dispatch is parallel, not live).
- **S13-3** (PolicyEnforcer !Send): engine bridge clones PolicyEnforcer out
  of the RwLock BEFORE building Caps to avoid holding `parking_lot::RwLockReadGuard`
  across `.await`. Compile-time Send test on each verb adapter future.
- **S13-4/5** (BuiltinRouter/McpPool traits): added in S13-A0 to nika-kernel.
- **S13-6** (error message format): pre-flight grep found no tests asserting
  on exact error substrings in nika-engine test modules. No impact.
- **S13-7** (Caps expansion): done in S13-A0 with constructors to work
  around `#[non_exhaustive]` construction rules.

## Constellation Session 15 — 2026-04-11

S15 expands the kernel `McpPool` trait surface from 3 thin methods
returning raw `serde_json::Value` / `String` into a 4-method surface
with structured DTOs that preserve every field of
`nika_mcp::ToolCallResult`. Verb crates no longer need to import
`nika-mcp` for MCP call results; the engine bridge in `invoke.rs`
now delegates the transport layer to a concrete `McpPoolAdapter`
impl living in `nika-engine::runtime::mcp_pool_adapter`.

Phase 1 (4-agent parallel review) ran before any code execution,
per the S12-G1/G2/G3 multi-session refactor protocol. The review
caught three architectural issues in the mega-prompt's initial
design and corrected them before commit S15-A0 was written:

1. **`McpCallOptions` cannot hold `Arc<dyn EventEmitter>`** — `nika-kernel`
   has no `nika-event` dep (confirmed via `caps.rs` header doc), and
   adding one would be a new upward coupling. Adapter holds `Arc<EventLog>`
   as a struct field instead, same pattern as `BuiltinRouterAdapter`.
2. **`nika-runtime::dispatch` test helper cannot wire `McpPoolAdapter`** —
   runtime has no `nika-engine` dep (would be a dependency inversion).
   Must use `MockMcpPool` from `nika-kernel-mock` instead.
3. **LOC shrinkage estimate was optimistic** — the mega-prompt's
   "~−500 to −1000 LOC in invoke.rs" target is impossible while the
   media pipeline (coupled to `CasStore`, `MediaProcessor`, and the
   `datastore.set_media` side-channel) stays in the engine bridge.
   Realistic: ~+55 LOC net (adapter construction + error mapping
   helper). The real S15 win is architectural decoupling (engine now
   consumes the kernel `McpPool` trait, not `McpClient` directly),
   not LOC reduction.

### S15 commits (7)

| Commit | Hash | Purpose |
|--------|------|---------|
| S15-A0 | `8c11d2eed` | **kernel: expand `McpPool` trait surface + fix `parse_retry_after` L1 leak.** New DTOs in `nika-kernel/src/mcp.rs`: `McpToolResult` (non_exhaustive + infallible `new()` with byte-identical size computation, 50 MB cap enforcement lives in adapters), `McpResourceContent = nika_core::mcp::ResourceContent` type alias (preserves the `blob` field the pre-S15 trait dropped), `McpToolDescriptor`, `McpCallOptions { task_id, cancel }` (owned fields, no events — invariant #23). `McpError` gains `ResultTooLarge { bytes, limit }` + `Cancelled { server, tool }` variants, `#[non_exhaustive]`, `Clone`. `MAX_MCP_RESULT_SIZE = 50 * 1024 * 1024` const. Trait rewritten with `#[async_trait]`: `call_tool`/`read_resource`/`list_tools`/`has_server`. Object safety asserted via `fn _assert_object_safe(_: &dyn McpPool)`. **Invariant #23 fix:** `parse_retry_after(&reqwest::header::HeaderMap)` → `parse_retry_after(Option<&str>)`; engine callsite in `fetch.rs` now extracts the header value explicitly; `reqwest` moved to `[dev-dependencies]` in `nika-verb-fetch/Cargo.toml`. Tests: +8 kernel/mcp, +2 retry primitive path. |
| S15-A1 | `fa204a0d3` | **kernel-mock: add `MockMcpPool` fixtures + `McpError` Clone derive.** New `nika-kernel-mock/src/mcp.rs` (~330 LOC + 11 tests) — programmable per-server tool queues, resource map, descriptor list, optional call recording. Fixture ctors: `happy()` / `error()` / `oversized()` (builds 51 MB text block — adapter callers assert their `ResultTooLarge` guard fires). Cancellation honored via `opts.cancel.is_cancelled()` pre-check. `nika-kernel-mock` gains `tokio-util` dep for `CancellationToken`. |
| S15-A2 | `1a8400f8d` | **verb-invoke: migrate test stubs to `MockMcpPool`.** Drop the inline `StubMcpPool` impl (~45 LOC of async-trait boilerplate) and use `nika_kernel_mock::MockMcpPool::new()` in test fixtures. Drop `async-trait` dev-dep (no longer needed). 6 tests unchanged in assertions. |
| S15-A3 | `a2097f08e` | **engine: implement `McpPoolAdapter` (no wiring).** New file `nika-engine/src/runtime/mcp_pool_adapter.rs` (~380 LOC + 6 tests). Wraps `Arc<McpClientPool>` + `Arc<EventLog>`, implements the 4-method trait via `#[async_trait]`. **Responsibility split:** adapter owns transport + 50 MB cap + cancel select + DTO translation; engine bridge retains media pipeline (coupled to `CasStore` / `MediaProcessor` / datastore) + McpInvoke/McpResponse boundary events + outer timeout+workflow-cancel select. `map_mcp_error(nika_mcp::McpError, …)` translates every source variant to the kernel `McpError` with wildcard fallthrough (invariant #25). `biased;` select races cancel vs inner call for fine-grained responsiveness. Cancel latency caveat documented in rustdoc: "fires within one in-flight rmcp round-trip or current backoff sleep, not instantaneously". |
| S15-A4 | `06d41fe4b` | **runtime: migrate dispatch test helper to `MockMcpPool`.** Delete `nika-runtime::dispatch::NoopMcpPool` (−41 LOC), use `nika_kernel_mock::mcp::MockMcpPool::new()` in `test_capabilities()`. Drop `async-trait` dev-dep. NoopBuiltinRouter stays inline (BuiltinRouter mock migration is out of S15 scope). |
| S15-A5 | `d066939c2` | **engine: route `invoke.rs` MCP path through `McpPoolAdapter`.** Wire `run_invoke`'s non-builtin branch through the adapter. Delete inline 50 MB checks (adapter owns them), delete inline `let client = self.get_mcp_client(...)` + the dead `Arc<McpClient>` tuple carrier at the old line 744, replace `client.call_tool_with_retry_events(...)` / `client.read_resource(...)` with `adapter.call_tool(...)` / `adapter.read_resource(...)`. New `map_kernel_mcp_error(err, tool_hint, &self.mcp_pool)` helper preserves the `McpNotConfigured` vs `McpNotConnected` distinction by calling `self.mcp_pool.has_config(name)` — pre-S15 behavior the engine tests depend on. Media pipeline stays in engine bridge (iterates `McpToolResult.content` through `MediaProcessor`, emits MediaExtracted/Processed/Stored/Failed as before). 7 invoke integration tests pass unchanged. Net: invoke.rs +98/-43 LOC (decoupling, not shrinkage — true LOC reduction lives in S16+ when dispatch() activation moves the outer select + media pipeline into nika-runtime). |
| S15-A6 | `38ee31418` | **verb-fetch: `run_with_retry` wrapper + `RetryPolicy`.** Forward-investment helper (~140 LOC + 7 tests). `RetryPolicy { max_attempts (floored to ≥1), base_delay_ms, multiplier, deadline }` with `new()` + `with_deadline()` builder (invariant #19). `run_with_retry(input, caps, event_log, policy)` loops over `run()` with `safe_backoff_delay` (migrated in S14-β) between attempts, racing each sleep against `caps.cancel` so cancelled tasks exit within one sleep interval. `is_retryable(err)` classifies 429 + 5xx + HttpError::Timeout/Connection as retryable, exhaustive match over `VerbFetchError` variants (same-crate). Failure modes map to S14-γ's `RetryExhausted` / `DeadlineExceeded`. Not wired — engine bridge's retry loop stays because it also consumes FetchAux (cache, robots, rate limiter, cookies) which is not a kernel trait yet (S16+ target). |

### Verification ritual

Every commit passed the S12-G3 sacred ritual:

```
cargo check --workspace                        # 0 errors
cargo check --workspace --no-default-features  # 0 errors
cargo test --workspace --lib                   # 0 failures
cargo clippy --workspace --all-targets         # 26 warnings (baseline, unchanged)
```

### Post-S15 measurements

**Engine LOC:** ~146,600 → **~146,650** (S15 net: +55 invoke.rs, +380 new `mcp_pool_adapter.rs`, −41 runtime dispatch, ≈+395 total. Shrinkage still lives in S16+ when dispatch() activation starts.)

**Crate count:** **35 total** (unchanged — no new crates in S15; 32 diamond-participating + 3 outside: napi, py, macros).

**Tests:** ~10,900 → **~10,897** (+8 kernel/mcp, +11 kernel-mock/mcp, +6 engine adapter, +7 verb-fetch retry, +2 verb-fetch retry helpers = ~+34 new; a handful of unrelated tests were deleted or merged in the process so the count is net ≈10,897).

**Verb crate matrix post-S15:**

| Crate | Tests | Bridge live? | dispatch arm | `run_with_retry` wrapper? |
|-------|-------|--------------|---------------|----------------------------|
| `nika-verb-exec` | 13 | YES (S13-B2) | NotImpl | n/a |
| `nika-verb-fetch` | 37 | partial (helpers + retry wrapper) | NotImpl | YES (S15-A6, unwired) |
| `nika-verb-invoke` | 6 | partial (builtin only, MCP through adapter) | NotImpl | n/a |
| `nika-verb-infer` | 10 | **NO** (W14-B2 deferred) | NotImpl | n/a |

### Follow-ups carried to S16+

(Items 2 and 7 from the original S15 list were cleared in Session
16 — see the S16 block below — and have been removed from this
list. The post-S16 carried state is:)

1. **`NoopMcpPool` / `NullBlobStore` / `NullHttpClient`** still exist in the builtin branch of `invoke.rs` (lines ~149-180 struct defs, ~350/357/360 constructor sites). Removing them requires a real `BlobStoreAdapter` + `HttpClientAdapter` in `nika-engine` — out of S15 scope; still open post-S16.
2. **Engine fetch retry loop migration to `verb-fetch::run_with_retry`** — blocked on kernel `FetchAux` trait (robots.txt, rate limiter, cookie jar, ETag cache).
3. **Dispatch() activation (Wave D)** — all 5 arms still return `NotImplemented` because template resolution + binding + skills + spotlight live in engine.
4. **Wave C (`nika-verb-agent`)** — 9 TEMP engine deps in `rig_agent_loop/` untouched.
5. **`McpPoolAdapter` as `TaskExecutor` field** — currently rebuilt per `run_invoke` call. Minor perf cleanup, zero correctness impact.

## Constellation Session 16 — 2026-04-11

S16 landed 7 commits — 1 S15.5 hotfix, 1 deterministic-flake fix,
4 W16 refactor/test commits, and 1 drive-by cleanup. The headline
item from the S16 mega-prompt (**W14-B2** — route the engine's
minimum infer path through `nika_verb_infer::run()`) was
**explicitly scope-reduced to Option A** after Phase 1 review
discovered the engine has no "non-streaming text path" for the
verb crate to pick up: every real engine infer path uses
`infer_stream_with_options` or `infer_vision`, and the verb
crate's `run()` only covers non-streaming `Provider::infer`. The
bridge flip therefore requires streaming support in the verb
crate, which is multi-session work. S16 instead delivered the
mechanically-possible subset that makes the eventual flip safer:
invariant #24 single-helper emission across engine + verb crate,
kernel plumbing for `finish_reason_raw` (closing S15 debt #7),
and the engine-side golden oracle that will catch bridge-flip
regressions when they happen.

### Phase 1 (4-agent parallel review) findings

All four agents (`rust-architect`, `code-explorer`,
`rust-async-expert`, `rust-pro`) ran before any code was written,
per the S12-G1/G2/G3 multi-session refactor protocol. Key
findings that reshaped the mega-prompt's Option A design:

1. **`provider.infer()` is never called from engine `infer.rs`.**
   All 6 provider call sites go through `infer_stream_with_options`,
   `infer_stream`, `infer_vision`, or the `make_infer_callback`
   closure that wraps `infer_with_options` for L3/L4 structured
   retries. The non-streaming path the mega-prompt assumed the
   verb crate could pick up DOES NOT EXIST in today's engine.
   Scope-reducing the bridge flip was the correct call.
2. **Cost computation divergence.** Engine computes `cost_usd`
   four different ways (hourly endpoint, streaming-cache-aware,
   non-streaming, vision). Verb crate reads
   `response.cost_usd.unwrap_or(0.0)` — `None` from `RigProvider`
   today. The shared helper MUST take `cost_usd: f64` as an
   explicit primitive, not via an `InferResponse` field, so each
   engine call site can pre-sanitize via
   `if cost.is_finite() { cost } else { 0.0 }` and pass the
   already-computed value.
3. **`RigProvider: Provider` impl already exists.** At
   `kernel_bridge.rs:258`. No adapter construction needed. The
   eventual W14-B2 flip can coerce `self.provider.clone()` to
   `Arc<dyn Provider>` directly.
4. **Zero `parking_lot::RwLock*Guard` in `infer.rs`** — grep
   returned no hits across all 2157 lines. Invariants #1/#16 not
   at risk. All 7 `ProviderResponded` emission sites have zero
   `.await` between response materialization and the emit, so a
   synchronous helper is safe.
5. **Test coverage gap** — the S14-δ golden oracle lives only in
   `nika-verb-infer`. The engine's mock fast-path at
   `infer.rs:621` bypasses the Provider trait entirely and
   synthesizes the response inline, so no existing test catches
   silent drift on the engine side.

### S16 commits (7 + 1 drive-by)

| Commit | Hash | Purpose |
|--------|------|---------|
| S15.5 hotfix | `800cd2683` | **3 post-S15 review P1 fixes.** (a) `MockMcpPool` docstring at `nika-kernel-mock/src/mcp.rs:9` promised a `cancelled()` fixture ctor that didn't exist — cancellation is per-call via `opts.cancel`, not pool state. Docstring rewritten + pattern pointer to `cancelled_token_returns_cancelled_error` test. (b) `McpPoolAdapter::new` took `Arc<McpClientPool>` + `Arc<EventLog>` — both inner types are `#[derive(Clone)]` with internal `Arc`-backed state, so the outer `Arc<…>` wrap was pointless double indirection. Struct fields + ctor now take owned values, `invoke.rs:569` drops the `Arc::new(...clone())` wrap. `inner()` accessor returns `&McpClientPool` instead of `&Arc<McpClientPool>`. `use std::sync::Arc` removed from the adapter (now unused). (c) Comment added at `invoke.rs:834` explaining the double-cancel path: adapter's internal `biased; tokio::select!` catches cancel first via `McpError::Cancelled`, outer `tokio::select!` at the engine level exists only to abort cancellation that arrives DURING the post-adapter media pipeline iteration (`MediaProcessor::process_all`, engine-owned). Both paths produce identical `NikaError::TaskCancelled` so the double check is benign but load-bearing. |
| S16-flake-fix | `c5b2ed999` | **Deterministic `SecretStore` state bleed fix.** Pre-W16 baseline verification hit 11 failures in `nika-engine` including `test_auto_fallback_to_groq` reporting `left: "anthropic", right: "groq"`. Root cause: `RigProvider::auto()` → `has_provider_key(p)` → `store::resolve_env(env_var)` checks the in-process `DashMap` `SecretStore` BEFORE falling back to `std::env::var`. `SecretStore` is populated by `secrets::fallback::load_from_daemon_or_fallback` on first call from provider credentials in `~/.nika/secrets/vault.enc` (dev machines with real vault). `std::env::remove_var("ANTHROPIC_API_KEY")` does NOT clear the `SecretStore`, so subsequent fallback-chain tests see cached provider credentials and the auto-detect returns the wrong provider. **Fix:** `clear_all_provider_env_vars` in `nika-engine/src/provider/rig/tests.rs` now iterates a `PROVIDER_KEYS` const and removes every entry from BOTH `std::env` AND `secrets::store`. `test_rig_provider_auto_detects_claude` was also cleaned: uses the full helper (not the ad-hoc subset that missed `GEMINI_API_KEY`/`XAI_API_KEY`) and cleans up its own `ANTHROPIC_API_KEY` set_var on exit. Test-helper-only commit; no production changes. **Per the "never skip flakes" rule** — landed as its own commit ahead of W16-A0 so the baseline is known-green. |
| W16-A0 | `0f5cc1e9a` | **Extract `emit_provider_responded` helper + route engine through it.** New `pub fn` in `tools/nika-verb-infer/src/emit.rs` (236 LOC incl. 3 unit tests) takes 8 primitive params matching the `EventKind::ProviderResponded` variant shape 1:1 — `event_log`, `&Arc<str> task_id`, `request_id`, `input_tokens`, `output_tokens`, `cache_read_tokens`, `ttft_ms`, `finish_reason`, `cost_usd`. All 7 engine sites at `infer.rs:621/1156/1330/1388/1527/1592/1898` + the existing verb-crate site at `lib.rs:155` now route through the helper. `nika-engine` gains a `# TEMP (W16-A0)` dep on `nika-verb-infer` (sideways L2, verified no cycle via `cargo tree -p nika-verb-infer \| grep nika-engine` → empty). Helper is synchronous (no `.await`) — Phase 1 rust-async confirmed zero `.await` between response materialization and emit at every site. Tests use `let … else { panic!() }` destructures to avoid the `wildcard_enum_match_arm` clippy lint. Invariant #24 (S14.5) satisfied: future field additions now need to touch exactly 1 helper definition + 8 destructure-forced call sites instead of 8 independent inline struct literals that could drift independently. Engine LOC +35 net (helper replacements are a line or two longer than inline literals); W16-A0 is a refactor for invariant #24, NOT a shrinkage target. |
| W16-B1 | `93de89802` | **Engine-side golden oracle `test_run_infer_mock_emits_provider_responded_with_all_fields`.** New test in `nika-engine/src/runtime/executor/tests.rs` exercises the `TaskExecutor::run_infer` mock fast-path at `infer.rs:621` via the real `TaskExecutor` entry point (not through the Provider trait). Destructures all 8 `ProviderResponded` fields and pins (a) exactly 1 event emitted, (b) `task_id` exact match, (c) `request_id == Some("mock-request")` sentinel, (d) `input_tokens > 0` and `output_tokens > 0` (estimator internal, don't over-specify), (e) `cache_read_tokens == 0`, (f) `ttft_ms == Some(0)` (mock contract — `None` would break TUI rendering), (g) `finish_reason == FinishReason::Mock` (distinct from `Stop`/`EndTurn`), (h) `cost_usd == 0.0_f64` exact. The full destructure is the forcing function for invariant #24: future variant additions must update both the verb-crate golden (`nika-verb-infer::infer_emits_provider_responded_with_all_fields`) AND this engine-side mirror. Phase 1 rust-pro flagged this as the single biggest regression gap on the post-W16-A0 baseline. |
| W16-B3 | `34c33587b` | **`finish_reason_raw: Option<String>` on `InferResponse` + option (ii) mapping.** Kernel: `nika-kernel/src/provider.rs` — add `pub finish_reason_raw` to the `#[non_exhaustive]` `InferResponse` struct, defaulted to `None` by `InferResponse::new` so existing call sites (`kernel_bridge.rs::text_response`, S14-δ golden, MockProvider stream synth) compile unchanged. Kernel-mock: `nika-kernel-mock/src/provider.rs` — the stream-synthesis path at the mock's `infer_stream` impl now propagates `response.finish_reason_raw.clone()` into `InferEvent::Done` so tests that enqueue a response with a raw string see it in BOTH the non-streaming and streaming paths symmetrically. Verb crate: `stop_reason_to_finish_reason` signature changes from `(reason: &StopReason)` to `(reason: &StopReason, finish_reason_raw: Option<&str>)` and implements **option (ii)** from the Phase 1 rust-async audit — typed `StopReason` variants (`EndTurn`, `MaxTokens`, `StopSequence`, `ToolUse`) stay authoritative and ignore the raw string; `ContentFilter` + `Unknown(s)` prefer the external raw string when present and fall back to the hardcoded `"content_filter"` / internal `s` otherwise. The `run` call site at `lib.rs:~162` now passes `response.finish_reason_raw.as_deref()` to the mapping. 5 new verb-crate tests: `infer_content_filter_prefers_finish_reason_raw`, `infer_content_filter_defaults_when_no_raw`, `infer_unknown_stop_reason_prefers_external_raw_over_internal`, `infer_unknown_stop_reason_uses_internal_when_no_raw`, `infer_typed_stop_reasons_ignore_finish_reason_raw`. Closes S15 review debt #7. |
| W16-A1 | `9967f5bfd` | **5 verb-crate test coverage gap closers** from Phase 1 rust-pro audit. `infer_defaults_cost_usd_to_zero_when_response_has_none` (S14-δ golden only tested Some path), `infer_empty_string_system_produces_user_only_request` (pins the `!system.is_empty()` guard in `build_infer_request`), and 3 `ProviderError` variant propagation tests (`Api`, `AuthFailed`, `ModelNotFound`). Each error test uses a typed `matches!` matcher pinning the EXACT inner variant with field-value assertions — an earlier draft used a looser `matches!(Err(VerbInferError::Provider(_)))` which the Socratic review caught as insufficient (would not catch a hypothetical mapping-layer bug that folds variants together). Shared `assert_no_provider_responded(&event_log)` helper pins the "failed provider call does NOT emit ProviderResponded" contract. **NOT added (scope-gated):** `stop_sequences` pass-through (field doesn't exist on `InferInput` — would need a feature addition outside W16 scope) and mid-await cancellation (same select arm as pre-cancel, zero additional coverage, deferred to S17+ when MockProvider grows a yielding ctor). |
| drive-by | `9a60c681e` | **Delete tautological `test_theme_dark_is_default`** in `nika-tui/src/theme/mod.rs:524`. Baseline clippy had been flagging `unused variable: dark` across every S16 verification step; inspection revealed the test was not just unused-variable but literally tautological (`let dark = Theme::dark(); let dark = Theme::dark(); assert_eq!(dark.background, dark.background)` — x == x after shadowing). The adjacent `test_theme_dark_is_same_as_default` already covers the intended "dark equals default" check via field-by-field equality. Per the "fix every problem you see" rule locked in during S16 — a test that asserts nothing is as much of a silent regression risk as an env-var leak. Clippy baseline drops from 26 → 24 (both the inline warning AND the "nika-tui lib test generated 1 warning" summary vanish). |

### Verification ritual

Every S16 commit passed the S12-G3 sacred ritual:

```
cargo check --workspace                        # 0 errors
cargo check --workspace --no-default-features  # 0 errors
cargo test --workspace --lib                   # 0 failures
cargo clippy --workspace --all-targets         # 26 → 24 warnings (drive-by improvement)
```

### Post-S16 measurements

**Engine LOC:** 146,839 → **147,020** (+181). W16-A0 helper replacements
are 1-2 lines longer per site than inline struct literals, W16-B1 adds
a ~125 LOC new engine test, the drive-by deletes ~8 lines. Net positive
LOC is correct for W16-A0 — the commit was about invariant #24
satisfaction, not shrinkage. True shrinkage still lives in W14-B2 when
streaming support lands in the verb crate and the engine delegates
execution paths wholesale.

**Crate count:** **35 total** (unchanged — 32 diamond-participating + 3
outside). No new crates in S16; `nika-engine` gains a sideways dep on
the existing `nika-verb-infer`.

**Tests:** ~10,897 → **10,911** post-parallel-run (+14 net: +3 emit
helper + 1 engine golden + 5 verb-crate W16-B3 + 5 verb-crate W16-A1,
minus the 1 deleted tautological theme test).

**Clippy:** 26 → **24** (drive-by improvement from the deleted theme
test). The 24 remaining warnings are a pre-existing baseline across
`nika-sdk`, `nika-kernel`, `nika-kernel-mock`, `nika-verb-fetch`,
`nika-verb-invoke`, and `nika-macros` test fixtures — scope-gated.

**Verb crate matrix post-S16:**

| Crate | Tests | Bridge live? | dispatch arm | `run_with_retry` wrapper? | Uses `emit_provider_responded`? |
|-------|-------|--------------|---------------|----------------------------|----------------------------------|
| `nika-verb-exec` | 13 | YES (S13-B2) | NotImpl | n/a | n/a |
| `nika-verb-fetch` | 37 | partial (helpers + retry wrapper) | NotImpl | YES (S15-A6, unwired) | n/a |
| `nika-verb-invoke` | 6 | partial (builtin only, MCP through adapter) | NotImpl | n/a | n/a |
| `nika-verb-infer` | **23** | **NO** (W14-B2 deferred to S17+) | NotImpl | n/a | **YES — canonical helper (W16-A0)** |

### Follow-ups carried to S17+

1. **W14-B2 proper** — the bridge flip that routes the engine's
   non-streaming text path through `nika_verb_infer::run()`.
   Blocked on streaming support in the verb crate (the engine has
   no non-streaming call site today). Multi-session refactor.
2. **`NoopMcpPool` / `NullBlobStore` / `NullHttpClient`** still in
   the builtin branch of `invoke.rs`. Removing needs real
   `BlobStoreAdapter` + `HttpClientAdapter`.
3. **Engine fetch retry loop migration** to
   `verb-fetch::run_with_retry` — blocked on kernel `FetchAux` trait.
4. **Dispatch() activation (Wave D)** — all 5 arms still
   `NotImplemented`.
5. **Wave C (`nika-verb-agent`)** — 9 TEMP engine deps in
   `rig_agent_loop/` untouched.
6. **11 exec/runner load-induced test flakes** — see
   `project_s17_followup_exec_runner_load_flakes` memory. Not the
   same class as the S16-flake-fix `SecretStore` bleed (which was
   deterministic and fixed on the spot). These 11 fail only under
   heavy concurrent `cargo test` + `clippy` + `check` contention;
   root cause is subprocess spawn + tokio runtime starvation under
   hardcoded 10s timeouts. Scope-gated because the fix is a
   clock+subprocess mocking refactor that falls out of W14-B2/C
   naturally. Masking with `#[ignore]` or bumping timeouts would
   hide the pathology.
7. **S16 W16-B1 engine oracle + `finish_reason_raw`** — the
   engine-side mock path hardcodes `FinishReason::Mock` so the new
   golden oracle does not yet exercise the `finish_reason_raw`
   code path. A content-filter raw-path engine golden would need a
   non-mock provider injection in `TaskExecutor::new`, which is
   S17+ scope.

### S16 sacred invariant additions

None. S14.5 invariants #23/#24/#25 covered every S16 architectural
concern. The only feedback memory added was the deterministic vs
load-flake class separation in `feedback_never_skip_flakes` and
`project_s17_followup_exec_runner_load_flakes` — process rules,
not architectural.

## Module Map

### Top-level (`src/`)

| Module | LOC | Responsibility |
|--------|-----|---------------|
| `lib.rs` | ~100 | Public API surface |
| `error.rs` | ~2900 | `NikaError` enum, NIKA-XXX codes, `FixSuggestion` trait |
| `error_domains.rs` | ~250 | Domain sub-enums: `ExecutionError`, `ProviderError`, `BindingError`, `DagError` |
| `config.rs` | ~490 | Configuration types |

### `ast/` — Three-Phase Pipeline (~22k LOC)

```
YAML → Raw AST → Analyzed AST → Lower (Runtime Types)
```

| File | LOC | Purpose |
|------|-----|---------|
| `lower.rs` | ~2900 | Phase 3. Converts Analyzed AST into runtime-ready types |
| `action.rs` | ~1900 | Action/task conversion |
| `agent.rs` | ~1400 | Agent task conversion |
| `invoke.rs` | ~490 | Invoke verb conversion |
| `workflow.rs` | ~1030 | Workflow-level conversion |
| `schema_validator.rs` | ~1060 | Schema validation |
| `import_loader.rs` | ~1200 | `include:` partial workflow loading |
| `tests_200_workflows.rs` | ~10k | Comprehensive YAML parsing tests |

Phase 1 (parser) and Phase 2 (analyzer) live in `nika-core`. The analyzer was
split in Session 3 from a single 5531-LOC `analyze.rs` into 6 files under
`nika-core/src/ast/analyzer/analyze/` (mod, cycle_detection, task_table,
validation, verb_analysis, tests).

### `binding/` — Data Flow Engine (~11k LOC)

| File | LOC | Purpose |
|------|-----|---------|
| `template/mod.rs` | ~2050 | `{{with.x \| transform}}` resolution, spotlight wrapping |
| `template/tests.rs` | ~2890 | Template resolution tests |
| `resolve.rs` | **~3950** | `$task_id.path` resolution, null coalescing (`??`) |
| `token_budget.rs` | ~199 | Budget enforcement bridge (pure estimation in nika-core) |

**Moved to nika-core in S21:** `jsonpath.rs` (480 LOC), `mention.rs` (851 LOC),
`validate.rs` (355 LOC), `token_budget` pure estimation (226 LOC). Engine keeps
thin re-exports + `token_budget::enforce_budget()` bridge.

**Note:** `template.rs` was split into `template/mod.rs` + `template/tests.rs`
in Session 2 (was 4938 LOC monolith).

**God file:** `resolve.rs` at 3,948 LOC was identified in V2.2 as a new god file.
Target: split in Phase 15 (post-launch).

**Invariant:** All template resolution is trust-aware. Untrusted data is
spotlight-fenced before interpolation.

### `runtime/` — Execution Core

The heart of the engine. Orchestrates the DAG, dispatches tasks, manages
concurrency, and enforces security policy.

#### Runner (`runtime/runner/`)

- `mod.rs` (~2350): `Runner` struct — builds the execution plan, walks the DAG,
  manages `for_each` fan-out, artifact collection, and result assembly.
  Builder pattern with `#[must_use]` on all 10 chainable methods.
- `tests.rs` (~4820): Runner integration tests.

#### Executor (`runtime/executor/`)

One file per verb:

| File | LOC | Purpose |
|------|-----|---------|
| `infer.rs` | ~2320 | LLM generation (streaming + non-streaming, structured output). Post-S16: routes all 7 `ProviderResponded` emission sites through `nika_verb_infer::emit_provider_responded` per invariant #24. |
| `fetch.rs` | ~1400 | HTTP requests (SSRF protection, 9 extract modes) |
| `exec.rs` | ~470 | Shell command execution (blocklist, `\| shell` enforcement). Kernel-bridged to `nika-verb-exec` since S13-B2. |
| `invoke.rs` | ~575 | MCP tool calls + builtin routing. Post-S15-A5 the MCP path delegates to `McpPoolAdapter`; post-S16-swarm S15.5 the adapter takes owned types, not double-Arc. |
| `agent.rs` | ~600 | Multi-turn agent loop setup |
| `verbs.rs` | ~760 | `dispatch_verb()` router |
| `decompose.rs` | ~350 | Task decomposition helpers |
| `mod.rs` | ~970 | Executor types and dispatch logic |
| `tests*.rs` | ~6700 | Shield, wiremock, and E2E tests |

**Note:** `extract.rs` was deleted in Session 12 (S12-F7/F8) when
HTML extraction moved into the standalone `nika-extract` L2 crate
— the executor no longer owns a file by that name. Earlier
revisions of this table carried a phantom `extract.rs | ~1330`
entry that was incorrect post-S12.

**Key file:** `infer.rs` (~2160 LOC) contains the 4-layer structured output
pipeline and provider auto-retry logic.

#### Builtin Tools (`runtime/builtin/`) — Split Across Two Crates

63 tools accessible via `invoke: nika:*`. Phase 12 (Constellation) split them:

**nika-builtin crate (37 tools, ~10k LOC, 264 tests):**
- Core (7): sleep, log, emit, assert, complete, prompt, run
- File (5): read, write, edit, glob, grep
- Data (13): jq, map, filter, group_by, chunk, token_count, enrich, zip, set_diff, json_merge, json_diff, tree_data, inject
- Data Sprint 2 (6): json_verify, yaml_validate, locale_lookup, aggregate, json_flatten, json_unflatten
- Introspection (6): cost, dag_info, threads, task_status, records, orchestrate
- Bridged to engine via `KernelToolAdapter<T>` (kernel BuiltinError -> NikaError)

**Still in nika-engine runtime/builtin/ (26 tools):**
- `router.rs` — Dispatch by tool name (sealed trait, `BuiltinToolRouter`)
- `trait.rs` — Engine `BuiltinTool` trait + `KernelToolAdapter<T>` bridge
- `media/` — 25 media tools via MediaToolAdapter (import, decode, thumbnail, chart, etc.)
- `fetch_tool.rs` — nika:fetch (SSRF + extract, coupled to PolicyEnforcer)
- `rig_adapter.rs` — `NikaBuiltinToolAdapter` wraps BuiltinTool as rig ToolDyn

#### Security Layer (`runtime/shield.rs`, `canary.rs`, `spotlight.rs`)

6-layer prompt injection defense (Nika Shield):

- `shield.rs` (~200) — `SecurityContext` aggregate wrapping `SpotlightFence` + `CanarySystem`
- `canary.rs` (~300) — 3 random 16-char canary tokens per run, suffix-injected
- `spotlight.rs` (~140) — `wrap_untrusted()` with randomized fence IDs
- `security.rs` (~2470) — Command blocklist, env validation, NIKA-053

**Invariant:** Trust propagated via `task_local!` in `runner.rs`. Never passed
as function argument to `BuiltinTool::call`.

#### Other Runtime Files

| File | LOC | Purpose |
|------|-----|---------|
| `structured_output.rs` | ~2030 | 4-layer structured output engine |
| `artifact_processor.rs` | ~2770 | Persist task outputs to files |
| `task_dispatch.rs` | ~1110 | Task dispatch orchestration |
| `chat_workflow.rs` | ~1300 | `nika chat` interactive mode |
| `boot.rs` | ~1230 | Startup: env validation, feature detection |
| `policy.rs` | ~1260 | Token budget enforcement, rate limiting |
| `limit_tracker.rs` | ~990 | Cost/time limit enforcement |
| `output.rs` | ~1010 | Output formatting and assembly |
| `skill_injector.rs` | ~540 | `skills:` auto-injection into system prompts |
| `spawn.rs` | ~710 | `nika:run` nested workflow execution |
| `resolve_typed.rs` | ~610 | `Templatable<T>` resolution (65 typed fields) |
| `for_each.rs` | ~525 | Parallel loop execution (`concurrency:`, `fail_fast:`) |
| `context_loader.rs` | ~670 | Workflow `context:` file loading |
| `structured_retry.rs` | ~350 | Schema validation retry + LLM repair |
| `resolver.rs` | ~950 | Binding resolution orchestrator |
| `mock_json.rs` | ~245 | Mock provider deterministic responses |
| `output_scanner.rs` | ~215 | Post-execution output analysis |
| `orchestrate.rs` | ~340 | `nika:orchestrate` sub-DAG routing |

### `provider/` — LLM Providers (~10k LOC)

- `rig/` — Cloud providers via rig-core (Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI)
- `native/` — Local GGUF inference via mistral.rs (~2270 LOC, opt-in feature)
- `endpoints.rs` — Custom OpenAI-compatible endpoints (vLLM, TGI, Ollama)
- `cost.rs` — Token cost estimation per model

#### Kernel Bridge (`provider/rig/kernel_bridge.rs`)

725 LOC. `impl nika_kernel::provider::Provider for RigProvider` — bridges the
kernel Provider trait (L0.5) to the concrete RigProvider enum. The `dispatch_rig!`
macro lives inside the trait impl (connect, don't delete pattern).

`TaskExecutor::get_dyn_provider(name)` returns `Arc<dyn Provider>` — the
keystone method that verb crates (Phase 12+) will consume instead of using
RigProvider directly.

### `dag/` — DAG Validation (~3680 LOC)

- `flow.rs` (~1840) — Topological sort, cycle detection
- `stable.rs` (~430) — Stable DAG serialization (petgraph-backed, TUI)
- `validate.rs` (~1410) — DAG validation rules

### `store/` — Runtime State

- `run_context.rs` (~2000): `RunContext` — DashMap-backed task result store,
  context/inputs storage, media staging, workspace root (`OnceLock`).
- `record_writer.rs` (~260): NDJSON record serialization.
- `context.rs`: `LoadedContext` for workflow `context:` block.

### `lsp/` — Language Server (opt-in, ~12k LOC)

- `server.rs` (~970) — LSP server lifecycle
- `model_intel.rs` (~1510) — Model capability intelligence
- `handlers/` (~7450) — 8 handler files (completion, hover, definition, code_action, etc.)
- `ast_index.rs`, `conversion.rs`, `document_store.rs` — Supporting infrastructure

### `display/` — Re-exports

- `mod.rs` (5 LOC) — Re-exports from `nika-display` crate (extracted in S1).

### Other Modules

| Module | LOC | Purpose |
|--------|-----|---------|
| `tools/` | ~150 | ToolContext + PermissionMode only (check_path_readable + submit_tool removed in S11) |
| `io/` | ~2680 | Atomic file I/O, security checks, template I/O |
| `core/` | ~2290 | Internal re-exports, MCP config, paths, storage backend |
| `new/` | ~2600 | `nika new` workflow scaffolding + templates |
| `secrets/` | ~1080 | Daemon IPC + vault fallback |
| `source/` | ~6 | Source span re-exports |
| `util/` | ~860 | Constants, fs helpers, string interner |
| `mcp/` | ~14 | MCP re-exports |
| `event/` | ~5 | Event re-exports |
| `media/` | ~4880 | Media E2E tests |

## Key Invariants

1. **AST Pipeline:** Always Raw -> Analyzed -> Lower. Never skip phases.
2. **Trust Propagation:** Via `task_local!` in runner, not function arguments.
3. **Error Type:** Always `NikaError` with NIKA-XXX codes, never `anyhow`.
4. **Timeout Unit:** Always seconds at the API boundary (parser converts to ms).
5. **Binding Prefix:** `with: { alias: $task_id }` — `$` prefix required.
6. **MCP Naming:** `server::tool_name` (double colon), never slash.
7. **Tests:** `cargo test --lib` only (no keychain popups).
8. **Provider Access:** Downstream crates use `Arc<dyn Provider>` via `get_dyn_provider()`, never `RigProvider` directly.
9. **Effect Traits:** Every side effect (HTTP, FS, exec, blob, clock) goes through a nika-kernel trait so it can be mocked.
10. **Zero unwrap:** New code must not use `.unwrap()` in production src (CI ratchet enforced per V2.3).

## Historical Scaffolding

These exist but are not yet fully wired:

- `error_domains.rs`: 4 domain sub-enums with `From` impls ready, but most call
  sites still construct `NikaError` directly. Target: promote to primary error
  path (Phase 6, ~16h per V2.2 estimate).
- `EventEmitter` trait: defined with 27 structs and 351 emit sites. Partially
  wired — blanket impl for `Arc<T>` shipped in Session 2 (Phase 5.1). Events
  still go through `EventLog` directly in most paths.

## Constellation Refactor — Current State

### Already Extracted (Sessions 2-6)

8 new crates created since Session 1:

| Crate | Layer | LOC | Session |
|-------|-------|-----|---------|
| `nika-kernel` | L0.5 | 668 | S3 |
| `nika-kernel-mock` | L0.5 | ~200 | S3 |
| `nika-clock` | L1 | ~120 | S3 |
| `nika-fs` | L1 | ~180 | S3 |
| `nika-blob` | L1 | ~250 | S3 |
| `nika-http` | L1 | ~300 | S3 |
| `nika-exec-runner` | L1 | ~200 | S3 |
| `nika-builtin` | L2 | 6440 | S6 |

Phase 11 (Provider bridge) shipped in Session 4 — `kernel_bridge.rs` (725 LOC)
implements `nika_kernel::provider::Provider` for `RigProvider`, enabling
downstream crates to consume `Arc<dyn Provider>` without rig-core dependency.

Phase 12 (nika-builtin) started in Session 6 — 37/63 builtin tools extracted
into `nika-builtin` crate. Uses `KernelToolAdapter<T>` to bridge kernel
`BuiltinTool` trait (returns `BuiltinError`) to engine `BuiltinTool` trait
(returns `NikaError`). Sealed trait prevents external implementations.

Session 10: engine-side file tools deleted (−4k LOC). Agent loop migrated
to nika-builtin file tools. FileToolAdapter, RigFileTool, FileTool trait
all removed.

Session 11: P0 security fixes from S10 verification (3 regressions: write/edit
trust gate, grep/glob sensitive file skip, ReadTool 50MB DoS pre-check) +
registry NUKE (−3,427 LOC). Engine: 152,219 → 148,792 LOC. Rust-security
agent re-verified all three P0 fixes green.

### Next Phases

| Phase | Target | What | Status |
|-------|--------|------|--------|
| 3 | `nika-macros` | 4 derives + 1 declarative macro (syn 2 + darling) | Done (S5) |
| 12 | `nika-builtin` | 37/63 tools migrated. Remaining 26 (media+fetch) stay in engine. | Substantially complete |
| 13 | verb crates | `nika-verb-{infer,exec,fetch,invoke,agent}` + dedup | Planned |
| 14 | `nika-runtime` | Runner + executor extraction (~30k LOC) | Planned |
| 15 | post-launch | `nika-binding` + `nika-dag` to L0 kernel (~15k LOC) | Planned |

## Constellation Session 17 — 2026-04-11

S17 landed 2 commits targeting the W14-B2 bridge flip. The headline
is: **kernel bridge returns real metadata + simple text infer delegates
to the verb crate**. Phase 1 (4 parallel agents: rust-architect,
rust-async-expert, code-explorer, nika-code-reviewer) confirmed the
S16 finding and recommended the Z+X hybrid approach.

### Approach: Option Z + Option X

- **Option Z (S17-A0):** Fix the root cause of metadata loss. The
  kernel bridge's `Provider::infer()` text path was calling
  `infer_with_options()` which returns only a `String` — losing all
  token counts, request_id, ttft_ms. S17-A0 routes it through
  `infer_stream_with_options()` internally (with a drain channel for
  the chunks we don't need), converting the `StreamResult` into an
  `InferResponse` with real metadata. Added `stream_result_to_infer_response`
  + `finish_reason_to_stop_reason` reverse mapping + 7 tests.

- **Option X (S17-A2):** With Z in place, the engine can safely
  delegate simple text infer to `nika_verb_infer::run()`. The verb
  crate calls `Provider::infer()` via the kernel trait (now with
  real metadata), emits `ProviderResponded` via the shared helper,
  and returns `InferOutput`. Added `map_verb_infer_error` with
  invariant #25 wildcard arm.

### Delegation predicate

```
!has_structured && !has_content && infer.extended_thinking != Some(true)
```

The engine's streaming path stays for: structured output (L2-L3
retry), vision, extended thinking. These paths are unchanged.
Task-level retry in runner.rs handles transient failures for the
delegated simple text path.

### Commit chain

```
d9bee6292 feat(engine): Provider::infer() returns real metadata via streaming (S17-A0)
d049a9aa9 feat(engine): delegate simple text infer to verb crate (S17-A2)
```

### Measurements

| Metric | Baseline (S16) | Post-S17 | Delta |
|---|---|---|---|
| Engine LOC (src tree) | 147,020 | 147,303 | +283 |
| infer.rs LOC | 2,175 | 2,258 | +83 |
| kernel_bridge.rs LOC | ~530 | ~830 | +300 |
| Workspace tests | 10,910 | 10,916 | +6 |
| Clippy warnings | 0 | 0 | 0 |
| Crates | 35 (32 diamond) | 35 (32 diamond) | 0 |

**LOC target (-150) NOT met.** The delegation adds code (predicate +
error mapping + InferCaps construction) without deleting the streaming
path, which must stay for structured/vision/thinking. LOC reduction
comes when the verb crate supports streaming (S18+) and the engine's
streaming text path can be removed wholesale.

### What S17 unblocked

The verb crate matrix is now: exec ✓ live | fetch helpers (unwired) |
invoke builtin + MCP | **infer partially live (simple text delegates,
streaming/structured/vision/thinking stay engine-owned)** | agent ✗.

## Constellation DX-2 — 2026-04-11 (Pure Deletion)

**First engine LOC reduction since S13.** Zero new features, pure
deletion session. Spent 30 minutes finding deletable fossils per
the `feedback_deletion_first_not_abstraction_first` rule, then
deleted them in 4 micro-commits with a 5th commit fixing a flake
caught during the final verification run.

### Commits

```
7e68302f1 chore(engine): delete dead IndexedDag module (-880 LOC)
6ad6229c8 chore(engine): delete dead_code fossils (-63 LOC)
286837278 chore(engine): delete dead rig provider helpers (-51 LOC)
fe146d969 chore(engine): delete validate_exec_command wrapper (-7 LOC)
521d04993 fix(tests): add #[serial] to test_debug_works_for_all_variants
```

### Measurements

| Metric | S17 | DX-2 | Delta |
|---|---|---|---|
| Engine LOC (src tree) | 147,373 | 146,383 | **-990** |
| Tests | 10,916 | 10,913 | -3 (deleted helper tests) |
| Clippy warnings | 0 | 0 | 0 |
| Crates | 35 | 35 | 0 |
| D/A ratio | n/a | ≈∞ | first net-negative since S13 |

### What was deleted

1. **`dag/indexed.rs`** (880 LOC) — `IndexedDag` struct with
   Vec-adjacency + Kahn's algorithm topological sort. Never used
   outside its own file. Engine uses `Dag` (`flow.rs`) for execution
   and `StableDag` (petgraph) for TUI visualization.

2. **7 `#[allow(dead_code)]` fossils** across 7 files — speculative
   fields ("used in 12.12"), tautological tests (comparing `inner()`
   vs `inner_arc()` pointer equality), unused helpers, and test
   fixtures that were really feature-gated but masked with `#[allow]`.

3. **3 dead rig provider helpers** — `supports_native_structured_output`
   (free fn), `RigProvider::supports_vision()`, `RigProvider::supports_thinking()`.
   All called only from tests. The live capability checks go through
   `nika_core::catalogs::ModelCapabilities`.

4. **`validate_exec_command` thin wrapper** — 3-line wrapper around
   `validate_exec_command_with_shell(cmd, false)`. Test sites updated.

### Flake caught and fixed (DX-2 bonus)

Final workspace test run surfaced a 1-test failure:
`test_auto_fallback_to_mistral` returned `"anthropic"` instead of
`"mistral"`. Root cause: `test_debug_works_for_all_variants` set
`ANTHROPIC_API_KEY` without `#[serial]`, racing with the serial
fallback tests. Per `feedback_never_skip_flakes`, fixed on the spot.

### Why this matters

Engine trajectory S12 → DX-2:
```
S12: 148,792
S13: 146,557  (-2,235) ← real reduction
S14: 146,196  (-361)
S14.5: 146,600  (+404)
S15: 146,839  (+239)
S16: 147,020  (+181)
S17: 147,373  (+353)
DX-2: 146,383  (-990)  ← first reduction since S13
```

S14-S17 were all +LOC sessions despite shipping useful scaffolding
(kernel traits, verb crates, invariant #24 helper, delegation). The
"prep-then-defer" pattern was exactly what `feedback_deletion_first`
warned about. DX-2 broke the streak.

Target: 100k LOC. Remaining: **-46,383** from DX-2 baseline.

### Size Targets (V2.3, research-backed)

```
Near-term:           nika-engine <= 100k LOC  (from 149k, after Phase 14) — no hard date
Post-launch:         nika-engine <  80k LOC   (after Phase 15 binding/dag split)
```

Reference: `docs/sprints/CONSTELLATION-V2.3-AGGRESSIVE-TARGETS.md`

Additional V2.3 commitments:
- **blake3 cache** on Analyzed AST boundary (replaces Salsa, 2 weeks vs 2 months)
- **CI ratchet** on `.unwrap()` count — full migration 6-10 weeks
- **nika-macros** — firm: 4 derives, 2-3 weeks for 1 engineer
