// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The lethal-trifecta lane (NEP-0002 · `NIKA-SEC-009`) — needle-thin BY
//! DESIGN (the [`super::policy`] precedent): this module only PROJECTS the
//! workflow (declared boundary · per-task egress capability · gate-ness ·
//! direct parents from the ONE edge derivation); the pure judge lives in
//! `nika-cap` ([`nika_cap::trifecta_violations`]).
//!
//! Projection choices (documented, not silent):
//! - **egress-capable** = an `exec:` task · an `invoke:` whose builtin
//!   carries a net or fs-write effect ([`nika_cap::builtin_effect`] — the
//!   ONE effect table, so this lane and the escape checker cannot drift) ·
//!   an `mcp:` tool call (server-side effects — the `tools:` grant is the
//!   boundary, fail-closed) · an `agent:` loop with a non-empty tools
//!   whitelist. `infer:` is not egress (provider egress rides the media
//!   plane). A child-workflow call is NOT judged here — spec 14's
//!   `NIKA-COMP-002` owns the child's boundary containment.
//! - **blocking human gate** = an `invoke:` of `nika:prompt` with NO
//!   `default:` arg (a default lets the run proceed unattended — the NEP's
//!   Rule-of-Two escape is the HUMAN decision). This is intentionally
//!   stricter than `policy: require.human_gate_before`, which accepts a
//!   `default: false` fail-closed prompt; the trifecta wants consent, not
//!   just a pause shape (NEP-0002 §Specification · v2 refinement candidate).
//! - The lane is gated on a valid DAG (the caller passes the derived
//!   edges plus a topological order) and on a DECLARED `permits:` block —
//!   without one the legs are not decidable as declared, so there is no
//!   claim (skipped, never wrong).

use nika_cap::TrifectaSubject;

use crate::analyzer::Edge;
use crate::raw::{RawAction, RawWorkflow};

/// Judge the trifecta over the derived graph. Empty unless the workflow
/// declares `permits:` AND the trifecta legs all hold AND an egress-capable
/// task escapes gate dominance.
pub(super) fn scan_trifecta(
    wf: &RawWorkflow,
    edges: &[Edge],
    topo_order: &[usize],
) -> Vec<nika_cap::TrifectaViolation> {
    let Some(permits) = wf.permits.as_ref() else {
        return Vec::new();
    };
    let mut uses_fetch = false;
    let mut subjects: Vec<TrifectaSubject> = wf
        .tasks
        .iter()
        .map(|t| {
            let task = &t.value;
            TrifectaSubject::new(
                task.id.value.clone(),
                egress_capable(&task.action, &mut uses_fetch),
                human_gate(&task.action),
            )
        })
        .collect();
    for e in edges {
        if let Some(s) = subjects.get_mut(e.to) {
            s.parents.push(e.from);
        }
    }
    nika_cap::trifecta_violations(&permits.value, uses_fetch, &subjects, topo_order)
}

/// The task's egress capability (see the module doc for the table).
fn egress_capable(action: &RawAction, uses_fetch: &mut bool) -> bool {
    match action {
        RawAction::Exec(_) => true,
        RawAction::Agent(a) => !a.tools.is_empty(),
        RawAction::Infer(_) => false,
        RawAction::Invoke(inv) => {
            let Some(tool) = inv.tool() else {
                // A child-workflow call: spec 14 (COMP-002) owns the child's
                // boundary — this lane does not re-judge it.
                return false;
            };
            let id = tool.value.as_str();
            if id == "nika:fetch" {
                *uses_fetch = true;
            }
            if id.starts_with("mcp:") {
                // Server-side effects — the `tools:` grant is the boundary.
                return true;
            }
            let args = inv.args.as_ref().map(|a| &a.value);
            match nika_cap::builtin_effect(id, args) {
                Some(nika_cap::BuiltinEffect::Net { .. }) => true,
                Some(nika_cap::BuiltinEffect::Fs { writes, .. }) => writes,
                None => false,
            }
        }
    }
}

/// A BLOCKING `invoke: nika:prompt` (no `default:` arg) is the NEP's gate.
fn human_gate(action: &RawAction) -> bool {
    let RawAction::Invoke(inv) = action else {
        return false;
    };
    let Some(tool) = inv.tool() else {
        return false;
    };
    if tool.value != nika_cap::HUMAN_GATE_TOOL {
        return false;
    }
    inv.args
        .as_ref()
        .and_then(|a| a.value.as_object())
        .is_none_or(|o| !o.contains_key("default"))
}

