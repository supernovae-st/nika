// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tokens of the closed rule grammar: words (folded for the tables, kept as written
//! for keys and values), numbers in canonical text, comparison symbols split off the word
//! they touch, and quoted spans kept whole. The grammar itself lives in `rules`.

use super::rules::Comparator;

/// A number followed by a size unit bounds prose, not data.
pub const SIZE_UNITS: &[&str] = &[
    "word",
    "words",
    "mot",
    "mots",
    "line",
    "lines",
    "ligne",
    "lignes",
    "bullet",
    "bullets",
    "puce",
    "puces",
    "sentence",
    "sentences",
    "phrase",
    "phrases",
    "character",
    "characters",
    "chars",
    "caractere",
    "caracteres",
    "paragraph",
    "paragraphs",
    "paragraphe",
    "paragraphes",
    "palabra",
    "palabras",
    "linea",
    "lineas",
    "oracion",
    "oraciones",
    "frase",
    "frasi",
    "parola",
    "parole",
    "riga",
    "righe",
    "caratteri",
    "wort",
    "worter",
    "zeile",
    "zeilen",
    "satz",
    "satze",
    "zeichen",
    "token",
    "tokens",
    "page",
    "pages",
];

/// A number followed by an attempt or turn unit bounds a loop, not data.
pub const ATTEMPT_UNITS: &[&str] = &[
    "attempt",
    "attempts",
    "try",
    "tries",
    "retry",
    "retries",
    "turn",
    "turns",
    "time",
    "times",
    "fois",
    "essai",
    "essais",
    "tentative",
    "tentatives",
    "tour",
    "tours",
    "iteration",
    "iterations",
    "round",
    "rounds",
    "cycle",
    "cycles",
    "intento",
    "intentos",
    "vuelta",
    "vueltas",
    "tentativo",
    "tentativi",
    "versuch",
    "versuche",
    "runde",
    "runden",
];

/// Lowercase with Latin diacritics folded to ASCII, so every table matches one spelling.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => out.push('a'),
            'ç' => out.push('c'),
            'è' | 'é' | 'ê' | 'ë' => out.push('e'),
            'î' | 'ï' | 'í' => out.push('i'),
            'ô' | 'ö' | 'ó' | 'õ' => out.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => out.push('u'),
            'ñ' => out.push('n'),
            'ß' => out.push_str("ss"),
            other => out.push(other),
        }
    }
    out
}

// ── tokens ───────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Word,
    /// A number with its canonical text (`100`, `1.5`, `-3`).
    Number(String),
    Symbol(Comparator),
    /// The exact text between a pair of quotes.
    Quoted,
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    /// The text as written (punctuation trimmed), for keys and values.
    pub(crate) original: String,
    /// Lowercase, diacritics folded, for the tables.
    pub(crate) folded: String,
    pub(crate) kind: Kind,
}

impl Token {
    pub(crate) fn word(&self) -> Option<&str> {
        matches!(self.kind, Kind::Word).then_some(self.folded.as_str())
    }
}

/// Comparison symbols, longest first so `>=` is never read as `>` then `=`.
const SYMBOLS: &[(&str, Comparator)] = &[
    (">=", Comparator::Ge),
    ("<=", Comparator::Le),
    ("==", Comparator::Eq),
    ("!=", Comparator::Ne),
    ("<>", Comparator::Ne),
    ("≥", Comparator::Ge),
    ("≤", Comparator::Le),
    ("≠", Comparator::Ne),
    (">", Comparator::Gt),
    ("<", Comparator::Lt),
    ("=", Comparator::Eq),
];

fn quote_close(open: char) -> Option<char> {
    match open {
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        '«' => Some('»'),
        '“' => Some('”'),
        '‘' => Some('’'),
        _ => None,
    }
}

fn is_punctuation(c: char) -> bool {
    matches!(
        c,
        '.' | ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '!' | '?' | '"' | '«' | '»'
    )
}

