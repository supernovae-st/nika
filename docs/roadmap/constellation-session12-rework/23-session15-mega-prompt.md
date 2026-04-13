# Session 15 — Mega-Prompt (post-review enriched)

> **Auteur / date:** Claude Opus 4.6, 2026-04-11 (post-S14 + post-review)
> **Source:** S14 execution (5 commits + hotfix) + 4-agent post-S14 review
>            (code-reviewer + rust-architect ×2 + code-explorer)
> **Supersède:** `21-session15-handoff.md` (draft v1, partially obsolete)
> **Companion docs:**
>   - `20b-session14-scope-correction.md` — S14 phase 1 findings + postmortem
>   - `22-agent-v2-design.md` — Wave C / nika-verb-agent design (separate concern)
> **Gitignored** — local only
> **Baseline attendue:** HEAD post-S14.5 `12407d125` (ARCHITECTURE.md update for Session 14 + 14.5). The +invariants commit is `144f5abeb`, followed by the ARCHITECTURE.md doc update `12407d125`.

---

## 0. Orientation

```
Working dir : tools/
Tests       : cargo test --workspace --lib   (jamais --test — Keychain macOS)
Co-author   : Co-Authored-By: Nika 🦋 <nika@supernovae.studio>   (JAMAIS Claude)
Launch      : 2026-05-05  — refactor partiel = état valide
Langue      : Franglais conversation, EN code/docs/commits
```

**Action pré-flight #0:** `git log --oneline -10` pour vérifier le HEAD réel.
Si ce doc référence un état périmé, **stop and re-baseline** avant d'écrire du code.
Le S14 mega-prompt v3 a appris cette leçon à ses dépens.

---

## 1. Baseline réelle (post-S14 + S14.5 hotfix, mesurée)

```
HEAD          : 12407d125  docs(constellation): ARCHITECTURE.md update for Session 14 + 14.5
Recent chain  : 12407d125  (S14.5 ARCHITECTURE.md update)
                144f5abeb  (S14.5-B docs + invariants #23/#24/#25)
                53513e5ee  (A hotfix: f64::EPSILON + non_exhaustive + TEMP markers)
                acf9d1784  (S14-ε verb-exec pre-cancel)
                aebea1cd9  (S14-δ golden oracle)
                935658eae  (S14-γ VerbFetchError variants)
                9f384e07a  (S14-β fetch helpers migration)
                c96dec861  (S14-α InferEvent::Done struct variant)
                eaa7f16c2  (S14-BUG2 regression fix — pre-S14)

Crates        : 35 (32 diamond + 3 outside: nika-napi, nika-py, nika-macros)
Engine LOC    : ~146,600  (−277 from fetch.rs S14-β; rest of engine unchanged)
Tests         : ~10,900 lib (verb-fetch 28, verb-exec 13, verb-invoke 6, verb-infer 10,
                engine 3873, kernel 50, kernel-mock 52, etc.)
Clippy        : 0 warnings
no-default    : 0 errors (G3)
```

**Top 10 biggest engine files** (target: split anything >1500 LOC):

| File | LOC |
|------|-----|
| `ast/tests_200_workflows.rs` | 9781 |
| `runtime/runner/tests.rs` | 4238 |
| `binding/resolve.rs` | 3391 |
| `ast/lower.rs` | 2669 |
| `error.rs` | 2618 |
| `runtime/executor/tests.rs` | 2524 |
| `binding/template/tests.rs` | 2482 |
| `runtime/artifact_processor.rs` | 2497 |
| `runtime/rig_agent_loop/tests.rs` | 2087 |
| `runtime/executor/infer.rs` | **2029** ⭐ W14-B2 target |

`structured_output.rs` is **1813 LOC**, also coupled to `infer.rs`.

---

## 2. Carte extraction post-S14

