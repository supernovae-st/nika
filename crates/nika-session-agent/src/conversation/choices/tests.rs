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
