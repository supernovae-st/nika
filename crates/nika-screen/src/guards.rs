// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ADR-081 security guards — capture-LED indicator (guard 6) + consent (guard 7).
//!
//! **B.2 skeleton.** [`ConsentGate`] + [`LedIndicator`] are placeholder structs;
//! the real consent state machine + indicator state land at B.5. Both are
//! **pure-Rust** — the crate is `unsafe_code = forbid` (workspace lint), so the
//! OS-native indicator polling (macOS `AVCaptureDevice` / Windows LED API) is
//! out of scope for this crate and deferred to a future unsafe-allowing layer
//! (or the Olympus cockpit's own FFI). This is ADR-081 guard 6's documented
//! "explicit UI indicator otherwise" fallback: the indicator exposes capture
//! STATE that the consuming UI renders.

use crate::error::ScreenError;

/// Capture-LED indicator (ADR-081 guard 6) — skeleton.
///
/// B.5 ships the real `is_capturing` state + change-notification hook the UI
/// surfaces. The OS-native LED enhancement is deferred (unsafe FFI · separate
/// layer).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct LedIndicator;

impl LedIndicator {
    /// Construct a new capture-LED indicator.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Engage the capture indicator (skeleton no-op · real state at B.5).
    ///
    /// # Errors
    /// Never errors in the skeleton; B.5 may return
    /// [`ScreenError::IndicatorUnavailable`] when no indicator surface exists.
    pub fn engage(&self) -> Result<(), ScreenError> {
        Ok(())
    }

    /// Disengage the capture indicator (skeleton no-op).
    ///
    /// # Errors
    /// Never errors in the skeleton.
    pub fn disengage(&self) -> Result<(), ScreenError> {
        Ok(())
    }
}

/// Consent gate (ADR-081 guard 7) — skeleton.
///
/// B.5 ships the real per-app / session-scoped, revocable consent state
/// machine (in-memory this sprint; persistence location resolved at M2.4
/// nika-input per the M2.1 plan carry-forward).
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ConsentGate;

impl ConsentGate {
    /// Construct a new consent gate.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Check whether capture consent is currently granted (skeleton: denied).
    ///
    /// The skeleton denies by default (fail-closed · sovereignty) — B.5 wires
    /// the real grant/revoke state machine.
    ///
    /// # Errors
    /// Returns [`ScreenError::ConsentDenied`] until B.5 wires real consent.
    pub fn check(&self) -> Result<(), ScreenError> {
        Err(ScreenError::ConsentDenied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Skeleton consent gate fails closed (denied by default · sovereignty).
    #[test]
    fn consent_gate_fails_closed() {
        let gate = ConsentGate::new();
        let res = gate.check();
        assert!(res.is_err(), "skeleton consent denied by default");
        assert_eq!(res.unwrap_err().code(), "NIKA-1006");
    }

    /// Skeleton LED indicator engage/disengage are infallible no-ops.
    #[test]
    fn led_indicator_skeleton_noops() {
        let led = LedIndicator::new();
        assert!(led.engage().is_ok());
        assert!(led.disengage().is_ok());
    }
}