```
VERBE       CRATE              BRIDGE ENGINE          DISPATCH ARM
─────────────────────────────────────────────────────────────────
exec    ✅ S13-B1+S14-ε       ✅ S13-B2 (live)        ✅ NotImpl  (Wave D)
fetch   ✅ S13-D1+S14-β/γ     ⚠️  partial             ✅ NotImpl
invoke  ✅ S13-C1 (builtins)  ⚠️  builtin live, MCP=inline  ✅ NotImpl
infer   ✅ W14-B1+S14-α/δ     ❌ NO BRIDGE CALL       ✅ NotImpl
agent   ❌ N'EXISTE PAS       ❌                       ❌ (Wave C)
─────────────────────────────────────────────────────────────────
```

**Critical finding** (code-explorer): **`nika-verb-infer` has no engine bridge call.**
W14-B1 created the crate, W14-B3 wired the runtime adapter, but
`nika-engine/src/runtime/executor/infer.rs` has zero `use nika_verb_infer`
imports. The extraction is structurally complete but the bridge flip
(W14-B2) is the biggest piece of S15/S16 work.

---

## 3. Real architectural debts (post-review confirmed)

### Debt #1 — McpPool trait too thin (4 missing methods)

**Surface today** (`tools/nika-kernel/src/mcp.rs`, 88 LOC, 3 methods):
```rust
async fn call_tool(&self, server, tool, args: Value) -> Result<Value, McpError>;
async fn read_resource(&self, uri) -> Result<String, McpError>;
fn has_server(&self, server) -> bool;
```

**What's missing for `McpPoolAdapter`** (from invoke.rs reverse-engineering):

| Method needed | Why | Blocking |
|---|---|---|
| `call_tool` returning `McpToolResult` (not bare Value) | Engine reads `is_error`, `was_cached`, `content_size_bytes()`, `has_media()` from `ToolCallResult` | YES |
| `read_resource` returning `McpResourceContent` (not String) | Bridge needs `.blob` for media pipeline; String drops it entirely | YES (W14-B4) |
| Retry + event emission surface | `call_tool_with_retry_events` emits `McpRetry` events; trait has no event-emission param | YES |
| 50 MB result size cap | Currently enforced inline in invoke.rs; must move to adapter to preserve invariant | YES |
| Cancel token in signatures | Adapter cannot interrupt `get_or_connect()` server spawn | YES |
| `list_tools(server)` | Tool discovery for agent verb | YES (Wave C prereq) |

### Debt #2 — `infer.rs` 7-site emission (invariant #24 violation at birth)

**Confirmed line numbers:**
- `infer.rs:621` — mock path
- `infer.rs:1156` — non-streaming path
- `infer.rs:1330` — structured output tool-injection (streaming)
- `infer.rs:1388` — structured output tool-injection (non-streaming)
- `infer.rs:1527` — native streaming path
- `infer.rs:1592` — native non-streaming path
- `infer.rs:1898` — `check_infer_guardrails` secondary path

When W14-B2 flips the bridge to call `nika_verb_infer::run()`, **all 7 sites
must be deleted atomically** or golden tests see double-emit. The async-expert
Phase 1 review flagged 2 sites; the post-S14 code-explorer found 7. The drift
between Phase 1 (pre-S14) and post-S14 mapping is itself a lesson: **always
re-grep at session start** because the codebase moves.

### Debt #3 — `parse_retry_after` reqwest leak (invariant #23 violation)

`tools/nika-verb-fetch/src/retry.rs:62` exposes `&reqwest::header::HeaderMap`
in a public verb-crate signature. Kernel `nika-kernel::http::HttpRequest`
already uses `HashMap<String, String>` — that is the canonical pattern.

**S15-A0 fix**: change signature to `parse_retry_after(header_value: Option<&str>)`.
Engine bridge call site becomes:
```rust
parse_retry_after(response.headers().get("retry-after").and_then(|v| v.to_str().ok()))
```
Then move `reqwest` to `[dev-dependencies]` in `nika-verb-fetch/Cargo.toml`
(only the test module needs it for the `HeaderMap::insert` API).

