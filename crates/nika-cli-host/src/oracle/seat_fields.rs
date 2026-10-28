// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a seat cannot honour, said by `check` before the run (spec 02 §Subscription harness
//! access: « a non-executing preview … MUST apply the same access admission meet as a live
//! run »). Judged per task from the route's DECLARED capabilities, never by dialing it:
//!
//! - an `infer:` over ACP carries the prompt, system, model, native effort, a `schema:` the
//!   verb validates and `vision:` files (as image blocks, on a route whose adapter reads
//!   images) — `temperature`, `max_tokens`, an enabled `thinking` budget and a `vision:` URL
//!   refuse at run (NIKA-432), so they block RUN READY here. A seat with no audited ACP
//!   one-shot is the access line's refusal, judged there, never here;
//! - an `infer:` on a seat's direct one-shot (no `protocol: acp`) carries the prompt, system,
//!   model and a `schema:` on its command, never an image: a `vision:` input refuses at run
//!   (NIKA-432), so it blocks RUN READY here. A seat with no direct one-shot is again the
//!   access line's refusal;
//! - an `agent:` on a seat refuses a `tools:` whitelist (NIKA-465): the harness runs its own;
//! - `images:` is a seat's field: an `agent:` the plan does not seat refuses it (NIKA-465).
//!
//! The tasks a seat serves are also returned: their `[cost]` hints, which push
//! `max_tokens`/`max_tokens_total`, would make the run refuse or price the seat as an API.

use std::collections::BTreeSet;

use nika_check::CheckReport;
use nika_providers::ExecutionAccessPlan;
use nika_schema::raw::{RawAction, RawInferAction, RawWorkflow, VisionInput};
use nika_types::access::{AccessProtocol, HarnessRuntime};

/// The seat judgement of one verdict.
#[derive(Debug, Default)]
pub(super) struct SeatFields {
    /// RUN READY blockers, one per task that sets what its seat cannot honour.
    pub(super) blockers: Vec<String>,
    /// The `infer:`/`agent:` tasks a seat serves, in file order.
    pub(super) seated: BTreeSet<String>,
}

/// Judge every task of `wf` against the seat the plan (or `run.access.via`) gives it.
pub(super) fn judge(
    wf: &RawWorkflow,
    report: &CheckReport,
    plan: &ExecutionAccessPlan,
) -> SeatFields {
    let access = (wf.run.as_ref()).and_then(|run| run.value.access.as_ref());
    let declared = access
        .and_then(|access| access.via.as_deref())
        .filter(|via| HarnessRuntime::lookup(via).is_some());
    let acp = access.and_then(|access| access.protocol) == Some(AccessProtocol::Acp);
    let mut out = SeatFields::default();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        let model = (report.requirements.models.iter())
            .find(|req| req.tasks.iter().any(|t| t == id))
            .map_or("", |req| req.model.as_str());
        let seat = plan.seat_for(model).or(declared);
        match (&task.value.action, seat) {
            (RawAction::Infer(infer), Some(seat)) => {
                out.seated.insert(id.to_owned());
                if !acp {
                    out.blockers.extend(direct_vision(id, seat, infer));
                    continue;
                }
                let Some(reads_images) = one_shot(seat) else {
                    continue;
                };
                let mut named: Vec<&str> = Vec::new();
                if infer.temperature.is_some() {
                    named.push("`temperature`");
                }
                if infer.max_tokens.is_some() {
                    named.push("`max_tokens`");
                }
                // The run's own predicate: an enabled budget is what reaches the session.
                if (infer.thinking.as_ref())
                    .is_some_and(|t| t.value.enabled && t.value.budget_tokens.is_some())
                {
                    named.push("`thinking`");
                }
                if infer
                    .vision
                    .iter()
                    .any(|v| matches!(v.value, VisionInput::Url { .. }))
                {
                    named.push("a `vision:` URL (Nika does not fetch it: use a local file)");
                } else if !infer.vision.is_empty() && !reads_images {
                    named.push("`vision:` (its adapter advertises no image prompts)");
                }
                if !named.is_empty() {
                    out.blockers.push(format!(
                        "seat · `{id}` is an infer: on `{seat}` over ACP, which carries the \
                         prompt, system, model, effort, a `schema:` Nika validates and \
                         `vision:` files, not {} — the run refuses it before anything is sent \
                         (NIKA-432): drop it or declare an API route",
                        named.join(" · ")
                    ));
                }
            }
            (RawAction::Agent(agent), Some(seat)) => {
                out.seated.insert(id.to_owned());
                if !agent.tools.is_empty() {
                    out.blockers.push(format!(
                        "seat · `{id}` is an agent: on `{seat}`, which runs its own tools: a \
                         `tools:` whitelist cannot be enforced there and the run refuses it \
                         (NIKA-465) — drop `tools:` or run the task on an API or local model"
                    ));
                }
            }
            (RawAction::Agent(agent), None) if agent.images.is_some() => {
                out.blockers.push(format!(
                    "seat · `{id}` declares `images:`, which collects what an agent \
                     application delivers on its own session, and no seat serves it here \
                     (the run refuses it, NIKA-465) — declare `run.access: {{ via: codex, \
                     protocol: acp }}` or drop `images:`"
                ));
            }
            _ => {}
        }
    }
    out
}

/// The blocker of an `infer:` that names `vision:` on `seat`'s direct one-shot, whose command
/// never carries an image: the run refuses it before anything is sent, as it would otherwise
/// answer without the images. The remedy names the seat's ACP one-shot where it reads images.
fn direct_vision(id: &str, seat: &str, infer: &RawInferAction) -> Option<String> {
    if infer.vision.is_empty() || !direct_one_shot(seat) {
        return None;
    }
    let remedy = if one_shot(seat) == Some(true) {
        format!(
            "declare `run.access: {{ via: {seat}, protocol: acp }}` (its ACP one-shot carries \
             `vision:` files), drop `vision:` or declare an API route"
        )
    } else {
        "drop `vision:` or declare an API route".to_owned()
    };
    Some(format!(
        "seat · `{id}` is an infer: on `{seat}`'s direct one-shot, whose command carries the \
         prompt, system, model and a `schema:`, never an image: the model would not see its \
         `vision:`, so the run refuses it before anything is sent (NIKA-432) — {remedy}"
    ))
}

/// Whether `seat` has an infer-grade direct one-shot (`codex exec`, `claude -p` …): the
/// static row its meet reads, never a spawn.
#[cfg(feature = "access-harness")]
fn direct_one_shot(seat: &str) -> bool {
    nika_harness::meet_infer_grade(seat, nika_harness::StructuredOutputGrade::Text).is_ok()
}

/// Without the adapters there is no direct one-shot either: the access line refuses it.
#[cfg(not(feature = "access-harness"))]
const fn direct_one_shot(_seat: &str) -> bool {
    false
}

/// `seat`'s audited ACP one-shot, as whether it reads images (its measured `initialize`):
/// `None` when it has none, which the access line refuses before anything is sent.
#[cfg(feature = "access-harness")]
fn one_shot(seat: &str) -> Option<bool> {
    nika_harness::meet_acp_one_shot(seat)
        .ok()
        .map(nika_harness::AcpOneShot::image_prompts)
}

/// Without the adapters there is no ACP one-shot: the access line already refuses it.
#[cfg(not(feature = "access-harness"))]
const fn one_shot(_seat: &str) -> Option<bool> {
    None
}
