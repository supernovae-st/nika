// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The sessions a workflow's own declared ACP route opens, and the one door
//! that reads what such a session offers without a prompt.
//!
//! The route is the file's, never a caller's: `run.access.via` names the
//! seat and `run.access.protocol: acp` selects its sessions. A workflow that
//! declares neither has no session to ask, and nothing here guesses one from
//! what the machine could serve. The sessions are the two shapes its tasks
//! run in: the route's audited one-shot completion profile for the models
//! its `infer:` tasks resolve to, the route's agent session for the models
//! its `agent:` tasks resolve to. A model rendered at dispatch is not a
//! static requirement and names no session here.
//!
//! [`read_offer`] is the door over [`nika_harness::session_offer`]: the same
//! registry adapter, identity judgment and profile a run meets, ended before
//! any prompt. An offer grants nothing: a run still selects, reads back and
//! refuses on its own session.

use std::time::Duration;

use nika_check::CheckReport;
use nika_kernel::ai::harness::HarnessError;
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_types::access::{AccessProtocol, AccessRequirement};

pub use nika_harness::{EffortOffer, ModelOffer, OfferProfile, ServiceTierOffer, SessionOffer};

/// One session a workflow opens on its declared route: the shape, the model
/// as the file names it, and the tasks that run in it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OfferTarget {
    /// The session shape: the completion profile (`infer:`) or the agent session (`agent:`).
    pub profile: OfferProfile,
    /// The model, in the combined `<provider>/<name>` form the check reports.
    pub model: String,
    /// The ids of the tasks that resolve to this model in this shape, in file order.
    pub tasks: Vec<String>,
}

impl OfferTarget {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(profile: OfferProfile, model: impl Into<String>, tasks: Vec<String>) -> Self {
        Self {
            profile,
            model: model.into(),
            tasks,
        }
    }
}

/// The seat whose ACP sessions the file itself selects: `run.access.via`
/// under `run.access.protocol: acp`.
///
/// # Errors
///
/// The reason no session can be asked, in words a client may show: the file
/// declares no route, or a route that is not an ACP one.
pub fn declared_acp_seat(requirement: Option<&AccessRequirement>) -> Result<&str, &'static str> {
    let Some(requirement) = requirement else {
        return Err("the workflow declares no `run.access`: it names no route to ask");
    };
    let Some(seat) = requirement.via.as_deref() else {
        return Err("the workflow declares no `run.access.via`: it names no route to ask");
    };
    if requirement.protocol != Some(AccessProtocol::Acp) {
        return Err(
            "the workflow does not declare `run.access.protocol: acp`: only an ACP route has \
             sessions to ask",
        );
    }
    Ok(seat)
}

/// The sessions `wf` opens on its route, from its checked model requirements
/// (task `model:` ?? envelope) joined with the verb of every task that
/// resolves to each model: completion targets first, then agent targets,
/// each in the report's model order.
#[must_use]
pub fn offer_targets(wf: &RawWorkflow, report: &CheckReport) -> Vec<OfferTarget> {
    let mut completion = Vec::new();
    let mut agent = Vec::new();
    for requirement in &report.requirements.models {
        let (mut infer_tasks, mut agent_tasks) = (Vec::new(), Vec::new());
        for id in &requirement.tasks {
            let action = wf
                .tasks
                .iter()
                .find(|task| task.value.id.value == *id)
                .map(|task| &task.value.action);
            match action {
                Some(RawAction::Infer(_)) => infer_tasks.push(id.clone()),
                Some(RawAction::Agent(_)) => agent_tasks.push(id.clone()),
                _ => {}
            }
        }
        if !infer_tasks.is_empty() {
            completion.push(OfferTarget::new(
                OfferProfile::Completion,
                requirement.model.clone(),
                infer_tasks,
            ));
        }
        if !agent_tasks.is_empty() {
            agent.push(OfferTarget::new(
                OfferProfile::Agent,
                requirement.model.clone(),
                agent_tasks,
            ));
        }
    }
    completion.extend(agent);
    completion
}

/// Read what a session of `seat` offers for `target`, without a prompt
/// ([`nika_harness::session_offer`]). `timeout` bounds the whole reading.
///
/// # Errors
///
/// What a run on the same route meets before its prompt: the adapter is
/// absent, silent or outside its pin, its identity or the completion profile
/// is refused, or the session dies.
pub async fn read_offer(
    seat: &str,
    target: &OfferTarget,
    timeout: Duration,
) -> Result<SessionOffer, HarnessError> {
    nika_harness::session_offer(seat, target.profile, Some(&target.model), timeout).await
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn checked(source: &str) -> (RawWorkflow, CheckReport) {
        let wf = nika_schema::parse(
            source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("the fixture parses");
        let report = nika_check::check(&wf);
        (wf, report)
    }

    const ROUTED: &str = "nika: t\nmodel: openai/gpt-6-luna\nrun:\n  access: { via: codex, protocol: \
                          acp, fallback: none }\n  reasoning: { effort: low }\ntasks:\n  brief:\n    \
                          infer: { prompt: hi }\n  second_brief:\n    infer: { prompt: hi }\n  \
                          render:\n    agent: { prompt: go, tools: [] }\n  other_model:\n    agent: \
                          { prompt: go, tools: [], model: \"openai/gpt-6-terra\" }\n  shout:\n    \
                          exec: { command: [\"true\"] }\n";

    #[test]
    fn a_declared_acp_route_names_its_seat_and_anything_else_names_why_not() {
        let (wf, _) = checked(ROUTED);
        let requirement = crate::access::declared_requirement(&wf);
        assert_eq!(declared_acp_seat(requirement.as_ref()), Ok("codex"));

        assert!(
            declared_acp_seat(None)
                .expect_err("no declaration")
                .contains("declares no `run.access`")
        );
        let no_via = AccessRequirement::new().with_protocol(Some(AccessProtocol::Acp));
        assert!(
            declared_acp_seat(Some(&no_via))
                .expect_err("no seat")
                .contains("`run.access.via`")
        );
        let mut api = AccessRequirement::new().with_protocol(Some(AccessProtocol::Api));
        api.via = Some("openai".to_owned());
        assert!(
            declared_acp_seat(Some(&api))
                .expect_err("an API route has no session")
                .contains("`run.access.protocol: acp`")
        );
        let mut unstated = AccessRequirement::new();
        unstated.via = Some("codex".to_owned());
        assert!(
            declared_acp_seat(Some(&unstated)).is_err(),
            "a seat without the protocol is not an ACP selection"
        );
    }

    #[test]
    fn each_model_names_one_session_per_shape_its_tasks_run_in() {
        let (wf, report) = checked(ROUTED);
        let targets = offer_targets(&wf, &report);
        let rows: Vec<(&str, &str, Vec<&str>)> = targets
            .iter()
            .map(|target| {
                (
                    target.profile.as_str(),
                    target.model.as_str(),
                    target.tasks.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "completion",
                    "openai/gpt-6-luna",
                    vec!["brief", "second_brief"]
                ),
                ("agent", "openai/gpt-6-luna", vec!["render"]),
                ("agent", "openai/gpt-6-terra", vec!["other_model"]),
            ],
            "the envelope model in both shapes, the task pin in its own, the exec task in none"
        );
    }

    #[test]
    fn a_workflow_without_a_model_task_names_no_session() {
        let (wf, report) =
            checked("nika: t\ntasks:\n  shout:\n    exec: { command: [\"true\"] }\n");
        assert!(offer_targets(&wf, &report).is_empty());
    }
}