### Debt #4 — `safe_backoff_delay` silent truncation

`safe_backoff_delay(1000, 0.8, 2)` returns **1ms** instead of ~640ms because
`factor < 1.0` casts to `0u64` and clamps. Documented edge case for extreme
inputs (`0.5^10`), undocumented for "reasonable fractional" multipliers.
Risk: a workflow with `retry: { backoff: 0.8 }` (anti-backoff/jitter reduction)
gets a tight 1ms retry loop.

**S15 cleanup**: either document explicitly with a test
asserting `safe_backoff_delay(1000, 0.8, 2) == 1` (deliberate floor), or fix
with `factor.round() as u64`. Decide based on whether anti-backoff is a
real use case.

### Debt #5 — `finish_reason_raw` dead carriage

S14-α plumbed `finish_reason_raw: Option<String>` through `InferEvent::Done`
but `nika-verb-infer/src/lib.rs:175-184` `stop_reason_to_finish_reason()`
hardcodes `"content_filter"` instead of consuming the field.

**W14-B2 fix**: change signature to
`fn map_finish(stop: &StopReason, raw: Option<&str>) -> FinishReason` and
populate from both streaming and non-streaming paths.

### Debt #6 — Wave C: `rig_agent_loop` extraction blocked

5363 LOC across 8 files. **9 TEMP engine deps still present** (none extracted
since S14 scope doc):
- `SkillInjector` (run.rs imports)
- `LimitTracker` (limit_tracker.rs)
- `DynamicSubmitTool` (agent.rs:356,385)
- `NikaMcpTool` (builtin/trait.rs)
- `ProviderKind` (providers.rs)
- `STREAM_CHUNK_TIMEOUT` (util/constants.rs)
- `EngineRunExecutor` (run_executor.rs)
- `KernelToolAdapter` (builtin/trait.rs)
- `SecurityContext` (infer.rs:mod)

S15/S16 work. See `22-agent-v2-design.md` for the dedicated Wave C plan.

### Debt #7 — Wave D dispatch() activation blocked

All 5 arms in `nika-runtime::dispatch::dispatch()` return `NotImplemented`
because template resolution + binding + skills + spotlight live in `nika-engine`.
Cannot build `InferInput`/`ExecInput`/etc. from `AnalyzedTaskAction` until
those move. **S15/S16 prerequisite work**, then dispatch() proper.

---

## 4. S15 scope decision: Option A / B / C / D

Three options refined from review:

### Option A — "Conservative McpPool only" (8 commits, ~2 sessions worth)

McpPool trait expansion + adapter + shim removal. Clears Debts #1, #2 partially
(only NoopMcpPool, not the 7-site emission). Engine LOC unchanged (~0).

### Option C — "Realistic" (11 commits, ~3 sessions worth)

Option A + fetch retry orchestration extraction + `parse_retry_after` signature
fix. Engine LOC drop ~−500. Clears Debts #1, #3, #4 fully. W14-B2 deferred.

### Option D — "Ambitious" (15+ commits, S15+S16)

Option C + W14-B2 infer.rs surgery + 7-site collapse. Clears Debts #1-#5.
Wave C still S16+. Risk: scope overrun.

**RECOMMENDED: Option C** for S15. McpPool is the hardest blocker; fetch retry
is isomorphic but simpler than infer; W14-B2 is its own multi-session beast.
Defer to S16/S17.

---

## 5. S15 commit sequence (Option C, from rust-architect deep-dive)

### Phase 0 — pre-flight (no code, ~30 min)

