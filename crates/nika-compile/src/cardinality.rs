// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bounds a request states on the produced content ("exactly 5 lines", "at least 12
//! lines", "3 bullets", "12 lignes max", "under 150 words"), read as typed intervals over
//! one unit. Two bounds on the same unit whose intervals do not meet contradict each other:
//! the request cannot be honoured as stated and is refused, never run on a prompt that
//! silently obeys one of them.

use super::cues::NUMBER_WORDS;
use super::rules::{self, Comparator};
use super::shape;

/// One bound on the produced content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Bound {
    /// The unit, folded and singular (`line`, `ligne`, `zeile`, `bullet`, `word`).
    pub unit: String,
    pub comparator: Comparator,
    pub value: u64,
}

impl Bound {
    /// The closed interval of values that honour the bound.
    fn interval(&self) -> Option<(u64, u64)> {
        match self.comparator {
            Comparator::Eq => Some((self.value, self.value)),
            Comparator::Ge => Some((self.value, u64::MAX)),
            Comparator::Gt => Some((self.value.saturating_add(1), u64::MAX)),
            Comparator::Le => Some((0, self.value)),
            Comparator::Lt => Some((0, self.value.saturating_sub(1))),
            Comparator::Ne => None,
        }
    }
    /// Whether both bounds can hold at once.
    pub(super) fn compatible(&self, other: &Self) -> bool {
        if self.unit != other.unit {
            return true;
        }
        match (self.interval(), other.interval()) {
            (Some((lo_a, hi_a)), Some((lo_b, hi_b))) => lo_a.max(lo_b) <= hi_a.min(hi_b),
            _ => true,
        }
    }
}

/// Words that make the bound an exact count, an upper bound or a lower bound, when they
/// stand beside the number or after the unit (EN · FR · ES · IT · DE · PT, folded).
const EXACT_WORDS: &[&str] = &[
    "exactly",
    "precisely",
    "exactement",
    "precisement",
    "exactamente",
    "esattamente",
    "genau",
    "exatamente",
];
const UPPER_WORDS: &[&str] = &[
    "max",
    "maxi",
    "maximum",
    "at most",
    "no more than",
    "not more than",
    "up to",
    "au plus",
    "au maximum",
    "pas plus de",
    "como maximo",
    "al massimo",
    "hochstens",
    "maximal",
    "no maximo",
];
const LOWER_WORDS: &[&str] = &[
    "min",
    "mini",
    "minimum",
    "at least",
    "no less than",
    "au moins",
    "au minimum",
    "al menos",
    "como minimo",
    "almeno",
    "mindestens",
    "minimal",
    "pelo menos",
    "no minimo",
];
/// Strict bounds: fewer than the number, or more than it.
const BELOW_WORDS: &[&str] = &[
    "under",
    "below",
    "less than",
    "fewer than",
    "moins de",
    "menos de",
    "meno di",
    "weniger als",
    "unter",
];
const ABOVE_WORDS: &[&str] = &[
    "over",
    "above",
    "more than",
    "plus de",
    "mas de",
    "piu di",
    "mehr als",
    "uber",
];

/// Irregular plurals of the size units; every other unit drops a trailing `s`.
const SINGULARS: &[(&str, &str)] = &[
    ("zeilen", "zeile"),
    ("worter", "wort"),
    ("satze", "satz"),
    ("righe", "riga"),
    ("parole", "parola"),
    ("frasi", "frase"),
    ("oraciones", "oracion"),
    ("caracteres", "caractere"),
    ("caratteri", "carattere"),
    ("paragraphes", "paragraphe"),
    ("paragraphs", "paragraph"),
];

fn singular(unit: &str) -> String {
    if let Some((_, one)) = SINGULARS.iter().find(|(many, _)| *many == unit) {
        return (*one).to_owned();
    }
    if unit.len() > 3 && unit.ends_with('s') && !unit.ends_with("ss") {
        return unit[..unit.len() - 1].to_owned();
    }
    unit.to_owned()
}

fn number(word: &str) -> Option<u64> {
    if word.chars().all(|c| c.is_ascii_digit()) && !word.is_empty() {
        return word.parse().ok();
    }
    NUMBER_WORDS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(_, n)| u64::from(*n))
}

fn phrase_in(table: &[&str], padded: &str) -> bool {
    table.iter().any(|w| padded.contains(&format!(" {w} ")))
}

