// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run ceiling journeys (NIK-170): a Session opened on a record whose inference exposure was
//! budgeted has no saved decision to stand for a typed request's Run ceiling, and one reopened
//! mid-conversation keeps no draft ceiling for its later proposals. Either way the run asks its
//! ceiling instead of refusing that it cannot be proved, and the amount picked runs it.

use serde_json::json;

use super::{
    ACCEPT, ADD_GEOPOLITICS, Act, DIGEST, HACKER_NEWS, LE_MONDE, LE_MONDE_WORDS, REQUEST, Ran,
    Scenario, TECHCRUNCH, accepted_pick, accepted_steps, digest, resolution, with_le_monde,
};
use crate::peer::{Script, call, call_saying, say};

/// The request: the two sites and the file, then its ceiling for each run (QA gauntlet q09).
const BUDGETED: &str =
    "résume Hacker News et TechCrunch dans ./news/digest.md, budget max 0,10 $ par run";
/// The French rerun of the saved workflow.
pub(crate) const RELANCE: &str = "relance-le";
/// The words of `COSTED` that state its ceiling: what each run may cost, then its aside.
pub(crate) const RUN_COST: &str =
    "chaque exécution doit coûter au maximum 0,10 $ (budget max 0,10 $ par run)";
/// The request saying what each run may cost, as QA gauntlet q09 did.
const COSTED: &str = "résume Hacker News et TechCrunch dans ./news/digest.md, et chaque exécution doit coûter au maximum 0,10 $ (budget max 0,10 $ par run)";

/// A project record whose inference exposure was budgeted: the Session opened on it restores
/// the exposure as a restriction, never an allowance.
const BUDGETED_RECORD: &str = r#"{"version":1,"updated_at":"2026-10-15T09:00:00Z","inference_observations":[{"schema":"nika/session-decision-seat@2","state":"Closed","attempts":[],"cost":"unknown"}]}"#;

/// Save and run once, as a host's native action sends it.
pub(crate) const SAVE_AND_RUN_ONCE: &str = "save & run";
/// The amount the person picks among the ceilings offered.
pub(crate) const PICKED: &str = "0.25";
/// The French run line the person types after saving.
pub(crate) const LANCE: &str = "lance-le";

/// Opened on a budgeted record: the offer's recommendation picked, the proposal, `save & run`,
/// then the ceiling it asks, picked.
pub(super) fn ceiling_restored() -> Scenario {
    Scenario {
        files: vec![(".nika/session-state.json", BUDGETED_RECORD)],
        history: true,
        preparing: true,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Submit(SAVE_AND_RUN_ONCE),
            Act::Submit(PICKED),
        ],
        script: Script {
            agent: accepted_steps(),
            replies: Vec::new(),
            picks: accepted_pick(),
            verdicts: Vec::new(),
        },
    }
}

/// The digest `BUDGETED` asks for: the two sites it names, written to the file it names.
fn budgeted_digest() -> serde_json::Value {
    json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH)], DIGEST),
        "resolutions": [
            resolution(HACKER_NEWS, "named", "read_source", "u1", "Hacker News"),
            resolution(TECHCRUNCH, "named", "read_source", "u1", "TechCrunch"),
        ],
        "summary": "Hacker News et TechCrunch résumés dans ./news/digest.md",
    })
}

/// A question about the ceiling, answered by the author (QA gauntlet q09).
const RESPECTED: &str = "mon plafond de 0,10 $ a-t-il été respecté ?";

/// A ceiling stated alone for the proposal waiting (QA gauntlet q03).
pub(crate) const CEILING_ALONE: &str = "avec un plafond de 2 $";
/// The request without any ceiling.
const UNBUDGETED: &str = "résume Hacker News et TechCrunch dans ./news/digest.md";

/// Proposed with no ceiling; a ceiling stated alone binds the proposal waiting with no author;
/// saved with « oui », then « relance-le » runs under it.
pub(super) fn ceiling_for_the_pending() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: true,
        acts: vec![
            Act::Turn(UNBUDGETED),
            Act::Submit(CEILING_ALONE),
            Act::Submit("oui"),
            Act::Turn(RELANCE),
        ],
        script: Script {
            agent: vec![
                call("candidate_write", budgeted_digest()),
                call_saying("Voici le workflow.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The budgeted digest with Le Monde added by `u2` (the « oui » at the door is no line of the
/// conversation), every other value kept from the saved one.
fn budgeted_with_le_monde() -> serde_json::Value {
    let kept = |value: &str, role: &str| json!({"value": value, "kind": "retained", "role": role, "message": "u1"});
    json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH),
            ("le_monde", LE_MONDE)], DIGEST),
        "resolutions": [
            kept(HACKER_NEWS, "read_source"),
            kept(TECHCRUNCH, "read_source"),
            resolution(LE_MONDE, "named", "read_source", "u2", LE_MONDE_WORDS),
        ],
        "summary": "Le Monde international ajouté",
    })
}

