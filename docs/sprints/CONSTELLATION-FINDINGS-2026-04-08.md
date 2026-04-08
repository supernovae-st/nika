# Constellation Audit — Findings & Action Items

> **Date:** 2026-04-08
> **Scope:** Full codebase scan for bugs, improvements, dead code, quick wins
> **Source:** 11 exploration agents + mega-plan analysis + manual review
> **Status:** COMPLETE — all 11 exploration agents finished

---

## P0 — CRITICAL BUGS (fix before refactor)

### BUG-C1: Dead MPSC Receiver in infer.rs (2 instances)

**Files:** `nika-engine/src/runtime/executor/infer.rs:1075` and `:1277`
**Impact:** Correctness + performance — stream chunks allocated then silently dropped

```rust
// Line 1075 — retry path
let (tx, _rx) = mpsc::channel::<StreamChunk>(1);
provider.infer_stream(&prompt, tx, model, None).await

// Line 1277 — repair/fallback path  
let (tx_rf, _rx_rf) = mpsc::channel::<StreamChunk>(1);
provider.infer_stream_with_options(prompt, tx_rf, &rf_options).await
```

**Root cause:** Both create channel, drop receiver immediately. Provider writes to dead channel.
`tx.try_send()` fails silently because receiver is gone. Stream chunks allocated then dropped.

**Correct pattern exists** — the main streaming path (around line ~800-900) properly spawns a tokio task
to drain the receiver. These 2 sites are non-streaming fallback paths that use streaming API
as a fire-and-forget RPC.

**Fix options:**
1. Wire a real receiver that drains chunks (correct but heavier)
2. Check `tx.is_closed()` before sending (band-aid)
3. Add a non-streaming `provider.infer()` method that these paths call instead (cleanest)

**Priority:** P0 — affects retry and structured output repair paths

---

### BUG-C2: McpInvoke Params Written UNREDACTED to Traces

**File:** `nika-engine/src/runtime/executor/invoke.rs:110-117`
**Impact:** SECURITY — MCP tool parameters (API keys, DB creds, tokens) in `.nika/traces/*.ndjson`

**What's leaked (confirmed by agent):**
- `McpInvoke.params` — NO redaction applied: `params: resolved_params.clone().map(Arc::new)`
- Can contain: API keys, database passwords, bearer tokens, webhook secrets

**What's ALREADY SAFE (most events properly redacted):**

| Event | Redaction | Status |
|-------|-----------|--------|
| TaskStarted.inputs | `to_value_redacted()` | SAFE |
| TemplateResolved.result | `redact_for_event()` (pattern + 200B truncate) | SAFE |
| McpResponse.response | `redact_value()` (recursive JSON walk) | SAFE |
| ProviderCalled | Metadata only (no content) | SAFE |
| ProviderResponded | Metrics only (no content) | SAFE |
| CanaryInjected | task_id only (no tokens) | SAFE |
| **McpInvoke.params** | **NONE** | **LEAKED** |

**Redaction infrastructure exists and is solid:**
- `redact_secrets()` in `util/mod.rs:58-98` — regex patterns for 12+ key formats
- `redact_for_event()` in `verbs.rs:670-682` — secrets + 200B truncation
- `redact_value_recursive()` in `resolve.rs:184-210` — recursive JSON walk

**Canary tokens: SAFE.** Custom Debug impl redacts them. Not stored in events.
CanaryInjected/CanaryDetected events store only task_id and metadata.

**Fix (1 line):** In `invoke.rs:116`:
```rust
// Before: params: resolved_params.clone().map(Arc::new),
// After:
params: resolved_params.as_ref().map(|p| Arc::new(redact_value(p))),
```

**Priority:** P0 — but scope is smaller than initially thought (1 event type, not all)

---

### BUG-C3: Serve Mutex Held Across .await

**File:** `nika-serve/src/routes/workflows.rs:320-343`
**Impact:** Starvation risk under concurrent cancel requests

