// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The knowledge door's retrieval helpers, below the door that composes a pack from an admitted
//! release (`nika_onboard::knowledge`): BM25 over a release's rows by their informative words or
//! by those words' light French stems, the relevance interleave of ranked sources, a row's text
//! fields and a block row's metadata as a seat reads it. Pure over the admitted rows.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use nika_compile_reader::text::{RETRIEVAL_STOPWORDS, STOPWORDS};
use serde_json::Value;

/// Take ranked sources in turn — the first of each, then the second of each… — keeping each id
/// once with the reason it first arrived with: the sources' relevance order, never the ids'.
#[must_use]
pub fn interleave(sources: &[Vec<(String, String)>]) -> Vec<(String, String)> {
    let mut taken: Vec<(String, String)> = Vec::new();
    let depth = sources.iter().map(Vec::len).max().unwrap_or(0);
    for at in 0..depth {
        for source in sources {
            if let Some((id, why)) = source.get(at)
                && !taken.iter().any(|(seen, _)| seen == id)
            {
                taken.push((id.clone(), why.clone()));
            }
        }
    }
    taken
}

/// The metadata of a block row that keeps a seat from misusing the block — the version it was
/// checked at first (its status and proof), then the holes to fill (owner, note), effects,
/// authority, capabilities, callables and known failure modes — each a whole line.
#[must_use]
pub fn block_metadata(row: &Value) -> String {
    let list = |key: &str, sep: &str| -> Option<String> {
        let items: Vec<&str> = row
            .get(key)?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        (!items.is_empty()).then(|| items.join(sep))
    };
    let holes = row.get("holes").and_then(Value::as_array).map(|holes| {
        let holes: Vec<String> = holes
            .iter()
            .filter_map(|hole| {
                let name = hole.get("name")?.as_str()?;
                let owner = hole
                    .get("owner")
                    .and_then(Value::as_str)
                    .unwrap_or("unowned");
                Some(match hole.get("note").and_then(Value::as_str) {
                    Some(note) => format!("{name} ({owner}: {note})"),
                    None => format!("{name} ({owner})"),
                })
            })
            .collect();
        holes.join("; ")
    });
    let version: Vec<String> = [
        ("", "/pin/binary"),
        ("spec ", "/pin/spec_sha"),
        ("check ", "/check_receipt/verdict"),
        ("", "/status"),
        ("proof ", "/proof_level"),
    ]
    .iter()
    .filter_map(|(label, pointer)| {
        let value = row.pointer(pointer)?.as_str()?;
        let value = if *pointer == "/pin/spec_sha" {
            value.get(..12).unwrap_or(value)
        } else {
            value
        };
        Some(format!("{label}{value}"))
    })
    .collect();
    let fields = [
        (
            "version",
            (!version.is_empty()).then(|| version.join(" · ")),
        ),
        ("holes", holes.filter(|h| !h.is_empty())),
        ("effects", list("effects", ", ")),
        ("authority", list("authority", ", ")),
        ("capabilities", list("interfaces", ", ")),
        ("callables", list("callables", ", ")),
        ("known failures", list("known_failure_modes", "; ")),
    ];
    let mut text = String::new();
    for (label, value) in fields {
        if let Some(value) = value {
            use std::fmt::Write as _;
            let _ = writeln!(text, "{label}: {value}");
        }
    }
    text
}

/// The words of a text, folded and lowercased, three letters or more.
#[must_use]
pub fn tokens(text: &str) -> Vec<String> {
    nika_compile::fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(str::to_owned)
        .collect()
}

/// The words relevance is read past: the compiler's shared [`STOPWORDS`] (function words and the
/// envelope's own keywords) and its retrieval lexicon ([`RETRIEVAL_STOPWORDS`]: French function
/// words, English request scaffolding), as the compile's own retrieval reads them.
fn stopwords() -> &'static BTreeSet<&'static str> {
    static WORDS: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    WORDS.get_or_init(|| {
        (STOPWORDS.iter().copied())
            .chain(RETRIEVAL_STOPWORDS.split_whitespace())
            .collect()
    })
}

/// The words relevance is read from: a text's [`tokens`] without the [`stopwords`]. Every row
/// and every request share those, in English or in French, so they separate nothing: a row that
/// shares only « the », « with » or « les » with a request is never recalled.
fn informative(text: &str) -> Vec<String> {
    let mut words = tokens(text);
    words.retain(|word| !stopwords().contains(word.as_str()));
    words
}

/// The [`informative`] words, each read by its [`stem`]. A stopword leaves before its stem is
/// read, so it never becomes another word.
fn stems(text: &str) -> Vec<String> {
    (informative(text).into_iter())
        .map(|word| stem(&word))
        .collect()
}

