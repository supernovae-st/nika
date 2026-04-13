# nika-kernel Trait Audit — Priority-Ranked Findings

> Synthesis of the `spn-rust:rust-pro` research agent's audit of `nika-kernel` (L0.5) trait shape. Dispatched 2026-04-10.

**Overall verdict:** 60% well-designed, 30% under-specified, 10% architecturally mistaken. The `TaskScope` splinter pattern and `BuiltinTool` sealed trait are textbook. `HttpClient` is wrong-shaped for the `fetch:` verb. The proposed `trait Verb` should NOT be built.

---

## Per-trait findings

### `ShellExecutor` — ✅ well-shaped, needs cancellation

**File:** `tools/nika-kernel/src/shell.rs`
**Impl:** `nika-exec-runner::TokioShell` (344 LOC)

**Shape:** Single-method `async fn run(cmd: ShellCommand) -> Result<ShellResult, ShellError>`. `ShellCommand` is a clean DTO (program, args, env, cwd, timeout, stdin, shell). This is the C-OBJECT ideal.

**Missing:** `CancellationToken` integration. The engine's `exec.rs` uses `tokio::select!` between `cancel_token.cancelled()` and the process future. The trait has no cancellation concept. Fix: add `cancel: Option<CancellationToken>` field to `ShellCommand`.

**Missing error variant:** `Cancelled`.

**Why hasn't engine adopted it?** Because the 471-LOC `exec.rs` is 70% security validation and 30% `tokio::process`. Moving the 30% doesn't reduce LOC meaningfully. The real win is creating `nika-verb-exec` where the security pipeline moves with the spawn logic.

**Verdict:** Fine. Add cancellation in S12. Adopt in S13 via `nika-verb-exec`.

---

### `HttpClient` — ❌ under-designed, not fit for `fetch:`

**File:** `tools/nika-kernel/src/http.rs`
**Impl:** `nika-http::ReqwestClient` (194 LOC)

**Shape:** Single method `send(HttpRequest) -> HttpResponse`. DTOs for request (method, url, headers, body, timeout, follow_redirects) and response (status, headers, body, final_url).

**Red flags for `fetch:`:**
1. **No redirect chain capture.** `fetch.rs:249-305` builds a custom `reqwest` client per-request to capture the redirect chain via `Policy::custom(...)`. Inexpressible through the trait.
2. **No DNS pinning hook.** `fetch.rs` does `resolve_and_pin_ssrf` → `builder.resolve(host, addr)` to defeat TOCTOU rebinding. No trait hook.
3. **No streaming body.** `HttpResponse::body: Bytes` forces full-buffer. `fetch.rs` uses `read_body_with_limit` / `read_bytes_with_limit` for early abort on size. Completely inexpressible.
4. **No cookie jar.** `fetch.rs` does `builder.cookie_provider(Arc::clone(&self.cookie_jar))` for `session: true`. Per-request cookie state not in the trait.
5. **`follow_redirects: bool` per-request is a lie.** `reqwest` sets redirect policy at CLIENT level, not per-request. The `nika-http` impl even admits this in a comment (`lib.rs:103`). The trait promises a behavior the impl cannot deliver.
6. **`HttpError::SsrfBlocked`** is defined but dead code — the engine returns `NikaError::PolicyViolation` before ever calling `HttpClient::send`.

**Works for:** `provider.rs` (OpenAI-compat), `invoke:` builtin HTTP, registry pings, webhooks. Anything that is "send request, get response" — about 50% of Nika's HTTP use cases.

**Does NOT work for:** `fetch:` verb. It is specialized.

**Verdict:** Do NOT try to bolt cookies/cache/rate-limit/robots onto `HttpClient` — that turns a clean general-purpose trait into a fetch-shaped monster and breaks the 50% of callers that don't need those concerns. **Priority 1 fix:** add `send_streaming` for the 50MB early-abort case (covers point 3). **Accept** that `nika-verb-fetch` owns its own `reqwest::Client` directly for the custom redirect policy + DNS pinning + cookie jar (points 1, 2, 4, 5). Document the exception clearly.

See ADR-003 and [07-session13-extraction-1.md](07-session13-extraction-1.md) Part 4 for the `FetchAux` bundle approach.

---

### `Provider` — ✅ well-shaped, missing vision/tool-use methods

**File:** `tools/nika-kernel/src/provider.rs`
**Impl:** `nika-engine::provider::rig::RigProvider` via `kernel_bridge.rs`

**Shape:** `infer` + `infer_stream` + `name` + `capabilities`. Clean DTOs (`InferRequest`, `InferResponse`, `InferEvent`). `Pin<Box<dyn Stream>>` for streaming. Strong baseline.

