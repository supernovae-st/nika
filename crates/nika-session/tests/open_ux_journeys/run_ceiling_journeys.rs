// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A run no ceiling can be proved for asks its ceiling (NIK-170): after a restore, Save and run
//! once and « lance-le » stop at a question in the person's words, never at the money law's
//! refusal; nothing runs before an amount, and the amount picked runs it under exactly that
//! ceiling.

use serde_json::{Value, json};

use crate::peer::{Kind, Seen};
use crate::scenarios::{CEILING_ALONE, LANCE, PICKED, RELANCE, RUN_COST, SAVE_AND_RUN_ONCE};
use crate::{outcome_kind, run, waiting, work};

/// The state a judge's request shows it: the JSON its user turn holds after `STATE:`.
fn judged_state(seen: &Seen) -> Option<Value> {
    let said = (seen.body["messages"].as_array().into_iter().flatten())
        .filter(|message| message["role"] == "user")
        .find_map(|message| message["content"].as_str()?.strip_prefix("STATE:\n"))?;
    let (state, _) = said.split_once("\n\nOPTIONS:\n")?;
    serde_json::from_str(state).ok()
}

/// The Run ceiling each judge of `journey` was shown beside the bytes.
fn judged_money(journey: &crate::Journey) -> Vec<Value> {
    (journey.seen.iter())
        .filter(|seen| seen.kind == Kind::Judge)
        .filter_map(judged_state)
        .map(|state| state["presentation"]["money"].clone())
        .collect()
}

/// What each run may cost, said in the request (QA gauntlet q09), is the proposal's ceiling: its
/// verification reads it beside the bytes with the words that state it, the Save keeps it, and
/// « relance-le » runs the saved workflow under it and names it.
#[test]
fn a_request_ceiling_rides_beside_the_bytes_and_every_rerun_names_it() {
    let journey = run("ceiling_in_request");
    let proposed = journey.step(0);
    assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
    let said = proposed["outcome"]["text"].as_str().unwrap_or_default();
    assert!(
        said.contains("argent : $0.10 USD · le montant que vous avez donné"),
        "{said}"
    );
    let money = judged_money(&journey);
    assert!(!money.is_empty(), "the scripted judge was asked");
    let beside = json!({"excerpt": RUN_COST, "usd": 0.1});
    assert!(money.iter().all(|m| *m == beside), "{money:#?}");
    let rerun = journey.step(2);
    assert_eq!(rerun["line"], RELANCE);
    assert_eq!(outcome_kind(rerun), "run_requested", "{rerun:#}");
    let report = rerun["outcome"]["text"].as_str().unwrap_or_default();
    assert!(
        report.ends_with(" · plafond $0.10 (enregistré à côté du workflow, hors de son fichier)"),
        "{report}"
    );
}

/// The question the outcome of `step` asks, by its key and words.
fn asked(step: &Value) -> (&str, &str) {
    (
        step["outcome"]["key"].as_str().unwrap_or_default(),
        step["outcome"]["text"].as_str().unwrap_or_default(),
    )
}

/// Whether the outcome of `step` requested a run under exactly `ceiling`.
fn runs_under(step: &Value, ceiling: &str) -> bool {
    outcome_kind(step) == "run_requested"
        && (step["outcome"]["run"].as_str())
            .is_some_and(|run| run.contains(&format!("max_cost_usd: {ceiling},")))
}

/// A revision of the saved workflow is judged with the ceiling its request stated beside its
/// bytes, as the first proposal was, saved alone or saved and run first: the judge never asks a
/// task of the bytes to keep a run's money (live gauntlet wave 6 r09 B1, 2026-10-18: every
/// revision after the save was refused, « the judge finds no task performing » the 0,10 $ per
/// run).
#[test]
fn a_revision_of_the_saved_workflow_is_judged_with_its_ceiling_beside() {
    for (name, revision) in [
        ("ceiling_kept_by_a_revision", 2),
        ("ceiling_kept_by_a_revision_after_a_run", 3),
    ] {
        let journey = run(name);
        let revised = journey.step(revision);
        assert_eq!(outcome_kind(revised), "proposal", "{name}: {revised:#}");
        let money = judged_money(&journey);
        assert!(
            money.len() >= 2,
            "{name}: both proposals were judged: {money:#?}"
        );
        let beside = json!({"excerpt": RUN_COST, "usd": 0.1});
        assert!(money.iter().all(|m| *m == beside), "{name}: {money:#?}");
    }
}

