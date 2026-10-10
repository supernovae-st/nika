// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::sync::Arc;

use nika_session_change::outcome::Incarnation;
use serde_json::json;

use super::*;

const HN: &str = "https://news.ycombinator.com";
const TC: &str = "https://techcrunch.com";
const DIGEST: &str = "./news/digest.md";

fn citations(lines: &[(&str, &str)]) -> Citations {
    let mut out = Citations::default();
    for (k, (cite, text)) in lines.iter().enumerate() {
        let at = u64::try_from(k).unwrap() * 3 + 1;
        out.record(at, Some(((*cite).into(), (*text).into())));
    }
    out
}

fn plan() -> Value {
    json!({"questions": [{"key": "plan", "question": "HN et TC → ./news/digest.md ?",
        "options": [{"key": "recommended", "label": "Oui", "recommended": true, "values": [
            {"role": "read_source", "value": HN, "name": "Hacker News"},
            {"role": "read_source", "value": TC, "name": "TechCrunch"},
            {"role": "output_path", "value": DIGEST}]},
            {"key": "other", "label": "Autre"}],
        "free_text": true}]})
}

fn offered(value: &str, role: &str, message: &str) -> Value {
    json!({"value": value, "kind": "offered", "role": role, "message": message,
        "question": "plan", "option": "recommended"})
}

fn retained(value: &str, role: &str) -> Value {
    json!({"value": value, "kind": "retained", "role": role, "message": "u2"})
}

/// A conversation where the plan offer was asked and the person's u2 accepted it.
fn accepted(asker: &Arc<Incarnation>) -> (Conversation, Citations) {
    let mut conversation = Conversation::default();
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), asker);
    let reply = conversation.ask(&Citations::default(), &plan(), Some("c1"), &mut mint);
    assert!(reply.ends_turn, "{reply:?}");
    assert_eq!(conversation.asked_ids().len(), 1);
    let lines = citations(&[("u1", "un digest des news tech"), ("u2", "oui tout me va")]);
    conversation.answered_by("u2");
    assert!(conversation.asked_ids().is_empty());
    (conversation, lines)
}

fn write_removing(
    conversation: &mut Conversation,
    lines: &Citations,
    source: &str,
    (rows, removed): (Value, Value),
) -> ToolReply {
    let rows = rows.as_array().cloned().unwrap_or_default();
    let removed = removed.as_array().cloned().unwrap_or_default();
    let texts = (source.to_owned(), "s".to_owned());
    conversation.write(lines, texts, (&rows, &removed), &mut |_| Ok(()))
}

fn write(
    conversation: &mut Conversation,
    lines: &Citations,
    source: &str,
    rows: Value,
) -> ToolReply {
    write_removing(conversation, lines, source, (rows, json!([])))
}

fn kinds(conversation: &Conversation) -> Vec<(String, ProvenanceKind, String)> {
    (conversation.bindings().iter())
        .map(|b| {
            (
                b.value.clone(),
                b.provenance.kind,
                b.provenance.message.clone(),
            )
        })
        .collect()
}

/// The accepted digest, proposed: HN, TC and DIGEST offered by u2.
fn proposed(asker: &Arc<Incarnation>) -> (Conversation, Citations) {
    let (mut conversation, lines) = accepted(asker);
    let rows = json!([
        offered(HN, "read_source", "u2"),
        offered(TC, "read_source", "u2"),
        offered(DIGEST, "output_path", "u2")
    ]);
    let reply = write(
        &mut conversation,
        &lines,
        &format!("{HN} {TC} {DIGEST}"),
        rows,
    );
    assert!(!reply.is_error, "{reply:?}");
    assert!(conversation.dropped().is_empty());
    assert_eq!(
        conversation.propose(&lines, &json!({}), "S1".into()),
        Ok(None)
    );
    (conversation, lines)
}

#[test]
fn an_accepted_offer_binds_its_values_as_offered_by_the_accepting_line() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines) = accepted(&asker);
    let rows = json!([
        offered(HN, "read_source", "u2"),
        offered(TC, "read_source", "u2"),
        offered(DIGEST, "output_path", "u2")
    ]);
    let reply = write(
        &mut conversation,
        &lines,
        &format!("{HN} {TC} {DIGEST}"),
        rows,
    );
    assert!(!reply.is_error, "{reply:?}");
    let kinds = kinds(&conversation);
    assert_eq!(kinds.len(), 3);
    assert!(
        kinds
            .iter()
            .all(|(_, kind, message)| *kind == ProvenanceKind::Offered && message == "u2")
    );
    // An offer stated as accepted by another line, or a value no option carries, is refused.
    for wrong in [
        offered(HN, "read_source", "u1"),
        offered("https://example.org", "read_source", "u2"),
    ] {
        let reply = write(&mut conversation, &lines, HN, json!([wrong]));
        assert!(
            reply.is_error && reply.text.contains("offered"),
            "{reply:?}"
        );
    }
}

