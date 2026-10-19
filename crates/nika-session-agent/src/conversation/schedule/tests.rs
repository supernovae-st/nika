// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The schedule a proposal cites from what the person picked: the label of the offer their answer
//! picked holds their words, and a zone an offer they picked binds is theirs, so the door that
//! refuses to show it again as settled and the schedule agree.

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_session_change::outcome::{Incarnation, QuestionId};
use nika_session_change::work::ProvenanceKind;
use serde_json::{Value, json};

use crate::conversation::{Citations, Conversation, Pick};

/// The person's zone, shown as a proposal's choice.
const ZONE: &str = "Indian/Mauritius";
/// The request of the zone journeys, with the words that say when.
const WEEKLY: &str = "chaque lundi à 9h, résume ./notes.md dans ./out/resume.md";
/// The words of `WEEKLY` that say when.
const WHEN: &str = "chaque lundi à 9h";
/// The other zone the choice offers.
const PARIS: &str = "Europe/Paris";
/// The label of the cadence offer the person picks.
const DAILY: &str = "Tous les jours à 9h";
/// A line that leaves a choice to Nika.
const AS_YOU_LIKE: &str = "comme tu veux";

/// The cadence asked with its offers and no default, free words beside them.
fn cadence(multi_select: bool) -> Value {
    json!({"key": "cadence", "question": "À quelle fréquence ?", "free_text": true,
        "multi_select": multi_select,
        "options": [
            {"key": "daily", "label": DAILY},
            {"key": "weekdays", "label": "Du lundi au vendredi à 9h"},
            {"key": "sunday", "label": "Le dimanche à 10h"}]})
}

/// The zone as a choice whose default is the option holding `default` (none for another word).
fn zone_choice_defaulting(default: &str) -> Value {
    let mut choice = json!({"key": "timezone", "question": "Quel fuseau horaire ?",
        "role": "value",
        "options": [
            {"key": "mu", "label": "Maurice (Indian/Mauritius)",
             "values": [{"role": "value", "value": ZONE}]},
            {"key": "paris", "label": "Paris (Europe/Paris)",
             "values": [{"role": "value", "value": PARIS}]}]});
    for option in choice["options"].as_array_mut().into_iter().flatten() {
        let recommended = option["values"][0]["value"] == default;
        option["recommended"] = json!(recommended);
    }
    choice
}

/// The zone shown as a choice whose default is the person's.
fn zone_choice() -> Value {
    zone_choice_defaulting(ZONE)
}

/// `propose`'s schedule: the words cited from a line, and the zone they follow.
fn schedule(message: &str, excerpt: &str, zone: &str) -> Value {
    json!({"schedule": {"message": message, "excerpt": excerpt, "timezone": zone}})
}

