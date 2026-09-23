// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Candidate recall for the compiler: lexical retrieval over the pattern
//! families and the canonical skeletons.
//!
//! This is compiler-private DEVELOPMENT knowledge. It is not a language key,
//! a verb, an SDK noun or a registry, and it grants nothing: a [`Hit`] is a
//! candidate worth reading, never a selection verdict, a semantic truth or
//! authority. The bounded decision seats and Check judge everything after it.
//!
//! Two kinds of documents share one index. The canonical skeletons come from
//! the embedded pack ([`nika_pack::template_names`]): name, headline and the
//! structural words a body actually carries (builtins, verbs, fan-out). The
//! pattern families are a compact projection of the public spec's
//! `eval/hot/families.yaml` (Apache-2.0 · `assets/pattern_families.json`):
//! id, domain, title, example intent, structure signature, pattern words and
//! the skeleton that covers the family when one does. That inventory measures
//! the compiler; it does not promise that any family compiles.
//!
//! Ranking is BM25 (`nika-bm25`) over one normalized token stream: French
//! diacritics fold to ASCII, function words drop (English and French), a
//! light suffix stemmer unifies inflections, and a closed alias table maps
//! everyday and French operation words onto the family pattern vocabulary.
//! Aliases are tokens, never sentence rules, and documents pass through the
//! same pipeline as queries, so the expansion is symmetric. No provider, no
//! network, no file system: the corpus is embedded and built once.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use nika_bm25::{BmIndex, BmParams};
use serde::Deserialize;

/// The compact family projection (see the module doc).
const FAMILIES: &str = include_str!("../assets/pattern_families.json");

/// Which corpus a hit came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HitKind {
    /// A pattern family from the spec inventory (`A01` … `J15`).
    Family,
    /// A canonical skeleton from the embedded pack (`chain` · `fanout` · …).
    Skeleton,
}

/// One recalled candidate. Recall only: the score orders candidates for a
/// reader; it is not a confidence, a verdict or an authority claim.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Hit {
    /// Family id or skeleton name.
    pub id: String,
    pub kind: HitKind,
    /// Family title or skeleton headline.
    pub title: String,
    /// Pattern words the document carries (family patterns · derived
    /// skeleton structure words).
    pub patterns: Vec<String>,
    /// Raw BM25 score (corpus-relative; never normalized).
    pub score: f64,
    /// The canonical skeleton that covers a family, or a skeleton hit's own name.
    pub skeleton: Option<String>,
    /// A family's structure signature (`FETCH → EXTRACT → SUMMARIZE`); none for a skeleton.
    pub signature: Option<String>,
}

/// Recall at most `k` candidates for a free-form intent, best first.
///
/// An intent that carries no signal (function words only · unknown words)
/// recalls nothing: an empty result is the honest answer, not a guess.
#[must_use]
pub fn retrieve(query: &str, k: usize) -> Vec<Hit> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let normalized = normalize(query);
    let tokens: Vec<&str> = normalized
        .iter()
        .map(String::as_str)
        .filter(|token| seen.insert(token))
        .collect();
    if tokens.is_empty() || k == 0 {
        return Vec::new();
    }
    let corpus = corpus();
    corpus
        .index
        .top_k(&tokens.join(" "), k)
        .into_iter()
        .filter(|(_, score)| *score > 0.0)
        .filter_map(|(id, score)| {
            let doc = usize::try_from(id).ok().and_then(|i| corpus.docs.get(i))?;
            Some(Hit {
                id: doc.id.clone(),
                kind: doc.kind,
                title: doc.title.clone(),
                patterns: doc.patterns.clone(),
                score,
                skeleton: doc.skeleton.clone(),
                signature: doc.signature.clone(),
            })
        })
        .collect()
}

/// Recall from the operation words of a semantic plan (the COLD → retrieval
/// leg): the plan's closed op vocabulary, its effect verbs and its obligation
/// kinds. Each word expands to the family pattern words it corresponds to;
/// any other word passes through the ordinary query normalization.
#[must_use]
pub fn retrieve_by_ops(ops: &[&str], k: usize) -> Vec<Hit> {
    let query: Vec<String> = ops.iter().map(|op| op_words(op)).collect();
    retrieve(&query.join(" "), k)
}