#[test]
fn a_dropped_value_waits_for_the_persons_words_and_a_replaced_output_does_not() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, mut lines) = proposed(&asker);
    // TechCrunch dropped from the document and from the rows: the revision is written, never
    // proposed, and the value is named.
    let dropped = json!([retained(HN, "read_source"), retained(DIGEST, "output_path")]);
    let reply = write(
        &mut conversation,
        &lines,
        &format!("{HN} {DIGEST}"),
        dropped,
    );
    assert!(!reply.is_error, "{reply:?}");
    let why = conversation.dropped();
    assert!(
        why.len() == 1 && why[0].contains("techcrunch.com"),
        "{why:?}"
    );
    // Still in the document though no row names it: kept, with its own provenance.
    let silent = json!([retained(HN, "read_source"), retained(DIGEST, "output_path")]);
    write(
        &mut conversation,
        &lines,
        &format!("{HN} {TC} {DIGEST}"),
        silent,
    );
    assert!(conversation.dropped().is_empty());
    let tc = kinds(&conversation)
        .into_iter()
        .find(|(v, ..)| v == TC)
        .unwrap();
    assert_eq!((tc.1, tc.2.as_str()), (ProvenanceKind::Offered, "u2"));
    // Another authorized output replaces the accepted one: nothing is dropped.
    let elsewhere = json!([retained(HN, "read_source"), retained(TC, "read_source"),
        {"value": "./news/autre.md", "kind": "derived", "role": "output_path", "message": "u1",
         "excerpt": "un digest"}]);
    write(
        &mut conversation,
        &lines,
        &format!("{HN} {TC} ./news/autre.md"),
        elsewhere,
    );
    assert!(
        conversation.dropped().is_empty(),
        "{:?}",
        conversation.dropped()
    );
    // The person's own words remove a value: it is no longer theirs to keep.
    lines.record(20, Some(("u3".into(), "enleve techcrunch".into())));
    let rows = json!([retained(HN, "read_source"), retained(DIGEST, "output_path")]);
    let forged = json!([{"value": TC, "message": "u3", "excerpt": "garde tout"}]);
    let reply = write_removing(
        &mut conversation,
        &lines,
        &format!("{HN} {DIGEST}"),
        (rows.clone(), forged),
    );
    assert!(reply.is_error, "{reply:?}");
    let removed = json!([{"value": TC, "message": "u3", "excerpt": "enleve techcrunch"}]);
    let reply = write_removing(
        &mut conversation,
        &lines,
        &format!("{HN} {DIGEST}"),
        (rows, removed),
    );
    assert!(!reply.is_error, "{reply:?}");
    assert!(
        conversation.dropped().is_empty(),
        "{:?}",
        conversation.dropped()
    );
}

#[test]
fn a_question_the_person_settled_is_answered_by_the_session() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines) = accepted(&asker);
    let rows = json!([
        offered(HN, "read_source", "u2"),
        offered(DIGEST, "output_path", "u2")
    ]);
    write(&mut conversation, &lines, &format!("{HN} {DIGEST}"), rows);
    let again = json!({"questions": [{"key": "output", "role": "output_path",
        "question": "Dans quel fichier ?", "options": []}]});
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let reply = conversation.ask(&lines, &again, Some("c9"), &mut mint);
    assert!(!reply.ends_turn && !reply.is_error, "{reply:?}");
    assert!(reply.text.contains("news/digest.md"), "{reply:?}");
    assert!(conversation.questions().is_empty());
}

