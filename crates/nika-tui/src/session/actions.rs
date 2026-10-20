// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Finite actions use the existing Session protocol, bound to what was painted.
//! A fresh presentation has no selection. Rendering and navigation grant nothing.

use nika_session::{SessionRuntime, work};

use super::Live;
use super::typed_answer::Reply;
use crate::lang::Voice;
use crate::model::{Asked, Beat, Offer, Shape, Turn, Waiting};

/// The key the Session asks a landed declaration's trigger under (D1): install it on this Mac
/// or keep the schedule declared.
pub(super) const INSTALL: &str = "trigger.install";

/// A local presentation receipt, never a Session authority or a durable consent.
pub(super) struct Pending {
    waiting: work::Waiting,
    context: Option<String>,
    witness: String,
    offers: Vec<Offer>,
    free_intent: bool,
    /// The identity of the optional offer about the last result, when this is it: the keys
    /// the native control picks answer it by that identity; words go to the conversation.
    question: Option<nika_session::QuestionId>,
}

/// Only facts that identify the decision enter the comparison, not activity or costs
/// from unrelated turns. Decisions without a strong owner ID also get a fresh nonce.
fn context(runtime: &SessionRuntime, snapshot: &work::Work) -> Option<String> {
    match &snapshot.waiting {
        work::Waiting::CostChoice => runtime.cost_choice_details(),
        work::Waiting::Free => (runtime.waiting_judgment())
            .or_else(|| {
                (runtime.stale_saved_run()).map(|path| format!("stale run {}", path.display()))
            })
            .or_else(|| {
                (snapshot.feedback.offer.as_ref())
                    .map(|offer| format!("feedback {}", offer.question.id.as_str()))
            }),
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
        work::Waiting::RunCeiling { .. } => {
            serde_json::to_string(&runtime.waiting_run_ceiling_choices()?).ok()
        }
        _ => None,
    }
}

pub(super) fn capture(
    runtime: &SessionRuntime,
    snapshot: &work::Work,
    voice: Voice,
) -> Option<(Pending, Waiting)> {
    if let Some(offer) = (snapshot.feedback.offer.as_ref())
        .filter(|_| snapshot.waiting == work::Waiting::Free)
        .filter(|_| runtime.waiting_judgment().is_none() && runtime.stale_saved_run().is_none())
    {
        return Some(feedback(snapshot, offer));
    }
    let gate_choices = runtime.waiting_gate_choices();
    let gate_prompt = runtime.waiting_gate_prompt();
    let gate_expiry = runtime.waiting_gate_expiry();
    // The acts and the question this host builds in `voice` (the language the person writes
    // and the glyph column in use); the Session's own sentences and options, and every key, as
    // they are.
    let (key, label, offers) = offered(runtime, snapshot, gate_choices.as_ref(), voice)?;
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
                (voice.say("Choose a value for {name} ({kind})"))
                    .replace("{kind}", kind)
                    .replace("{name}", name)
            }
            // The activation's question in its own words: its sentence, as the Session asks it.
            work::Waiting::Activation { .. } => {
                let (_, said, _, _) = runtime.waiting_activation_choices()?;
                said.lines().next().unwrap_or(label).to_owned()
            }
            // So is the run's ceiling, asked because none could be proved.
            work::Waiting::RunCeiling { .. } => {
                let (_, said, _, _) = runtime.waiting_run_ceiling_choices()?;
                said.lines().next().unwrap_or(label).to_owned()
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
        // A question beside its rows (an activation's, a run's ceiling) takes the person's
        // typed line, a delegation included, as the Session reads it, and so does the recovery
        // of a drifted saved workflow (words of one's own stay open beside it). A consent keeps
        // its acts (a gate answers yes or no, a cost decision its rows), and a run input its
        // exact values: nothing answers for the person there.
        free_intent: matches!(key, "judgment" | "stale_run")
            || matches!(
                snapshot.waiting,
                work::Waiting::Consent { .. }
                    | work::Waiting::Activation { .. }
                    | work::Waiting::RunCeiling { .. }
            ),
        question: None,
    };
    let why = if let work::Waiting::Gate { gate } = &snapshot.waiting
        && gate_expiry
            .as_ref()
            .is_some_and(|(id, expired, _)| id == gate && *expired)
    {
        voice.say("Expired: a fresh question is needed before approval.")
    } else {
        ""
    };
    // A decision names no request: the Session's goal is the conversation's first one with
    // its corrections, never necessarily the one this decision answers (a proposal's card
    // reads its own request).
    let mut asked = Asked::new(label, why, true, Shape::Choice(offers), witness, epoch);
    asked.free_text = pending.free_intent;
    Some((
        pending,
        Waiting::Action {
            key: key.to_owned(),
            asked,
        },
    ))
}

