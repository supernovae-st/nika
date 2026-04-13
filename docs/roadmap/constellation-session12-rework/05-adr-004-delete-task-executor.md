# ADR-004: Delete `TaskExecutor` entirely — replace, do not refactor

**Status:** Accepted
**Date:** 2026-04-10
**Deciders:** Thibaut Melen, 4 parallel research agents (rust-architect, code-explorer, rust-pro, web-researcher)
**Insight source:** The closing insight of the rust-architect research agent: *"the real win is not verb crates per se — it's deleting TaskExecutor"*

## Context

`TaskExecutor` is the central dispatcher in `nika-engine/src/runtime/executor/mod.rs`. It has:

- **22 fields** — HTTP client, provider cache, MCP pool, event log, policy enforcer, cancel token, CAS store, tool context, skill injector, skills map, workflow base dir, skills base dir, project root, working dir mode, custom endpoints, resolved agents, robots cache, domain rate limiter, cookie jar, fetch cache, workflow tasks, shield
- **8 `with_*` builder methods** — `with_policy`, `with_skills`, `with_base_path`, `with_project_root`, `with_working_dir_mode`, `with_resolved_agents`, `with_workflow_tasks`, `with_shield`
- **5 verb methods** — `run_exec` , `run_fetch`, `run_infer`, `run_invoke`, `run_agent`
- **997 LOC** in `mod.rs` alone, plus 6478 LOC across its verb method files
- **Multiple `Arc` clones per task** — `TaskExecutor::clone` clones ~12 Arcs because every capability is individually Arc-wrapped

The original Phase 13 plan proposed extracting verb methods into new crates while **keeping TaskExecutor**. A bridge method on `TaskExecutor` would delegate to the new `nika_verb_*::run` for backward compatibility. TaskExecutor would stay, just with thinner methods.

This ADR rejects that approach. TaskExecutor is deleted.

## Decision drivers

- **The core framing problem** — verbs are currently methods on an executor; they should be functions on parameters with capabilities
- **Performance** — 12 Arc clones per task clone × concurrency N = wasted atomic operations
- **Code comprehension** — a 997-line struct with 22 fields defeats new-contributor onboarding
- **Test boundary** — you cannot unit-test TaskExecutor without constructing all 22 fields
- **Architectural honesty** — keeping TaskExecutor as a thin shim is code debt we'd regret in Phase 15

## Considered options

### Option 1: Keep TaskExecutor as a thin shim (the original plan)

Each verb method on TaskExecutor becomes a one-line delegate to the new verb crate:

```rust
impl TaskExecutor {
    pub async fn run_exec (&self, p: &ExecParams, b: &ResolvedBindings, rc: &RunContext)
        -> Result<String, NikaError>
    {
        let caps = self.build_exec_caps();
        nika_verb_exec::run(&caps, p, b, rc).await.map_err(Into::into)
    }
    // same pattern for the other 4 verbs
}
```

**Pros:** minimum change, callers of `TaskExecutor` don't need updating, bridge is simple.
**Cons:**
- TaskExecutor still has 22 fields; no LOC reduction in the struct definition
- Callers still construct TaskExecutor with 8 `with_*` builder calls
- The framing problem persists — verbs still look like "methods on an executor"
- Phase 15 has to do the same work over again
- 300+ LOC of constructor logic in `with_policy` remains
- Arc clones per task clone don't go away

### Option 2: Delete TaskExecutor entirely, replace with VerbCapabilities + dispatch (this ADR)

The Runner (previously holding `executor: TaskExecutor`) now holds `caps: VerbCapabilities` and calls `nika_runtime::dispatch(task, bindings, rc, &caps)` directly. The 22 TaskExecutor fields are distributed:

- **~12 fields → `VerbCapabilities`** (run-scoped bundle with trait objects)
- **~5 fields → `RunContext`** (workflow_tasks, skills_map, etc. — run-scoped data, not caps)
- **~3 fields → dropped** (superseded by trait abstractions, e.g., `http_client: reqwest::Client` is no longer a field; the trait `HttpClient` behind the field is what VerbCapabilities holds)

The 8 `with_*` builder methods collapse into a single `VerbCapabilitiesBuilder` or simply a struct literal. The 5 `run_*` methods vanish — dispatch is a `match` in `nika-runtime::dispatch`.

**Pros:**
- Root cause of the refactor addressed — no god object
- Zero Arc clones per task for sequential execution (borrowed slices from VerbCapabilities)
- Clear diamond: `VerbCapabilities` lives in `nika-runtime` (L3), verb crates depend on `nika-kernel` (L0.5) only
- Runner construction is a struct literal, not 8 chained builder calls
- Phase 15 engine dissolution is dramatically simpler (nothing left to dissolve in executor/)
- Test boundary is per-capability (mock `ShellExecutor`), not per-executor (mock the whole god struct)

