# Constellation V2.3 — Mega Plan (Sessions 12/13/14)

> **Supersedes:** the original Phase 13 of `2026-04-08-constellation-v2-mega-plan.md` and `2026-04-10-constellation-session12-handoff.md`.
> **Date:** 2026-04-10 · **Launch gate:** 2026-05-05 · **J-25.**

## North star

Transform the `nika-engine` monolith (148,792 LOC, 22-field `TaskExecutor` god object) into a diamond-layered architecture with 5 verb crates dispatching through a typed capability bundle. **Engine LOC target: ≤100k** (Constellation V2.3 firm target). **Post-S14 target: ~138k.** Full ≤100k closure in Phase 15+.

## What was learned on 2026-04-10

Four parallel research agents (code-explorer, rust-architect, web-researcher studying Restate/Ruff/uv, rust-pro kernel audit) independently converged on an architecture the original plan didn't propose. See [`01-architecture-vision.md`](01-architecture-vision.md) for the end state and ADRs [001-004] for the key decisions.

**Critical insight:** the nika-kernel L0.5 layer already contains 10 well-designed trait definitions with production implementations in sibling crates (`nika-exec-runner`, `nika-http`, `nika-blob`, etc.) — but `nika-engine` bypasses all of them. `exec.rs` calls `tokio::process::Command::new()` directly. `fetch.rs` uses `reqwest::Client` directly. **The entire diamond L1 layer is dead code.** Session 12's foundation phase is the first step that actually makes the kernel traits load-bearing.

---

## Three-session roadmap

### Session 12 — Foundation (IN PROGRESS)

**Scope:** Kernel trait extension + two new L1/L2 crates. **Zero verb extraction. Zero TaskExecutor changes.**

**Status:** 6 commits already landed (D/P/E phases):
- S12.D1 hard error on agent cwd (`3897870e8`)
- S12.D2 reject `..` in glob patterns (`c92075fe2`) — TDD reproduced a real `vault.enc` leak
- S12.D3+E1 canonicalize cache + hard error (`2381c491c`)
- S12.P1 `current_is_tainted()` + `BuiltinError::denied()` (`ffdfa770d`)
- S12.P2 `file/limits.rs` u64 unification (`30f7a67a5`)
- S12.P3 `file/test_util.rs run_as()` (`1c5d48954`)

**Remaining 10 commits** (tonight, ~4h):
1. `feat(kernel): PolicyChecker trait` — object-safe, 4 methods, 3-4 unit tests
2. `feat(kernel): HttpClient::send_streaming + HttpStreamResponse` — additive, default error impl
3. `feat(kernel): ShellCommand::cancel: Option<CancellationToken>` — TokioShell via `tokio::select!`
4. `feat(kernel): split Filesystem → FsRead + FsWrite splinters` — blanket umbrella impl
5. `feat(policy): create nika-policy L1 crate` — verbatim move of PolicyEnforcer (1263 LOC)
6. `chore(engine): delete duplicated policy code`
7. `feat(extract): create nika-extract L2 crate` — verbatim move of extract.rs (1327 LOC)
8. `chore(engine): delete runtime/executor/extract.rs`
9. `feat(kernel): add ExecCaps/FetchCaps/InferCaps/InvokeCaps/AgentCaps` — structs only, not wired
10. `docs: ARCHITECTURE.md + session12 memory update`

**S12 metrics:**

| Metric | Before S12 | After S12 |
|---|---|---|
| Engine LOC | 148,792 | ~146,200 (−2,590) |
| Crates | 28 | **30** (+nika-policy, +nika-extract) |
| Tests | 10,769 | ~10,795 (+26: new trait + crate tests) |
| Binary size | not recorded | **118 MB** baseline recorded |

**S12 explicitly does NOT:** extract any verb, delete TaskExecutor, create nika-runtime, wire ExecCaps into any code.

**Details:** [`06-session12-foundation.md`](06-session12-foundation.md)

---

### Session 13 — Extraction Pass 1 (NEXT SESSION, FRESH CONTEXT)

**Scope:** Create `nika-runtime` L3 crate with `VerbCapabilities` + `dispatch()`. Extract exec + invoke + fetch as `nika-verb-*` L2 crates.

**18 commits, ~10-12h.** See [`07-session13-extraction-1.md`](07-session13-extraction-1.md) for commit-by-commit detail.