/// The optional offer about the last result, at the free prompt: its own question and options
/// (checked together for an adjustment, « Nothing to change » and « Later » standing alone),
/// optional, its own words open beside it as the person's next line to the conversation.
fn feedback(snapshot: &work::Work, offer: &work::FeedbackOffer) -> (Pending, Waiting) {
    let question = &offer.question;
    let offers: Vec<Offer> = (question.options.iter())
        .map(|option| {
            let offer = Offer::new(option.key.clone(), option.label.clone());
            if option.exclusive {
                offer.exclusive()
            } else {
                offer
            }
        })
        .collect();
    let shape = if question.multi_select {
        Shape::Multiple(offers.clone())
    } else {
        Shape::Choice(offers.clone())
    };
    let epoch = super::asked::epoch();
    let witness = format!("action:{epoch}");
    let pending = Pending {
        waiting: snapshot.waiting.clone(),
        context: Some(format!("feedback {}", question.id.as_str())),
        witness: witness.clone(),
        offers,
        free_intent: true,
        question: Some(question.id.clone()),
    };
    // The offer is about the result above it, never about the conversation's latest line:
    // the request it would name is that line (live gauntlet wave 7, 2026-10-19: « Request ·
    // Je n'y connais rien en modèles, choisis pour moi. » on a reading list's offer).
    let mut asked = Asked::new(question.question.clone(), "", false, shape, witness, epoch);
    asked.free_text = true;
    let key = question.key.clone();
    (pending, Waiting::Action { key, asked })
}

fn offered(
    runtime: &SessionRuntime,
    snapshot: &work::Work,
    gate_choices: Option<&(nika_session::GateId, Vec<String>, String)>,
    voice: Voice,
) -> Option<(&'static str, &'static str, Vec<Offer>)> {
    let say = |en: &'static str| voice.say(en);
    Some(match &snapshot.waiting {
        work::Waiting::Consent { proposal }
            if !snapshot
                .open_choices
                .iter()
                .any(|q| q.state == work::AskedState::Open) =>
        {
            (
                "save",
                say("What should happen to this reviewed proposal?"),
                proposal_offers(runtime, proposal, voice)?,
            )
        }
        work::Waiting::RunReview { .. } => (
            "run_cost",
            say("Continue this run once?"),
            decisions(say("Run once"), say("Cancel this run"), voice),
        ),
        work::Waiting::CostChoice if runtime.cost_choice_details().is_some() => (
            "unknown_cost",
            say("Continue this unpriced request once?"),
            decisions(say("Continue once"), say("Cancel this request"), voice),
        ),
        work::Waiting::Gate { gate } if runtime.waiting_confirm_gate().as_ref() == Some(gate) => (
            "gate",
            say("Answer this workflow confirmation"),
            vec![
                Offer::new("yes", say("Approve this step")),
                Offer::new("no", say("Refuse this step")),
                Offer::new("why?", say("Explain this step")),
            ],
        ),
        work::Waiting::Gate { gate } => (
            "gate_choice",
            say("Choose the answer for this workflow step"),
            gate_choice_offers(gate, gate_choices?)?,
        ),
        work::Waiting::Input { name } => (
            "run_input",
            say("Choose a value for this input"),
            input_offers(runtime, name, voice)?,
        ),
        work::Waiting::Free if runtime.waiting_judgment().is_some() => (
            "judgment",
            say("This candidate still needs qualification"),
            vec![
                Offer::new("retry", say("Retry qualification")),
                Offer::new("cancel", say("Discard the unqualified candidate")),
            ],
        ),
        work::Waiting::Free => (
            "stale_run",
            say(
                "The saved workflow or the files it reads changed since its rehearsal: it was not run",
            ),
            stale_offers(runtime.stale_saved_run()?, voice),
        ),
        // The activation's finite questions (the policy for a missed run, the ceiling per run):
        // the Session's own options, its recommendation preselected, then the way out. The
        // trigger's decision (D1) comes after the declaration landed: its two acts are its way
        // out (keeping the schedule declared is one of them), and no setup is left to cancel.
        work::Waiting::Activation { key } => {
            let (asked, _, options, _) = runtime
                .waiting_activation_choices()
                .filter(|(asked, ..)| asked == key)?;
            let mut offers = recommending(options);
            if asked != INSTALL {
                offers.push(Offer::new("cancel", say("Cancel schedule setup")));
            }
            (asked, say("Answer this schedule question"), offers)
        }
        // A run whose ceiling no saved decision proves: the amounts, the one recommended
        // preselected with why, then the way out. Nothing runs before a pick or a typed amount.
        work::Waiting::RunCeiling { .. } => {
            let (asked, _, options, _) = runtime.waiting_run_ceiling_choices()?;
            let mut offers = recommending(options);
            offers.push(Offer::new("cancel", say("Cancel this run")));
            (asked, say("Choose this run's ceiling"), offers)
        }
        _ => return None,
    })
}