/// What the propose door reads of `args`: the choices it shows in `open`, then its schedule.
fn read(
    conversation: &Conversation,
    lines: &Citations,
    args: &Value,
) -> Result<Option<(String, String)>, String> {
    let asker = Arc::new(Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let open =
        conversation.open_choices(lines, args, Some("c9"), (&mut mint, &mut |_: &str| None))?;
    conversation.schedule(lines, args, &open)
}

/// The words and the zone a schedule shows.
fn shown(words: &str, zone: &str) -> (String, String) {
    (words.to_owned(), zone.to_owned())
}

/// A request that does not say when it runs, the cadence asked, and the person's line `u2`
/// answering it: picked by protocol, else as the Session read it (`reading`).
fn answered(line: &str, reading: Option<Pick>, multi_select: bool) -> (Conversation, Citations) {
    let asker = Arc::new(Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let mut conversation = Conversation::default();
    let mut lines = Citations::default();
    lines.record(
        1,
        Some(("u1".into(), "dis moi la météo en markdown".into())),
    );
    let questions = json!({"questions": [cadence(multi_select)]});
    let posed = conversation.ask(
        &lines,
        &questions,
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(posed.ends_turn, "{posed:?}");
    lines.record(4, Some(("u2".into(), line.into())));
    let read: BTreeMap<String, Pick> = (reading.into_iter())
        .map(|pick| ("cadence".to_owned(), pick))
        .collect();
    conversation.answered_by("u2", line, &read);
    (conversation, lines)
}

/// The cadence cited from `message` with `excerpt`, its zone shown as a choice with its default.
fn cadence_cited(message: &str, excerpt: &str) -> Value {
    let mut args = schedule(message, excerpt, ZONE);
    args["open"] = json!([zone_choice()]);
    args
}

#[test]
fn a_cadence_the_person_picked_is_cited_from_the_label_they_picked() {
    // A native list sends the picked offer's key as the person's line.
    let (conversation, lines) = answered("daily", None, false);
    let args = cadence_cited("u2", DAILY);
    assert_eq!(
        read(&conversation, &lines, &args),
        Ok(Some(shown(DAILY, ZONE)))
    );
    let args = cadence_cited("u2", "jours à 9h");
    assert_eq!(
        read(&conversation, &lines, &args),
        Ok(Some(shown("jours à 9h", ZONE)))
    );
    // A sentence the Session read as that pick cites the same label.
    let (conversation, lines) = answered("le premier", Some(Pick::Offer("daily".into())), false);
    let args = cadence_cited("u2", DAILY);
    assert_eq!(
        read(&conversation, &lines, &args),
        Ok(Some(shown(DAILY, ZONE)))
    );
}

#[test]
fn words_no_picked_label_holds_are_never_the_persons() {
    let law = "cite the person's line and their own words that say when it runs";
    let (conversation, lines) = answered("daily", None, false);
    for (message, excerpt) in [
        // The label of an offer the person did not pick.
        ("u2", "Du lundi au vendredi à 9h"),
        // Words no label holds.
        ("u2", "Tous les jours à 10h"),
        // The picked label, cited from a line that picked nothing.
        ("u1", DAILY),
        ("u9", DAILY),
        ("u2", "  "),
    ] {
        let refused = read(&conversation, &lines, &cadence_cited(message, excerpt));
        assert!(
            refused.as_ref().is_err_and(|why| why.contains(law)),
            "{message} « {excerpt} »: {refused:?}"
        );
    }
    // A line that left the choice to Nika, or picked none of the offers, picks no label.
    for (line, reading) in [
        ("comme tu veux", Pick::Delegated),
        ("je ne sais pas", Pick::Nothing),
        ("hmm", Pick::Unread),
    ] {
        let (conversation, lines) = answered(line, Some(reading), false);
        let refused = read(&conversation, &lines, &cadence_cited("u2", DAILY));
        assert!(refused.is_err_and(|why| why.contains(law)), "{line}");
    }
}

#[test]
fn each_offer_of_a_multiple_pick_holds_the_persons_words() {
    let (conversation, lines) = answered(r#"["daily","sunday"]"#, None, true);
    for picked in [DAILY, "Le dimanche à 10h"] {
        let args = cadence_cited("u2", picked);
        assert_eq!(
            read(&conversation, &lines, &args),
            Ok(Some(shown(picked, ZONE)))
        );
    }
    let unpicked = read(
        &conversation,
        &lines,
        &cadence_cited("u2", "Du lundi au vendredi à 9h"),
    );
    assert!(unpicked.is_err(), "{unpicked:?}");
}

/// The weekly request written and proposed with its zone shown as a choice, then the person's
/// line `u2` answering that choice.
fn zone_answered(line: &str) -> (Conversation, Citations) {
    zone_answered_to(ZONE, line, None)
}

/// The weekly request written and proposed with its zone shown as a choice whose default holds
/// `default`, then the person's line `u2` answering that choice: picked by protocol, else as the
/// Session read it (`reading`).
fn zone_answered_to(default: &str, line: &str, reading: Option<Pick>) -> (Conversation, Citations) {
    let mut conversation = Conversation::default();
    let mut lines = Citations::default();
    lines.record(1, Some(("u1".into(), WEEKLY.into())));
    let source = "nika: weekly\ntasks: {}\n".to_owned();
    let texts = (source, "résume les notes".to_owned());
    let written = conversation.write(&lines, texts, (&[], &[]), &mut |_| Ok(()));
    assert!(!written.is_error, "{written:?}");
    let mut args = schedule("u1", WHEN, default);
    args["open"] = json!([zone_choice_defaulting(default)]);
    let asker = Arc::new(Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let open = conversation
        .open_choices(&lines, &args, Some("c1"), (&mut mint, &mut |_: &str| None))
        .expect("the zone is shown with its default");
    assert_eq!(
        conversation.schedule(&lines, &args, &open),
        Ok(Some(shown(WHEN, default)))
    );
    let proposed = conversation.propose(&lines, &args, ("S0".to_owned(), open));
    assert_eq!(proposed, Ok(None));
    lines.record(4, Some(("u2".into(), line.into())));
    let read: BTreeMap<String, Pick> = (reading.into_iter())
        .map(|pick| ("timezone".to_owned(), pick))
        .collect();
    let said = conversation.answered_by("u2", line, &read);
    assert!(said.iter().any(|said| said.contains("with u2")), "{said:?}");
    (conversation, lines)
}

#[test]
fn a_zone_the_person_picked_is_theirs_and_never_shown_again() {
    let (conversation, lines) = zone_answered("mu");
    // Read back after a reopen, the binding their pick made still says so.
    let reopened = Conversation::restored(&conversation.kept()).expect("a kept conversation");
    for conversation in [&conversation, &reopened] {
        // Proposed again without the choice: the zone is the one they picked.
        let again = schedule("u1", WHEN, ZONE);
        assert_eq!(
            read(conversation, &lines, &again),
            Ok(Some(shown(WHEN, ZONE)))
        );
        // Shown again, it is refused as settled: the two doors agree.
        let mut shown_again = again.clone();
        shown_again["open"] = json!([zone_choice()]);
        let settled = read(conversation, &lines, &shown_again);
        assert!(
            settled
                .as_ref()
                .is_err_and(|why| why.contains("already settled by the person")),
            "{settled:?}"
        );
    }
}

#[test]
fn a_zone_the_person_asks_to_change_is_theirs_no_more() {
    let law = "is neither in the person's words nor the default of a choice";
    let (mut conversation, mut lines) = zone_answered("mu");
    let change = "change le fuseau horaire";
    lines.record(7, Some(("u3".into(), change.into())));
    let mut choice = zone_choice();
    choice["options"][0]["recommended"] = json!(false);
    choice["reopens"] = json!({"message": "u3", "excerpt": change});
    let asker = Arc::new(Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let questions = json!({"questions": [choice]});
    let posed = conversation.ask(
        &lines,
        &questions,
        Some("c2"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(posed.ends_turn, "{posed:?}");
    // Asked again on their words, the zone they picked before is no longer bound.
    let refused = read(&conversation, &lines, &schedule("u1", WHEN, ZONE));
    assert!(refused.is_err_and(|why| why.contains(law)));
}

#[test]
fn a_zone_the_person_did_not_pick_is_still_refused() {
    let law = "is neither in the person's words nor the default of a choice";
    let (conversation, lines) = zone_answered("paris");
    assert_eq!(
        read(&conversation, &lines, &schedule("u1", WHEN, PARIS)),
        Ok(Some(shown(WHEN, PARIS)))
    );
    // The default they were shown and did not pick, and a zone no offer or word of theirs holds.
    for zone in [ZONE, "Asia/Dubai"] {
        let refused = read(&conversation, &lines, &schedule("u1", WHEN, zone));
        assert!(refused.is_err_and(|why| why.contains(law)), "{zone}");
    }
}

/// The zone the person picked, proposed again without its choice and shown: the values that
/// proposal bound are the ones the person saw.
fn zone_shown() -> (Conversation, Citations) {
    let (mut conversation, lines) = zone_answered("mu");
    let again = schedule("u1", WHEN, ZONE);
    assert_eq!(
        read(&conversation, &lines, &again),
        Ok(Some(shown(WHEN, ZONE)))
    );
    let proposed = conversation.propose(&lines, &again, ("S1".to_owned(), Vec::new()));
    assert_eq!(proposed, Ok(None));
    (conversation, lines)
}

/// A revision the person's line `cite` asked for, whose bytes never hold the zone.
fn revise(conversation: &mut Conversation, lines: &mut Citations, (cite, at): (&str, u64)) {
    lines.record(
        at,
        Some((cite.into(), "ajoute la date en haut du résumé".into())),
    );
    let source = format!("nika: weekly\n# revision for {cite}\ntasks: {{}}\n");
    let texts = (source, "avec la date".to_owned());
    let written = conversation.write(lines, texts, (&[], &[]), &mut |_| Ok(()));
    assert!(!written.is_error, "{written:?}");
}

/// The zone binding the person's pick made, as `(kind, message)`.
fn zone_binding(conversation: &Conversation, zone: &str) -> Option<(ProvenanceKind, String)> {
    (conversation.bindings().iter())
        .find(|binding| binding.value == zone)
        .map(|binding| (binding.provenance.kind, binding.provenance.message.clone()))
}

#[test]
fn a_revision_keeps_the_zone_a_shown_schedule_carried_beside_it() {
    let (mut conversation, mut lines) = zone_shown();
    revise(&mut conversation, &mut lines, ("u3", 7));
    // The bytes never hold the zone, yet the revision drops nothing the person saw bound.
    assert_eq!(conversation.dropped(), Vec::<String>::new());
    let picked = Some((ProvenanceKind::Offered, "u2".to_owned()));
    assert_eq!(zone_binding(&conversation, ZONE), picked);
    // Proposed again without the choice, the zone is theirs, and never shown again.
    let again = schedule("u1", WHEN, ZONE);
    assert_eq!(
        read(&conversation, &lines, &again),
        Ok(Some(shown(WHEN, ZONE)))
    );
    let mut shown_again = again.clone();
    shown_again["open"] = json!([zone_choice()]);
    let settled = read(&conversation, &lines, &shown_again);
    assert!(settled.is_err_and(|why| why.contains("already settled by the person")));
    // So does a revision after a reopen.
    let mut reopened = Conversation::restored(&conversation.kept()).expect("a kept conversation");
    revise(&mut reopened, &mut lines, ("u4", 10));
    assert_eq!(reopened.dropped(), Vec::<String>::new());
    assert_eq!(zone_binding(&reopened, ZONE), picked);
}

#[test]
fn a_revision_never_keeps_a_zone_the_person_changed() {
    let (mut conversation, mut lines) = zone_shown();
    let change = "change le fuseau horaire";
    lines.record(7, Some(("u3".into(), change.into())));
    let mut choice = zone_choice();
    choice["options"][0]["recommended"] = json!(false);
    choice["reopens"] = json!({"message": "u3", "excerpt": change});
    let asker = Arc::new(Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let questions = json!({"questions": [choice]});
    let posed = conversation.ask(
        &lines,
        &questions,
        Some("c3"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(posed.ends_turn, "{posed:?}");
    lines.record(10, Some(("u4".into(), "paris".into())));
    conversation.answered_by("u4", "paris", &BTreeMap::new());
    revise(&mut conversation, &mut lines, ("u5", 13));
    // The zone they saw before is not brought back beside the revision: their answer replaced it.
    assert_eq!(zone_binding(&conversation, ZONE), None);
    let paris = Some((ProvenanceKind::Offered, "u4".to_owned()));
    assert_eq!(zone_binding(&conversation, PARIS), paris);
    let refused = read(&conversation, &lines, &schedule("u1", WHEN, ZONE));
    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn a_zone_left_to_nika_as_the_default_shown_is_theirs_and_never_shown_again() {
    let (conversation, lines) = zone_answered_to(ZONE, AS_YOU_LIKE, Some(Pick::Delegated));
    let left = Some((ProvenanceKind::Delegated, "u2".to_owned()));
    assert_eq!(zone_binding(&conversation, ZONE), left);
    // Proposed again without the choice: the zone is the default they left to Nika.
    let again = schedule("u1", WHEN, ZONE);
    assert_eq!(
        read(&conversation, &lines, &again),
        Ok(Some(shown(WHEN, ZONE)))
    );
    // Shown again, it is refused as settled: the two doors agree on a delegation too.
    let mut shown_again = again.clone();
    shown_again["open"] = json!([zone_choice()]);
    let settled = read(&conversation, &lines, &shown_again);
    assert!(settled.is_err_and(|why| why.contains("already settled by the person")));
}

/// The weekly request with its zone asked with no default, the person's line `u2` leaving it to
/// Nika, and the author's candidate binding the zone it chose as delegated on those words.
fn zone_chosen_by_the_author() -> (Conversation, Citations) {
    let mut conversation = Conversation::default();
    let mut lines = Citations::default();
    lines.record(1, Some(("u1".into(), WEEKLY.into())));
    let asker = Arc::new(Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let questions = json!({"questions": [zone_choice_defaulting("none")]});
    let posed = conversation.ask(
        &lines,
        &questions,
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(posed.ends_turn, "{posed:?}");
    lines.record(4, Some(("u2".into(), AS_YOU_LIKE.into())));
    let read = BTreeMap::from([("timezone".to_owned(), Pick::Delegated)]);
    conversation.answered_by("u2", AS_YOU_LIKE, &read);
    let rows = [
        json!({"value": ZONE, "kind": "delegated", "role": "value", "message": "u2",
        "excerpt": AS_YOU_LIKE}),
    ];
    let texts = (
        "nika: weekly\ntasks: {}\n".to_owned(),
        "résume les notes".to_owned(),
    );
    let written = conversation.write(&lines, texts, (&rows, &[]), &mut |_| Ok(()));
    assert!(!written.is_error, "{written:?}");
    (conversation, lines)
}

#[test]
fn a_zone_left_to_nika_is_theirs_only_as_the_default_they_were_shown() {
    let law = "is neither in the person's words nor the default of a choice";
    // The choice they left to Nika recommended Paris: Maurice, offered beside it, is not theirs.
    let (conversation, lines) = zone_answered_to(PARIS, AS_YOU_LIKE, Some(Pick::Delegated));
    let paris = read(&conversation, &lines, &schedule("u1", WHEN, PARIS));
    assert_eq!(paris, Ok(Some(shown(WHEN, PARIS))));
    let refused = read(&conversation, &lines, &schedule("u1", WHEN, ZONE));
    assert!(refused.is_err_and(|why| why.contains(law)));
    // Asked with no default and left to Nika, the zone the author chose was never shown to them.
    let (conversation, lines) = zone_chosen_by_the_author();
    let chosen = Some((ProvenanceKind::Delegated, "u2".to_owned()));
    assert_eq!(zone_binding(&conversation, ZONE), chosen);
    let refused = read(&conversation, &lines, &schedule("u1", WHEN, ZONE));
    assert!(refused.is_err_and(|why| why.contains(law)));
}

#[test]
fn a_cadence_picked_before_a_reopen_is_still_cited_from_its_label() {
    for (line, reading, multi_select) in [
        ("daily", None, false),
        (r#"["daily","sunday"]"#, None, true),
        ("le premier", Some(Pick::Offer("daily".into())), false),
    ] {
        let (conversation, lines) = answered(line, reading, multi_select);
        let reopened = Conversation::restored(&conversation.kept()).expect("a kept conversation");
        let args = cadence_cited("u2", DAILY);
        assert_eq!(
            read(&reopened, &lines, &args),
            Ok(Some(shown(DAILY, ZONE))),
            "{line}"
        );
        // A label it did not pick is still no word of the person's.
        let unpicked = cadence_cited("u2", "Du lundi au vendredi à 9h");
        assert!(read(&reopened, &lines, &unpicked).is_err(), "{line}");
    }
    // A choice left to Nika before the reopen picks no label after it either.
    let (left, lines) = answered(AS_YOU_LIKE, Some(Pick::Delegated), false);
    let reopened = Conversation::restored(&left.kept()).expect("a kept conversation");
    assert!(read(&reopened, &lines, &cadence_cited("u2", DAILY)).is_err());
}