**Missing:**
1. **Cancellation.** `infer` has no cancel token — callers drop the future, which works with `reqwest` but doesn't emit events for observability.
2. **`infer_vision`** — RigProvider has it, the trait doesn't. `infer.rs` bypasses the trait for vision content.
3. **`infer_with_tools`** — same story. Tool-use inference is concrete-only.
4. **`infer_with_options`** — called in `make_infer_callback` for structured output retry with `max_tokens` override. Exists on RigProvider, not trait.
5. **4 capability probes:** `supports_vision`, `supports_native_structured_output`, `supports_thinking`, `is_anthropic_compatible`. Concrete-only.
6. **`tokenize` / `count_tokens`** — `nika:token_count` builtin will need this eventually.

**Priority 1 fix (S14 Wave A):** add all 5 missing methods with default impls returning `Err(ProviderError::Unsupported)`. Override on `RigProvider` via the bridge. This unblocks `nika-verb-infer` extraction.

**`async_trait` note:** `Provider` uses `#[async_trait]` (necessary for dyn-safety). Acceptable because `Provider::infer` is called once per verb call, not in a hot inner loop.

---

### `Filesystem` — ⚠️ 9 methods is borderline

**File:** `tools/nika-kernel/src/filesystem.rs`

**Shape:** 9 methods (read / write / metadata / create_dir_all / remove_file / exists / glob / canonicalize / read_to_string).

**C-OBJECT says trait objects should be small.** 9 methods crosses into wide-surface territory.

**Security concern:** builtin file tools running under `current_is_tainted()` should take `&dyn FsRead`, not `&dyn Filesystem`. The ability to pass a read-only file capability to a tainted tool is a compile-time security feature.

**Priority 2 fix (S12 F4):** split into `FsRead` (read / read_to_string / metadata / exists / canonicalize / glob) + `FsWrite` (write / create_dir_all / remove_file). Blanket impl umbrella: `impl<T: FsRead + FsWrite> Filesystem for T {}` so existing code compiles. Consumers can narrow to `&dyn FsRead` where appropriate.

**Priority 3 fix:** drop `read_to_string` — caller does `String::from_utf8`. Minimal interface. Defer to Phase 15.

---

### `Clock` — ✅ 3 methods, fine but underused

**File:** `tools/nika-kernel/src/clock.rs`

**Shape:** `now` / `sleep` / `elapsed`. Good.

**Missing:** `sleep_until(Instant)` for the deadline pattern in `fetch.rs:487` (`if Instant::now() >= overall_deadline`). Cheap add, Priority 3.

---

### `events.rs` — ✅ empty marker module (correct)

**File:** `tools/nika-kernel/src/events.rs`

Canonical `EventEmitter` trait lives in `nika-event` (L1) to avoid signature divergence. The kernel `events.rs` is an empty marker. Good diamond-layer discipline.

---

### `TaskScope` splinters — ✅ THE BEST PART of the kernel

**File:** `tools/nika-kernel/src/scope.rs`

`TaskResults + BindingScope + MediaStaging + RecordStore + VaultLookup + InvocationContext` with a blanket umbrella impl. Textbook C-SEND-SYNC + capability-oriented design. **This is how the other traits should look.**

**Note:** `scope.rs` already defines the correct narrow interfaces. It says:
> `async fn run_fetch(scope: &mut S, ...) where S: BindingScope + MediaStaging`

This is EXACTLY the per-verb typed context pattern ADR-002 proposes. The splinter traits already exist — ADR-002 extends them with per-verb struct wrappers (`FetchCaps<'a>`).

**Priority 3:** rename `RecordStore` (write) → `RecordSink` to stop colliding with `RecordQuery` (read).

---

### `BuiltinTool` — ✅ sealed, well-designed

**File:** `tools/nika-kernel/src/builtin.rs`

Sealed trait with `#[doc(hidden)] pub mod __sealed`. Explicit `Pin<Box<dyn Future>>` instead of `#[async_trait]` — correct choice because it's called 63 times and `async_trait` adds `Box::pin` per call.

Error type well-partitioned (`InvalidArgs`/`Io`/`Parse`/`Timeout`/`Schema`/`Denied`/`AssertionFailed`/`Other`) with NIKA-XXX codes in Display. Constructor helpers (`invalid_params`, `tool_error`, newly `denied` from S12.P1) are macro-friendly.

**Red flag (accepted):** `call(&self, args: String) -> Result<String, _>` — JSON-in, JSON-out. Loses type safety at every call site, forces every tool to re-parse its args. **Acceptable** because it matches MCP wire format. Documented in the trait doc comment.

