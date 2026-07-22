// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The runtime integrity label (F-O1 PR-1 · the `Integ` axis of RS-06's
//! `Value⟨T, Conf, Integ⟩` trifecta) — COARSE, per-task-output, and
//! ADDITIVE: no gate consumes it yet (PR-2 is the re-gate).
//!
//! The lattice is the two-point one every prod survivor converged on
//! (RS-06 §2 · Perl's one taint bit · Windows MIC): [`Integrity::Trusted`]
//! ⊔ [`Integrity::Untrusted`] = `Untrusted` — a value is only as
//! trustworthy as its least-trustworthy source (Fides, « propagation par
//! join »). The witness (`source`) names the born origin — an ingress
//! task id or `inputs.<name>` — and rides along unchanged through
//! propagation, the same semantics as [`crate::TaintWitness`]'s
//! `source → sink`.
//!
//! Born-untrusted sources (v1 · the mission's provenance table):
//!
//! - the output of an invoked `nika:fetch` (network content);
//! - the output of any `mcp:*` tool — fail-closed: server content is
//!   untrusted UNLESS the catalog's `content_trust` mark clears it, and
//!   the runtime does not consult the mark in v1 (mirrors the check's
//!   absent/unknown default — `nika_check`'s content-flow parity);
//! - the output of an `agent:` whose `tools:` whitelist admits an
//!   ingress-capable tool (a browsing agent's final message is
//!   attacker-influenced even with a clean prompt);
//! - any `${{ inputs.X }}` read — the caller boundary (Perl-taint slot
//!   rule: the plan exposes the slot, so the plan treats it as
//!   untrusted, whether or not THIS run overrode the default).
//!
//! Everything else is trusted by default: the authored plan, `const:`,
//! `config:` (the operator/deployment is the trust anchor), `secrets:`
//! (integrity axis — confidentiality is ADR-092's separate lattice),
//! `exec:` outputs (the file channel argv cannot see is the declared v1
//! residual), child `workflow:` calls (spec 14 owns its boundary).
//!
//! **`infer:`/`agent:` outputs are NOT born untrusted** — RS-06's typed
//! law is `out.integrity = min(inputs) ⊓ ModelCeiling` with
//! `ModelCeiling = Trusted` on the integrity axis: an LLM NEVER raises
//! integrity (a prompt that sees untrusted content yields an untrusted
//! output — « un LLM produit du contenu non fiable »), and a
//! trusted-only prompt stays at the ceiling (`CaMeL`'s P-LLM precedent).
//! Marking every model output untrusted at birth would taint the whole
//! plan and make the label noise — Ruby's `$SAFE` lesson (RS-06 §4), and
//! it would break check≡run parity with the check's content-flow
//! prompt-taint inversion, the drift ENGINE.md's step 1 forbids by
//! construction.

use crate::permits::glob_matches;

/// The coarse integrity label carried by one task's output record
/// (closed at v1 — the `TerminalCause`-style discipline: a new lattice
/// level is a spec amendment, never a boolean beside the label).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Integrity {
    /// Authored content · trusted sources only.
    Trusted,
    /// Attacker-influenceable content — `source` names the born origin
    /// (the ingress task id · `inputs.<name>`), propagated unchanged so
    /// a diagnostic can say WHERE the taint entered.
    Untrusted {
        /// The born-origin witness (see module docs).
        source: String,
    },
}

impl Integrity {
    /// The trusted label (the default — an unlabeled value is trusted,
    /// which is what makes old journals and old records readable).
    #[must_use]
    pub const fn trusted() -> Self {
        Self::Trusted
    }

    /// The untrusted label, witnessed by its born origin.
    #[must_use]
    pub fn untrusted(source: impl Into<String>) -> Self {
        Self::Untrusted {
            source: source.into(),
        }
    }

    /// `true` when no untrusted source reached the value.
    #[must_use]
    pub const fn is_trusted(&self) -> bool {
        matches!(self, Self::Trusted)
    }

    /// `true` when attacker-influenceable content reached the value.
    #[must_use]
    pub const fn is_untrusted(&self) -> bool {
        matches!(self, Self::Untrusted { .. })
    }

    /// The born-origin witness, when untrusted.
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        match self {
            Self::Trusted => None,
            Self::Untrusted { source } => Some(source),
        }
    }

    /// The spec wire word (`trusted` · `untrusted`).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::Untrusted { .. } => "untrusted",
        }
    }

    /// The lattice join (Denning 1976): `Untrusted` dominates. The
    /// witness is the FIRST untrusted origin — deterministic, and the
    /// earliest point the taint entered (the one a fix must address).
    /// This is RS-06's `min(inputs)` — the propagation law that makes a
    /// task's output exactly as trustworthy as its least-trustworthy
    /// source.
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Trusted, Self::Trusted) => Self::Trusted,
            (u @ Self::Untrusted { .. }, _) | (_, u @ Self::Untrusted { .. }) => u,
        }
    }
}

