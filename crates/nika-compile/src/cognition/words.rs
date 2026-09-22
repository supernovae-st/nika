// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The word tables of the proposal merge and their classifiers: the verbs and nouns a seat
//! uses for a draft that is no language work (a serialization of computed data, a number
//! alone), the words of a conversion between formats, the phrases that keep the columns as
//! they are, and the content words of a clause. Six languages, folded. Knowledge only:
//! the folds that use these live in `proposal.rs`.

/// The key of a clause for the one-clause folds: whitespace folded, lowercased, the closing
/// punctuation trimmed — « Poste la réponse dans le fil Slack. » and « Poste la réponse dans
/// le fil Slack » are one clause.
pub(super) fn clause_key(text: &str) -> String {
    fold_words(text)
        .trim_end_matches(['.', ';', ',', '!', ':'])
        .trim()
        .to_owned()
}

pub(super) fn fold_words(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Verbs a seat uses for a draft that is no language work (six languages, folded).
pub(super) const SERIALIZE_VERBS: &[&str] = &[
    "prepar",
    "prépar",
    "serializ",
    "sérialis",
    "format",
    "generat",
    "génér",
    "gener",
    "produ",
    "produir",
    "produz",
    "erzeug",
    "schreib",
    "write ",
    "writ",
    "écri",
    "ecri",
    "escrib",
    "scriv",
    "escrev",
    "assembl",
    "build",
    "compo",
    "compos",
    "costru",
    "constru",
    "erstell",
    "monta",
    "montez",
    "construi",
    "compos",
];
/// The data the draft would only carry: the rows, the file content, the result.
pub(super) const DATA_WORDS: &[&str] = &[
    "csv",
    "json",
    "body",
    "cuerpo",
    "corps",
    "corpo",
    "payload",
    "content",
    "contenu",
    "contenido",
    "contenuto",
    "inhalt",
    "conteúdo",
    "conteudo",
    "rows",
    "row ",
    "lignes",
    "ligne ",
    "filas",
    "fila ",
    "righe",
    "riga ",
    "zeilen",
    "zeile ",
    "linhas",
    "linha ",
    "array",
    "tableau",
    "table",
    "tabelle",
    "list",
    "liste",
    "lista",
    "result",
    "résultat",
    "resultat",
    "resultado",
    "risultato",
    "ergebnis",
    "output",
    "file",
    "fichier",
    "archivo",
    "ficheiro",
    "datei",
    "record",
    "enregistrement",
    "column",
    "colonne",
    "columna",
    "colonna",
    "spalte",
    "coluna",
    "field",
    "champ",
];
/// Words that make a draft language work whatever else it says: a summary, a note, a
/// digest, a reply, a translation, a text with headings.
pub(super) const LANGUAGE_WORDS: &[&str] = &[
    "compte rendu",
    "compte-rendu",
    "minutes",
    "verbale",
    "protokoll",
    "acta ",
    "memo",
    "email",
    "e-mail",
    "courriel",
    "message",
    "summar",
    "résum",
    "resum",
    "riassunt",
    "zusammenfass",
    "note",
    "digest",
    "report",
    "rapport",
    "informe",
    "relazione",
    "bericht",
    "relatório",
    "relatorio",
    "reply",
    "réponse",
    "reponse",
    "respuesta",
    "risposta",
    "antwort",
    "resposta",
    "translat",
    "traduc",
    "traduz",
    "übersetz",
    "ubersetz",
    "letter",
    "lettre",
    "carta",
    "heading",
    "titre",
    "título",
    "titulo",
    "titoli",
    "überschrift",
    "uberschrift",
    "prose",
    "paragraph",
    "paragraphe",
    "sentence",
    "phrase",
    "message",
    "brief",
    "explain",
    "expliqu",
    "describ",
    "décri",
    "decri",
    "narrat",
    "bullet",
    "puces",
];

/// A draft whose detail only prepares, formats or serializes the rows a computation
/// produced (« préparer le contenu CSV filtré pour écriture », « serialize the resulting
/// array as JSON ») is no language work: the write takes the computed rows as they are. A
/// detail that names language work (a summary, a note, a digest, headings) stays a draft.
pub(super) fn serialization_draft(detail: &str) -> bool {
    let detail = fold_words(detail);
    let verb = SERIALIZE_VERBS.iter().any(|v| detail.contains(v));
    // « écrivez ce nombre seul », « scrivi solo il numero »: the computed number alone.
    let number_alone = NUMBER_WORDS.iter().any(|w| detail.contains(w))
        && ONLY_WORDS.iter().any(|w| detail.contains(w));
    let cue = verb && (DATA_WORDS.iter().any(|w| detail.contains(w)) || number_alone);
    cue && !LANGUAGE_WORDS.iter().any(|w| detail.contains(w))
}

/// The words of a computed number, six languages, folded.
pub(super) const NUMBER_WORDS: &[&str] = &[
    "nombre",
    "number",
    "número",
    "numero",
    "zahl",
    "anzahl",
    "count",
    "total",
    "totale",
    "somme",
    "suma",
    "summe",
    "montant",
    "amount",
    "résultat",
    "resultat",
    "result",
    "risultato",
    "resultado",
    "ergebnis",
];

/// « alone », « only »: the number and nothing else, six languages, folded.
pub(super) const ONLY_WORDS: &[&str] = &[
    " seul",
    " seule",
    " uniquement",
    " rien d'autre",
    " only",
    " alone",
    " nothing else",
    " solo ",
    " solo.",
    " sólo",
    " solamente",
    " nada más",
    " nada mas",
    " soltanto",
    " nient'altro",
    " nur ",
    " nur.",
    " nichts anderes",
    " apenas",
    " somente",
    " nada mais",
];

/// Words that name a conversion between formats, folded.
pub(super) const CONVERSION_WORDS: &[&str] = &[
    "convert",
    "conversion",
    "convertir",
    "convertis",
    "convierte",
    "converti",
    "konvertier",
    "wandle",
    "converta",
    "json array",
    "array json",
    "tableau json",
    "json-array",
    "one object per row",
    "un objet par ligne",
    "un objeto por fila",
    "un oggetto per riga",
    "ein objekt pro zeile",
    "um objeto por linha",
    "row becomes",
    "each row becomes",
    "as json",
    "to json",
    "en json",
    "in json",
    "into json",
    "as csv",
    "to csv",
    "en csv",
    "in csv",
    "into csv",
    "parse the csv",
    "parse the json",
];

/// A draft whose detail names nothing but a destination and a structure law (« → ./out/
/// titres.txt. Rien d'autre dans le fichier. ») beside produced data: nothing to draft, the
/// rows are written as they are.
pub(super) fn only_a_place_and_a_law(detail: &str) -> bool {
    let rest: String = detail
        .split_whitespace()
        .filter(|word| {
            let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '/');
            !(word.starts_with("./")
                || word.starts_with('/')
                || word.starts_with('~')
                || word == "→"
                || word == "->")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let rest = rest
        .trim()
        .trim_matches(|c: char| c == '.' || c == ':' || c == ',')
        .trim();
    !rest.is_empty() && crate::structure::binds_no_operation(rest)
}

/// Phrases that name the columns kept as they are, six languages, folded.
pub(super) const SAME_COLUMNS: &[&str] = &[
    "same columns",
    "the same columns",
    "with the same columns",
    "all columns",
    "every column",
    "mêmes colonnes",
    "memes colonnes",
    "les mêmes colonnes",
    "les memes colonnes",
    "avec les mêmes colonnes",
    "toutes les colonnes",
    "mismas columnas",
    "las mismas columnas",
    "con las mismas columnas",
    "todas las columnas",
    "stesse colonne",
    "le stesse colonne",
    "con le stesse colonne",
    "tutte le colonne",
    "dieselben spalten",
    "die gleichen spalten",
    "alle spalten",
    "mesmas colunas",
    "as mesmas colunas",
    "com as mesmas colunas",
    "todas as colunas",
    "same row order",
    "mismo orden",
    "el mismo orden",
    "en el mismo orden",
    "même ordre",
    "meme ordre",
    "le même ordre",
    "dans le même ordre",
    "stesso ordine",
    "lo stesso ordine",
    "nello stesso ordine",
    "gleiche reihenfolge",
    "dieselbe reihenfolge",
    "mesma ordem",
    "a mesma ordem",
    "na mesma ordem",
];

/// Whether a detail says nothing but a format the computation keeps by construction: every
/// segment (split at commas and conjunctions) is a same-columns phrase or a lines-mode tail
/// (« tal cual », « in order », « one per line »).
pub(super) fn only_format_words(detail: &str) -> bool {
    let lower = detail.to_lowercase();
    let segments: Vec<String> = lower
        .split([',', ';'])
        .flat_map(|part| {
            part.split(" y ")
                .flat_map(|p| p.split(" and "))
                .flat_map(|p| p.split(" et "))
                .flat_map(|p| p.split(" e "))
                .flat_map(|p| p.split(" und "))
                .collect::<Vec<_>>()
        })
        .map(|s| s.trim().trim_end_matches('.').trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    !segments.is_empty()
        && segments
            .iter()
            .all(|s| SAME_COLUMNS.contains(&s.as_str()) || crate::rules::by_construction_tail(s))
}

/// The words of a text that carry content: four characters or more, punctuation stripped,
/// lowercased.
pub(super) fn content_words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| word.chars().count() >= 4)
}