**Verdict:** ship-it. No changes needed.

---

### `BlobStore` — ✅ clean

**File:** `tools/nika-kernel/src/store.rs`

5 methods (put/get/exists/stat/delete), one error type with `NotFound`/`Io`/`TooLarge`. Good.

**Minor gap:** `CasStore` in nika-engine has additional methods (`workspace_default`, `with_working_dir`) not on the trait. The trait's `get` is equivalent for blob retrieval — the extras are construction-time conveniences, not runtime methods. Fine.

---

### `task_local.rs` — ✅ load-bearing, no trait

**File:** `tools/nika-kernel/src/task_local.rs`

Not a trait — `tokio::task_local!` cells. The pattern (trust and depth via task_local, never as function args) is deliberate and correct. S12.P1 just added `current_is_tainted()` helper. Well-documented.

---

### Existing helper traits (`MediaContext`, `RunExecutor`, `HitlPrompt`, `RecordQuery`)

**Files:** `tools/nika-kernel/src/scope.rs`

Escape-hatch traits to break cycles (nika-builtin → nika-engine → nika-builtin). `MediaContext` explicitly documents why `compute_blocking<F,T>` is NOT on the trait (generics break vtable). Object-safety compile guards at `scope.rs:280`. Excellent Rust discipline.

**Verdict:** ship-it.

---

## Cross-cutting concerns

### `#[async_trait]` everywhere

Every I/O trait uses `#[async_trait::async_trait]`. As of Rust 1.75 (Dec 2023), native AFIT (`async fn` in traits) is stable.

**Constraint:** for traits that must be object-safe via `dyn`, you can't fully escape async_trait yet because of dyn-safety rules.

**Recommendation:** keep async_trait for hot-path traits that need `dyn` (`ShellExecutor`, `HttpClient`, `Provider`). Use `Pin<Box<dyn Future>>` explicitly for the highest-frequency trait (`BuiltinTool` — already does this correctly). Don't mix styles within the same module.

**Priority 3 (post-launch):** migrate to `#[trait_variant::make]` (Niko Matsakis / lang team blessed pattern) once it's more mature. Phase 15+.

---

## The Verb trait that doesn't exist yet

**Recommendation: DO NOT BUILD IT.**

The plan originally proposed `trait Verb { async fn run(&self, ctx: VerbCtx) -> Result<Value, NikaError> }`. **This is wrong** for a closed fixed-size set of 5 verbs.

See [ADR-001](02-adr-001-enum-dispatch.md) for the full argument. Summary: `enum TaskAction` (already in nika-core) + match-based dispatch + free functions per verb crate beats trait objects on every axis that matters for a closed sum (compile-time exhaustiveness, zero-cost dispatch, no boxing, no `async_trait` in the hot path).

**Priority 1: do not add `Verb` trait to nika-kernel.** Instead, add the 5 per-verb `Caps<'a>` structs ([ADR-002](03-adr-002-typed-contexts.md)) as plain data types. Verb crates expose free `pub async fn run()` functions that take these caps.

---

## VerbCtx monolithic vs per-verb typed — typed, always

**Recommendation: per-verb typed contexts.**

Monolithic `VerbCtx { shell: Option<...>, http: Option<...>, ... }` violates C-NEWTYPE (encode invariants in the type system) and the parse-don't-validate principle. Every verb impl begins with `ctx.http.as_ref().expect("fetch needs http")` which is a runtime panic waiting to happen.

**Solution:** per-verb typed contexts with compile-time capability enforcement. See [ADR-002](03-adr-002-typed-contexts.md) and [01-architecture-vision.md](01-architecture-vision.md) for the code sketches.

**Precedent:** axum::extract, Restate SDK `Context<'ctx>`, Ruff `Checker<'a>`.

---

## PolicyEnforcer — concrete or trait?

**Recommendation: trait in nika-kernel, concrete impl in `nika-policy` L1 crate.**

Reference implementations in the Rust ecosystem use concrete structs for their central config-like objects:
- **rustc's `Session`** — concrete struct, passed as `&Session`. Not a trait. Why? Because there's only one impl and the team explicitly rejected the abstraction.
- **rust-analyzer's `Config`** — concrete in `crates/rust-analyzer/src/config.rs`, 3000+ LOC. Not a trait.
- **cargo's `Config`** — same.

**The rust-pro agent argued** for Nika to follow this pattern: concrete `PolicyEnforcer` in `nika-policy`, passed as `&PolicyEnforcer`, no trait. Rationale: traits are for polymorphism you actually need.