```rust
// Lock acquired in `if let` expression — held for entire block
if let Some(handle) = state.workers.lock().await.remove(&id) {
    // ... signal handling ...
}
// Lock STILL HELD here during async storage update:
state.storage.update_state(&id, Cancelled, ...).await?;
```

**Type:** `tokio::sync::Mutex` (not std — so no hard deadlock, but starvation)
**Risk:** MEDIUM — concurrent requests serialize on workers lock during slow storage ops

**Fix:** Extract lock result before `.await`:
```rust
let handle = state.workers.lock().await.remove(&id);
// Lock dropped here
if let Some(handle) = handle { ... }
state.storage.update_state(...).await?;  // No lock held
```

**Priority:** P0 — latent under load

---

## P1 — HIGH-PRIORITY IMPROVEMENTS

### IMP-H1: IndexedDag Never Used by Runner

**Files:**
- `nika-engine/src/dag/indexed.rs` (878 lines) — O(1) lookups via Vec<SmallVec<[TaskId; 4]>>
- `nika-engine/src/dag/flow.rs` (1595 lines) — HashMap<String, _> based Dag
- `nika-engine/src/runtime/runner/mod.rs` — uses flow::Dag

**Current state:** IndexedDag exists with pre-computed topo order, O(1) dependency lookups,
depth computation, and `all_deps_done(id, &[bool])` — perfect for the runner's hot loop.
Runner instead uses HashMap-keyed Dag with string lookups.

**APIs compared:**
- IndexedDag: `dependencies(TaskId) -> &[TaskId]` (array index, O(1))
- Dag: `get_dependencies(&str) -> &[Arc<str>]` (hash lookup, O(n) string compare)

**Difficulty:** MEDIUM — Runner uses string-based task IDs throughout. Need to either:
1. Switch Runner to use TaskId (u32) internally (big change, best with Phase 14)
2. Create an adapter that wraps IndexedDag with string lookup bridge (easier)

**Priority:** P1 — performance win, pairs with Phase 14 RunContext refactor

---

### IMP-H2: Interner Doesn't Actually Intern

**File:** `nika-engine/src/util/interner.rs:18-20`

```rust
pub fn intern(s: &str) -> Arc<str> {
    Arc::from(s)  // Just allocates a new Arc every time
}
```

**Callers:** ~55 call sites across the codebase
**Impact:** Every call creates a new allocation. High-frequency task IDs, model names,
provider names get duplicated thousands of times per workflow run.

**Fix:** Real interner with `DashMap<u64, Weak<str>>` or `FxHashMap` + mutex:
```rust
static CACHE: LazyLock<DashMap<u64, Weak<str>>> = LazyLock::new(DashMap::new);
pub fn intern(s: &str) -> Arc<str> {
    let hash = fxhash::hash64(s);
    if let Some(weak) = CACHE.get(&hash) {
        if let Some(arc) = weak.upgrade() { return arc; }
    }
    let arc: Arc<str> = Arc::from(s);
    CACHE.insert(hash, Arc::downgrade(&arc));
    arc
}
```

**Priority:** P1 — perf win, 1 commit, zero risk

---

### IMP-H3: EventEmitter Trait — 0 Production Uses (Detailed)

**File:** `nika-event/src/emitter.rs` — trait defined, 2 impls (EventLog, NoopEmitter)

**Trait:**
```rust
pub trait EventEmitter: Send + Sync {
    fn emit(&self, kind: EventKind) -> u64;
}
```

**Current state (confirmed by agent):**
- **49 production files** use `EventLog` concretely (not 59 as handoff said)
- **27 structs** store `event_log: EventLog` as value field
- **2 structs** use `Arc<EventLog>` (StructuredOutputEngine + RigAdapterBuilder)
- **351 `.emit()` call sites** across the codebase
- **40+ `.clone()` calls** for passing EventLog to child structures

