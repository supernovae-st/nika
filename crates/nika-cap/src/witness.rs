// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The permit-decision witness (NEP-0007 law 2 · spec 17 §the permit
//! witness) — every decision taken at the DISPATCH boundary is recorded
//! here during the attempt and emitted as one `permit_checked` frame per
//! decision at settle, between `task_started` and the terminal frame
//! (the hash chain binds the decisions to the task that took them).
//!
//! Granted decisions are witnessed, not only refusals: an auditor
//! reconstructs WHAT authority was exercised. The v1 planes are the
//! dispatch-visible gates (exec program · tool grant · the NEP-0004
//! re-gate · the NEP-0005 env composition · the NEP-0006 sink); the
//! per-operation fs/net enforcement decisions live INSIDE the builtin
//! sinks and their allow-side witness is the DECLARED v1 residual
//! (NEP-0007 law 2 · a dispatch-side pre-witness could lie against
//! canonicalize-then-confine).
//!
//! The record + collector live in `nika-cap` (descended from the
//! runtime at the 15k wall · P3 B5): the decision over a [`crate::Permits`]
//! boundary is capability-boundary DATA, and the collector is pure
//! `std` — cheap lock + push during the attempt, drained once by the
//! attempt that created it (telemetry never panics a run).

use std::sync::Mutex;

/// One permit decision (the `permit_checked` frame's payload · spec 17).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PermitDecision {
    /// The plane · `exec` · `tool` · `regate` · `env` · `sink`.
    pub plane: &'static str,
    /// The bound consulted (the program · the tool id · the escaped
    /// bound · the passed names · the artifact class).
    pub gate: String,
    /// `allow` | `deny`.
    pub decision: &'static str,
    /// The law applied (the teaching one-liner).
    pub why: String,
}

/// Collects one attempt's permit decisions (lock + push · drained by
/// the attempt that created it).
#[derive(Debug, Default)]
pub struct PermitWitness {
    decisions: Mutex<Vec<PermitDecision>>,
}

impl PermitWitness {
    /// Construct (INV-019).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one decision (poisoning recovers — the witness must never
    /// panic a run; a poisoned lock keeps the earlier decisions).
    pub fn record(
        &self,
        plane: &'static str,
        gate: impl Into<String>,
        decision: &'static str,
        why: impl Into<String>,
    ) {
        let entry = PermitDecision {
            plane,
            gate: gate.into(),
            decision,
            why: why.into(),
        };
        match self.decisions.lock() {
            Ok(mut d) => d.push(entry),
            Err(poisoned) => poisoned.into_inner().push(entry),
        }
    }

    /// Take the buffered decisions (once · at attempt end).
    #[must_use]
    pub fn take(&self) -> Vec<PermitDecision> {
        match self.decisions.lock() {
            Ok(mut d) => std::mem::take(&mut *d),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_take_roundtrip_and_take_drains() {
        let w = PermitWitness::new();
        w.record("tool", "nika:fetch", "allow", "permits.tools covers it");
        w.record("exec", "rm", "deny", "not in the program allowlist");
        let taken = w.take();
        assert_eq!(taken.len(), 2);
        assert_eq!(taken[0].plane, "tool");
        assert_eq!(taken[0].decision, "allow");
        assert_eq!(taken[1].decision, "deny");
        assert!(w.take().is_empty(), "take drains");
    }
}
