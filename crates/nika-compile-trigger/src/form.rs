// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The form of a trigger clause (season 2): one unit of work per item, an order between the
//! program's own steps, a cadence, or an outside event.

use crate::words::{ARRIVAL_WORDS, COMPLETION_WORDS, EVENT_HEADS, SEQUENCE_HEADS, TIME_WORDS};
use nika_compile_reader::{hot, shape, words::recurrence};

/// What form a trigger clause takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerForm {
    /// One unit of work per item of the material ("for each file", "pour chaque ligne").
    Distributive,
    /// An order between the program's own steps ("once all three are done", "after that").
    Sequence,
    /// A cadence the program runs on ("every morning", "tous les matins", "at 9:00").
    Schedule,
    /// An outside event the program runs on ("when Stripe sends …", "dès qu'un ticket arrive").
    Event,
}

/// The folded phrase, one space between words, a leading space for whole-word heads.
fn padded(phrase: &str) -> String {
    let folded = hot::fold(phrase);
    let mut out = String::with_capacity(folded.len() + 2);
    out.push(' ');
    let mut space = false;
    for c in folded.chars() {
        if c.is_alphanumeric() || matches!(c, '\'' | ':' | '-') {
            out.push(c);
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out
}

fn words(padded: &str) -> impl Iterator<Item = &str> {
    padded.split(' ').filter(|w| !w.is_empty())
}

/// A clock time: `9:00`, `09:30`, `9h`, `9h30`, `14h`.
fn clock_time(word: &str) -> bool {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != ':');
    let (hours, rest) = match word.find([':', 'h']) {
        Some(at) => (&word[..at], &word[at + 1..]),
        None => return false,
    };
    !hours.is_empty()
        && hours.len() <= 2
        && hours.chars().all(|c| c.is_ascii_digit())
        && (rest.is_empty() || (rest.len() == 2 && rest.chars().all(|c| c.is_ascii_digit())))
}

/// Whether a distributive phrase quantifies over arriving items rather than a located set.
#[must_use]
pub fn arriving(phrase: &str) -> bool {
    let padded: String = format!(" {} ", hot::fold(phrase))
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' {
                c
            } else {
                ' '
            }
        })
        .collect();
    ARRIVAL_WORDS
        .iter()
        .any(|w| padded.contains(&format!(" {w} ")))
}

/// Read the form of a trigger clause.
#[must_use]
pub fn classify(phrase: &str) -> TriggerForm {
    let padded = padded(phrase);
    let mentions_time = words(&padded).any(|w| TIME_WORDS.contains(&w) || clock_time(w));
    let completes = words(&padded).any(|w| COMPLETION_WORDS.contains(&w));
    if SEQUENCE_HEADS
        .iter()
        .any(|h| padded.starts_with(&format!(" {h}")))
    {
        return TriggerForm::Sequence;
    }
    if EVENT_HEADS
        .iter()
        .any(|h| padded.starts_with(&format!(" {h}")))
    {
        return if completes {
            TriggerForm::Sequence
        } else {
            TriggerForm::Event
        };
    }
    // A recurrence without its cadence is a schedule; « every » in « every so often »
    // quantifies no item.
    if recurrence(phrase).is_some() {
        return TriggerForm::Schedule;
    }
    if shape::led_by_quantifier(phrase) && !mentions_time {
        return TriggerForm::Distributive;
    }
    if mentions_time {
        return TriggerForm::Schedule;
    }
    TriggerForm::Distributive
}
