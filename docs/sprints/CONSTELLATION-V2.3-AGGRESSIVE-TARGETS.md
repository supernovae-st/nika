# Constellation v2.3 — Aggressive Targets Addendum

> **Date:** 2026-04-09
> **Codename:** Constellation v2.3
> **Philosophy:** `perfection > timing`. Honest targets based on ecosystem research.
> **Supersedes:** implicit "nothing is deferred" targets in v2.2 that were not backed by research.
> **Source:** 4 parallel research agents (Perplexity / training-data for fallback), covering incremental compute, zero-unwrap enforcement, proc-macros, monolith decomposition.
> **Status:** AUTHORITATIVE — binds the Constellation roadmap from SESSION 5 onward.

---

## 0. Why v2.3 exists

v2.1 set the direction. v2.2 killed the "deferred post-launch" language. **v2.3 sets the numbers.**

Four concrete questions were left ambiguous in v2.2:

| Question | v2.2 answer | v2.3 verdict (research-backed) |
|----------|-------------|-------------------------------|
| Target LOC for nika-engine after decomposition? | "shrinks to ~140k" (vague) | **92-95k post-Phase-14** honestly. Sub-80k is post-launch. |
| Do we ship Salsa incremental compute? | Not mentioned | **No. Bolt blake3-keyed cache on Analyzed boundary. 2 weeks, not 2 months.** |
| What's the real plan to kill 5,576 unwrap/expect? | `target: <5000` in validation gates | **CI ratchet day 1. Full migration 6-10 weeks. Fuchsia-pattern OWNERS-reviewed allows.** |
| nika-macros — 4 derives or vaporware? | "Phase 3 (nika-macros) + linkme" | **Firm commitment: 4 derives + 1 declarative macro, syn 2 + darling + trybuild + macrotest, 2-3 weeks for 1 engineer.** |

This addendum fills all four gaps with **research-backed numbers** and **concrete phase assignments**.

---

## 1. Target #1 — nika-engine post-decomposition size

### Research input (web-researcher, 2026-04-09)

> "Sub-80k LOC for nika-engine post-decomposition is **achievable but aggressive**. The honest pre-launch target is **~90-110k** after Phases 12-14, with sub-80k requiring a Phase 15 (runtime/binding split) that should not block May 5."
>
> Precedents:
> - **rust-analyzer** took ~3 years to reach 32 crates. Central crate `hir-def` landed around **~45k LOC**.
> - **ruff** took ~2 years to reach 20+ crates. Central crate ~25-30k LOC.
> - **zed at 231 crates** is a cautionary tale — quadratic linker time, trait resolution recompute.
> - **matklad sweet spot:** "crates with clearly different reasons to change." rust-analyzer averages ~6k LOC per crate, biggest are 40-50k.

### Decomposition math (research output)

Starting from **160k** LOC:

| Phase | Extraction | LOC out | Running total |
|-------|------------|---------|---------------|
| S3 complete | L1 effect crates (clock/fs/blob/http/exec-runner) | ~10k already extracted | (counted separately) |
| S4 (Phase 11) | Provider bridge — no extraction, just impl | 0 | 160k |
| **S6-S9 (Phase 12)** | **nika-builtin** — all 63 tools incl. media | **22-24k** | **~137k** |
| **S10-S11 (Phase 13)** | nika-http + nika-exec-runner (already L1, remove dup) | ~6k | **~131k** |
| **S12-S13 (Phase 13/14)** | nika-verb-{infer,exec,fetch,invoke,agent} + nika-runtime | ~30k | **~100k** |
| **S14 (Phase 14)** | nika-cache (CacheBackend trait + impls) | ~4-5k | **~95k** |
| **POST-LAUNCH** | Phase 15 — nika-binding + nika-dag to L0 kernel | ~15k | **~80k** |

### Verdict

**Pre-launch target (by May 5): `nika-engine ≤ 100k LOC`.** Achievable. Honest.

