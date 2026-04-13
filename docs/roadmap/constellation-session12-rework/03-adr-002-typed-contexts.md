# ADR-002: Per-verb borrowed typed contexts, not a monolithic `VerbCtx`

**Status:** Accepted
**Date:** 2026-04-10
**Deciders:** Thibaut Melen, 4 parallel research agents
**Supersedes:** `struct VerbCtx { shell: Option<Arc<…>>, http: Option<Arc<…>>, … }` in the original Phase 13 handoff

## Context

Each verb needs a different subset of side-effect capabilities:

- **exec:** `ShellExecutor`, `PolicyChecker`, `EventLog`, `Clock`, `CancellationToken`, `workflow_base_dir`, optional `default_cwd`
- **fetch:** `HttpClient`, `BlobStore`, `PolicyChecker`, `EventLog`, `Clock`, `ShieldContext`, `FetchAux` (cookies/cache/rate-limit/robots), `CancellationToken`
- **infer:** `ProviderRegistry`, `BlobStore` (vision), `ShieldContext`, `SkillInjector`, `skills_map`, `skills_base_dir`, `PolicyChecker`, `EventLog`, `Clock`, `CancellationToken`, `workflow_base_dir`
- **invoke:** `McpClientPool`, `BuiltinToolRouter`, `PolicyChecker`, `EventLog`, `Clock`, `ShieldContext`, `CancellationToken`
- **agent:** everything infer needs + everything invoke needs + `ResolvedAgents` (agent presets)

**Question:** how should these capabilities be passed to each verb's `run()` function?

## Decision drivers

- **Compile-time least privilege** — a verb that doesn't need HTTP should be unable to accidentally reach HTTP
- **Zero-cost at the hot path** — for_each at `concurrency: 20` spawns 20 parallel verbs per iteration; unnecessary Arc clones matter
- **Testability** — each verb's test should construct EXACTLY the caps it needs with mocks, nothing more
- **Ergonomic at the call site** — building a verb's capabilities in `dispatch()` must be a few lines, not a builder chain

## Considered options

### Option 1: Monolithic `VerbCtx` with `Option<>` fields (the original plan)

```rust
pub struct VerbCtx {
    pub shell: Option<Arc<dyn ShellExecutor>>,
    pub http: Option<Arc<dyn HttpClient>>,
    pub provider: Option<Arc<dyn Provider>>,
    pub policy: Option<Arc<dyn PolicyChecker>>,
    pub cookie_jar: Option<Arc<CookieStore>>,
    // ... 15+ more fields ...
}
```

**Pros:** one type to pass around, simple `dispatch()`.
**Cons:** every verb body begins with `ctx.shell.as_ref().expect("exec needs shell")` — a runtime panic waiting to happen. Violates Rust API Guidelines C-NEWTYPE (encode invariants in types) and the "parse, don't validate" principle. Tests must construct a partially-filled `VerbCtx` with `None` for unused fields, which is ugly and error-prone. Adding a new capability is a diff across all 5 verbs even if only one verb uses it.

### Option 2: Per-verb typed contexts with borrowed slices (this ADR)

```rust
pub struct ExecCaps<'a> {
    pub shell: &'a dyn ShellExecutor,
    pub policy: &'a dyn PolicyChecker,
    pub events: &'a EventLog,
    pub clock: &'a dyn Clock,
    pub shield: &'a ShieldContext,
    pub cancel: &'a CancellationToken,
    pub workflow_base_dir: &'a Path,
    pub default_cwd: Option<&'a Path>,
}

pub struct FetchCaps<'a> { /* HTTP + cookies + cache + ... */ }
pub struct InferCaps<'a> { /* provider + shield + skills + ... */ }
pub struct InvokeCaps<'a> { /* mcp + builtins + ... */ }
pub struct AgentCaps<'a> { /* everything */ }
```

Constructed by accessor methods on the run-scoped `VerbCapabilities` bundle:

```rust
impl VerbCapabilities {
    pub fn exec_caps(&self) -> ExecCaps<'_> {
        ExecCaps {
            shell: &*self.shell,
            policy: &*self.policy,
            events: &self.events,
            clock: &*self.clock,
            shield: &self.shield,
            cancel: &self.cancel_token,
            workflow_base_dir: &self.workflow_base_dir,
            default_cwd: self.default_cwd.as_deref(),
        }
    }
}
```

