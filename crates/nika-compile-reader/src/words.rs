// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The day-part compounds, the numbered days of a month and the recurrences a request names
//! (`day_part_compound`, `day_of_month`, `recurrence`), read by the reader, the compiler and
//! the trigger reading. Moved here from
//! nika-compile at the 15k prod-LOC wall (2026-09-22), unchanged; the word tables of the
//! proposal merge and their classifiers ascended to `nika_compile_clauses::words` (ADR-145).

/// The German day-part compound a schedule names as one word — « Montagmorgen »,
/// « Freitagabend » — split into its day and its part, both folded (`hot::fold`). None for
/// any other word.
#[must_use]
pub fn day_part_compound(folded: &str) -> Option<(&'static str, &'static str)> {
    let day = GERMAN_DAYS
        .iter()
        .copied()
        .find(|d| folded.starts_with(d))?;
    let part = folded.get(day.len()..)?;
    GERMAN_DAY_PARTS
        .iter()
        .copied()
        .find(|p| *p == part)
        .map(|part| (day, part))
}

/// The day of the month a folded word numbers: digits with or without an ordinal ending
/// (« 1er », « 2e », « 15th »), or the first by name (« first », « premier »). Whether every
/// month has that day is the schedule's reading to judge, never this word's.
#[must_use]
pub fn day_of_month(folded: &str) -> Option<u32> {
    if matches!(folded, "first" | "premier" | "premiere") {
        return Some(1);
    }
    let digits = folded.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let ending = folded.get(digits.len()..)?;
    let ordinal = ["", "st", "nd", "rd", "th", "er", "re", "ere", "e", "eme"].contains(&ending);
    let numeral =
        !digits.is_empty() && digits.len() <= 2 && digits.bytes().all(|b| b.is_ascii_digit());
    if !ordinal || !numeral {
        return None;
    }
    digits.parse().ok()
}

/// The byte span of the first recurrence a text states without its cadence
/// ([`crate::trigger_words::RECURRENT`] · « régulièrement », « de temps en temps »,
/// « regularly », « from time to time »): whole words compared folded ([`crate::hot::fold`]),
/// the span in the text's own spelling. None when the text states none — « régularité »,
/// « irregular » and « un rapport régulier » are not one.
#[must_use]
pub fn recurrence(text: &str) -> Option<(usize, usize)> {
    let mut words: Vec<(usize, usize, String)> = Vec::new();
    let mut start = None;
    for (at, c) in text.char_indices().chain([(text.len(), ' ')]) {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                let word = text.get(from..at).unwrap_or_default();
                words.push((from, at, super::hot::fold(word)));
                start = None;
            }
            _ => {}
        }
    }
    (0..words.len()).find_map(|i| {
        super::trigger_words::RECURRENT.iter().find_map(|phrase| {
            let wanted: Vec<&str> = phrase.split(' ').collect();
            let window = words.get(i..i + wanted.len())?;
            let same = window
                .iter()
                .zip(&wanted)
                .all(|((_, _, word), want)| word == want);
            let (first, last) = (window.first()?, window.last()?);
            same.then_some((first.0, last.1))
        })
    })
}

const GERMAN_DAYS: &[&str] = &[
    "montag",
    "dienstag",
    "mittwoch",
    "donnerstag",
    "freitag",
    "samstag",
    "sonntag",
];

const GERMAN_DAY_PARTS: &[&str] = &[
    "morgen",
    "morgens",
    "fruh",
    "vormittag",
    "vormittags",
    "mittag",
    "mittags",
    "nachmittag",
    "nachmittags",
    "abend",
    "abends",
    "nacht",
    "nachts",
];