**<80k is post-launch.** Chasing it before May 5 means Phase 15 (binding + dag to kernel) which touches AST flow and breaks everything temporarily. Not worth the risk.

**Public positioning:** "nika-engine shrank from 160k to <100k — 38% reduction — through 5 effect crates and 5 verb crates. Comparable to ruff's central crate (~25-30k) at 3-year milestone."

### Anti-pattern check

- **Don't create crates < 1k LOC.** No `nika-canary`, no `nika-spotlight` — keep those as modules inside nika-shield.
- **Don't split by file count.** Split by **reason to change**. `nika-builtin` = adds tools. `nika-provider` = LLM API drops. Those are different reasons.
- **Don't use feature flags to fake decomposition.** Full crate split or nothing.
- **Don't delete `nika-engine` until the facade period proves stable** (2+ weeks CI green post-Phase-14).

### Phase assignment

Already in the plan. v2.3 just commits to the **numbers**:
- After S9 (Phase 12): 137k (builtin out)
- After S13 (Phase 13): 100k (verb crates + runtime out)
- After S14 (Phase 14): 95k (cache out)
- **Launch state: 95-100k. Published target: 100k ceiling.**

---

## 2. Target #2 — Incremental compute (AST cache)

### Research input (web-researcher, 2026-04-09)

> "For Nika's `check workflow.nika.yaml` re-run case, **salsa is overkill**. A blake3-keyed persistent cache over `(file_hash, schema_version, include_graph_hash) -> AnalyzedWorkflow` gives you 90% of the win in ~2 weeks. Salsa only pays off when you also want fine-grained LSP query reuse across edits inside a file."

### Decision

**DO NOT adopt salsa.** Reasons:
1. **Retrofit cost onto async tokio = 4-6 weeks minimum, realistically 2 months.** Salsa is sync-per-query; rust-analyzer runs it on a dedicated blocking thread pool. 475k LOC of async tokio code would need a major restructure.
2. **Salsa shines for LSP queries** (cursor moves inside an unchanged file). Nika's hot path is `nika check`/`nika run` on CI, which is whole-file. Whole-file caching = 95% of the win.
3. **rust-analyzer + mun are the only mainstream users.** Compiler-adjacent. Not our category.

### What we ship instead — Phase 23 (NEW)

**`nika-cache` enriched with an AST caching layer.** Already in the plan as Phase 15 (nika-cache extraction). v2.3 firms up the cache strategy.

```
Cache key   = blake3(file_bytes || schema_version || include_graph_fingerprint || nika_cli_version)
Cache value = bincode(AnalyzedWorkflow)
Location    = .nika/cache/analyzed/<first-2-hex>/<full-hash>.bin
Hot layer   = moka::sync::Cache<Blake3Hash, Arc<AnalyzedWorkflow>> on top for LSP/serve
```

### Target performance

- `nika check workflow.nika.yaml` on unchanged file: **~200ms → <5ms** (40x)
- `nika run workflow.nika.yaml` repeat: parse phase eliminated, save ~30% cold start
- LSP cursor move on unchanged file: skip re-analyze entirely
- CI re-runs of same workflow: **90% faster analysis**

### Implementation plan (2 weeks, 1 engineer)

**Week 1 — core infra**
- Day 1-2: Add `Serialize + Deserialize` to `AnalyzedWorkflow` (Arcs, spans — real cost is here)
- Day 3: Hash file + transitively walk include graph, fingerprint everything
- Day 4: `try_load_analyzed(path) -> Option<AnalyzedWorkflow>` in `nika check` + `nika run` before parse phase
- Day 5: Namespace prefix `analyzed:` in existing `.nika/media/store/` CAS — **reuse the existing blob store**, don't create a new one

