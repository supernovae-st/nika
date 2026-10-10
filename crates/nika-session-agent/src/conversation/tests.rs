// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;
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
    let reply = conversation.ask(
        &Citations::default(),
        &plan(),
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(reply.ends_turn, "{reply:?}");
    assert_eq!(conversation.asked_ids().len(), 1);
    let lines = citations(&[("u1", "un digest des news tech"), ("u2", "oui tout me va")]);
    // A sentence picks by the Session's reading: here, the recommended offer.
    let read = BTreeMap::from([("plan".to_owned(), Pick::Offer("recommended".to_owned()))]);
    let said = conversation.answered_by("u2", "oui tout me va", &read);
    assert!(said[0].contains("picked `recommended`"), "{said:?}");
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
    let reply = conversation.ask(&lines, &again, Some("c9"), (&mut mint, &mut |_: &str| None));
    assert!(!reply.ends_turn && !reply.is_error, "{reply:?}");
    assert!(reply.text.contains("news/digest.md"), "{reply:?}");
    assert!(conversation.questions().is_empty());
}

#[test]
fn a_settled_value_is_asked_again_only_on_the_persons_later_words() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, _) = accepted(&asker);
    let lines = citations(&[
        ("u1", "un digest des news tech"),
        ("u2", "oui tout me va"),
        ("u3", "garde le même résultat, mais change le nom"),
    ]);
    let rows = json!([
        offered(HN, "read_source", "u2"),
        offered(DIGEST, "output_path", "u2")
    ]);
    write(&mut conversation, &lines, &format!("{HN} {DIGEST}"), rows);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let name = |reopens: Value| {
        json!({"questions": [{"key": "new_name", "role": "output_path",
            "question": "Quel nouveau nom ?", "options": [], "reopens": reopens}]})
    };
    // Words from before the value was settled, or not in the cited line, reopen nothing.
    let unsettling = [
        json!({"message": "u1", "excerpt": "un digest"}),
        json!({"message": "u3", "excerpt": "change la source"}),
    ];
    for reopens in unsettling {
        let args = name(reopens);
        let reply = conversation.ask(&lines, &args, Some("c8"), (&mut mint, &mut |_: &str| None));
        assert!(!reply.ends_turn, "a settled value stays settled");
        assert!(reply.text.contains("already settled"), "given back");
    }
    let args = name(json!({"message": "u3", "excerpt": "change le nom"}));
    let reply = conversation.ask(&lines, &args, Some("c9"), (&mut mint, &mut |_: &str| None));
    assert!(reply.ends_turn, "the person's later words reopen the value");
    assert_eq!(conversation.questions().len(), 1);
    let released = !(conversation.bindings().iter()).any(|b| b.value == DIGEST);
    assert!(released, "the old name is the question's now");
}

#[test]
fn an_answer_the_author_reads_is_bound_by_its_question_and_the_rest_asked_again() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let first = json!({"questions": [
        {"key": "team_webhook", "role": "value", "question": "Équipe ?", "options": []},
        {"key": "support_webhook", "role": "value", "question": "Support ?", "options": []}]});
    conversation.ask(
        &Citations::default(),
        &first,
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    );
    let before = conversation.asked_ids();
    let line = "equipe : https://hooks.example.org/team, support je sais pas";
    conversation.answered_by("u2", line, &BTreeMap::new());
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
    let reply = conversation.ask(&lines, &rest, Some("c2"), (&mut mint, &mut |_: &str| None));
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
    let reply = conversation.ask(
        &lines,
        &forged,
        Some("c3"),
        (&mut mint, &mut |_: &str| None),
    );
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
    // Those words ran that revision: a repair proposed on them, whatever it changed, is not theirs.
    let again = conversation.propose(&lines, &said, "S2".into());
    let stale = again.is_err_and(|why| why.contains("`u3` does not authorize it"));
    assert!(stale, "stale words authorize nothing");
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

const YC: &str = "https://www.ycombinator.com/blog/";

/// « Which source for Y Combinator? » asked after the request u1, Hacker News recommended.
fn ask_source(conversation: &mut Conversation, lines: &Citations, asker: &Arc<Incarnation>) {
    let source = json!({"questions": [{"key": "yc_source", "role": "read_source",
        "question": "Pour « Y Combinator », quelle source ?", "options": [
            {"key": "hackernews", "label": "Hacker News", "recommended": true,
             "values": [{"role": "read_source", "value": HN, "name": "Hacker News"}]},
            {"key": "yc_blog", "label": "Le blog YC",
             "values": [{"role": "read_source", "value": YC, "name": "Le blog YC"}]}]}]});
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), asker);
    let reply = conversation.ask(lines, &source, Some("c1"), (&mut mint, &mut |_: &str| None));
    assert!(reply.ends_turn, "{reply:?}");
}

