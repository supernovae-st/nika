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
/// The words the « Let Nika choose » act records as the person's.
const LEFT_TO_NIKA: &str = "Je laisse le choix à Nika.";

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
            let mandate = asking_the_request(&mut test);
            // The first two defaults are kept with no author; the last pick reaches it, which
            // proposes the same bytes again so that the request's mandate continues.
            test.script.agent.push(call("propose", mandate));
            test.acts = vec![
                Act::Turn(TO_RUN),
                Act::AnswerProposalChoiceOf(0, 0, "hn"),
                Act::AnswerProposalChoiceOf(0, 1, "news"),
                Act::AnswerProposalChoiceOf(0, 2, "seat"),
            ];
        }
        "proposal_choice_delegated_keeps_the_request" => delegated_keeps_the_request(&mut test),
        "proposal_choice_grouped_delegation" => grouped_delegation(&mut test),
        _ => panic!("unknown proposal choice scenario {name}"),
    }
    test
}

/// A shown choice left to Nika by the host's own control, its other folder labelled with the
/// act's own words, between two native picks: the request's save and run are kept.
fn delegated_keeps_the_request(test: &mut Scenario) {
    let mandate = asking_the_request(test);
    let Step::Call { args, .. } = &mut test.script.agent[1] else {
        panic!("proposal")
    };
    // The other folder's label reads as the act's own words: only the act decides.
    args["open"][1]["options"][1]["label"] = json!(LEFT_TO_NIKA);
    // The default picked first (Hacker News) is kept with no author call; the act and the last
    // pick reach the author, which proposes the same bytes again on the request.
    test.script.agent.extend([
        say("I took the news folder, as recommended; you can change it."),
        call("propose", mandate),
    ]);
    test.acts = vec![
        Act::Turn(TO_RUN),
        Act::AnswerProposalChoiceOf(0, 0, "hn"),
        Act::DelegateProposalChoiceOf(0, 1),
        Act::AnswerProposalChoiceOf(0, 2, "seat"),
    ];
}

/// Both choices offer the keys `a` and `b`: the second is left to Nika first, then the first is
/// answered in words a label shares with that act.
fn grouped_delegation(test: &mut Scenario) {
    let Step::Call { args, .. } = &mut test.script.agent[1] else {
        panic!("proposal")
    };
    // Both choices offer the keys `a` and `b`, for distinct roles: the site, the folder.
    // The site's other option is labelled with the act's own words (a whole line is
    // compared to a label without its final stop).
    let collision = LEFT_TO_NIKA.trim_end_matches('.');
    for (choice, other) in [(0, collision), (1, "notes/")] {
        args["open"][choice]["options"][0]["key"] = json!("a");
        args["open"][choice]["options"][1]["key"] = json!("b");
        args["open"][choice]["options"][1]["label"] = json!(other);
    }
    test.script.agent.extend([
        say("I took the news folder, as recommended; you can change it."),
        say("The YC blog, noted: I revise the proposal before it is saved."),
    ]);
    // The second choice first, left to Nika; then the first, answered in words a label
    // shares with that act.
    test.acts = vec![
        test.acts[0],
        Act::DelegateProposalChoiceOf(0, 1),
        Act::AnswerProposalChoiceOf(0, 0, LEFT_TO_NIKA),
    ];
}

/// The proposal of `test` asks to save and run on the request's own words; the mandate it
/// cites, to propose again once the person answered its choices. The native answers are exact:
/// only the consent to the request is read.
fn asking_the_request(test: &mut Scenario) -> Value {
    let mandate = json!({"acts": ["save", "run"],
        "authorized_by": {"message": "u1", "excerpt": SAVE_THEN_RUN}});
    let Step::Call { args, .. } = &mut test.script.agent[1] else {
        panic!("proposal")
    };
    args["acts"] = mandate["acts"].clone();
    args["authorized_by"] = mandate["authorized_by"].clone();
    test.script.picks = vec!["change_then_save_and_run".into()];
    mandate
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
        2,
        "only the two preparations reach the author: each default picked is kept without it"
    );
    for at in 2..5 {
        let kept = journey.step(at);
        assert_eq!(outcome_kind(kept), "facts", "{kept:#}");
        assert!(step_text(kept).starts_with("défaut gardé"), "{kept:#}");
        assert_eq!(
            kept["shown"]["proposal"],
            journey.step(0)["shown"]["proposal"],
            "the proposal shown keeps its identity"
        );
    }
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
        2,
        "the kept default, reopening and stale input dispatch nothing"
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
    for at in 1..3 {
        assert_eq!(
            outcome_kind(journey.step(at)),
            "facts",
            "kept with no author"
        );
    }
    assert_eq!(
        journey.agent.len(),
        3,
        "two preparations and the last pick, which proposes the same bytes on the request"
    );
}

