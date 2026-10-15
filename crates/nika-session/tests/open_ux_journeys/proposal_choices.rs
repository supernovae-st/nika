// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native proposal-choice identity doors through the actual Session and scripted API wire.

use super::peer::{Step, call, say};
use super::scenarios::{Act, SEAT, Scenario, digest, resolution};
use super::*;

/// The news request that also asks to save the workflow and run it.
const TO_RUN: &str = concat!(
    "Récupère les dernières actualités tech de Y Combinator et TechCrunch, fais-moi un ",
    "résumé et sauvegarde-le en Markdown dans un dossier, puis enregistre le workflow et ",
    "lance-le.",
);
/// The words of that request that ask for the save and the run.
const SAVE_THEN_RUN: &str = "puis enregistre le workflow et lance-le.";

pub(crate) fn scenario(name: &str) -> Scenario {
    let mut test = super::scenarios::scenario("open_pick_other");
    test.script.agent.truncate(2);
    test.script.picks.clear();
    match name {
        "proposal_choice_protocol" => {
            let Step::Call { args, .. } = &mut test.script.agent[1] else {
                panic!("proposal")
            };
            args["open"][0]["options"][0]["key"] = json!("yes");
            args["open"][1]["options"][0]["key"] = json!("yes");
            args["open"][2]["options"][0]["key"] = json!("save & run");
            test.script.agent.extend([
                say("First selected."),
                say("Second selected."),
                say("Third selected."),
            ]);
            test.acts = vec![
                test.acts[0],
                Act::AnswerProposalChoiceElsewhere(0, 0, "yes"),
                Act::AnswerProposalChoiceOf(0, 0, "yes"),
                Act::AnswerProposalChoiceOf(0, 1, "yes"),
                Act::AnswerProposalChoiceOf(0, 2, "save & run"),
                Act::AnswerProposalChoiceOf(0, 0, "yes"),
            ];
        }
        "proposal_choice_reopen" => {
            test.history = true;
            test.script.agent.push(say("First selected."));
            test.acts = vec![
                test.acts[0],
                Act::AnswerProposalChoiceOf(0, 0, "hn"),
                Act::Reopen,
                Act::AnswerProposalChoiceOf(0, 1, "news"),
            ];
        }
        "proposal_choice_multiple" => {
            let Step::Call { args, .. } = &mut test.script.agent[1] else {
                panic!("proposal")
            };
            args["open"][0]["multi_select"] = json!(true);
            test.script
                .agent
                .extend([say("Both selected."), say("Folder selected.")]);
            test.script.picks = vec!["notes".into()];
            test.acts = vec![
                test.acts[0],
                Act::AnswerProposalChoiceOf(0, 0, r#"["blog","hn"]"#),
                Act::AnswerProposalChoiceOf(0, 1, "Je préfère le dossier notes, sans sauvegarder."),
            ];
        }
        "proposal_choice_revision" => {
            let Step::Call { args, .. } = &test.script.agent[1] else {
                panic!("proposal")
            };
            let mut next = args.clone();
            next["open"].as_array_mut().unwrap().remove(0);
            test.script.agent.extend([
                call("candidate_write", json!({
                    "source": digest(&[("yc_blog", YC_BLOG), ("techcrunch", TECHCRUNCH)], DIGEST),
                    "summary": "YC blog and TechCrunch",
                    "resolutions": [
                        {"value": YC_BLOG, "role": "read_source", "kind": "offered",
                         "message": "u2", "question": "yc", "option": "blog"},
                        resolution(TECHCRUNCH, "named", "read_source", "u1", "TechCrunch")
                    ]
                })),
                call("propose", next),
            ]);
            test.acts = vec![
                test.acts[0],
                Act::AnswerProposalChoiceOf(0, 0, "blog"),
                Act::AnswerProposalChoiceOf(0, 1, "news"),
            ];
        }
        "proposal_choice_defaults_keep_the_request" => {
            let mandate = json!({"acts": ["save", "run"],
                "authorized_by": {"message": "u1", "excerpt": SAVE_THEN_RUN}});
            let Step::Call { args, .. } = &mut test.script.agent[1] else {
                panic!("proposal")
            };
            args["acts"] = mandate["acts"].clone();
            args["authorized_by"] = mandate["authorized_by"].clone();
            test.script.agent.extend([
                say("Hacker News, as shown."),
                say("The news folder, as shown."),
                call("propose", mandate),
            ]);
            // The native picks are exact: only the consent to the request is read.
            test.script.picks = vec!["change_then_save_and_run".into()];
            test.acts = vec![
                Act::Turn(TO_RUN),
                Act::AnswerProposalChoiceOf(0, 0, "hn"),
                Act::AnswerProposalChoiceOf(0, 1, "news"),
                Act::AnswerProposalChoiceOf(0, 2, "seat"),
            ];
        }
        _ => panic!("unknown proposal choice scenario {name}"),
    }
    test
}

fn no_effect(journey: &Journey) {
    assert_eq!(journey.report["files"], json!([]));
    for step in journey.report["steps"].as_array().unwrap() {
        assert!(work(step)["saved"].is_null(), "{step:#}");
        assert!(work(step)["requested"].is_null(), "{step:#}");
    }
}

#[test]
fn proposal_choice_protocol_words_are_values_and_other_sessions_or_spent_ids_are_refused() {
    let journey = run("proposal_choice_protocol");
    no_effect(&journey);
    assert_eq!(
        journey.agent.len(),
        5,
        "only two preparations and three answers reach the author"
    );
    for at in [1, 5] {
        assert_eq!(outcome_kind(journey.step(at)), "refusal");
        assert_eq!(journey.step(at)["outcome"]["class"], "stale_revision");
    }
    let first = work(journey.step(2));
    assert_eq!(first["open_choices"].as_array().unwrap().len(), 2);
    assert_eq!(
        provenance(journey.step(2), HACKER_NEWS),
        ("offered".into(), "u2".into())
    );
    assert!(
        binding(journey.step(2), DIGEST).is_none(),
        "the other yes offer remains unanswered"
    );
    assert_eq!(
        provenance(journey.step(3), DIGEST),
        ("offered".into(), "u3".into())
    );
    assert_eq!(
        work(journey.step(3))["open_choices"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(waiting(journey.step(3)), "consent");
    assert_eq!(
        provenance(journey.step(4), SEAT),
        ("offered".into(), "u4".into())
    );
    // Work omits empty choice lists from its wire.
    assert_eq!(work(journey.step(4)).get("open_choices"), None);
    assert_eq!(waiting(journey.step(4)), "consent");
}

#[test]
fn proposal_choice_reopening_keeps_the_selection_without_live_authority() {
    let journey = run("proposal_choice_reopen");
    no_effect(&journey);
    assert_eq!(
        provenance(journey.step(2), HACKER_NEWS),
        ("offered".into(), "u2".into())
    );
    assert_eq!(work(journey.step(2)).get("open_choices"), None);
    assert!(journey.step(2)["shown"]["proposal"].is_null());
    assert_eq!(outcome_kind(journey.step(3)), "refusal");
    assert_eq!(journey.step(3)["outcome"]["class"], "stale_revision");
    assert_eq!(
        journey.agent.len(),
        3,
        "reopening and stale input dispatch nothing"
    );
}

#[test]
fn proposal_multiple_and_free_replies_bind_only_the_named_choice() {
    let journey = run("proposal_choice_multiple");
    no_effect(&journey);
    assert_eq!(journey.agent.len(), 4);
    for value in [HACKER_NEWS, YC_BLOG] {
        assert_eq!(
            provenance(journey.step(1), value),
            ("offered".into(), "u2".into())
        );
    }
    assert_eq!(
        work(journey.step(1))["open_choices"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        provenance(journey.step(2), "./notes/digest.md"),
        ("offered".into(), "u3".into())
    );
    let readings = journey
        .seen
        .iter()
        .filter(|seen| seen.kind == Kind::Pick)
        .collect::<Vec<_>>();
    assert_eq!(
        readings.len(),
        1,
        "only the free reply for the folder needs reading"
    );
    assert!(
        readings[0]
            .text()
            .contains("Je préfère le dossier notes, sans sauvegarder.")
    );
}

#[test]
fn a_native_alternative_revises_without_saving_and_old_proposal_choices_read_nothing() {
    let journey = run("proposal_choice_revision");
    no_effect(&journey);
    assert_eq!(
        outcome_kind(journey.step(1)),
        "proposal",
        "{:#}",
        journey.step(1)
    );
    assert_ne!(
        journey.step(0)["shown"]["proposal"],
        journey.step(1)["shown"]["proposal"]
    );
    assert_eq!(
        provenance(journey.step(1), YC_BLOG),
        ("offered".into(), "u2".into())
    );
    assert_eq!(outcome_kind(journey.step(2)), "refusal");
    assert_eq!(journey.step(2)["outcome"]["class"], "stale_revision");
    assert_eq!(journey.agent.len(), 4, "the stale answer calls no model");
}

/// The request asked to save and run, and its proposal showed choices with their defaults: once
/// the person picks each default natively, the same proposal, proposed again on the request, is
/// saved and run with no second consent, and the consent reading sees those picks.
#[test]
fn native_default_picks_keep_the_requests_save_and_run() {
    let journey = run("proposal_choice_defaults_keep_the_request");
    for at in 0..3 {
        let step = journey.step(at);
        assert_ne!(outcome_kind(step), "run_requested", "{step:#}");
        assert!(work(step)["saved"].is_null(), "{step:#}");
    }
    let said = journey.step(3);
    assert_eq!(outcome_kind(said), "run_requested", "{said:#}");
    assert_ne!(waiting(said), "consent", "no second consent: {said:#}");
    let saved = work(said)["saved"]["workflow"].as_str().expect("saved");
    let files = journey.report["files"].as_array().unwrap();
    assert!(
        files.iter().any(|f| f.as_str() == Some(saved)),
        "{saved} landed: {files:?}"
    );
    let landed = journey.dir.path().join("project").join(saved);
    let bytes = std::fs::read_to_string(landed).expect("the saved workflow");
    assert_eq!(bytes, candidate(journey.step(0)), "the bytes shown run");
    let readings = (journey.seen.iter())
        .filter(|seen| seen.kind == Kind::Pick)
        .collect::<Vec<_>>();
    assert_eq!(readings.len(), 1, "only the consent is read");
    let consent = readings[0].text();
    assert!(consent.contains(SAVE_THEN_RUN) && consent.contains("Le modèle de cette Session"));
    assert_eq!(journey.agent.len(), 5, "two preparations and three answers");
}