fn source_offered(value: &str, option: &str, message: &str) -> Value {
    json!({"value": value, "kind": "offered", "role": "read_source", "message": message,
        "question": "yc_source", "option": option})
}

/// F2: a refusal picks no offer, so no value of any offer is admitted as offered from it.
#[test]
fn a_refusal_picks_no_offer_and_admits_none() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let lines = citations(&[("u1", "les news de Y Combinator"), ("u2", "non")]);
    ask_source(&mut conversation, &lines, &asker);
    let said = conversation.answered_by("u2", "non", &BTreeMap::new());
    assert!(said[0].contains("declined"), "{said:?}");
    assert!(conversation.bindings().is_empty());
    for (value, option) in [(HN, "hackernews"), (YC, "yc_blog")] {
        let reply = write(
            &mut conversation,
            &lines,
            value,
            json!([source_offered(value, option, "u2")]),
        );
        assert!(
            reply.is_error && reply.text.contains("declined the offers"),
            "{reply:?}"
        );
    }
    // A sentence the reading finds no offer in admits none either.
    let mut conversation = Conversation::default();
    let lines = citations(&[
        ("u1", "les news de Y Combinator"),
        ("u2", "non merci, autre chose"),
    ]);
    ask_source(&mut conversation, &lines, &asker);
    let read = BTreeMap::from([("yc_source".to_owned(), Pick::Nothing)]);
    conversation.answered_by("u2", "non merci, autre chose", &read);
    let reply = write(
        &mut conversation,
        &lines,
        YC,
        json!([source_offered(YC, "yc_blog", "u2")]),
    );
    assert!(
        reply.is_error && reply.text.contains("picked none"),
        "{reply:?}"
    );
}

/// F2: the Session binds the offer the person picked, and only that offer is admitted.
#[test]
fn only_the_offer_the_person_picked_is_admitted() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let lines = citations(&[("u1", "les news de Y Combinator"), ("u2", "le blog yc stp")]);
    ask_source(&mut conversation, &lines, &asker);
    let read = BTreeMap::from([("yc_source".to_owned(), Pick::Offer("yc_blog".to_owned()))]);
    conversation.answered_by("u2", "le blog yc stp", &read);
    let bound = kinds(&conversation);
    assert_eq!(
        bound,
        [(YC.to_owned(), ProvenanceKind::Offered, "u2".to_owned())]
    );
    let other = json!([source_offered(HN, "hackernews", "u2")]);
    let reply = write(&mut conversation, &lines, HN, other);
    assert!(
        reply.is_error && reply.text.contains("picked `yc_blog`"),
        "{reply:?}"
    );
    let picked = json!([source_offered(YC, "yc_blog", "u2")]);
    let reply = write(&mut conversation, &lines, YC, picked);
    assert!(!reply.is_error, "{reply:?}");
}

/// Item 4(a): a reply that deflects takes the recommendation, visibly delegated.
#[test]
fn a_deflection_takes_the_recommendation_as_delegated() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let deflects = "Je t'ai déjà donné les sources.";
    let lines = citations(&[("u1", "les news de Y Combinator"), ("u2", deflects)]);
    ask_source(&mut conversation, &lines, &asker);
    let read = BTreeMap::from([("yc_source".to_owned(), Pick::Delegated)]);
    let said = conversation.answered_by("u2", deflects, &read);
    assert!(said[0].contains("recommended `hackernews`"), "{said:?}");
    let binding = &conversation.bindings()[0];
    assert_eq!(
        (
            binding.value.as_str(),
            binding.provenance.kind,
            binding.key.as_deref()
        ),
        (HN, ProvenanceKind::Delegated, Some("yc_source"))
    );
    assert_eq!(binding.provenance.excerpt.as_deref(), Some(deflects));
    assert_eq!(binding.provenance.option.as_deref(), Some("hackernews"));
    let delegation = &conversation.delegations()[0];
    assert_eq!(
        (delegation.message.as_str(), delegation.scope),
        ("u2", ValueRole::ReadSource)
    );
    // The non-recommended offer is not the person's: refused as offered.
    let reply = write(
        &mut conversation,
        &lines,
        YC,
        json!([source_offered(YC, "yc_blog", "u2")]),
    );
    assert!(
        reply.is_error && reply.text.contains("left the choice to Nika"),
        "{reply:?}"
    );
}

