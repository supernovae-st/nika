// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Finite actions use the existing Session protocol, bound to what was painted.
//! A fresh presentation has no selection. Rendering and navigation grant nothing.

use nika_session::{SessionRuntime, work};

use super::Live;
use crate::model::{Asked, Beat, Offer, Retained, Shape, Turn, Waiting};

/// A local presentation receipt, never a Session authority or a durable consent.
pub(super) struct Pending {
    waiting: work::Waiting,
    context: Option<String>,
    witness: String,
    offers: Vec<Offer>,
    free_intent: bool,
}

/// Only facts that identify the decision enter the comparison, not activity or costs
/// from unrelated turns. Decisions without a strong owner ID also get a fresh nonce.
fn context(runtime: &SessionRuntime, snapshot: &work::Work) -> Option<String> {
    match &snapshot.waiting {
        work::Waiting::CostChoice => runtime.cost_choice_details(),
        work::Waiting::Free => runtime.waiting_judgment(),
        work::Waiting::Input { .. } => {
            serde_json::to_string(&runtime.waiting_input_choices()?).ok()
        }
        work::Waiting::Gate { .. } => serde_json::to_string(&(
            runtime.waiting_gate_prompt()?,
            runtime.waiting_gate_choices(),
            runtime.waiting_gate_expiry(),
        ))
        .ok(),
        work::Waiting::Consent { .. } => serde_json::to_string(&snapshot.open_choices).ok(),
        work::Waiting::Activation { .. } => {
            serde_json::to_string(&(&snapshot.request, &snapshot.saved)).ok()
        }
        _ => None,
    }
}

pub(super) fn capture(
    runtime: &SessionRuntime,
    snapshot: &work::Work,
) -> Option<(Pending, Waiting)> {
    let gate_choices = runtime.waiting_gate_choices();
    let gate_prompt = runtime.waiting_gate_prompt();
    let gate_expiry = runtime.waiting_gate_expiry();
    let (key, label, offers) = offered(runtime, snapshot, gate_choices.as_ref())?;
    let label = if let work::Waiting::Gate { gate } = &snapshot.waiting {
        gate_prompt.as_ref().filter(|(id, _)| id == gate)?.1.clone()
    } else {
        match &snapshot.waiting {
            work::Waiting::Input { name } => {
                let (_, values, _) = runtime.waiting_input_choices()?;
                let kind = if values.iter().all(serde_json::Value::is_boolean) {
                    "bool"
                } else {
                    "enum"
                };
                format!("Choose a value for {name} ({kind})")
            }
            _ => label.to_owned(),
        }
    };
    let epoch = super::asked::epoch();
    let witness = format!("action:{epoch}");
    let pending = Pending {
        waiting: snapshot.waiting.clone(),
        context: if matches!(snapshot.waiting, work::Waiting::Gate { .. }) {
            gate_prompt.as_ref().and_then(|prompt| {
                serde_json::to_string(&(prompt, &gate_choices, &gate_expiry)).ok()
            })
        } else if matches!(snapshot.waiting, work::Waiting::Input { .. }) {
            Some(context(runtime, snapshot)?)
        } else {
            context(runtime, snapshot)
        },
        witness: witness.clone(),
        offers: offers.clone(),
        free_intent: key == "judgment" || matches!(snapshot.waiting, work::Waiting::Consent { .. }),
    };
    let why = if let work::Waiting::Gate { gate } = &snapshot.waiting
        && gate_expiry
            .as_ref()
            .is_some_and(|(id, expired, _)| id == gate && *expired)
    {
        "Expired: a fresh question is needed before approval."
    } else {
        ""
    };
    let mut asked = Asked::new(label, why, true, Shape::Choice(offers), witness, epoch)
        .retaining(Retained::new(snapshot.request.goal.clone(), Vec::new()));
    asked.free_text = pending.free_intent;
    Some((
        pending,
        Waiting::Action {
            key: key.to_owned(),
            asked,
        },
    ))
}