```bash
cd tools/
git log --oneline -15                                        # confirm baseline
cargo check --workspace 2>&1 | grep "^error"                 # 0
cargo check --workspace --no-default-features 2>&1 | grep "^error"  # 0
cargo test --workspace --lib 2>&1 | tail -3                  # ~10,900 ok
cargo clippy --workspace --all-targets 2>&1 | grep "^warning\|^error" | head -5  # near 0

# Phase 0 verification of debt state
grep -n "ProviderResponded {" tools/nika-engine/src/runtime/executor/infer.rs  # expect 7
grep -n "NoopMcpPool\|NullBlobStore\|NullHttpClient" tools/nika-engine/src/runtime/executor/invoke.rs  # 6 sites
wc -l tools/nika-engine/src/runtime/executor/{infer,fetch,invoke,exec}.rs
wc -l tools/nika-kernel/src/mcp.rs                           # 88
```

### Phase 1 — parallel review dispatch (~20 min, BEFORE code)

**Sacred invariant** from S14 lessons: never write code without Phase 1.

Dispatch 4 agents in parallel:

1. **rust-architect** — validate the McpPool trait expansion design below.
   Object safety? `async_trait` interaction with `Arc<dyn EventEmitter>`?
2. **rust-pro** — map `call_tool_with_retry_events` dependencies and the
   path from `ToolCallResult` to `McpToolResult`. List every field that
   must round-trip.
3. **code-explorer** — trace every `McpClient::*` call-site in invoke.rs.
   Confirm the 6 shim sites are still at lines 91/134/149/342/349/352.
4. **rust-async-expert** — audit cancel propagation through the new trait
   surface. Specifically `CancellationToken` lifetime in `Pin<Box<Future>>`
   returned by `#[async_trait]`.

Each agent gets self-contained context, file paths, and the shape of the
`McpToolResult` / `McpResourceContent` / `McpCallOptions` types proposed
below. Each reports under 400 words.

**Synthesize** before writing code. If any GATE finding emerges, ask user
sign-off. **Do not proceed past Phase 1 with unresolved blockers.**

### Phase 2 — execution (8 commits, 1 session)

```
S15-A0  nika-kernel       Add McpToolResult / McpResourceContent / McpToolDescriptor /
                          McpCallOptions types. Add McpError::ResultTooLarge variant.
                          Switch McpPool trait to #[async_trait] form. NO impls touched.
                          ALSO: refactor parse_retry_after to take Option<&str> and
                          move reqwest to [dev-dependencies] (Debt #3, invariant #23).
                          VERIFY: cargo check -p nika-kernel -p nika-verb-fetch
                                  cargo check --workspace --no-default-features
                                  cargo test -p nika-kernel -p nika-verb-fetch --lib

S15-A1  nika-kernel-mock  Expand MockMcpPool to new 4-method surface. Add fixtures:
                          happy path, cancelled, oversized result, error.
                          VERIFY: cargo test -p nika-kernel-mock --lib

S15-A2  nika-verb-invoke  Rewrite verb body against new trait surface. Update tests
                          to McpCallOptions. Drop NoopMcpPool import from tests.
                          VERIFY: cargo test -p nika-verb-invoke --lib

S15-A3  nika-engine       Implement McpPoolAdapter in
                          src/runtime/mcp/adapter.rs wrapping Arc<McpClientPool>.
                          Enforce 50 MB cap inside adapter.
                          Translate ToolCallResult → McpToolResult::new().
                          Preserve call_tool_with_retry_events via opts.events
                          + opts.task_id (Arc-owned, not borrowed — see Trap #1).
                          DO NOT wire into invoke.rs yet. Adapter lives alone.
                          VERIFY: cargo check -p nika-engine
                                  cargo test -p nika-engine --lib runtime::mcp::adapter

S15-A4  nika-runtime      Replace NoopMcpPool in dispatch.rs test helper with
                          McpPoolAdapter. Remove TEMP comment. Dispatch Invoke arm
                          becomes wireable (still NotImplemented for now, but
                          McpPool field now Real).
                          VERIFY: cargo test -p nika-runtime --lib

S15-A5  nika-engine       Shrink invoke.rs MCP path: delete inline 50MB check,
                          delete direct call_tool_with_retry_events call,
                          delegate to McpPoolAdapter via verb-invoke crate.
                          Preserve EventLog ordering (McpInvoke before adapter
                          call, McpResponse after).
                          GOLDEN ORACLE assertion on lifecycle + output (G2).
                          Delete NullBlobStore + NullHttpClient + NoopMcpPool
                          structs from invoke.rs (no construction sites left).
                          VERIFY: cargo test --workspace --lib
                                  cargo check --workspace --no-default-features
                                  Run any wiremock invoke regression tests

S15-A6  nika-verb-fetch   Add task-level retry loop wrapper. Reuses helpers from
                          S14-β. Independent of A0-A5; lowest-risk tail commit.
                          VERIFY: cargo test -p nika-verb-fetch --lib

S15-A7  docs              Update ARCHITECTURE.md with S15 changes.
                          Update CHANGELOG.md under [Unreleased].
                          Append session journal entry to memory.
                          VERIFY: doc-only, no compile.
```

