// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The MODELS rung of the check ladder (#320) + the pricing preflight
//! (#213) + the four layered verdicts (ADR-123) — the judges the `check`
//! verb folds, hosted HERE so every door reaches the same ones: the CLI
//! verb re-exports them, and the MCP oracle (which reaches the host,
//! never the CLI) folds the same layers (One Door parity). The
//! finding TYPES live beside their renderer (`nika_display::check_render`).

use nika_display::check_render::{ModelFinding, ModelsAudit, VerdictLayers};
use nika_providers::ExecutionAccessPlan;
use nika_providers::resolve_access::AccessRefusal;
use nika_schema::raw::RawWorkflow;
use nika_types::access::AccessPlan;

/// The admission-time access decision per statically-known model
/// (D-2026-08-04-N1 · P2.5) — the SAME frozen plan the run executes
/// (One Door · wave 1: [`crate::access::resolve_plan`] over this
/// machine's probe rows, verb-eligibility included — an ACP-only seat
/// never serves an `infer:` lane here either). Advisory: `clean` and
/// the exit codes never read it.
#[must_use]
pub fn access_decisions(
    wf: &RawWorkflow,
    report: &nika_check::CheckReport,
) -> Vec<(String, Result<AccessPlan, AccessRefusal>)> {
    crate::access::resolve_plan(wf, report, None, None).into_decisions()
}

/// The R-2 boot-manifest access stamps (P3 B5): `access_pin` verbatim
/// plus `access_plan`, the per-model admission decision as ONE compact
/// JSON text — PROJECTED from the frozen [`ExecutionAccessPlan`] the run
/// executes (One Door · wave 1), never re-derived beside it.
#[must_use]
pub fn boot_access_fields(
    plan: &ExecutionAccessPlan,
) -> Vec<(&'static str, nika_types::resource::Value)> {
    nika_service_execution::access::boot_access_fields(plan)
}

/// The CAPACITY laws in this rung's finding shape (wave 2) — the judge
/// is [`nika_check::capacity_findings`]; the fold sites pin it.
#[must_use]
pub fn capacity_findings(wf: &RawWorkflow) -> Vec<ModelFinding> {
    nika_check::capacity_findings(wf)
        .into_iter()
        .map(|f| ModelFinding::new(f.model, vec![f.task], f.why))
        .collect()
}

/// Whether ANY task in this file will dial a model — the ACCESS
/// question's PREMISE, not its answer.
///
/// False and the question is MOOT: no `infer:`/`agent:` task exists, no
/// seat will ever be asked for, and nothing a reader types can change
/// that. True with no static lane and the question is UNANSWERED: a task
/// dials, but its model arrives at run time. Both used to render
/// `access_ready: None`, and the layers line spelled both `○` (a persona wave ·
/// the operations sceptic: `run ready ○` on a builtin-only file that then ran 3/3 green,
/// with `--access mock` unable to move it).
#[must_use]
pub fn dials_a_model(wf: &RawWorkflow) -> bool {
    wf.tasks.iter().any(|task| {
        matches!(
            task.value.action,
            nika_schema::raw::RawAction::Infer(_) | nika_schema::raw::RawAction::Agent(_)
        )
    })
}

/// The four layered verdicts (wave 2) — computed ONCE beside the exit
/// code from the frozen plan and the folded audit; the render and the
/// JSON both project this value.
#[must_use]
pub fn verdict_layers(
    plan: &ExecutionAccessPlan,
    valid: bool,
    capacity: &[ModelFinding],
) -> VerdictLayers {
    verdict_layers_for(plan, valid, capacity, None)
}

