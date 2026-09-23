// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The cadence head of a sentence — « Chaque lundi matin, … », « Jeden Montagmorgen schick mir
//! … » — cut off before the clause is read: where it ends, with or without a comma, and what
//! it records (the plan's trigger, or a read step after « à partir de »). Beside `lexicon.rs`
//! at the 1,500-line file cap.
use super::cues::{CADENCE_WORDS, CLOCK_SUFFIXES, HEAD_FILLERS, TRIGGER_PREFIXES};
use super::{Reading, normalize, number_word};
use crate::plan::{Op, Step};
use crate::{hot, words};

/// Where a cadence head ends and its clause begins: `(head_end, rest_start)` as byte offsets
/// of the lowercase sentence. The head runs from the prefix over the cadence words (a named
/// day, a part of the day, a period, a German day-part compound), the small words between
/// them and the clock tokens, and ends after its last cadence word or clock token — with or
/// without a comma: « Jeden Montagmorgen schick mir … », « Every Monday morning send me … »,
/// « Tous les jours à 18h, … ». A prefix no cadence word follows keeps the first comma as its
/// end (an event, a supplied document: « Dès qu'un ticket arrive, … », « Pour chaque fichier
/// de ./x, … »); none at all is no head.
fn head_bounds(body_lower: &str, prefix: &str) -> Option<(usize, usize)> {
    let tail = body_lower.get(prefix.len()..).unwrap_or_default();
    let mut at = prefix.len();
    let mut end = None;
    for piece in tail.split_inclusive(' ') {
        let folded = hot::fold(piece);
        let word = folded.trim_matches(|c: char| !c.is_alphanumeric());
        let cadence = CADENCE_WORDS.contains(&word) || words::day_part_compound(word).is_some();
        let clock = end.is_some() && (clock_token(word) || CLOCK_SUFFIXES.contains(&word));
        let filler =
            HEAD_FILLERS.contains(&word) || clock_token(word) || number_word(word).is_some();
        if !word.is_empty() && !cadence && !clock && !filler {
            break;
        }
        let word_end = at + piece.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
        at += piece.len();
        if cadence || clock {
            end = Some(word_end);
        }
        if piece.trim_end().ends_with(',') {
            break;
        }
    }
    match end {
        Some(end) => {
            let rest = body_lower.get(end..).unwrap_or_default();
            let skipped = rest.len() - rest.trim_start_matches([' ', ',']).len();
            Some((end, end + skipped))
        }
        None => body_lower.find(',').map(|comma| (comma, comma + 1)),
    }
}

/// A clock token: `9`, `9:30`, `9h`, `9h30`, `18h`, `9am`, `9pm`.
fn clock_token(word: &str) -> bool {
    let word = word
        .strip_suffix("am")
        .or_else(|| word.strip_suffix("pm"))
        .unwrap_or(word);
    word.chars().any(|c| c.is_ascii_digit())
        && word
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, ':' | 'h'))
}

/// The sentence with its cadence or supplied-document head cut off: the head is recorded as
/// the plan's trigger (the first one) or, after « à partir de … », as a read step; the rest of
/// the sentence comes back in its original case and in lowercase. No head: the sentence as is.
pub(super) fn cut_head<'a>(
    sentence: &'a str,
    text: &str,
    reading: &mut Reading,
) -> (&'a str, String) {
    let Some(prefix) = TRIGGER_PREFIXES.iter().find(|p| text.starts_with(*p)) else {
        return (sentence, text.to_owned());
    };
    let Some((head_end, rest_start)) = head_bounds(text, prefix) else {
        return (sentence, text.to_owned());
    };
    let head = text.get(..head_end).unwrap_or_default().to_owned();
    if prefix.starts_with("à partir de")
        || prefix.starts_with("a partire da")
        || prefix.starts_with("a partir de")
        || prefix.starts_with("from the")
        || prefix.starts_with("starting from")
    {
        let detail = head
            .get(prefix.len()..)
            .unwrap_or_default()
            .trim()
            .to_owned();
        reading.plan.push_step(Step {
            op: Op::Read,
            evidence: sentence.to_owned(),
            detail,
            categories: Vec::new(),
        });
    } else if reading.plan.trigger.is_none() {
        reading.plan.trigger = Some(head);
    }
    let rest_lower = text.get(rest_start..).unwrap_or_default().trim().to_owned();
    let body = match normalize(sentence).find(&rest_lower) {
        Some(pos) => sentence.get(pos..).map_or(sentence, str::trim),
        None => sentence,
    };
    (body, rest_lower)
}

#[cfg(test)]
mod head_tests {
    use super::head_bounds;
    use crate::lexicon::read;

    #[test]
    fn a_schedule_no_comma_closes_is_the_head_in_german_portuguese_and_english() {
        for (intent, head, rest) in [
            (
                "Jeden Montagmorgen schick mir eine Zusammenfassung der offenen Tickets aus ./tickets.json",
                "jeden montagmorgen",
                "schick mir",
            ),
            (
                "Toda segunda-feira de manhã, envie-me um resumo dos tickets abertos de ./tickets.json",
                "toda segunda-feira de manhã",
                "envie-me",
            ),
            (
                "Every Monday morning at 9 pm send me the open tickets from ./tickets.json",
                "every monday morning at 9 pm",
                "send me",
            ),
            (
                "Tous les jours à 18h, lis ./tickets.json",
                "tous les jours à 18h",
                "lis",
            ),
            ("Every 2 hours, fetch ./x.json", "every 2 hours", "fetch"),
        ] {
            let reading = read(intent);
            assert_eq!(reading.plan.trigger.as_deref(), Some(head), "{intent}");
            let lower = intent.to_lowercase();
            let prefix = crate::lexicon::cues::TRIGGER_PREFIXES
                .iter()
                .find(|p| lower.starts_with(*p))
                .expect("a prefix");
            let (head_end, rest_start) = head_bounds(&lower, prefix).expect("a head");
            assert_eq!(&lower[..head_end], head, "{intent}");
            assert!(
                lower[rest_start..].starts_with(rest),
                "{intent}: {}",
                &lower[rest_start..]
            );
        }
    }

    #[test]
    fn a_prefix_no_cadence_word_follows_keeps_the_comma_and_none_is_no_head() {
        let event = "Dès qu'un ticket arrive, rédige un accusé de réception dans ./out/accuse.md";
        assert_eq!(
            read(event).plan.trigger.as_deref(),
            Some("dès qu'un ticket arrive"),
            "the elided event head"
        );
        let plain = "Quand un ticket arrive, rédige un accusé de réception dans ./out/accuse.md";
        assert_eq!(
            read(plain).plan.trigger.as_deref(),
            Some("quand un ticket arrive")
        );
        let bare = "Jeden Montag";
        assert_eq!(head_bounds(&bare.to_lowercase(), "jeden "), Some((12, 12)));
        assert_eq!(
            head_bounds("wenn ein ticket ankommt schreib", "wenn "),
            None
        );
    }
}
