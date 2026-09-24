// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Where a request places an unnamed file: a destination, somewhere it denies or where material
//! already is, or a place the words leave unclear (asked, never guessed).

use super::{unclear_destination, unnamed_destination};
use crate::lexicon;
use crate::plan::{EffectPolicy, EffectVerb};

#[test]
fn a_denial_of_the_write_or_a_location_asks_for_no_write_and_no_question() {
    for detail in [
        "my notes not into a file",
        "les notes contenues dans un fichier",
        "my notes, but do not write them into a file",
        "my notes, don't save them into a file",
        "my notes without saving them into a file",
        "mes notes mais ne les mets pas dans un fichier",
        "mes notes mais ne l'écris pas dans un fichier",
        "mes notes sans les enregistrer dans un fichier",
        "the text that is already in a file",
        "le texte déjà dans un fichier",
        "the report whose source is in a file",
        "le rapport dont la source est dans un fichier",
    ] {
        assert_eq!(unnamed_destination(detail), None, "{detail}");
        assert_eq!(unclear_destination(detail), None, "{detail}");
    }
}

#[test]
fn a_negated_placing_or_unknown_verb_and_an_unrequested_state_are_asked() {
    for (detail, excerpt) in [
        ("my notes, don't put them into a file", "into a file"),
        (
            "mes notes sans les mettre dans un fichier",
            "dans un fichier",
        ),
        // A verb negator over a word that only contains a write verb: unclear, asked.
        ("my notes, don't rewrite them into a file", "into a file"),
        ("the notes stored in a file", "in a file"),
        ("the text written in a file", "in a file"),
        ("le texte écrit dans un fichier", "dans un fichier"),
    ] {
        assert_eq!(unnamed_destination(detail), None, "{detail}");
        assert_eq!(unclear_destination(detail), Some(excerpt), "{detail}");
    }
}

#[test]
fn a_requested_destination_keeps_its_write_past_a_negated_modifier() {
    for (detail, excerpt, phrase) in [
        ("mes notes dans un fichier", "dans un fichier", "un fichier"),
        (
            "my notes without losing details into a file",
            "into a file",
            "a file",
        ),
        (
            "mes notes sans inventer de faits dans un fichier",
            "dans un fichier",
            "un fichier",
        ),
        // A negated send, payment or rewrite is not a write: the file stays.
        (
            "my notes without sending emails into a file",
            "into a file",
            "a file",
        ),
        (
            "mes notes sans envoyer de messages dans un fichier",
            "dans un fichier",
            "un fichier",
        ),
        (
            "the summary without a payment into a file",
            "into a file",
            "a file",
        ),
        (
            "my notes without rewriting them into a file",
            "into a file",
            "a file",
        ),
        ("i want the summary saved in a file", "in a file", "a file"),
        (
            "je veux un résumé écrit dans un fichier",
            "dans un fichier",
            "un fichier",
        ),
        (
            "ne garde que les décisions dans un fichier",
            "dans un fichier",
            "un fichier",
        ),
        (
            "my notes, do not email them, but save the summary into a file",
            "into a file",
            "a file",
        ),
        ("what is new into a file", "into a file", "a file"),
        (
            "les notes dont j'ai besoin dans un fichier",
            "dans un fichier",
            "un fichier",
        ),
    ] {
        assert_eq!(
            unnamed_destination(detail),
            Some((excerpt, phrase)),
            "{detail}"
        );
        assert_eq!(unclear_destination(detail), None, "{detail}");
    }
}

#[test]
fn a_negated_other_effect_keeps_the_file_and_is_never_performed() {
    // The whole reader: the file stays a write, the denied effect is never automatic, and
    // the modifier's words survive somewhere in the plan (a forbidden effect, a constraint,
    // a step, an unresolved clause or an unknown), never silently lost.
    for (intent, target, kept, denied) in [
        (
            "Summarize my notes without sending emails into a file.",
            "a file",
            "sending emails",
            EffectVerb::Send,
        ),
        (
            "Résume mes notes sans envoyer de messages dans un fichier.",
            "un fichier",
            "envoyer de messages",
            EffectVerb::Send,
        ),
        (
            "Summarize the notes without a payment into a file.",
            "a file",
            "a payment",
            EffectVerb::Pay,
        ),
    ] {
        let reading = lexicon::read(intent);
        let plan = &reading.plan;
        assert!(
            plan.effects
                .iter()
                .any(|e| e.verb == EffectVerb::Write && e.target == target),
            "{intent}: {plan:?}"
        );
        assert!(
            plan.effects
                .iter()
                .all(|e| e.verb != denied || e.policy == EffectPolicy::Forbidden),
            "{intent}: {plan:?}"
        );
        let survives = plan.effects.iter().any(|e| e.evidence.contains(kept))
            || plan.constraints.iter().any(|c| c.contains(kept))
            || plan.steps.iter().any(|s| s.detail.contains(kept))
            || plan.unknowns.iter().any(|u| u.contains(kept))
            || reading.unresolved.iter().any(|u| u.contains(kept));
        assert!(survives, "{intent}: {plan:?}");
    }
}

#[test]
fn the_probes_and_a_denied_write_still_write_nothing() {
    for intent in [
        "Summarize my notes not into a file.",
        "Résume les notes contenues dans un fichier.",
    ] {
        let reading = lexicon::read(intent);
        assert!(
            reading
                .plan
                .effects
                .iter()
                .all(|e| e.verb != EffectVerb::Write),
            "{intent}: {:?}",
            reading.plan.effects
        );
    }
}