/// [`verdict_layers`] with the model-less task the seat must serve
/// (W3-F13): `modelless` names the first `infer:`/`agent:` task whose
/// effective model is empty, including alongside explicit model lanes.
#[must_use]
pub fn verdict_layers_for(
    plan: &ExecutionAccessPlan,
    valid: bool,
    capacity: &[ModelFinding],
    modelless: Option<&str>,
) -> VerdictLayers {
    let mut lines = Vec::new();
    let mut blockers = Vec::new();
    // W3-F1 · the pin's own verdict (a seat whose binary this machine
    // lacks · an unsatisfied pin) is a blocker before any lane is read.
    if let (Some(pin), Some(refusal)) = (&plan.pin, &plan.pin_refusal) {
        let line = format!("pin `{pin}` refused · {}", pin_message(refusal));
        blockers.push(format!("access: {line}"));
        lines.push(line);
    }
    for (model, lane) in plan.admitted() {
        let note = match lane.plan.chosen {
            nika_types::access::AccessClass::Harness => "seat present · sign-in judged at run",
            nika_types::access::AccessClass::Api => {
                "key present · not validated (check never dials)"
            }
            nika_types::access::AccessClass::Mock => "mock · never dials · nothing to judge",
            _ => "present · liveness judged at run",
        };
        let others = lane.candidates.saturating_sub(1);
        let tail = if others == 0 {
            String::new()
        } else {
            format!(" · chosen over {others} other path(s)")
        };
        lines.push(format!(
            "{model} → {} ({} · {} · {}) · {note}{tail}",
            lane.plan.access,
            lane.plan.chosen.as_str(),
            lane.plan.billing.as_str(),
            lane.plan.trust.as_str()
        ));
    }
    for (model, refusal) in plan.lanes.iter().filter_map(|(m, v)| match v {
        nika_providers::LaneVerdict::Refused(r) => Some((m, r)),
        _ => None,
    }) {
        let witnesses: Vec<String> = refusal
            .rejected
            .iter()
            .map(nika_types::access::AccessRejection::witness_line)
            .collect();
        let line = if witnesses.is_empty() {
            format!("{model} → no path on this machine")
        } else {
            format!(
                "{model} → no path on this machine · {}",
                witnesses.join(" · ")
            )
        };
        blockers.push(format!("access: {line}"));
        lines.push(line);
    }
    let modelless_ready = if let Some(task) = modelless {
        // W3-F13 · a model-less infer rides a seat or nothing.
        if let Some(seat) = &plan.seat {
            lines.push(format!(
                "`{task}` → {seat} (harness · seat) · pinned · seat present · sign-in judged at run"
            ));
            true
        } else {
            let line = format!(
                "task `{task}` names no model and no seat is pinned · set `model: <provider/name>` or run with `--access <seat>`"
            );
            blockers.push(format!("access: {line}"));
            lines.push(line);
            false
        }
    } else {
        true
    };
    // Every static lane, the pin, and a model-less task's seat requirement
    // must hold together. None of these facts can override another refusal.
    let access_ready =
        if !plan.lanes.is_empty() || modelless.is_some() || plan.pin_refusal.is_some() {
            Some(plan.is_admitted() && modelless_ready)
        } else {
            None
        };
    if let Some(first) = capacity.first() {
        blockers.push(format!("capacity: {} · {}", first.model, first.why));
    }
    let seat_served: Vec<String> = plan
        .admitted()
        .filter(|(_, lane)| lane.plan.chosen == nika_types::access::AccessClass::Harness)
        .map(|(model, _)| model.to_owned())
        .collect();
    VerdictLayers::new(valid, access_ready, lines, capacity.is_empty(), blockers)
        .with_seat_served(seat_served)
}