**Pros:**
- It is a COMPILE ERROR to pass a verb without the capabilities it needs — no `Option<>`, no `.unwrap()`
- Borrowed slices cost a single reference per field (no Arc clones for sequential execution)
- Adding a new capability only touches the verbs that actually need it
- Tests construct `ExecCaps { ... }` with exactly the fields that verb reads, with mocks
- Matches the existing `nika-kernel::scope` splinter-trait pattern (already load-bearing in the codebase)

**Cons:**
- Five types instead of one (manageable — they live in `nika-kernel`)
- `for_each` with `tokio::spawn` can't cross the `'a` lifetime → need `exec_owned()` variant for spawned paths (documented, explicit cost)

### Option 3: Registry pattern (`ctx.get::<dyn ShellExecutor>()`)

```rust
impl VerbCtx {
    pub fn get<T: Any>(&self) -> Option<&T> { /* runtime typemap lookup */ }
}
```

**Pros:** flexible, no type proliferation.
**Cons:** erases types, loses compile-time safety, runtime panic/Option at every capability access, no IDE autocomplete, not Rust-idiomatic.

### Option 4: Generic `VerbCtx<Caps>` with trait bounds

```rust
async fn run<C: HasShell + HasEvents + HasPolicy>(ctx: C, params: &ExecParams) -> …
```

**Pros:** maximum compile-time safety.
**Cons:** ergonomic nightmare, trait bound soup, `C` must implement many separate "Has" traits, hard to debug. Also: the existing verbs already use this implicitly via `TaskScope` splinters, and the pattern has not scaled well beyond 3-4 traits.

## Decision

**Use Option 2 — per-verb typed borrowed context structs (`ExecCaps<'a>`, etc.) defined in `nika-kernel`, constructed by accessor methods on `VerbCapabilities` (the run-scoped bundle in `nika-runtime`).**

## Rationale

### Compile-time capability enforcement is a security win

Nika Shield's Layer 4 is *capability-based tool restriction* — untrusted tasks cannot access dangerous tools. The Rust type system can enforce this at the verb level: if `ExecCaps` does not contain a `PolicyChecker` that authorizes unrestricted shell execution, there is literally no way for the verb body to run the command. **Compile-time proof** that is stronger than any runtime check.

Monolithic `VerbCtx` with Options defeats this — the verb body has to check `ctx.policy.as_ref().unwrap()`, which is a runtime assertion. Typed contexts make it impossible to construct an ExecCaps without a policy.

### Borrowed slices are zero-cost at the hot path

A single `TaskExecutor::clone()` today clones ~12 Arcs (atomic increments) because every capability is behind an Arc. `for_each` with `concurrency: 20` spawns 20 tasks, each cloning 12 Arcs → 240 atomic increments per inner-loop iteration. That's pure waste when the capabilities are immutable within a run.

Borrowed slices (`&'a dyn ShellExecutor`) cost ONE reference per field. For the 95% of verb calls that run sequentially (not via spawn), zero Arc clones. For the 5% that run via `tokio::spawn`, we provide an `exec_owned()` variant that explicitly clones the Arcs at the spawn point — the cost is visible and intentional.

### Ruff precedent: `&mut Checker<'a>` god-context-by-borrow

Ruff's `Checker<'a>` struct has 30+ borrowed fields and is passed by `&mut` to every lint rule function. The Rust team reviewed Ruff's architecture and praised this pattern. Every lint rule takes `checker: &mut Checker<'_>` and pulls only the slices it needs via method calls. Zero `Arc`, zero heap allocation for dispatch. This is the blueprint for per-verb typed contexts.

**Difference from Ruff:** we use per-verb struct slices instead of accessor methods on one god-context, because the 5 verbs have non-overlapping capability sets (fetch needs HTTP, exec does not), and typed per-verb structs make the difference visible at the type level. Ruff's rules all need the same semantic model, so one `Checker` is correct for them.

### Restate SDK precedent: `Context<'ctx>` trait-sliced bundle

The Restate Rust SDK exposes capabilities as separate traits on its `Context<'ctx>` struct: `ContextSideEffects`, `ContextTimers`, `ContextReadState`, etc. Each handler takes `Context<'ctx>` and uses ONLY the traits it needs via method calls (`ctx.run(...)`, `ctx.sleep(...)`, `ctx.get(...)`). This is capability-oriented programming done right in Rust.