/// The endings [`stem`] reads past, longest first: one goes only when the stem it leaves keeps at
/// least so many letters.
const ENDINGS: [(&str, usize); 27] = [
    ("issements", 5),
    ("issement", 5),
    ("ements", 5),
    ("ement", 5),
    ("ations", 5),
    ("ation", 5),
    ("atrices", 5),
    ("atrice", 5),
    ("ateurs", 5),
    ("ateur", 5),
    ("euses", 4),
    ("euse", 4),
    ("eurs", 4),
    ("eur", 4),
    ("ites", 4),
    ("ite", 4),
    ("ives", 4),
    ("ive", 4),
    ("ifs", 4),
    ("if", 4),
    ("ees", 4),
    ("ee", 4),
    ("er", 4),
    ("ir", 4),
    ("re", 4),
    ("es", 4),
    ("e", 4),
];

/// A folded word's light French stem, so a request and a row meet whatever the inflection:
/// « -aux » reads « -al », a final « s » or « x » goes (never « ss »), then one inflectional or
/// common derivational ending, the stem keeping its first letters: « importées », « importer »
/// and « import » all read « import ». English words take the same rules on both sides of a
/// comparison, so « feeds » meets « feed ».
pub(super) fn stem(word: &str) -> String {
    let letters = word.chars().count();
    if letters > 4
        && let Some(base) = word.strip_suffix("aux")
    {
        return format!("{base}al");
    }
    let word = match word.strip_suffix(['s', 'x']) {
        Some(single) if letters > 3 && !word.ends_with("ss") => single,
        _ => word,
    };
    (ENDINGS.iter())
        .find_map(|&(ending, keep)| {
            (word.strip_suffix(ending)).filter(|rest| rest.chars().count() >= keep)
        })
        .unwrap_or(word)
        .to_owned()
}

/// The text of a row's `fields`, joined by a blank: a text as written, any other value as JSON.
#[must_use]
pub fn text_of(row: &Value, fields: &[&str]) -> String {
    fields
        .iter()
        .filter_map(|f| row.get(*f))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// BM25 over the rows' informative words for one query (their [`tokens`] without the shared
/// stopwords): every id that shares an informative word with it, best first.
pub fn rank<'a>(
    query: &str,
    rows: impl IntoIterator<Item = &'a Value>,
    text: impl Fn(&Value) -> String,
) -> Vec<(String, f64)> {
    bm25(query, rows, text, informative)
}

/// [`rank`] over the words' light French stems, so a request and a row meet whatever the
/// inflection: « importées » meets « importer ». The knowledge door ranks its families so; the rows
/// it reaches by their own text keep their words.
pub fn rank_by_stems<'a>(
    query: &str,
    rows: impl IntoIterator<Item = &'a Value>,
    text: impl Fn(&Value) -> String,
) -> Vec<(String, f64)> {
    bm25(query, rows, text, stems)
}

/// BM25 over the words `words` reads in the query and in each row: every id that shares one with
/// the query, best first.
// The counts are rows and words of a snapshot (hundreds, thousands): far below the 2^53
// where a usize stops converting exactly.
#[allow(clippy::cast_precision_loss)]
fn bm25<'a>(
    query: &str,
    rows: impl IntoIterator<Item = &'a Value>,
    text: impl Fn(&Value) -> String,
    words: fn(&str) -> Vec<String>,
) -> Vec<(String, f64)> {
    let docs: Vec<(String, Vec<String>)> = rows
        .into_iter()
        .filter_map(|r| {
            let id = r.get("id")?.as_str()?.to_owned();
            Some((id, words(&text(r))))
        })
        .collect();
    if docs.is_empty() {
        return Vec::new();
    }
    let n = docs.len() as f64;
    let avg = docs.iter().map(|(_, t)| t.len()).sum::<usize>() as f64 / n;
    let mut df: BTreeMap<&str, f64> = BTreeMap::new();
    for (_, terms) in &docs {
        for term in terms.iter().collect::<BTreeSet<_>>() {
            *df.entry(term.as_str()).or_insert(0.0) += 1.0;
        }
    }
    let query: Vec<String> = words(query);
    let (k1, b) = (1.5_f64, 0.75_f64);
    let mut scored: Vec<(String, f64)> = docs
        .iter()
        .map(|(id, terms)| {
            let len = terms.len() as f64;
            let score: f64 = query
                .iter()
                .map(|q| {
                    let tf = terms.iter().filter(|t| *t == q).count() as f64;
                    if tf == 0.0 {
                        return 0.0;
                    }
                    let d = df.get(q.as_str()).copied().unwrap_or(0.0);
                    let idf = ((n - d + 0.5) / (d + 0.5) + 1.0).ln();
                    idf * (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * len / avg))
                })
                .sum();
            (id.clone(), score)
        })
        .filter(|(_, s)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}