**Verification ritual after EVERY commit** (G3 invariant):
1. `cargo check --workspace`
2. `cargo test --workspace --lib`
3. `cargo check --workspace --no-default-features`
4. `cargo clippy --workspace --all-targets -- -D warnings`

---

## 6. Kernel DTOs (ready to paste in S15-A0)

```rust
// tools/nika-kernel/src/mcp.rs

use std::sync::Arc;
use async_trait::async_trait;
use nika_core::mcp::{ContentBlock, ResourceContent};
use tokio_util::sync::CancellationToken;

/// Kernel-facing tool result. Mirrors nika-mcp::ToolCallResult but is the
/// contract the trait speaks, so verb crates never import nika-mcp.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct McpToolResult {
    pub content: Vec<ContentBlock>,
    pub is_error: bool,
    pub was_cached: bool,
    /// Precomputed by the adapter under the 50 MB cap — never recomputed
    /// downstream. Invariant: always <= MAX_MCP_RESULT_SIZE.
    pub content_size_bytes: usize,
}

impl McpToolResult {
    /// Constructor required because struct is `#[non_exhaustive]`.
    /// Computes `content_size_bytes` from the blocks.
    pub fn new(content: Vec<ContentBlock>, is_error: bool, was_cached: bool) -> Self {
        let content_size_bytes = content.iter().map(|b| b.byte_size()).sum();
        Self { content, is_error, was_cached, content_size_bytes }
    }

    pub fn text(&self) -> String { /* join Text blocks */ todo!() }
    pub fn has_media(&self) -> bool { self.content.iter().any(|b| !b.is_text()) }
    pub fn media_blocks(&self) -> impl Iterator<Item = &ContentBlock> {
        self.content.iter().filter(|b| !b.is_text())
    }
}

/// Kernel view of ReadResource. Re-export nika-core's type — it already has
/// `{ uri, mime_type, text, blob }`. No information loss vs ToolCallResult.
pub type McpResourceContent = ResourceContent;

/// Tool descriptor for list_tools. Minimal — JSON Schema validation belongs
/// to whoever consumes it, not the kernel.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct McpToolDescriptor {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
}

impl McpToolDescriptor {
    pub fn new(name: String) -> Self {
        Self { name, description: None, input_schema: None }
    }
}

/// Call options aggregate — future-proofs the trait.
///
/// Uses `Arc<dyn EventEmitter>` (owned, not `&'a dyn EventEmitter`) because
/// the runtime spawns calls into `tokio::spawn` which requires `'static`.
/// EventLog already lives behind Arc internally, so this is a free clone.
#[derive(Clone)]
#[non_exhaustive]
pub struct McpCallOptions {
    pub task_id: Arc<str>,
    pub events: Arc<dyn nika_event::EventEmitter + Send + Sync>,
    pub cancel: CancellationToken,
}

impl McpCallOptions {
    pub fn new(
        task_id: Arc<str>,
        events: Arc<dyn nika_event::EventEmitter + Send + Sync>,
        cancel: CancellationToken,
    ) -> Self {
        Self { task_id, events, cancel }
    }
}