**The rust-architect agent argued** for a trait: because the `nika-http::ReqwestClient` redirect closure also needs to check policy hop-by-hop, and pulling in `nika-policy` creates a dep cycle unless there's a trait in `nika-kernel`.

**Resolution (adopted in S12 plan):** SMALL trait in `nika-kernel` (4 methods), concrete impl in `nika-policy`. Best of both worlds:
- Verbs take `&dyn PolicyChecker` (trait object, mockable)
- `nika-http` can depend on the trait without circular dep
- The concrete impl in `nika-policy` is still a plain struct with real behavior
- Trait has 4 methods only, not 40 — small enough to justify

**Priority 1 (S12 F1):** add `PolicyChecker` trait to `nika-kernel` as the first foundation commit.

---

## Priority-ranked changes

### Priority 1 — BLOCKING (Session 12 must complete)

1. **Kill the `Verb` trait idea.** Use `enum TaskAction` (already exists) + per-verb free functions. Dispatcher in nika-runtime does exhaustive match.
2. **Define per-verb typed contexts** (`ExecCaps`, `FetchCaps`, `InferCaps`, `InvokeCaps`, `AgentCaps`) in nika-kernel. No `Option<>` fields. Follow axum::extract.
3. **Add `CancellationToken` to every verb context.** Move from TaskExecutor field to context struct.
4. **Add `PolicyChecker` trait to nika-kernel.** 4 methods (`check_exec`, `check_fetch`, `check_tool_call`, `is_host_allowed`).
5. **Move `PolicyEnforcer` to `nika-policy` L1 crate as a concrete struct** implementing `PolicyChecker`.
6. **Decide HttpClient scope explicitly.** Thin trait for provider/registry/invoke. `nika-verb-fetch` owns reqwest directly for the custom redirect policy. Document this in `http.rs` doc comment.
7. **Add `HttpClient::send_streaming`** for fetch's 50MB early-abort case.
8. **Add cancellation to `ShellCommand`.**
9. **Split `Filesystem` into `FsRead` + `FsWrite` splinters.**

### Priority 2 — STRONGLY RECOMMENDED (Sessions 13/14)

10. **Extract `nika-extract` crate** (1327 LOC of pure extract logic). ADR-003.
11. **Enrich `Provider` trait** with `infer_vision`, `infer_with_tools`, `infer_with_options`, 4 capability probes. S14 Wave A.
12. **Create `ProviderRegistry` trait** in nika-runtime (wraps the DashMap cache).
13. **Extract `nika-shield` crate** for SecurityContext/SpotlightFence/CanarySystem. S14 Wave A.
14. **Add `Cancelled` variant** to `ShellError`, `HttpError`, `ProviderError`.
15. **Add `sleep_until(Instant)` to `Clock`.**

### Priority 3 — NICE-TO-HAVE (Post-launch, Phase 15+)

16. **Migrate verb-boundary traits to native AFIT** via `#[trait_variant::make]` where dyn-safety allows.
17. **Rename `scope::RecordStore` → `scope::RecordSink`.**
18. **Drop `read_to_string` from `Filesystem`** — caller does `String::from_utf8`.
19. **Remove `HttpError::SsrfBlocked`** if engine never returns it through the trait.
20. **Add `tokenize` to `Provider`** for `nika:token_count` builtin.

---

## Summary verdict

The nika-kernel layer is **60% well-designed and load-bearing**. The TaskScope splinter pattern, BuiltinTool sealed trait, BlobStore, and Clock are textbook Rust design and should be preserved unchanged.

The **30% under-specified** portion (HttpClient gaps, Provider missing vision/tool methods, Filesystem too wide) is fixable in S12/S14 with additive changes and no breaking migrations.

The **10% architecturally mistaken** portion — the `Verb` trait and monolithic `VerbCtx` proposals — **should not be built**. Replace with enum dispatch + per-verb typed contexts per ADR-001 and ADR-002.

**The good news:** the kernel is strong enough that the verb extraction refactor is a straightforward "make engine use the traits it already has, then extract". The bad news is that S14 has to enrich the Provider trait significantly before infer can move — that's the scope cost of the refactor being honest about its dependencies.

---

## References

- Research source: `spn-rust:rust-pro` agent, dispatched 2026-04-10
- ADR-001: [enum dispatch](02-adr-001-enum-dispatch.md)
- ADR-002: [typed contexts](03-adr-002-typed-contexts.md)
- ADR-003: [nika-extract](04-adr-003-nika-extract.md)
- ADR-004: [delete TaskExecutor](05-adr-004-delete-task-executor.md)
- Rust API Guidelines: https://rust-lang.github.io/api-guidelines/