/// Opened on a budgeted record, the conversation answers the offer, proposes, and `save & run`
/// saves, then asks the run's ceiling with the save's report: no refusal in the money law's
/// words, nothing requested. The amount picked runs it.
#[test]
fn a_restored_save_and_run_asks_the_ceiling_then_runs_the_amount_picked() {
    let journey = run("ceiling_restored");
    assert_eq!(waiting(journey.step(1)), "consent", "{:#}", journey.step(1));
    let saved = journey.step(2);
    assert_eq!(saved["line"], SAVE_AND_RUN_ONCE);
    assert_eq!(outcome_kind(saved), "question", "{saved:#}");
    let (key, said) = asked(saved);
    assert_eq!(key, "run.ceiling");
    assert!(said.contains("applied · wrote"), "the save stands: {said}");
    assert!(
        said.ends_with("I did not start the run: what ceiling for this run? (e.g. 0.25)"),
        "{said}"
    );
    for law in [
        "restored inference exposure",
        "cannot be proved",
        "Budget: 10 USD",
    ] {
        assert!(!said.contains(law), "{law}: {said}");
    }
    assert_eq!(waiting(saved), "run_ceiling", "{saved:#}");
    let workflow = work(saved)["saved"]["workflow"].as_str().expect("saved");
    assert_eq!(saved["shown"]["waiting"]["workflow"], workflow);
    assert!(work(saved)["requested"].is_null(), "nothing requested");
    let picked = journey.step(3);
    assert_eq!(picked["line"], PICKED);
    assert!(runs_under(picked, PICKED), "{picked:#}");
    assert_eq!(waiting(picked), "free", "{picked:#}");
}

/// Reopened mid-conversation, the later proposal carries no ceiling: saved with « oui », then
/// « lance-le » asks the ceiling in French, the conversation's language the run line shows; the
/// amount picked runs it.
#[test]
fn a_run_after_a_reopen_asks_its_ceiling_in_the_persons_words() {
    let journey = run("ceiling_reopened");
    let revised = journey.step(3);
    assert_eq!(waiting(revised), "consent", "{revised:#}");
    let saved = journey.step(4);
    assert!(
        (saved["outcome"]["text"].as_str()).is_some_and(|said| said.contains("applied · wrote")),
        "{saved:#}"
    );
    let ran = journey.step(5);
    assert_eq!(ran["line"], LANCE);
    assert_eq!(outcome_kind(ran), "question", "{ran:#}");
    assert_eq!(
        asked(ran),
        (
            "run.ceiling",
            "Je n'ai pas lancé : quel plafond pour ce lancement ? (par ex. 0.25)"
        )
    );
    assert_eq!(waiting(ran), "run_ceiling", "{ran:#}");
    assert!(work(ran)["requested"].is_null(), "nothing requested");
    let picked = journey.step(6);
    assert!(runs_under(picked, PICKED), "{picked:#}");
}

/// A revision of the saved workflow that states no ceiling keeps the one saved with it (QA
/// gauntlet q09: the revision bound the $0.25 default, and « Relance-le » ran under it).
#[test]
fn a_revision_stating_no_ceiling_keeps_the_saved_one_through_its_rerun() {
    let journey = run("ceiling_kept_by_revision");
    for step in [0, 4] {
        let proposed = journey.step(step);
        assert_eq!(outcome_kind(proposed), "proposal", "{proposed:#}");
        let said = proposed["outcome"]["text"].as_str().unwrap_or_default();
        assert!(
            said.contains("argent : $0.10 USD · le montant que vous avez donné"),
            "step {step}: {said}"
        );
    }
    // The revision's verification reads the saved ceiling beside its bytes, with the words
    // of the request that state it.
    let money = judged_money(&journey);
    let beside = json!({"excerpt": "budget max 0,10 $ par run", "usd": 0.1});
    assert!(money.len() >= 2, "{money:#?}");
    assert!(money.iter().all(|m| *m == beside), "{money:#?}");
    for step in [2, 6] {
        let rerun = journey.step(step);
        assert_eq!(rerun["line"], RELANCE);
        assert_eq!(outcome_kind(rerun), "run_requested", "{rerun:#}");
        let report = rerun["outcome"]["text"].as_str().unwrap_or_default();
        assert!(
            report
                .ends_with(" · plafond $0.10 (enregistré à côté du workflow, hors de son fichier)"),
            "step {step}: {report}"
        );
        let run = rerun["outcome"]["run"].as_str().unwrap_or_default();
        assert!(run.contains("max_cost_usd: 0.1,"), "step {step}: {run}");
    }
}

/// A ceiling stated alone at the proposal waiting binds it with no author call (QA gauntlet q03:
/// five minutes of re-authoring for the very bytes): the same bytes under a fresh identity and
/// that ceiling; « oui » saves it, and « relance-le » runs under it.
#[test]
fn a_ceiling_for_the_proposal_waiting_binds_it_with_no_author() {
    let journey = run("ceiling_for_the_pending");
    let first = journey.step(0);
    assert_eq!(outcome_kind(first), "proposal", "{first:#}");
    let bound = journey.step(1);
    assert_eq!(bound["line"], CEILING_ALONE);
    assert_eq!(outcome_kind(bound), "proposal", "{bound:#}");
    assert_ne!(bound["outcome"]["id"], first["outcome"]["id"]);
    let said = bound["outcome"]["text"].as_str().unwrap_or_default();
    assert!(
        said.contains("argent : $2 USD · le montant que vous avez donné"),
        "{said}"
    );
    assert_eq!(journey.agent.len(), 2, "the author wrote and proposed once");
    let rerun = journey.step(3);
    assert_eq!(outcome_kind(rerun), "run_requested", "{rerun:#}");
    let report = rerun["outcome"]["text"].as_str().unwrap_or_default();
    assert!(
        report.ends_with(" · plafond $2 (enregistré à côté du workflow, hors de son fichier)"),
        "{report}"
    );
}