/// Cross `requirements.models` against the RESOLVER (the runnable
/// provider set, [`nika_providers::CANONICAL_IDS`]) — never the vendor
/// catalog, which advertises providers this binary cannot drive (the
/// azure class: cataloged, unresolvable, green until the run died).
#[must_use]
pub fn unresolvable_models(
    report: &nika_check::CheckReport,
    wf: &nika_schema::raw::RawWorkflow,
) -> ModelsAudit {
    let mut audit = ModelsAudit::new(Vec::new(), 0, 0);
    for m in &report.requirements.models {
        // A TEMPLATED `model:` is not a static fact — its value arrives
        // at run time (`--var`) — but its DECLARED DEFAULT is: a bare
        // `${{ <authority>.<name> }}` whose declaration carries a
        // literal string is judged AS that default, through the ONE
        // shared resolver ([`nika_check::static_literal_of`] — the same
        // fn the cost lane counts `for_each` fan-outs with; a third
        // private copy is how lanes drift). This keeps the rung's teeth
        // on the parameterization pattern the spec recommends BY NAME
        // (08 §H8 · measured 2026-07-29: the fix before this one skipped
        // `${{ const.model }}` wholesale, and the fix before THAT
        // refused it as « a bare model id » on the spec's own fixture,
        // `stdlib/providers/005-valid-parameterized-model`). Anything
        // the resolver cannot answer gets NO claim — skipped, never
        // wrong — and is COUNTED, so the headline says so.
        let (judged, via_default) = if m.model.contains("${{") {
            let Some(default_model) =
                nika_check::static_literal_of(wf, &m.model).and_then(serde_json::Value::as_str)
            else {
                audit.unjudged += 1;
                continue;
            };
            (default_model, true)
        } else {
            (m.model.as_str(), false)
        };
        // The ONE law, shared with the MCP lane (#320 follow-up: the two
        // machine surfaces consult the same fn beside the resolver —
        // they cannot drift apart again).
        if let Some(refusal) = nika_providers::resolve_refusal(judged) {
            // A via-default refusal names BOTH halves: the template
            // the author wrote and the default that was judged.
            let why = if via_default {
                format!("declared default `{judged}` — {}", refusal.why)
            } else {
                refusal.why
            };
            let mut finding = ModelFinding::new(m.model.clone(), m.tasks.clone(), why);
            if let Some(code) = refusal.code {
                finding = finding.with_code(code);
            }
            audit.findings.push(finding);
        } else {
            // B-5's sibling: a resolvable model on a server-backed
            // keyless engine earns the green line's liveness nuance —
            // this rung never dialed the server it names.
            if judged
                .split_once('/')
                .is_some_and(|(provider, _)| nika_providers::server_backed_local(provider))
            {
                audit.local_server += 1;
            }
            if via_default {
                audit.via_default += 1;
            }
        }
        // The sister law (audit UX 2026-07-31): a model that RESOLVES
        // but matches nothing the snapshot prices warns — advisory,
        // never a finding, spoken ONCE.
        if let Some(why) = nika_providers::catalog_warning(judged) {
            audit
                .catalog_warnings
                .push(ModelFinding::new(m.model.clone(), m.tasks.clone(), why));
        }
    }
    audit
}

/// The `infer.thinking` judgments in this rung's finding shape — the
/// judge is [`nika_check::thinking_findings`]; the fold sites pin it.
#[must_use]
pub fn thinking_findings(wf: &nika_schema::raw::RawWorkflow) -> Vec<ModelFinding> {
    nika_check::thinking_findings(wf)
        .into_iter()
        .map(|f| ModelFinding::new(f.model, vec![f.task], f.why))
        .collect()
}

/// The rates the preflight shows BEFORE the first run (#213), priced
/// from the vendored catalog — UNKNOWN is null, never 0.00 (a missing
/// price must look missing), and a model the resolver cannot run is
/// NEVER priced (the table fuzzy-matches: unpriced beats conjured).
/// `snapshot` = the catalog's provenance + counts DERIVED at read time.
#[must_use]
pub fn pricing_section(
    report: &nika_check::CheckReport,
    model_findings: &[ModelFinding],
) -> serde_json::Value {
    let models: Vec<serde_json::Value> = report
        .requirements
        .models
        .iter()
        .map(|m| {
            let resolvable = !model_findings.iter().any(|f| f.model == m.model);
            let priced = resolvable
                .then(|| nika_catalog::find_pricing_for(&m.model))
                .flatten();
            serde_json::json!({
                "model": m.model,
                "input_per_million": priced.map(|p| p.input_per_million),
                "output_per_million": priced.map(|p| p.output_per_million),
            })
        })
        .collect();
    let snap = nika_catalog::pricing_snapshot();
    let rules = nika_catalog::all_pricing();
    let providers: std::collections::BTreeSet<&str> = rules.iter().map(|p| p.provider).collect();
    serde_json::json!({
        "snapshot": {
            "source": snap.source,
            "as_of": snap.as_of,
            "source_sha256_16": snap.source_sha256_16,
            // DERIVED at read time, never embedded (the born-stale law).
            "rules": rules.len(),
            "providers": providers.len(),
        },
        "models": models,
    })
}

