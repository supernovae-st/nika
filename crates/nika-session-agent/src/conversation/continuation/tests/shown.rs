// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A proposal that shows choices waits for the person. Once they pick, with the native choice,
//! the default it carries for each of them, its very bytes are proposed again on the request it
//! cited: the same revision, scope, mandate and acts, and no other line since.

use super::*;

/// The original request: create, save then run, the technical details left to Nika.
const REQUEST: &str = concat!(
    "Crée le rapport des ventes payées en EUR. ",
    "Enregistre le workflow sous rapport.nika puis lance-le. ",
    "Tu peux choisir les détails techniques nécessaires.",
);
/// The words of the request that ask for the save and the run.
const SAVE_AND_RUN: &str = "Enregistre le workflow sous rapport.nika puis lance-le.";
const QUESTION: &str = "Quel modèle DeepSeek rédige le rapport ?";
const MODEL: &str = "deepseek/deepseek-flash";
const FLASH: &str = "deepseek/deepseek-flash (celui de cette session)";
const PRO: &str = "deepseek/deepseek-v4-pro";
const OUTPUT: &str = "out/rapport.md";
const SCOPE: &str = "S1";
const RUN: [&str; 2] = ["save", "run"];

/// The run model, shown beside the candidate with the Session's model as its default.
fn run_model() -> AskedQuestion {
    let offer = |key: &str, label: &str, default: bool, model: &str| {
        let value = OfferValue::new(ValueRole::RunModel, model, None);
        Offer::new(key, label, default).with_values(vec![value])
    };
    AskedQuestion::new(
        QuestionId::new("run_model".into(), &Arc::new(Incarnation)),
        "run_model",
        QUESTION,
    )
    .with_options(
        vec![
            offer("deepseek_flash", FLASH, true, MODEL),
            offer("deepseek_v4_pro", PRO, false, PRO),
        ],
        true,
        false,
    )
    .with_role(ValueRole::RunModel)
}

/// The report's path, shown with its derived default.
fn output() -> AskedQuestion {
    let offer = |key: &str, path: &str, default: bool| {
        let value = OfferValue::new(ValueRole::OutputPath, path, None);
        Offer::new(key, path, default).with_values(vec![value])
    };
    AskedQuestion::new(
        QuestionId::new("output".into(), &Arc::new(Incarnation)),
        "output",
        "Où écrire le rapport ?",
    )
    .with_options(
        vec![
            offer("out", OUTPUT, true),
            offer("reports", "reports/rapport.md", false),
        ],
        true,
        false,
    )
    .with_role(ValueRole::OutputPath)
}

/// `acts` on the person's line `u1`, quoting its `excerpt`.
fn citing(acts: &[&str], excerpt: &str) -> Value {
    json!({"acts": acts, "authorized_by": {"message": "u1", "excerpt": excerpt}})
}

/// The candidate carrying the run model and the report path the choices show as defaults.
fn prepare_report(conversation: &mut Conversation, citations: &Citations) {
    let source = format!("nika: rapport\nmodel: {MODEL}\n# writes {OUTPUT}\ntasks: {{}}\n");
    let texts = (source, "Rapport".to_owned());
    let reply = conversation.write(citations, texts, (&[], &[]), &mut |_| Ok(()));
    assert!(!reply.is_error, "{reply:?}");
}

/// Line 1 is `request`; revision 2 is written, as in the replay; at line 2 the proposal asking
/// `acts` on it shows `choices` with their defaults, and waits for the person.
fn showing(
    request: &str,
    acts: &[&str],
    choices: Vec<AskedQuestion>,
) -> (Conversation, Citations, Vec<AskedQuestion>) {
    let mut conversation = Conversation::default();
    let mut citations = Citations::default();
    citations.record(1, Some(("u1".into(), request.into())));
    prepare_report(&mut conversation, &citations);
    prepare_report(&mut conversation, &citations);
    citations.record(2, None);
    let open = (SCOPE.into(), choices.clone());
    let waits = conversation.propose(&citations, &citing(acts, request), open);
    assert!(waits.is_err_and(|why| why.contains("waits for their consent")));
    assert_eq!(conversation.consumed_through, 0);
    (conversation, citations, choices)
}