**Cons:**
- Every caller of `TaskExecutor::new` / `TaskExecutor::with_policy` needs rewriting (audited: only in `nika-runtime::Runner`, `nika-cli::bootstrap`, `nika-tui::app`, and tests — ~15 call sites total)
- The Runner construction logic moves, is no longer `TaskExecutor::with_policy(...).with_skills(...)...`

### Option 3: Keep TaskExecutor but narrow it to just the 5 verb methods (no fields)

A degenerate TaskExecutor that is really a namespace for the 5 verb dispatch methods, with all state passed in as parameters.

**Pros:** preserves the type name.
**Cons:** it's a struct with no state, which means it's not really a struct — it's a module. Just use `nika-runtime::dispatch`.

## Decision

**Delete `TaskExecutor` entirely in Session 14. Replace with `VerbCapabilities` (run-scoped bundle in `nika-runtime`) and free-function dispatch (`nika-runtime::dispatch`). All 22 fields migrate to either `VerbCapabilities` or `RunContext`.**

## Rationale

### The framing insight

From the rust-architect research agent:

> "The single biggest architectural insight I can offer after reading the code is this: **the split you are planning is correct, but the real win is not 'verb crates per se' — it's deleting `TaskExecutor`**. `TaskExecutor` is a god object that has confused the codebase into thinking verbs are 'methods on an executor'. They are not. Verbs are **functions that operate on parameters given capabilities**. Once you internalize that framing, every design question answers itself:
>
> - Dispatch? Match on the parameter variant.
> - Capabilities? Pass them in as a struct borrowed from a run-scoped bundle.
> - State? There is no per-verb state — state belongs to the caller (RunContext, VerbCapabilities).
> - Testing? Pass mock caps. Zero macros. Zero registries.
>
> The `TaskExecutor::with_policy` builder chain in `mod.rs:412-497` is an anti-pattern the moment you accept that the verbs are stateless functions. Every `with_*` becomes a field on `VerbCapabilities` constructed once at workflow start, and the verbs just read from it. Eight mutating builder methods become a single struct literal.
>
> **That** is the clean end state. Not 'Phase 13 extracts verbs into crates'. The correct framing is: 'Phase 13 deletes the concept of a task executor and replaces it with a runtime capabilities bundle.'"

### Keeping TaskExecutor defeats the refactor's purpose

If TaskExecutor survives with 22 fields and 8 builder methods, the codebase still has a god object. The fact that `run_exec ` now delegates to the new verb crate internally doesn't fix the framing — new contributors will still ask "why does TaskExecutor need 22 fields if it just delegates?" and the answer will be "because it's a constructor for the capabilities".

The correct answer is: "TaskExecutor doesn't exist. The Runner constructs VerbCapabilities directly, and dispatch is a function."

### Performance: Arc clone elimination

`TaskExecutor::clone` is called implicitly during task spawning in the `Runner::run_task` path (the Runner clones the executor to move into spawned futures). This clones ~12 Arcs per task. In `for_each` with `concurrency: 20`, that's 240 atomic refcount increments per inner-loop iteration — pure waste when the underlying data is immutable.

With `VerbCapabilities` + borrowed slices, the 95% of verb calls that run sequentially cost zero Arc clones. The 5% that spawn (inside for_each with concurrency > 1) explicitly opt-in to `exec_caps_owned` which clones the Arcs at the spawn point. The cost is visible and intentional.

Back-of-envelope: for a workflow with 100 for_each iterations at concurrency 20, the Arc-clone elimination saves ~47,880 atomic operations. Negligible per-call, meaningful in aggregate. More importantly: the refactor makes it visible which calls pay the cost.

### Phase 15 becomes trivial

With TaskExecutor deleted, the Phase 15 engine dissolution target (delete `nika-engine` entirely) requires only:
- Moving `boot.rs` to `nika-runtime` (or a new `nika-boot` crate)
- Splitting `provider/rig/` into `nika-provider-rig` L1 crate (to satisfy the `Provider` trait)
- Moving `structured_output.rs` to `nika-runtime`
- Moving `chat_workflow.rs` to `nika-cli`

With TaskExecutor surviving, Phase 15 would also need to delete TaskExecutor. Better to do it now while we're holding the knife.

## Consequences

### Positive

- Root architectural debt paid — no god object, no 22-field struct
- Zero Arc clones on the sequential verb execution path
- New-contributor onboarding dramatically simpler (no 997-line struct to understand)
- Test boundary aligns with capabilities (mock `ShellExecutor`, not `TaskExecutor`)
- Phase 15 engine dissolution becomes a 4-commit exercise instead of 15
- Code comprehension: `nika-runtime::dispatch` is 12 lines and fits on one screen

