// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The failing-check journey (QA gauntlet q04): a candidate whose own check fails (a temperature
//! on a Claude 5 model, which refuses any but its default) is never shown, so nothing offers to
//! save or run it; the author reads the check's findings and repairs the bytes first.

use serde_json::{Value, json};

use super::{Act, DIGEST, HACKER_NEWS, Scenario, TECHCRUNCH, digest, resolution};
use crate::peer::{Script, Step, call, call_saying};

/// The run model the person names: a Claude 5 model, which fixes its sampling.
pub(crate) const CLAUDE: &str = "anthropic/claude-sonnet-5";
/// The request: the two sites, the file and the run model, all in the person's words.
const REQUEST: &str =
    "résume Hacker News et TechCrunch dans ./news/digest.md avec anthropic/claude-sonnet-5";
/// The temperature the first candidate sets on that model.
pub(crate) const TEMPERATURE: &str = "temperature: 0";

/// The digest on the named model, room for its reasoning, with `extra` lines after its token
/// bound.
fn on_claude(extra: &str) -> String {
    let source = digest(
        &[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)],
        DIGEST,
    )
    .replace(super::SEAT, CLAUDE);
    source.replacen(
        "      max_tokens: 1000\n",
        &format!("      max_tokens: 16384\n{extra}"),
        1,
    )
}

/// A candidate of `source`, every value named in the request.
fn written(source: &str) -> Value {
    json!({
        "source": source,
        "resolutions": [
            resolution(HACKER_NEWS, "named", "read_source", "u1", "Hacker News"),
            resolution(TECHCRUNCH, "named", "read_source", "u1", "TechCrunch"),
        ],
        "summary": "Hacker News et TechCrunch résumés dans ./news/digest.md",
    })
}

/// The author's steps: the failing candidate proposed, then repaired and proposed again.
fn steps() -> Vec<Step> {
    vec![
        call(
            "candidate_write",
            written(&on_claude(&format!("      {TEMPERATURE}\n"))),
        ),
        call_saying("Voici le workflow.", "propose", json!({})),
        call("candidate_write", written(&on_claude(""))),
        call_saying("Voici le workflow corrigé.", "propose", json!({})),
    ]
}

/// What the author claims after its proposal was refused, and the same author answering a plain
/// question with no proposal in its turn.
pub(crate) const CLAIMED_RUN: &str = "C'est enregistré et lancé.";
/// The author's plain answer to a change, proposing nothing and refused nothing.
pub(crate) const PLAIN_ANSWER: &str = "Un digest rassemble les titres du jour.";

/// The request, answered by a candidate whose check fails and the author's claim that it was
/// saved and run (live gauntlet wave 9 r07, 2026-10-20), then a change the author answers in words.
pub(super) fn claimed_after_refusal() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Turn("ajoute aussi Le Monde au digest"),
        ],
        script: Script {
            agent: vec![
                call(
                    "candidate_write",
                    written(&on_claude(&format!("      {TEMPERATURE}\n"))),
                ),
                call_saying("Voici le workflow.", "propose", json!({})),
                crate::peer::say(CLAIMED_RUN),
                crate::peer::say(PLAIN_ANSWER),
                crate::peer::say(PLAIN_ANSWER),
                crate::peer::say(PLAIN_ANSWER),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// A proposal shown and still waiting, then a change whose revision Nika's check refuses and the
/// author's claim that it was saved and run (live gauntlet wave 10 r08, 2026-10-20: « Rien n'a été
/// proposé … » beside a proposal the person could still save).
pub(super) fn claimed_beside_a_pending_proposal() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Turn("et mets la température du modèle à zéro"),
        ],
        script: Script {
            agent: vec![
                call("candidate_write", written(&on_claude(""))),
                call_saying("Voici le workflow.", "propose", json!({})),
                call(
                    "candidate_write",
                    written(&on_claude(&format!("      {TEMPERATURE}\n"))),
                ),
                call_saying("Voici le workflow corrigé.", "propose", json!({})),
                crate::peer::say(CLAIMED_RUN),
                crate::peer::say(CLAIMED_RUN),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The request, answered by the author's failing then repaired candidate.
pub(super) fn failing_check() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: false,
        acts: vec![Act::Turn(REQUEST)],
        script: Script {
            agent: steps(),
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}