/// Errors from the MCP layer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpError {
    // ... existing variants ...

    #[error("mcp: tool result {bytes} bytes exceeds {limit} byte limit")]
    ResultTooLarge { bytes: usize, limit: usize },
}

#[async_trait]
pub trait McpPool: Send + Sync {
    async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        args: serde_json::Value,
        opts: McpCallOptions,
    ) -> Result<McpToolResult, McpError>;

    async fn read_resource(
        &self,
        server: &str,
        uri: &str,
        cancel: &CancellationToken,
    ) -> Result<McpResourceContent, McpError>;

    async fn list_tools(
        &self,
        server: &str,
    ) -> Result<Vec<McpToolDescriptor>, McpError>;

    fn has_server(&self, server: &str) -> bool;
}
```

---

## 7. Top 3 Rust traps to watch

### Trap #1 — `async_trait` + `&dyn EventEmitter` lifetime

`#[async_trait]` desugars to `Pin<Box<dyn Future + Send + '_>>`. Passing
`&'a dyn EventEmitter` ties the future to `'a`. Fine for direct callers,
but `nika-runtime::dispatch::dispatch()` will eventually `tokio::spawn` the
call, requiring `'static`. **Solution**: `McpCallOptions` holds
`Arc<dyn EventEmitter + Send + Sync>` (owned), not `&'a dyn`. EventLog is
already `Arc<EventLog>` internally, so the clone is free. Encode this in
S15-A0; do not learn it the hard way in S15-A3.

### Trap #2 — Object safety from `impl Iterator` returns

`McpToolResult::media_blocks()` returns `impl Iterator` — fine on a struct,
but if anyone adds it to `McpPool` itself ("let the pool materialize media"),
object safety breaks instantly. **Pre-empt**: add a doc-comment on `McpPool`
saying _"no methods returning `impl Trait` or borrowing `&self` outputs with
non-`'static` lifetimes — must stay object-safe"_, and add a compile-time
assertion `static_assertions::assert_obj_safe!(dyn McpPool)`.

### Trap #3 — `#[non_exhaustive]` + cross-crate construction (E0639)

`McpToolResult` marked `#[non_exhaustive]` means `nika-kernel-mock` cannot
construct it via struct literal. **MUST** ship `McpToolResult::new(...)` in
the same commit (S15-A0). Same for `McpToolDescriptor::new()` and
`McpCallOptions::new()`. This is invariant #19 — do not skip it. The S14
review already caught one violation (`#[non_exhaustive]` on verb errors
broke engine match sites); do not repeat the lesson.

---

## 8. Anti-goals for S15

- **Do NOT** add `supports_streaming_tool_calls` / `supports_progress_tokens`
  / `supports_sampling` capability methods to `McpPool` until a verb crate
  needs them. Invariant #18 — capabilities land with the consumer.
- **Do NOT** move retry policy into the kernel. Backoff, jitter, max-attempts
  stay in `nika-mcp::retry` behind the adapter. The trait is a call surface,
  not a policy engine.
- **Do NOT** split `McpPool` into `McpToolCaller` + `McpResourceReader` +
  `McpIntrospector`. It is one pool, one trait. Splinter only when a verb
  crate needs exactly one face — not pre-emptively. (Compare `FsRead`/`FsWrite`,
  splintered when nika-verb-fetch had a real read-only consumer.)
- **Do NOT** touch `invoke.rs` builtin path. S14-BUG2 regression territory.
  The builtin emission path stays byte-for-byte identical; only the
  non-builtin (MCP) path gets the adapter.
- **Do NOT** attempt W14-B2 (engine `infer.rs` shrinking + 7-site collapse)
  in S15. It's its own multi-session effort. If A0–A6 finish early, do A7
  docs and stop. Save W14-B2 for S16 with a fresh dispatch + Phase 1 review.
