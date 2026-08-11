// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE OS-sandbox selection (ADR-095 Layer 6 · #822/#888) — every
//! composition root rides the same decision: the exec runner (`compose`)
//! and the MCP spawn (`nika-mcp`) used to carry twin selectors that would
//! have drifted N×1. The selection lives here once and answers a
//! [`SandboxDecision`](crate::sandbox_select::SandboxDecision) — the
//! record the runtime confines with, the doctor
//! (#891) will report, and the journal witnesses. The selection itself is
//! unchanged (Seatbelt on macOS · bwrap on Linux · the deliberate loud
//! [`NoopSandbox`](nika_kernel::command_sandbox::NoopSandbox) elsewhere)
//! and backend ids stay stable — renaming is a locked non-goal (#822 P3).

use std::sync::Arc;

use nika_kernel::command_sandbox::CommandSandbox;

/// The decision record one selection produces — backend Arc, stable id,
/// and confinement verdict, so no caller re-decides (#889's policy knob
/// and #891's doctor row consume this too).
pub struct SandboxDecision {
    sandbox: Arc<dyn CommandSandbox>,
    backend: &'static str,
}

impl SandboxDecision {
    /// Consume the decision for the backend — the Arc moves into the shell
    /// after [`Self::backend`] is read.
    #[must_use]
    pub fn into_sandbox(self) -> Arc<dyn CommandSandbox> {
        self.sandbox
    }

    /// The stable backend id (`seatbelt` · `landlock` · `noop`) — the impl
    /// names itself, so note, journal, and shell read one string.
    #[must_use]
    pub fn backend(&self) -> &'static str {
        self.backend
    }

    /// True when the selection confines — anything but the deliberate
    /// `noop`, which always answers and confines nothing.
    #[must_use]
    pub fn is_confined(&self) -> bool {
        self.backend != "noop"
    }
}

/// Select the OS command sandbox for this platform (ADR-095 Layer 6):
/// Seatbelt on macOS when `sandbox-exec` answers, bwrap on Linux when the
/// launcher is present, the deliberate loud `NoopSandbox` anywhere else —
/// selected HERE, named by the caller, never the silent default (the
/// kernel seam's law; #889 makes the fail-open refusable at the contract).
#[must_use]
pub fn select_command_sandbox() -> SandboxDecision {
    #[cfg(target_os = "macos")]
    if nika_sandbox_seatbelt::SeatbeltSandbox::available() {
        let sandbox: Arc<dyn CommandSandbox> =
            Arc::new(nika_sandbox_seatbelt::SeatbeltSandbox::new());
        let backend = sandbox.backend();
        return SandboxDecision { sandbox, backend };
    }
    #[cfg(target_os = "linux")]
    if nika_sandbox_landlock::LandlockSandbox::available() {
        let sandbox: Arc<dyn CommandSandbox> =
            Arc::new(nika_sandbox_landlock::LandlockSandbox::new());
        let backend = sandbox.backend();
        return SandboxDecision { sandbox, backend };
    }
    let sandbox: Arc<dyn CommandSandbox> = Arc::new(nika_kernel::command_sandbox::NoopSandbox);
    let backend = sandbox.backend();
    SandboxDecision { sandbox, backend }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The selection matches the host's probe results — the expectation is
    /// computed from the RAW cfgs so the test stays the independent oracle.
    #[test]
    fn the_selection_matches_the_host() {
        let expected = if cfg!(target_os = "macos")
            && nika_sandbox_seatbelt::SeatbeltSandbox::available()
        {
            "seatbelt"
        } else if cfg!(target_os = "linux") && nika_sandbox_landlock::LandlockSandbox::available() {
            "landlock"
        } else {
            "noop"
        };
        let decision = select_command_sandbox();
        assert_eq!(decision.backend(), expected);
        assert_eq!(decision.is_confined(), expected != "noop");
    }

    /// The record yields its Arc intact — the id read BEFORE the move is
    /// the id the moved sandbox still answers.
    #[test]
    fn the_decision_yields_the_arc() {
        let decision = select_command_sandbox();
        let backend = decision.backend();
        let sandbox = decision.into_sandbox();
        assert_eq!(sandbox.backend(), backend);
    }
}
