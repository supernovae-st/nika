# Session 12 Foundation — Post-Mortem & Session 13 Handoff

> **Status:** Session 12 Foundation COMPLETE — 19 commits landed and pushed to `origin/main`.
> **Range:** `c5ea27438..304b1d3c2` (19 commits, 2026-04-10).
> **Next:** Session 13 — `nika-runtime` crate + 3 verb crates (`exec`, `invoke`, `fetch`).
> **Launch gate:** 2026-05-05 (J-25).

---

## Table of contents

1. [Executive summary](#1-executive-summary)
2. [Complete commit manifest](#2-complete-commit-manifest)
3. [Verified metrics](#3-verified-metrics)
4. [Post-review corrections (G1, G2)](#4-post-review-corrections-g1-g2)
5. [Architecture inventory — what S13 inherits](#5-architecture-inventory)
6. [Known issues & intentional debt](#6-known-issues--intentional-debt)
7. [Session 13 readiness checklist](#7-session-13-readiness-checklist)
8. [Session 13 — detailed scope](#8-session-13-detailed-scope)
9. [Session 13 — commit plan](#9-session-13-commit-plan)
10. [Traps, gotchas & landmines](#10-traps-gotchas--landmines)
11. [Session 13 mega-prompt (drop-in)](#11-session-13-mega-prompt)

---

## 1. Executive summary

### What S12 Foundation accomplished

- **Kernel trait surface extended** with 4 surgical additions (`PolicyChecker`, `HttpClient::send_streaming`, `ShellExecutor` cancellation, `Filesystem → FsRead + FsWrite` split).
- **2 pure crates extracted** from the engine monolith:
  - `nika-policy` (L1) — `PolicyEnforcer` + SSRF helpers (1,263 LOC moved).
  - `nika-extract` (L2) — 9-mode fetch post-processing pipeline (1,327 LOC moved).
- **5 per-verb `Caps` structs** defined in `nika-kernel::caps` (ExecCaps/FetchCaps/InferCaps/InvokeCaps/AgentCaps) — types only, not wired yet; Session 13 threads them through verb functions.
- **5-verb golden regression suite** landed as an oracle for S13/S14 verb extraction (snapshots both workflow lifecycle AND task output content).
- **Post-review:** 2 critical bugs caught and fixed (G1: pipe deadlock + zombie children; G2: golden oracle strengthened).

### Why this matters for S13

Session 13 creates `nika-runtime` (L3) and extracts 3 verb crates. S12 made that mechanical: every trait and type S13 needs already exists in `nika-kernel`. There is **no more "first figure out the design" work** — just wire the pieces.

### Launch gate impact

- **Budget remaining:** ~25 days to launch (J-25 on 2026-04-10).
- **S13 budget:** 10–12h (~15–18 commits).
- **S14 budget:** 17–22h (~21 commits).
- **Slack:** ~18 days of polish after all refactor sessions complete.

---

## 2. Complete commit manifest

### Phase D/P/E (pre-Foundation, landed 2026-04-09)

| # | SHA | Subject |
|---|-----|---------|
| D1 | `3897870e8` | `fix(engine): hard error on agent file tools cwd lookup failure (S12.D1)` |
| D2 | `c92075fe2` | `fix(builtin): reject '..' in glob patterns (S12.D2)` |
| D3/E1 | `2381c491c` | `perf,fix(builtin): cache canonicalized working_dir + hard error (S12.D3+E1)` |
| P1 | `ffdfa770d` | `refactor(kernel,builtin): current_is_tainted() + BuiltinError::denied() (S12.P1)` |
| P2 | `30f7a67a5` | `refactor(builtin): file/limits.rs — unify MAX_*_BYTES on u64 (S12.P2)` |
| P3 | `1c5d48954` | `refactor(builtin): file/test_util.rs shared run_as() test helper (S12.P3)` |

### Phase Foundation (landed 2026-04-10)

| # | SHA | Subject | LOC Δ |
|---|-----|---------|-------|
| F1 | `870571f74` | `feat(kernel): add PolicyChecker trait` | +94 |
| F2 | `2b5908bca` | `feat(kernel): HttpClient::send_streaming + HttpStreamResponse` | +100 |
| F3 | `f235b9913` | `feat(kernel): cancellation support in ShellExecutor` | +142, -45 |
| F4 | `82d43cc17` | `feat(kernel): split Filesystem into FsRead + FsWrite splinters` | +172, -56 |
| F5 | `2d9ae1a3a` | `feat(policy): create nika-policy L1 crate` | +1,409, -48 |
| F6 | `f2632a0b9` | `chore(engine): delete duplicated policy code` | +7, -1,264 |
| F7 | `719b26522` | `feat(extract): create nika-extract L2 crate` | +1,450, -1,307 |
| F8 | `f5068768b` | `chore(engine): delete runtime/executor/extract.rs` | +17, -53 |
| F9 | `c6a0d78bc` | `feat(kernel): add 5 per-verb Caps structs` | +118 |
| F10 | `f288baa43` | `docs(constellation): ARCHITECTURE.md + session12 memory update` | +37, -8 |
| F11 | `2cb4f0bcb` | `test(runtime): golden e2e regression tests for 5 verbs` | +254 |

### Phase Gap fixes (post-review, landed 2026-04-10)

| # | SHA | Subject | LOC Δ |
|---|-----|---------|-------|
| G1 | `8ef810b9f` | `fix(exec-runner): pipe buffer deadlock + kill_on_drop` | +59, -17 |
| G2 | `304b1d3c2` | `test(runtime): golden tests now assert output content` | +128, -85 |

**Total:** 19 commits. All pushed to `origin/main`.

---

## 3. Verified metrics

| Metric | Pre-S12 (S11 HEAD) | Post-S12 Foundation + G1/G2 | Δ |
|---|---|---|---|
| Workspace tests (cargo test --workspace --lib) | 10,780 | **10,805** | +25 |
| nika-engine LOC (src/ only) | 148,792 | **146,452** | −2,340 |
| Workspace crate count | 26 | **28** | +2 |
| Clippy warnings | 0 | **0** | 0 |
| Release binary size | 118 MB | **~118 MB** | 0 |
| New crates | — | `nika-policy` (L1), `nika-extract` (L2) | +2 |

### Diamond layering (verified)

```bash
cargo tree -p nika-policy  | grep nika-engine  →  empty  ✅
cargo tree -p nika-extract | grep nika-engine  →  empty  ✅
cargo tree -p nika-kernel  | grep nika-policy  →  empty  ✅
cargo tree -p nika-kernel  | grep nika-extract →  empty  ✅
```

All new crates compile without `nika-engine` in their dep graph. No circular dependencies.

### Feature flag parity

`nika-extract` features mirror engine `fetch-*` flags and are ON by default:
`fetch-markdown`, `fetch-html`, `fetch-article`, `fetch-feed`, `fetch-sitemap`.

---

## 4. Post-review corrections (G1, G2)

Two bugs slipped into the initial Foundation phase and were caught by an independent review pass before the handoff was written.

### G1 — `fix(exec-runner): pipe buffer deadlock + kill_on_drop`

**Two bugs in one commit** (same file, same section, coupled root cause).

#### Bug 1: Pipe buffer deadlock (severity: **P0 — dormant bomb**)

**Root cause:** F3's refactor of `TokioShell::run()` unified all paths (timeout / cancel / no-timeout) through a single `child_fut` async block. That block read stdout/stderr **sequentially after** `child.wait()`:

```rust
// F3 (BROKEN) — sequential
let status = child.wait().await?;              // blocks forever if pipe full
let stdout = out.read_to_end(&mut buf).await?; // never reached
let stderr = err.read_to_end(&mut buf).await?; // never reached
```

For any command producing more than the OS pipe buffer capacity (~64 KB Linux / ~16 KB macOS), the child process blocks on `write()` waiting for the parent to drain, while the parent blocks on `wait()` waiting for the child to exit → **deadlock**.

**Historical note:** pre-F3, the no-timeout path used `child.wait_with_output()` which internally drains pipes concurrently with wait. F3 regressed this to the buggy pattern. The timeout path had the same bug pre-F3, but it was dormant because `yes` pipe-heavy commands are rare in workflow tests.

**Fix:** use `tokio::try_join!` to poll wait + drain concurrently:

```rust
// G1 (CORRECT) — concurrent drain
async fn drain<R: tokio::io::AsyncRead + Unpin>(
    handle: Option<R>,
) -> std::io::Result<Vec<u8>> { /* read_to_end or empty */ }

let (status, stdout, stderr) = tokio::try_join!(
    child.wait(),
    drain(stdout_handle),
    drain(stderr_handle),
)?;
```

**Regression test:** `large_output_does_not_deadlock` pipes 1 MB through `yes | head -c 1048576` under a 30s timeout. Pre-fix, hangs forever. Post-fix, completes in ~60ms.

#### Bug 2: Zombie children on cancel/timeout/panic (severity: **P0 — resource leak**)

**Root cause:** `TokioShell` never called `cmd.kill_on_drop(true)` on the tokio `Command`. When `tokio::select!` fires a cancel or timeout arm, `child_fut` is dropped — which drops the `tokio::process::Child`. Tokio's `Child` does NOT kill the process on drop unless `kill_on_drop(true)` was explicitly set. Result: the subprocess runs until it finishes naturally or the OS reaps it.

**Fix:** single-line addition: `cmd.kill_on_drop(true);` before `cmd.spawn()`.

**Platform guarantee:** `kill_on_drop` is cross-platform (SIGKILL on Unix, `TerminateProcess` on Windows).

### G2 — `test(runtime): golden tests now assert output content`

**Root cause:** AMEND-1 in the original plan required golden tests to "assert output content AND event sequence." F11 as initially written only snapshotted the workflow lifecycle (`WorkflowStarted → TaskScheduled → TaskStarted → TaskCompleted → WorkflowCompleted`). A verb extraction commit that produced the correct lifecycle but corrupted task output (wrong value, empty string, wrong JSON shape) would have passed all 5 golden tests.

**Fix:** extended `golden_snapshot()` helper to snapshot BOTH lifecycle AND the exact task output from `RunContext::get(task_id).output_str()`. Now snapshots capture real output shape:

```yaml
# exec golden
lifecycle:
  - WorkflowStarted
  - TaskScheduled(greet)
  - TaskStarted(greet)
  - TaskCompleted(greet)
  - WorkflowCompleted
output: hello golden  # ← captures actual stdout
```

```yaml
# invoke golden
output: '{"logged":true,"level":"info","message":"golden invoke fixture"}'
```

Any verb extraction that alters observable output shape will now fail the matching golden snapshot in under a second.

---

## 5. Architecture inventory

### Crate dependency diagram (post-S12)

```
L0     nika-core          AST, PolicyConfig, SecurityPolicyConfig, TaintMode
       nika-event         EventLog, EventKind, TraceWriter
L0.5   nika-kernel        Trait definitions (ShellExecutor, HttpClient, Provider,
                          Filesystem → FsRead + FsWrite, BlobStore, Clock,
                          PolicyChecker — NEW, caps module — NEW)
       nika-kernel-mock   Hand-written mocks for all kernel traits (dev-dep)
L1     nika-clock         SystemClock (tokio::time)
       nika-fs            TokioFs (tokio::fs + globset, FsRead+FsWrite split)
       nika-blob          DiskBlobStore (blake3 CAS)
       nika-http          ReqwestClient (SSRF: IPv4/v6/CGN/metadata)
       nika-exec-runner   TokioShell (blocklist + NFKC + kill_on_drop + concurrent drain)
       nika-policy        ⭐ NEW — PolicyEnforcer, SSRF helpers, TokenBudget, RAII TokenReservation
L2     nika-engine        Monolith (~146k LOC post-S12, still contains runtime/verbs/TaskExecutor)
       nika-builtin       37 builtin tools
       nika-extract       ⭐ NEW — pure 9-mode fetch post-processing (no I/O, no async)
       nika-media         CAS store, image/document ops
       nika-mcp           MCP client
       nika-vault         Encrypted secrets
L3     nika-daemon        Background daemon
L4     nika-cli, nika-tui, nika-serve, nika-sdk, nika-lsp, nika-init, nika-lsp-core, nika-display
L5     nika               Binary entry point
```

### What's NEW in `nika-kernel` (for S13 to consume)

1. **`nika_kernel::policy::PolicyChecker`** — object-safe trait with 4 methods. `nika-policy::PolicyEnforcer` implements it. Verb crates depend on the trait only.

2. **`nika_kernel::http::HttpClient::send_streaming`** — default returns `HttpError::Unsupported`. S13 must implement it in `nika-http::ReqwestClient` for the fetch: 50 MB early-abort.

3. **`nika_kernel::shell::ShellCommand::cancel: Option<CancellationToken>`** — `TokioShell` already honors this (with `kill_on_drop` from G1). Verb crates can pass a runtime-scoped cancel token.

4. **`nika_kernel::filesystem::{FsRead, FsWrite, Filesystem}`** — splinter traits. `Filesystem` is a blanket alias `impl<T: FsRead + FsWrite> Filesystem for T {}`. Verb crates should depend on the narrowest splinter they need.

5. **`nika_kernel::caps::{ExecCaps, FetchCaps, InferCaps, InvokeCaps, AgentCaps}`** — 5 per-verb borrowed-slice capability structs. All `#[non_exhaustive]` so S13 can add fields without breaking consumers.

### Canonical Caps definitions (nika-kernel/src/caps.rs)

```rust
use std::sync::Arc;
use crate::clock::Clock;
use crate::filesystem::{FsRead, FsWrite};
use crate::http::HttpClient;
use crate::policy::PolicyChecker;
use crate::provider::Provider;
use crate::shell::ShellExecutor;
use crate::store::BlobStore;

#[non_exhaustive]
pub struct ExecCaps<'a> {
    pub shell: &'a dyn ShellExecutor,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
    pub fs_read: &'a dyn FsRead,
}

#[non_exhaustive]
pub struct FetchCaps<'a> {
    pub http: &'a dyn HttpClient,
    pub policy: &'a dyn PolicyChecker,
    pub blobs: &'a dyn BlobStore,
    pub clock: &'a dyn Clock,
}

#[non_exhaustive]
pub struct InferCaps<'a> {
    pub provider: Arc<dyn Provider>,  // Arc — providers outlive tasks
    pub fs_read: &'a dyn FsRead,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
}

#[non_exhaustive]
pub struct InvokeCaps<'a> {
    pub fs_read: &'a dyn FsRead,
    pub fs_write: &'a dyn FsWrite,
    pub http: &'a dyn HttpClient,
    pub blobs: &'a dyn BlobStore,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
}

#[non_exhaustive]
pub struct AgentCaps<'a> {
    pub provider: Arc<dyn Provider>,
    pub invoke: InvokeCaps<'a>,  // composition: agent can also invoke
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
}
```

**Design rationale:**
- Fields are borrowed slices (`&'a dyn Trait`) because they're scoped to a single task invocation.
- `provider` is `Arc<dyn Provider>` because providers have async methods that may outlive the task (for streaming, multi-turn).
- `#[non_exhaustive]` allows S13 to add `shield`, `events`, `cancel`, `workflow_base_dir` fields without breaking verb crates.
- `AgentCaps` composes `InvokeCaps` because agents also call tools — reuse the invoke capability surface.

### What's NEW in `nika-policy` (for engine + S13 to consume)

`nika-policy` is the canonical home of `PolicyEnforcer`. Engine accesses it via `pub use nika_policy as policy;` in `runtime/mod.rs`, so all existing `crate::runtime::policy::*` imports continue to work unchanged (zero call-site churn).

Pub exports:
- `PolicyEnforcer` (concrete struct, implements `nika_kernel::policy::PolicyChecker`)
- `PolicyDecision` (local enum, distinct from kernel's)
- `PolicyError` (L1-independent error type)
- `TokenBudget`, `TokenReservation` (RAII guard)
- `is_ssrf_blocked`, `resolve_and_check_ssrf`, `resolve_and_pin_ssrf`, `ssrf_safe_redirect_policy`

`PolicyConfig` itself lives in `nika-core::policy` so both `nika-policy` (enforcement) and future `nika-runtime` (pass-through) can consume it without a cycle.

### What's NEW in `nika-extract` (for S13 `nika-verb-fetch` to consume)

Pure 9-mode extraction pipeline. Zero I/O, zero async, zero locks. Default features enable all modes (`fetch-markdown`, `fetch-html`, `fetch-article`, `fetch-feed`, `fetch-sitemap`).

Public API:
- `nika_extract::extract(body, mode, selector, base_url) -> Result<String, ExtractError>` — canonical entry point.
- `nika_extract::apply_extract_with_base(...)` — internal 4-arg version (kept for call compatibility).
- `nika_extract::parse_link_header_hreflang(&[String]) -> Vec<Value>` — Link header HREFLANG parser.
- `nika_extract::ExtractError` with variants `Failed(String)` and `FeatureDisabled(&'static str)`.

Engine converts to `NikaError` via `impl From<nika_extract::ExtractError> for NikaError` in `nika-engine/src/error.rs`.

---

## 6. Known issues & intentional debt

### Intentional debt (documented, plan-compliant)

1. **`Caps` structs not wired yet** — types only. Session 13 builds `VerbCapabilities` in `nika-runtime` with accessor methods returning borrowed slices.

2. **`ReqwestClient::send_streaming` not implemented** — default returns `HttpError::Unsupported`. Session 13's `nika-verb-fetch` will implement it when the fetch verb is extracted.

3. **Engine still owns `runtime::task_dispatch`** — per AMEND-4 of the plan, engine's `task_dispatch` remains the live dispatch path during S13. Session 13 builds a parallel `nika-runtime::dispatch` with stub arms. The wiring switch happens in Session 14 Wave C.

4. **`TaskExecutor` still a 22-field god struct** — deletion deferred to Session 14 W14-A0 (a dedicated 3-4h test migration commit).

5. **No verb crates yet** — Session 13 extracts `nika-verb-exec`, `nika-verb-invoke`, `nika-verb-fetch`. Session 14 extracts `nika-verb-infer`, `nika-verb-agent`.

6. **`nika-shield` doesn't exist as a crate** — runtime `SecurityContext` still lives in `nika-engine/runtime/shield.rs`. Extraction is a Session 14 deliverable.

### Intentional design choices worth documenting

1. **F3 cancel semantics:** `tokio::select!` uses `biased;` with the cancel arm first. This means: if a token is pre-cancelled and the child finishes instantly, we return `Cancelled` rather than the completed output. This is defensible — a pre-cancel means "I want to stop"; whether the child happened to finish is irrelevant. The existing test `cancelled_before_start_returns_cancelled` validates this.

2. **`PolicyEnforcer::enforce()` now returns `Result<(), PolicyError>` instead of `Result<(), NikaError>`** — this is intentional. `nika-policy` is L1 and must not depend on `NikaError`. Callers convert via `?` + the `From<PolicyError> for NikaError` impl in `nika-engine/src/error.rs`. Audit confirmed zero engine callers relied on the old signature.

3. **`Filesystem` umbrella trait is a blanket alias** — `pub trait Filesystem: FsRead + FsWrite {} impl<T: FsRead + FsWrite + ?Sized> Filesystem for T {}`. No methods of its own. Any existing `T: Filesystem` bound still works. Audit confirmed zero `dyn Filesystem` consumers in the codebase, so object-safety is moot (but would work anyway since FsRead and FsWrite are both object-safe).

### Non-issues flagged during review and resolved

- **F5 `map_decision` completeness** — all 3 PolicyDecision variants mapped.
- **F6 `pub use` trick** — all 8 pub items from old `runtime::policy::*` still resolve correctly via the re-export.
- **F9 `Arc<dyn Provider>` vs `&dyn Provider`** — intentional asymmetry, documented in module doc.
- **F7 extract purity** — verified: no `reqwest`, no `tokio::fs`, no async, only 2 mechanical `use crate::` imports.

### Remaining open questions (defer to S13 planning)

- **Q1:** Should `ExecCaps` grow a `cancel: &'a CancellationToken` field in S13, or keep cancellation passed separately via `ShellCommand::cancel`? Both work; the Caps field is more ergonomic.
- **Q2:** `AgentCaps` composes `InvokeCaps` by embedding a borrowed `InvokeCaps<'a>`. Does this create lifetime pain in the runtime accessor? Prototype before committing.

---

## 7. Session 13 readiness checklist

- [x] Kernel trait surface complete (PolicyChecker, FsRead/FsWrite, streaming HTTP, shell cancel)
- [x] `nika-policy` crate published, diamond clean
- [x] `nika-extract` crate published, diamond clean
- [x] 5 Caps structs defined in nika-kernel (types only)
- [x] Golden regression suite with output assertions (5 tests)
- [x] TokioShell hardened (no pipe deadlock, no zombies)
- [x] All 10,805 workspace tests pass
- [x] Zero clippy warnings workspace-wide
- [x] Documentation updated (ARCHITECTURE.md, session memory)
- [x] All 19 S12 commits pushed to origin/main
- [x] Engine LOC down to 146,452 (from 148,792)

### What Session 13 does NOT need to build from scratch

- Policy enforcement infrastructure (done in F5/F6)
- Extraction pipeline (done in F7/F8)
- Per-verb capability type shapes (done in F9)
- Regression oracle for verb behaviour (done in F11/G2)
- Shell cancellation primitive (done in F3/G1)
- Streaming HTTP trait surface (done in F2)

### What Session 13 MUST build

See section 8.

---

## 8. Session 13 — detailed scope

### Goal statement

Create `nika-runtime` (L3) as the new execution orchestrator. Extract 3 of 5 verbs (`exec`, `invoke`, `fetch`) into standalone crates. Keep `TaskExecutor` as a bridge during S13 — deletion is S14's job.

### Deliverables

1. **New crate `nika-runtime` (L3)**
   - `VerbCapabilities` bundle (Arc-shared across tasks in a workflow)
   - `VerbCapabilities::exec_caps()`, `::fetch_caps()`, `::invoke_caps()` accessors returning borrowed slices
   - `enum TaskAction { Exec(ExecParams), Fetch(FetchParams), Infer(InferParams), Invoke(InvokeParams), Agent(AgentParams) }` (closed sum, matches the 5 verbs)
   - `pub async fn dispatch(action: TaskAction, caps: &VerbCapabilities) -> Result<Value, RuntimeError>` with 5 match arms (Exec/Fetch/Invoke filled, Infer/Agent as `todo!()` — **parallel** to engine's `task_dispatch`, not live)

2. **New crate `nika-verb-exec`**
   - Depends on `nika-kernel` only (no `nika-engine`)
   - `pub async fn run(action: ExecParams, caps: ExecCaps<'_>) -> Result<Value, VerbExecError>`
   - Engine's `runtime/executor/exec.rs` becomes a thin bridge: `nika_verb_exec::run(params, exec_caps).await.map_err(Into::into)`

3. **New crate `nika-verb-invoke`**
   - Depends on `nika-kernel`, `nika-builtin`, `nika-mcp` (via trait injection for MCP)
   - `pub async fn run(action: InvokeParams, caps: InvokeCaps<'_>) -> Result<Value, VerbInvokeError>`
   - Handles both `nika:*` builtin routing and MCP tool routing
   - Engine's `runtime/executor/invoke.rs` becomes a thin bridge

4. **New crate `nika-verb-fetch`**
   - Depends on `nika-kernel`, `nika-extract`, `nika-policy` (for SSRF redirect policy)
   - `pub async fn run(action: FetchParams, caps: FetchCaps<'_>) -> Result<Value, VerbFetchError>`
   - **Implements `nika-http::ReqwestClient::send_streaming`** for 50 MB early-abort
   - Engine's `runtime/executor/fetch.rs` becomes a thin bridge

5. **Engine updates**
   - Add `nika-runtime`, `nika-verb-exec`, `nika-verb-invoke`, `nika-verb-fetch` as deps
   - `TaskExecutor` verb methods become bridge calls to verb crates
   - `task_dispatch` continues to call TaskExecutor — no behaviour change

6. **Tests**
   - Each new verb crate gets its own unit test suite (not just engine-side)
   - Golden regression suite from S12 must still pass (this is the S13 kill criterion)

### Dispatch strategy during S13 (from AMEND-4)

- `nika-runtime::dispatch()` is built with 5 arms but is NOT called by the Runner yet.
- Engine's `task_dispatch` continues to call `TaskExecutor` verb methods.
- Each `TaskExecutor` verb method becomes a bridge that delegates to `nika_verb_*::run`.
- Session 14 Wave C (W14-D1) switches Runner to call `nika-runtime::dispatch` directly.
- **Consequence:** `todo!()` arms in `nika-runtime::dispatch` during S13 are safe — they are never called at runtime.

### Out of scope for S13 (deferred to S14)

- `nika-verb-infer` extraction (S14 Wave B1)
- `nika-verb-agent` extraction (S14 Wave B2)
- `TaskExecutor` deletion (S14 Wave C)
- `nika-shield` crate extraction (S14 Wave A)
- `rig_agent_loop/` refactor (S14 Wave B)
- Schema bump / new verbs (never — 5 verbs sacred)

---

## 9. Session 13 — commit plan

Target: 15–18 commits, ~10–12h wall clock. Grouped into 3 phases.

### Phase A — Runtime crate foundation (4 commits)

| # | Type | Subject |
|---|------|---------|
| S13-A1 | `feat(runtime)` | Create nika-runtime L3 crate with VerbCapabilities bundle |
| S13-A2 | `feat(runtime)` | Add per-verb Caps accessors (exec_caps, fetch_caps, invoke_caps) |
| S13-A3 | `feat(runtime)` | Add TaskAction enum + dispatch function (parallel, not live) |
| S13-A4 | `test(runtime)` | VerbCapabilities construction + accessor unit tests |

### Phase B — First verb crate (nika-verb-exec, 4 commits)

| # | Type | Subject |
|---|------|---------|
| S13-B1 | `feat(verb-exec)` | Create nika-verb-exec crate with pub async fn run(caps) |
| S13-B2 | `feat(engine)` | TaskExecutor::run_exec bridges to nika_verb_exec::run |
| S13-B3 | `test(verb-exec)` | Happy path + error path unit tests |
| S13-B4 | `chore(runtime)` | Wire dispatch match arm for Exec variant |

### Phase C — nika-verb-invoke (4-5 commits)

| # | Type | Subject |
|---|------|---------|
| S13-C1 | `feat(verb-invoke)` | Create nika-verb-invoke crate with builtin routing |
| S13-C2 | `feat(verb-invoke)` | MCP tool routing via trait injection |
| S13-C3 | `feat(engine)` | TaskExecutor::run_invoke bridges to nika_verb_invoke::run |
| S13-C4 | `test(verb-invoke)` | Unit tests for builtin + MCP branches |
| S13-C5 | `chore(runtime)` | Wire dispatch match arm for Invoke variant |

### Phase D — nika-verb-fetch (5-6 commits)

| # | Type | Subject |
|---|------|---------|
| S13-D1 | `feat(http)` | Implement ReqwestClient::send_streaming with size cap |
| S13-D2 | `feat(verb-fetch)` | Create nika-verb-fetch crate using nika-extract + nika-http |
| S13-D3 | `feat(verb-fetch)` | SSRF redirect policy via nika-policy |
| S13-D4 | `feat(engine)` | TaskExecutor::run_fetch bridges to nika_verb_fetch::run |
| S13-D5 | `test(verb-fetch)` | Wiremock-based integration tests |
| S13-D6 | `chore(runtime)` | Wire dispatch match arm for Fetch variant |

### Phase E — Close (2 commits)

| # | Type | Subject |
|---|------|---------|
| S13-E1 | `docs(constellation)` | Update ARCHITECTURE.md with 4 new crates + memory |
| S13-E2 | `test(regression)` | Verify golden suite still green end-to-end |

---

## 10. Traps, gotchas & landmines

### Rust-specific traps

1. **`tokio::try_join!` borrow-checker trap.** When calling `try_join!(child.wait(), drain(handle_a), drain(handle_b))`, the futures must not alias mutable state. In our TokioShell fix (G1), `stdout_handle` and `stderr_handle` are `take()`-ed out of child first, so drain futures own them. `child.wait()` then has exclusive `&mut child`. If S13 needs to replicate this in verb crates, keep the pattern.

2. **`biased;` in `tokio::select!`** — priorities the first branch deterministically. We use `biased; _ = cancel => err; r = child => r`. This means cancel always wins if ready. If S13 wants "cancel only if child not yet done", put child first: `biased; r = child => r; _ = cancel => err`. Decide case-by-case.

3. **`kill_on_drop(true)`** — MUST be set on every `tokio::process::Command` before spawn, otherwise dropped children become zombies. Verb crates that spawn subprocesses must not forget this.

4. **`Pin<Box<dyn Stream<...>>>` without `Unpin`** — `HttpStreamResponse::body` is this shape. When you poll it, use `Pin::as_mut()` carefully. Consider using `futures::StreamExt` methods that take `self: &mut Self` when `Unpin`, or `self: Pin<&mut Self>` otherwise.

5. **`Arc<dyn Provider>` in caps vs `&dyn`** — the asymmetry is intentional. `Provider` has async methods that cross `.await` points; the `Arc` lives for the entire run. All other caps are scoped to a task and can be `&dyn Trait` borrowed from the bundle.

6. **`#[non_exhaustive]` construction** — verb crates CANNOT build `ExecCaps { shell, policy, clock, fs_read }` directly. They receive it from `VerbCapabilities::exec_caps()`. Internal nika-runtime code CAN build it because `#[non_exhaustive]` only affects external construction.

7. **Engine feature forwarding** — `nika-engine/Cargo.toml` has `fetch-html = ["nika-media/fetch-html", "dep:scraper"]`. `nika-extract` has its own `fetch-html` feature that is ON by default. Do not double-forward (caused cfg confusion in S12). Let the default features handle it unless disabling explicitly.

8. **`parking_lot::RwLockReadGuard` is not `Send`** — if S13 holds a guard across `.await`, it will NOT compile. Use `std::sync::RwLock` if you need Send-across-await, or (preferred) drop the guard before the await.

### Nika-specific gotchas

1. **`cargo test --workspace --lib`** is the ONLY safe test command. `cargo test` (no `--lib`) triggers macOS Keychain popups in nika-vault integration tests. Never use it.

2. **Integration tests in `tools/nika/tests/`** are NOT run by `--lib`. They only run with bare `cargo test` (keychain risk). Place verb crate tests inside the crate (`src/` with `#[cfg(test)]`) or in a separate lib module, NOT in a top-level `tests/` directory.

3. **`quiet()` on Runner** — `runner.run()` is verbose by default. For lib tests, always chain `.quiet()` before `.run()` to avoid polluting test output.

4. **EventLog clone before run** — `EventLog::new()` returns a handle that shares state via `Arc<Mutex<...>>`. Clone it BEFORE passing to Runner if you want to read events after the runner moves it (or don't clone and accept that you can't inspect events post-run).

5. **`parse_analyzed` vs `parse_workflow`** — `nika_engine::ast::parse_analyzed` returns `AnalyzedWorkflow`, which is what Runner accepts. `parse_workflow` returns the raw AST.

6. **`provider: mock` is the only zero-config provider** — use it in all lib tests. Never hardcode `anthropic` or any cloud provider in test fixtures.

7. **Engine's `pub use nika_policy as policy;` re-export** — means all existing `crate::runtime::policy::*` imports in engine code still resolve. When S13 adds verb crates, they can still see `nika_policy::*` directly without going through the engine.

8. **`nika-extract` has no engine dep** — a verb crate that uses both `nika-extract` and bridge engine types must NOT cycle through engine. Verify with `cargo tree -p nika-verb-fetch | grep nika-engine` — should be empty for the new verb crate.

### Process gotchas

1. **One fix = one commit.** The S12 review caught bugs because commits were atomic. Resist the urge to bundle "small cleanups" into a feature commit.

2. **`cargo insta accept --workspace`** — when updating snapshots, always pass `--workspace` to cover all crates. Missing this caused one golden snapshot to not get accepted in S12-F11.

3. **Pre-commit hooks** — auto-format and clippy run on every commit. Don't bypass with `--no-verify`. If a hook fails, fix the underlying issue.

4. **Commit co-author** — ONLY `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`. NEVER add a Claude trailer. This is a sacred invariant.

5. **AGPL header** — every new `.rs` file MUST start with:
   ```rust
   // SPDX-License-Identifier: AGPL-3.0-or-later
   // Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
   ```

6. **Do NOT touch user's parallel work** — `AGENTS.md`, `.github/SECURITY.md`, `CLA.md`, `COMMERCIAL_LICENSE.md`, `CHANGELOG.md`, `README.md`, `MANIFESTO.md`, `CONTRIBUTING.md`, `CONVENTIONS.md`, `editors/*`, `docs/launch/` — these are launch-prep work owned by Thibaut. Changes there must be his.

7. **Push authorization** — all commits land on local `main` and are pushed ONLY after explicit user authorization. Never auto-push.

---

## 11. Session 13 mega-prompt

**Paste this into a fresh Claude Code session when you're ready to start S13.** It is fully self-contained.

```
You are Claude Code starting **Session 13 of the Nika Constellation refactor**.

**Project:** /Users/thibaut/dev/supernovae/nika  
**Launch:** 2026-05-05 (J-25 as of 2026-04-10)

## What happened before you

**Session 12 is COMPLETE and pushed to origin/main** (19 commits, range
c5ea27438..304b1d3c2). It extended the kernel trait surface and extracted
two pure crates (nika-policy L1, nika-extract L2) to make Session 13
mechanical. Full post-mortem:
`docs/plans/constellation-session12-rework/14-session12-handoff-postmortem.md`

**HEAD:** `304b1d3c2 test(runtime): golden tests now assert output content (S12-G2)`
**Tests:** 10,805 (cargo test --workspace --lib)  
**Engine LOC:** 146,452 (target <=100k by Phase 15)  
**Crates:** 28 (+2 new: nika-policy, nika-extract)  
**Clippy:** 0 warnings workspace-wide

## Your job: execute Session 13 (15-18 commits, ~10-12h)

Create **nika-runtime (L3)** plus **3 verb crates** (nika-verb-exec,
nika-verb-invoke, nika-verb-fetch). Keep TaskExecutor as a bridge — its
deletion is Session 14's job. The golden regression suite from Session 12
is your kill criterion: all 5 golden snapshots must still pass after every
commit.

**Phases:**
- Phase A (4 commits): nika-runtime crate + VerbCapabilities bundle
- Phase B (4 commits): nika-verb-exec + engine bridge
- Phase C (4-5 commits): nika-verb-invoke (builtin + MCP routing)
- Phase D (5-6 commits): nika-verb-fetch (includes ReqwestClient::send_streaming impl)
- Phase E (2 commits): docs + regression verification

See section 9 of the handoff doc for the detailed commit plan.

## Mandatory pre-flight (in this order)

```bash
cd /Users/thibaut/dev/supernovae/nika
git log --oneline -5                       # expect: 304b1d3c2 at HEAD
git status                                 # expect: user's launch-prep files modified (do NOT touch)
cd tools
cargo test --workspace --lib 2>&1 | grep -E "^test result" | awk '{s+=$4} END{print s}'
# expect: 10805
cargo clippy --workspace --lib -- -D warnings   # expect: clean
cargo tree -p nika-policy  | grep nika-engine    # expect: empty
cargo tree -p nika-extract | grep nika-engine    # expect: empty
```

## Files you MUST read IN ORDER

**Handoff + plan (source of truth):**
1. `docs/plans/constellation-session12-rework/14-session12-handoff-postmortem.md` — THIS FILE (read fully)
2. `docs/plans/constellation-session12-rework/13-plan-corrections.md` — authoritative amendments
3. `docs/plans/constellation-session12-rework/07-session13-extraction-1.md` — original S13 plan (read with corrections in mind)
4. `docs/plans/constellation-session12-rework/01-architecture-vision.md` — target end-state
5. `docs/plans/constellation-session12-rework/02-adr-001-enum-dispatch.md` — why no trait Verb
6. `docs/plans/constellation-session12-rework/03-adr-002-typed-contexts.md` — why per-verb Caps
7. `docs/plans/constellation-session12-rework/09-risk-register.md` — known landmines

**S12 deliverables (what you inherit):**
8. `tools/nika-kernel/src/caps.rs` — 5 Caps structs (ExecCaps/FetchCaps/...)
9. `tools/nika-kernel/src/policy.rs` — PolicyChecker trait
10. `tools/nika-policy/src/lib.rs` — PolicyEnforcer concrete impl
11. `tools/nika-extract/src/lib.rs` — pure 9-mode extraction pipeline
12. `tools/nika-engine/src/runtime/runner/tests_golden_verbs.rs` — your regression oracle
13. `tools/nika-exec-runner/src/lib.rs` — TokioShell with G1 fixes (study the tokio::try_join! pattern)

**Nika project rules (always follow):**
14. `nika/CLAUDE.md` — project rules
15. `nika/.claude/rules/architecture.md` — diamond layering
16. `nika/.claude/rules/git-workflow.md` — 1 fix = 1 commit

## Skills to announce and use

- `spn-powers:executing-plans` — you're executing a documented plan
- `spn-powers:test-driven-development` — write tests before implementation
- `spn-powers:verification-before-completion` — before every git commit
- `spn-powers:systematic-debugging` — if any test fails
- `spn-rust:rust-core` — trait design, error handling, ownership patterns
- `spn-rust:rust-async` — tokio patterns, Send+Sync across await, Arc<dyn>

## Sacred invariants (NEVER violate)

1. AGPL-3.0-or-later header on every new .rs file
2. Co-author: `Nika 🦋 <nika@supernovae.studio>` — NEVER Claude
3. Tests only via `cargo test --workspace --lib` (no keychain)
4. Zero .unwrap()/.expect() in new production code (tests OK)
5. Diamond layering: new verb crates MUST NOT depend on nika-engine
   (verify: `cargo tree -p nika-verb-X | grep nika-engine` → empty)
6. 1 fix = 1 commit
7. Push only after explicit user authorization
8. No `trait Verb` — enum dispatch only (ADR-001)
9. Verb crates receive Caps by reference (&ExecCaps<'_>), never own them
10. Never touch user's parallel launch-prep files (AGENTS.md, CLA.md,
    CHANGELOG.md, README.md, MANIFESTO.md, editors/, docs/launch/, etc.)

## Session 13 explicit non-goals

- Do NOT extract nika-verb-infer (Session 14 Wave B1)
- Do NOT extract nika-verb-agent (Session 14 Wave B2)
- Do NOT delete TaskExecutor (Session 14 Wave C)
- Do NOT create nika-shield crate (Session 14 Wave A)
- Do NOT touch rig_agent_loop/ (Session 14 Wave B)
- Do NOT bump the schema version (stays @0.12 forever)
- Do NOT wire nika-runtime::dispatch as the live path (S14 W14-D1 does this)
  — parallel construction only, TaskExecutor stays live during S13

## Done criteria (all must be green)

- [ ] nika-runtime crate compiles, passes its unit tests
- [ ] VerbCapabilities bundle has working exec_caps/fetch_caps/invoke_caps accessors
- [ ] TaskAction enum + dispatch() function with 5 arms (3 filled, 2 todo!())
- [ ] nika-verb-exec crate compiles, has its own tests, engine bridges to it
- [ ] nika-verb-invoke crate handles both nika:* builtins + MCP tools
- [ ] nika-verb-fetch crate implements ReqwestClient::send_streaming
- [ ] All 5 S12 golden tests still pass (regression oracle green)
- [ ] `cargo test --workspace --lib` passes with ≥10,805 tests
- [ ] `cargo clippy --workspace --lib -- -D warnings` clean
- [ ] Diamond: nika-verb-exec, nika-verb-invoke, nika-verb-fetch have zero nika-engine dep
- [ ] Engine LOC down to ~143,800 (-2,400 from S13 start of 146,452)
- [ ] Crate count: 28 → 32 (+nika-runtime, +3 verb crates)
- [ ] ARCHITECTURE.md updated
- [ ] Session memory file updated
- [ ] User authorized push
- [ ] `git push origin main` completed

## Workflow

```
Read plans → Create TodoWrite tasks → Execute phase-by-phase →
Report per phase → Continue → Run golden suite after every verb commit →
Close with docs + regression verification
```

Announce skill usage. Create TodoWrite items for each phase. Report after
each phase for user feedback. Use the executing-plans skill. TDD every
commit. Run `cargo test -p nika-engine --lib runner::tests_golden_verbs`
after every verb extraction commit — this is your regression oracle.

**Think hard. Ship clean. The kernel surface is ready for you — every
field, every trait, every Caps struct already has a home. Session 13 is
mechanical if you follow the plan.**
```

---

## Appendix A — Session 12 review notes

### What went right

1. **The 4-agent research was worth it.** Dispatching parallel research agents before F1 caught the original Phase 13 plan's fatal flaw (moving code without extending kernel traits). The reworked plan saved Sessions 13/14 from being LOC-shuffling exercises.

2. **The `pub use nika_policy as policy;` trick.** A 2-file change instead of rewriting 9 consumer files. Preserved all existing call-site syntax.

3. **Per-commit TDD with insta.** The golden tests caught the G2 weakness because I could see the snapshot before accepting it. Without insta, I would have shipped lifecycle-only oracle.

4. **Independent code review.** The feature-dev:code-reviewer agent caught the pipe deadlock (G1) that I missed. Having an independent pass after the work is complete — even though I was confident — prevented a production-hostile regression.

### What went wrong (and lessons)

1. **F3 regressed the no-timeout path.** I unified all paths through `child_fut` without checking that pre-F3's no-timeout path used `wait_with_output()` (correct) while the timeout path used the sequential bug (dormant). Lesson: when unifying code paths, understand what the DIFFERENT paths were doing, not just one.

2. **F11 initial golden tests were too weak.** I optimized for deterministic snapshots and stripped output from the oracle. The reviewer had to point out that lifecycle-only doesn't catch output regressions. Lesson: when the plan says "assert X AND Y", do not drop Y for convenience.

3. **I missed `kill_on_drop` initially.** I was so focused on the `tokio::select!` cancellation pattern that I didn't check whether dropped children are actually killed. Lesson: for any subprocess spawning code, `kill_on_drop(true)` is non-negotiable.

4. **I didn't verify compile on `--no-default-features`.** Both new crates have feature flags. I verified default features compile, not minimal. Session 13 should `cargo check -p nika-verb-X --no-default-features` for each new crate before committing.

### Metrics that moved

| What | Before | After | Delta |
|------|--------|-------|-------|
| Total commits | 6 (D/P/E) | 19 (D/P/E + F + G) | +13 |
| Tests | 10,780 | 10,805 | +25 |
| Engine LOC | 148,792 | 146,452 | −2,340 |
| New crates | 0 | 2 | +2 |
| Review cycles | 0 | 1 | — |
| Bugs caught by review | — | 2 (P0) | — |
| Bugs shipped | — | 0 (all fixed) | — |

---

## Appendix B — Verification commands

Run these at any time to verify S12 state is intact:

```bash
cd /Users/thibaut/dev/supernovae/nika

# Commit range
git log --oneline c5ea27438..HEAD | wc -l  # 19

# Test count
cd tools
cargo test --workspace --lib 2>&1 | \
  grep -E "^test result" | \
  awk '{s+=$4} END{print s}'  # 10805

# Clippy
cargo clippy --workspace --lib -- -D warnings  # clean

# Engine LOC
find nika-engine/src -name "*.rs" | xargs wc -l | tail -1  # 146452

# Diamond
cargo tree -p nika-policy  | grep nika-engine   # empty
cargo tree -p nika-extract | grep nika-engine   # empty

# Crate count
grep -c '^    "nika' Cargo.toml  # 28

# Golden regression suite
cargo test -p nika-engine --lib runner::tests_golden_verbs  # 5 passed
```

---

**End of Session 12 handoff. Session 13 is cleared to begin.**
