// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The kept-default journeys: a weekly digest proposed at once with its run model and its
//! schedule's zone as choices with their defaults. Picking each default natively (a host sends
//! the recommended row's key on Enter) keeps the proposal shown, with no author round trip, until
//! its save decision; picking another option still sends the author to revise.

use serde_json::{Value, json};

use super::schedules::zone_choice;
use super::{Act, DIGEST, HACKER_NEWS, SEAT, Scenario, TECHCRUNCH, WHEN, ZONE, digest, resolution};
use crate::peer::{Script, Step, call, call_saying};

/// The weekly request: when (`WHEN`), the two sites and the file, all in the person's words.
const WEEKLY_NEWS: &str =
    "chaque lundi à 9h, résume Hacker News et TechCrunch dans ./news/digest.md";
/// The zone the person may pick instead of the default.
pub(crate) const PARIS: &str = "Europe/Paris";
/// What the author says with its first proposal.
const SHOWN: &str = "Voici le workflow : le résumé de Hacker News et TechCrunch chaque lundi.";

/// The digest: the two sites the person named, and the file they typed (no selection).
fn written() -> Value {
    json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], DIGEST),
        "resolutions": [
            resolution(HACKER_NEWS, "named", "read_source", "u1", "Hacker News"),
            resolution(TECHCRUNCH, "named", "read_source", "u1", "TechCrunch"),
        ],
        "summary": "Hacker News et TechCrunch résumés dans ./news/digest.md, chaque lundi",
    })
}

/// The run model shown as a choice: the Session's seat, which the candidate runs on.
fn model_choice() -> Value {
    json!({"key": "model", "question": "Quel modèle pour le résumé ?", "role": "run_model",
        "options": [
            {"key": "seat", "label": "Le modèle de cette Session", "recommended": true,
             "values": [{"role": "run_model", "value": SEAT}]}]})
}

/// `propose` with the schedule in `zone` and the choices still `open`.
fn proposing(zone: &str, open: &[Value]) -> Value {
    json!({"schedule": {"message": "u1", "excerpt": WHEN, "timezone": zone}, "open": open})
}

/// The digest written, then proposed with the run model and the zone shown as choices.
fn proposed() -> Vec<Step> {
    vec![
        call("candidate_write", written()),
        call_saying(
            SHOWN,
            "propose",
            proposing(ZONE, &[model_choice(), zone_choice(WHEN)]),
        ),
    ]
}

/// One journey over the scripted author.
fn journey(acts: Vec<Act>, agent: Vec<Step>) -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts,
        script: Script {
            agent,
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The proposal with both choices; Enter on the model's default, then on the zone's: each kept
/// with no author, the save decision next; « oui » saves.
pub(super) fn kept_defaults() -> Scenario {
    let acts = vec![
        Act::Turn(WEEKLY_NEWS),
        Act::AnswerProposalChoiceOf(0, 0, "seat"),
        Act::AnswerProposalChoiceOf(0, 1, "local"),
        Act::Submit("oui"),
    ];
    journey(acts, proposed())
}

/// The proposal with both choices; another zone picked: the author revises, proposing the
/// schedule in that zone with the model still shown; Enter on the model's default then keeps it
/// with no author.
pub(super) fn kept_defaults_other_zone() -> Scenario {
    let mut agent = proposed();
    agent.push(call_saying(
        "Le résumé suivra Europe/Paris.",
        "propose",
        proposing(PARIS, &[model_choice()]),
    ));
    let acts = vec![
        Act::Turn(WEEKLY_NEWS),
        Act::AnswerProposalChoiceOf(0, 1, "paris"),
        Act::AnswerProposalChoiceOf(1, 0, "seat"),
    ];
    journey(acts, agent)
}
