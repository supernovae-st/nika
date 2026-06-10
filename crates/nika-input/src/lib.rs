// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-input` — synthetic pointer + keyboard L1 effect crate (write-side).
//!
//! Implements the L0.5 [`nika_kernel::io::input::InputDevice<S>`] trait
//! (`move_cursor` · `click` · `type_text` · `key_press`) so the Olympus
//! cockpit can drive the desktop — close the see → understand → **act**
//! computer-use loop after `nika-screen` (capture) · `nika-ocr` (read) ·
//! `nika-a11y` (locate). This is the **first write-side** computer-use crate,
//! and carries the highest review scrutiny in M2.
//!
//! # Two MANDATORY ADR-081 guards (this crate owns both)
//!
//! **Guard 1 · password-typing redaction** ([`redact_typed_text`]) — the text a
//! `type_text` call synthesizes MUST NEVER reach any journal / log / error /
//! `Debug`. A synthetic-input crate that logs what it types leaks passwords by
//! construction. The guard is a PURE emission-boundary transform: `type_text`
//! passes the raw `&str` straight to the backend and NEVER stores or formats it;
//! every observability path uses [`redact_typed_text`] (e.g. `"<redacted · 11
//! code points>"`). `InputError` never carries raw typed text.
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
//!    [`InputError::ConsentExpired`]. Time is read at the L1 dispatch site and
//!    passed into the PURE predicate (test hermeticity · Invariant #27 · the
//!    pure core never reads a clock).
//!
//! # Status (B.2 · the pure security core)
//!
//! B.2 ships the type-state [`EnigoInputDevice<S>`] + the two pure guards
//! (headless-complete + mutation-killable) + the consent flow. The `enigo`
//! cross-platform backend (the OS event synthesis · macOS `CGEvent` · Windows
//! `SendInput` · Linux `uinput`) lands at **B.3**; until then the dispatch
//! methods honour Guard 2 then return [`InputError::BackendUnavailable`]. `enigo`
//! encapsulates the per-OS unsafe FFI, so this crate stays `unsafe_code = forbid`.

// Tests assert on Result/Option outcomes; `.unwrap()`/`.expect()` are the
// idiomatic test-failure path (never in `src/` non-test code per Diamond Rule).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::marker::PhantomData;

pub use nika_kernel::io::input::InputError as Error;
use nika_kernel::io::input::{
    Authorized, ConsentProof, ConsentState, InputError, KeyCode, KeyMods, MouseButton, Point,
    Unconfirmed,
};

/// Guard 1 (ADR-081 · MANDATORY) · the ONLY representation typed text may take
/// in any log / journal / error — **pure** · headless-testable · mutation-killable.
///
/// Returns a count-only summary (e.g. `"<redacted · 11 code points>"`) that
/// contains NONE of the input's content. Counts Unicode scalar values (`chars`),
/// the user-visible unit, not bytes. `type_text` passes the raw `&str` straight
/// to the backend and uses this for every observability path — the typed text
/// NEVER appears anywhere else.
#[must_use]
pub fn redact_typed_text(text: &str) -> String {
    let n = text.chars().count();
    let unit = if n == 1 { "code point" } else { "code points" };
    format!("<redacted · {n} {unit}>")
}

/// Guard 2 (ADR-081 · MANDATORY) · is the proof still valid at `now_ns` —
/// **pure** · headless-testable. `ttl_ns == 0` means infinite (until system
/// revocation). Otherwise valid iff `now_ns - granted_at_ns <= ttl_ns`. A
/// `now_ns` BEFORE `granted_at_ns` (clock skew) is treated as valid (elapsed
/// saturates to 0) — fail-OPEN here is safe because the compile-time type-state
/// already proved consent was granted; this predicate only guards staleness.
#[must_use]
pub fn consent_valid(proof: &ConsentProof, now_ns: u64) -> bool {
    if proof.ttl_ns == 0 {
        return true;
    }
    let elapsed = now_ns.saturating_sub(proof.granted_at_ns);
    elapsed <= proof.ttl_ns
}

/// Read the current wall-clock as nanoseconds since the Unix epoch — the L1
/// dispatch-site clock for the Guard 2 TTL re-check. Kept OUT of the pure core
/// (Invariant #27): the pure [`consent_valid`] takes `now_ns` explicitly.
///
/// Saturates to `u64::MAX` past the year 2554 (the `u64`-ns ceiling) — a
/// pathological far-future clock makes every finite-TTL proof read as expired
/// (fail-CLOSED), which is the safe direction for a consent gate.
fn now_ns() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

/// Synthetic-input backend (cross-platform via `enigo` at B.3). Generic over the
/// consent type-state — only `EnigoInputDevice<Authorized>` exposes the mutating
/// methods (Guard 2 · compile-time). Zero-size: the `enigo` handle lands at B.3.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct EnigoInputDevice<S: ConsentState> {
    _state: PhantomData<S>,
}

impl EnigoInputDevice<Unconfirmed> {
    /// Construct an un-consented device. The mutating methods are un-callable
    /// (`S::Granted = Infallible`) until [`Self::request_consent`] transitions
    /// the device to `Authorized`.
    ///
    /// # Errors
    /// None today. B.3 may add a backend pre-flight (e.g. enigo init) →
    /// [`InputError::BackendUnavailable`].
    pub fn new() -> Result<Self, InputError> {
        Ok(Self {
            _state: PhantomData,
        })
    }