/// The request says what each run may cost: proposed with that ceiling beside its bytes and
/// saved with « oui », then the saved workflow revised (one more source): the revision is judged
/// with the same ceiling beside its bytes (live gauntlet wave 6 r09 B1, 2026-10-18).
pub(super) fn ceiling_kept_by_a_revision() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: true,
        acts: vec![
            Act::Turn(COSTED),
            Act::Submit("oui"),
            Act::Turn(ADD_GEOPOLITICS),
        ],
        script: Script {
            agent: vec![
                call("candidate_write", budgeted_digest()),
                call_saying("Voici le workflow.", "propose", json!({})),
                call("candidate_write", budgeted_with_le_monde()),
                call_saying("J'ai ajouté Le Monde international.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// [`ceiling_kept_by_a_revision`], saved and run once by the host's action before the revision,
/// as the live gauntlet did.
pub(super) fn ceiling_kept_by_a_revision_after_a_run() -> Scenario {
    let mut scenario = ceiling_kept_by_a_revision();
    scenario.acts = vec![
        Act::Turn(COSTED),
        Act::Submit(SAVE_AND_RUN_ONCE),
        Act::RunEnds(Ran::Succeeded),
        Act::Turn(ADD_GEOPOLITICS),
    ];
    scenario
}

/// The request says what each run may cost: proposed with that ceiling beside its bytes,
/// saved with « oui », then « relance-le » runs the saved workflow under it.
pub(super) fn ceiling_in_request() -> Scenario {
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: true,
        acts: vec![Act::Turn(COSTED), Act::Submit("oui"), Act::Turn(RELANCE)],
        script: Script {
            agent: vec![
                call("candidate_write", budgeted_digest()),
                call_saying("Voici le workflow.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// The request states its ceiling for each run and is saved with it, runs, and the author
/// answers a question; a later line adds a source and states none: the revision keeps the saved
/// ceiling, its Save keeps it, and « relance-le » runs under it.
pub(super) fn ceiling_kept_by_revision() -> Scenario {
    let revised = json!({
        "source": digest(&[("hacker_news", HACKER_NEWS), ("techcrunch", TECHCRUNCH),
            ("le_monde", LE_MONDE)], DIGEST),
        "resolutions": [
            resolution(HACKER_NEWS, "named", "read_source", "u1", "Hacker News"),
            resolution(TECHCRUNCH, "named", "read_source", "u1", "TechCrunch"),
            resolution(LE_MONDE, "named", "read_source", "u3", LE_MONDE_WORDS),
        ],
        "summary": "Le Monde international ajouté",
    });
    Scenario {
        files: Vec::new(),
        history: false,
        preparing: true,
        acts: vec![
            Act::Turn(BUDGETED),
            Act::Submit("oui"),
            Act::Turn(RELANCE),
            Act::Turn(RESPECTED),
            Act::Turn(ADD_GEOPOLITICS),
            Act::Submit("oui"),
            Act::Turn(RELANCE),
        ],
        script: Script {
            agent: vec![
                call("candidate_write", budgeted_digest()),
                call_saying("Voici le workflow.", "propose", json!({})),
                say("Oui : le run indique son plafond, 0,10 $."),
                call("candidate_write", revised),
                call_saying("J'ai ajouté Le Monde international.", "propose", json!({})),
            ],
            replies: Vec::new(),
            picks: Vec::new(),
            verdicts: Vec::new(),
        },
    }
}

/// Reopened mid-conversation: the later proposal carries no ceiling; saved with « oui », then
/// « lance-le » asks the ceiling in French; the amount picked runs it.
pub(super) fn ceiling_reopened() -> Scenario {
    let mut agent = accepted_steps();
    agent.extend([
        call("candidate_write", with_le_monde()),
        call_saying(
            "J'ai ajouté Le Monde international ; le reste ne change pas.",
            "propose",
            json!({}),
        ),
    ]);
    Scenario {
        files: Vec::new(),
        history: true,
        preparing: true,
        acts: vec![
            Act::Turn(REQUEST),
            Act::Submit(ACCEPT),
            Act::Reopen,
            Act::Turn(ADD_GEOPOLITICS),
            Act::Submit("oui"),
            Act::Turn(LANCE),
            Act::Submit(PICKED),
        ],
        script: Script {
            agent,
            replies: Vec::new(),
            picks: accepted_pick(),
            verdicts: Vec::new(),
        },
    }
}
