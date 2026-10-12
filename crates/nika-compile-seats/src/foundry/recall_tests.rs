// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::recall::{rank, rank_by_stems, stem, text_of};

/// Three rows: a csv total, a ticket digest, a page fetch.
fn rows() -> Vec<Value> {
    vec![
        json!({"id": "csv", "t": "read a csv file and total the amounts"}),
        json!({"id": "digest", "t": "summarize open tickets every monday"}),
        json!({"id": "fetch", "t": "fetch a page"}),
    ]
}

fn ranked(query: &str) -> Vec<String> {
    let rows = rows();
    (rank(query, &rows, |r| text_of(r, &["t"])).into_iter())
        .map(|(id, _)| id)
        .collect()
}

/// A word every row and request share separates nothing: the csv row shares only « the » with the
/// request and is not recalled.
#[test]
fn a_row_sharing_only_a_stopword_with_the_request_is_never_recalled() {
    assert_eq!(ranked("summarize the tickets"), ["digest"]);
}

/// A request made of shared stopwords and envelope words recalls nothing at all.
#[test]
fn a_request_of_stopwords_alone_recalls_nothing() {
    assert_eq!(
        ranked("the workflow with this and that"),
        Vec::<String>::new()
    );
}

/// The evaluator's French news request, against rows in both languages (the release holds French
/// rows too): a French article or preposition (« les », « dans ») recalls nothing, while the
/// request's content words still reach the French row and the English one.
#[test]
fn a_french_request_recalls_by_its_content_words_never_by_its_articles() {
    let rows = vec![
        json!({"id": "pages", "t": "Parcourir toutes les pages par identité stable"}),
        json!({"id": "markdown", "t": "fetch a source and convert it to markdown"}),
        json!({"id": "digest", "t": "Résumer les actualités du jour dans un dossier"}),
        json!({"id": "imperative", "t": "fais moi"}),
    ];
    let seed = "Récupère les dernières actualités tech de Y Combinator et TechCrunch, fais-moi un résumé et sauvegarde-le en Markdown dans un dossier.";
    let ranked: Vec<String> = (rank(seed, &rows, |r| text_of(r, &["t"])).into_iter())
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ranked, ["digest", "markdown"]);
}

/// The rare words a request shares with a row still rank it first, before a row sharing one
/// informative word.
#[test]
fn the_rows_sharing_the_rare_words_rank_first() {
    assert_eq!(ranked("summarize the tickets of a csv"), ["digest", "csv"]);
    assert_eq!(ranked("zzz qqq"), Vec::<String>::new());
}

/// French inflections and derivations meet at one stem, as do English plurals; a double « s »,
/// a short word and an ending that would leave too short a stem stay.
#[test]
fn a_word_and_its_inflections_meet_at_one_stem() {
    for words in [
        &["importees", "importee", "importer", "import"][..],
        &["reunions", "reunion"][..],
        &["actualites", "actualite"][..],
        &["chevaux", "cheval"][..],
        &["feeds", "feed"][..],
        &["invoices", "invoice"][..],
    ] {
        let stems: BTreeSet<String> = words.iter().map(|&word| stem(word)).collect();
        assert_eq!(stems.len(), 1, "{words:?} read {stems:?}");
    }
    for kept in ["class", "bus", "date", "les", "user"] {
        assert_eq!(stem(kept), kept);
    }
}

/// A French request reaches a row that states its words in another inflection by their stems,
/// never by its words as written: « les commandes importées » shares no word with « importer un
/// fichier de commande ».
#[test]
fn a_request_reaches_a_row_by_the_stems_of_its_words_only_when_ranked_by_stems() {
    let rows = [
        json!({"id": "import", "t": "Importer un fichier de commande"}),
        json!({"id": "pages", "t": "Parcourir toutes les pages"}),
    ];
    let request = "les commandes importées";
    let by_stems: Vec<String> = (rank_by_stems(request, &rows, |r| text_of(r, &["t"])))
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(by_stems, ["import"]);
    assert!(rank(request, &rows, |r| text_of(r, &["t"])).is_empty());
}
