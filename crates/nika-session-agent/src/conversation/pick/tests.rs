// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::sync::Arc;

use nika_session_change::outcome::{Incarnation, QuestionId};
use nika_session_change::work::Offer;

use super::*;

/// « Which source for Y Combinator? » with Hacker News recommended, as E1 asked it.
fn source() -> AskedQuestion {
    let id = QuestionId::new("witness-source".to_owned(), &Arc::new(Incarnation));
    let options = vec![
        Offer::new("hackernews", "Hacker News", true),
        Offer::new("yc_blog", "Le blog YC", false),
    ];
    AskedQuestion::new(id, "yc_source", "Pour « Y Combinator », quelle source ?")
        .with_options(options, true, false)
}

#[test]
fn a_whole_line_picks_by_protocol_and_a_sentence_never_does() {
    let q = source();
    let offer = |key: &str| Some(Pick::Offer(key.to_owned()));
    assert_eq!(Pick::of_line(&q, " yc_blog "), offer("yc_blog"));
    assert_eq!(Pick::of_line(&q, "le blog yc."), offer("yc_blog"));
    assert_eq!(Pick::of_line(&q, "oui"), offer("hackernews"));
    assert_eq!(Pick::of_line(&q, "Non."), Some(Pick::Declined));
    for sentence in [
        "Je t'ai déjà donné les sources.",
        "non merci, prends plutôt le blog",
        "oui tout me va, je suis tes recos",
    ] {
        assert_eq!(Pick::of_line(&q, sentence), None, "{sentence}");
    }
}

#[test]
fn the_consent_word_takes_only_a_single_recommendation() {
    let id = QuestionId::new("witness-two".to_owned(), &Arc::new(Incarnation));
    let both = vec![Offer::new("a", "A", true), Offer::new("b", "B", true)];
    let q = AskedQuestion::new(id, "two", "A ou B ?").with_options(both, true, false);
    assert_eq!(
        Pick::of_line(&q, "oui"),
        None,
        "two recommendations: a reading decides"
    );
}

#[test]
fn the_reading_shows_the_offers_and_the_line_verbatim() {
    let prompt = Pick::prompt(&source(), "  Je t'ai déjà donné les sources.  ");
    assert!(
        prompt.contains("«Pour « Y Combinator », quelle source ?»"),
        "{prompt}"
    );
    assert!(
        prompt.contains("«hackernews» (Hacker News, recommended) · «yc_blog» (Le blog YC)"),
        "{prompt}"
    );
    assert!(
        prompt.contains("The human replied: «Je t'ai déjà donné les sources.»"),
        "{prompt}"
    );
    assert!(
        prompt.contains("DELEGATE") && prompt.ends_with("Answer:"),
        "{prompt}"
    );
    let id = QuestionId::new("witness-name".to_owned(), &Arc::new(Incarnation));
    let open = AskedQuestion::new(id, "new_name", "Quel nouveau nom ?");
    assert!(Pick::prompt(&open, "fais au mieux").contains("It offered no answers."));
}

#[test]
fn a_reading_binds_an_offered_key_or_a_closed_word_and_nothing_else() {
    let q = source();
    let offer = |key: &str| Pick::Offer(key.to_owned());
    assert_eq!(Pick::read("hackernews", &q), offer("hackernews"));
    assert_eq!(Pick::read("Answer: «yc_blog»\n", &q), offer("yc_blog"));
    assert_eq!(Pick::read("DELEGATE", &q), Pick::Delegated);
    assert_eq!(Pick::read("NONE", &q), Pick::Nothing);
    for unread in ["Hacker News", "hackernews\nyc_blog", "", "maybe"] {
        assert_eq!(Pick::read(unread, &q), Pick::Unread, "{unread:?}");
    }
}