/// The bound a constraint states, when it states one: a number (a digit run or a number
/// word) followed within two words by a size unit, with the comparator read from the words
/// around it (exact by default: "3 bullets" means three). A concurrency bound ("2 at a
/// time") is structure, not content, and states none.
pub(super) fn bound(constraint: &str) -> Option<Bound> {
    if super::bindings::parallel_bound(constraint).is_some() {
        return None;
    }
    let folded = shape::fold(constraint);
    let words: Vec<&str> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let unit_at = |i: usize| words.get(i).filter(|w| shape::SIZE_UNITS.contains(w));
    for (index, word) in words.iter().enumerate() {
        let Some(value) = number(word) else {
            continue;
        };
        let unit = unit_at(index + 1).or_else(|| {
            words
                .get(index + 1)
                .filter(|w| matches!(**w, "a" | "de" | "di" | "of"))
                .and_then(|_| unit_at(index + 2))
        });
        let Some(unit) = unit else {
            continue;
        };
        let before = format!(" {} ", words[index.saturating_sub(5)..index].join(" "));
        let after = format!(
            " {} ",
            words[(index + 1)..words.len().min(index + 5)].join(" ")
        );
        let comparator = if phrase_in(EXACT_WORDS, &before) || phrase_in(EXACT_WORDS, &after) {
            Comparator::Eq
        } else if phrase_in(UPPER_WORDS, &before) || phrase_in(UPPER_WORDS, &after) {
            Comparator::Le
        } else if phrase_in(LOWER_WORDS, &before) || phrase_in(LOWER_WORDS, &after) {
            Comparator::Ge
        } else if phrase_in(BELOW_WORDS, &before) {
            Comparator::Lt
        } else if phrase_in(ABOVE_WORDS, &before) {
            Comparator::Gt
        } else {
            rules::numeric_cue(before.trim()).unwrap_or(Comparator::Eq)
        };
        return Some(Bound {
            unit: singular(unit),
            comparator,
            value,
        });
    }
    None
}

/// The measure of a text a unit counts, for a run-time law over the drafted body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Measure {
    Lines,
    Bullets,
    Words,
    Sentences,
    Characters,
    Paragraphs,
}

impl Measure {
    /// The measure a folded singular unit names; a page or a token measures nothing at run.
    pub(super) fn of(unit: &str) -> Option<Self> {
        match unit {
            "line" | "ligne" | "zeile" | "riga" | "linea" | "linha" => Some(Self::Lines),
            "bullet" | "puce" | "stichpunkt" | "aufzahlungspunkt" | "vineta" | "marcador" => {
                Some(Self::Bullets)
            }
            "word" | "mot" | "wort" | "parola" | "palabra" | "palavra" => Some(Self::Words),
            "sentence" | "phrase" | "satz" | "frase" | "oracion" => Some(Self::Sentences),
            "character" | "char" | "caractere" | "zeichen" | "carattere" | "caracter" => {
                Some(Self::Characters)
            }
            "paragraph" | "paragraphe" | "absatz" | "paragrafo" | "parrafo" => {
                Some(Self::Paragraphs)
            }
            _ => None,
        }
    }
    /// jq over the body string (`.`) counting the measure: nonblank lines, bullet lines
    /// (`-`, `*`, `•` or a numbered marker), whitespace-separated words, sentences ended by
    /// `.`, `!` or `?`, characters, blank-line-separated paragraphs.
    pub(super) const fn jq(self) -> &'static str {
        match self {
            Self::Lines => r#"([split("\n")[] | select(test("\\S"))] | length)"#,
            Self::Bullets => {
                r#"([split("\n")[] | select(test("^\\s*([-*•]|[0-9]+[.)])\\s+"))] | length)"#
            }
            Self::Words => r#"([scan("\\S+")] | length)"#,
            Self::Sentences => r#"([scan("[^.!?]+[.!?]+")] | length)"#,
            Self::Characters => "length",
            Self::Paragraphs => r#"([split("\n\n")[] | select(test("\\S"))] | length)"#,
        }
    }
}

impl Bound {
    /// The jq predicate over the body string that holds exactly when the bound does; none
    /// when the unit measures nothing at run.
    pub(super) fn law(&self) -> Option<String> {
        let measure = Measure::of(&self.unit)?;
        Some(format!(
            "({} {} {})",
            measure.jq(),
            self.comparator.symbol(),
            self.value
        ))
    }
}

