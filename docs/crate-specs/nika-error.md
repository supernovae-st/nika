# Crate spec — `nika-error`

| | |
|---|---|
| Status | **ADMITTED 2026-04-13** (`42909b1c7`) · first sub-crate of the `nika-core` split |
| Layer | L0 (PURE, zero I/O, zero async) |
| Design | **Option C+** (trait-based error hierarchy) |
| LOC budget | ≤800 src (target ~700, alarm at 800) |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Source on `main` (reference) | `tools/nika-core/src/error.rs` (40 LOC), `tools/nika-core/src/error_codes.rs` (293 LOC) |
| Crate version | tracks workspace (bumped to `0.90.0-alpha.1` at Phase 1 close) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |

---

## 1. Purpose

`nika-error` defines the **error infrastructure** for the Nika diamond:

- **`NikaErrorCode`** trait — contract all per-crate error enums implement
- **`NikaError`** wrapper — `Box<dyn NikaErrorCode>`, the unified error type
- **`CoreError`** enum — cross-cutting errors (Validation, NotFound, Unsupported, Internal)
- **`NikaCode`** struct — dual wire ("NIKA-140") + typed (num, category, severity, slug)
- **`NikaResult<T>`** alias — `Result<T, NikaError>`

This is the L0 anchor: zero `nika-*` dependencies, reachable from every crate.
Sole owner of the `NIKA-XXX` error-code namespace.

Resolves shadow zone **GATE 6** (`PRE_LAUNCH_GATES.md`): every admitted
`NIKA-XXX` ships with a Display parity golden test.

Decision D2 (error type = C+ = NikaError holds a boxed CoreError + context).
Rationale: preserve diagnostic info through layers without stack explosion.

---

## 2. Public API surface

```rust
// ── Trait ──────────────────────────────────────────────────────
pub trait NikaErrorCode:
    std::error::Error + miette::Diagnostic + Send + Sync + 'static
{
    fn nika_code(&self) -> NikaCode;         // structured NIKA-XXX code
    fn is_transient(&self) -> bool { false }  // retry policy hint
    fn fingerprint(&self) -> u64 { .. }       // deduplication hash (default: hash of num)
}

// ── Wrapper ───────────────────────────────────────────────────
pub struct NikaError(Box<dyn NikaErrorCode>);
pub type NikaResult<T> = Result<T, NikaError>;

impl NikaError {
    pub fn new<E: NikaErrorCode>(e: E) -> Self;
    pub fn nika_code(&self) -> NikaCode;
    pub fn is_transient(&self) -> bool;
    pub fn fingerprint(&self) -> u64;
    pub fn downcast_ref<E: NikaErrorCode>(&self) -> Option<&E>;
}

impl<E: NikaErrorCode> From<E> for NikaError;  // blanket conversion
impl Display for NikaError;                      // delegates to inner
impl Debug for NikaError;                        // delegates to inner
impl Error for NikaError;                        // delegates source()

// ── Core errors ───────────────────────────────────────────────
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[non_exhaustive]
pub enum CoreError {
    Validation { reason: String },
    NotFound { what: String },
    Unsupported { feature: String },
    Internal { context: String, detail: String },
}
impl NikaErrorCode for CoreError;

// ── Code registry ─────────────────────────────────────────────
pub struct NikaCode { pub num: u16, pub category: Category, pub severity: Severity, pub slug: &'static str }
#[non_exhaustive] pub enum Category { Core, Shell, FileIo, Http, Auth, Mcp, Schema, Binding, Provider, Verb, Runtime }
pub enum Severity { Error, Warning }

// Phase 1 codes
pub const NIKA_001: NikaCode;  // Core / validation-failed
pub const NIKA_002: NikaCode;  // Core / not-found
pub const NIKA_003: NikaCode;  // Core / unsupported
pub const NIKA_999: NikaCode;  // Core / internal

pub const ALL: &[NikaCode];
pub fn code_help(code: NikaCode) -> &'static str;
```

---

## 3. Per-crate error pattern (convention for downstream)

Every crate defines its own `#[non_exhaustive]` error enum and implements
`NikaErrorCode`. The blanket `From<E> for NikaError` gives free conversion:

```rust
// Example: in nika-http (NOT in nika-error)
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[non_exhaustive]
pub enum HttpError {
    #[error("SSRF blocked: {url}")]
    SsrfBlocked { url: String },
}

impl NikaErrorCode for HttpError {
    fn nika_code(&self) -> NikaCode {
        match self { Self::SsrfBlocked { .. } => codes::NIKA_140 }
    }
}
// From<HttpError> for NikaError = blanket, free
```

---

## 4. Numeric code ranges (convention, xtask-enforced)

```
001-049   Core (validation, notfound, internal)
050-099   Shell/exec
100-139   File/IO
140-189   Http/network
190-229   Auth/vault
230-279   MCP/tools
280-329   Schema/parse
330-379   Provider (moved from 380-429 on 2026-05-11)
380-429   Shield (RESERVED · crate not yet admitted — no code may allocate here)
430-479   Verb-specific
480-529   Runtime/dispatch
600-649   Memory
700-749   WasmPlugin
750-799   Sandbox
800-819   Observability
1000-1099 Screen · 1100-1199 Ocr · 1200-1299 A11y · 1300-1399 Input · 1400-1499 Browser (ADR-081)
1800-1849 Access (execution access · D-2026-08-04-N1)
```

`Binding` is a reserved category with no allocated range (its original
330-379 slot was reassigned to Provider on 2026-05-11). The authoritative
allocation is the `Category` doc comment in `crates/nika-error/src/codes.rs`
— this table mirrors it; on any drift, the registry wins.

Phase 1 admits only codes 001-003, 999. Other codes ship with their owner crates.

---

## 5. File layout

```
crates/nika-error/
  Cargo.toml
  src/
    lib.rs          (~50 LOC — pub mod + re-exports)
    traits.rs       (~60 LOC — NikaErrorCode trait + AsAny helper)
    codes.rs        (~150 LOC — NikaCode struct + Category + Severity + consts + ALL + serde)
    core_error.rs   (~100 LOC — CoreError enum + NikaErrorCode impl)
    nika_error.rs   (~120 LOC — NikaError wrapper + From blanket + Display/Debug/Error)
```

Target: ~500 LOC src, ~400 LOC tests = ~900 LOC total.

---

## 6. Dependencies

```toml
[dependencies]
thiserror = { workspace = true }
miette    = { workspace = true, features = ["fancy-no-backtrace"] }
serde     = { workspace = true, optional = true }

[features]
default = ["serde"]
serde = ["dep:serde", "miette/serde"]

[dev-dependencies]
insta      = { workspace = true }
proptest   = { workspace = true }
rstest     = { workspace = true }
serde_json = { workspace = true }
```

---

## 7. Test plan

| Test location | Scope | Count |
|---|---|---|
| `src/codes.rs` #[cfg(test)] | Category, Severity, Display format, serde roundtrip | ~6 |
| `src/core_error.rs` #[cfg(test)] | Display parity, NikaErrorCode impl, is_transient | ~6 |
| `src/nika_error.rs` #[cfg(test)] | From blanket, downcast_ref, Display delegation | ~6 |
| `src/traits.rs` #[cfg(test)] | fingerprint default, is_transient default | ~3 |
| proptest in codes.rs | code uniqueness, Display format "NIKA-XXX" | ~2 |

All unit tests inline (`#[cfg(test)] mod tests`), run with `cargo test --lib`.

---

## 8. Gate exemptions

- **Gate 7 (Benchmarks)**: Exempt. L0 error types, no hot path.
- **Gate 9 (Canary E2E)**: Exempt. No runtime yet.

---

## 9. Actual metrics (post-admission)

| Metric | Value |
|---|---|
| Total LOC | 1,013 |
| Tests | 44 |
| Mutation score | 100% |
| Clippy warnings | 0 |
| Doc warnings | 0 |
| Commit | `42909b1c7` |

## 10. Audit trail

| Date | Author | Change |
|---|---|---|
| 2026-04-13 | Phase 1 W1 | Initial spec (option B fat-enum). |
| 2026-04-13 | Phase 1 W1 | Rewritten for option C+ (trait-based hierarchy). |
| 2026-04-13 | Phase 1 W1 | Admitted to workspace. All 12 gates passed. |

🦋