/// The proposal of [`showing`] with the run model as its one choice.
fn shown(request: &str, acts: &[&str]) -> (Conversation, Citations, AskedQuestion) {
    let (conversation, citations, mut choices) = showing(request, acts, vec![run_model()]);
    (conversation, citations, choices.remove(0))
}

/// The person's line `cite` at `at`: the native answer `text` to `choice`.
fn pick(
    conversation: &mut Conversation,
    citations: &mut Citations,
    choice: &AskedQuestion,
    (at, cite, text): (u64, &str, &str),
) {
    citations.record(at, Some((cite.into(), text.into())));
    conversation.answered_one_by(&choice.id, cite, text, &BTreeMap::new());
}

/// The revision shown, proposed again with `acts` on the request's save-and-run words, `scope`.
fn again(
    conversation: &mut Conversation,
    citations: &Citations,
    (acts, scope): (&[&str], &str),
) -> Result<Option<Acts>, String> {
    let request = citing(acts, SAVE_AND_RUN);
    conversation.propose(citations, &request, (scope.into(), vec![]))
}

const SAVE_RUN_REVISED: Acts = Acts {
    save: true,
    run: true,
    revised: true,
};

#[test]
fn the_shown_revision_keeps_its_mandate_once_the_person_picks_its_default() {
    let (mut conversation, mut citations, choice) = shown(REQUEST, &RUN);
    let bytes = conversation
        .candidate()
        .map(|c| (c.number, c.source.clone()));
    assert_eq!(bytes.as_ref().map(|(number, _)| *number), Some(2));
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (4, "u2", "deepseek_flash"),
    );
    let receipt = &conversation.continuations["u2"];
    assert_eq!(receipt.request, 0);
    assert_eq!(receipt.witness, Witness::of(b"deepseek_flash").0);
    assert_eq!(receipt.meaning["key"], "run_model");
    assert_eq!(
        receipt.meaning["selected_offers"][0]["key"],
        "deepseek_flash"
    );
    let proposed = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert_eq!(proposed, Ok(Some(SAVE_RUN_REVISED)));
    assert_eq!(
        conversation.consumed_through, 1,
        "the request, not the pick, is consumed"
    );
    let after = conversation
        .candidate()
        .map(|c| (c.number, c.source.clone()));
    assert_eq!(after, bytes, "nothing was written");
    let text = conversation.authorization_text(&citations, "u1").unwrap();
    for phrase in [REQUEST, "u2", QUESTION, FLASH] {
        assert!(text.contains(phrase), "missing {phrase}: {text}");
    }
    assert!(!text.contains(PRO), "the option not picked grants nothing");
    assert!(Pick::consented(
        &text,
        SAVE_RUN_REVISED,
        choice.id.clone(),
        |prompt| {
            assert!(prompt.contains(SAVE_AND_RUN) && prompt.contains(FLASH));
            Some("change_then_save_and_run".into())
        }
    ));
    let twice = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert!(twice.is_err(), "one line, one proposal: {twice:?}");
}

#[test]
fn every_choice_shown_answered_by_its_default_keeps_the_mandate() {
    let (mut conversation, mut citations, choices) =
        showing(REQUEST, &RUN, vec![run_model(), output()]);
    pick(
        &mut conversation,
        &mut citations,
        &choices[0],
        (4, "u2", "deepseek_flash"),
    );
    pick(
        &mut conversation,
        &mut citations,
        &choices[1],
        (5, "u3", "out"),
    );
    let proposed = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert_eq!(proposed, Ok(Some(SAVE_RUN_REVISED)));
}