/// The conjunction of every measurable bound the constraints state, with the constraints it
/// covers, so the drafted text is judged at run and not only asked for in the prompt.
pub(super) fn body_law(constraints: &[String]) -> Option<(String, Vec<String>)> {
    let mut laws = Vec::new();
    let mut covered = Vec::new();
    for constraint in constraints {
        if let Some(law) = bound(constraint).and_then(|b| b.law()) {
            laws.push(law);
            covered.push(constraint.clone());
        }
    }
    if laws.is_empty() {
        return None;
    }
    Some((laws.join(" and "), covered))
}

/// The first pair of constraints whose bounds cannot both hold, as indexes into the slice.
pub(super) fn contradiction(constraints: &[String]) -> Option<(usize, usize)> {
    let bounds: Vec<(usize, Bound)> = constraints
        .iter()
        .enumerate()
        .filter_map(|(i, c)| bound(c).map(|b| (i, b)))
        .collect();
    for (a, (i, first)) in bounds.iter().enumerate() {
        for (j, second) in &bounds[a + 1..] {
            if !first.compatible(second) {
                return Some((*i, *j));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> (String, Comparator, u64) {
        let b = bound(text).unwrap_or_else(|| panic!("no bound in {text}"));
        (b.unit, b.comparator, b.value)
    }

    #[test]
    fn a_bound_is_a_number_a_unit_and_the_comparator_around_them() {
        assert_eq!(read("genau 5 Zeilen"), ("zeile".into(), Comparator::Eq, 5));
        assert_eq!(
            read("mindestens 12 Zeilen lang"),
            ("zeile".into(), Comparator::Ge, 12)
        );
        assert_eq!(read("3 bullets"), ("bullet".into(), Comparator::Eq, 3));
        assert_eq!(
            read("as five bullets"),
            ("bullet".into(), Comparator::Eq, 5)
        );
        assert_eq!(read("12 lignes max"), ("ligne".into(), Comparator::Le, 12));
        assert_eq!(
            read("a brief of under 150 words"),
            ("word".into(), Comparator::Lt, 150)
        );
        assert_eq!(read("at least 3 lines"), ("line".into(), Comparator::Ge, 3));
        assert_eq!(
            read("no more than 2 pages"),
            ("page".into(), Comparator::Le, 2)
        );
        assert_eq!(
            read("résumé de chaque en 3 lignes max"),
            ("ligne".into(), Comparator::Le, 3)
        );
        for none in [
            "in a warm tone",
            "Process at most 2 products at a time",
            "the top 3 countries",
            "never infer amounts",
        ] {
            assert!(bound(none).is_none(), "{none}");
        }
    }

    #[test]
    fn a_measurable_bound_lowers_to_a_law_over_the_body() {
        let (law, covered) = body_law(&[
            "3 bullets".into(),
            "under 150 words".into(),
            "in a warm tone".into(),
        ])
        .expect("two measurable bounds");
        assert_eq!(
            law,
            r#"(([split("\n")[] | select(test("^\\s*([-*•]|[0-9]+[.)])\\s+"))] | length) == 3) and (([scan("\\S+")] | length) < 150)"#
        );
        assert_eq!(covered, ["3 bullets", "under 150 words"]);
        assert_eq!(
            bound("12 lignes max").and_then(|b| b.law()).as_deref(),
            Some(r#"(([split("\n")[] | select(test("\\S"))] | length) <= 12)"#)
        );
        // A page or a token measures nothing at run; the prompt keeps the instruction.
        assert!(bound("at most 2 pages").and_then(|b| b.law()).is_none());
        assert!(body_law(&["in a warm tone".into()]).is_none());
    }

    #[test]
    fn two_bounds_on_one_unit_that_cannot_both_hold_contradict() {
        let c = |a: &str, b: &str| contradiction(&[a.to_owned(), b.to_owned()]);
        assert_eq!(
            c("genau 5 Zeilen", "mindestens 12 Zeilen lang"),
            Some((0, 1))
        );
        assert_eq!(c("exactly 3 bullets", "at most 2 bullets"), Some((0, 1)));
        assert_eq!(c("under 10 lines", "at least 12 lines"), Some((0, 1)));
        assert_eq!(c("at least 3 bullets", "5 bullets"), None);
        assert_eq!(c("3 bullets", "under 150 words"), None);
        assert_eq!(c("12 lignes max", "in a warm tone"), None);
        // Three constraints: the contradicting pair is named, not the innocent one.
        assert_eq!(
            contradiction(&[
                "in a warm tone".into(),
                "exactly 5 lines".into(),
                "at least 12 lines".into()
            ]),
            Some((1, 2))
        );
    }
}
