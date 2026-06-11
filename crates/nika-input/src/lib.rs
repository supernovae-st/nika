// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-input` — synthetic pointer + keyboard L1 effect crate (write-side).
//!
//! Implements the L0.5 [`nika_kernel::io::input::InputDeviceDyn<S>`] trait —
//! the `Send` variant; the local `InputDevice<S>` arrives via the kernel's
//! one-way blanket impl (`Dyn` ⇒ local, never the reverse) so generic L2/L3
//! orchestrators can `tokio::spawn` the returned futures. Four verbs
//! (`move_cursor` · `click` · `type_text` · `key_press`) let the Olympus
//! cockpit drive the desktop — closing the see → understand → **act**
//! computer-use loop after `nika-screen` (capture) · `nika-ocr` (read) ·
//! `nika-a11y` (locate). This is the **first write-side** computer-use crate,
//! and carries the highest review scrutiny in M2.
//!
//! # Two MANDATORY ADR-081 guards (this crate owns both)
//!
//! **Guard 1 · password-typing redaction** ([`redact_typed_text`]) — the text a
//! `type_text` call synthesizes MUST NEVER reach any journal / log / error /
//! `Debug`. A synthetic-input crate that logs what it types leaks passwords by
//! construction. Two layers: `type_text` moves the raw text into the
//! `TypedText` wrapper (NO `Debug`/`Display` — interpolating it anywhere fails
//! to COMPILE) until the single backend call site, and every observability
//! path uses [`redact_typed_text`] (e.g. `"<redacted · 11 code points>"`).
//! `InputError` never carries raw typed text (text-path enigo errors are
//! sanitized to static reasons).
//!
//! **Guard 2 · `ConsentProof`-TTL enforcement** ([`consent_valid`]) — no
//! synthetic event dispatches without a NON-EXPIRED [`ConsentProof`].
//! Two-layered:
//! 1. **Compile-time** — the mutating methods bound `S: ConsentState<Granted =
//!    ConsentProof>`. With `S = Unconfirmed` the proof type is `Infallible` →
//!    the method is un-callable. Consent MUST be acquired
//!    ([`EnigoInputDevice::request_consent`] → `EnigoInputDevice<Authorized>`)
//!    first. This is the kernel trait's design; this crate honours it (adds no
//!    bypass).
//! 2. **Runtime** — every dispatch re-checks [`consent_valid`] (`now -
//!    granted_at_ns <= ttl_ns`, `ttl_ns == 0` = infinite). A stale proof returns
//!    [`InputError::ConsentExpired`]. Timestamps come from a process-local
//!    MONOTONIC anchor (`now_ns` · `std::time::Instant`), never the wall
//!    clock — rolling the system clock back cannot extend consent, and a
//!    finite-TTL proof whose grant reads as in-the-future (cross-process
//!    replay · anchor reset) fails CLOSED. The clock is read at the L1
//!    dispatch site and passed into the PURE predicate (test hermeticity ·
//!    Invariant #27 · the pure core never reads a clock).
//!
//! # Backend (B.3 · `enigo` cross-platform · per-call worker-local)
//!
//! The OS event synthesis is delegated to [`enigo`] (macOS `CGEvent` · Windows
//! `SendInput` · Linux X11/Wayland) — it encapsulates the per-OS unsafe FFI, so
//! this crate stays `unsafe_code = forbid`. Three structural choices:
//!
//! - **Per-call construction inside `spawn_blocking`** — the `Enigo` handle holds
//!   `!Send` OS resources (macOS `CGEventSource`/`CGDisplay`) and its methods take
//!   `&mut self`, so each dispatch constructs + uses + drops the handle entirely
//!   on one blocking worker (the `nika-a11y` worker-local pattern). Each call is
//!   self-contained: a `key_press` presses modifiers, taps, and releases within
//!   ONE handle, and `release_keys_when_dropped` (enigo default) cleans up held
//!   keys even on a mid-sequence error.
//! - **The OS permission IS the real consent barrier** — [`ConsentProof`] is a
//!   flow/TTL token (forgeable in-process by design); what actually gates the
//!   capability is the OS grant (macOS Accessibility · checked by `Enigo::new`).
//!   The check happens at DISPATCH: every per-call construction verifies the
//!   grant silently (`open_prompt_to_get_permissions = false` — an agent loop
//!   must never pop an OS dialog) and a denied grant returns
//!   [`InputError::ConsentDenied`] with remediation guidance.
//!   [`EnigoInputDevice::request_consent`] mints the flow token only and touches
//!   no OS API (hermetic · Invariant #27 — `cargo test --lib` must never prompt).
//! - **Guard-1 error hygiene** — on the `type_text`/`key_press` paths the enigo
//!   error message is NEVER forwarded ([`enigo::InputError::Mapping`] carries a
//!   `String` derived from the typed character on X11); those paths map to
//!   [`InputError::EventPostFailed`] with a STATIC reason. Pointer paths
//!   (`move_cursor`/`click`) forward the enigo detail (no text involved).
//!
//! CANCEL SAFETY: dropping a dispatch future before completion detaches the
//! `spawn_blocking` worker (tokio blocking tasks are not cancellable); the
//! worker finishes posting its single event batch and drops the handle —
//! `release_keys_when_dropped` guarantees no key stays held.

