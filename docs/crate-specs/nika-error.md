# Crate spec — `nika-error`

| | |
|---|---|
| Status | Phase 1 — first sub-crate of `nika-core` split |
| Layer | L0 (PURE, zero I/O, zero async) |
| LOC budget | ≤2,500 (target 2k, alarm at 2.4k) |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Source on `main` (reference) | `tools/nika-core/src/error.rs` (40 LOC), `tools/nika-core/src/error_codes.rs` (293 LOC) |
| Crate version | tracks workspace `0.90.0-alpha.1` (bumped at Phase 1 close) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |

---

## 1. Purpose

`nika-error` defines the **canonical user-facing error type** for the entire
Nika diamond — `NikaError` — plus the `NikaResult<T>` alias. Every other
crate either:

- emits its own internal error enum (e.g. `nika_extract::ExtractError`)
  and provides `impl From<X> for NikaError` to surface it to users, or
- returns `NikaResult<T>` directly when the failure is already a Nika-level
  concern (e.g. `nika-cli`, `nika-runtime` outermost paths).

This is the L0 anchor: it has zero `nika-*` dependencies, is reachable from
every crate via re-export from `nika-core` and (eventually) `nika`, and is
the sole owner of the `NIKA-XXX` error-code namespace.

It also resolves shadow zone **GATE 6** (`PRE_LAUNCH_GATES.md`): every
admitted `NIKA-XXX` ships with a Display parity golden test in this crate.

---

## 2. Public API surface

The crate exposes exactly these items at its root (`pub use` from `lib.rs`):

```rust
// Types
pub enum NikaError { /* see §3 */ }
pub type NikaResult<T> = Result<T, NikaError>;

// Re-exported helpers
pub use codes::{NikaCode, code_help};   // §5

// Convenience constructors (see §4 for the rationale)
impl NikaError {
    pub fn internal(context: impl Into<String>, detail: impl Into<String>) -> Self;
    pub fn validation(reason: impl Into<String>) -> Self;
}

// Required inherent methods
impl NikaError {
    pub fn code(&self) -> &'static str;       // "NIKA-053", "NIKA-999", etc.
    pub fn is_transient(&self) -> bool;       // for retry policy in upper layers
}

// Trait impls
impl std::fmt::Display for NikaError;          // via thiserror derive
impl std::fmt::Debug for NikaError;            // via thiserror derive
impl std::error::Error for NikaError;          // via thiserror derive
impl miette::Diagnostic for NikaError;         // via miette derive
```

`NikaError` is `#[non_exhaustive]`. Constructing it from outside the crate
is allowed via the convenience constructors and via documented variants;
the wildcard `Internal { context, detail }` is the catch-all every
`From<X>` impl must reach when no specific mapping fits.

---

## 3. Variant catalog — Phase 1 (L0) scope

For Phase 1 admission, `nika-error` ships exactly the variants whose
failure modes can be detected without I/O — i.e. parsing, schema validation,
DAG analysis, binding resolution, JSONPath validation, plus the catch-all.

L1+ variants (`NIKA-030..039` provider, `NIKA-053` blocked command,
`NIKA-093/096` IO, `NIKA-100..110` MCP, `NIKA-200..213` tool,
`NIKA-251..259` media, `NIKA-280..285` artifact, `NIKA-300..303` structured
output, `NIKA-320..324` record) are **out of scope here**. They ship with
their owner crates and reach `NikaError` via `From` impls. See §6 routing.

The 24 variants admitted in Phase 1:

| Code | Variant | Format string | Category |
|---|---|---|---|
| NIKA-001 | `WorkflowYamlSyntax { source }` | `[NIKA-001] workflow YAML is invalid: {source}` | Workflow |
| NIKA-002 | `WorkflowSchemaMissing` | `[NIKA-002] workflow header missing 'schema:' (use schema: "nika/workflow@0.12")` | Workflow |
| NIKA-003 | `WorkflowFileNotFound { path }` | `[NIKA-003] workflow file not found: {path}` | Workflow |
| NIKA-004 | `WorkflowStructure { detail }` | `[NIKA-004] workflow structure invalid: {detail}` | Workflow |
| NIKA-005 | `WorkflowSchemaMismatch { detail }` | `[NIKA-005] workflow does not match schema: {detail}` | Workflow |
| NIKA-013 | `SchemaFileMissing { path }` | `[NIKA-013] schema file not found: {path}` | Schema |
| NIKA-014 | `SchemaJsonInvalid { source }` | `[NIKA-014] schema file is not valid JSON: {source}` | Schema |
| NIKA-020 | `DagCycle { task_chain }` | `[NIKA-020] dependency cycle: {task_chain}` | DAG |
| NIKA-021 | `DagMissingDep { task, missing }` | `[NIKA-021] task '{task}' depends on missing task '{missing}'` | DAG |
| NIKA-022 | `DagDuplicateTaskId { id }` | `[NIKA-022] duplicate task id: '{id}'` | DAG |
| NIKA-041 | `BindingFormatInvalid { raw }` | `[NIKA-041] invalid binding expression: '{raw}' (expected {{with.alias}})` | Binding |
| NIKA-042 | `BindingAliasUnknown { alias }` | `[NIKA-042] unknown binding alias: '{alias}'` | Binding |
| NIKA-043 | `BindingTypeMismatch { alias, expected, actual }` | `[NIKA-043] binding '{alias}' type mismatch: expected {expected}, got {actual}` | Binding |
| NIKA-047 | `InvokeParamsInvalid { detail }` | `[NIKA-047] invoke params invalid: {detail}` | Binding |
| NIKA-050 | `InvalidPath { path }` | `[NIKA-050] invalid path syntax: '{path}' (use task_id.field.subfield)` | Path/Task |
| NIKA-052 | `BindingNullOrMissing { path }` | `[NIKA-052] binding '{path}' is null or missing (use ?? default)` | Path/Task |
| NIKA-055 | `InvalidTaskId { id, reason }` | `[NIKA-055] invalid task id '{id}': {reason}` | Path/Task |
| NIKA-056 | `InvalidDefault { raw, reason }` | `[NIKA-056] invalid default value '{raw}': {reason}` | Path/Task |
| NIKA-060 | `OutputNotJson { detail }` | `[NIKA-060] task output is not valid JSON: {detail}` | Output |
| NIKA-061 | `OutputSchemaMismatch { detail }` | `[NIKA-061] task output does not match declared schema: {detail}` | Output |
| NIKA-062 | `OutputNotSerializable { detail }` | `[NIKA-062] output value is not serializable: {detail}` | Output |
| NIKA-073 | `BindingFieldOnNonObject { path }` | `[NIKA-073] cannot access field on non-object value at '{path}'` | With block |
| NIKA-090 | `JsonPathUnsupported { path }` | `[NIKA-090] JSONPath '{path}' uses unsupported syntax (use simple paths like $.a.b)` | JSONPath |
| NIKA-094 | `JsonInvalid { source }` | `[NIKA-094] JSON parsing failed: {source}` | JSONPath |
| NIKA-999 | `Internal { context, detail }` | `[NIKA-999] internal: {context}: {detail}` | Catch-all |

All 25 variants get `#[diagnostic(code(nika::<snake_case>))]` via miette.
The `code()` inherent method returns the bracketed `NIKA-XXX` string for
log grepping, structured event emission, and docs cross-reference.

If a variant carries `source` (`#[error(... {source})]`), its inner type
must implement `std::error::Error + Send + Sync + 'static`. Phase 1
sources are limited to `serde_yaml::Error`, `serde_json::Error`, and
`std::io::Error` (used only for `WorkflowFileNotFound` and
`SchemaFileMissing` constructors — the variants themselves carry plain
`PathBuf`-stringified `path` to stay pure).

---

## 4. Custom methods — contracts

### `NikaError::code(&self) -> &'static str`

