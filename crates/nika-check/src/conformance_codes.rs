// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The report's EXTRA-conformance codes — every check-only invalidating
//! surface, projected as the spec codes the conformance runner matches
//! on (a `SchemaError`'s own code is the analyze tier; none of those
//! come through here).
//!
//! Split out of `lib.rs` at the 1500-LOC wall the day the unconditional
//! order law joined the list. One subject, one file: nothing here judges
//! anything, it only names what the judges already found.

use nika_schema::error::spec_code::{SpecCategory, SpecCode};

use crate::{CheckReport, PermitTaintKind, reach};

/// Every extra-conformance code this report carries.
pub(crate) fn of(report: &CheckReport) -> Vec<SpecCode> {
    let builtin = SpecCode::new("BUILTIN", 1, SpecCategory::ValidationError);
    let mut codes = Vec::new();
    codes.extend(report.capability_escapes.iter().map(|e| {
        // Floor escapes carry the code the run would emit (SEC-005 ·
        // the always-on SSRF floor); an effect judged against the
        // F-O8 zero boundary (no `permits:` declared) is AUTH-006;
        // declared-boundary escapes stay SEC-004.
        if e.floor {
            SpecCode::new("SEC", 5, SpecCategory::SecurityError)
        } else if e.undeclared {
            SpecCode::new("AUTH", 6, SpecCategory::SecurityError)
        } else {
            SpecCode::new("SEC", 4, SpecCategory::SecurityError)
        }
    }));
    // The argv exec floor (#605): the code the run would stamp on the
    // same argv's `ShellError::Blocked` — SEC-001, check ≡ run.
    codes.extend(
        report
            .exec_floor_findings
            .iter()
            .map(|_| SpecCode::new("SEC", 1, SpecCategory::SecurityError)),
    );
    // The permits-block taint findings: the finding's own kind maps to
    // its ONE wire code (NEP-0004 law 1 → AUTH-007 · law 2 → AUTH-008 ·
    // NEP-0005 law 3 env dead grant → AUTH-009 · F-P5 net wildcard →
    // AUTH-010 · all check-time security refusals).
    codes.extend(report.permit_taints.iter().map(|t| match t.kind {
        PermitTaintKind::BoundInterpolated => SpecCode::new("AUTH", 7, SpecCategory::SecurityError),
        PermitTaintKind::ArgEscapes => SpecCode::new("AUTH", 8, SpecCategory::SecurityError),
        PermitTaintKind::EnvDeadGrant => SpecCode::new("AUTH", 9, SpecCategory::SecurityError),
        PermitTaintKind::NetWildcard => SpecCode::new("AUTH", 10, SpecCategory::SecurityError),
    }));
    // The data-as-code sink (NEP-0006) → NIKA-SEC-008.
    let sink_code = SpecCode::new("SEC", 8, SpecCategory::SecurityError);
    codes.extend(report.sink_findings.iter().map(|_| sink_code));
    extend_law_codes(report, &mut codes);
    codes.extend(report.unknown_tools.iter().map(|t| {
        if t.tool.starts_with("mcp:") {
            SpecCode::new("INVOKE", 1, SpecCategory::ValidationError)
        } else {
            builtin
        }
    }));
    codes.extend(report.unknown_args.iter().map(|_| builtin));
    codes.extend(report.missing_args.iter().map(|_| builtin));
    // Gate liveness (03 §static liveness · check-only, reach.rs):
    // DAG-006 statically dead task · DAG-007 out-of-vocabulary literal.
    codes.extend(report.gate_findings.iter().map(|g| match g.kind {
        reach::GateFindingKind::DeadTask => SpecCode::new("DAG", 6, SpecCategory::ValidationError),
        reach::GateFindingKind::BadStatusLiteral => {
            SpecCode::new("DAG", 7, SpecCategory::ValidationError)
        }
    }));
    // F-P3 · the run: declaration contradicted by the body — the
    // dedicated NIKA-PARSE-028 mint (NEP-0010 · the 87f764a pack).
    codes.extend(
        report
            .run_decl_findings
            .iter()
            .map(|_| SpecCode::new("PARSE", 28, SpecCategory::ValidationError)),
    );
    // F-P15 · the write-write law (NEP-0014 law 1) — the security
    // class: an effect overlap the boundary never sanctioned.
    codes.extend(
        report
            .write_conflicts
            .iter()
            .map(|_| SpecCode::new("SEC", 12, SpecCategory::SecurityError)),
    );
    // Composition lane (spec 14): COMP-002 is the security law
    // (child boundary ⊄ parent); 001/003/004 are validation.
    codes.extend(report.composition.iter().map(|f| match f.code {
        "NIKA-COMP-002" => SpecCode::new("COMP", 2, SpecCategory::SecurityError),
        "NIKA-COMP-003" => SpecCode::new("COMP", 3, SpecCategory::ValidationError),
        "NIKA-COMP-004" => SpecCode::new("COMP", 4, SpecCategory::ValidationError),
        _ => SpecCode::new("COMP", 1, SpecCategory::ValidationError),
    }));
    codes
}

/// The named laws that judge one body in their own right — the
/// lethal trifecta, the affirmative-consent law, the unconditional
/// order law, and the effect-safe retry law. They are one family: a
/// security refusal no permit buys off.
fn extend_law_codes(report: &CheckReport, codes: &mut Vec<SpecCode>) {
    // Lethal trifecta (NEP-0002) → NIKA-SEC-009.
    let trifecta_code = SpecCode::new("SEC", 9, SpecCategory::SecurityError);
    codes.extend(report.trifecta_findings.iter().map(|_| trifecta_code));
    // The affirmative-consent law (NEP-0020) → NIKA-SEC-014.
    codes.extend(
        report
            .consent_findings
            .iter()
            .map(|_| SpecCode::new("SEC", 14, SpecCategory::SecurityError)),
    );
    // The unconditional order law (spec 10) → NIKA-SEC-015.
    codes.extend(
        report
            .order_findings
            .iter()
            .map(|_| SpecCode::new("SEC", 15, SpecCategory::SecurityError)),
    );
    // The effect-safe retry law (#1371) → NIKA-SEC-016.
    codes.extend(
        report
            .retry_safety_findings
            .iter()
            .map(|_| SpecCode::new("SEC", 16, SpecCategory::SecurityError)),
    );
    // The authored doors rule 6 (spec 10) → NIKA-AUTH-011. A validation
    // error, not a security one: the file is not dangerous, it is
    // MISLEADING — and review reads the difference.
    codes.extend(
        report
            .lift_findings
            .iter()
            .map(|_| SpecCode::new("AUTH", 11, SpecCategory::ValidationError)),
    );
}