**Week 2 — polish + LSP**
- Day 1: LRU eviction (mtime-based GC when cache >500MB)
- Day 2: In-memory `moka::sync::Cache` hot layer for LSP/serve
- Day 3: `CacheHit { phase: "analyze", ns: ... }` telemetry event wired into existing EventLog
- Day 4: LSP integration — cursor moves on same file short-circuit analyze
- Day 5: Integration tests on showcase workflows (115 of them)

### Anti-patterns (from rust-analyzer postmortems + Typst discussions)

- **Don't cache at function granularity** until profiled. Whole-file caching is 95% of the win.
- **Don't forget `nika_cli_version` in the cache key.** A new binary after `cargo install` must not return stale Analyzed.
- **Walk the include graph**. Hash every include, every skill file, every context file.
- **Bound the cache.** Without GC, `.nika/cache/analyzed/` will grow to GBs in a week.

### Alternatives considered and rejected

| Crate | Verdict |
|-------|---------|
| `salsa` (new tracked queries) | Reject. 4-6 week retrofit, LSP-specific win we don't need yet. |
| `comemo` (Typst's memoization) | Keep for Phase 24 post-launch. If LSP cursor-move latency is still >16ms after blake3 cache, adopt `comemo` at sub-file granularity. |
| `cached` crate macros | Reject. Too coarse for cross-phase reuse. |
| Hand-rolled DashMap | **Adopt.** Full control, no framework lock-in, reuses existing patterns. |
| `moka` | **Adopt** for in-memory hot layer. |

### Phase assignment

**NEW Phase 23 — `nika-cache` AST caching** (was Phase 15 in v2.2, renumbered).

Lands in the same session as nika-cache extraction. 2 weeks, 1 engineer.

**Post-launch Phase 24:** `comemo` sub-file memoization IF profiling shows need.

---

## 3. Target #3 — Zero-unwrap enforcement

### Research input (web-researcher, 2026-04-09)

> "Clippy ships the exact lints you need (`unwrap_used`, `expect_used`, `panic`, `indexing_slicing`), scoping to `src/` only works cleanly via `#![cfg_attr(not(test), warn(...))]`, and a 1600-site migration in a 475k-LOC Rust workspace is realistically a **6–10 week sprint for 1 person**, not a weekend — but the "no new unwraps" CI gate can land in under a day."

### Baseline (measured 2026-04-09)

```
Total unwrap + expect in workspace: ~9,495
  in prod src (not tests):          ~5,576
  in tests:                         ~3,919
```

v2.2 target `<5000` is **basically flat** — current is 5576. That's not a target, it's a ceiling.

### New target — 3-phase migration

**Phase 21a (day 1) — CI ratchet (non-negotiable)**
```toml
# .cargo/config.toml
[target.'cfg(all())']
rustflags = [
  "-Wclippy::unwrap_used",
  "-Wclippy::expect_used",
  "-Wclippy::panic",
  "-Wclippy::indexing_slicing",
]
```

```rust
// at top of every crate lib.rs + main.rs
#![cfg_attr(not(test), warn(clippy::unwrap_used))]
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::panic))]
#![cfg_attr(not(test), warn(clippy::indexing_slicing))]
```

CI step in `.github/workflows/ci.yml`:
```yaml
- name: Clippy zero-unwrap ratchet
  run: |
    COUNT=$(cargo clippy --workspace --all-targets --message-format=json 2>/dev/null \
      | jq -r 'select(.reason=="compiler-message") | .message.code.code // empty' \
      | grep -c "unwrap_used\|expect_used\|panic\|indexing_slicing" || echo 0)
    BASELINE=$(cat baseline.txt)
    if [ "$COUNT" -gt "$BASELINE" ]; then
      echo "::error::unwrap count rose from $BASELINE to $COUNT"
      exit 1
    fi
    echo "unwrap count: $COUNT (baseline $BASELINE)"
```

**After day 1**: no new unwraps can enter the codebase. Existing ones flagged but not blocking.

**Phase 21b (weeks 1-6) — Hot path first migration**

Prioritization (research agent + repo analysis):

| Week | Crate layer | Unwrap count | Target | Strategy |
|------|-------------|--------------|--------|----------|
| 1-2 | `nika-runtime` / `nika-engine/runtime/` (runner, executor, dispatch) | ~300 | 0 | `?` + typed errors. Highest blast radius. |
| 3 | `nika-provider`, `nika-http`, `nika-exec-runner` | ~250 | 0 | Side-effect boundaries. Panic = workflow mid-run death. |
| 4 | `nika-builtin` (all 63 tools) | ~300 | 0 | Per-tool isolation, convert to `BuiltinError`. |
| 5 | `nika-core` AST/analyze | ~200 | 0 | Type-state refactor where possible. |
| 6 | `nika-tui`, `nika-cli`, `nika-display` | ~400 | `expect("reason")` with allow | Cold path — user sees message and exits. |

**Phase 21c (weeks 7-8) — Polish + ratchet flip**
- `nika-lsp`, `nika-serve`, `nika-sdk`, `nika-init` — ~150 sites
- Flip `warn` → `deny` on all `nika-*` crates (non-test)
- Delete `baseline.txt` (CI now enforces zero)
- Document escape hatch convention

### Escape hatch convention (Fuchsia pattern)

```rust
// REASON: compile-time constant, validated by test `regex_is_valid`
#[allow(clippy::unwrap_used)]
static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
```

Every `#[allow(clippy::unwrap_used)]` MUST have a `// REASON:` comment on the previous line. Pre-commit hook enforces it.

### Hot vs cold path distinction

| Context | Policy |
|---------|--------|
| CLI arg parsing, config load, `main()` setup | `expect("reason")` + allow ok — user sees error + exit |
| Test code | No lint (already scoped out via `cfg_attr`) |
| `tokio::spawn` tasks, runtime loop, request handlers | **Zero tolerance.** Panic kills a worker silently. |
| `once_cell`/`LazyLock` regex init | `expect("valid regex at compile time")` + allow + test proves validity |
| `NonZero*::new(literal).unwrap()` | allow + comment (clippy false positive on literals) |

### Real-world evidence (from research)

| Project | Unwrap count (non-test) | Enforcement |
|---------|--------------------------|-------------|
| **ruff** | <10 | `deny(clippy::unwrap_used)` |
| **rust-analyzer** | ~600 | `warn`, new crates stricter |
| **tokio** | ~200 | `warn`, all `#[track_caller]` |
| **Deno** | ~50 in `deno_core`, 0 in new code | CI ratchet |
| **Fuchsia** | near-zero | `deny`, OWNERS review on allows |
| **Nika (now)** | ~5576 | none |
| **Nika (target)** | ~0 in hot path, <50 total with `// REASON:` | `deny` workspace-wide |

### Timeline reality check

**1 engineer, 6-10 weeks** to reach true zero. **2-3 weeks** if terminal state is `expect("reason")` (soft zero).

**2 engineers split by crate layer, 4-6 weeks.**

This is **a quarter of work, not a week.** "Just run `cargo fix`" = wrong. ~20% are mechanical; rest require invariant understanding, function signature changes, or error variant introduction.

### Phase assignment

**NEW Phase 21 — Zero-unwrap migration** (was mentioned in v2.2 validation gates, now a full phase).

- Phase 21a (day 1): CI ratchet. Lands immediately.
- Phase 21b (weeks 1-6): Hot path migration.
- Phase 21c (weeks 7-8): Polish + deny flip.

**Lands as a dedicated sprint, separate from Phases 12-14** because it touches every crate and needs its own mental model.

### Anti-pattern: no big-bang

**Do not** try to migrate 5576 unwraps in one PR. The ratchet pattern is proven (Deno, Fuchsia). Day 1 = gate. Progressive migration over 6-10 weeks.

---

## 4. Target #4 — nika-macros (proc-macros)

### Research input

Web-researcher agent failed (no Perplexity tools in its sandbox — reported gracefully instead of hallucinating). Training-data-only summary:

- `syn 2.x + quote 1.x + proc-macro2 1.x` — stable API, mainstream
- `darling 0.20+` — attribute parsing, still the standard (deluxe is less common)
- `proc-macro-error2` — diagnostic messages with proper hygiene
- `trybuild` — compile-failure tests (mandatory)
- `macrotest` — snapshot tests of expanded code
- `linkme` (already decided in v2.2) — `distributed_slice` for compile-time registration
- `inventory` — alternative, but older and has caveats
- **2-3 weeks for 1-2 engineers** is realistic for a 4-derive + 1-declarative-macro crate

### 4 derives + 1 declarative macro (firm commitment)

**D1: `#[builtin_tool]`** (saves ~3,000 LOC across 63 tools)

Before (per tool, ~90 LOC):
```rust
pub struct SleepTool;

impl BuiltinTool for SleepTool {
    fn name(&self) -> &'static str { "sleep" }
    fn description(&self) -> &'static str { "Sleep for duration" }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": { ... } })
    }
    fn call<'a>(&'a self, args: String)
        -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move { ... })
    }
}
```

After (per tool, ~25 LOC):
```rust
#[builtin_tool(
    name = "sleep",
    description = "Sleep for duration",
    schema = SleepArgs,
)]
pub async fn sleep(args: SleepArgs) -> Result<SleepResponse, BuiltinError> {
    tokio::time::sleep(args.duration).await;
    Ok(SleepResponse { slept_for_ms: args.duration.as_millis() as u64 })
}
```

Generated: `impl BuiltinTool for __SleepToolStub { ... }` + `impl sealed::Sealed for __SleepToolStub { }` + `linkme::distributed_slice` entry for router auto-registration.

**D2: `#[nika_error]`** (saves ~1,500 LOC across 114 variants)

Before:
```rust
#[derive(Debug, thiserror::Error)]
pub enum NikaError {
    #[error("Template error: {reason}")]
    TemplateError { reason: String },
    // ...
}
impl NikaError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::TemplateError { .. } => "NIKA-041",
            // ...
        }
    }
}
```

After:
```rust
#[derive(Debug, NikaError)]
pub enum NikaError {
    #[nika_error(code = "NIKA-041", message = "Template error: {reason}")]
    TemplateError { reason: String },
    // ...
}
// `code()`, `Display`, `thiserror::Error` all derived
```

**D3: `#[event_kind]`** (saves ~500 LOC across 98 variants in `nika-event/log.rs`)

Generates display, category classification, filter matcher from enum variants.

**D4: `#[derive(Catalog)]`** for `nika-core/catalogs/*` (saves ~800 LOC)

Derives `all()`, `find()`, `find_by_alias()` from `#[catalog(id = "...", aliases = [...])]`.

**M1: `transform!` declarative macro** (saves ~2,000 LOC in `binding/transform.rs`)

Before:
```rust
("upper", NikaTransform::Upper) => { /* 15 lines null-guard + dispatch */ },
("lower", NikaTransform::Lower) => { /* 15 lines */ },
// x64
```

After:
```rust
transform! {
    upper(s: &str) -> String = s.to_uppercase(),
    lower(s: &str) -> String = s.to_lowercase(),
    trim(s: &str) -> String = s.trim().to_string(),
    // x64 in one-liners
}
```

Generates match arms, null guards, and registry entry.

### Total savings

| Pattern | LOC today | LOC after | Saved |
|---------|-----------|-----------|-------|
| `#[builtin_tool]` × 63 | ~5,600 | ~1,600 | **~4,000** |
| `#[nika_error]` × 114 | ~2,300 | ~900 | **~1,400** |
| `#[event_kind]` × 98 | ~1,500 | ~900 | **~600** |
| `#[derive(Catalog)]` × ~8 | ~1,200 | ~400 | **~800** |
| `transform!` × 64 | ~2,500 | ~500 | **~2,000** |
| **TOTAL** | **~13,100** | **~4,300** | **~8,800** |

**Net boilerplate reduction: 8,800 LOC (1.8% of total).** Not a reviewability revolution, but:
- Adding a 64th tool drops from 90 LOC to 25 LOC — 4x faster
- Adding an error variant drops from 6 LOC to 2 LOC — no code() method maintenance
- Transform catalog fits on one screen instead of 15

### Dependencies

```toml
# nika-macros/Cargo.toml
[package]
name = "nika-macros"
version.workspace = true
edition = "2021"

[lib]
proc-macro = true

[dependencies]
syn = { version = "2.0", features = ["full"] }
quote = "1.0"
proc-macro2 = "1.0"
darling = "0.20"
proc-macro-error2 = "2.0"

[dev-dependencies]
trybuild = "1.0"
macrotest = "1.0"
```

### Testing strategy

```
nika-macros/
├── src/
│   ├── lib.rs
│   ├── builtin_tool.rs      (~400 LOC)
│   ├── nika_error.rs        (~300 LOC)
│   ├── event_kind.rs        (~250 LOC)
│   ├── catalog.rs           (~200 LOC)
│   └── transform_macro.rs   (~350 LOC)
└── tests/
    ├── trybuild.rs          (pass + fail cases)
    ├── expand/              (macrotest snapshots)
    │   ├── builtin_tool.rs
    │   ├── nika_error.rs
    │   └── ...
    └── expand_expected/     (expanded snapshot files)
```

### Phase assignment

**Phase 3 — `nika-macros`** (was pending in v2.1/v2.2, now firm).

- **When:** lands BEFORE Phase 12 (nika-builtin). The `#[builtin_tool]` derive is the #1 use case and Phase 12 would have to rewrite boilerplate otherwise.
- **Duration:** 2-3 weeks, 1 engineer. 1.5-2 weeks with 2 engineers.
- **Blocker removal:** once Phase 3 lands, every subsequent phase that touches builtins/errors/events/catalogs gets the macro benefit automatically.

### Anti-patterns

- **Don't hide logic in macros.** `#[builtin_tool]` generates the trait impl, not the tool body.
- **Don't skip `trybuild` tests.** Proc-macro bugs ship silently; tests must cover compile-failure paths.
- **Don't use `inventory` on 2024+ toolchains** — linkme is better for const-init friendliness.
- **Don't couple error derive to thiserror internals.** Generate standalone `impl std::error::Error` to avoid thiserror 2.0 → 3.0 break risks.

---

## 5. Revised phase roadmap (v2.3)

| Phase | Title | Target | When | Status |
|-------|-------|--------|------|--------|
| 0-11 | Foundation + effect crates + Provider bridge | — | S1-S4 | ✅ |
| 15 | main.rs → nika-cli migration | main.rs <900 LOC | **S5** | ⏳ NEXT |
| **3** | **nika-macros (4 derives + 1 macro)** | **~8,800 LOC saved** | **S6 (before Phase 12)** | 🆕 FIRM |
| 12 | nika-builtin (63 tools + sealed trait) | nika-engine 160k → 137k | S7-S9 | ⏳ |
| 13 | nika-verb-{infer,exec,fetch,invoke,agent} | nika-engine 137k → ~115k | S10-S11 | ⏳ |
| 14 | nika-runtime + nika-cache | nika-engine ~115k → **95-100k** | S12-S13 | ⏳ |
| 6 | error_domains promotion (180 sites) | NIKA-XXX consolidation | S14 | ⏳ |
| 7 | LSP absorption (11.9k LOC dup) | -4000 LOC engine | S15 | ⏳ |
| 17 | nika-tui split (widgets/core/views/app) | TUI reviewability | S16 | ⏳ |
| 19 | Type system hardening (TaskId, sealed, Runner<State>) | — | S17 | ⏳ |
| **21** | **Zero-unwrap migration** | **5576 → <50 with REASON** | **S18-S21 (4-week sprint)** | 🆕 FIRM |
| 22 | Performance hardening + PGO | Binary size + cold start | S22 | ⏳ |
| **23** | **nika-cache AST incremental (blake3 + moka)** | **nika check 200ms → <5ms** | **S23** | 🆕 FIRM |
| 20 | Polish (public API, cargo bloat, compile time) | launch-ready | S24 | ⏳ |
| **24** | **POST-LAUNCH: `comemo` sub-file memoization** | LSP cursor <16ms | post-launch | 🆕 |
| **25** | **POST-LAUNCH: Phase 15 finale (binding + dag to L0)** | **nika-engine ~80k** | post-launch | 🆕 |

### Ordering rationale

- **Phase 3 BEFORE Phase 12.** Macros land first so Phase 12 gets them for free.
- **Phase 21 (zero-unwrap) BEFORE Phase 22 (PGO).** Can't PGO a codebase that still panics randomly.
- **Phase 23 (incremental cache) AFTER Phase 14 (nika-cache extracted).** Can't build on a crate that doesn't exist yet.
- **Phase 24-25 are POST-LAUNCH.** Honest. Sub-80k and comemo are not blockers.

---

## 6. Honest numbers — launch state

| Metric | v2.2 target (vague) | **v2.3 target (firm)** | Stretch |
|--------|---------------------|------------------------|---------|
| `nika-engine` LOC | "~140k" | **≤100k** | 85k |
| Unwrap in prod code | "<5000" | **<50 with REASON, 0 in hot path** | 0 total |
| Incremental compute | not mentioned | **blake3 cache on Analyzed, <5ms repeat** | comemo sub-file |
| Proc-macro boilerplate saved | "7000 LOC (Phase 3)" | **~8,800 LOC (4 derives + transform!)** | — |
| `main.rs` LOC | "<500" | **<900** | <500 |
| Total crates | "~32" | **27-28** | 32 |
| Tests | "10,850+" | **10,800+** | — |
| Clippy warnings | 0 | **0 with deny unwrap** | — |
| God files (>2k LOC, non-test) | 2 | **0** (main.rs, error.rs, runner/mod.rs, binding/resolve.rs all split) | — |

### What we will ship that peer projects don't

1. **Trait-based effect injection for 9 side effects** (Clock, Fs, Blob, HttpClient, Shell, Provider, RunExecutor, HitlPrompt, MediaContext). Only ruff has 1 (`System`), tauri has 1 (`Runtime`). **Nika will have 9.**
2. **6-layer prompt injection defense wired into the runtime** (Shield). No peer ships this at all.
3. **Taint analysis at workflow file level** via `nika check --security`. No peer ships this.
4. **Zero-unwrap enforcement in workspace** with CI ratchet. Matches Fuchsia / Deno strictness.
5. **blake3-keyed incremental AST cache** with `.nika/cache/analyzed/` persistent + moka hot layer. Not standard in workflow engines.
6. **4 proc-macros eliminating ~8,800 LOC boilerplate** via linkme distributed_slice registration.

### What we will NOT ship for launch (with reason)

- **Salsa incremental compute** — retrofit cost too high (2 months async retrofit), payoff is LSP-specific. Post-launch if profiling demands.
- **Sub-80k nika-engine** — requires Phase 15 (binding + dag to kernel) which is disruptive. Post-launch.
- **comemo sub-file memoization** — conditional on LSP cursor latency profiling. Post-launch.
- **231-crate decomposition** (Zed model) — over-splitting tax (linker time, trait resolution). We stop at 27-28.

---

## 7. Validation gates (v2.3 update)

### Per-commit (unchanged from v2.2)
- cargo test --workspace --lib (no regressions)
- cargo clippy --workspace -- -D warnings (zero warnings)
- git history: 1 logical change per commit, Nika 🦋 co-author

### Per-phase (NEW for v2.3)

```bash
# Phase 3 (nika-macros) completion gate
cargo test -p nika-macros
cargo expand -p nika-engine --lib 2>&1 | grep -c "impl BuiltinTool"  # verify macro expansion
trybuild failing tests pass

# Phase 12 completion gate
wc -l tools/nika-engine/src/**/*.rs  # ≤140k
cargo test --workspace --lib  # all pass

# Phase 14 completion gate
wc -l tools/nika-engine/src/**/*.rs  # ≤100k ← HARD CEILING
cargo clippy --workspace -- -D warnings

# Phase 21 completion gate (zero-unwrap)
cargo clippy --workspace --all-targets --message-format=json 2>/dev/null \
  | jq -r 'select(.reason=="compiler-message") | .message.code.code // empty' \
  | grep -c "unwrap_used\|expect_used\|panic\|indexing_slicing"
# must be <50 with all documented via `// REASON:`

# Phase 23 completion gate (incremental cache)
./scripts/bench-nika-check.sh  # repeat run on unchanged file <5ms
```

### Pre-launch final
- `wc -l tools/nika-engine/src/**/*.rs` **≤ 100,000**
- Unwrap count in prod (non-test) **< 50, all with `// REASON:`**
- `nika check` repeat run on unchanged workflow **< 5ms**
- Proc-macro expansion test suite green
- Show HN post mentions: "zero-unwrap workspace", "blake3 AST cache", "9 effect trait boundaries"

---

## 8. TL;DR brutal

```
v2.2 a dit "everything is in scope, pas de deferred".
v2.3 dit "OK mais VOILA les numéros".

nika-engine:      ≤100k LOC pour launch (pas <80k — c'est post-launch)
unwrap:           CI ratchet day 1 + 6-10 semaines migration (pas "warn")
incremental:      blake3 cache, PAS salsa (2 semaines pas 2 mois)
proc-macros:      4 derives + 1 macro, Phase 3 AVANT Phase 12 (firm)

9 nouveaux traits d'injection (9 > ruff's 1, tauri's 1, zero for the rest)
5 à 6 phases supplémentaires (21, 23) en plus de Constellation v2.2
~18 sessions total pour atteindre l'état launch, pas 13

Launch features qui comptent vraiment:
  1. Shield — runtime-level prompt injection defense (unique)
  2. Taint analysis en pre-exec — `nika check --security` (unique)
  3. Single binary sans runtime — `brew install nika` (vs JVM/Python/Node)
  4. Zero-unwrap workspace — ecosystem benchmark move (ahead of most peers)
  5. blake3 AST cache — `nika check` repeat sub-5ms (ahead of n8n/Dify/Airflow)
  
Launch features qu'on arrête de vanter:
  1. "9 providers" — commodity
  2. "63 tools" — commodity
  3. "Fast because Rust" — non-argument (LLM calls dominate)
  4. "Structured output" — Instructor does it
```

---

## Appendix: Research agent outputs

**Agent 1 (Salsa / incremental):** 780 words, cutoff May 2025, high confidence on architecture
**Agent 2 (proc-macros):** agent failed gracefully — no web tools. Covered from training data.
**Agent 3 (zero-unwrap):** 900 words, detailed migration plan, high confidence backed by Deno/Fuchsia configs
**Agent 4 (decomposition):** 780 words, detailed LOC math + rust-analyzer/ruff precedents, medium-high confidence

All reports archived in session output files under `/private/tmp/claude-501/-Users-thibaut-dev-supernovae-nika/c6fc4706-2404-43f6-b14e-617ab3d21ec7/tasks/`.