**Difference from Restate:** Restate has one `Context<'ctx>` struct that implements all the trait facets, because Restate handlers can request any subset of capabilities dynamically. Nika has 5 STATIC verb shapes, so we lift the trait facets into explicit per-verb structs. This is strictly stronger enforcement — a Restate handler could call `ctx.run()` from a context that only needs timers (at compile time it works), whereas Nika's exec verb cannot even name `HttpClient` because `ExecCaps` doesn't have it.

### axum::extract precedent: typed state extraction per handler

axum's `FromRequest` trait lets each handler declare the exact state slice it needs:

```rust
async fn get_user(State(db): State<Arc<Db>>, Extension(auth): Extension<Arc<Auth>>) -> … { … }
```

The handler's signature is the source of truth for its dependencies. Adding a 6th dependency to one handler doesn't touch any other handler. Nika's per-verb caps follow the same principle.

### Tests become trivial

```rust
let shell = MockShellExecutor::new().with_output("hello\n", 0);
let policy = MockPolicyChecker::allow_all();
// ... other mocks ...
let caps = ExecCaps { shell: &shell, policy: &policy, /* ... */ };
let result = nika_verb_exec::run(&task_id, &params, &bindings, &rc, caps).await.unwrap();
```

There is no way to misconfigure the test — the compiler enforces that every capability is present. No `VerbCtx::builder().with_shell(...).with_policy(...).build()` ceremony.

## Consequences

### Positive

- Compile-time capability enforcement (no `.unwrap()` in verb bodies to access caps)
- Zero Arc clones for sequential verb execution
- Each verb's capability set is self-documenting via its `Caps<'a>` struct
- Adding a new capability touches only the verbs that need it
- Tests construct exactly the caps the verb reads, no more
- Matches Ruff, Restate SDK, axum::extract — battle-tested patterns

### Negative

- Five context types instead of one (manageable, all in `nika-kernel`)
- `VerbCapabilities::exec_caps()` etc. is one more accessor method per verb (5 total)
- Lifetime parameter `<'a>` surfaces in verb function signatures (idiomatic Rust, but adds a character)
- `tokio::spawn` at the `for_each` level can't cross `'a` → need explicit `exec_owned()` variant for that path

### Risks

- **Risk:** verb signatures drift as capabilities are added. Mitigation: each `Caps<'a>` struct is `#[non_exhaustive]` and the `VerbCapabilities::*_caps()` accessors are the only constructors. All changes flow through one file per verb.
- **Risk:** `FetchCaps` becomes unwieldy (9+ fields). Mitigation: bundle fetch-specific auxiliaries (`cookies`, `cache`, `rate_limit`, `robots`) into a single `FetchAux` struct held via `&'a FetchAux`. Cuts the FetchCaps field count.

## Implementation notes

- The 5 `*Caps<'a>` structs live in `nika-kernel` (or a small dedicated `nika-kernel/src/verb_caps.rs` module).
- `VerbCapabilities` lives in `nika-runtime` (it holds the owned Arcs).
- Builder methods on `VerbCapabilities`: `exec_caps()`, `fetch_caps()`, `infer_caps()`, `invoke_caps()`, `agent_caps()` — each returns a borrowed slice.
- For owned contexts (needed for `tokio::spawn` in `for_each`): `exec_caps_owned()`, etc. These clone the underlying Arcs explicitly. The cost is visible at the call site.
- Each `Caps<'a>` struct is `#[non_exhaustive]` so adding a field is not a breaking change for external consumers (though there shouldn't be any — the structs are only used by verb crates).

## Related decisions

- **ADR-001:** enum dispatch — the counterpart on the dispatch side
- **ADR-004:** delete TaskExecutor — the motivation for moving capabilities out of the god struct

## References

- **Ruff `Checker<'a>`:** https://github.com/astral-sh/ruff/blob/main/crates/ruff_linter/src/checkers/ast/mod.rs
- **Restate `Context<'ctx>`:** https://github.com/restatedev/sdk-rust/blob/main/src/context/mod.rs
- **axum::extract typed state:** https://docs.rs/axum/latest/axum/extract/index.html
- **Rust API Guidelines C-NEWTYPE:** https://rust-lang.github.io/api-guidelines/type-safety.html#newtypes-provide-static-distinctions-c-newtype
- **Parse, don't validate (Alexis King):** https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/
- **nika-kernel existing scope splinters:** `tools/nika-kernel/src/scope.rs` (existing pattern to mirror)
