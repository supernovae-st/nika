// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_compile_fidelity::fidelity::resolution::ResolutionRole;
use nika_session_change::outcome::{Incarnation, QuestionId};
use nika_session_change::work::ProvenanceKind;
use serde_json::json;

use super::*;

const DIGEST: &str = "./news/digest.md";
const HTML: &str = "./news/digest.html";

fn lines(said: &[(&str, &str)]) -> Citations {
    let mut out = Citations::default();
    for (k, (cite, text)) in said.iter().enumerate() {
        let at = u64::try_from(k).unwrap() * 3 + 1;
        out.record(at, Some(((*cite).into(), (*text).into())));
    }
    out
}

/// Where the digest goes: the Markdown file or the HTML page, `recommended` saying which.
fn output(recommended: (bool, bool)) -> Value {
    json!({"key": "output", "question": "Où écrire le résumé ?", "role": "output_path",
        "options": [
            {"key": "md", "label": "Markdown", "recommended": recommended.0,
             "values": [{"role": "output_path", "value": DIGEST}]},
            {"key": "html", "label": "Page HTML", "recommended": recommended.1,
             "values": [{"role": "output_path", "value": HTML}]}]})
}

fn write(conversation: &mut Conversation, lines: &Citations, source: &str, rows: &Value) {
    let rows = rows.as_array().cloned().unwrap_or_default();
    let texts = (source.to_owned(), "s".to_owned());
    let reply = conversation.write(lines, texts, (&rows, &[]), &mut |_| Ok(()));
    assert!(!reply.is_error, "{reply:?}");
}

/// A candidate that carries the Markdown digest, written on the person's u1.
fn written() -> (Conversation, Citations) {
    let mut conversation = Conversation::default();
    let lines = lines(&[("u1", "un résumé des news tech dans un fichier")]);
    let source = format!("nika: news\n# writes {DIGEST}\n");
    write(&mut conversation, &lines, &source, &json!([]));
    (conversation, lines)
}

fn shown(
    conversation: &Conversation,
    lines: &Citations,
    open: &Value,
    asker: &Arc<Incarnation>,
) -> Result<Vec<AskedQuestion>, String> {
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), asker);
    let args = json!({"open": open});
    let mut facts = |_: &str| None;
    conversation.open_choices(lines, &args, Some("c1"), (&mut mint, &mut facts))
}

/// The output choice, its Markdown default recommended, as the Session shows it.
fn markdown(
    conversation: &Conversation,
    lines: &Citations,
    asker: &Arc<Incarnation>,
) -> Vec<AskedQuestion> {
    shown(conversation, lines, &json!([output((true, false))]), asker).unwrap()
}

/// Shown with the output choice, its Markdown default recommended.
fn proposed(asker: &Arc<Incarnation>) -> (Conversation, Citations) {
    let (mut conversation, lines) = written();
    let open = markdown(&conversation, &lines, asker);
    let shown = conversation.propose(&lines, &json!({}), ("S1".into(), open));
    assert_eq!(shown, Ok(None));
    (conversation, lines)
}

#[test]
fn a_choice_is_shown_only_with_one_default_the_candidate_carries() {
    let asker = Arc::new(Incarnation);
    let (conversation, lines) = written();
    let open = markdown(&conversation, &lines, &asker);
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].key, "output");
    let one = "recommends exactly one option";
    let refusals = [
        (output((false, false)), one),
        (output((true, true)), one),
        (output((false, true)), "does not carry"),
    ];
    for (choice, why) in refusals {
        let refused = shown(&conversation, &lines, &json!([choice]), &asker).unwrap_err();
        assert!(refused.contains(why), "{refused}");
    }
    let twice = json!([output((true, false)), output((true, false))]);
    let refused = shown(&conversation, &lines, &twice, &asker).unwrap_err();
    assert!(refused.contains("shown twice"), "{refused}");
}

/// A schedule's zone shown as a choice beside the candidate is refused with where it belongs,
/// never with « write it in the candidate » (live gauntlet r05: 35 proposals refused so).
#[test]
fn a_zone_the_candidate_does_not_carry_is_said_to_be_the_schedules() {
    let asker = Arc::new(Incarnation);
    let (conversation, lines) = written();
    let zone = json!({"key": "schedule_zone", "question": "Quel fuseau ?", "options": [
        {"key": "mauritius", "label": "Indian/Mauritius", "recommended": true,
         "values": [{"role": "value", "value": "Indian/Mauritius"}]},
        {"key": "utc", "label": "UTC", "values": [{"role": "value", "value": "UTC"}]}]});
    let refused = shown(&conversation, &lines, &json!([zone]), &asker).unwrap_err();
    assert!(refused.contains("the schedule's `timezone`"), "{refused}");
    assert!(refused.contains("leave it out of `open`"), "{refused}");
}

