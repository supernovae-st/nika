// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE OS-sandbox selection (ADR-095 Layer 6 · #822/#888) — every
//! composition root rides the same decision: the exec runner (`compose`)
//! and the MCP spawn (`nika-mcp`) used to carry twin selectors that would
//! have drifted N×1. The selection lives here once and answers a
//! [`SandboxDecision`] — the
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

pub use nika_types::sandbox_policy::{
    SandboxPolicy, SandboxPolicyParseError, SandboxVerdict, parse_policy_env,
};

/// Read the ONE knob at the composition root (#889) — the
/// `config_from_env` precedent: env is read HERE, at the seam, never deep
/// in the stack (and never inside `nika-types` — the L0 leaf is `no_std`
/// and zero-I/O by law; the pure parse stays there). Unset is `Auto`; an
/// unparsable value REFUSES to start (a typo'd security knob loudly
/// defaulting would be the fail-open class the policy exists to kill).
///
/// # Errors
/// [`SandboxPolicyParseError`] when `NIKA_SANDBOX` holds anything but
/// `auto | require | off`.
pub fn sandbox_policy_from_env() -> Result<SandboxPolicy, SandboxPolicyParseError> {
    #[allow(clippy::disallowed_methods)] // the sanctioned env boundary (config_from_env)
    let raw = std::env::var(SandboxPolicy::ENV_VAR).ok();
    parse_policy_env(raw.as_deref())
}

/// The typed launch refusal when confinement is required and the platform
/// answers noop (#889 · NIKA-1710 — the NIKA-1708/1709 launch-refusal
/// precedent: run-abort BEFORE the prologue, zero events, zero spend, no
/// `on_codes` ladder by design). Carries the exact per-OS fix and the
/// witnessed opt-out.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("NIKA-1710 · {policy_line} — {fix}")]
#[diagnostic(code(nika::sandbox::required_unavailable))]
pub struct SandboxRefusal {
    policy_line: String,
    fix: &'static str,
}

impl SandboxRefusal {
    /// Build the refusal for one policy — the message names WHY the run
    /// refuses (the contract's terms) and BOTH exits.
    #[must_use]
    pub fn for_policy(policy: SandboxPolicy, permits_declared: bool) -> Self {
        let policy_line = match (policy, permits_declared) {
            (SandboxPolicy::Require, _) => {
                "NIKA_SANDBOX=require and no OS sandbox backend on this host".to_owned()
            }
            (_, true) => "this workflow declares `permits:` and no OS sandbox backend can confine \
                 them on this host (NIKA_SANDBOX=auto)"
                .to_owned(),
            (_, false) => "no OS sandbox backend on this host".to_owned(),
        };
        let fix = if cfg!(target_os = "linux") {
            "install bubblewrap (apt install bubblewrap) and re-run · or set \
             NIKA_SANDBOX=off to proceed unconfined — the waiver is attested on the \
             journal · or drop the permits: declaration"
        } else if cfg!(target_os = "macos") {
            "macOS ships sandbox-exec with the OS — a missing launcher means a broken \
             host · or set NIKA_SANDBOX=off to proceed unconfined — the waiver is \
             attested on the journal"
        } else {
            "this platform ships without the OS sandbox layer (ADR-080's documented \
             gap) · set NIKA_SANDBOX=off to proceed unconfined — the waiver is \
             attested on the journal · or drop the permits: declaration"
        };
        Self { policy_line, fix }
    }
}