#[test]
fn the_shown_proposal_bounds_the_acts_proposed_again() {
    let save = ["save"];
    let (mut conversation, mut citations, choice) = shown(REQUEST, &save);
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (4, "u2", "deepseek_flash"),
    );
    let more = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert!(more.is_err(), "the run was never shown: {more:?}");
    let (mut conversation, mut citations, choice) = shown(REQUEST, &save);
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (4, "u2", "deepseek_flash"),
    );
    assert_eq!(
        again(&mut conversation, &citations, (&save, SCOPE)),
        Ok(Some(Acts {
            run: false,
            ..SAVE_RUN_REVISED
        }))
    );
}

#[test]
fn the_consent_reading_still_reads_a_request_to_save_only() {
    let save_only = "Crée le rapport des ventes et enregistre-le sous rapport.nika.";
    let (mut conversation, mut citations, choice) = shown(save_only, &RUN);
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (4, "u2", "deepseek_flash"),
    );
    let request = citing(&RUN, save_only);
    let proposed = conversation.propose(&citations, &request, (SCOPE.into(), vec![]));
    let acts = proposed.unwrap().unwrap();
    let text = conversation.authorization_text(&citations, "u1").unwrap();
    let read = |prompt: &str| {
        assert!(prompt.contains(save_only) && prompt.contains(FLASH));
        Some("change_then_save".to_owned())
    };
    assert!(!Pick::consented(&text, acts, choice.id.clone(), read));
    assert!(Pick::consented(
        &text,
        Acts { run: false, ..acts },
        choice.id,
        read
    ));
}

#[test]
fn no_pick_after_the_shown_choice_waits_for_consent() {
    let (mut conversation, citations, _) = shown(REQUEST, &RUN);
    let unanswered = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert!(unanswered.is_err(), "{unanswered:?}");
    assert_eq!(conversation.consumed_through, 0);
    // The refused proposal replaced the one shown with an empty set: retrying proves nothing.
    let retried = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert!(retried.is_err(), "{retried:?}");
    assert_eq!(conversation.consumed_through, 0);
}

#[test]
fn an_unanswered_choice_shown_waits_for_consent() {
    let (mut conversation, mut citations, choices) =
        showing(REQUEST, &RUN, vec![run_model(), output()]);
    pick(
        &mut conversation,
        &mut citations,
        &choices[0],
        (4, "u2", "deepseek_flash"),
    );
    let half = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert!(half.is_err(), "the path was never answered: {half:?}");
    assert_eq!(conversation.consumed_through, 0);
    assert!(
        !conversation.answered.contains_key("output"),
        "never answered"
    );
    let retried = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert!(retried.is_err(), "{retried:?}");
    assert_eq!(conversation.consumed_through, 0);
}

#[test]
fn another_option_than_the_default_waits_for_the_rewrite() {
    let (mut conversation, mut citations, choice) = shown(REQUEST, &RUN);
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (4, "u2", "deepseek_v4_pro"),
    );
    // The refused proposal clears the pick it waits on: proposing again still refuses.
    for _ in 0..2 {
        let stale = again(&mut conversation, &citations, (&RUN, SCOPE));
        assert!(stale.is_err(), "it still carries the default: {stale:?}");
    }
}

#[test]
fn a_free_line_read_as_the_default_waits_for_consent() {
    let (mut conversation, mut citations, choice) = shown(REQUEST, &RUN);
    let said = "Garde Flash, comme d'habitude";
    citations.record(4, Some(("u2".into(), said.into())));
    let read = BTreeMap::from([("run_model".to_owned(), Pick::Offer("deepseek_flash".into()))]);
    conversation.answered_one_by(&choice.id, "u2", said, &read);
    assert!(conversation.open().is_empty(), "bound by the reading");
    let read_only = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert!(read_only.is_err(), "{read_only:?}");
    assert_eq!(conversation.authorization_text(&citations, "u1"), None);
}

#[test]
fn a_withdrawal_after_the_default_pick_waits_for_consent() {
    let (mut conversation, mut citations, choice) = shown(REQUEST, &RUN);
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (4, "u2", "deepseek_flash"),
    );
    citations.record(7, Some(("u3".into(), "Finalement, ne lance rien".into())));
    let withdrawn = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert!(withdrawn.is_err(), "{withdrawn:?}");
    assert_eq!(conversation.authorization_text(&citations, "u1"), None);
    assert_eq!(conversation.consumed_through, 0);
}