- **Do NOT** introduce `serde::{Serialize, Deserialize}` on `McpToolDescriptor`
  or `McpCallOptions`. The trait is a runtime call surface, not a wire format.
  `ContentBlock` already serializes via `nika-core::mcp` — that's the only
  serialization surface.
- **Do NOT** skip `cargo check --no-default-features` after S15-A2 and S15-A5.
  G3 invariant. The McpPool surface change touches feature-gated code paths.
- **Do NOT** let S15-A5 land tests + impl in the same commit without golden
  oracle assertion on BOTH lifecycle AND output. G2 invariant.
- **Do NOT** propose new architecture invariants based on a hunch. If a new
  pattern emerges, write it down in `.claude/rules/architecture.md` only
  after Phase 1 review confirms it. Hunches become technical debt.

---

## 9. Sacred invariants reminder (#1–#25)

S15 must respect every invariant from S12, S13, S14, S14.5. Quick reference:

- **#1** No `!Send` guards across `.await` (parking_lot, std::sync)
- **#7** `cargo check --no-default-features` after every commit
- **#11** `kill_on_drop(true)` on every `tokio::process::Command`
- **#14** LOC estimates conservative — actual ≤ promised
- **#17** No `infer_vision` / `infer_with_tools` trait methods — unify into `InferRequest`
- **#19** `new()` constructor on every `#[non_exhaustive]` struct in same commit
- **#22** `# TEMP` markers on TEMP deps with clearance condition
- **#23** Kernel-adjacent helpers stay primitive-typed (no reqwest/tokio leaks)
- **#24** Event emission singletons — one site per `EventKind` variant per file
- **#25** All verb-crate errors `#[non_exhaustive]` from day one

---

## 10. End-of-session ritual

When S15 wraps:

1. `cargo test --workspace --lib` final headline
2. `git log --oneline 12407d125..HEAD | wc -l` → S15 commit count (baseline = post-S14.5)
3. `cargo check --workspace --no-default-features`
4. Update `MEMORY.md` quick state line
5. Write `24-session16-handoff.md` IF anything was deferred (W14-B2 likely will be)
6. Git push
7. One-paragraph session summary to user

---

## 11. Reference files (read these first)

- `/Users/thibaut/dev/supernovae/nika/tools/nika-kernel/src/mcp.rs` — McpPool trait
- `/Users/thibaut/dev/supernovae/nika/tools/nika-mcp/src/types.rs` — `ToolCallResult`, `ResourceContent`, `ContentBlock`
- `/Users/thibaut/dev/supernovae/nika/tools/nika-mcp/src/client.rs` — `call_tool_with_retry_events`
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/invoke.rs` — bridge + 6 shim sites
- `/Users/thibaut/dev/supernovae/nika/tools/nika-runtime/src/dispatch.rs` — dispatch arms
- `/Users/thibaut/dev/supernovae/nika/tools/nika-event/src/emitter.rs` — `EventEmitter` trait (must be `+ Send + Sync`)
- `/Users/thibaut/dev/supernovae/nika/.claude/rules/architecture.md` — invariants #1–#25
- `/Users/thibaut/dev/supernovae/nika/docs/plans/constellation-session12-rework/22-agent-v2-design.md` — Wave C design (S15+ scope)

---

## 12. Si l'exécution dérive

Si pendant S15 tu trouves quelque chose qui contredit ce doc :

1. **STOP** — ne continue pas en hoping for the best.
2. Re-grep / re-read / re-mesure.
3. Update ce doc (ou crée `23b-session15-correction.md`).
4. Présente les findings au user, demande GATE sign-off.
5. Repars depuis une baseline confirmée.

C'est la leçon S14 mega-prompt v3. La drift entre handoff et reality coûte
plus cher à découvrir mid-execution qu'à attraper en Phase 0.

**Bonne S15.** Si tu fais A0 → A7 dans l'ordre, avec Phase 1 review au
début et G3 verification après chaque commit, S15 ship clean.
