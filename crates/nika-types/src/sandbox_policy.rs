// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The sandbox policy vocabulary (#889 · #822 DRI answers) — ONE tri-state
//! knob, parsed once at the composition root, never re-read deep in the
//! stack. The severity derives from the DECLARED CONTRACT, never from a
//! machine profile: a workflow that declares `permits:` asserts an
//! authority boundary, and the engine confines it or refuses — a
//! permit-less run keeps ADR-080 Q4.B's platform-gated best-effort.

use core::fmt;
use core::str::FromStr;

/// `NIKA_SANDBOX=auto|require|off` — the one knob (#889 · house env style,
/// the `NIKA_MCP_TOKEN` precedent). Default when unset: [`Self::Auto`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SandboxPolicy {
    /// The derived posture: confinement is REQUIRED when the workflow
    /// declares `permits:` · permit-less runs stay best-effort.
    #[default]
    Auto,
    /// The operator's hard line: confinement is required, permits or not.
    Require,
    /// The explicit waiver — the run may proceed unconfined, and every
    /// waiver is WITNESSED (the journal's opening frame attests it).
    Off,
}

/// The verdict one policy reaches over one selection — the truth table the
/// composition root applies. Invariant (#889's property): NO cell yields a
/// silent unconfined-with-permits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxVerdict {
    /// The host confines — proceed.
    Confined,
    /// No backend, nothing declared — best-effort noop, honestly (Q4.B).
    PermitlessBestEffort,
    /// No backend, permits declared, the operator waived — proceed
    /// unconfined AND attested.
    Waived,
    /// No backend and the policy refuses — the run never starts.
    Refused,
}

impl SandboxPolicy {
    /// The env var's name — read ONCE at the composition root.
    pub const ENV_VAR: &'static str = "NIKA_SANDBOX";

    /// The stable spelling (journal attestation · notes).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Require => "require",
            Self::Off => "off",
        }
    }

    /// The truth table, pure: policy × confined × permits-declared.
    #[allow(clippy::match_same_arms)]
    // a truth table NAMES every cell —
    // identical outcomes across distinct cells are the contract here.
    #[must_use]
    pub fn judge(self, confined: bool, permits_declared: bool) -> SandboxVerdict {
        if confined {
            return SandboxVerdict::Confined;
        }
        match (self, permits_declared) {
            (Self::Require, _) => SandboxVerdict::Refused,
            (Self::Auto, true) => SandboxVerdict::Refused,
            (Self::Auto, false) => SandboxVerdict::PermitlessBestEffort,
            (Self::Off, true) => SandboxVerdict::Waived,
            (Self::Off, false) => SandboxVerdict::PermitlessBestEffort,
        }
    }
}

impl FromStr for SandboxPolicy {
    type Err = SandboxPolicyParseError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "auto" => Ok(Self::Auto),
            "require" => Ok(Self::Require),
            "off" => Ok(Self::Off),
            _ => Err(SandboxPolicyParseError(alloc::string::String::from(raw))),
        }
    }
}

/// An unparsable `NIKA_SANDBOX` value — a REFUSAL, never a loud fallback:
/// a typo'd security knob silently degrading to `auto` is the fail-open
/// class the policy exists to kill (#889).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPolicyParseError(pub alloc::string::String);

impl fmt::Display for SandboxPolicyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NIKA-1711 · invalid {} value {:?} — expected auto | require | off",
            SandboxPolicy::ENV_VAR,
            self.0
        )
    }
}

impl core::error::Error for SandboxPolicyParseError {}

/// The env-free parse seam (#889): unset is Auto, anything set must parse
/// (the pure half — the env READ lives at the runtime's composition root,
/// never in this L0 leaf, which is `no_std` and zero-I/O by law).
///
/// # Errors
/// [`SandboxPolicyParseError`]
/// when the word is anything but `auto | require | off`.
pub fn parse_policy_env(raw: Option<&str>) -> Result<SandboxPolicy, SandboxPolicyParseError> {
    match raw {
        None => Ok(SandboxPolicy::Auto),
        Some(word) => word.parse(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full truth table — policy × confined × permits, every cell
    /// named. The #889 invariant: no cell is a SILENT unconfined-with-
    /// permits (`Off + permits` is loud by construction — witnessed).
    #[test]
    fn the_truth_table() {
        use {SandboxPolicy as P, SandboxVerdict as V};
        for policy in [P::Auto, P::Require, P::Off] {
            for permits in [true, false] {
                assert_eq!(policy.judge(true, permits), V::Confined);
            }
        }
        assert_eq!(P::Require.judge(false, false), V::Refused);
        assert_eq!(P::Require.judge(false, true), V::Refused);
        assert_eq!(P::Auto.judge(false, true), V::Refused);
        assert_eq!(P::Auto.judge(false, false), V::PermitlessBestEffort);
        assert_eq!(P::Off.judge(false, true), V::Waived);
        assert_eq!(P::Off.judge(false, false), V::PermitlessBestEffort);
    }

    /// #822 · the default arm. A mutant that turns Auto+permits+no-backend
    /// into `PermitlessBestEffort` or `Waived` is the fail-open that shipped.
    #[test]
    fn default_arm_fails_closed_when_permits_meet_a_missing_backend() {
        assert_eq!(SandboxPolicy::default(), SandboxPolicy::Auto);
        assert_eq!(
            SandboxPolicy::default().judge(false, true),
            SandboxVerdict::Refused
        );
        assert_ne!(
            SandboxPolicy::default().judge(false, true),
            SandboxVerdict::PermitlessBestEffort
        );
        assert_ne!(
            SandboxPolicy::default().judge(false, true),
            SandboxVerdict::Waived
        );
    }

    #[test]
    fn the_knob_parses_exactly_three_words() {
        assert_eq!("auto".parse(), Ok(SandboxPolicy::Auto));
        assert_eq!("require".parse(), Ok(SandboxPolicy::Require));
        assert_eq!("off".parse(), Ok(SandboxPolicy::Off));
        assert!("requrie".parse::<SandboxPolicy>().is_err());
        assert!("AUTO".parse::<SandboxPolicy>().is_err());
        assert_eq!(SandboxPolicy::default(), SandboxPolicy::Auto);
        let text = match "requrie".parse::<SandboxPolicy>() {
            Ok(_) => String::new(),
            Err(err) => err.to_string(),
        };
        assert!(!text.is_empty(), "a typo must not parse");
        assert!(text.contains("requrie"), "{text}");
        assert!(text.contains("auto | require | off"), "{text}");
    }
}