Total function. Returns a `'static` string of the form `"NIKA-XXX"` for
every variant. The exhaustiveness of this match is enforced by the test
suite (see §7), and by clippy's
`clippy::wildcard_enum_match_arm` lint.

For `Internal { .. }` returns `"NIKA-999"`.

### `NikaError::is_transient(&self) -> bool`

Total function. Returns `true` when the same operation, retried after a
short backoff, has a non-trivial chance of succeeding without input changes
(network blip, rate limit, transient resource unavailability).

For Phase 1 L0 variants: **all return `false`** (parsing/schema/DAG/binding
errors are deterministic on input and do not benefit from retry). The
`true` branch exists for L1+ variants that will land via `From` impls
(e.g. provider rate-limit, MCP timeout, IO `WouldBlock`).

### `NikaError::internal(context, detail)` and `NikaError::validation(reason)`

Convenience constructors so that callers don't need to write
`NikaError::Internal { context: c.into(), detail: d.into() }` everywhere.
`validation` was kept from the legacy `CoreError::ValidationError` shape
but now maps to a proper `Internal { context: "validation", detail: reason }`
(no separate variant — anything that was just "validation failed: {reason}"
is recoverable as internal-with-context).

---

## 5. `NikaCode` registry and `code_help`

The legacy `error_codes.rs` (293 LOC) provided `fix_suggestion_for_code(&str) -> Option<&'static str>`.
We carry that forward as:

```rust
pub mod codes {
    /// Strongly-typed handle on every NIKA-XXX code in the namespace.
    /// Constructed via `NikaCode::parse("NIKA-053")`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NikaCode(&'static str);

    impl NikaCode {
        pub const fn as_str(&self) -> &'static str { self.0 }
        pub fn parse(input: &str) -> Option<Self>;     // table lookup
    }

    /// Returns a short, actionable hint to help a user resolve an error.
    pub fn code_help(code: &str) -> Option<&'static str>;
}
```

`code_help` is a pure function, the renamed `fix_suggestion_for_code`. The
miette `help(...)` directive on each variant calls this internally so
terminal output carries the hint automatically; the function is also
public for the display layer (which only has the bracketed code string
from event logs) and for tooling.

The `NikaCode` registry covers **every** known NIKA-XXX (~80 codes), not
just Phase 1 L0 variants. This lets crates that ship later variants
reference codes from a single source of truth without re-defining them.

---

## 6. From-conversion convention (invariant #25)

Every leaf crate has its own `#[non_exhaustive]` error enum
(`HttpError`, `ExtractError`, `ProviderError`, etc.). At the boundary
between that crate and a Nika-level path, the conversion is:

```rust
// in nika-http/src/lib.rs, NOT in nika-error itself.
impl From<HttpError> for NikaError {
    fn from(e: HttpError) -> NikaError {
        match e {
            HttpError::SsrfBlocked { url } => NikaError::Internal {
                context: "fetch.ssrf".into(),
                detail: format!("blocked URL: {url}"),
            },
            HttpError::Timeout { url, after_ms } => NikaError::Internal {
                context: "fetch.timeout".into(),
                detail: format!("{url} timed out after {after_ms}ms"),
            },
            // wildcard arm catches future #[non_exhaustive] additions.
            other => NikaError::Internal {
                context: "fetch".into(),
                detail: format!("{other:?}"),
            },
        }
    }
}
```

Rules:

- **No `#[from]` cross-layer.** Verbose wins over magic. A grep for
  `impl From<` lists every boundary explicitly.
- **Wildcard arm is mandatory** because every leaf error is `#[non_exhaustive]`.
- **The leaf crate owns the impl**, not `nika-error`. `nika-error` cannot
  depend on `nika-http`/`nika-extract`/etc. (would invert the layer order).

This is a documented contract in this spec. Each leaf crate's spec must
restate which `From<...> for NikaError` it provides and which NIKA-XXX
context strings it uses, so the catalog stays discoverable.

---

## 7. Test plan (Gate 2 deliverables)

Tests live in `tools/nika-error/tests/` (integration tests against the
public API only — Gate 2 must NOT poke private internals).

