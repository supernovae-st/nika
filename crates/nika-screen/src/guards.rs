// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ADR-081 security guards — capture-LED indicator (guard 6) + consent (guard 7).
//!
//! **B.5 · real state machines.** Both are pure-Rust (the crate is
//! `unsafe_code = forbid`); the OS-native enhancements (macOS TCC consent
//! dialog · `AVCaptureDevice` recording LED · Windows capture API) are
//! ADR-081's "OS-native when available" path, deferred to a future
//! unsafe-allowing layer (or the Olympus cockpit's own FFI). These types are
//! the "explicit error gate / explicit UI indicator otherwise" fallback · they
//! hold the canonical capture STATE that the OS layer drives and the consuming
//! UI renders.
//!
//! Consent is **in-memory + session-scoped** (ADR-081 · persistence DEFERRED
//! to M2.4 `nika-input` · `~/.olympus/cache/consent/` when the daemon ships) ·
//! **fail-closed** · **revocable**. Telemetry-canon §0 · zero cloud · in-memory
//! only. Both guards use atomics so they are shared behind `&self` (the
//! `ScreenCapture` trait methods take `&self`).

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use crate::error::ScreenError;

/// Capture-consent state (ADR-081 guard 7) — session-scoped · revocable.
///
/// `Unknown` is the fail-closed default: capture is denied until an explicit
/// grant (driven by the OS consent-dialog result at the deferred FFI layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Consent {
    /// No decision yet — fail-closed (capture denied).
    Unknown,
    /// Capture explicitly granted for the session.
    Granted,
    /// Capture explicitly denied.
    Denied,
    /// Consent was granted then revoked — active capture must tear down.
    Revoked,
}

impl Consent {
    /// Stable wire encoding (`Unknown == 0` so `AtomicU8::default()` is fail-closed).
    const fn as_u8(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Granted => 1,
            Self::Denied => 2,
            Self::Revoked => 3,
        }
    }

    /// Decode from the atomic store (unknown encodings fail closed to `Unknown`).
    const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Granted,
            2 => Self::Denied,
            3 => Self::Revoked,
            _ => Self::Unknown,
        }
    }
}

/// Consent gate (ADR-081 guard 7) — in-memory · fail-closed · revocable.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ConsentGate {
    state: AtomicU8,
}

impl ConsentGate {
    /// Construct a fail-closed consent gate (`Consent::Unknown`).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current consent state.
    #[must_use]
    pub fn status(&self) -> Consent {
        Consent::from_u8(self.state.load(Ordering::SeqCst))
    }

    /// Grant capture consent for the session (idempotent).
    pub fn grant(&self) {
        self.state.store(Consent::Granted.as_u8(), Ordering::SeqCst);
    }

    /// Explicitly deny capture consent (fail-closed).
    pub fn deny(&self) {
        self.state.store(Consent::Denied.as_u8(), Ordering::SeqCst);
    }

    /// Revoke previously-granted consent — active capture must tear down.
    pub fn revoke(&self) {
        self.state.store(Consent::Revoked.as_u8(), Ordering::SeqCst);
    }

    /// Gate a capture · `Ok(())` only when consent is `Granted`.
    ///
    /// # Errors
    /// [`ScreenError::ConsentRevoked`] if revoked mid-session · otherwise
    /// [`ScreenError::ConsentDenied`] (fail-closed for `Unknown` / `Denied`).
    pub fn check(&self) -> Result<(), ScreenError> {
        match self.status() {
            Consent::Granted => Ok(()),
            Consent::Revoked => Err(ScreenError::ConsentRevoked),
            Consent::Unknown | Consent::Denied => Err(ScreenError::ConsentDenied),
        }
    }
}

/// Capture-LED indicator (ADR-081 guard 6) — engaged-count state machine.
///
/// `is_engaged()` is true while ≥1 capture is active. [`LedIndicator::engage`]
/// returns a [`LedScope`] RAII guard that disengages on drop — so a panicking
/// or early-returning capture path can never leave the indicator stuck on.
/// Shared behind `Arc` so a streaming-capture worker can hold the scope for the
/// stream's whole lifetime.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct LedIndicator {
    active: AtomicUsize,
}

impl LedIndicator {
    /// Construct a disengaged capture indicator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether capture is currently active (indicator lit).
    #[must_use]
    pub fn is_engaged(&self) -> bool {
        self.active.load(Ordering::SeqCst) > 0
    }

    /// Engage the indicator for the lifetime of the returned [`LedScope`].
    ///
    /// Increments the active-capture count; the indicator disengages when the
    /// count returns to zero (every scope dropped). An associated function (not
    /// a method) because `self: &Arc<Self>` is not a stable receiver type — the
    /// scope must own an `Arc` clone to outlive a `&self` borrow (streaming).
    #[must_use = "drop the LedScope to disengage the capture indicator"]
    pub fn engage(led: &Arc<Self>) -> LedScope {
        led.active.fetch_add(1, Ordering::SeqCst);
        LedScope {
            led: Arc::clone(led),
        }
    }
}

/// RAII guard keeping the capture-LED indicator engaged · disengages on drop.
#[derive(Debug)]
#[non_exhaustive]
pub struct LedScope {
    led: Arc<LedIndicator>,
}

