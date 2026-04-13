# ADR-001: Enum dispatch, not `trait Verb`, for the 5-verb set

**Status:** Accepted
**Date:** 2026-04-10
**Deciders:** Thibaut Melen (decision authority), 4 parallel research agents (analysis)
**Supersedes:** `trait Verb { async fn run(...) }` proposal in the original Phase 13 handoff

## Context

Nika has 5 sacred verbs (`exec`, `fetch`, `infer`, `invoke`, `agent`) declared in `schema: nika/workflow@0.12`. The project's feedback memory (`feedback_no_data_verb.md`) explicitly states: *"NEVER add new verbs. 5 verbs sacred."* The set is closed by design, by spec, and by user-imposed constraint.

The original Phase 13 plan proposed a `trait Verb` abstraction:

```rust
trait Verb: Sealed + Send + Sync {
    async fn run(&self, ctx: VerbCtx) -> Result<Value, NikaError>;
}
```

…with each concrete verb implementing the trait and dispatch through `Arc<dyn Verb>` stored in a registry. Before executing, the question was raised: is this the right Rust shape for a closed fixed-size set?

## Decision drivers

- **Closed set** — 5 verbs, immutable by spec and by project policy
- **Compile-time exhaustiveness** — adding a (forbidden) 6th verb MUST be a compile error, not a silent omission
- **Hot path** — `dispatch()` is called once per task, and `for_each` with `concurrency: N` spawns N concurrent tasks; overhead matters
- **Test boundary** — each verb must be unit-testable in isolation without the other 4
- **Rust idioms** — follow how the ecosystem handles closed sums (std, rustc, serde, etc.)
- **Zero-cost abstraction principle** — don't pay for flexibility you don't use

## Considered options

### Option 1: `trait Verb` with `Arc<dyn Verb>` dispatch (the original plan)

```rust
#[async_trait]
trait Verb: Sealed + Send + Sync {
    async fn run(&self, ctx: VerbCtx) -> Result<Value, NikaError>;
}
struct VerbRegistry { verbs: HashMap<String, Arc<dyn Verb>> }
```

**Pros:** extensible, familiar OO pattern, clean crate boundary.
**Cons:** boxing per call, vtable dispatch kills inlining, requires `#[async_trait]` macro, no compile-time exhaustiveness (silent omission when adding a variant to `TaskAction`), forces object-safe signatures which tends toward `&dyn Any` or type-erased parameters, registry lookup is stringly-typed, inviting future "add 6th verb" pressure which project policy rejects.

### Option 2: `enum Verb` with match-based dispatch (this ADR)

The existing `nika_core::ast::TaskAction` is already the correct shape:

```rust
pub enum TaskAction {
    Exec(ExecParams),
    Fetch(FetchParams),
    Infer(InferParams),
    Invoke(InvokeParams),
    Agent(AgentParams),
}
```

Dispatch becomes a 12-line exhaustive match in `nika-runtime`:

```rust
match &task.action {
    TaskAction::Exec(p)   => nika_verb_exec::run(p, bindings, rc, vc.exec_caps()).await,
    TaskAction::Fetch(p)  => nika_verb_fetch::run(p, bindings, rc, vc.fetch_caps()).await,
    TaskAction::Infer(p)  => nika_verb_infer::run(p, bindings, rc, vc.infer_caps()).await,
    TaskAction::Invoke(p) => nika_verb_invoke::run(p, bindings, rc, vc.invoke_caps()).await,
    TaskAction::Agent(p)  => nika_verb_agent::run(p, bindings, rc, vc.agent_caps()).await,
}
```

Each verb crate exposes exactly one `pub async fn run()` — a free function, no trait implementation.

**Pros:** zero-cost dispatch (inlineable across LTO), compile-time exhaustive (adding a 6th variant breaks the build in exactly one place), no trait object boxing, no `async_trait` macro, no registry, no runtime string lookup, each verb function signature explicitly declares its capabilities via typed parameters, mirrors how the existing `TaskAction` AST already works.
**Cons:** the dispatcher crate (`nika-runtime`) must depend on all 5 verb crates — no "pluggable" verb loading. **This is the correct trade-off** because plugable verbs are explicitly rejected.

### Option 3: Free async functions with a top-level CLI-style dispatcher (uv pattern)

Same as Option 2 but without even wrapping the match in a function — put it directly in `Runner::run_task()`. Skipped because centralizing dispatch in `nika-runtime::dispatch()` is cleaner for testing and for error conversion at the boundary.

## Decision

**Use Option 2 — `enum TaskAction` + exhaustive `match` in `nika-runtime::dispatch()` + free `pub async fn run()` per verb crate. No `trait Verb`. No `Box<dyn Verb>`. No registry.**

## Rationale

### Rust ecosystem precedent is unanimous

Every fixed-arity sum in the Rust ecosystem uses `enum`, not `Box<dyn Trait>`:

- `std::task::Poll<T>` — fixed `Ready | Pending`
- `std::ops::ControlFlow<B, C>` — fixed `Break | Continue`
- `syn::Expr` — fixed AST node kinds
- `serde_json::Value` — fixed `Null | Bool | Number | String | Array | Object`
- `proc_macro::TokenTree` — fixed token kinds
- `rustc::HIR` and `rustc::MIR` — giant enum trees
- `rustc_codegen_*` — free functions exported from backend crates, not trait objects
- `ruff::Rule` — enum of 800+ lint rules with match-based dispatch via `Checker<'a>`
- `uv::Commands` — enum of ~20 subcommands, free-function dispatch in `main.rs`
- `rust-analyzer::ide::SymbolKind` — enum