/// « Let Nika choose » on a shown choice is the person's own act, never words: it binds the
/// default that choice carries, as delegated, even when another option's label reads as the
/// act's words, and like a native default pick it keeps the request's save and run.
#[test]
fn letting_nika_choose_a_shown_choice_keeps_the_requests_save_and_run() {
    let journey = run("proposal_choice_delegated_keeps_the_request");
    for at in 0..3 {
        let step = journey.step(at);
        assert_ne!(outcome_kind(step), "run_requested", "{step:#}");
        assert!(work(step)["saved"].is_null(), "{step:#}");
    }
    let left = journey.step(2);
    let folder = binding(left, DIGEST).expect("the default folder is bound");
    let delegated = json!({"kind": "delegated", "message": "u3", "excerpt": LEFT_TO_NIKA,
        "question": "folder", "option": "news"});
    assert_eq!(folder["provenance"], delegated, "{folder:#}");
    assert!(
        binding(left, "./notes/digest.md").is_none(),
        "never the option whose label reads as the act"
    );
    assert!(
        (rows(left, "delegations").iter())
            .any(|d| d["message"] == "u3" && d["scope"] == "output_path"),
        "{:#}",
        work(left)
    );
    // The author is told of the act on its first call after the preparations: the default
    // picked before it was kept with no author call.
    let told = journey.agent[2].text();
    assert!(
        told.contains("left the choice to Nika with u3") && told.contains("recommended `news`"),
        "{told}"
    );
    let said = journey.step(3);
    assert_eq!(outcome_kind(said), "run_requested", "{said:#}");
    assert_ne!(waiting(said), "consent", "no second consent: {said:#}");
    let saved = work(said)["saved"]["workflow"].as_str().expect("saved");
    let landed = journey.dir.path().join("project").join(saved);
    let bytes = std::fs::read_to_string(landed).expect("the saved workflow");
    assert_eq!(bytes, candidate(journey.step(0)), "the bytes shown run");
    let readings = (journey.seen.iter())
        .filter(|seen| seen.kind == Kind::Pick)
        .collect::<Vec<_>>();
    assert_eq!(
        readings.len(),
        1,
        "the act is never read: only the consent is"
    );
    let consent = readings[0].text();
    assert!(
        consent.contains(SAVE_THEN_RUN) && consent.contains(r#""delegated":true"#),
        "{consent}"
    );
    assert!(
        !consent.contains("./notes/digest.md"),
        "the option not taken grants nothing: {consent}"
    );
    assert_eq!(
        outcome_kind(journey.step(1)),
        "facts",
        "the default picked first is kept with no author"
    );
    assert_eq!(
        journey.agent.len(),
        4,
        "two preparations, the act and the last pick; the default picked first needs no author"
    );
}

/// Two choices shown together offer the same keys for distinct roles. Left to Nika, the second
/// binds its own recommended default as delegated, cited to that act, and the first keeps
/// waiting; the first, then answered in the very words of the act (its option's legal label),
/// binds that option as offered, and the second's delegated default stays as it was.
#[test]
fn a_grouped_choice_left_to_nika_binds_only_its_own_default() {
    let journey = run("proposal_choice_grouped_delegation");
    no_effect(&journey);
    let left = journey.step(1);
    let delegated = json!({"kind": "delegated", "message": "u2", "excerpt": LEFT_TO_NIKA,
        "question": "folder", "option": "a"});
    let folder = binding(left, DIGEST).expect("the folder's default is bound");
    assert_eq!(folder["provenance"], delegated, "{folder:#}");
    for value in [HACKER_NEWS, YC_BLOG] {
        assert!(
            binding(left, value).is_none(),
            "the site still waits: {value}"
        );
    }
    let open = |step: &Value| -> Vec<String> {
        (work(step)["open_choices"].as_array().into_iter().flatten())
            .filter(|choice| choice["state"] == "open")
            .filter_map(|choice| choice["key"].as_str().map(str::to_owned))
            .collect()
    };
    assert_eq!(open(left), ["yc", "model"], "{:#}", work(left));
    let answered = journey.step(2);
    assert_eq!(
        provenance(answered, YC_BLOG),
        ("offered".into(), "u3".into()),
        "the label shared with the act is that option, as offered"
    );
    assert!(binding(answered, HACKER_NEWS).is_none());
    let kept = binding(answered, DIGEST).expect("the folder stays bound");
    assert_eq!(kept["provenance"], delegated, "{kept:#}");
    assert_eq!(open(answered), ["model"], "{:#}", work(answered));
    let told = journey.agent[3].text();
    let label = LEFT_TO_NIKA.trim_end_matches('.');
    let picked = format!("the person picked `b` ({label}) with u3");
    assert!(told.contains(&picked), "{told}");
    assert!(!told.contains("left the choice to Nika with u3"), "{told}");
    let readings = (journey.seen.iter())
        .filter(|seen| seen.kind == Kind::Pick)
        .count();
    assert_eq!(readings, 0, "neither the act nor a label's words is read");
    assert_eq!(journey.agent.len(), 4, "one preparation, two answers");
}