**Part 1 — nika-runtime scaffold (4 commits):**
- Create nika-runtime crate with VerbCapabilities struct
- `dispatch()` function with 5-arm match (4 arms are `todo!()` initially)
- Move `Runner` from nika-engine to nika-runtime
- Engine depends on nika-runtime for Runner

**Part 2 — nika-verb-exec (4 commits):**
- Create nika-verb-exec with `pub async fn run(caps: ExecCaps<'_>, ...)`
- Wire `dispatch()` Exec arm
- Bridge TaskExecutor::run_exec to delegate
- Delete engine/runtime/executor/exec.rs (−471 LOC)

**Part 3 — nika-verb-invoke (3 commits):** Same pattern. Cleanest verb (522 LOC).

**Part 4 — nika-verb-fetch (5 commits):**
- Create `FetchAux` bundle (cookies/cache/rate-limit/robots) — 4 small kernel traits OR keep concrete
- Create nika-verb-fetch depending on `nika-extract` (from S12) + `HttpClient::send_streaming`
- Wire dispatch Fetch arm
- Delete engine/runtime/executor/fetch.rs (−1399 LOC)

**Part 5 — Session close (2 commits):** ARCHITECTURE.md + memory.

**S13 metrics:**

| Metric | After S12 | After S13 |
|---|---|---|
| Engine LOC | ~146,200 | **~143,800** (−2,392) |
| Crates | 30 | **34** (+nika-runtime, +nika-verb-exec, +nika-verb-invoke, +nika-verb-fetch) |
| Tests | ~10,795 | ~10,810 |

---

### Session 14 — Extraction Pass 2 + TaskExecutor Dissolution

**Scope:** Enrich Provider trait, create `nika-shield` L1 crate, extract `nika-verb-infer` (2157 LOC monster) + `nika-verb-agent` (602 LOC + 2500 LOC of rig_agent_loop/), **delete TaskExecutor entirely**.

**20 commits, ~14-18h over 2 working days.** See [`08-session14-extraction-2.md`](08-session14-extraction-2.md) for commit-by-commit detail.

**Wave A — Prerequisites (4 commits):**
- Enrich `Provider` trait with `infer_vision`, `infer_with_tools`, `infer_with_options`, 4 capability probes
- Fill `impl Provider for RigProvider` with new methods
- `ProviderRegistry` trait + impl in nika-runtime
- Create `nika-shield` L1 crate (move SecurityContext/SpotlightFence/CanarySystem)

**Wave B1 — nika-verb-infer (5 commits):**
- Create crate with submodules: prompt/vision/guardrails/callbacks/structured/run
- 2157 LOC monolith becomes ~300 LOC orchestrator calling free functions
- Wire dispatch Infer arm
- Bridge + delete infer.rs (−2157 LOC)

**Wave B2 — nika-verb-agent (4 commits):**
- **The highest-risk step:** verbatim move of entire `rig_agent_loop/` directory (2500 LOC, 7 files) with import path surgery (~30-50 unresolved imports)
- Implement `run.rs` calling the moved agent loop
- Wire dispatch Agent arm
- Bridge + delete agent.rs (−602) + decompose.rs (−352) + rig_agent_loop/ (−2500) = −3454 LOC

**Wave C — TaskExecutor dissolution (5 commits):**
- Runner builds VerbCapabilities directly
- **Delete TaskExecutor struct** + 300 LOC of constructor logic
- Delete `runtime/executor/` directory entirely
- Remove shield re-export shims
- Mark nika-engine as thin shim in Cargo.toml

**Wave D — Close (2 commits):** ARCHITECTURE.md + memory + release binary size comparison.

**S14 metrics:**

| Metric | After S13 | After S14 |
|---|---|---|
| Engine LOC | ~143,800 | **~138,500** (−5,300 net) |
| Crates | 34 | **37** (+nika-shield, +nika-verb-infer, +nika-verb-agent) |
| Tests | ~10,810 | ~10,830 |
| TaskExecutor | 22-field god object | **DELETED** |
| Binary size | 118 MB | ~120-123 MB (prediction) |

---

## End-state architecture (after S14)