| Test file | Scope | Count est. |
|---|---|---|
| `tests/display_parity.rs` | Display parity goldens — one assertion per variant matching the canonical format string from §3. Resolves shadow zone GATE 6 for L0 variants. | 25 |
| `tests/code_method.rs` | Exhaustive `code()` table — each variant maps to its bracketed `NIKA-XXX`, no two variants share. | 26 (1 uniqueness check + 25 mappings) |
| `tests/is_transient.rs` | Matrix: every variant currently returns `false`. Test guards future regressions. | 25 |
| `tests/code_help.rs` | `code_help` lookup: every L0 code present, every unknown code returns `None`, alias codes (NIKA-001/095, NIKA-041/074) share the same hint. | 6 |
| `tests/diagnostic_help.rs` | `miette::Diagnostic::help()` matches `code_help(self.code())` for the L0 variants that have a hint. | ≤25 |
| `tests/from_internal.rs` | `NikaError::internal("ctx", "detail")` round-trips: `code()` == "NIKA-999", display matches `[NIKA-999] internal: ctx: detail`. | 3 |
| `tests/property/debug_roundtrip.rs` (proptest) | For every variant, `Debug` and `Display` never panic on randomized payload. | 1 prop, ~256 cases default |
| `tests/property/code_total.rs` (proptest) | `code()` returns a non-empty `'static` `&str` matching `^NIKA-\d{3}$` for any variant. Constructed via a small variant strategy. | 1 prop |

L0 minimum per gate spec is 5 — we ship ~9 test files (mostly small,
focused), well above the bar. Property tests use `proptest` 1.x.

### TDD ordering for Gate 2 commit

1. Add `tools/nika-error/Cargo.toml` declaring `[package]` + thiserror +
   miette deps; add to root `Cargo.toml` `members = ["tools/nika-error"]`.
2. Add `tools/nika-error/src/lib.rs` with **all variants declared and
   thiserror Display strings filled in**, but with `code()` and
   `is_transient()` returning `unimplemented!()`. This makes Display
   tests PASS (thiserror generates Display from format strings) while
   `code()`-driven tests run but fail loudly when called.
3. Add `tools/nika-error/src/codes.rs` with `NikaCode` skeleton + an
   empty `code_help` returning `None` always. Code-help tests fail.
4. Add all `tests/*.rs` files. Failing tests are tagged `#[ignore = "Gate 3 IMPL"]`
   so `cargo test --workspace --lib` returns 0; goldens that already
   pass via thiserror Display run normally.

This satisfies the sacred "each commit compile + tests pass" rule while
honouring TDD intent: tests exist BEFORE the impl that satisfies them.

### Gate 3 commit

1. Implement `code()` (exhaustive match on variants, returns `&'static str`).
2. Implement `is_transient()` (returns `false` for all L0 variants).
3. Port the full `code_help` table (~80 entries) from
   `git show main:tools/nika-core/src/error_codes.rs`.
4. Remove every `#[ignore = "Gate 3 IMPL"]`. All tests pass.

### Gate 4 (CLIPPY 0)

`cargo clippy -p nika-error --all-targets --all-features -- -D warnings`.
The crate uses `#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]`
at lib root so test code can use ergonomic assertions while production
code stays strict.

### Gate 5 (MUTATION ≥90%)

Tool: `cargo-mutants 25.x`.
Targets: `code()`, `is_transient()`, `code_help`, `NikaCode::parse`.
Mutants killed by Gate 2 tests are removed; survivors trigger added test
cases until kill rate ≥90%.

### Gate 6 (PROPERTY)

Already covered in test plan (`tests/property/*`).

### Gate 7 (BENCHMARKS)

**Skipped.** `nika-error` has no hot path (errors are constructed at
boundaries, never in tight loops). Documented decision per Gate 7's
"if hot path" qualifier.

### Gate 8 (DOCS)

`cargo doc -p nika-error --no-deps` exits 0 with zero warnings. Every
`pub` item has a doc comment. Crate-level docstring summarizes purpose,
points to this spec.

