// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `/meaning` beside a conversation an intelligence leads: what the Session keeps of the
//! person's request, read from its own records in the person's language — the request in their
//! words, the values the candidate binds with where each comes from, the choices they left to
//! Nika and the questions still open. No compiler reads such a request, so there is no clause
//! ledger to show. Live gauntlet r08, 2026-10-16: `/meaning` beside an authored proposal printed
//! the money card in its place.

use std::fmt::Write as _;

use super::{AskedState, Binding, ProvenanceKind, ValueRole, Work};
use crate::Lang;

/// The request `work` keeps, its values, delegations and open questions, in `lang`; `None` when
/// it keeps none of them.
#[must_use]
pub fn understood(work: &Work, lang: Lang) -> Option<String> {
    let goal = work.request.said().map(|(goal, _)| goal);
    let open: Vec<&str> = (work.questions.iter())
        .filter(|question| question.state == AskedState::Open)
        .map(|question| question.question.as_str())
        .collect();
    if goal.is_none() && work.bindings.is_empty() && work.delegations.is_empty() && open.is_empty()
    {
        return None;
    }
    let mut text = lang
        .pick(
            "What I kept of your request",
            "Ce que j'ai retenu de votre demande",
        )
        .to_owned();
    if let Some(goal) = goal {
        let _ = write!(text, "\n  « {goal} »");
    }
    if !work.bindings.is_empty() {
        text += lang.pick(
            "\nThe values the workflow relies on",
            "\nLes valeurs sur lesquelles repose le workflow",
        );
        for binding in &work.bindings {
            let _ = write!(
                text,
                "\n  · {} · {} · {}",
                binding.value,
                role(binding.role, lang),
                origin(binding, lang)
            );
        }
    }
    if !work.delegations.is_empty() {
        text += lang.pick(
            "\nWhat you left to Nika",
            "\nCe que vous avez laissé choisir à Nika",
        );
        for delegation in &work.delegations {
            let scope = role(delegation.scope, lang);
            let _ = write!(text, "\n  · {scope} · « {} »", delegation.excerpt);
        }
    }
    if !open.is_empty() {
        text += lang.pick("\nStill open", "\nEncore ouvert");
        for question in open {
            let _ = write!(text, "\n  · {question}");
        }
    }
    text += lang.pick(
        "\n  this is what Nika keeps of your words; if something you asked is missing, say it again in your own words",
        "\n  voilà ce que Nika garde de vos mots ; s'il manque quelque chose que vous avez demandé, redites-le avec vos mots",
    );
    Some(text)
}

/// The part a value plays, in `lang`.
fn role(role: ValueRole, lang: Lang) -> &'static str {
    match role {
        ValueRole::ReadSource => lang.pick("a source it reads", "une source qu'il lit"),
        ValueRole::OutputPath => lang.pick("a file it writes", "un fichier qu'il écrit"),
        ValueRole::RunModel => lang.pick("the model its run uses", "le modèle qu'il utilise"),
        _ => lang.pick("a value", "une valeur"),
    }
}

/// Where a value comes from, in `lang`, with the person's words when it rests on them.
fn origin(binding: &Binding, lang: Lang) -> String {
    let provenance = &binding.provenance;
    let words = (provenance.excerpt.as_deref())
        .filter(|excerpt| !excerpt.trim().is_empty())
        .map(|excerpt| format!(" « {excerpt} »"))
        .unwrap_or_default();
    let kind = match provenance.kind {
        ProvenanceKind::Named => lang.pick("named by your words", "nommée par vos mots"),
        ProvenanceKind::Delegated => lang.pick(
            "chosen by Nika, as you left it",
            "choisie par Nika, comme vous le lui avez laissé",
        ),
        ProvenanceKind::Derived => lang.pick(
            "a routine name for an output you asked for",
            "un nom ordinaire pour une sortie que vous avez demandée",
        ),
        ProvenanceKind::Offered => lang.pick(
            "from the proposal you accepted",
            "de la proposition que vous avez acceptée",
        ),
        ProvenanceKind::Answered => lang.pick("as you typed it", "telle que vous l'avez tapée"),
        ProvenanceKind::Retained => lang.pick(
            "kept from the version you accepted",
            "gardée de la version que vous avez acceptée",
        ),
    };
    format!("{kind}{words}")
}

#[cfg(test)]
mod tests {
    use super::super::{Binding, Provenance, ProvenanceKind, ValueRole};
    use super::{Lang, origin, role};

    /// Each value says, in the person's language, its part and where it comes from, with the
    /// person's words when it rests on them.
    #[test]
    fn a_value_says_its_part_and_origin_in_the_persons_language() {
        let named = Binding::new(
            ValueRole::ReadSource,
            "https://www.lemonde.fr/international/rss_full.xml",
            Provenance::new(ProvenanceKind::Named, "u3").with_excerpt("le monde international"),
        );
        assert_eq!(
            origin(&named, Lang::Fr),
            "nommée par vos mots « le monde international »"
        );
        assert_eq!(
            origin(&named, Lang::En),
            "named by your words « le monde international »"
        );
        assert_eq!(
            role(ValueRole::ReadSource, Lang::Fr),
            "une source qu'il lit"
        );
        let offered = Binding::new(
            ValueRole::OutputPath,
            "./news/digest.md",
            Provenance::new(ProvenanceKind::Offered, "u2"),
        );
        assert_eq!(
            origin(&offered, Lang::Fr),
            "de la proposition que vous avez acceptée"
        );
        assert_eq!(role(ValueRole::OutputPath, Lang::En), "a file it writes");
    }
}