/// A number's canonical text: currency and percent signs dropped, a single decimal
/// comma read as a point, thousands separators removed; anything else is not a number.
pub(crate) fn number(word: &str) -> Option<String> {
    let trimmed = word
        .trim_start_matches(['€', '$', '£', '+'])
        .trim_end_matches(['€', '$', '£', '%']);
    let (sign, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", trimmed),
    };
    if !digits.starts_with(|c: char| c.is_ascii_digit())
        || !digits
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ','))
    {
        return None;
    }
    let canonical = if digits.contains(',') {
        let groups: Vec<&str> = digits.split(',').collect();
        let thousands = !digits.contains('.')
            && groups.len() > 1
            && groups.iter().skip(1).all(|g| g.len() == 3)
            && groups.first().is_some_and(|g| (1..=3).contains(&g.len()));
        if thousands {
            digits.replace(',', "")
        } else if groups.len() == 2 && !digits.contains('.') {
            digits.replace(',', ".")
        } else {
            return None;
        }
    } else {
        digits.to_owned()
    };
    let points = canonical.matches('.').count();
    if points > 1 || canonical.ends_with('.') {
        return None;
    }
    Some(format!("{sign}{canonical}"))
}

fn push_word(word: &str, out: &mut Vec<Token>) {
    let word = word.trim_matches(is_punctuation);
    if word.is_empty() {
        return;
    }
    let symbol = SYMBOLS
        .iter()
        .filter_map(|(s, c)| word.find(s).map(|at| (at, *s, *c)))
        .min_by_key(|(at, s, _)| (*at, std::cmp::Reverse(s.len())));
    if let Some((at, symbol, comparator)) = symbol {
        if let Some(before) = word.get(..at) {
            push_word(before, out);
        }
        out.push(Token {
            original: symbol.to_owned(),
            folded: symbol.to_owned(),
            kind: Kind::Symbol(comparator),
        });
        if let Some(after) = word.get(at + symbol.len()..) {
            push_word(after, out);
        }
        return;
    }
    let kind = number(word).map_or(Kind::Word, Kind::Number);
    out.push(Token {
        original: word.to_owned(),
        folded: fold(word),
        kind,
    });
}

/// Words, numbers, symbols and quoted spans. A quote opens only at the start of a token,
/// so an apostrophe inside a word (`l'ordre`, `n'est`) stays in the word.
pub(crate) fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let Some(first) = rest.chars().next() else {
            break;
        };
        if let Some(close) = quote_close(first)
            && let Some(inner_from) = rest.get(first.len_utf8()..)
            && let Some(end) = inner_from.find(close)
            && let Some(inner) = inner_from.get(..end).map(str::trim)
        {
            out.push(Token {
                original: inner.to_owned(),
                folded: fold(inner),
                kind: Kind::Quoted,
            });
            rest = inner_from
                .get(end + close.len_utf8()..)
                .unwrap_or_default()
                .trim_start();
            continue;
        }
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if let Some(word) = rest.get(..end) {
            push_word(word, &mut out);
        }
        rest = rest.get(end..).unwrap_or_default().trim_start();
    }
    out
}

pub(crate) fn phrase(tokens: &[Token], at: usize, width: usize) -> Option<String> {
    let slice = tokens.get(at..at + width)?;
    if slice.iter().any(|t| t.word().is_none()) {
        return None;
    }
    Some(
        slice
            .iter()
            .map(|t| t.folded.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_fold_currency_and_separators() {
        assert_eq!(number("100").as_deref(), Some("100"));
        assert_eq!(number("€100").as_deref(), Some("100"));
        assert_eq!(number("100€").as_deref(), Some("100"));
        assert_eq!(number("15%").as_deref(), Some("15"));
        assert_eq!(number("-3").as_deref(), Some("-3"));
        assert_eq!(number("1,5").as_deref(), Some("1.5"));
        assert_eq!(number("1,000").as_deref(), Some("1000"));
        assert_eq!(number("1,000,000").as_deref(), Some("1000000"));
        assert_eq!(number("12.50").as_deref(), Some("12.50"));
        for not in ["abc", "1.2.3", "1,2,3", "T-4471", "", "1.000,50"] {
            assert_eq!(number(not), None, "{not}");
        }
    }
}