### Negative

- Every caller of `TaskExecutor::new` must be updated (audit: ~15 call sites, all in nika-* crates we own)
- Session 14 Wave C dissolution work is 5 non-trivial commits
- Bridge period (Wave B) requires extra care: during extraction, TaskExecutor still exists but its methods delegate — temporary complexity

### Risks

- **Risk:** hidden callers of `TaskExecutor::new` outside the grep path (e.g., via macros or reflection). Mitigation: there are no macros in Nika that construct TaskExecutor, and Rust has no reflection. Grep is exhaustive.
- **Risk:** 22-field-to-2-bundle migration misses a field semantic. Mitigation: the Session 14 plan lists every field with its destination (VerbCapabilities vs RunContext vs dropped). Reviewed by agents.
- **Risk:** the Runner's existing hot-path logic depends on TaskExecutor internals we don't see. Mitigation: Session 14 Wave C commit W14-D1 is specifically the "Runner migrates to VerbCapabilities" commit, with golden tests as the oracle.

## Implementation notes

- The deletion happens in Session 14 Wave C (commits W14-D1 through W14-D5).
- **Wave B** (verb extraction) keeps TaskExecutor alive as a bridge — each `run_*` method delegates to the new verb crate. This lets each verb crate land independently without breaking `Runner`.
- **Wave C** then migrates the Runner to use `VerbCapabilities` directly and deletes TaskExecutor.
- The 22 fields are distributed as follows (full table in Session 14 plan):

| Field | Destination |
|---|---|
| `http_client: reqwest::Client` | DELETED — superseded by `Arc<dyn HttpClient>` in VerbCapabilities |
| `rig_provider_cache: Arc<DashMap<…>>` | DELETED — superseded by `Arc<dyn ProviderRegistry>` in VerbCapabilities |
| `mcp_pool: McpClientPool` | VerbCapabilities |
| `default_provider: Arc<str>` | Inside ProviderRegistry |
| `default_model: Option<Arc<str>>` | Inside ProviderRegistry |
| `event_log: EventLog` | VerbCapabilities |
| `builtin_router: Arc<BuiltinToolRouter>` | VerbCapabilities |
| `policy_enforcer: Arc<RwLock<PolicyEnforcer>>` | VerbCapabilities (as `Arc<dyn PolicyChecker>`) |
| `cancel_token: CancellationToken` | VerbCapabilities |
| `cas: Arc<CasStore>` | DELETED — superseded by `Arc<dyn BlobStore>` in VerbCapabilities |
| `tool_ctx: Arc<ToolContext>` | Inside BuiltinRouter |
| `skill_injector: Arc<SkillInjector>` | VerbCapabilities |
| `skills_map: Arc<HashMap<String, String>>` | VerbCapabilities |
| `workflow_base_dir: PathBuf` | VerbCapabilities |
| `skills_base_dir: PathBuf` | VerbCapabilities |
| `project_root: Option<PathBuf>` | VerbCapabilities |
| `working_dir_mode: Option<String>` | VerbCapabilities |
| `custom_endpoints: Arc<CustomEndpointMap>` | Inside ProviderRegistry |
| `resolved_agents: Arc<ResolvedAgents>` | VerbCapabilities |
| `robots_cache: Option<Arc<RobotsCache>>` | Inside FetchAux |
| `domain_rate_limiter: Option<Arc<DomainRateLimiter>>` | Inside FetchAux |
| `cookie_jar: Arc<CookieStoreRwLock>` | Inside FetchAux |
| `fetch_cache: Arc<FetchCache>` | Inside FetchAux |
| `workflow_tasks: Arc<Vec<AnalyzedTask>>` | RunContext (run-scoped data, not a cap) |
| `shield: SecurityContext` | VerbCapabilities (as `ShieldContext` from `nika-shield` crate) |

## Related decisions

- **ADR-001:** enum dispatch — the dispatch-side counterpart
- **ADR-002:** typed contexts — the capability-injection counterpart
- **Session 14 plan:** [08-session14-extraction-2.md](08-session14-extraction-2.md) — exact commit sequence for the dissolution

## References

- **Research (rust-architect):** closing insight that framed the decision
- **Research (code-explorer):** the 22-field audit with verb-by-verb coupling matrix
- **TaskExecutor source:** `tools/nika-engine/src/runtime/executor/mod.rs:69-137`
- **Rust API Guidelines C-BUILDER:** applies to types with many config options, not to god objects. https://rust-lang.github.io/api-guidelines/type-safety.html