```
L0    nika-core                AST (ExecParams, FetchParams, ...), policy config, trust
L0.5  nika-kernel              10 traits: Shell, Http, Provider, BlobStore, Clock,
                               Filesystem (FsRead+FsWrite), PolicyChecker, BuiltinTool,
                               events, scope splinters
                               + InferOptions, ToolDef, ToolChoice (pure data)
                               + ExecCaps/FetchCaps/InferCaps/InvokeCaps/AgentCaps structs
      nika-kernel-mock         5 hand-written mocks for trait testing
L1    nika-clock, nika-fs, nika-blob, nika-http (+send_streaming), nika-exec-runner
      nika-event, nika-policy (PolicyEnforcer impl), nika-shield
      (SecurityContext/SpotlightFence/CanarySystem)
L2    nika-core-effects:       nika-media, nika-mcp, nika-vault, nika-storage, nika-display
      nika-builtin             47+/63 builtin tools (sealed BuiltinTool trait)
      nika-extract             9 extract modes (pure, zero I/O)
      nika-verb-exec           pub async fn run(caps: ExecCaps<'_>, ...)
      nika-verb-invoke         pub async fn run(caps: InvokeCaps<'_>, ...)
      nika-verb-fetch          pub async fn run(caps: FetchCaps<'_>, ...) — depends on nika-extract
      nika-verb-infer          pub async fn run(caps: InferCaps<'_>, ...)
      nika-verb-agent          pub async fn run(caps: AgentCaps<'_>, ...) — owns agent_loop/
L3    nika-runtime             VerbCapabilities + dispatch() + Runner + decompose
                               (dissolution target of nika-engine)
      nika-engine (shim)       residual: provider/rig/, boot.rs, structured_output.rs,
                               chat_workflow.rs — target Phase 15 deletion
      nika-daemon              background daemon
L4    nika-cli, nika-tui, nika-serve, nika-lsp, nika-sdk, nika-init
L5    nika                     binary entry point, <900 LOC
```

**Crate count journey:** 28 (pre-S12) → 30 (post-S12) → 34 (post-S13) → 37 (post-S14) → ? (post-Phase 15, engine fully dissolved).

---

## Milestones & gate criteria

| Milestone | Gate | Target date |
|---|---|---|
| S12 foundation complete | All 10 S12 commits + `cargo test --workspace --lib` green + binary size baseline recorded | 2026-04-11 (tonight) |
| S13 extraction pass 1 | nika-runtime + 3 verb crates live, engine ~143k LOC | 2026-04-12 |
| S14 TaskExecutor dissolved | 5 verb crates, TaskExecutor deleted, engine ~138k LOC | 2026-04-14 |
| Phase 15 full engine dissolution | engine ≤100k, possibly deleted entirely | 2026-04-17 |
| **Launch** | `nika` v0.80.0 public, Show HN 14h Paris | **2026-05-05** |

**J-25 budget:** 4 working days for architecture refactor, 21 days for polish/distribution/launch prep. Feasible with the foundation landing tonight.

---

## Risk posture

The research agents unanimously flagged Session 14's `rig_agent_loop/` import surgery as the highest-risk single step (Wave B2, commit W14-C1). Session 13's nika-runtime scaffold (commits S13-P1-P3) is the second riskiest because it touches the Runner hot path.

Full mitigation plans in [`09-risk-register.md`](09-risk-register.md). Golden e2e tests (prerequisite, Session 12 closure) act as the safety net for every subsequent commit.

**Rollback target:** if any session goes sideways, `git reset --hard c5ea27438` (pre-S12 HEAD) is the fail-safe. User authorization required.

---

## References

- [`01-architecture-vision.md`](01-architecture-vision.md) — end-state design with code sketches
- [`02-adr-001-enum-dispatch.md`](02-adr-001-enum-dispatch.md) — why enum, not trait
- [`03-adr-002-typed-contexts.md`](03-adr-002-typed-contexts.md) — why per-verb borrowed slices
- [`04-adr-003-nika-extract.md`](04-adr-003-nika-extract.md) — why extract is its own crate
- [`05-adr-004-delete-task-executor.md`](05-adr-004-delete-task-executor.md) — why delete, not refactor
- [`09-risk-register.md`](09-risk-register.md) — complete landmine list
- [`10-migration-verification.md`](10-migration-verification.md) — how to prove each commit safe
- [`11-kernel-trait-audit.md`](11-kernel-trait-audit.md) — kernel trait shape review (priority-ranked)
