// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ORDER and LIFT rungs of `nika check` — the two spec-10 laws whose
//! findings the wire stamped (`NIKA-SEC-015` · `NIKA-AUTH-011`) and the
//! human lane never printed (measured 2026-08-19 on the published
//! 0.109.2 · every row green · `✖ findings above` pointing at nothing).
//! Split from `check_render.rs` at the 1500-line file wall, the JOURNEY
//! precedent — same seams, the section helpers stay where they were.

use nika_check::CheckReport;
use nika_schema::raw::RawWorkflow;

use crate::check_render::{section_list, section_or_skip};
use crate::theme::Theme;

/// ORDER rung (spec 10 · the unconditional order law · NIKA-SEC-015) ·
/// always present, like WRITES — a universal static law reads as one.
/// DAG-gated at the source (`lib.rs` runs `scan_order` only while
/// conformance is clean), so the SKIP is announced, never a verdict the
/// lane did not compute.
///
/// Measured 2026-08-19 on the published 0.109.2 · a beat workflow whose
/// `exec: sleep 3` etiquette gap sat one `after:` edge downstream of a
/// `nika:fetch` exited rc=2 with EVERY row green and `✖ findings above`
/// pointing at nothing — only `--json` carried the finding, because this
/// file read no `order_findings`. The mute-diagnostic class the PERMITS
/// panel names, one lane over: a law that refuses in the wire and says
/// nothing in the human lane is a law the operator cannot repair.
pub(crate) fn order_rung(out: &mut String, report: &CheckReport, t: Theme) {
    section_or_skip(
        out,
        report,
        t,
        "ORDER",
        "no exec: sits downstream of a net-effecting task · unauthored content never reaches a shell",
        report
            .order_findings
            .iter()
            .map(|f| format!("[{}] {}", nika_check::OrderFinding::WIRE_CODE, f.detail))
            .collect(),
    );
}

/// RETRY rung (#1371 · the effect-safe retry law · NIKA-SEC-016) · always
/// present, like WRITES/EXEC — a universal static law reads as one. The
/// scan is DAG-independent (a per-task syntactic judgment), so the ✔ is
/// never a gated skip: a declared `retry:` on a provably keyless,
/// effect-replaying `nika:fetch` is refused, judged by the SAME predicate
/// the runtime's transient classification reads (`nika_types::net` — one
/// definition, check ≡ run). A templated method or an undecidable
/// `headers:` shape makes no claim — the runtime belt re-judges the
/// resolved call, and the green line says so.
pub(crate) fn retry_rung(out: &mut String, report: &CheckReport, t: Theme) {
    section_list(
        out,
        t,
        "RETRY",
        "no `retry:` on a keyless mutating `nika:fetch` · a templated method/headers is the RUN's verdict",
        report
            .retry_safety_findings
            .iter()
            .map(|f| {
                format!(
                    "[{}] task `{}` · {} · fix: {}",
                    f.wire_code(),
                    f.task,
                    f.detail,
                    f.fix
                )
            })
            .collect(),
    );
}

/// LIFT rung (spec 10 rule 6 · the authored doors · NIKA-AUTH-011) ·
/// only when a task declares `lift:` — a file with no door renders
/// unchanged. Not DAG-gated at the source (`scan_idle_doors` reads the
/// tasks, not the order), so a plain section: green when every door
/// guards a law that fires, one code-first row per idle door. Same
/// 2026-08-19 gap as ORDER: the wire stamped `NIKA-AUTH-011`, the human
/// lane had no row to print it in.
///
/// The green tick is AUTH-011, not the Rule of Two. `lift: taint` opens
/// permit-parameterization taint; SEC-009's only door is a blocking
/// `nika:prompt`. Measured on #1065: a valid taint door next to a
/// complete trifecta read as `✔ LIFT` beside `✖ TRIFECTA [NIKA-SEC-009]`,
/// and two readers independently concluded the hatch was inert. When the
/// trifecta lane has findings, the green line names the other law so the
/// tick cannot stand as a contradiction. The lane stays green: AUTH-011
/// is satisfied.
pub(crate) fn lift_rung(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if !wf.tasks.iter().any(|task| !task.value.lift.is_empty()) {
        return;
    }
    // Two statics, not a format, so the connecting clause is a second
    // sentence the catching test can grep — not a row that would redden
    // a door AUTH-011 already cleared.
    let ok_msg = if report.trifecta_findings.is_empty() {
        "every authored door guards a law that fires · a taint entry's binding reaches the task · a data-as-code entry meets a code-bearing fetch"
    } else {
        "every authored door guards a law that fires · a taint entry's binding reaches the task · a data-as-code entry meets a code-bearing fetch. This door does not open SEC-009; the Rule of Two wants a blocking nika:prompt"
    };
    section_list(
        out,
        t,
        "LIFT",
        ok_msg,
        report
            .lift_findings
            .iter()
            .map(|f| {
                format!(
                    "[{}] {} · fix: {}",
                    nika_check::LiftFinding::WIRE_CODE,
                    f.detail,
                    f.fix
                )
            })
            .collect(),
    );
}