/// Item 4(b): a value left to the author is chosen once, bound delegated, never asked again.
#[test]
fn a_value_left_to_the_author_is_never_asked_again() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let lines = citations(&[
        ("u1", "un digest des news tech"),
        ("u2", "change le nom"),
        ("u3", "Oui, fais au mieux."),
    ]);
    let name = json!({"questions": [{"key": "new_name", "role": "output_path",
        "question": "Quel nouveau nom ?", "options": []}]});
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    conversation.ask(&lines, &name, Some("c1"), (&mut mint, &mut |_: &str| None));
    let read = BTreeMap::from([("new_name".to_owned(), Pick::Delegated)]);
    let said = conversation.answered_by("u3", "Oui, fais au mieux.", &read);
    assert!(said[0].contains("never ask it again"), "{said:?}");
    let reply = conversation.ask(&lines, &name, Some("c2"), (&mut mint, &mut |_: &str| None));
    assert!(
        !reply.ends_turn && reply.text.contains("left it to you"),
        "{reply:?}"
    );
    let chosen = json!([{"value": "./news/actualites-tech.md", "kind": "delegated",
        "role": "output_path", "message": "u3", "excerpt": "fais au mieux"}]);
    let reply = write(
        &mut conversation,
        &lines,
        "./news/actualites-tech.md",
        chosen,
    );
    assert!(!reply.is_error, "{reply:?}");
}

/// Item 4(d): a question whose every offered value is already bound is not asked.
#[test]
fn a_question_over_values_already_bound_is_not_asked() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, lines) = accepted(&asker);
    let confirm = json!({"questions": [{"key": "confirm", "question": "C'est bon ?",
        "options": [{"key": "ok", "label": "Oui", "recommended": true,
            "values": [{"role": "read_source", "value": HN}, {"role": "output_path", "value": DIGEST}]}]}]});
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    let reply = conversation.ask(
        &lines,
        &confirm,
        Some("c9"),
        (&mut mint, &mut |_: &str| None),
    );
    assert!(
        !reply.ends_turn && reply.text.contains("already bound"),
        "{reply:?}"
    );
}

/// Addendum to F2: words Nika had to ask about never name the value they were ambiguous about;
/// a later line that names it does.
#[test]
fn words_nika_asked_about_never_name_the_value() {
    let asker = Arc::new(Incarnation);
    let mut conversation = Conversation::default();
    let mut lines = citations(&[("u1", "les dernières actualités tech de Y Combinator")]);
    ask_source(&mut conversation, &lines, &asker);
    lines.record(
        4,
        Some(("u2".into(), "Je t'ai déjà donné les sources.".into())),
    );
    conversation.answered_by("u2", "Je t'ai déjà donné les sources.", &BTreeMap::new());
    let named = |message: &str, excerpt: &str| {
        json!([{"value": YC, "kind": "named", "role": "read_source", "message": message,
            "excerpt": excerpt}])
    };
    let reply = write(&mut conversation, &lines, YC, named("u1", "Y Combinator"));
    assert!(
        reply.is_error && reply.text.contains("answers `yc_source`"),
        "{reply:?}"
    );
    lines.record(7, Some(("u3".into(), "prends le blog YC".into())));
    let reply = write(&mut conversation, &lines, YC, named("u3", "le blog YC"));
    assert!(!reply.is_error, "{reply:?}");
}

/// Lane B's T3: a removal must cite words that name the value, and a kept value must still be
/// in the document.
#[test]
fn a_removal_names_the_value_and_a_kept_value_is_in_the_document() {
    let asker = Arc::new(Incarnation);
    let (mut conversation, mut lines) = proposed(&asker);
    lines.record(
        20,
        Some((
            "u3".into(),
            "En fait je voulais les articles d'aujourd'hui.".into(),
        )),
    );
    let rows = json!([retained(TC, "read_source"), retained(DIGEST, "output_path")]);
    let unnamed = json!([{"value": HN, "message": "u3", "excerpt": "les articles d'aujourd'hui"}]);
    let reply = write_removing(
        &mut conversation,
        &lines,
        &format!("{TC} {DIGEST}"),
        (rows, unnamed),
    );
    assert!(
        reply.is_error && reply.text.contains("does not name"),
        "{reply:?}"
    );
    lines.record(23, Some(("u4".into(), "enleve hacker news".into())));
    let rows = json!([retained(TC, "read_source"), retained(DIGEST, "output_path")]);
    let named = json!([{"value": HN, "message": "u4", "excerpt": "hacker news"}]);
    let reply = write_removing(
        &mut conversation,
        &lines,
        &format!("{TC} {DIGEST}"),
        (rows, named),
    );
    assert!(!reply.is_error, "{reply:?}");
    // A value stated as kept that the document no longer carries.
    let gone = json!([retained(TC, "read_source"), retained(DIGEST, "output_path")]);
    let reply = write(&mut conversation, &lines, DIGEST, gone);
    assert!(
        reply.is_error && reply.text.contains("no longer carries"),
        "{reply:?}"
    );
}
