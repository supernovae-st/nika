// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The TRIFECTA rung, descended from [`super`] at the 1500-LOC file cap
//! (ADR-110's wall, one cap down: one architectural unit, two members).
//!
//! It lives alone because its tick is the only one on the card that is
//! DERIVED from another lane's findings rather than from its own, and
//! that reasoning wants room to be stated.

use nika_check::CheckReport;
use nika_schema::raw::RawWorkflow;

use super::{Theme, section_list, section_or_skip};

/// TRIFECTA rung (NEP-0002) · only when a boundary is declared — without
/// `permits:` the legs are not decidable as declared and the lane is
/// inert (the default-deny/floor lanes own that world).
pub(super) fn trifecta_rung(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if wf.permits.is_some() {
        // The tick is DERIVED, never asserted. This lane clears a sink
        // when a blocking `nika:prompt` dominates it — a purely
        // syntactic credit (`human_gate`: an invoke of the tool with no
        // `default:` key, one task, no route analysis). The consent lane
        // runs the refusal-substitution walk and can PROVE that same
        // gate lets the effect fire on 'no'.
        //
        // Measured 2026-08-20 on 0.111.0, one file, one card ·
        //
        //   ✔ TRIFECTA no lethal trifecta … without a human gate
        //   ✖ CONSENT  [NIKA-SEC-014] task `leak` … fires on 'no'
        //
        // A control run (the same file with the prompt deleted) fires
        // SEC-009, so the trifecta was COMPLETE and the tick was bought
        // entirely by the gate the next line refutes.
        //
        // Where a credit is refuted the lane says so and hands the
        // reader to the code that owns the repair. It does NOT raise a
        // second finding: the consent row already names this defect and
        // teaches its fix, and one defect wearing two codes is the
        // double-count the mootness law forbids.
        // No type is named here on purpose: spelling `nika_cap::…` would
        // add a dependency EDGE, and an edge re-renders the public
        // surface of crates that gained no item.
        let refuted = report.trifecta_mitigations.iter().find(|m| {
            report
                .consent_findings
                .iter()
                .any(|c| c.prompt == m.gate && c.sink == m.sink)
        });
        if report.trifecta_findings.is_empty()
            && let Some(m) = refuted
        {
            section_list(
                out,
                t,
                "TRIFECTA",
                "the trifecta is complete and its only mitigation is refused below",
                vec![format!(
                    "gate `{}` dominates `{}` but CONSENT proves the effect fires on \
                     'no' — this lane credits a blocking prompt, not a gate that \
                     closes · repair it at NIKA-SEC-014 below",
                    m.gate, m.sink
                )],
            );
            return;
        }
        // "over the declared permits:" is the honest preposition. The
        // legs are read off the DECLARATION, not the body — leg ① is
        // literally `permits.fs.read` non-empty (`nika-cap/trifecta.rs`
        // `legs()`), and the grant halves of ② and ③ are the `tools` and
        // `net.http`/`fs.write` blocks. Only the ingress WITNESS comes
        // from realized flow. So the judge answers "is this DECLARED
        // boundary a lethal trifecta", and a body that under-declares was
        // (F8) able to shorten a leg. DAG-gated at the source too.
        section_or_skip(
            out,
            report,
            t,
            "TRIFECTA",
            "no lethal trifecta over the declared permits: without a human gate",
            report
                .trifecta_findings
                .iter()
                .map(|f| format!("[NIKA-SEC-009] {}", f.detail))
                .collect(),
        );
    }
}