fn op_words(op: &str) -> String {
    let key = op.trim().to_ascii_lowercase();
    let words = match key.as_str() {
        "read" => "read document",
        "fetch" => "fetch url",
        "lookup" => "lookup records",
        "search" => "search corpus",
        "extract" => "extract fields",
        "classify" => "classify route",
        "draft" => "draft summarize generate",
        "compute" => "aggregate compute score",
        "validate" => "validate verify",
        "explore" => "explore agent",
        "send" => "send human",
        "publish" => "publish human",
        "notify" => "notify",
        "create" => "create upload",
        "update" => "update human",
        "refund" => "refund human policy",
        "pay" => "pay payment human",
        "order" => "order human",
        "merge" => "merge dedup",
        "delete" => "delete human",
        "write" => "write persist",
        "dedup" => "dedup",
        "retry_bound" => "batch bounded",
        "revision_check" => "diff snapshot revision",
        _ => return key,
    };
    words.to_owned()
}

// ── the corpus ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FamilyRow {
    id: String,
    domain: String,
    title: String,
    intent: String,
    signature: String,
    patterns: Vec<String>,
    #[serde(default)]
    skeleton: Option<String>,
    #[serde(default)]
    gate: Option<String>,
}

struct Doc {
    id: String,
    kind: HitKind,
    title: String,
    patterns: Vec<String>,
    skeleton: Option<String>,
    signature: Option<String>,
}

struct Corpus {
    docs: Vec<Doc>,
    index: BmIndex,
}

static CORPUS: OnceLock<Corpus> = OnceLock::new();

fn corpus() -> &'static Corpus {
    CORPUS.get_or_init(build)
}

fn build() -> Corpus {
    // A corrupt embedded projection yields an empty family corpus; the unit
    // test below pins the row count so the defect cannot ship silently.
    let families: Vec<FamilyRow> = serde_json::from_str(FAMILIES).unwrap_or_default();
    let mut coverage: BTreeMap<&str, String> = BTreeMap::new();
    for row in &families {
        if let Some(skeleton) = &row.skeleton {
            let text = coverage.entry(skeleton.as_str()).or_default();
            text.push_str(&row.title);
            text.push('\n');
            text.push_str(&row.intent);
            text.push('\n');
            text.push_str(&row.patterns.join(" "));
            text.push('\n');
        }
    }
    let mut docs = Vec::new();
    let mut texts = Vec::new();
    for row in &families {
        docs.push(Doc {
            id: row.id.clone(),
            kind: HitKind::Family,
            title: row.title.clone(),
            patterns: row.patterns.clone(),
            skeleton: row.skeleton.clone(),
            signature: Some(row.signature.clone()),
        });
        texts.push(family_text(row));
    }
    for name in nika_pack::template_names() {
        let body = nika_pack::template(&name).unwrap_or_default();
        let words = structure_words(body);
        let headline = crate::text::banner_sentence(body).unwrap_or_else(|| name.clone());
        let mut text = format!(
            "{}\n{headline}\n{}\n",
            name.replace('-', " "),
            words.join(" ")
        );
        if let Some(covered) = coverage.get(name.as_str()) {
            text.push_str(covered);
        }
        docs.push(Doc {
            id: name.clone(),
            kind: HitKind::Skeleton,
            title: headline,
            patterns: words,
            skeleton: Some(name),
            signature: None,
        });
        texts.push(text);
    }
    let mut index = BmIndex::new(BmParams::default());
    for (i, text) in texts.iter().enumerate() {
        if let Ok(id) = u32::try_from(i) {
            index.add_document(id, &normalize(text).join(" "));
        }
    }
    index.finalize();
    Corpus { docs, index }
}

fn family_text(row: &FamilyRow) -> String {
    let mut text = String::new();
    for part in [&row.title, &row.intent, &row.signature, &row.domain] {
        text.push_str(part);
        text.push('\n');
    }
    for pattern in &row.patterns {
        text.push_str(&pattern.replace('_', " "));
        text.push(' ');
    }
    if let Some(skeleton) = &row.skeleton {
        text.push_str(&skeleton.replace('-', " "));
    }
    if row.gate.is_some() {
        text.push_str(" human approve gate");
    }
    text
}

/// Structural vocabulary a skeleton body actually carries — derived from its
/// meaningful lines (comments stripped, the surface the router indexes too),
/// never from prose.
const STRUCTURE_WORDS: &str = include_str!("../assets/structure_words.tsv");

fn structure_words(body: &str) -> Vec<String> {
    let meaningful = body
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .map(|line| line.split_once(" #").map_or(line, |(before, _)| before))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    let mut words: Vec<String> = Vec::new();
    for (needle, expansion) in STRUCTURE_WORDS.lines().filter_map(|l| l.split_once('\t')) {
        if meaningful.contains(needle) {
            for word in expansion.split(' ') {
                if !words.iter().any(|w| w == word) {
                    words.push(word.to_owned());
                }
            }
        }
    }
    words
}