#[test]
fn a_value_the_person_settled_is_never_shown_as_a_choice() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let lines = lines(&[("u1", "écris le résumé dans ./news/digest.md")]);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let bind = json!({"answered": [{"key": "output", "value": DIGEST, "message": "u1",
        "excerpt": "./news/digest.md"}]});
    let reply = conversation.ask(&lines, &bind, Some("c0"), (&mut mint, &mut |_: &str| None));
    assert!(!reply.ends_turn, "{reply:?}");
    let source = format!("nika: news\n# writes {DIGEST}\n");
    write(&mut conversation, &lines, &source, &json!([]));
    let open = json!([output((true, false))]);
    let refused = shown(&conversation, &lines, &open, &asker).unwrap_err();
    assert!(
        refused.contains("already settled by the person"),
        "{refused}"
    );
}

#[test]
fn a_proposal_that_shows_choices_takes_no_act_from_earlier_words() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, first) = written();
    assert_eq!(
        conversation.propose(&first, &json!({}), ("S1".into(), Vec::new())),
        Ok(None)
    );
    let later = lines(&[
        ("u1", "un résumé des news tech dans un fichier"),
        ("u2", "parfait, enregistre-le"),
    ]);
    let open = markdown(&conversation, &later, &asker);
    let said = json!({"acts": ["save"],
        "authorized_by": {"message": "u2", "excerpt": "enregistre-le"}});
    let refused = conversation.propose(&later, &said, ("S1".into(), open));
    assert!(refused.unwrap_err().contains("has not seen"));
    assert_eq!(
        conversation.open().len(),
        1,
        "shown, it waits for the consent"
    );
}

#[test]
fn a_consent_binds_each_default_shown_as_offered_by_its_line() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines) = proposed(&asker);
    let retained = json!([{"value": DIGEST, "kind": "retained", "role": "output_path",
        "message": "u2"}]);
    // Before the consent, no proposal the person saw bound the default.
    let rows = retained.as_array().cloned().unwrap_or_default();
    let texts = (format!("nika: news\n# writes {DIGEST}\n"), "s".to_owned());
    let early = conversation.write(&lines, texts, (&rows, &[]), &mut |_| Ok(()));
    assert!(early.is_error, "{early:?}");
    assert_eq!(conversation.unsaved(&["output".to_owned()]), None);
    assert_eq!(conversation.consented("u2"), vec!["output".to_owned()]);
    let bound: Vec<(&str, ProvenanceKind, &str, Option<&str>)> = (conversation.bindings().iter())
        .map(|b| {
            let kind = b.provenance.kind;
            (
                b.value.as_str(),
                kind,
                b.provenance.message.as_str(),
                b.key.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        bound,
        vec![(DIGEST, ProvenanceKind::Offered, "u2", Some("output"))]
    );
    assert!(conversation.open().is_empty());
    // What the person consented to is theirs, as a proposal they saw binds it.
    let source = format!("nika: news\n# writes {DIGEST}\n");
    write(&mut conversation, &lines, &source, &retained);
}

#[test]
fn a_pick_of_another_option_binds_it_and_waits_for_a_revision() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines) = proposed(&asker);
    // A line about something else picks nothing: the choice stays open with its default.
    assert_eq!(conversation.to_read("ajoute aussi Le Monde").len(), 1);
    let none = BTreeMap::from([("output".to_owned(), Pick::Nothing)]);
    assert!(
        conversation
            .answered_by("u2", "ajoute aussi Le Monde", &none)
            .is_empty()
    );
    assert_eq!(conversation.open().len(), 1);
    // The HTML page, by its key: bound, and the Markdown proposal is no longer saved as it is.
    let said = conversation.answered_by("u3", "html", &BTreeMap::new());
    assert!(said[0].contains("picked `html`"), "{said:?}");
    assert!(said[0].contains("propose again"), "{said:?}");
    assert!(conversation.open().is_empty());
    let html =
        (conversation.bindings().iter()).any(|b| b.value == HTML && b.provenance.message == "u3");
    assert!(html, "the pick is bound");
    let why = conversation.unsaved(&["output".to_owned()]).unwrap();
    assert!(why.contains("picked `html`"), "{why}");
    // Proposed again, the revision is what the person reviews.
    let again = conversation.propose(&lines, &json!({}), ("S2".into(), Vec::new()));
    assert_eq!(again, Ok(None));
    assert_eq!(conversation.unsaved(&[]), None);
}