// Tests assert on Result/Option outcomes; `.unwrap()`/`.expect()` are the
// idiomatic test-failure path (never in `src/` non-test code per Diamond Rule).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::marker::PhantomData;

use enigo::{Button, Coordinate, Direction, Enigo, Keyboard, Mouse, NewConError, Settings};
pub use nika_kernel::io::input::InputError as Error;
use nika_kernel::io::input::{
    Authorized, ConsentProof, ConsentState, InputError, KeyCode, KeyMods, MouseButton, Point,
    Unconfirmed,
};

mod keymap;

use keymap::{ChordPlan, chord_plan};

/// Guard 1 (ADR-081 · MANDATORY) · the ONLY representation typed text may take
/// in any log / journal / error — **pure** · headless-testable · mutation-killable.
///
/// Returns a count-only summary (e.g. `"<redacted · 11 code points>"`) that
/// contains NONE of the input's content. Counts Unicode scalar values
/// (`chars`) — a stable, content-free unit (NOT user-perceived graphemes: a
/// combining sequence like `e\u{301}` counts as 2). `type_text` carries the raw
/// text to the backend inside the un-formattable `TypedText` wrapper and uses
/// this for every observability path — the typed text NEVER appears anywhere
/// else.
#[must_use]
pub fn redact_typed_text(text: &str) -> String {
    let n = text.chars().count();
    let unit = if n == 1 { "code point" } else { "code points" };
    format!("<redacted · {n} {unit}>")
}

/// Guard 2 (ADR-081 · MANDATORY) · is the proof still valid at `now_ns` —
/// **pure** · headless-testable. `ttl_ns == 0` means infinite (until system
/// revocation). Otherwise valid iff `0 <= now_ns - granted_at_ns <= ttl_ns`.
///
/// A finite-TTL proof whose `granted_at_ns` reads as in-the-FUTURE
/// (`now_ns < granted_at_ns`) is **expired** — a TTL gate that cannot trust
/// its clock reading must fail CLOSED, never extend consent. With the
/// process-local monotonic `now_ns` source this only happens for a proof
/// minted by a DIFFERENT process epoch (cross-process replay · anchor reset),
/// which is exactly what must be rejected.
#[must_use]
pub fn consent_valid(proof: &ConsentProof, now_ns: u64) -> bool {
    if proof.ttl_ns == 0 {
        return true;
    }
    match now_ns.checked_sub(proof.granted_at_ns) {
        Some(elapsed) => elapsed <= proof.ttl_ns,
        // Grant reads as in-the-future → non-monotonic reading → fail CLOSED.
        None => false,
    }
}

