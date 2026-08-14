// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The AFFIRMATIVE permits statement — what this workflow is ALLOWED to
//! touch, stated positively on every report (#346).
//!
//! Violations were always first-class (`capability_escapes` ·
//! `secret_leaks` · `secret_egresses`) but a GREEN check said nothing a
//! consumer could render: the nika-action comment derived tool lists
//! from graph labels (lossy) because the report carried no grants. This
//! module states the contract both ways — the boundary in force (the
//! declared `permits:` block, or the F-O8 ZERO boundary when none is
//! declared · « absent = zero authority ») AND the tightest boundary the
//! body statically needs (the SAME derivation `nika check
//! --infer-permits` prints — one inference, two consumers).

use serde::Serialize;

use super::permits_infer;
use nika_schema::raw::RawWorkflow;
use nika_schema::types::Permits;

/// Where the effective boundary comes from. `lowercase` on the wire.
/// Additive: `report_version` stays 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PermitsSource {
    /// A `permits:` block is declared — the checker fits every effect
    /// against it and the runtime enforces it (default-deny).
    Declared,
    /// No boundary declared — F-O8 « absent = zero authority »: the
    /// boundary in force is the EMPTY set (every effect refused · the
    /// always-on floors — masking · taint · SSRF · blocklist — stay on
    /// top, independent). The pre-F-O8 « engine floor = unconfined »
    /// reading is retired.
    Absent,
}

/// The effective grants + the derived need, on every report — consumers
/// render the positive contract on a green check instead of
/// reconstructing it lossily; violations stay in `capability_escapes` /
/// `secret_*`. Additive: `report_version` stays 1.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct EffectivePermits {
    /// Where the boundary in force comes from.
    pub source: PermitsSource,
    /// The declared `permits:` block, spec-shaped (`None` on
    /// [`PermitsSource::Absent`]).
    pub declared: Option<Permits>,
    /// The TIGHTEST boundary the body statically needs — the
    /// `--infer-permits` derivation (exec programs · net hosts · fs
    /// paths · tools), independent of what is declared.
    pub needed: Permits,
    /// Effects too dynamic to pin statically (a computed host, a
    /// templated path) — the inference's honesty notes, verbatim — plus
    /// the loopback-declassification statements (#395): each exact
    /// loopback literal in the declared `net.http`, stated affirmatively
    /// so a green check TEACHES that the SSRF floor is cleared for it.
    pub notes: Vec<String>,
}

/// State the affirmative contract for one workflow (total — a
/// half-broken workflow still states what it declares and needs).
pub(super) fn collect(wf: &RawWorkflow) -> EffectivePermits {
    let inferred = permits_infer::infer(wf);
    let declared = wf.permits.as_ref().map(|p| p.value.clone());
    let mut notes = inferred.notes;
    // The loopback declassification, stated (#395 · ADR-092 egress
    // precedent): the exact literal is the author's explicit act — the
    // note is the informational surface that replaces the pre-#395
    // dead-grant/floor findings for these entries.
    if let Some(net) = declared.as_ref().and_then(|p| p.net.as_ref()) {
        for entry in &net.http {
            if nika_types::net::is_exact_loopback_literal(entry) {
                notes.push(format!(
                    "permits.net.http entry `{entry}` is an exact loopback literal — \
                     the explicit permit clears the always-on SSRF floor \
                     (NIKA-SEC-005) for that host only"
                ));
            }
        }
    }
    EffectivePermits {
        source: if declared.is_some() {
            PermitsSource::Declared
        } else {
            PermitsSource::Absent
        },
        declared,
        needed: inferred.permits,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn wf(yaml: &str) -> RawWorkflow {
        parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses")
    }

    /// No `permits:` block → the F-O8 zero boundary, stated as such:
    /// `declared` is null on the wire and the derived need still names
    /// what the body touches (the renderer's affirmative line — and the
    /// NIKA-AUTH-006 repair surface when the need is non-empty).
    #[test]
    fn absent_states_the_zero_and_still_derives_the_need() {
        let report = collect(&wf(
            "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.github.com/repos\" } }\n",
        ));
        assert_eq!(report.source, PermitsSource::Absent);
        assert!(report.declared.is_none());
        let json = serde_json::to_value(&report).expect("serializes");
        assert_eq!(json["source"], "absent");
        assert_eq!(json["declared"], serde_json::Value::Null);
        let hosts = json["needed"]["net"]["http"]
            .as_array()
            .expect("derived hosts");
        assert!(
            hosts.iter().any(|h| h == "api.github.com"),
            "the need names the host: {json}"
        );
    }

    /// A declared block rides the report spec-shaped, and the derived
    /// need stays independent of it (declared wide · needed tight).
    #[test]
    fn declared_boundary_rides_beside_the_derived_need() {
        let report = collect(&wf(
            "nika: w\npermits: { exec: false, net: { http: [\"api.github.com\", \"example.com\"] } }\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.github.com/repos\" } }\n",
        ));
        assert_eq!(report.source, PermitsSource::Declared);
        let json = serde_json::to_value(&report).expect("serializes");
        assert_eq!(json["source"], "declared");
        let declared_hosts = json["declared"]["net"]["http"]
            .as_array()
            .expect("declared hosts");
        assert_eq!(declared_hosts.len(), 2, "the grant is stated as written");
        let needed_hosts = json["needed"]["net"]["http"]
            .as_array()
            .expect("needed hosts");
        assert_eq!(needed_hosts.len(), 1, "the need stays tight: {json}");
    }

    /// A dynamic effect (templated URL) lands in `notes` — stated
    /// honesty, never a silently-missing grant.
    #[test]
    fn dynamic_effects_surface_as_notes() {
        let report = collect(&wf(
            "nika: w\ninputs:\n  host: { type: string, default: \"https://x.dev\" }\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"${{ inputs.host }}\" } }\n",
        ));
        assert!(
            !report.notes.is_empty(),
            "the un-pinnable fetch is named: {:?}",
            report.notes
        );
    }

    /// An exact loopback literal in the declared boundary is STATED
    /// (#395): the informational note names the entry and the clearing —
    /// and an ordinary public grant stays silent.
    #[test]
    fn loopback_declassification_is_stated_as_a_note() {
        let report = collect(&wf(
            "nika: w\npermits: { net: { http: [\"127.0.0.1\", \"api.x.com\"] }, tools: [\"nika:fetch\"], exec: false }\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://127.0.0.1:8971/price.json\" } }\n",
        ));
        let loopback: Vec<&String> = report
            .notes
            .iter()
            .filter(|n| n.contains("exact loopback literal"))
            .collect();
        assert_eq!(loopback.len(), 1, "one note per qualifying entry");
        assert!(loopback[0].contains("`127.0.0.1`"), "{}", loopback[0]);
        assert!(loopback[0].contains("NIKA-SEC-005"), "{}", loopback[0]);
        assert!(
            !loopback[0].contains("api.x.com"),
            "public grants are not loopback-stated"
        );
    }
}
