# Crate spec — `nika-a11y`

| | |
|---|---|
| Status | **WIP** (Phase 2 M2.3 · third L1 effect crate · B.1 spec · backend-scope decision OPEN · §4) |
| Layer | L1 — effect implementation · async · `Send + Sync` · depends only on L0 / L0.5 |
| Sub-tier | L1-effect — accessibility-tree query behind the L0.5 `AccessibilityTree` trait. macOS backend via the safe-API `accessibility` crate (encapsulates `AXUIElement` unsafe FFI · so `nika-a11y` stays `unsafe_code = forbid`) |
| Design | macOS-first adapter over **accessibility 0.2.0** (`AXUIElement` · `TreeWalker`/`TreeVisitor` · MIT/Apache · safe Rust API · the unsafe ApplicationServices FFI is encapsulated inside the crate, same posture `nika-screen` got from `xcap` + `nika-ocr` from `ocrs`). Sync `AXUIElement` walk runs inside `tokio::task::spawn_blocking` (the `AXUIElement` handle is `!Send` · stays worker-local · produces the `Send` `AxNode` tree · kernel CANCEL SAFETY contract · same pattern as `nika-screen`'s `xcap::Monitor`). AX tree → `AxNode` mapping → **mandatory AX-secure-field redaction (Guard 3)** → `Vec<AxNode>` |
| LOC budget | ≤1,300 src |
| File cap | ≤1,500 LOC each · Function cap ≤100 lines |
| Crate version | tracks workspace (`0.80.0`) · License `AGPL-3.0-or-later` · Edition 2024 · Publish `false` |
| ADRs | ADR-003 (12-gate admission) · ADR-081 (7 L1 security guards forever · nika-a11y owns **Guard 3 · AX-secure-field redaction · MANDATORY-at-admission**) |
| Error range | **NIKA-1200..1299** (per ADR-081 `nika_codes` matrix · **supersedes** the stale `io/a11y.rs` doc-comment "NIKA-1060..1079" which predates ADR-081 · reconciled here · same pattern as nika-ocr NIKA-1100..1199) |
| Reference | [`accessibility`](https://docs.rs/accessibility/0.2.0) (MIT/Apache · macOS AXUIElement) · `nika-kernel::io::a11y` (L0.5 sealed `AccessibilityTree` trait + `AxNode`/`AxRole`/`AxQuery` DTOs) |

---

## 1. Purpose

`nika-a11y` is the **third computer-use L1 effect crate** (M2.3 · after
`nika-screen` M2.1 + `nika-ocr` M2.2). It implements the L0.5
`nika_kernel::io::a11y::AccessibilityTree` trait — `snapshot()` +
`find(&AxQuery)` + `resolve_ref(&str)` — exposing the active window's
accessibility tree as `AxNode` records (id · role · label · value · bbox ·
children · attributes) so the Olympus cockpit can query UI structure
semantically (find the "Submit" button · read a field's value · locate a
heading) instead of pixel-guessing.

The OS query is delegated to **`accessibility`** (safe Rust API over macOS
`AXUIElement`), so `nika-a11y` itself contains **zero `unsafe`** and honours
`unsafe_code = "forbid"` — the same sovereign / no-hand-written-FFI posture
`nika-screen` got from `xcap` and `nika-ocr` from `ocrs`.

`nika-a11y` reuses the `Rect` DTO from `nika-screen` (M2.1 · `AxNode.bbox`),
so capture → OCR → a11y all share one pixel-coordinate space (single canonical
geometry per `no-legacy-no-back-compat.md` Class 1).

## 2. Public API

```rust
//! `nika-a11y` · accessibility-tree-query L1 effect crate.

/// Accessibility backend (macOS `AXUIElement` via the safe `accessibility`
/// crate). Walks the focused application's tree in `spawn_blocking` (the
/// `AXUIElement` handle is `!Send`) and applies the mandatory secure-field
/// redaction (Guard 3) before exposing any `AxNode`.
#[non_exhaustive]
pub struct AxBackend { /* ref cache: BTreeMap<String, ...> | pid target */ }

impl AxBackend {
    /// Construct a backend bound to the system-wide / focused-app element.
    /// Errors NIKA-1201 if the process lacks macOS Accessibility trust.
    pub fn new() -> Result<Self, A11yError>;
}

impl nika_kernel::io::a11y::AccessibilityTree for AxBackend {
    async fn snapshot(&self) -> io::Result<AxNode>;
    async fn find(&self, query: &AxQuery) -> io::Result<Vec<AxNode>>;
    async fn resolve_ref(&self, ref_id: &str) -> io::Result<Option<AxNode>>;
}

/// Errors · NIKA-1201..12NN · #[non_exhaustive] + code() + is_transient().
#[non_exhaustive]
pub enum A11yError { /* PermissionDenied(1201) .. TaskJoinFailed(12NN) */ }
```

## 3. Layer discipline

- **L1 effect** — implements one L0.5 trait (`AccessibilityTree`). Depends only
  on `nika-kernel` (L0.5) + permissive externals (`accessibility` on macOS ·
  `tokio` rt for `spawn_blocking` · `thiserror`).
- `tokio` layer-legal at L1 (deny.toml wrappers allowlist · add `nika-a11y`) ·
  `spawn_blocking` only (sync `AXUIElement` walk · `!Send` handle worker-local).
- Zero `nika-*` cross-deps beyond `nika-kernel`. No upward imports.

## 4. OS-backend scope — architecture decision (DECISION POINT)

Querying *another app's* accessibility tree is **per-OS** — there is no mature
single cross-platform pure-Rust QUERY crate (`accesskit` is the inverse: it
*publishes* an app's own tree to ATs, not query). Three vetted backends exist,
all permissively licensed (verified crates.io 2026-05-25):

| OS | Crate | Version | License | Notes |
|---|---|---|---|---|
| macOS | `accessibility` | 0.2.0 | MIT / Apache-2.0 | safe API over `AXUIElement` · `TreeWalker`/`TreeVisitor` · unsafe encapsulated |
| Linux | `atspi` | 0.30.0 | Apache-2.0 OR MIT | pure-Rust AT-SPI2 D-Bus client (zbus) |
| Windows | `uiautomation` | 0.25.0 | Apache-2.0 | UIA · COM FFI encapsulated |

**RECOMMENDED · macOS-first (Option A).** Ship the macOS `accessibility`
backend now (the Olympus dev + atelier platform is darwin · computer-use tools
ship macOS-first). Linux/Windows are `#[cfg(target_os = ...)]` skeleton
backends returning `A11yError::BackendUnavailable` (NIKA-12NN) until a consumer
signal lands (LOCK-031 spirit). The cross-platform `AxNode` DTO + the **pure
mandatory redaction guard ship all-OS, headless** (§5). Deps stay macOS-gated
(`[target.'cfg(target_os="macos")'.dependencies] accessibility`).

**Alternative (Option B) · full 3-OS now** — adds `atspi` + `uiautomation`
backends + their per-OS attribute→`AxNode` mapping + tri-platform CI. ~3× the
surface · not needed while the only consumer (Olympus cockpit) is darwin.

> **OPEN** · confirm Option A (macOS-first · recommended) vs Option B (full
> 3-OS) before B.2. The mandatory Guard 3 + DTO mapping + ref-cache are
> backend-agnostic and identical either way.

## 5. Mandatory Guard 3 — AX-secure-field redaction (ADR-081 · MANDATORY-at-admission)

Per ADR-081 §matrix, `nika-a11y` owns **Guard 3 · MANDATORY-at-admission**:
secure-text fields (macOS `AXSecureTextField` subrole · `NSAccessibilityProtectedContent`
· AT-SPI `STATE_SENSITIVE`) MUST have their `AxNode.value` **stripped** before
any node leaves the crate — passwords never reach a caller.

**Design — the guard is a PURE tree-transform** (the security-critical core is
backend-independent + 100 % headless-testable + mutation-killable):

```rust
/// Redact secure-field values across the tree. Pure · headless-testable.
fn redact_secure_fields(node: AxNode) -> AxNode;          // recursive
fn is_secure_field(node: &AxNode) -> bool;                // role/attr predicate
```

`is_secure_field` reads the canonical convention the backend populates while
walking: `attributes["AXSubrole"] == "AXSecureTextField"` (macOS) OR
`attributes["sensitive"] == "true"` (AT-SPI `STATE_SENSITIVE`). When true, the
node's `value` is replaced with `None` (NOT a masked string — zero leak). The
guard is applied to **every** `snapshot`/`find`/`resolve_ref` result before
return. Per ADR-081 per-guard contract: ≥3 unit tests (happy / redacted /
nested-secure-child) + the pure transform is the all-OS mandatory gate.

## 6. Batch plan (skeleton-option-A · per nika-screen / nika-ocr precedent)

- **B.1** spec (this file) · backend research done · decision OPEN (§4).
- **B.2** crate skeleton + `A11yError` NIKA-1201.. + `AxBackend` skeleton
  (snapshot/find/resolve_ref return `BackendNotWired`-style placeholder) +
  the **pure Guard 3 redaction** (`redact_secure_fields` / `is_secure_field`)
  + DTO-mapping pure helpers (`ax_role_from_str`, attribute extraction) +
  headless tests (guard + mapping). Mandatory guard headless-complete at B.2.
- **B.3** wire the macOS `accessibility` backend (`AXUIElement::application` /
  `system_wide` · `TreeWalker` → `AxNode` · subrole→secure flag · frame→`Rect`
  · ref-cache for `resolve_ref`) inside `spawn_blocking` · closes the skeleton.
  Linux/Windows `#[cfg]` `BackendUnavailable`.
- **B.4** mutation (`cargo mutants -p nika-a11y -- --lib`) → ≥90 % on the pure
  surface (guard + mapping + query-filter) + Rule-2 exemption for the
  `AXUIElement` walk residue + ADR-003 canonical 12-gate close + review swarm
  (or Foreman-direct per PE-5.1) + admission commit.

## 7. Gate-5 mutation posture (forward note)

The `AXUIElement` walk is OS-permission-dependent (needs a real focused app +
macOS Accessibility trust) → covered by `#[ignore]` smoke tests, not headless
CI → Rule-2 exempt (same shape as nika-screen OS-FFI + nika-ocr model-inference
residue). All headless-reachable logic — **the mandatory Guard 3 redaction**,
the `AxRole` string mapping, the `AxQuery` filter (role / label_contains /
value_contains / max_depth), error `code()`/`is_transient()` — targets 100 %
mutation kill.

## 8. Security (ADR-081)

`nika-a11y` owns **Guard 3 (AX-secure-field redaction) · MANDATORY-at-admission**.
The redaction is a pure, always-on tree-transform applied before any node
exposure (no opt-out · zero password leak by construction). Telemetry-canon §0:
zero cloud · no guard-state egress. Sovereignty Rule 1: no vendor-hosted state.
The remaining 6 ADR-081 guards belong to other L1 crates (input / browser /
vision-local / screen) per the §matrix.
