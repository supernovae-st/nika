# Crate spec — `nika-input`

| | |
|---|---|
| Status | **PROPOSED** (Phase 2 M2.4 · fourth L1 effect crate · write-side · ADR-003 12 gates · **TWO** ADR-081 guards MANDATORY-at-admission · Guard 1 password-typing redaction + Guard 2 ConsentProof-TTL type-enforcement) |
| Layer | L1 — effect implementation · async · `Send + Sync` · depends only on L0 / L0.5 |
| Sub-tier | L1-effect — synthetic pointer + keyboard event dispatch behind the L0.5 `InputDevice<S: ConsentState>` trait. **Cross-platform** backend via `enigo` (macOS + Linux + Windows · encapsulates the per-OS unsafe FFI · so `nika-input` stays `unsafe_code = forbid` · same encapsulation posture `nika-a11y` got from `accessibility`) |
| Design | **Cross-platform** adapter over **`enigo`** (macOS + Linux + Windows day-1 · MIT · per ADR-083 cross-platform doctrine · exact API surface **verified at Gate 3 / B.3** via `.crate` tarball extraction, same rigor `nika-a11y` applied to `accessibility`). Sync `enigo` calls run inside `tokio::task::spawn_blocking` (the OS event APIs are synchronous · worker-local · kernel CANCEL SAFETY contract · same pattern as `nika-a11y`'s walk). The crate is **generic over the type-state** `S: ConsentState` — mutating dispatch is only callable on `InputDevice<Authorized>` (`S::Granted = ConsentProof`); `Unconfirmed` makes the proof `Infallible` → un-callable at compile time (Guard 2 is partly the type system, partly the runtime `ttl_ns` re-check). |
| LOC budget | ≤1,300 src |
| File cap | ≤1,500 LOC each · Function cap ≤100 lines |
| Crate version | tracks workspace (`0.80.0`) · License `AGPL-3.0-or-later` · Edition 2024 · Publish `false` |
| ADRs | ADR-003 (12-gate admission) · ADR-081 (7 L1 security guards forever · nika-input owns **Guard 1 · password-typing redaction** + **Guard 2 · ConsentProof-TTL type-enforcement** · both MANDATORY-at-admission) · **ADR-083 (L1 computer-use cross-platform doctrine · backend = `enigo`)** |
| Error range | **NIKA-1300..1399** (per ADR-081 `nika_codes` matrix line 20 · **supersedes** the stale `io/input.rs` doc-comment "NIKA-1080..1099" which predates ADR-081 · reconciled here · same pattern as nika-a11y NIKA-1200..1299 superseding the stale 1060-1079) |
| Reference | [`enigo`](https://docs.rs/enigo) (MIT · cross-platform synthetic input · macOS/Linux/Windows) · `nika-kernel::io::input` (L0.5 sealed `InputDevice<S: ConsentState>` trait + `ConsentProof`/`Point`/`MouseButton`/`KeyMods`/`KeyCode`(66) DTOs) |

---

## 1. Purpose

`nika-input` is the **fourth computer-use L1 effect crate** (M2.4 · after
`nika-screen` M2.1 + `nika-ocr` M2.2 + `nika-a11y` M2.3) and the **first
write-side one** — it does not read the screen, it *acts on* it. It implements
the L0.5 `nika_kernel::io::input::InputDevice<S: ConsentState>` trait —
`move_cursor(Point)` + `click(MouseButton)` + `type_text(&str)` +
`key_press(KeyCode, KeyMods)` — so the Olympus cockpit can drive the desktop
(move the cursor to a button `nika-a11y` located, click it, type into a field)
to close the see → understand → **act** computer-use loop.

The OS event synthesis is delegated to **`enigo`** (cross-platform · macOS +
Linux + Windows · per ADR-083 cross-platform doctrine), so `nika-input` itself
contains **zero `unsafe`** (enigo encapsulates the per-OS FFI) and honours
`unsafe_code = "forbid"` — the same sovereign / no-hand-written-FFI posture
`nika-screen` got from `xcap`, `nika-ocr` from `ocrs`, `nika-a11y` from
`accessibility`.

`nika-input` reuses the `Point` DTO from `io::input` (canonically the
top-left-origin pixel sibling of `nika-screen`'s `Rect`), so capture → OCR →
a11y → **input** all share one pixel-coordinate space (single canonical geometry
per `no-legacy-no-back-compat.md` Class 1). Per-OS coordinate conventions (e.g.
the macOS bottom-left flip) are handled inside `enigo` at the syscall boundary
(the `io::input::Point` doc-comment describes the top-left-origin canon).

**Why this crate carries the highest review scrutiny in M2:** it is write-side
(can type passwords, click destructive buttons) AND it is the home of the
type-state consent contract. Both ADR-081 guards it owns are MANDATORY and
block Gate 2.

## 2. Public API

```rust
//! `nika-input` · synthetic pointer + keyboard L1 effect crate (write-side).

/// Synthetic-input backend (cross-platform via `enigo`). ZERO-SIZE: the `Enigo`
/// handle is constructed PER CALL inside `spawn_blocking` (it holds `!Send` OS
/// resources and takes `&mut self` — worker-local, never stored). Generic over
/// the consent type-state — only `EnigoInputDevice<Authorized>` exposes the
/// mutating methods. Deliberately NOT `Default` (an `Authorized` device must
/// only come from `request_consent` — review-swarm finding, Guard-2 landmine).
#[derive(Debug, Clone, Copy)]   // no Default
#[non_exhaustive]
pub struct EnigoInputDevice<S: ConsentState> { /* PhantomData<S> */ }

impl EnigoInputDevice<Unconfirmed> {
    /// Construct an un-consented device. Mutating methods are un-callable
    /// (`S::Granted = Infallible`) until consent is acquired. Hermetic (no OS API).
    pub fn new() -> Result<Self, InputError>;

    /// Mint the FLOW token (TTL stamped on the process-MONOTONIC clock) and
    /// transition to `Authorized`. Touches no OS API (hermetic — tests never
    /// prompt). The real capability barrier is the OS grant (macOS
    /// Accessibility+Input-Monitoring · Linux uinput/display session · Windows
    /// UIAccess), verified SILENTLY at every dispatch (never pops a dialog
    /// from inside an agent loop) → NIKA-1301 ConsentDenied there.
    pub fn request_consent(self, ttl_ns: u64) -> Result<(EnigoInputDevice<Authorized>, ConsentProof), InputError>;
}

// The SEND variant is the impl target (kernel one-way blanket impl derives the
// local `InputDevice` from it — never the reverse). Pattern A (FCI-023bis):
// every method returns the kernel's typed `InputError`, NEVER io::Result.
impl nika_kernel::io::input::InputDeviceDyn<Authorized> for EnigoInputDevice<Authorized> {
    async fn move_cursor(&self, to: Point, proof: &ConsentProof) -> Result<(), InputError>;
    async fn click(&self, button: MouseButton, proof: &ConsentProof) -> Result<(), InputError>;
    async fn type_text(&self, text: &str, proof: &ConsentProof) -> Result<(), InputError>;
    async fn key_press(&self, key: KeyCode, modifiers: KeyMods, proof: &ConsentProof) -> Result<(), InputError>;
}

// Errors: the kernel-owned `nika_kernel::io::input::InputError` (Pattern A ·
// FCI-023bis · NO crate-local enum, NO thiserror dep — the L1 re-exports it as
// `nika_input::Error`). Codes are kernel-owned constants in nika_error::codes:
// ConsentDenied(1301) · ConsentExpired(1302) · EventPostFailed(1303) ·
// BackendUnavailable(1304) · TaskJoinFailed(1305). NIKA-1300 was never minted
// (no placeholder phase — the kernel enum landed typed at M1).
```

The `InputError` enum **never carries raw typed text** (Guard 1 · §5). Its
`Debug`/`Display` and any future journal hook use the redacted char-count form.

## 3. Layer discipline

- **L1 effect** — implements one L0.5 trait (`InputDeviceDyn<S>` · the Send
  variant · local trait via the kernel blanket impl). Depends only on
  `nika-kernel` (L0.5) + permissive externals (`enigo` cross-platform · `tokio`
  rt for `spawn_blocking`). NO `thiserror` (Pattern A / FCI-023bis: the error
  enum is kernel-owned · re-exported).
- `tokio` layer-legal at L1 (deny.toml wrappers allowlist · add `nika-input`) ·
  `spawn_blocking` only (sync `enigo` calls · worker-local).
- Zero `nika-*` cross-deps beyond `nika-kernel`. No upward imports.
- `enigo` is a **normal (non-target-gated) dep** — it resolves the per-OS backend
  internally (macOS / Linux / Windows), so no `[target.'cfg(...)']` split is
  needed (contrast `nika-a11y`, which gates per-OS because no cross-platform
  query crate exists · per ADR-083).

## 4. OS-backend scope — RESOLVED · cross-platform via `enigo` (ADR-083)

Synthetic input is per-OS at the syscall (macOS `CGEvent` · Windows `SendInput`
· Linux `uinput`/wayland) — but a **mature cross-platform Rust crate exists**:

| Path | Crate(s) | License | Notes |
|---|---|---|---|
| **✅ `enigo`** (chosen) | `enigo` 0.x | MIT | one dep → macOS + Linux + Windows day-1 · encapsulates the per-OS unsafe FFI (keeps `unsafe_code = forbid`) · sync calls wrapped in `spawn_blocking` |
| macOS-only CGEvent (rejected) | servo `core-graphics` | MIT/Apache | would re-introduce the macOS-only inconsistency ADR-083 removes |

**RESOLVED 2026-05-26 · cross-platform via `enigo` · per ADR-083** (L1
computer-use cross-platform doctrine · macOS + Linux prio · Windows + all). The
backend-selection rule: prefer one mature cross-platform crate behind the L0.5
trait — and `enigo` covers macOS + Linux + Windows. The macOS-only CGEvent path
was the M2.4 first-draft recommendation; it is **rejected** because it would
re-create the very macOS-only inconsistency ADR-083 exists to remove (cf
`nika-screen`/`nika-ocr` already cross-platform · `nika-a11y` macOS-only only
because no cross-platform query crate exists).

`enigo` does **not** weaken the guards · they are pure + at *our* layer (§5/§5b)
· `enigo` only performs the final OS event post. Exact `enigo` API surface
(`Enigo::new` · `Keyboard`/`Mouse` traits · `key`/`button`/`move_mouse`/`text`)
is **primary-source verified at Gate 3 / B.3** via `.crate` tarball extraction
(the rigor that confirmed `accessibility` for `nika-a11y` · no phantom symbols
before verification) — incl. the Linux X11-vs-Wayland + permissions story.

## 5. Mandatory Guard 1 — password-typing redaction (ADR-081 · MANDATORY-at-admission)

Per ADR-081 §matrix, `nika-input` owns **Guard 1 · MANDATORY-at-admission**:
the text a `type_text` call synthesizes **MUST NEVER reach any journal / log /
error / Debug representation** — a synthetic-input crate that logs what it types
leaks passwords by construction.

**Design — the guard is a PURE emission-boundary transform** (security-critical
core is backend-independent + 100 % headless-testable + mutation-killable):

```rust
/// The ONLY representation typed text may take in any log/journal/error.
/// Returns e.g. "<redacted · 11 code points>" — never the content. Pure.
fn redact_typed_text(text: &str) -> String;
```

`type_text` passes the raw `&str` straight to the `enigo` call and **never
stores or formats it**; every observability path (`InputError`, any future
`skill.invoked`/journal hook) uses `redact_typed_text`. Per ADR-081 per-guard
contract: ≥3 unit tests (empty / multi-byte UTF-8 / never-leaks-content) + the
pure transform is the all-OS mandatory gate. Invariant under proptest: for any
input string, the redacted form contains **none** of the input's non-whitespace
content substrings.

## 5b. Mandatory Guard 2 — ConsentProof-TTL type-enforcement (ADR-081 · MANDATORY-at-admission)

Per ADR-081 §matrix, `nika-input` owns **Guard 2 · MANDATORY-at-admission**:
no synthetic event dispatches without a **non-expired** `ConsentProof`. The
enforcement is two-layered:

1. **Compile-time (the type system).** Mutating methods bound
   `S: ConsentState<Granted = ConsentProof>`. With `S = Unconfirmed`,
   `Granted = Infallible` → no caller can supply the proof argument → the
   method is un-callable. Consent MUST be acquired (`request_consent()` →
   `EnigoInputDevice<Authorized>`) first. This is the kernel trait's design;
   nika-input is the impl that honours it (does **not** add a bypass).
2. **Runtime (the `ttl_ns` re-check).** Every dispatch re-checks
   `now - proof.granted_at_ns <= proof.ttl_ns` (`ttl_ns == 0` = infinite).
   Stale proof returns the typed `InputError::
   ConsentExpired` (NIKA-1302). This is a **pure** predicate:

```rust
/// True if the proof is still valid at `now_ns`. Pure · headless-testable.
fn consent_valid(proof: &ConsentProof, now_ns: u64) -> bool; // ttl_ns==0 ⇒ always
```

Per ADR-081 per-guard contract: ≥3 unit tests (valid / expired / infinite-ttl)
+ the `now_ns` is injected (kernel `io::clock` style) so the predicate is
deterministic and headless. Time is **not** read inside the pure core — the L1
dispatch path reads the clock and passes `now_ns` in (test hermeticity ·
Invariant #27).

## 6. Batch plan (skeleton-option-A · per nika-screen / nika-ocr / nika-a11y precedent)

- **B.1** spec (this file) · backend research done · Option A recommended,
  decision OPEN (§4) pending confirm.
- **B.2** ✅ SHIPPED `95fcf4002` — crate skeleton + the kernel-owned typed
  `InputError` (NIKA-1301..1305 · Pattern A · no placeholder code was ever
  minted) + `EnigoInputDevice<S>` type-state skeleton + the **two pure guards**
  (`redact_typed_text` / `consent_valid`) + headless tests. Both mandatory
  guards headless-complete at B.2 (the security cores ship before any FFI).
- **B.3** `enigo` cross-platform backend wired (API primary-source verified
  on the 0.6.1 tarball) · `move_cursor`/`click`/`type_text`/`key_press` inside
  `spawn_blocking` (per-call handle · worker-local) · `KeyCode`(66) →
  `enigo::Key` map (fail-closed on unknown variants) · `KeyMods` → press/tap/
  release chord · no per-OS `#[cfg]` split (enigo resolves macOS/Linux/Windows
  internally) · 3-lens review-swarm findings folded in (InputDeviceDyn Send
  variant · no Default derive · monotonic fail-closed consent clock ·
  `TypedText` un-formattable wrapper · static text-path error reasons).
- **B.4** mutation (`cargo mutants -p nika-input -- --lib`) → ≥90 % of the
  headless surface (both pure guards + keycode map + error surface) + Rule-2
  exemption for the enigo-post FFI residue (§7.1) + ADR-003 canonical 12-gate
  close (§7) + 3-lens review (Foreman-direct if the 1M-context wall hits per
  PE-5.1) + admission commit.

## 7. Gate status — ADR-003 canonical 12 gates

| # | Gate | Status | Evidence |
|---|------|--------|----------|
| 1 | SPEC | ✅ | this file |
| 2 | TDD | ⬜ | tests precede impl (both guards headless first) |
| 3 | IMPL | ⬜ | `enigo` cross-platform backend · API verified before write |
| 4 | CLIPPY | ⬜ | `clippy --workspace --all-targets -D warnings` 0 |
| 5 | MUTATION | ⬜ | `cargo mutants -p nika-input -- --lib` ≥90 % headless + Rule-2 FFI exemption (§7.1) |
| 6 | PROPERTY | ⬜ | proptest · Guard 1 no-content-leak invariant + Guard 2 ttl predicate |
| 7 | BENCHMARKS | ⚪ N/A | thin `enigo` adapter · post latency is OS-bound, not a Nika hot path (Rule 2) |
| 8 | DOCS | ⬜ | `cargo doc --no-deps` 0 warnings · all pub items documented |
| 9 | CANARY E2E | ⚪ N/A | L1 effect crate · no `.nika.yaml` surface · a `#[ignore]` real-dispatch smoke needs consent grant |
| 10 | PARITY | ⚪ N/A | NEW computer-use crate (M2.4) · no v0.79 brouillon synthetic-input equivalent |
| 11 | REVIEW SWARM | ⬜ | 3-lens (rust-pro + Diamond + bug-hunt) · highest-scrutiny write-side crate · Foreman-direct fallback per PE-5.1 |
| 12 | ATOMIC COMMIT | ⬜ | the admission commit |

### 7.1 Gate 5 mutation exemption (ADR-003 Rule 2 · `enigo` post — anticipated)

`nika-input` is a thin adapter over the synchronous `enigo` calls. The mutants
expected to be **exempt** live on the `enigo`-post control-flow, reachable only
with a real consent grant + a live desktop session (exercised by an `#[ignore]`
smoke test, not headless CI) — e.g. the `spawn_blocking` `enigo` call + the
`KeyCode`→`enigo::Key` / `KeyMods`→modifier application at the dispatch site.
All **headless-reachable** logic targets 100 % mutation kill — both MANDATORY
guards (`redact_typed_text` + `consent_valid`), the `KeyCode`→`enigo::Key` map,
and the full `InputError` surface. Per ADR-003 Rule 2 the `enigo`-post residue
is documented-exempt, not skipped. Exact exempt-mutant list lands at B.4.

## 8. Security (ADR-081)

`nika-input` owns **TWO** MANDATORY-at-admission guards: **Guard 1
(password-typing redaction)** — typed text never reaches any observability path
(pure `redact_typed_text` · no opt-out) — and **Guard 2 (ConsentProof-TTL
type-enforcement)** — no dispatch without a non-expired proof (compile-time
type-state + runtime `consent_valid` re-check). Telemetry-canon §0: zero cloud ·
no guard-state egress. Sovereignty Rule 1: no vendor-hosted state · consent is
in-memory at the engine layer (persistence is daemon-domain · `~/.olympus/cache/
consent/` when olympus-os ships at S33+ per ADR-081 §90). This is the write-side
crate — both guards are LOAD-BEARING for atelier integrity and block Gate 2.