#[test]
fn a_choice_the_proposal_does_not_show_keeps_a_consent_from_saving() {
    let asker = Arc::new(Incarnation);
    let (conversation, _) = proposed(&asker);
    let why = conversation.unsaved(&[]).unwrap();
    assert!(
        why.contains("`output` is a choice this proposal does not show"),
        "{why}"
    );
    assert_eq!(conversation.unsaved(&["output".to_owned()]), None);
}

#[test]
fn the_defaults_shown_are_stated_offered_to_the_judge() {
    let asker = Arc::new(Incarnation);
    let (conversation, lines) = written();
    let rows = Conversation::defaults(&markdown(&conversation, &lines, &asker));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, DIGEST);
    assert_eq!(rows[0].kind, ResolutionKind::Offered);
    assert_eq!(rows[0].role, ResolutionRole::OutputPath);
}

#[test]
fn the_card_states_the_doubt_and_each_choice_shown_with_its_default() {
    let asker = Arc::new(Incarnation);
    let (conversation, _) = proposed(&asker);
    let card = conversation.card("le résumé", Some("The verifier doubted it."));
    assert!(
        card.starts_with("le résumé\n  doubt · The verifier doubted it.\n"),
        "{card}"
    );
    assert!(
        card.contains("\n  open choices · `yes` keeps every default"),
        "{card}"
    );
    assert!(card.contains("\n    · Où écrire le résumé ?"), "{card}");
    assert!(
        card.contains("\n      `md` Markdown (default): ./news/digest.md"),
        "{card}"
    );
    assert!(
        card.contains("\n      `html` Page HTML: ./news/digest.html"),
        "{card}"
    );
    assert_eq!(Conversation::default().card("le résumé", None), "le résumé");
}

/// A default the candidate carries, picked by protocol (its key or label as the whole line, or a
/// consent word) while nothing else changed, is kept with no revision; another option, a
/// sentence, a refusal, a choice the card does not show or another question is not.
#[test]
fn a_default_picked_by_protocol_is_kept_while_nothing_else_changed() {
    let asker = Arc::new(Incarnation);
    let (conversation, _) = proposed(&asker);
    let id = conversation.open()[0].id.clone();
    let shown = ["output".to_owned()];
    for line in ["md", "Markdown", "yes", "oui", " md. "] {
        assert!(
            conversation.keeps_default(&id, line, (&shown, None)),
            "{line}"
        );
    }
    let changes = [
        "html",
        "Page HTML",
        "md, et ajoute un titre",
        "no",
        "choisis toi-même",
    ];
    for line in changes {
        assert!(
            !conversation.keeps_default(&id, line, (&shown, None)),
            "{line}"
        );
    }
    assert!(
        !conversation.keeps_default(&id, "md", (&[], None)),
        "a choice the card does not show"
    );
    let other = QuestionId::new("another question".to_owned(), &asker);
    assert!(!conversation.keeps_default(&other, "md", (&shown, None)));
}

/// The author keeps the line when the candidate is no longer the revision shown, or when the
/// proposal deferred the person's words to save to its last choice: its author proposes those
/// bytes again, and the mandate continues.
#[test]
fn a_rewritten_candidate_or_a_mandate_on_its_last_choice_keeps_the_author() {
    let asker = Arc::new(Incarnation);
    let shown = ["output".to_owned()];
    let (mut conversation, citations) = proposed(&asker);
    let id = conversation.open()[0].id.clone();
    let source = format!("nika: news\n# writes {DIGEST}\n# rewritten\n");
    write(&mut conversation, &citations, &source, &json!([]));
    assert!(!conversation.keeps_default(&id, "md", (&shown, None)));

    let (mut conversation, _) = written();
    let later = lines(&[
        ("u1", "un résumé des news tech dans un fichier"),
        ("u2", "parfait, enregistre-le"),
    ]);
    let open = markdown(&conversation, &later, &asker);
    let said = json!({"acts": ["save"],
        "authorized_by": {"message": "u2", "excerpt": "enregistre-le"}});
    assert!(
        conversation
            .propose(&later, &said, ("S1".into(), open))
            .is_err()
    );
    let id = conversation.open()[0].id.clone();
    assert!(!conversation.keeps_default(&id, "md", (&shown, None)));
}