// ── normalization ───────────────────────────────────────────────────────

/// Function words that carry no retrieval signal, on top of the router's
/// English list. French articles, pronouns and politeness; English request
/// scaffolding (« please », « I want », « help me »). Signal words such as
/// « each », « before » or « only » are deliberately absent.
const EXTRA_STOPWORDS: &str = include_str!("../assets/retrieve_stopwords.txt");

/// Alias table · `keys => pattern words`, entries separated by `;`. Keys are
/// folded lowercase tokens (diacritics removed), so a French user who types
/// without accents lands on the same entry. Expansion ADDS the pattern words
/// beside the original token; a key that is also an English pattern word
/// (`resume`) keeps its own reading and gains the alias.
const ALIASES: &str = include_str!("../assets/retrieve_aliases.txt");

struct Lexicon {
    stopwords: BTreeSet<&'static str>,
    aliases: BTreeMap<&'static str, Vec<&'static str>>,
}

impl Lexicon {
    /// The alias entry for a folded token: the token itself, its naive
    /// singular, then its stem — so `réunions` reaches the `reunion` entry.
    fn alias_for(&self, token: &str) -> Option<&Vec<&'static str>> {
        if let Some(words) = self.aliases.get(token) {
            return Some(words);
        }
        if let Some(singular) = token.strip_suffix('s')
            && let Some(words) = self.aliases.get(singular)
        {
            return Some(words);
        }
        self.aliases.get(stem(token).as_str())
    }
}

static LEXICON: OnceLock<Lexicon> = OnceLock::new();

fn lexicon() -> &'static Lexicon {
    LEXICON.get_or_init(|| {
        let mut stopwords: BTreeSet<&'static str> =
            crate::text::STOPWORDS.iter().copied().collect();
        stopwords.extend(EXTRA_STOPWORDS.split_whitespace());
        let mut aliases: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
        for entry in ALIASES.split(';') {
            if let Some((keys, words)) = entry.split_once("=>") {
                let words: Vec<&'static str> = words.split_whitespace().collect();
                for key in keys.split_whitespace() {
                    aliases
                        .entry(key)
                        .or_default()
                        .extend(words.iter().copied());
                }
            }
        }
        Lexicon { stopwords, aliases }
    })
}

/// Fold one character: ASCII letters and digits pass, French diacritics map
/// to their base letter, everything else separates tokens.
fn fold(ch: char, buf: &mut String) -> bool {
    let low = ch.to_lowercase().next().unwrap_or(ch);
    let folded = match low {
        'a'..='z' | '0'..='9' => low,
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'œ' => {
            buf.push_str("oe");
            return true;
        }
        'æ' => {
            buf.push_str("ae");
            return true;
        }
        _ => return false,
    };
    buf.push(folded);
    true
}

/// Light suffix stemmer for folded ASCII tokens: plural first, then
/// participles, adverbs and the final `e`, each only when a usable stem
/// remains. Consistency on both sides matters more than linguistic truth.
fn stem(word: &str) -> String {
    let mut w = word.to_owned();
    let n = w.len();
    if n > 4 && w.ends_with("ies") {
        w.truncate(n - 3);
        w.push('y');
    } else if n > 3 && w.ends_with('s') && !w.ends_with("ss") {
        w.truncate(n - 1);
    }
    let n = w.len();
    if n >= 8 && w.ends_with("ing") {
        w.truncate(n - 3);
    } else if n >= 6 && w.ends_with("ed") {
        w.truncate(n - 2);
        if w.ends_with('i') {
            w.pop();
            w.push('y');
        }
    }
    let n = w.len();
    if n >= 6 && w.ends_with("ly") {
        w.truncate(n - 2);
    }
    if w.len() > 3 && w.ends_with('e') {
        w.pop();
    }
    w
}