    /// Acquire consent and transition to `Authorized` (Guard 2 · compile-time).
    /// Returns the proof (TTL-stamped now) + the authorized device.
    ///
    /// `ttl_ns == 0` mints an infinite-TTL proof. B.3 wires the real per-OS
    /// entitlement check (macOS Accessibility + Input-Monitoring · Linux
    /// input-group/uinput · Windows `UIAccess`) and errors
    /// [`InputError::ConsentDenied`] when the process lacks the grant; B.2 grants
    /// unconditionally so the type-state + TTL flow is exercisable headlessly.
    ///
    /// # Errors
    /// [`InputError::ConsentDenied`] when the OS denies the entitlement (B.3).
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

impl nika_kernel::io::input::InputDevice<Authorized> for EnigoInputDevice<Authorized> {
    async fn move_cursor(&self, _to: Point, proof: &ConsentProof) -> Result<(), InputError> {
        Self::check_consent(proof)?; // Guard 2 · runtime TTL re-check
        Err(InputError::BackendUnavailable) // B.3 wires the enigo move
    }

    async fn click(&self, _button: MouseButton, proof: &ConsentProof) -> Result<(), InputError> {
        Self::check_consent(proof)?;
        Err(InputError::BackendUnavailable) // B.3 wires the enigo click
    }

    async fn type_text(&self, _text: &str, proof: &ConsentProof) -> Result<(), InputError> {
        Self::check_consent(proof)?;
        // Guard 1 · the raw `_text` is NEVER stored or formatted here; B.3 passes
        // it straight to `enigo.text(_text)`. Any observability path uses
        // `redact_typed_text(_text)`, never the content.
        Err(InputError::BackendUnavailable) // B.3 wires the enigo text synthesis
    }

    async fn key_press(
        &self,
        _key: KeyCode,
        _modifiers: KeyMods,
        proof: &ConsentProof,
    ) -> Result<(), InputError> {
        Self::check_consent(proof)?;
        Err(InputError::BackendUnavailable) // B.3 wires the enigo key map
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
    fn consent_clock_skew_before_grant_is_valid() {
        // now < granted_at (clock skew): elapsed saturates to 0 ⇒ valid.
        let p = ConsentProof::new(5_000, 100);
        assert!(consent_valid(&p, 4_000));
    }

    // ─── Type-state + consent flow ────────────────────────────────────────

    #[tokio::test]
    async fn unconfirmed_to_authorized_then_dispatch_honours_guard2() {
        let dev = EnigoInputDevice::new().expect("construct");
        let (auth, proof) = dev.request_consent(0).expect("grant (infinite ttl)");
        // B.2: the backend is not wired, so a VALID proof reaches the backend
        // stub → BackendUnavailable (NOT ConsentExpired — Guard 2 passed).
        let err = auth
            .move_cursor(Point::new(10, 20), &proof)
            .await
            .expect_err("backend not wired in B.2");
        assert_eq!(err.nika_code(), codes::NIKA_1304, "backend-unavailable");
    }

    #[tokio::test]
    async fn expired_proof_is_rejected_before_backend() {
        let dev = EnigoInputDevice::new().expect("construct");
        // Mint a proof that is already expired: granted far in the past, tiny ttl.
        let (auth, _live) = dev.request_consent(1).expect("grant");
        let stale = ConsentProof::new(0, 1); // granted at ns 0, ttl 1 ns → long expired
        let err = auth
            .type_text("secret", &stale)
            .await
            .expect_err("stale proof rejected");
        assert_eq!(
            err.nika_code(),
            codes::NIKA_1302,
            "Guard 2 · consent expired before the backend (and before any typed-text handling)"
        );
    }

    #[tokio::test]
    async fn all_four_methods_honour_consent_and_stub_backend() {
        let dev = EnigoInputDevice::new().unwrap();
        let (auth, proof) = dev.request_consent(0).unwrap();
        let mods = KeyMods::default();
        assert_eq!(
            auth.move_cursor(Point::new(0, 0), &proof)
                .await
                .unwrap_err()
                .nika_code(),
            codes::NIKA_1304
        );
        assert_eq!(
            auth.click(MouseButton::Left, &proof)
                .await
                .unwrap_err()
                .nika_code(),
            codes::NIKA_1304
        );
        assert_eq!(
            auth.type_text("hi", &proof).await.unwrap_err().nika_code(),
            codes::NIKA_1304
        );
        assert_eq!(
            auth.key_press(KeyCode::A, mods, &proof)
                .await
                .unwrap_err()
                .nika_code(),
            codes::NIKA_1304
        );
    }

    proptest::proptest! {
        /// Guard-1 invariant under any input: the redacted form leaks none of the
        /// input's content and is always the count-only shape.
        #[test]
        fn redact_leaks_nothing_proptest(s in ".{0,256}") {
            let red = redact_typed_text(&s);
            proptest::prop_assert!(red.starts_with("<redacted · "));
            proptest::prop_assert!(red.ends_with('>'));
            // A non-empty, non-whitespace input never appears verbatim.
            let trimmed = s.trim();
            if !trimmed.is_empty() && trimmed.len() > 2 {
                proptest::prop_assert!(!red.contains(trimmed));
            }
        }
    }
}
