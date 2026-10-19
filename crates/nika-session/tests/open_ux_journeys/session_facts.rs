// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the Session says of its own work (QA gauntlet q09): `/details` after a Save and a run
//! says what the conversation built, saved and ran, and the author reads the Session's spend so
//! far with each line the person types.

use serde_json::json;

use super::{Act, DIGEST, HACKER_NEWS, Ran, Scenario, TECHCRUNCH, digest, resolution};
use crate::peer::{Script, call, call_saying, say};

/// The request: the two sites and the file, in the person's words.
const REQUEST: &str = "résume Hacker News et TechCrunch dans ./news/digest.md";
/// The details card, asked for.
pub(crate) const DETAILS: &str = "/details";
/// The person asks what preparing cost (QA gauntlet q09).
pub(crate) const HOW_MUCH: &str = "combien a coûté la préparation jusqu'ici ?";

/// Proposed; then the person asks what the preparation cost, and the author answers.
pub(super) fn spend_asked() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: true,
        acts: vec![Act::Turn(REQUEST), Act::Turn(HOW_MUCH)],
        script: Script {
            agent: vec![
                call("candidate_write", written()),
                call_saying("Voici le workflow.", "propose", json!({})),
                say("La préparation est comptée dans la ligne de dépense ci-dessus."),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The digest of the two sites the request names.
fn written() -> serde_json::Value {
    json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], DIGEST),
        "resolutions": [
            resolution(HACKER_NEWS, "named", "read_source", "u1", "Hacker News"),
            resolution(TECHCRUNCH, "named", "read_source", "u1", "TechCrunch"),
        ],
        "summary": "Hacker News et TechCrunch résumés dans ./news/digest.md",
    })
}

/// Proposed, saved with « oui », run with « relance-le » to its end, then `/details`.
pub(super) fn details_after_a_run() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: true,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit("oui"),
            Act::Turn("relance-le"),
            Act::RunEnds(Ran::Succeeded),
            Act::Turn(DETAILS),
        ],
        script: Script {
            agent: vec![
                call("candidate_write", written()),
                call_saying("Voici le workflow.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}