**EventLog internals (cheap clone):**
```
events: Arc<RwLock<Vec<Event>>>    // shared
start_time: Instant                 // copy
next_id: Arc<AtomicU64>            // shared
broadcast_tx: Option<broadcast::Sender<Event>>  // clone
trace_writer: Option<Arc<TraceWriter>>          // shared
```

**5 hot sites:**

| Struct | Current Field | Pattern |
|--------|--------------|---------|
| TaskExecutor | `event_log: EventLog` | By value, cloned to children |
| Runner | `event_log: EventLog` | By value, cloned to TaskExecutor |
| StructuredOutputEngine | `log: Arc<EventLog>` | Already Arc |
| RigAgentLoop | `event_log: EventLog` | By value |
| BuiltinToolRouter | via rig_adapter | `Option<Arc<EventLog>>` |

**Fix (per v2.1 plan):**
1. Add blanket impl (3 lines): `impl<T: EventEmitter + ?Sized> EventEmitter for Arc<T>`
2. Add type alias: `pub type EventSink = Arc<dyn EventEmitter>;`
3. Flip 5 hot sites to `Arc<EventLog>` (eliminates 40+ clone calls)
4. No API breakage — EventLog still implements EventEmitter

**Priority:** P1 — Phase 5 of Constellation

---

### IMP-H4: error_domains.rs — 4 Sub-Enums, 0 Call Sites, READY FOR BIG-BANG

**File:** `nika-engine/src/error_domains.rs` (253 LOC)
**Contains:**
- `ProviderError` (7 variants) — `From` impl exists ✓
- `DagError` (3 variants) — `From` impl exists ✓
- `ExecutionError` (6 variants) — `From` impl exists ✓
- `BindingError` (3 variants) — `From` impl exists ✓

All 4 `From<DomainError> for NikaError` impls present. Tests validate roundtrips.

**NikaError:** 114 flat variants total. Migration targets:

| Flat Variant | Call Sites | Migration Target |
|-------------|-----------|-----------------|
| `Execution(String)` | **54** | `ExecutionError::General` |
| `ProviderApiError` | 15 | `ProviderError::ApiError` |
| `TemplateError` | 8 | `BindingError::TemplateError` |
| `CycleDetected` | 7 | `DagError::CycleDetected` |
| **Total** | **84** | 4 domain enums |

**Readiness: READY.** Infrastructure complete, From impls exist, tests exist.
84 call sites is manageable in 1 big-bang commit per v2.1 §18.9.

**Execution order:** Execution(54) → ProviderApiError(15) → TemplateError(8) → CycleDetected(7)

**Priority:** P1 — Phase 6 of Constellation

---

### IMP-H5: main.rs — 5,527 LOC (10-70x Reference Architectures)

**File:** `nika/src/main.rs` — 5,527 lines, 85% logic, 4% test coverage
**Reference:** rust-analyzer=365, Helix=160, Ruff=78

**Fix:** Migrate to `nika-cli/src/verbs/` per Phase 16. Target: <500 LOC main.rs.

**Priority:** P1 — Phase 16 of Constellation

---

## P2 — MEDIUM-PRIORITY IMPROVEMENTS

### IMP-M1: God Files (5 files, 27k LOC)

| File | LOC | Action |
|------|-----|--------|
| runner/mod.rs + tests | 7,154 | Extract scheduler.rs from 900-line run() |
| transform.rs | 5,645 | Split into 12 files + transform! macro |
| analyze.rs | 5,528 | Split into 11 files |
| main.rs | 5,527 | Migrate to nika-cli/verbs/ |
| template.rs | 4,938 | Split into 9 files |

**Additional large files found:**
| File | LOC | Notes |
|------|-----|-------|
| tests_200_workflows.rs | 10,445 | Test file — OK but huge |
| resolve.rs | 3,948 | binding resolution — candidate for split |
| lower.rs | 2,882 | AST lowering — monitor |
| error.rs | 2,874 | Will shrink after error_domains promotion |
| artifact_processor.rs | 2,767 | Candidate for extraction |
| security.rs | 2,474 | Shield code — recent, OK for now |