/// The one token pipeline for documents and queries: fold · split · drop
/// function words and bare numbers · alias-expand · stem. Repeats stay: a
/// pattern word a family names in its title, intent and signature is its
/// core, and BM25 term frequency is how that core is weighed. Queries are
/// deduplicated afterwards so alias expansion never multiplies a word.
fn normalize(text: &str) -> Vec<String> {
    let lex = lexicon();
    let mut out: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut flush = |buf: &mut String| {
        if buf.is_empty() {
            return;
        }
        let token = std::mem::take(buf);
        if token.len() < 2
            || lex.stopwords.contains(token.as_str())
            || token.bytes().all(|b| b.is_ascii_digit())
        {
            return;
        }
        let mut words: Vec<&str> = vec![token.as_str()];
        if let Some(expansions) = lex.alias_for(&token) {
            words.extend(expansions.iter().copied());
        }
        for word in words {
            let stemmed = stem(word);
            if !stemmed.is_empty() {
                out.push(stemmed);
            }
        }
    };
    for ch in text.chars() {
        if !fold(ch, &mut buf) {
            flush(&mut buf);
        }
    }
    flush(&mut buf);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(hits: &[Hit]) -> Vec<&str> {
        hits.iter().map(|h| h.id.as_str()).collect()
    }

    #[test]
    fn the_embedded_corpus_indexes_every_family_and_skeleton() {
        let corpus = corpus();
        let families = corpus
            .docs
            .iter()
            .filter(|d| d.kind == HitKind::Family)
            .count();
        let skeletons = corpus
            .docs
            .iter()
            .filter(|d| d.kind == HitKind::Skeleton)
            .count();
        assert_eq!(
            families, 220,
            "the spec inventory projection must parse whole"
        );
        assert_eq!(skeletons, nika_pack::template_names().len());
        let unique: BTreeSet<&str> = corpus.docs.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(unique.len(), corpus.docs.len(), "ids never collide");
        assert!(corpus.index.is_finalized());
        assert_eq!(corpus.index.doc_count(), corpus.docs.len());
    }

    #[test]
    fn normalization_folds_french_and_unifies_inflections() {
        let tokens = normalize("Résumé des réunions : classer les tickets");
        for expected in ["summariz", "meeting", "classify", "ticket"] {
            assert!(
                tokens.iter().any(|t| t == expected),
                "{expected}: {tokens:?}"
            );
        }
        assert!(!tokens.iter().any(|t| t == "de" || t == "le"), "{tokens:?}");
        assert_eq!(stem("classified"), "classify");
        assert_eq!(stem("routes"), "rout");
        assert_eq!(stem("independently"), "independent");
        assert_eq!(stem("meetings"), "meeting");
        assert_eq!(stem("meeting"), "meeting");
        assert!(normalize("routing").iter().any(|t| t == "rout"));
        assert!(normalize("classification").iter().any(|t| t == "classify"));
        assert!(normalize("do the thing please").is_empty());
        assert!(normalize("42 · 7").is_empty());
    }

    #[test]
    fn nonsense_and_empty_queries_recall_nothing() {
        assert!(retrieve("zzz qqq xxx", 5).is_empty());
        assert!(retrieve("", 5).is_empty());
        assert!(retrieve("classify these tickets", 0).is_empty());
        assert!(retrieve_by_ops(&[], 5).is_empty());
    }

    #[test]
    fn hits_are_ranked_descending_and_bounded() {
        let hits = retrieve("classify these support tickets and route them", 3);
        assert_eq!(hits.len(), 3);
        assert!(hits.iter().all(|h| h.score > 0.0));
        assert!(hits.windows(2).all(|w| w[0].score >= w[1].score));
        assert!(ids(&hits).contains(&"B05"), "{:?}", ids(&hits));
    }

    #[test]
    fn a_family_is_recalled_beside_its_covering_skeleton() {
        let hits = retrieve("Summarize https://example.com", 5);
        assert!(
            hits.iter()
                .any(|h| h.id == "A01" && h.kind == HitKind::Family),
            "{:?}",
            ids(&hits)
        );
        assert!(
            hits.iter()
                .any(|h| h.id == "website-brief" && h.kind == HitKind::Skeleton),
            "{:?}",
            ids(&hits)
        );
        let skeleton = hits
            .iter()
            .find(|h| h.kind == HitKind::Skeleton)
            .expect("a skeleton hit");
        assert!(skeleton.patterns.iter().any(|p| p == "fetch"));
        assert!(!skeleton.title.is_empty());
    }

    #[test]
    fn ops_from_a_plan_recall_structurally_matching_candidates() {
        let hits = retrieve_by_ops(&["lookup", "classify", "draft"], 5);
        assert!(ids(&hits).contains(&"B06"), "{:?}", ids(&hits));
        let hits = retrieve_by_ops(&["fetch", "extract", "draft"], 5);
        let names = ids(&hits);
        assert!(
            names.contains(&"A01") || names.contains(&"website-brief"),
            "{names:?}"
        );
        let hits = retrieve_by_ops(&["send", "draft"], 5);
        assert!(
            hits.iter()
                .any(|h| h.patterns.iter().any(|p| p == "human_approve")),
            "an effect verb recalls gated families: {:?}",
            ids(&hits)
        );
        let hits = retrieve_by_ops(&["unknownop"], 5);
        assert!(hits.is_empty());
    }
}