// ─── an asked question a shown choice covers is that choice ─────────────

/// The default the live card's topics choice carries (status 65), as the candidate states it.
const ALL_TOPICS: &str = "tous les sujets présents";

fn option(key: &str, label: &str) -> Value {
    json!({"key": key, "label": label})
}

/// The two questions the live author asked together after an adjust offer: depth, then topics,
/// neither recommending an option.
fn depth_and_topics() -> Value {
    json!({"questions": [
        {"key": "depth", "question": "Quelle profondeur pour le résumé ?", "role": "value",
         "free_text": true, "options": [option("headlines", "Très court : une ligne par news"),
             option("short", "Court : 2-3 phrases par news"),
             option("detailed", "Détaillé : un paragraphe par news avec contexte et enjeux")]},
        {"key": "topics", "question": "Quels sujets le digest doit-il couvrir ?", "role": "value",
         "free_text": true, "multi_select": true,
         "options": [option("all", "Tous les sujets présents dans sources.md"),
             option("dev", "Développement et langages"), option("security", "Sécurité"),
             option("ai", "Intelligence artificielle")]}]})
}

/// The live card's topics choice: `all` recommended with the value the candidate carries, then
/// the options `others` names.
fn topics_choice(others: &[(&str, &str)]) -> Value {
    let mut options = vec![
        json!({"key": "all", "label": "Tous les sujets présents dans sources.md",
        "recommended": true, "values": [{"role": "value", "value": ALL_TOPICS}]}),
    ];
    options.extend(others.iter().map(|(key, label)| option(key, label)));
    json!({"key": "topics", "question": "Quels sujets le digest doit-il couvrir ?", "role": "value",
        "free_text": true, "multi_select": true, "options": options})
}

/// The live choice's other options, the asked question's own.
const LIVE: [(&str, &str); 3] = [
    ("dev", "Développement et langages"),
    ("security", "Sécurité"),
    ("ai", "Intelligence artificielle"),
];

/// Depth and topics asked together, depth answered by its identity, then a candidate that carries
/// the default every topic, as the live author wrote it: topics still asked.
fn asked_then_depth(asker: &Arc<Incarnation>) -> (Conversation, Citations, QuestionId) {
    let mut conversation = Conversation::default();
    let lines = lines(&[
        ("u1", "un résumé des news tech dans un fichier"),
        ("u2", "detailed"),
        ("u3", "oui"),
    ]);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), asker);
    let reply = conversation.ask(
        &lines,
        &depth_and_topics(),
        Some("c0"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(!reply.is_error, "{reply:?}");
    let id = |key: &str| {
        (conversation.questions().iter())
            .find(|q| q.key == key)
            .map(|q| q.id.clone())
            .expect(key)
    };
    let (depth, topics) = (id("depth"), id("topics"));
    conversation.answered_one_by(&depth, "u2", "detailed", &BTreeMap::new());
    let source = format!("nika: news\n# résume en détail {ALL_TOPICS}\n");
    write(&mut conversation, &lines, &source, &json!([]));
    (conversation, lines, topics)
}

#[test]
fn an_asked_question_a_shown_choice_covers_is_that_choice() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines, _) = asked_then_depth(&asker);
    let open = shown(
        &conversation,
        &lines,
        &json!([topics_choice(&LIVE)]),
        &asker,
    )
    .unwrap();
    let choice = open[0].id.clone();
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("S1".into(), open)),
        Ok(None)
    );
    // One authoritative object: the asked `topics` is the card's choice now, under its identity.
    let keys: Vec<&str> = (conversation.questions().iter())
        .map(|q| q.key.as_str())
        .collect();
    assert!(!keys.contains(&"topics"), "the choice covers it: {keys:?}");
    let shown_ids: Vec<QuestionId> = conversation.open().iter().map(|q| q.id.clone()).collect();
    assert_eq!(shown_ids, vec![choice]);
    assert_eq!(conversation.unsaved(&["topics".to_owned()]), None);
    // A bare consent binds the default the card shows, and only it.
    assert_eq!(conversation.consented("u3"), vec!["topics".to_owned()]);
    let bound: Vec<(&str, ProvenanceKind, &str)> = (conversation.bindings().iter())
        .filter(|b| b.key.as_deref() == Some("topics"))
        .map(|b| {
            let kind = b.provenance.kind;
            (b.value.as_str(), kind, b.provenance.message.as_str())
        })
        .collect();
    assert_eq!(bound, vec![(ALL_TOPICS, ProvenanceKind::Offered, "u3")]);
}