---

### IMP-M2: LSP Duplication — 11,982 LOC (Intentional, Not Pure Duplicate)

**nika-engine/src/lsp/:** 16 files, 11,982 LOC — embedded LSP server with AST + model intel
**nika-lsp-core/src/:** 23 files, 11,885 LOC — protocol-agnostic pure functions

**Overlap:** ~7,000 LOC duplicated handler logic across 8 shared handlers:
- completion (1,451 vs 1,740), hover (1,056 vs 763), definition (1,232 vs 475)
- code_action (1,138 vs 689), semantic_tokens (1,315 vs 298), symbols (835 vs 307)

**Key insight:** NOT a pure duplicate. Engine version adds:
- AST-aware context (AstIndex) for completion/definition
- Model intelligence (ModelCatalog, 1,508 LOC) for hover/code_action
- tower-lsp-server shim (optional dep of nika-engine)
- 391 `cfg(feature = "lsp")` sites all in /lsp/ directory

**Engine already imports from nika-lsp-core:**
- parse_and_extract(), CursorContext, DefaultHandler
- 4 handlers delegated: references, folding_ranges, document_links, rename

**Unique to nika-lsp-core:** references, folding_ranges, document_links, rename, 
Tree-sitter recovery parser, WorldDatabase, LineIndex

**Strategy (revised from plan):** Incremental consolidation, NOT big-bang deletion.
Move semantic_tokens pure classification to core (~70 LOC saved), hover markdown gen (~300 LOC),
completion snippet gen (~200 LOC). Keep model_intel.rs and AST-aware handlers in engine.

---

### IMP-M3: nika-tui Feature Flags — Pass-Through by Design (Revised)

**File:** `nika-tui/Cargo.toml` declares 18 features, only `native-inference` has cfg usage

**Key insight (from agent):** These are NOT dead — they're **intentional pass-through** features:
```
nika-tui features → forward to nika-engine → used in nika-engine/src via cfg()
```
nika-tui doesn't need conditional compilation per media feature. The features exist
so end-users can build `nika-tui` with specific capability profiles.

**Actually used in nika-tui:** `native-inference` (3 cfg sites in lifecycle.rs, provider_modal/)