impl Drop for LedScope {
    fn drop(&mut self) {
        // Balanced with the `fetch_add` in `engage` (one scope == one increment).
        self.led.active.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::prelude::{NikaErrorCode, codes};
    use proptest::prelude::*;

    /// Guard 7 happy path · an explicit grant allows capture.
    #[test]
    fn consent_gate_grant_allows() {
        let gate = ConsentGate::new();
        gate.grant();
        assert_eq!(gate.status(), Consent::Granted);
        assert!(gate.check().is_ok(), "granted consent allows capture");
    }

    /// Guard 7 denial path · fail-closed by default + explicit deny (NIKA-1006).
    #[test]
    fn consent_gate_fails_closed() {
        let gate = ConsentGate::new();
        assert_eq!(gate.status(), Consent::Unknown, "default is fail-closed");
        assert_eq!(
            gate.check().expect_err("unknown denies").nika_code(),
            codes::NIKA_1006
        );
        gate.deny();
        assert_eq!(
            gate.check().expect_err("denied denies").nika_code(),
            codes::NIKA_1006
        );
    }

    /// Guard 7 revoke-mid-session (cancel) path · revoked → NIKA-1007.
    #[test]
    fn consent_gate_revoke_denies() {
        let gate = ConsentGate::new();
        gate.grant();
        assert!(gate.check().is_ok());
        gate.revoke();
        assert_eq!(gate.status(), Consent::Revoked);
        assert_eq!(
            gate.check().expect_err("revoked denies").nika_code(),
            codes::NIKA_1007
        );
    }

    /// Guard 6 happy path · the engage scope lights the indicator.
    #[test]
    fn led_engage_scope_lights() {
        let led = Arc::new(LedIndicator::new());
        assert!(!led.is_engaged(), "starts disengaged");
        let scope = LedIndicator::engage(&led);
        assert!(led.is_engaged(), "engaged while scope held");
        drop(scope);
        assert!(!led.is_engaged(), "disengaged after scope drop");
    }

    /// Guard 6 default / disengaged path.
    #[test]
    fn led_starts_disengaged() {
        let led = Arc::new(LedIndicator::new());
        assert!(!led.is_engaged());
    }

    /// Guard 6 concurrency (drop-mid) path · nested scopes count · the
    /// indicator stays lit until the LAST scope drops.
    #[test]
    fn led_nested_engage_counts() {
        let led = Arc::new(LedIndicator::new());
        let s1 = LedIndicator::engage(&led);
        let s2 = LedIndicator::engage(&led);
        assert!(led.is_engaged(), "lit with two scopes");
        drop(s1);
        assert!(led.is_engaged(), "still lit · one scope remains");
        drop(s2);
        assert!(!led.is_engaged(), "disengaged after the last scope");
    }

    /// Guard 7 · `deny()` records the explicit `Denied` state — distinct from
    /// the `Unknown` fail-closed default (both deny capture, but are different
    /// states). Pins `deny` against a no-op mutation.
    #[test]
    fn consent_gate_deny_sets_denied() {
        let gate = ConsentGate::new();
        assert_eq!(gate.status(), Consent::Unknown);
        gate.deny();
        assert_eq!(
            gate.status(),
            Consent::Denied,
            "deny() records Denied, not Unknown"
        );
    }

    proptest! {
        /// Gate 6 · `from_u8` decodes every canonical encoding back to its
        /// variant and fail-closes every out-of-range byte to `Unknown`
        /// (never panics) · canonical variants round-trip through `as_u8`.
        /// Pins each `from_u8` match arm + the fail-closed default.
        #[test]
        fn consent_u8_roundtrip_and_fail_closed(v in any::<u8>()) {
            let decoded = Consent::from_u8(v);
            match v {
                1 => prop_assert_eq!(decoded, Consent::Granted),
                2 => prop_assert_eq!(decoded, Consent::Denied),
                3 => prop_assert_eq!(decoded, Consent::Revoked),
                _ => prop_assert_eq!(decoded, Consent::Unknown), // fail-closed
            }
            prop_assert_eq!(Consent::from_u8(decoded.as_u8()), decoded);
        }

        /// Gate 6 · after any sequence of transitions, `status()` is the last
        /// applied state and `check()` is Ok IFF that state is `Granted`
        /// (1007 revoked · 1006 unknown/denied). Pins grant/deny/revoke
        /// against no-op mutations + the `check()` arms.
        #[test]
        fn consent_check_matches_final_state(
            ops in prop::collection::vec(0u8..3, 0..32),
        ) {
            let gate = ConsentGate::new();
            let mut last = Consent::Unknown;
            for op in &ops {
                match op {
                    0 => { gate.grant();  last = Consent::Granted; }
                    1 => { gate.deny();   last = Consent::Denied; }
                    _ => { gate.revoke(); last = Consent::Revoked; }
                }
            }
            prop_assert_eq!(gate.status(), last);
            match last {
                Consent::Granted => prop_assert!(gate.check().is_ok()),
                Consent::Revoked => {
                    prop_assert_eq!(gate.check().expect_err("revoked").nika_code(), codes::NIKA_1007);
                }
                _ => {
                    prop_assert_eq!(gate.check().expect_err("denied").nika_code(), codes::NIKA_1006);
                }
            }
        }

        /// Gate 6 · the capture-LED is engaged IFF ≥1 scope is held. Push N
        /// scopes (lit), then drop one at a time — lit until the last drop.
        /// Pins the engaged-count `fetch_add`/`fetch_sub`/`> 0` machine.
        #[test]
        fn led_engaged_iff_scopes_held(n in 0usize..16) {
            let led = Arc::new(LedIndicator::new());
            let mut scopes: Vec<LedScope> = (0..n).map(|_| LedIndicator::engage(&led)).collect();
            prop_assert_eq!(led.is_engaged(), n > 0);
            while !scopes.is_empty() {
                scopes.pop();
                prop_assert_eq!(led.is_engaged(), !scopes.is_empty());
            }
            prop_assert!(!led.is_engaged());
        }
    }
}