/// The values a workflow's choice gate `gate` offers, each sent as its JSON: none when the
/// choices the Session holds are another gate's.
fn gate_choice_offers(
    gate: &nika_session::GateId,
    (current, choices, _): &(nika_session::GateId, Vec<String>, String),
) -> Option<Vec<Offer>> {
    if current != gate {
        return None;
    }
    choices
        .iter()
        .map(|value| {
            serde_json::to_string(value)
                .ok()
                .map(|key| Offer::new(key, value))
        })
        .collect()
}

/// The Session's options as rows, its recommendation marked.
fn recommending(options: Vec<work::Offer>) -> Vec<Offer> {
    (options.into_iter())
        .map(|option| {
            let offer = Offer::new(option.key, option.label);
            if option.recommended {
                offer.recommended()
            } else {
                offer
            }
        })
        .collect()
}

/// The finite recovery of a saved workflow whose world drifted from its rehearsal: a request to
/// adapt it, proposed, rehearsed and admitted afresh, never a run on the withdrawn proof. Words of
/// one's own stay open beside it (restoring the file and asking again needs no choice).
fn stale_offers(workflow: &std::path::Path, voice: Voice) -> Vec<Offer> {
    vec![Offer::new(
        format!(
            "Adapt `{}` to the files as they are now.",
            workflow.display()
        ),
        voice.say("Adapt the workflow to the changed files"),
    )]
}

fn input_offers(runtime: &SessionRuntime, name: &str, voice: Voice) -> Option<Vec<Offer>> {
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
    offers.push(Offer::new("cancel", voice.say("Cancel this run")));
    Some(offers)
}

fn proposal_offers(
    runtime: &SessionRuntime,
    proposal: &nika_session::ProposalId,
    voice: Voice,
) -> Option<Vec<Offer>> {
    let candidate = runtime
        .candidate()
        .filter(|c| c.id == *proposal && !c.aside)?;
    let save_label = voice.say(
        if candidate
            .set
            .changes
            .iter()
            .any(nika_session::ProjectChange::is_workflow)
        {
            "Save workflow"
        } else {
            "Save these changes"
        },
    );
    // Bytes whose own check fails are never offered to save or run: the Session refuses both.
    let saves = candidate.set.failing_audits().is_empty();
    let mut offers = Vec::new();
    if saves {
        offers.push(Offer::new("yes", save_label));
    }
    if saves && candidate.set.save_run().is_ok() {
        offers.push(Offer::new("save & run", voice.say("Save and run once")));
    }
    offers.extend([
        Offer::new("no", voice.say("Discard this proposal")),
        Offer::new("/show", voice.say("Inspect the exact changes")),
    ]);
    Some(offers)
}

fn decisions(approve: &str, decline: &str, voice: Voice) -> Vec<Offer> {
    vec![
        Offer::new("yes", approve),
        Offer::new("no", decline),
        Offer::new("details", voice.say("Read the full cost evidence")),
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
    pub(super) fn action_answer(&mut self, reply: Reply<'_>, witness: &str) -> Turn {
        let text = reply.text();
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
        let offer = pending.question.clone();
        let Some(runtime) = self.runtime.as_mut() else {
            return self.not_shown(text);
        };
        // The offer about the last result takes the keys the native control picked, by its
        // identity; words typed beside it are the person's next line to the conversation.
        let outcome = match (offer, reply) {
            (Some(id), Reply::Picked(keys)) => runtime.answer_question_for(&id, keys),
            _ => runtime.submit(text, &shown),
        };
        let refused = matches!(outcome, nika_session::runtime::TurnOutcome::Refusal(_));
        let (mut beats, handoff) = self.map(outcome);
        if refused {
            beats.push(Beat::NotTaken(text.to_owned()));
        }
        Turn { beats, handoff }
    }
}