The idiomatic Rust rule is: **closed set → enum, open set → trait**. Nika has a closed set.

### The cost of `Box<dyn Verb>` is not zero

- One heap allocation per task execution (boxing the `Arc<dyn Verb>` clone into the dispatch future)
- Vtable lookup defeats the compiler's ability to inline verb bodies into the dispatcher hot path
- `async_trait` itself boxes the returned future, adding a second `Box::pin` per call
- Object-safety constraints force all verb methods to have the SAME signature, which means `Params` must be either a single enum (you've just pushed the match one level deeper) or `&dyn Any` (type erasure — a crime in a systems codebase)
- For `for_each: concurrency: 20`, that's 20 boxed futures × 20 vtable lookups per iteration of the inner loop. Measurable.

Ruff has 800+ lint rules and deliberately uses an enum because they benchmarked the difference. Nika has 5 verbs. The case is even stronger.

### Exhaustiveness is a safety feature, not a nuisance

With `enum TaskAction`, if someone adds a `Data(DataParams)` variant (which policy forbids), the compiler emits an error at every `match` site until it is handled. With `HashMap<String, Arc<dyn Verb>>`, the new variant silently returns "unknown verb" at runtime. The compile error is a feature — it forces architectural discipline.

### Crate boundary is preserved without a trait

The concern with Option 2 is: "doesn't `nika-runtime` need to depend on all 5 verb crates?" Yes — and **this is correct**. There should be exactly one place that knows about all 5 verbs, and that place is `nika-runtime::dispatch()`. Spreading that knowledge across a registry is a feature only if you have multiple dispatchers. Nika does not.

Each verb crate depends downward on `nika-kernel` + `nika-core` + `nika-event` only. The runtime crate depends UPWARD on the verb crates. The dependency graph is DAG, the diamond invariant holds.

### Extensibility is explicitly rejected

Project policy: **no pluggable verbs**. Memory files `feedback_no_data_verb.md` and `project_windows_and_pkg_decisions.md` confirm. The `nika pkg` command was even nuked in Session 11 (`8fd26289b`) specifically to preserve this invariant. Paying runtime cost for flexibility you will never use is an anti-pattern.

## Consequences

### Positive

- Zero-cost dispatch through inlined match arms
- Compile-time exhaustiveness enforces architectural discipline
- No `async_trait` macro in dispatch path → no double-boxing
- No stringly-typed registry → no runtime lookup errors
- Each verb function's parameter list is the source of truth for its capabilities (see ADR-002)
- Test boundary per verb crate is trivial (mock caps, call the function)
- Matches idiomatic Rust ecosystem-wide

### Negative

- `nika-runtime` depends on all 5 verb crates (acceptable — centralized dispatch is correct)
- Cannot add a 6th verb without touching `nika-core::ast::TaskAction` + every match site (acceptable — feature, not bug)
- No runtime verb injection for tests — must use mocked capabilities instead (cleaner anyway)

### Risks

- **Risk:** someone proposes adding a `trait Verb` in the future "for testability". Mitigation: this ADR + the `nika-kernel-mock` crate's per-capability mocks demonstrate that trait `Verb` is unnecessary for testing. Unit tests mock `ShellExecutor`/`HttpClient`/`PolicyChecker`, not a `Verb` abstraction.

## Implementation notes

- The 5 verb crates (`nika-verb-exec`, etc.) each expose exactly ONE public async function: `pub async fn run(...)`.
- `nika-runtime::dispatch()` is the sole caller of all 5 `run()` functions.
- Each verb crate defines its own `Error` type (`ExecError`, `FetchError`, etc.) — aggregation happens at the dispatcher boundary via `impl From<ExecError> for RunError`, etc. This prevents verb crates from depending on the god `NikaError` enum.
- No `#[doc(hidden)] pub mod __sealed` is needed — the crate boundary IS the seal. Nobody outside `nika-runtime` can construct a `VerbCapabilities`, and without one you cannot reach the verbs.

## Related decisions

- **ADR-002:** typed per-verb contexts (borrowed slices from `VerbCapabilities`) — the capability-injection counterpart to this dispatch decision
- **ADR-004:** delete `TaskExecutor` — this ADR removes the last reason to keep the god struct

## References

- **Rust API Guidelines C-OBJECT:** "Implementors create their own types" — applies to OPEN extension, not closed sums. https://rust-lang.github.io/api-guidelines/type-safety.html
- **Ruff `Rule` enum:** https://github.com/astral-sh/ruff/blob/main/crates/ruff_linter/src/registry.rs
- **uv `Commands` enum:** https://github.com/astral-sh/uv/blob/main/crates/uv/src/commands/mod.rs
- **rustc codegen backends as free functions:** https://github.com/rust-lang/rust/blob/master/compiler/rustc_codegen_llvm/src/lib.rs
- **Restate SDK (Rust) — free handler functions + Context:** https://github.com/restatedev/sdk-rust/blob/main/src/context/mod.rs
- **Research synthesis:** 4 parallel agents (code-explorer, rust-architect, rust-pro, web-researcher) dispatched 2026-04-10 converged on this decision unanimously.
