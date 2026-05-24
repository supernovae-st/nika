# Crate spec — `nika-screen`

| | |
|---|---|
| Status | **ADMITTED** (Phase 2 M2.1 · first L1 effect crate · 12-gate closed 2026-05-24) |
| Layer | L1 — effect implementation · async · `Send + Sync` · depends only on L0 / L0.5 |
| Sub-tier | L1-effect — OS screen capture behind the L0.5 `ScreenCapture` trait. OS FFI is encapsulated by the `xcap` dependency, so the crate itself is `unsafe_code = forbid`-clean |
| Design | Thin adapter over **xcap 0.9.5** (cross-platform screen capture · macOS CoreGraphics · Linux X11/Wayland portal · Windows DXGI). Sync `xcap` calls run inside `tokio::task::spawn_blocking` so the `!Send` `xcap::Monitor` stays worker-local and dropped futures surrender promptly (kernel CANCEL SAFETY contract) |
| LOC budget | ≤1,200 src (actual ~943 · error 205 + guards 240 + capture 446 + lib 52) |
| File cap | ≤1,500 LOC each (max `capture.rs` 446) |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace (`0.80.0`) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L1 effect crate (not a standalone crates.io satellite) |
| ADRs | ADR-003 (12-gate admission) · ADR-081 (7 L1 security guards forever · this crate owns guards 6 + 7) |
| Error range | **NIKA-1000..1009** (10 codes · `ScreenError::code()` grep-anchor) |
| Reference | [`xcap`](https://docs.rs/xcap/0.9.5) (Apache-2.0) · `nika-kernel::io::screen` (L0.5 sealed trait + DTOs) |

---

## 1. Purpose

`nika-screen` is **the first computer-use L1 effect crate** of the Diamond
engine. It implements the L0.5 `nika_kernel::io::screen::ScreenCapture` trait —
display enumeration plus full-display, sub-region, and continuous-stream
capture — on macOS, Linux, and Windows.

The OS FFI is delegated entirely to `xcap` (which wraps `objc2-*` on macOS,
`x11`/`zbus` on Linux, `windows` on Windows), so `nika-screen` itself contains
**zero `unsafe`** and honours the workspace `unsafe_code = "forbid"` lint. This
is the ADR-081 "OS-native when available" path; an unsafe-allowing FFI layer (or
the Olympus cockpit's own bindings) may later add a native recording-LED /
consent-dialog enhancement, but is out of scope here.

The crate is the **template for the next 5 L1 effect crates** (`nika-ocr` ·
`nika-a11y` · `nika-input` · `nika-browser` · `nika-vision-local`) per ADR-081's
guard-ownership matrix.

---

## 2. Public API

```rust
//! `nika-screen` · screen-capture L1 effect crate.

/// Cross-platform screen-capture backend (driven by `xcap`). Composes the
/// ADR-081 capture guards (fail-closed consent gate + shared LED indicator).
#[non_exhaustive]
pub struct ScreenBackend { /* consent: Arc<ConsentGate>, led: Arc<LedIndicator> */ }

impl ScreenBackend {
    pub fn new() -> Self;                 // consent fail-closed · LED off
    pub fn grant_consent(&self);          // guard 7 · session-scoped grant
    pub fn revoke_consent(&self);         // guard 7 · revoke (stream stops next frame)
    pub fn led_is_engaged(&self) -> bool; // guard 6 · UI renders this
}

impl nika_kernel::io::screen::ScreenCapture for ScreenBackend {
    async fn list_displays(&self) -> io::Result<Vec<DisplayInfo>>;
    async fn capture_full(&self, display: DisplayId) -> io::Result<Frame>;
    async fn capture_region(&self, display: DisplayId, region: Rect) -> io::Result<Frame>;
    async fn capture_stream(&self, display: DisplayId) -> io::Result<FrameStream>;
}

/// Errors · NIKA-1000..1009 · #[non_exhaustive] + code() + is_transient().
#[non_exhaustive]
pub enum ScreenError { /* BackendNotWired(1000) .. BackendInit(1009) */ }

/// ADR-081 guard 7 — in-memory · fail-closed · revocable consent state machine.
#[non_exhaustive] pub enum Consent { Unknown, Granted, Denied, Revoked }
#[non_exhaustive] pub struct ConsentGate { /* AtomicU8 */ }

/// ADR-081 guard 6 — engaged-count capture-LED indicator + RAII scope.
#[non_exhaustive] pub struct LedIndicator { /* AtomicUsize */ }
#[non_exhaustive] pub struct LedScope;  // disengages on drop
```

---

## 3. Layer discipline

- **L1 effect** — implements one L0.5 trait (`ScreenCapture`). Depends only on
  `nika-kernel` (L0.5) + permissive externals (`xcap`, `bytes`, `futures-core`,
  `tokio` rt+sync, `thiserror`, `miette`).
- `tokio` is layer-legal at L1 (deny.toml lists `nika-screen` in the tokio
  wrappers allowlist) — used for `spawn_blocking` (single-shot) + the bounded
  `mpsc` capture-stream worker.
- Zero `nika-*` cross-deps beyond `nika-kernel`. No upward imports.

## 4. ADR-081 security guards (MANDATORY-at-admission)

- **Guard 7 · consent UX** — `ConsentGate` fail-closed (`Unknown` denies),
  session-scoped, revocable. `check()` gates every pixel capture BEFORE any OS
  call (`NIKA-1006` denied · `NIKA-1007` revoked). In-memory only — persistence
  is DEFERRED to M2.4 `nika-input` (`~/.olympus/cache/consent/` when the daemon
  ships). The stream worker re-checks consent per frame; a mid-stream revoke
  surfaces `ConsentRevoked` then tears the stream down.
- **Guard 6 · capture-LED indicator** — `LedIndicator` engaged-count; `engage()`
  returns a RAII `LedScope` that disengages on drop (a panicking or
  early-returning capture path can never leave the indicator stuck on). The
  stream worker holds the scope for the stream's whole lifetime.

## 5. Gate status (12/12 green · ADR-003)

`registry` · `ADR-081 linked` · `schema N/A (trait-impl)` · `#[non_exhaustive]`
(6 public types) · `zero unwrap/expect in src` (clippy `unwrap_used = deny`) ·
`LOC 943` · `NIKA-1000..1009` · `CANCEL SAFETY docs` · `test --workspace --lib`
GREEN · `clippy --workspace --all-targets -D warnings` 0 · `cargo deny` ok ·
`forward-compat` (`#[non_exhaustive]` + `#[trait_variant::make]` + additive).

## 6. Deferred / carry-forward

- **GAP-3 `From<ScreenPoint>` shim** — carried forward to M2.4 `nika-input`.
  `ScreenPoint` is a `cockpit_overlay` (Olympus) type, so a `From` impl in
  `nika-screen` would be a Nika→Olympus dependency (cross-flow D-2026-05-08-N1
  violation), and the conversion is an `io::input` (cursor) concern, not
  `io::screen`. It lands on the Olympus consumer side (`cockpit-input-injection`
  already mirrors it).
- **OS-native recording-LED / consent-dialog FFI** — deferred to a future
  unsafe-allowing layer (ADR-081 "OS-native when available"). The pure-Rust
  state machines are the "explicit indicator / explicit error gate" fallback.
- **Real-capture smoke tests** — 2 `#[ignore]` tests need a Screen-Recording
  TCC grant; run `cargo test -p nika-screen -- --ignored` locally.