### Gate 9 (CANARY E2E)

`tests/canary-error.nika.yaml` is a workflow that intentionally triggers
each L0 NIKA-XXX (e.g. invalid YAML, unknown alias, JSONPath misuse) and
asserts the emitted event carries the expected code. Lives in this crate
during Phase 1; migrates to `tests/canaries/` when nika-runtime exists.

### Gate 10 (PARITY LEGACY)

For the 5 variants inherited from `CoreError` (NIKA-050/055/056/090 +
ValidationError → Internal), assert the new `Display` output matches the
legacy output for ≥3 sample inputs each. Goldens stored in
`tests/legacy_parity/`.

### Gate 11 (REVIEW SWARM)

Three parallel agents (`nika-reviewer`, `rust-pro`, `rust-architect`)
review `lib.rs`, `codes.rs`, and the test suite. P0/P1 fixed same session.

### Gate 12 (ATOMIC COMMIT)

Single commit `feat(nika-error): admit to workspace — all 12 gates passed`.
Cargo.toml `version = "0.90.0-alpha.1"` is updated only at the very end of
Phase 1 (after all 5 sub-crates land), not per crate.

---

## 8. Dependencies

```toml
# tools/nika-error/Cargo.toml
[package]
name = "nika-error"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true
repository.workspace = true
homepage.workspace = true

[dependencies]
thiserror = { workspace = true }
miette    = { workspace = true, features = ["fancy-no-backtrace"] }
serde     = { workspace = true, features = ["derive"], optional = true }
serde_yaml = { workspace = true, optional = true }
serde_json = { workspace = true, optional = true }

[features]
default = []
# `serde` makes NikaError implement Serialize/Deserialize via serde derive.
# Off by default — types in L0 do not derive Serialize per invariant #11.
# Enabled only by L4 wire crates (nika-cli, nika-lsp, nika-serve) at the boundary.
serde = ["dep:serde"]
# Source-conversion features expose `From<serde_*::Error>` for Phase 1
# parsing variants. Off by default — leaf crates pull these in as needed.
yaml-source = ["dep:serde_yaml"]
json-source = ["dep:serde_json"]

[dev-dependencies]
proptest  = { workspace = true }
miette    = { workspace = true, features = ["fancy-no-backtrace"] }

[lints]
workspace = true
```

Workspace `[workspace.dependencies]` additions (Gate 2 commit):

```toml
thiserror  = "2.0"
miette     = "7.6"
proptest   = "1.6"
serde      = "1.0"
serde_yaml = "0.9"
serde_json = "1.0"
```

(Versions to be pinned to the exact patch shipping in Rust 1.91.1 ecosystem
at Gate 2 time; these are minimum acceptable.)

---

## 9. Lints & file-level attributes

```rust
// tools/nika-error/src/lib.rs
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![doc = include_str!("../README.md")]   // optional, see Gate 8
```

The crate inherits all `[workspace.lints]` so `unwrap_used = "deny"`,
`expect_used = "warn"`, `unreachable_pub = "warn"` apply to production
code. Tests get the targeted allows above.

`unsafe_code = "forbid"` (workspace) holds — `nika-error` has zero need
for unsafe.

---

## 10. Out of scope

- L1+ variants (HTTP, MCP, provider, tool, media, artifact, structured
  output, record). They live in their owner crates with their own
  `From<X> for NikaError` impls.
- The legacy `nika-engine::NikaError` (100+ variants) — that enum is the
  HISTORICAL location of what becomes scattered ownership post-diamond,
  and is not migrated wholesale into this crate.
- Backtrace capture — miette `fancy-no-backtrace` feature deliberately
  disables backtraces (controlled at the display layer).
- Custom panic hooks — out of scope for an L0 error library.
- I18N / localization — error messages are EN only per project convention.

---

## 11. Audit trail

| Date | Author | Change |
|---|---|---|
| 2026-04-13 | Phase 1 W1 | Initial spec, Gate 1 commit. |

🦋