#[test]
fn a_changed_scope_waits_for_consent() {
    let (mut conversation, mut citations, choice) = shown(REQUEST, &RUN);
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (4, "u2", "deepseek_flash"),
    );
    let other = again(&mut conversation, &citations, (&RUN, "S2"));
    assert!(other.is_err(), "{other:?}");
    assert_eq!(conversation.consumed_through, 0);
}

#[test]
fn a_revision_written_before_the_pick_waits_for_consent() {
    let (mut conversation, mut citations, choice) = shown(REQUEST, &RUN);
    citations.record(3, None);
    prepare_report(&mut conversation, &citations);
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (4, "u2", "deepseek_flash"),
    );
    let unseen = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert!(unseen.is_err(), "{unseen:?}");
}

#[test]
fn a_replayed_pick_after_the_choice_closed_waits_for_consent() {
    let (mut conversation, mut citations, choice) = shown(REQUEST, &RUN);
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (4, "u2", "deepseek_flash"),
    );
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (7, "u3", "deepseek_flash"),
    );
    assert!(!conversation.continuations.contains_key("u3"), "no receipt");
    let replayed = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert!(replayed.is_err(), "{replayed:?}");
    assert_eq!(conversation.authorization_text(&citations, "u1"), None);
    assert_eq!(conversation.consumed_through, 0);
}

#[test]
fn a_default_the_candidate_does_not_carry_waits_for_consent() {
    let mut conversation = Conversation::default();
    let mut citations = Citations::default();
    citations.record(1, Some(("u1".into(), REQUEST.into())));
    prepare(&mut conversation, &citations);
    citations.record(2, None);
    let choice = run_model();
    let request = citing(&RUN, REQUEST);
    let waits = conversation.propose(&citations, &request, (SCOPE.into(), vec![choice.clone()]));
    assert!(waits.is_err(), "{waits:?}");
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (4, "u2", "deepseek_flash"),
    );
    let missing = again(&mut conversation, &citations, (&RUN, SCOPE));
    assert!(missing.is_err(), "the bytes do not carry it: {missing:?}");
}

#[test]
fn an_earlier_requests_mandate_cannot_cross_the_shown_choice() {
    let earlier = "Crée un autre workflow, enregistre-le et lance-le.";
    let mut conversation = Conversation::default();
    let mut citations = Citations::default();
    citations.record(1, Some(("u1".into(), earlier.into())));
    citations.record(3, Some(("u2".into(), REQUEST.into())));
    let new = json!({"message": "u2", "excerpt": REQUEST});
    assert!(!conversation.replace(&citations, &new).is_error);
    prepare_report(&mut conversation, &citations);
    citations.record(4, None);
    let choice = run_model();
    let request = json!({"acts": RUN,
        "authorized_by": {"message": "u2", "excerpt": SAVE_AND_RUN}});
    let waits = conversation.propose(&citations, &request, (SCOPE.into(), vec![choice.clone()]));
    assert!(waits.is_err(), "{waits:?}");
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (6, "u3", "deepseek_flash"),
    );
    let older = citing(&RUN, earlier);
    let crossed = conversation.propose(&citations, &older, (SCOPE.into(), vec![]));
    assert!(crossed.is_err(), "{crossed:?}");
    assert_eq!(conversation.consumed_through, 0);
}

#[test]
fn a_reopened_conversation_does_not_replay_the_shown_mandate() {
    let (mut conversation, mut citations, choice) = shown(REQUEST, &RUN);
    pick(
        &mut conversation,
        &mut citations,
        &choice,
        (4, "u2", "deepseek_flash"),
    );
    let mut reopened = Conversation::restored(&conversation.kept()).unwrap();
    let replayed = again(&mut reopened, &citations, (&RUN, SCOPE));
    assert!(replayed.is_err(), "{replayed:?}");
}