#[test]
fn a_choice_that_only_reuses_the_key_covers_nothing() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines, topics) = asked_then_depth(&asker);
    let other = topics_choice(&[("cloud", "Le cloud")]);
    let open = shown(&conversation, &lines, &json!([other]), &asker).unwrap();
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("S1".into(), open)),
        Ok(None)
    );
    // Another answer domain under the same key: the asked question stays and still holds consent.
    assert!(conversation.questions().iter().any(|q| q.id == topics));
    let why = conversation.unsaved(&["topics".to_owned()]).unwrap();
    assert!(why.contains("Nika asked `topics`"), "{why}");
}

#[test]
fn the_same_options_asked_another_way_cover_nothing() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines, topics) = asked_then_depth(&asker);
    // Which subjects to exclude, where the person was asked which to include: the same options.
    let mut exclude = topics_choice(&LIVE);
    exclude["question"] = json!("Quels sujets le digest doit-il exclure ?");
    let open = shown(&conversation, &lines, &json!([exclude]), &asker).unwrap();
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("S1".into(), open)),
        Ok(None)
    );
    assert!(conversation.questions().iter().any(|q| q.id == topics));
    let why = conversation.unsaved(&["topics".to_owned()]).unwrap();
    assert!(why.contains("Nika asked `topics`"), "{why}");
}

#[test]
fn a_card_the_session_refuses_removes_nothing() {
    let asker = Arc::new(Incarnation);
    let (conversation, lines, topics) = asked_then_depth(&asker);
    // Its default names a value the candidate does not carry: the card is refused, nothing moves.
    let mut uncarried = topics_choice(&LIVE);
    uncarried["options"][0]["values"] = json!([{"role": "value", "value": "seulement le cloud"}]);
    let refused = shown(&conversation, &lines, &json!([uncarried]), &asker).unwrap_err();
    assert!(refused.contains("does not carry"), "{refused}");
    assert!(conversation.questions().iter().any(|q| q.id == topics));
}

#[test]
fn an_asked_question_answered_after_the_proposal_needs_a_revision() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines, topics) = asked_then_depth(&asker);
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("S1".into(), Vec::new())),
        Ok(None)
    );
    let why = conversation.unsaved(&[]).unwrap();
    assert!(
        why.contains("Nika asked `topics`"),
        "uncovered, it holds: {why}"
    );
    conversation.answered_one_by(&topics, "u3", r#"["security","ai"]"#, &BTreeMap::new());
    // The proposal shown does not carry that answer: it is not saved as it is.
    let why = conversation.unsaved(&[]).unwrap();
    assert!(
        why.contains("`topics` was answered after this proposal"),
        "{why}"
    );
    assert!(why.contains("Nika revises it before saving"), "{why}");
    // The revision the author proposes is what the person consents to.
    let source = "nika: news\n# résume en détail la sécurité et l'IA\n";
    write(&mut conversation, &lines, source, &json!([]));
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("S2".into(), Vec::new())),
        Ok(None)
    );
    assert_eq!(conversation.unsaved(&[]), None);
}

#[test]
fn another_option_of_the_covering_choice_revises_and_its_revision_saves() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines, _) = asked_then_depth(&asker);
    let open = shown(
        &conversation,
        &lines,
        &json!([topics_choice(&LIVE)]),
        &asker,
    )
    .unwrap();
    let choice = open[0].id.clone();
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("S1".into(), open)),
        Ok(None)
    );
    let said =
        conversation.answered_one_by(&choice, "u3", r#"["security","ai"]"#, &BTreeMap::new());
    assert!(said.iter().any(|s| s.contains("propose again")), "{said:?}");
    let why = conversation.unsaved(&["topics".to_owned()]).unwrap();
    assert!(why.contains("Nika revises it before saving"), "{why}");
    let source = "nika: news\n# résume en détail la sécurité et l'IA\n";
    write(&mut conversation, &lines, source, &json!([]));
    assert_eq!(
        conversation.propose(&lines, &json!({}), ("S2".into(), Vec::new())),
        Ok(None)
    );
    assert_eq!(
        conversation.unsaved(&[]),
        None,
        "the revision is what the person consents to"
    );
}