impl Default for Integrity {
    /// Trusted by default — the additive-law keystone: anything written
    /// before the label existed reads as trusted.
    fn default() -> Self {
        Self::Trusted
    }
}

/// Does ONE `tools:` grant entry admit untrusted-ingress content?
/// `nika:fetch` (a glob like `nika:*` covers it) or any `mcp:*` server —
/// server-provided content is untrusted by construction. A negated entry
/// (`!…`) ADMITS nothing and never counts; first-party local builtins
/// (`nika:read` · `nika:write` · …) are not ingress.
///
/// THE shared predicate — the check's trifecta legs + content-taint
/// pass AND the runtime's agent-whitelist classification all read THIS
/// one, so the check≡run drift class is dead by construction (F-O1
/// ENGINE.md step 1).
#[must_use]
pub fn tool_grant_admits_ingress(grant: &str) -> bool {
    !grant.starts_with('!') && (grant.starts_with("mcp:") || glob_matches(grant, "nika:fetch"))
}

/// Does an invoked tool id name an untrusted-ingress source (v1)?
/// `nika:fetch` exactly, or any `mcp:*` server — fail-closed UNLESS the
/// caller resolved the catalog's `content_trust` mark for the server
/// (`mcp_content_trusted: true`). The runtime passes `false` (it does
/// not consult the catalog in v1 — the absent-mark default); the check
/// passes its catalog consult.
#[must_use]
pub fn invoke_tool_is_ingress(tool_id: &str, mcp_content_trusted: bool) -> bool {
    tool_id == "nika:fetch" || (tool_id.starts_with("mcp:") && !mcp_content_trusted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lattice_is_two_point_and_untrusted_dominates() {
        let t = Integrity::trusted();
        let u = Integrity::untrusted("fetch_page");
        assert!(t.is_trusted() && !t.is_untrusted());
        assert!(u.is_untrusted() && !u.is_trusted());
        assert_eq!(t.clone().join(t.clone()), t);
        assert_eq!(t.join(u.clone()), u, "trusted ⊔ untrusted = untrusted");
        assert_eq!(
            u.clone().join(Integrity::trusted()),
            u,
            "untrusted ⊔ trusted = untrusted"
        );
    }

    #[test]
    fn the_first_untrusted_witness_wins_deterministically() {
        let a = Integrity::untrusted("dl");
        let b = Integrity::untrusted("page");
        assert_eq!(a.join(b).source(), Some("dl"), "the earliest origin");
        let t = Integrity::trusted();
        assert_eq!(
            t.join(Integrity::untrusted("inputs.q")).source(),
            Some("inputs.q")
        );
        assert_eq!(Integrity::trusted().source(), None);
    }

    #[test]
    fn default_is_trusted_so_old_records_read_clean() {
        assert_eq!(Integrity::default(), Integrity::trusted());
        assert_eq!(Integrity::trusted().as_str(), "trusted");
        assert_eq!(Integrity::untrusted("x").as_str(), "untrusted");
    }

    #[test]
    fn grants_admit_ingress_exactly_like_the_trifecta_table() {
        assert!(tool_grant_admits_ingress("nika:fetch"));
        assert!(
            tool_grant_admits_ingress("nika:*"),
            "a nika:* grant covers fetch"
        );
        assert!(tool_grant_admits_ingress("mcp:browser/*"));
        assert!(
            !tool_grant_admits_ingress("!mcp:x"),
            "a negation admits nothing"
        );
        assert!(!tool_grant_admits_ingress("!nika:fetch"));
        assert!(
            !tool_grant_admits_ingress("nika:read"),
            "first-party local builtins are not ingress"
        );
        assert!(!tool_grant_admits_ingress("nika:connectome/*"));
    }

    #[test]
    fn invoked_tools_are_ingress_fail_closed() {
        assert!(invoke_tool_is_ingress("nika:fetch", false));
        assert!(invoke_tool_is_ingress("mcp:browser/open", false));
        assert!(
            !invoke_tool_is_ingress("mcp:browser/open", true),
            "the catalog's trusted mark clears the server"
        );
        assert!(!invoke_tool_is_ingress("nika:read", false));
        assert!(
            !invoke_tool_is_ingress("mcpish:browser", false),
            "the prefix is exact — `mcp:` only"
        );
    }
}