/// Read the process-local MONOTONIC clock as nanoseconds since the first call
/// (the anchor) — the Guard 2 TTL clock, per the kernel's `ConsentProof`
/// contract ("MONOTONIC nanoseconds"). `std::time::Instant` is immune to
/// wall-clock rollback: setting the system clock back cannot extend a
/// finite-TTL consent window (fail-CLOSED by construction).
///
/// Consequence: proof timestamps are meaningful ONLY within this process —
/// a persisted/replayed proof from another run reads as in-the-future or
/// absurdly stale and is rejected by [`consent_valid`]. That is intentional:
/// synthetic-input consent is a per-session grant, never a durable token.
/// Kept OUT of the pure core (Invariant #27): [`consent_valid`] takes
/// `now_ns` explicitly. Saturates at `u64::MAX` (~584 years of uptime).
fn now_ns() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static ANCHOR: OnceLock<Instant> = OnceLock::new();
    let anchor = *ANCHOR.get_or_init(Instant::now);
    u64::try_from(anchor.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Construct the per-dispatch `Enigo` handle — SILENT permission check
/// (`open_prompt_to_get_permissions = false`: an agent loop must never pop an
/// OS dialog; a missing grant fails CLOSED with remediation in the error text).
/// Called inside `spawn_blocking` only: the handle holds `!Send` OS resources
/// (macOS `CGEventSource`/`CGDisplay`) and lives entirely on one worker.
fn new_dispatch_enigo() -> Result<Enigo, InputError> {
    Enigo::new(&dispatch_settings()).map_err(|e| map_new_con_error(&e))
}

/// The dispatch-path enigo settings — PURE (headless-testable): the
/// security-relevant flag is `open_prompt_to_get_permissions = false`
/// (silent grant check · an agent loop must never pop an OS dialog).
fn dispatch_settings() -> Settings {
    Settings {
        open_prompt_to_get_permissions: false,
        ..Settings::default()
    }
}

/// Map enigo's connection error to the kernel's typed [`InputError`]:
/// a denied OS grant is a CONSENT failure (NIKA-1301 · remediation = grant
/// Accessibility / Input Monitoring); everything else means no usable backend
/// session on this host (NIKA-1304 · headless CI · no display server).
fn map_new_con_error(e: &NewConError) -> InputError {
    match e {
        NewConError::NoPermission => InputError::ConsentDenied,
        NewConError::EstablishCon(_) | NewConError::Reply | NewConError::NoEmptyKeycodes => {
            InputError::BackendUnavailable
        }
    }
}

/// Map the kernel's canonical 3-button enum to enigo's.
///
/// # Errors
/// [`InputError::EventPostFailed`] for a kernel variant newer than this crate
/// (the kernel enum is `#[non_exhaustive]` — fail closed, never guess).
fn map_button(button: MouseButton) -> Result<Button, InputError> {
    match button {
        MouseButton::Left => Ok(Button::Left),
        MouseButton::Right => Ok(Button::Right),
        MouseButton::Middle => Ok(Button::Middle),
        _ => Err(InputError::EventPostFailed {
            reason: "unmapped MouseButton variant (kernel newer than nika-input)".to_owned(),
        }),
    }
}

/// Guard 1 (ADR-081) · STRUCTURAL leak prevention for the typed text in
/// transit. The wrapper implements NEITHER `Debug` NOR `Display` (and never
/// will), so any attempt to interpolate it into an error / log / `format!`
/// fails to COMPILE — the guard is the type system, not reviewer discipline.
/// The only exit is [`Self::expose_to_backend`] (the single `enigo.text` call
/// site); observability uses [`redact_typed_text`] BEFORE wrapping.
struct TypedText(String);

impl TypedText {
    /// The single sanctioned read of the raw text — the backend call site.
    fn expose_to_backend(&self) -> &str {
        &self.0
    }
}

/// Guard 1 (ADR-081) · map an enigo error from a TEXT-CARRYING path
/// (`type_text` / `key_press`) to the kernel error with a STATIC reason.
///
/// enigo's `Mapping`/`Unmapping` payloads can embed the typed character (X11
/// keysym text) — forwarding them would leak typed content into the error
/// `Display`, violating Guard 1. Only the error KIND survives.
fn sanitize_text_error(e: &enigo::InputError) -> InputError {
    let reason = match e {
        enigo::InputError::Mapping(_) => "keymap: mapping a keycode failed",
        enigo::InputError::Unmapping(_) => "keymap: unmapping a keycode failed",
        enigo::InputError::NoEmptyKeycodes => "keymap: no empty keycodes available",
        enigo::InputError::Simulate(_) => "synthetic event simulation failed",
        enigo::InputError::InvalidInput(_) => "invalid input (e.g. NUL byte in text)",
    };
    InputError::EventPostFailed {
        reason: reason.to_owned(),
    }
}

/// Map an enigo error from a POINTER path (`move_cursor` / `click`) — no typed
/// text is involved, so the full enigo detail is safe to forward.
fn pointer_error(e: &enigo::InputError) -> InputError {
    InputError::EventPostFailed {
        reason: e.to_string(),
    }
}

/// Map a tokio join failure (worker panic / runtime shutdown) to NIKA-1305.
fn join_error(e: &tokio::task::JoinError) -> InputError {
    // Guard 1 (ADR-081) · NEVER `e.to_string()`: tokio's `JoinError` Display
    // embeds the panic PAYLOAD ("task N panicked with message {msg}"). If the
    // `spawn_blocking` backend panicked with the typed text in its message,
    // `to_string()` would leak the secret into NIKA-1305 — the exact leak Guard
    // 1 forbids. Distinguish the cause structurally; carry NO payload.
    let reason = if e.is_cancelled() {
        "input task cancelled"
    } else {
        "input task panicked (payload suppressed · Guard 1)"
    };
    InputError::TaskJoinFailed {
        reason: reason.to_owned(),
    }
}

/// Execute a [`ChordPlan`] on ONE handle: press held, tap, release in reverse
/// order. Releases are attempted even when the tap fails (the first release
/// error is reported only if the tap itself succeeded); enigo's
/// `release_keys_when_dropped` (default ON) backstops any modifier still held
/// when the handle drops. FFI residue (needs a live OS session) — the decision
/// logic lives in the PURE [`chord_plan`].
fn press_chord(enigo: &mut Enigo, plan: &ChordPlan) -> Result<(), InputError> {
    for &m in &plan.held {
        enigo
            .key(m, Direction::Press)
            .map_err(|e| sanitize_text_error(&e))?;
    }
    let tap = enigo
        .key(plan.tap, Direction::Click)
        .map_err(|e| sanitize_text_error(&e));
    let mut release_err = None;
    for &m in plan.held.iter().rev() {
        if let Err(e) = enigo.key(m, Direction::Release) {
            release_err.get_or_insert(sanitize_text_error(&e));
        }
    }
    tap?;
    match release_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Synthetic-input backend (cross-platform via `enigo`). Generic over the
/// consent type-state — only `EnigoInputDevice<Authorized>` exposes the mutating
/// methods (Guard 2 · compile-time). Zero-size: the `Enigo` handle is
/// constructed PER CALL inside `spawn_blocking` (it holds `!Send` OS resources
/// and its methods take `&mut self` — worker-local, never stored).
///
/// Deliberately NOT `Default`: an `Authorized` device must only ever come from
/// [`EnigoInputDevice::request_consent`]. A `Default` derive here would arm a
/// latent Guard-2 bypass — the day a kernel marker gains `Default`,
/// `EnigoInputDevice::<Authorized>::default()` would mint consent from thin air.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct EnigoInputDevice<S: ConsentState> {
    _state: PhantomData<S>,
}

impl EnigoInputDevice<Unconfirmed> {
    /// Construct an un-consented device. The mutating methods are un-callable
    /// (`S::Granted = Infallible`) until [`Self::request_consent`] transitions
    /// the device to `Authorized`.
    ///
    /// Touches no OS API (hermetic — `cargo test --lib` must never prompt or
    /// connect to a display server). The OS-grant check happens per dispatch.
    ///
    /// # Errors
    /// None today (the `Result` shape is the forward-compat seam every kernel
    /// constructor keeps).
    pub fn new() -> Result<Self, InputError> {
        Ok(Self {
            _state: PhantomData,
        })
    }

    /// Acquire consent and transition to `Authorized` (Guard 2 · compile-time).
    /// Returns the proof (TTL-stamped on the process-monotonic clock) + the
    /// authorized device. `ttl_ns == 0` mints an infinite-TTL proof.
    ///
    /// Mints the FLOW token only — no OS API is touched (hermetic ·
    /// Invariant #27). The real capability barrier is the OS grant (macOS
    /// Accessibility + Input-Monitoring · Linux uinput/display session ·
    /// Windows `UIAccess`), verified SILENTLY at every dispatch: a missing
    /// grant surfaces there as [`InputError::ConsentDenied`] — never as an OS
    /// prompt popped from inside an agent loop.
    ///
    /// # Errors
    /// None today (`Result` kept as the forward-compat seam — a future
    /// surface may add an explicit interactive-grant flow here).
    pub fn request_consent(
        self,
        ttl_ns: u64,
    ) -> Result<(EnigoInputDevice<Authorized>, ConsentProof), InputError> {
        let proof = ConsentProof::new(now_ns(), ttl_ns);
        Ok((
            EnigoInputDevice {
                _state: PhantomData,
            },
            proof,
        ))
    }
}

impl EnigoInputDevice<Authorized> {
    /// Guard 2 runtime re-check shared by every dispatch method — reads the
    /// clock at the call site and validates the proof's TTL (the pure
    /// [`consent_valid`] does the math).
    fn check_consent(proof: &ConsentProof) -> Result<(), InputError> {
        if consent_valid(proof, now_ns()) {
            Ok(())
        } else {
            Err(InputError::ConsentExpired)
        }
    }
}

// The SEND variant is the one to implement (the kernel's one-way blanket impl
// derives the local `InputDevice` from it — implementing the local trait
// instead would leave `T: InputDeviceDyn<S>` consumers, the documented
// downstream bound, unsatisfiable). Bodies are `Send` by construction: every
// captured value is owned + `Send`, the `Enigo` handle never crosses an await.
impl nika_kernel::io::input::InputDeviceDyn<Authorized> for EnigoInputDevice<Authorized> {
    async fn move_cursor(&self, to: Point, proof: &ConsentProof) -> Result<(), InputError> {
        Self::check_consent(proof)?; // Guard 2 · runtime TTL re-check
        tokio::task::spawn_blocking(move || {
            let mut enigo = new_dispatch_enigo()?;
            enigo
                .move_mouse(to.x, to.y, Coordinate::Abs)
                .map_err(|e| pointer_error(&e))
        })
        .await
        .map_err(|e| join_error(&e))?
    }

    async fn click(&self, button: MouseButton, proof: &ConsentProof) -> Result<(), InputError> {
        Self::check_consent(proof)?;
        let button = map_button(button)?;
        tokio::task::spawn_blocking(move || {
            let mut enigo = new_dispatch_enigo()?;
            enigo
                .button(button, Direction::Click)
                .map_err(|e| pointer_error(&e))
        })
        .await
        .map_err(|e| join_error(&e))?
    }

    async fn type_text(&self, text: &str, proof: &ConsentProof) -> Result<(), InputError> {
        Self::check_consent(proof)?;
        // Guard 1 · wrap IMMEDIATELY: from here to the single backend call the
        // text is structurally un-formattable (TypedText has no Debug/Display);
        // the error path is sanitize_text_error (static reasons only).
        let text = TypedText(text.to_owned());
        tokio::task::spawn_blocking(move || {
            let mut enigo = new_dispatch_enigo()?;
            enigo
                .text(text.expose_to_backend())
                .map_err(|e| sanitize_text_error(&e))
        })
        .await
        .map_err(|e| join_error(&e))?
    }

    async fn key_press(
        &self,
        key: KeyCode,
        modifiers: KeyMods,
        proof: &ConsentProof,
    ) -> Result<(), InputError> {
        Self::check_consent(proof)?;
        // Resolve the chord BEFORE touching any OS API: a fail-closed mapping
        // error (unknown kernel variant) surfaces without constructing a handle.
        let plan = chord_plan(key, modifiers)?;
        tokio::task::spawn_blocking(move || {
            let mut enigo = new_dispatch_enigo()?;
            press_chord(&mut enigo, &plan)
        })
        .await
        .map_err(|e| join_error(&e))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::io::input::InputDevice;
    use nika_kernel::prelude::{NikaErrorCode, codes};

    // ─── Guard 1 · redact_typed_text (MANDATORY · pure) ───────────────────

    #[test]
    fn redact_is_a_pure_function_of_length_only() {
        // The Guard-1 leak invariant, stated structurally: the redaction depends
        // ONLY on the input's code-point COUNT, never on its content. Two distinct
        // secrets of equal length MUST redact identically — that is what makes it
        // impossible for any byte of the secret to survive into an observability
        // path. (Substring non-containment alone is too weak: the fixed template
        // word "redacted" itself contains common letters like 'a'.)
        let pairs = [
            ("hunter2", "AAAAAAA"),   // 7 code points
            ("p@ssw0rd", "12345678"), // 8 code points
            ("🔐émojì", "abcdef"),    // 6 code points (multi-byte vs ASCII)
            ("", ""),                 // 0 code points
        ];
        for (a, b) in pairs {
            assert_eq!(
                a.chars().count(),
                b.chars().count(),
                "test fixtures equal length"
            );
            assert_eq!(
                redact_typed_text(a),
                redact_typed_text(b),
                "redaction must not depend on content — {a:?} vs {b:?} leaked a difference"
            );
        }
    }

    #[test]
    fn redact_does_not_leak_realistic_secrets_verbatim() {
        // Defense-in-depth for realistic secrets (the degenerate single-letter
        // case is covered structurally by the length-only test above).
        for secret in ["hunter2", "p@ssw0rd!", "🔐émojì-pass", "correct horse"] {
            let red = redact_typed_text(secret);
            for tok in secret.split_whitespace() {
                assert!(!red.contains(tok), "redaction leaked {tok:?} in {red:?}");
            }
            assert!(red.starts_with("<redacted · "), "redacted shape: {red}");
        }
    }

    #[test]
    fn redact_counts_code_points_not_bytes() {
        // 4 multi-byte chars (12 bytes) must report 4 code points, not 12.
        assert_eq!(redact_typed_text("é🔐ü水"), "<redacted · 4 code points>");
        assert_eq!(redact_typed_text("a"), "<redacted · 1 code point>");
        assert_eq!(redact_typed_text(""), "<redacted · 0 code points>");
    }

    // ─── Guard 2 · consent_valid (MANDATORY · pure) ───────────────────────

    #[test]
    fn consent_infinite_ttl_always_valid() {
        let p = ConsentProof::new(1_000, 0); // ttl 0 = infinite
        assert!(consent_valid(&p, 1_000));
        assert!(consent_valid(&p, u64::MAX));
        assert!(consent_valid(&p, 0)); // even "before" granted
    }

    #[test]
    fn consent_finite_ttl_boundary() {
        let p = ConsentProof::new(1_000, 500); // valid window [1000, 1500]
        assert!(consent_valid(&p, 1_000), "at grant");
        assert!(consent_valid(&p, 1_500), "exactly at ttl boundary is valid");
        assert!(!consent_valid(&p, 1_501), "one ns past ttl is expired");
        assert!(consent_valid(&p, 1_250), "mid-window");
    }

    #[test]
    fn consent_grant_in_future_fails_closed_for_finite_ttl() {
        // now < granted_at (non-monotonic reading · cross-process replay):
        // a TTL gate that cannot trust its clock must EXPIRE, never extend.
        let p = ConsentProof::new(5_000, 100);
        assert!(
            !consent_valid(&p, 4_000),
            "finite ttl + future grant = expired"
        );
        // Infinite TTL is exempt by contract (validity is not time-derived).
        let inf = ConsentProof::new(5_000, 0);
        assert!(consent_valid(&inf, 4_000));
    }

    #[test]
    fn now_ns_is_monotonic_nondecreasing() {
        let a = now_ns();
        let b = now_ns();
        let c = now_ns();
        assert!(a <= b && b <= c, "monotonic clock must never go backward");
    }

    // ─── Type-state + consent flow (hermetic — no OS dispatch) ───────────────

    #[test]
    fn request_consent_mints_a_faithful_proof() {
        // Pins the minting site: ttl_ns is stored VERBATIM and granted_at_ns is
        // a now_ns() reading taken inside the call (kills arg-swap / hardcode
        // mutants the dispatch tests cannot see).
        let before = now_ns();
        let (_auth, proof) = EnigoInputDevice::new()
            .expect("construct")
            .request_consent(60_000_000_000)
            .expect("grant");
        let after = now_ns();
        assert_eq!(proof.ttl_ns, 60_000_000_000, "ttl stored verbatim");
        assert!(
            (before..=after).contains(&proof.granted_at_ns),
            "granted_at_ns must be a now_ns() reading taken inside request_consent"
        );
        assert!(
            consent_valid(&proof, now_ns()),
            "a freshly minted 60s proof validates immediately"
        );
    }

    #[tokio::test]
    async fn all_four_methods_reject_an_expired_proof_before_any_backend() {
        // Guard 2 wired on EVERY dispatch method: the stale proof is rejected
        // by check_consent BEFORE spawn_blocking / Enigo construction — which
        // is also what keeps this test hermetic (no OS API is reached).
        let (auth, _live) = EnigoInputDevice::new()
            .expect("construct")
            .request_consent(1)
            .expect("grant");
        let stale = ConsentProof::new(0, 1); // long expired on the process clock
        let mods = KeyMods::default();
        let errs = [
            auth.move_cursor(Point::new(10, 20), &stale).await,
            auth.click(MouseButton::Left, &stale).await,
            auth.type_text("secret", &stale).await,
            auth.key_press(KeyCode::A, mods, &stale).await,
        ];
        for (i, r) in errs.into_iter().enumerate() {
            let err = r.expect_err("stale proof must be rejected");
            assert_eq!(
                err.nika_code(),
                codes::NIKA_1302,
                "method #{i}: Guard 2 must fire before the backend"
            );
            // Guard 1 cross-check: the rejection text never echoes typed content.
            assert!(!err.to_string().contains("secret"));
        }
    }

    // ─── enigo boundary maps (pure · backend never constructed) ──────────────

    #[test]
    fn map_button_covers_the_kernel_enum() {
        assert!(matches!(map_button(MouseButton::Left), Ok(Button::Left)));
        assert!(matches!(map_button(MouseButton::Right), Ok(Button::Right)));
        assert!(matches!(
            map_button(MouseButton::Middle),
            Ok(Button::Middle)
        ));
    }

    #[test]
    fn dispatch_settings_never_prompt() {
        // The security-relevant flag: dispatch-path Enigo construction must
        // NEVER pop the OS permission dialog (silent check · fail closed).
        assert!(!dispatch_settings().open_prompt_to_get_permissions);
    }

    #[test]
    fn typed_text_exposes_exactly_the_wrapped_text_to_the_backend() {
        let t = TypedText("hunter2".to_owned());
        assert_eq!(t.expose_to_backend(), "hunter2");
        assert_eq!(TypedText(String::new()).expose_to_backend(), "");
    }

    #[test]
    fn new_con_error_maps_no_permission_to_consent_denied() {
        assert_eq!(
            map_new_con_error(&NewConError::NoPermission).nika_code(),
            codes::NIKA_1301,
            "denied OS grant = consent failure with remediation"
        );
        assert_eq!(
            map_new_con_error(&NewConError::EstablishCon("no display")).nika_code(),
            codes::NIKA_1304,
            "no display session = backend unavailable"
        );
    }

    #[test]
    fn text_path_errors_never_forward_the_enigo_payload() {
        // Guard 1: enigo Mapping payloads can embed the typed character (X11
        // keysym). The sanitized reason must drop the payload entirely.
        let leaky = enigo::InputError::Mapping("keysym for 'h' of \"hunter2\"".to_owned());
        let sanitized = sanitize_text_error(&leaky);
        let shown = sanitized.to_string();
        assert!(!shown.contains("hunter2"), "payload leaked: {shown}");
        assert!(!shown.contains("keysym for"), "payload leaked: {shown}");
        assert_eq!(sanitized.nika_code(), codes::NIKA_1303);
        // Pointer paths carry no text — full detail is allowed through.
        let pointer = pointer_error(&enigo::InputError::Simulate("event tap failed"));
        assert!(pointer.to_string().contains("event tap failed"));
        assert_eq!(pointer.nika_code(), codes::NIKA_1303);
    }

    #[tokio::test]
    async fn join_error_never_leaks_a_panic_payload() {
        // Guard 1 · the panic-payload path: tokio's JoinError Display embeds the
        // panic message. A backend that panicked with the typed secret in its
        // message must NOT leak it through NIKA-1305. Force a spawn_blocking task
        // to panic WITH the secret in its payload, then map the JoinError.
        let secret = "hunter2-PANIC-PAYLOAD";
        let secret_owned = secret.to_owned();
        let join_err = tokio::task::spawn_blocking(move || {
            #[expect(
                clippy::panic,
                reason = "deliberate: forces the panic-payload leak path"
            )]
            {
                panic!("backend exploded while typing {secret_owned}");
            }
        })
        .await
        .expect_err("the task panicked");
        // Sanity: the raw JoinError Display DOES carry the secret (the trap).
        assert!(
            join_err.to_string().contains(secret),
            "precondition: tokio Display embeds the payload"
        );
        // Our mapping must NOT.
        let mapped = join_error(&join_err);
        let shown = mapped.to_string();
        assert!(
            !shown.contains(secret),
            "Guard 1 LEAK via panic payload: {shown}"
        );
        assert!(
            !shown.contains("backend exploded"),
            "payload leaked: {shown}"
        );
        assert_eq!(mapped.nika_code(), codes::NIKA_1305);
    }

    proptest::proptest! {
        /// Guard-1 invariant under ANY input, stated structurally: the redaction
        /// is a pure function of the code-point COUNT — equal-length inputs
        /// redact identically, so no byte of the content can survive into the
        /// output. (A substring non-containment assert would latently flake:
        /// generated inputs like "red" or "ode" are substrings of the FIXED
        /// template "<redacted · N code points>" — review-swarm finding.)
        #[test]
        fn redact_is_content_independent_proptest(s in ".{0,256}") {
            let red = redact_typed_text(&s);
            proptest::prop_assert!(red.starts_with("<redacted · "));
            proptest::prop_assert!(red.ends_with('>'));
            let same_len_probe = "x".repeat(s.chars().count());
            proptest::prop_assert_eq!(red, redact_typed_text(&same_len_probe));
        }
    }
}