/// The words a pin refusal carries (every variant is a message · the
/// enum is `#[non_exhaustive]`, so an unknown shape reads as refused).
fn pin_message(refusal: &nika_providers::resolve_access::PinRefusal) -> &str {
    use nika_providers::resolve_access::PinRefusal;
    match refusal {
        PinRefusal::UnknownToken { message }
        | PinRefusal::PinUnsatisfied { message }
        | PinRefusal::NoPath { message }
        | PinRefusal::Unavailable { message } => message,
        _ => "refused",
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::BTreeMap;

    use nika_providers::resolve_access::PinRefusal;

    use super::*;

    fn plan(
        pin: Option<&str>,
        seat: Option<&str>,
        refusal: Option<PinRefusal>,
    ) -> ExecutionAccessPlan {
        ExecutionAccessPlan::new(
            BTreeMap::new(),
            pin.map(str::to_owned),
            seat.map(str::to_owned),
            refusal,
        )
    }

    /// W3-F13 · an `infer:` with no model and no seat pinned has no path:
    /// ACCESS READY is false, and the blocker names the two fixes.
    #[test]
    fn a_model_less_infer_with_no_seat_is_not_ready() {
        let layers = verdict_layers_for(&plan(None, None, None), true, &[], Some("answer"));
        assert_eq!(layers.access_ready, Some(false));
        assert!(layers.run_ready() == Some(false), "{layers:?}");
        assert!(
            layers.blockers[0].contains("names no model")
                && layers.blockers[0].contains("--access"),
            "{:?}",
            layers.blockers
        );
    }

    /// W3-F13 · the same task with a seat pinned and present is ready;
    /// the line names the seat and what it serves.
    #[test]
    fn a_model_less_infer_on_a_present_seat_is_ready() {
        let layers = verdict_layers_for(
            &plan(Some("codex"), Some("codex"), None),
            true,
            &[],
            Some("answer"),
        );
        assert_eq!(layers.access_ready, Some(true));
        assert!(layers.blockers.is_empty(), "{:?}", layers.blockers);
        assert!(
            layers.access_lines[0].contains("`answer` → codex")
                && layers.access_lines[0].contains("seat present"),
            "{:?}",
            layers.access_lines
        );
    }

    /// An admitted lane and the model-less requirement are independent
    /// inputs to the fold: the lane cannot hide the missing path.
    #[test]
    fn an_admitted_lane_does_not_hide_a_modelless_task() {
        let plan = nika_providers::resolve_execution_plan(
            &[nika_providers::ModelNeed::new("mock/echo", true, false)],
            &[],
            None,
        );
        assert!(plan.is_admitted());
        assert!(!plan.lanes.is_empty());
        let layers = verdict_layers_for(&plan, true, &[], Some("answer"));
        assert_eq!(layers.access_ready, Some(false));
        assert_eq!(layers.run_ready(), Some(false));
        assert!(
            layers
                .access_lines
                .iter()
                .any(|line| line.starts_with("mock/echo →"))
        );
        assert!(
            layers
                .blockers
                .iter()
                .any(|line| line.contains("task `answer` names no model"))
        );
    }

    /// Even a supplied seat cannot turn a refused plan green. These pure
    /// projection fixtures deliberately keep the seat beside each refusal.
    #[test]
    fn a_modelless_seat_does_not_clear_pin_or_lane_refusals() {
        let pin_refused = plan(
            Some("codex"),
            Some("codex"),
            Some(PinRefusal::Unavailable {
                message: "Codex is not installed".to_owned(),
            }),
        );
        let mut lane_refused = plan(Some("codex"), Some("codex"), None);
        lane_refused.lanes.insert(
            "unavailable-provider/model".to_owned(),
            nika_providers::LaneVerdict::Refused(AccessRefusal::new(
                "unavailable-provider/model",
                "unavailable-provider",
                Vec::new(),
            )),
        );
        for plan in [pin_refused, lane_refused] {
            let layers = verdict_layers_for(&plan, true, &[], Some("answer"));
            assert_eq!(layers.access_ready, Some(false));
            assert_eq!(layers.run_ready(), Some(false));
            assert!(!layers.blockers.is_empty());
        }
    }

    /// W3-F1 · a refused pin (a seat this machine lacks) is a blocker
    /// before any lane is read, with the refusal's own words.
    #[test]
    fn a_refused_pin_is_a_blocker() {
        let refusal = PinRefusal::Unavailable {
            message: "Codex is not installed".to_owned(),
        };
        let layers = verdict_layers_for(
            &plan(Some("codex"), None, Some(refusal)),
            true,
            &[],
            Some("answer"),
        );
        assert_eq!(layers.access_ready, Some(false));
        assert!(
            layers.blockers[0].contains("pin `codex` refused")
                && layers.blockers[0].contains("not installed"),
            "{:?}",
            layers.blockers
        );
    }

    /// No infer at all and no lanes: nothing to judge, and the layer says
    /// so with `None`, never a false red.
    #[test]
    fn no_model_and_no_infer_leaves_access_unjudged() {
        let layers = verdict_layers_for(&plan(None, None, None), true, &[], None);
        assert_eq!(layers.access_ready, None);
        assert!(layers.blockers.is_empty());
    }
}