#[cfg(test)]
mod tests {
    use crate::check::check;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn report(yaml: &str) -> crate::check::CheckReport {
        check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    /// The six NEP-0002 conformance cases (the spec-side fixtures under
    /// `conformance/security/` mirror these one-for-one).
    const TRIFECTA: &str = "nika: v1\nworkflow:\n  id: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\", \"nika:write\", \"nika:prompt\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  leak:\n    after: { fetch_page: succeeded }\n    with: { body: \"${{ tasks.fetch_page.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/leak.txt\", content: \"${{ with.body }}\" }\n";

    /// ①∧②∧③ declared, no gate → the diagnostic, once per ungated egress
    /// task, message opening with the NEP's verbatim string.
    #[test]
    fn trifecta_complete_refuse() {
        let r = report(TRIFECTA);
        assert_eq!(
            r.trifecta_findings.len(),
            2,
            "fetch_page (net) + leak (fs write) are both ungated egress: {:?}",
            r.trifecta_findings
        );
        assert!(
            r.trifecta_findings[0]
                .detail
                .starts_with("lethal trifecta complete · human gate required"),
            "{}",
            r.trifecta_findings[0].detail
        );
        assert!(!r.is_clean(), "the lane gates the check");
        let f = r
            .findings
            .iter()
            .find(|f| f.kind == "trifecta")
            .expect("trifecta row in findings[]");
        assert_eq!(f.gate, "TRIFECTA");
        assert_eq!(f.code.as_deref(), Some("NIKA-SEC-009"));
        assert_eq!(
            f.docs_url.as_deref(),
            Some("https://nika.sh/errors/NIKA-SEC-009")
        );
        // The conformance-code surface speaks the same code (one voice).
        let codes: Vec<String> = r
            .extra_conformance_codes()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            codes.iter().filter(|c| *c == "NIKA-SEC-009").count(),
            2,
            "one SEC-009 per finding: {codes:?}"
        );
    }

    /// A blocking `nika:prompt` dominating every egress path → clean.
    #[test]
    fn trifecta_gated_pass() {
        let gated = TRIFECTA.replacen(
            "tasks:\n  fetch_page:",
            "tasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"choice\", message: \"exfiltrate?\", choices: [\"no\", \"yes\"] }\n  fetch_page:\n    after: { ask: succeeded }",
            1,
        );
        let r = report(&gated);
        assert!(
            r.trifecta_findings.is_empty(),
            "the gate dominates fetch_page AND leak: {:?}",
            r.trifecta_findings
        );
    }

    /// Drop each leg in turn → clean (the Rule of Two holds unattended).
    #[test]
    fn two_of_three_pass() {
        // No ① (no fs.read).
        let no_read = TRIFECTA.replacen(
            "  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n",
            "  fs: { write: [\"./out/**\"] }\n",
            1,
        );
        assert!(
            report(&no_read).trifecta_findings.is_empty(),
            "① dropped → clean"
        );
        // No ② (no fetch task, no tools grant) — the write stays, gated
        // by nothing, but with no ingress there is no trifecta.
        let no_ingress = "nika: v1\nworkflow:\n  id: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\ntasks:\n  think:\n    infer: { prompt: \"summarize\", max_tokens: 9 }\n";
        assert!(
            report(no_ingress).trifecta_findings.is_empty(),
            "② dropped → clean"
        );
        // No ③ (no net · workspace-confined writes · no exec) — fetch
        // still pulls untrusted content, but nothing can leave.
        let no_egress = "nika: v1\nworkflow:\n  id: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  tools: [\"nika:fetch\", \"nika:write\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  save:\n    after: { fetch_page: succeeded }\n    with: { body: \"${{ tasks.fetch_page.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/save.txt\", content: \"${{ with.body }}\" }\n";
        assert!(
            report(no_egress).trifecta_findings.is_empty(),
            "③ dropped → clean"
        );
    }

    /// A gate on a SIBLING branch dominates nothing → the diagnostic.
    #[test]
    fn gate_present_not_dominating_refuse() {
        let bypass = TRIFECTA.replacen(
            "tasks:\n  fetch_page:",
            "tasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"anything?\" }\n  fetch_page:",
            1,
        );
        // `ask` is an entry with no downstream edge — fetch_page/leak run
        // on a parallel branch the gate never dominates.
        let r = report(&bypass);
        assert_eq!(
            r.trifecta_findings.len(),
            2,
            "a bypassable gate mitigates nothing: {:?}",
            r.trifecta_findings
        );
    }

    /// A `default:`-carrying prompt is NOT a gate (the run proceeds
    /// unattended — the NEP wants the human decision, not the fallback).
    #[test]
    fn a_defaulted_prompt_is_not_blocking() {
        let defaulted = TRIFECTA.replacen(
            "tasks:\n  fetch_page:",
            "tasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"ok?\", default: true }\n  fetch_page:\n    after: { ask: succeeded }",
            1,
        );
        let r = report(&defaulted);
        assert_eq!(
            r.trifecta_findings.len(),
            2,
            "default: true answers without a human: {:?}",
            r.trifecta_findings
        );
    }

    /// No `permits:` block → the legs are not decidable as declared → the
    /// lane is inert (the default-deny/floor lanes own that world). The
    /// fixture is conformance-clean so the test measures the LANE, not
    /// the broken-DAG skip.
    #[test]
    fn no_declared_boundary_no_claim() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  leak:\n    after: { fetch_page: succeeded }\n    exec: { command: [\"echo\", \"x\"] }\n",
        );
        assert!(
            r.conformance.is_empty(),
            "fixture must be conformance-clean: {:?}",
            r.conformance
        );
        assert!(r.trifecta_findings.is_empty(), "{:?}", r.trifecta_findings);
    }

    /// An unanalyzable DAG yields NO claim (the IFC/policy gating
    /// precedent: skipped, never wrong).
    #[test]
    fn broken_dag_skips_the_lane() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npermits:\n  fs: { read: [\"./inbox/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  act:\n    after: { ghost: succeeded }\n    exec: { command: [\"echo\", \"x\"] }\n",
        );
        assert!(!r.conformance.is_empty());
        assert!(r.trifecta_findings.is_empty(), "{:?}", r.trifecta_findings);
    }
}