#[test]
fn an_answer_the_author_reads_is_bound_by_its_question_and_the_rest_asked_again() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let first = json!({"questions": [
        {"key": "team_webhook", "role": "value", "question": "Équipe ?", "options": []},
        {"key": "support_webhook", "role": "value", "question": "Support ?", "options": []}]});
    conversation.ask(&Citations::default(), &first, Some("c1"), &mut mint);
    let before = conversation.asked_ids();
    conversation.answered_by("u2");
    let lines = citations(&[
        ("u1", "envoie aux webhooks"),
        (
            "u2",
            "equipe : https://hooks.example.org/team, support je sais pas",
        ),
    ]);
    let rest = json!({"answered": [{"key": "team_webhook", "value": "https://hooks.example.org/team",
            "message": "u2", "excerpt": "https://hooks.example.org/team"}],
        "questions": [{"key": "support_webhook", "role": "value", "question": "Support ?",
            "options": []}]});
    let reply = conversation.ask(&lines, &rest, Some("c2"), &mut mint);
    assert!(reply.ends_turn, "{reply:?}");
    let team = &conversation.bindings()[0];
    assert_eq!(team.key.as_deref(), Some("team_webhook"));
    assert_eq!(team.provenance.kind, ProvenanceKind::Answered);
    let now = &conversation.questions()[0];
    assert_eq!(now.key, "support_webhook");
    assert!(!before.contains(&now.id), "a new identity");
    // A value the cited line does not hold binds nothing.
    let forged = json!({"answered": [{"key": "support_webhook", "value": "https://evil.example",
        "message": "u2", "excerpt": "support"}]});
    let reply = conversation.ask(&lines, &forged, Some("c3"), &mut mint);
    assert!(reply.text.contains("not in the person's line"), "{reply:?}");
    assert_eq!(conversation.bindings().len(), 1);
}

#[test]
fn words_cover_only_the_revision_whose_proposal_they_answered() {
    let asker = Arc::new(Incarnation);
    let said = json!({"acts": ["save", "run"],
        "authorized_by": {"message": "u3", "excerpt": "enregistre-le puis lance-le"}});
    let u3 = || Some(("u3".into(), "parfait, enregistre-le puis lance-le".into()));
    // Another scope: nothing authorized, the new proposal waits.
    let (mut conversation, mut lines) = proposed(&asker);
    lines.record(20, u3());
    let refused = conversation.propose(&lines, &said, "S2".into());
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(
        conversation.shown.as_ref().map(|s| s.scope.as_str()),
        Some("S2")
    );
    // The same scope the words answered: save and run.
    let (mut conversation, mut lines) = proposed(&asker);
    lines.record(20, u3());
    let acts = conversation.propose(&lines, &said, "S1".into());
    assert_eq!(
        acts,
        Ok(Some(Acts {
            save: true,
            run: true
        }))
    );
    // Words written before the proposal was shown authorize nothing.
    let (mut conversation, lines) = proposed(&asker);
    let early =
        json!({"acts": ["save"], "authorized_by": {"message": "u1", "excerpt": "un digest"}});
    assert!(conversation.propose(&lines, &early, "S1".into()).is_err());
}

#[test]
fn a_replacement_keeps_nothing_of_the_former_request() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, mut lines) = proposed(&asker);
    lines.record(20, Some(("u3".into(), "non laisse tomber tout ca".into())));
    let forged = conversation.replace(&lines, &json!({"message": "u3", "excerpt": "tout effacer"}));
    assert!(forged.is_error, "{forged:?}");
    let reply = conversation.replace(
        &lines,
        &json!({"message": "u3", "excerpt": "laisse tomber tout ca"}),
    );
    assert!(!reply.is_error, "{reply:?}");
    assert!(conversation.bindings().is_empty() && conversation.candidate().is_none());
    assert!(conversation.delegations().is_empty() && conversation.questions().is_empty());
    assert!(conversation.dropped().is_empty());
    assert_eq!(conversation.since(), 20);
    assert_eq!(
        lines.stated(conversation.since()),
        "non laisse tomber tout ca"
    );
}

#[test]
fn a_reopen_keeps_the_evidence_and_no_authority() {
    let asker = Arc::new(Incarnation);
    let (conversation, _) = proposed(&asker);
    let kept = conversation.kept();
    let restored = Conversation::restored(&kept).unwrap();
    assert_eq!(kinds(&restored), kinds(&conversation));
    assert_eq!(restored.candidate().map(|c| c.number), Some(1));
    assert!(restored.shown.is_none() && restored.questions().is_empty());
    // A kept value is still the person's to keep after the reopen.
    let lines = citations(&[("u1", "un digest des news tech"), ("u2", "oui tout me va")]);
    let mut restored = restored;
    let rows = json!([
        retained(HN, "read_source"),
        retained(TC, "read_source"),
        retained(DIGEST, "output_path")
    ]);
    let reply = write(&mut restored, &lines, &format!("{HN} {TC} {DIGEST}"), rows);
    assert!(!reply.is_error, "{reply:?}");
    assert!(Conversation::restored(&json!({"version": 9})).is_none());
}