**Actually dead:** `fetch-feed` in nika-media (declared with `dep:feed-rs`, zero cfg in nika-media/src — 
but IS used in nika-engine/src/runtime/executor/extract.rs, so workspace-level it's alive)

**nika-media feature usage is healthy:** 73 cfg occurrences across 7 source files for 18/19 features.

**Action (revised):** Per plan Phase 18, wire cfg-gated TUI panels for the pass-through features
(media-chart → cost_chart, media-phash → similarity panel, etc.) This CONNECTS them, doesn't just
forward them. But this is enhancement, not a bug fix.

---

### IMP-M4: rstest — Workspace Dep, Zero Usage (mockall not found)

**File:** `tools/Cargo.toml:259` — `rstest = "0.25"` declared as workspace dev-dep
**Usage:** 0 `#[rstest]` or `use rstest` anywhere in workspace
**Note:** mockall was NOT found in workspace deps (contrary to mega-plan claim)
**Fix:** Adopt rstest for transform tests (Phase 4 per plan). Add mockall if needed for traits,
or use hand-written mocks per v2.1 §18.14 recommendation.

---

### IMP-M5: nika-media Linker Error (ARM64)

**Symptom:** `cargo test --workspace --lib` fails on nika-media with:
```
ld: symbol(s) not found for architecture arm64
```
**Root cause (confirmed):** Multiple incompatible html5ever versions in dep tree + Thin LTO:
- `scraper@0.26` → `html5ever@0.39.0`
- `dom_smoothie@0.16` → `html5ever@0.36.1`
- `htmd@0.5` → `html5ever@0.38.0`
- tiff chain: `charts-rs@0.3` → `image@0.25.10` → `tiff@0.11.3`

With `lto = "thin"` + `codegen-units = 1`, cross-module LTO fails on ARM64.

**Fix (fastest):** Add to `tools/Cargo.toml`:
```toml
[profile.test]
opt-level = 1
lto = false  # Avoid cross-version html5ever conflict
```
**Alt fix:** Upgrade dom_smoothie 0.16→0.21+ (html5ever 0.39 unified)

**Priority:** P0 — blocks `cargo test --workspace --lib` entirely

---

## P3 — QUICK WINS (1 commit each, zero risk)

### QW-1: Add `#[must_use]` to 10 Runner Builder Methods
**File:** `nika-engine/src/runtime/runner/mod.rs`
**Lines:** 416, 427, 436, 447, 480, 506, 515, 522, 530, 540
Methods: `quiet()`, `with_invocation_source()`, `with_detail_level()`, `with_classic_renderer()`,
`with_initial_context()`, `with_permission_mode()`, `with_cancel_token()`, `with_base_path()`,
`with_project_root()`, `with_working_dir_mode()`
**Risk:** Silent config loss if return value discarded.

### QW-2: Replace `std::collections::HashSet` with `FxHashSet` in template.rs (6 sites)
**File:** `nika-engine/src/binding/template.rs`
**Lines:** 555, 641, 725, 1377, 1476, 1563
FxHashSet is already imported (line 76) but not used in these security-critical template
parsing locations. 6 instances of `std::collections::HashSet<String>` → `FxHashSet<String>`.

### QW-3: `std::mem::take` Instead of `.clone()` for StreamChunk::Done (2 sites)
**File:** `nika-engine/src/provider/rig/provider_streaming.rs`
**Lines:** 137, 306 — `tx.try_send(StreamChunk::Done(response_buf.clone()))`
`response_buf` is moved on line 151 anyway. Use `mem::take(&mut response_buf)`.

### QW-4: Demote RunContext `workspace_root: Arc<RwLock<PathBuf>>` to `OnceLock`
**File:** `nika-engine/src/store/run_context.rs:289`
Set once at init (line 342), `set_workspace_root()` only called during initialization.
`Arc<RwLock<>>` is unnecessary synchronization for a write-once value.

### QW-5: Hoist `Arc::from(task_id)` Out of Streaming Loops (5 sites)
**File:** `nika-engine/src/runtime/rig_agent_loop/streaming.rs`
**Lines:** 168, 253, 271, 647, 682
`Arc::from(self.task_id.as_str())` called on EVERY iteration. Create once before loop,
then `task_id.clone()` (cheap Arc clone).

### QW-6: ARCHITECTURE.md Missing
**File:** `tools/nika-engine/ARCHITECTURE.md` — does not exist
Per matklad rule, every 10k-200k LOC project needs one.
This is Pre-Phase 0 of Constellation.

---

## UNWRAP HOTSPOTS (Production Code Only)

| File | .unwrap() count | Notes |
|------|-----------------|-------|
| transform.rs | **399** | CRITICAL — panic hotspot |
| resolve.rs | **206** | CRITICAL — binding resolution |
| template.rs | **162** | HIGH — template parsing |
| lower.rs | **94** | HIGH — AST lowering |
| analyze.rs | **83** | HIGH — AST analysis |
| artifact_processor.rs | **81** | MEDIUM |
| error.rs | 10 | OK — safe patterns |
| main.rs | 7 | OK — regex/semaphore |

**Top 3 files account for 767 of ~9,269 total unwraps (8.3%).** 
transform.rs alone has 399 — Phase 8 god file split is the right time to address these.

---

## ARCHITECTURAL OBSERVATIONS

### God Objects (Top 5)

| Rank | Type | LOC | References | Blocks |
|------|------|-----|------------|--------|
| 1 | RunContext | 1,995 | 472 refs | Every extraction |
| 2 | EventLog | 4,544 | 428 clones | EventEmitter wiring |
| 3 | TaskExecutor | ~5,000 | 22 fields | VerbExecutor split |
| 4 | Runner | 2,331 | 5 files | Runtime extraction |
| 5 | RigProvider enum | 321 | 215 refs | Provider trait |

### Raw Side Effects (No Trait Boundary)

| Effect | Call Sites | Current | Target |
|--------|-----------|---------|--------|
| reqwest::Client | 16+ | Raw | HttpClient trait |
| shell process | 24+ | Raw | ShellExecutor trait |
| std::fs/tokio::fs | 30+ | Raw | Filesystem trait |
| CasStore concrete | many | Concrete | BlobStore trait |
| time/sleep | many | Raw | Clock trait |

### Macro Opportunities

| Macro | Target | LOC Saved |
|-------|--------|-----------|
| `#[builtin_tool]` | 40 tools × 90 LOC boilerplate | ~3,000 |
| `transform!` | 64 transforms | ~2,000 |
| `#[nika_error]` | 114 variants × 3 duplication points | ~1,500 |
| `#[event_kind]` | 98 EventKind variants | ~500 |
| **Total** | | **~7,000** |

### Async Architecture Notes

**Good patterns:**
- Zero locks across .await (except BUG-C3)
- Proper pinned timeouts
- `catch_unwind` on every spawn
- `MAX_CONCURRENT_TASKS = 64` global semaphore
- Per-for_each fail_fast cancellation tokens

**Anti-patterns:**
- `dispatch_rig!` hand-rolled vtable (215 references)
- `Runner::run()` 900-line god method
- `infer.rs` streaming-as-RPC hack (BUG-C1)

---

## METRICS BASELINE (v0.79.0)

| Metric | Value |
|--------|-------|
| Crates | 17 (target: ~38) |
| LOC (Rust) | ~555k |
| LOC nika-engine | ~160k (target: 0 — split) |
| .rs source files | 862 |
| Test attributes | 36,706 |
| Tests (cargo test --lib) | 10,666+ |
| .unwrap() calls | 9,269 (target: <5,000) |
| TODO/FIXME | 332 |
| cfg(feature) sites | 772 across 65 files |
| NikaError variants | 114 (target: 10 domain enums) |
| EventKind variants | 100 |
| Builtin tools | 63 |
| Pub traits | 17 (target: 27+) |
| Proc-macros | 0 (target: 4 derives + 1 declarative) |
| Largest file (source) | runner/mod.rs 2,334 (target: <1,500) |
| Largest file (tests) | tests_200_workflows.rs 10,445 |
| Binary size (release) | 112 MB (target: 45 MB) |
| rstest usage | 0 (target: 50+) |
| mockall usage | 0 (target: replaced by hand-written mocks) |

---

## CONSTELLATION PHASE READINESS

| Phase | Ready? | Blocker |
|-------|--------|---------|
| Pre-0: ARCHITECTURE.md | YES | None |
| 1: nika-kernel traits | YES | None |
| 2: nika-kernel-mock | YES | None |
| 3: nika-macros | YES | None |
| 4: rstest pilot | YES | None |
| 5: EventEmitter wire | YES | None |
| 6: error_domains promote | YES | None |
| 7: LSP absorption | YES | None |
| 8: God file splits | YES | None |
| 9-12: Effect crate extraction | AFTER Phase 1 | Needs kernel traits |
| 13: nika-verb-* | AFTER Phase 9 | Needs effects |
| 14: nika-runtime | AFTER Phase 13 | Needs verbs |
| 15: nika-cache | AFTER Phase 14 | Needs runtime |
| 16: main.rs migration | AFTER Phase 14 | Needs runtime |
| 17: analyze.rs split | YES (independent) | None |
| 18: TUI split | YES (independent) | None |
| 19: Type system | AFTER all extractions | Needs stable types |
| 20: Polish | LAST | Everything |

---

## EXECUTION RECOMMENDATIONS

### Do First (Today, Pre-Phase 0)
1. **Fix nika-media linker** (IMP-M5) — `[profile.test] lto = false` — 1 commit, UNBLOCKS ALL TESTS
2. Fix BUG-C1 (dead receiver) — 1 commit
3. Fix BUG-C3 (serve Mutex) — 1 commit  
4. Write ARCHITECTURE.md — 1 commit

### Do During Quick Wins Sprint
5. Fix BUG-C2 (trace redaction) — needs design, pairs with Phase 5
6. Fix interner (IMP-H2) — per-Runner scoped FxHashMap, 1 commit
7. QW-1 through QW-5 — 5 commits, no deps

### Do During Constellation Phases
8. Everything in P1 and P2 sections — matches phases 1-20
9. IndexedDag switch during Phase 14 (nika-runtime extraction)
10. LSP consolidation during Phase 7 (incremental, not big-bang)

---

## CODE QUALITY SCAN (main.rs binary crate)

**Scope:** `tools/nika/src/main.rs` (5,527 LOC)

| Metric | Count | Notes |
|--------|-------|-------|
| `.unwrap()` | 7 | 3 regex (safe), 1 semaphore (safe), 3 CLI parse (fixable) |
| `.expect()` | 1 | Well-justified with len check |
| `panic!()` | 0 | Excellent |
| `unsafe` | 0 | Excellent |
| TODO/FIXME | 0 | Clean |
| `.clone()` | 34 | Moderate, mostly justified |
| `#[allow(...)]` | 4 | All `too_many_arguments` — justified for CLI handlers |

**Note:** This scans only the binary crate. The engine (~160k LOC) has 9,269 unwraps.
Engine-wide scan pending from remaining agents.

---

## DETAILED AGENT FINDINGS

### IndexedDag Switch Assessment

**Verdict: NOT a simple swap — MEDIUM difficulty**

Runner uses string-based task IDs in 6 call sites (flow_graph.get_dependencies(&task.name)).
IndexedDag uses TaskId (u32) with O(1) Vec indexing.

**Interface mismatches:**
- IndexedDag has no `detect_cycles()` (Dag has it — move to analyzer)
- IndexedDag has no `get_deepest_final_task()` (but has depths, easy to add)
- IndexedDag has no `has_path()` (only needed in validate.rs, not Runner)
- IndexedDag `all_deps_done(id, &[bool])` is perfect for Runner's hot loop

**Recommendation:** Best done during Phase 14 (nika-runtime extraction) when Runner gets
TaskId internally anyway. Build name→TaskId lookup map in Runner init. Keep Dag for
validation-only (validate.rs needs has_path, detect_cycles).

### Interner History

The current no-op interner replaced a DashMap-based one (commit 9ec4e2347). 
Rationale: 80 bytes/entry overhead > 30 bytes/string savings at the time.
**But:** Per-Runner scoped FxHashMap avoids global contention and has ~0 overhead.

### infer.rs Dead Receiver — Full Context

Both bugs are in non-streaming paths that use the streaming API as fire-and-forget RPC.
The correct pattern exists in `rig_agent_loop/streaming.rs` (spawns task to drain rx).
Native provider at `provider/native/runtime.rs:431` also shows correct channel usage.

**Best fix:** Add a non-streaming `provider.infer()` method for these paths.
Alternatively, drain with a spawned task.

---

### EventEmitter Wiring Detail

**49 production files** with concrete EventLog, **27 structs** storing it as field,
**351 emit() calls**, **40+ clone() sites**.

Blanket impl is 3 lines. Flip 5 hot sites eliminates 40+ clones.
EventLog clone is already cheap (Arc internals), but trait boundary enables
mock testing (NoopEmitter, CollectingEmitter) and future nika-telemetry integration.

---

*Document completed 2026-04-08. All 11 exploration agents finished.*
*Agents: infer dead-rx, IndexedDag, serve Mutex+interner, trace redaction,*
*nika-media linker, god files+QW, EventEmitter, error_domains, dead features,*
*LSP duplication, unwrap/code smells.*