impl nika_error::traits::NikaErrorCode for SandboxRefusal {
    fn nika_code(&self) -> nika_error::codes::NikaCode {
        nika_error::codes::NIKA_1710
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_error::traits::NikaErrorCode;

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

    /// The knob's pure seam (#889): unset is Auto, set must parse, a typo
    /// refuses and names the grammar.
    #[test]
    fn the_policy_env_seam() {
        assert_eq!(parse_policy_env(None), Ok(SandboxPolicy::Auto));
        assert_eq!(
            parse_policy_env(Some("require")),
            Ok(SandboxPolicy::Require)
        );
        assert_eq!(parse_policy_env(Some("off")), Ok(SandboxPolicy::Off));
        assert!(parse_policy_env(Some("requrie")).is_err());
    }

    #[test]
    fn noop_and_unsupported_hosts_refuse_unless_the_operator_waives() {
        for policy in [SandboxPolicy::Auto, SandboxPolicy::Require] {
            assert_eq!(
                policy.judge(false, true),
                SandboxVerdict::Refused,
                "a permits-bearing run cannot compose a noop/unsupported backend"
            );
        }
        assert_eq!(
            SandboxPolicy::Off.judge(false, true),
            SandboxVerdict::Waived,
            "only the explicit, journalled off posture reaches Noop"
        );
    }

    fn noop_decision() -> SandboxDecision {
        let sandbox: Arc<dyn CommandSandbox> = Arc::new(nika_kernel::command_sandbox::NoopSandbox);
        let backend = sandbox.backend();
        SandboxDecision { sandbox, backend }
    }

    /// #822 at the composition gate: Auto (default) + permits + Noop must
    /// be NIKA-1710, not a composed unconfined run. Deleting the Refused
    /// arm of `apply_sandbox_policy_with` dies here.
    #[test]
    fn default_auto_refuses_to_compose_permits_on_noop() {
        let decision = noop_decision();
        assert!(!decision.is_confined());
        let refused = apply_sandbox_policy_with(SandboxPolicy::Auto, &decision, true);
        assert!(
            matches!(
                &refused,
                Err(e) if e.nika_code().to_string() == "NIKA-1710"
            ),
            "#822: the default arm must fail closed: {refused:?}"
        );
        assert!(
            apply_sandbox_policy_with(SandboxPolicy::Off, &decision, true).is_ok(),
            "only the explicit waiver composes Noop under permits"
        );
    }

    /// The refusal names the code, the reason, and BOTH exits (#889 ·
    /// NIKA-1710).
    #[test]
    fn the_refusal_teaches() {
        let refusal = SandboxRefusal::for_policy(SandboxPolicy::Auto, true);
        let text = refusal.to_string();
        assert!(text.contains("NIKA-1710"), "{text}");
        assert!(text.contains("permits:"), "{text}");
        assert!(text.contains("NIKA_SANDBOX=off"), "{text}");
        assert_eq!(refusal.spec_code(), "NIKA-1710");
        assert!(!refusal.is_transient());
        let hard = SandboxRefusal::for_policy(SandboxPolicy::Require, false);
        assert!(hard.to_string().contains("NIKA_SANDBOX=require"), "{hard}");
    }

    /// The compose channel delegates its code per arm (#889 · the error
    /// one-voice law): the policy typo reads NIKA-1711, the confinement
    /// refusal stays NIKA-1710.
    #[test]
    fn the_compose_channel_speaks_one_voice() {
        let policy = parse_policy_env(Some("requrie")).map_err(ComposeError::from);
        assert!(matches!(policy, Err(ComposeError::Policy(_))));
        assert_eq!(
            policy.err().map(|e| e.nika_code().to_string()).as_deref(),
            Some("NIKA-1711")
        );
        let sandbox: ComposeError = SandboxRefusal::for_policy(SandboxPolicy::Auto, true).into();
        assert_eq!(sandbox.nika_code().to_string(), "NIKA-1710");
    }
}

/// Composition-launch failures — the one channel the composition root
/// returns: the http plane's own error, the sandbox policy's typed
/// refusal (#889 · NIKA-1710), and the knob's parse error (NIKA-1711).
/// A refusal aborts BEFORE the prologue (zero events, zero spend).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ComposeError {
    /// The fetch/provider plane's own error (SSRF · timeout · connect).
    #[error(transparent)]
    #[diagnostic(transparent)]
    Http(#[from] nika_kernel::HttpError),
    /// Confinement required, no OS backend on this host (#889).
    #[error(transparent)]
    #[diagnostic(transparent)]
    Sandbox(#[from] SandboxRefusal),
    /// `NIKA_SANDBOX` holds anything but `auto | require | off` — the
    /// inner type is an L0 leaf (no miette there); the code rides here.
    #[error(transparent)]
    #[diagnostic(code(nika::sandbox::policy_invalid))]
    Policy(#[from] nika_types::sandbox_policy::SandboxPolicyParseError),
}

impl nika_error::traits::NikaErrorCode for ComposeError {
    fn nika_code(&self) -> nika_error::codes::NikaCode {
        match self {
            Self::Http(e) => e.nika_code(),
            Self::Sandbox(e) => e.nika_code(),
            Self::Policy(_) => nika_error::codes::NIKA_1711,
        }
    }
}

/// The policy gate (#889) — the ONE knob read once, judged over the ONE
/// selection. A Refused verdict is the typed launch refusal; the witness
/// rides back with the verdict.
///
/// # Errors
/// [`ComposeError::Sandbox`] when the verdict is Refused ·
/// [`ComposeError::Policy`] when `NIKA_SANDBOX` holds an unknown word.
pub fn apply_sandbox_policy(
    decision: &SandboxDecision,
    permits_declared: bool,
) -> Result<(SandboxPolicy, SandboxVerdict), ComposeError> {
    let policy = sandbox_policy_from_env()?;
    apply_sandbox_policy_with(policy, decision, permits_declared)
}

/// The composition refuse path with the policy already in hand — tests
/// pin #822 here so a deleted `Refused → Err` cannot hide behind env.
fn apply_sandbox_policy_with(
    policy: SandboxPolicy,
    decision: &SandboxDecision,
    permits_declared: bool,
) -> Result<(SandboxPolicy, SandboxVerdict), ComposeError> {
    let verdict = policy.judge(decision.is_confined(), permits_declared);
    if verdict == SandboxVerdict::Refused {
        return Err(SandboxRefusal::for_policy(policy, permits_declared).into());
    }
    Ok((policy, verdict))
}