fn offered(
    runtime: &SessionRuntime,
    snapshot: &work::Work,
    gate_choices: Option<&(nika_session::GateId, Vec<String>, String)>,
) -> Option<(&'static str, &'static str, Vec<Offer>)> {
    Some(match &snapshot.waiting {
        work::Waiting::Consent { proposal }
            if !snapshot
                .open_choices
                .iter()
                .any(|q| q.state == work::AskedState::Open) =>
        {
            (
                "save",
                "What should happen to this reviewed proposal?",
                proposal_offers(runtime, proposal)?,
            )
        }
        work::Waiting::RunReview { .. } => (
            "run_cost",
            "Continue this run once?",
            decisions("Run once", "Cancel this run"),
        ),
        work::Waiting::CostChoice if runtime.cost_choice_details().is_some() => (
            "unknown_cost",
            "Continue this unpriced request once?",
            decisions("Continue once", "Cancel this request"),
        ),
        work::Waiting::Gate { gate } if runtime.waiting_confirm_gate().as_ref() == Some(gate) => (
            "gate",
            "Answer this workflow confirmation",
            vec![
                Offer::new("yes", "Approve this step"),
                Offer::new("no", "Refuse this step"),
                Offer::new("why?", "Explain this step"),
            ],
        ),
        work::Waiting::Gate { gate } => {
            let (current, choices, _) = gate_choices?;
            if current != gate {
                return None;
            }
            let offers = choices
                .iter()
                .map(|value| {
                    serde_json::to_string(value)
                        .ok()
                        .map(|key| Offer::new(key, value))
                })
                .collect::<Option<Vec<_>>>()?;
            (
                "gate_choice",
                "Choose the answer for this workflow step",
                offers,
            )
        }
        work::Waiting::Input { name } => (
            "run_input",
            "Choose a value for this input",
            input_offers(runtime, name)?,
        ),
        work::Waiting::Free if runtime.waiting_judgment().is_some() => (
            "judgment",
            "This candidate still needs qualification",
            vec![
                Offer::new("retry", "Retry qualification"),
                Offer::new("cancel", "Discard the unqualified candidate"),
            ],
        ),
        work::Waiting::Activation { key } if key == "project.missed" => (
            "project.missed",
            "If this machine misses a scheduled run, what should happen?",
            vec![
                Offer::new("1", "Run once when it comes back"),
                Offer::new("2", "Skip the missed run"),
                Offer::new("3", "Replay every missed occurrence"),
                Offer::new("cancel", "Cancel schedule setup"),
            ],
        ),
        _ => return None,
    })
}

fn input_offers(runtime: &SessionRuntime, name: &str) -> Option<Vec<Offer>> {
    let (current, values, _) = runtime.waiting_input_choices()?;
    if current != name {
        return None;
    }
    let mut offers: Vec<_> = values
        .iter()
        .map(|value| {
            let label = value
                .as_str()
                .filter(|text| !text.trim().is_empty())
                .map_or_else(|| value.to_string(), str::to_owned);
            Offer::new(value.to_string(), label)
        })
        .collect();
    offers.push(Offer::new("cancel", "Cancel this run"));
    Some(offers)
}

fn proposal_offers(
    runtime: &SessionRuntime,
    proposal: &nika_session::ProposalId,
) -> Option<Vec<Offer>> {
    let candidate = runtime
        .candidate()
        .filter(|c| c.id == *proposal && !c.aside)?;
    let save_label = if candidate
        .set
        .changes
        .iter()
        .any(nika_session::ProjectChange::is_workflow)
    {
        "Save workflow"
    } else {
        "Save these changes"
    };
    let mut offers = vec![Offer::new("yes", save_label)];
    if candidate.set.save_run().is_ok() {
        offers.push(Offer::new("save & run", "Save and run once"));
    }
    offers.extend([
        Offer::new("no", "Discard this proposal"),
        Offer::new("/show", "Inspect the exact changes"),
    ]);
    Some(offers)
}

fn decisions(approve: &str, decline: &str) -> Vec<Offer> {
    vec![
        Offer::new("yes", approve),
        Offer::new("no", decline),
        Offer::new("details", "Read the full cost evidence"),
    ]
}

impl Pending {
    fn matches(&self, runtime: &SessionRuntime, text: &str, witness: &str) -> bool {
        let snapshot = runtime.work();
        self.witness == witness
            && self.waiting == snapshot.waiting
            && self.context == context(runtime, &snapshot)
            && (self.free_intent
                || self.offers.iter().any(|offer| offer.key == text)
                || matches!(text, "/quit" | "/exit" | "/help" | "/status"))
            && match &self.waiting {
                work::Waiting::Gate { gate } => {
                    runtime.waiting_confirm_gate().as_ref() == Some(gate)
                        || runtime
                            .waiting_gate_choices()
                            .is_some_and(|(current, _, _)| &current == gate)
                }
                _ => true,
            }
    }
}

impl Live {
    /// No action falls through into a new intent, another question or settings.
    pub(super) fn action_answer(&mut self, text: &str, witness: &str) -> Turn {
        let Some((pending, runtime)) = self.action.as_ref().zip(self.runtime.as_ref()) else {
            return self.not_shown(text);
        };
        if !pending.matches(runtime, text, witness) {
            return self.not_shown(text);
        }
        if let work::Waiting::Consent { proposal } = &pending.waiting
            && !self
                .candidate
                .as_ref()
                .is_some_and(|shown| !shown.aside() && shown.id() == proposal)
        {
            return self.not_shown(text);
        }
        let shown = pending.waiting.clone();
        let Some(runtime) = self.runtime.as_mut() else {
            return self.not_shown(text);
        };
        let outcome = runtime.submit(text, &shown);
        let refused = matches!(outcome, nika_session::runtime::TurnOutcome::Refusal(_));
        let (mut beats, handoff) = self.map(outcome);
        if refused {
            beats.push(Beat::NotTaken(text.to_owned()));
        }
        Turn { beats, handoff }
    }
}
