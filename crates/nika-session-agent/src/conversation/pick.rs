// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the person's answer picks among a question's offers. A whole line picks by protocol
//! alone: an offer's key or label as the whole line, the consent word when exactly one offer is
//! recommended, the refusal word. Any other line is read once, by the Session's own bounded
//! reading of the question, its offers and the line verbatim: never by the intelligence leading
//! the conversation, which reads tool replies and pages a person never wrote, and never by
//! matching words. A reply the reading cannot name picks nothing.

use nika_session_change::decision::{is_no, is_yes};
use nika_session_change::work::AskedQuestion;

/// What one answer picks among the offers of the question it answers.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Pick {
    /// The offer with this key.
    Offer(String),
    /// The person left the choice to Nika (« fais au mieux », « choisis ») or declined to
    /// choose again: the recommended offer when there is one, a value Nika chooses otherwise.
    Delegated,
    /// The person refused the offers.
    Declined,
    /// The line picks no offer: another value, a question, several offers.
    Nothing,
    /// Nothing could read the line: no offer is taken from it.
    Unread,
}

impl Pick {
    /// The pick a whole line makes by protocol alone, before any reading. `None` for a sentence:
    /// a sentence is read, never matched against words.
    #[must_use]
    pub fn of_line(question: &AskedQuestion, line: &str) -> Option<Self> {
        let whole = line.trim().trim_end_matches(['.', '!']).trim();
        let named = question.options.iter().find(|offer| {
            offer.key.eq_ignore_ascii_case(whole)
                || (!offer.label.is_empty() && offer.label.trim().eq_ignore_ascii_case(whole))
        });
        if let Some(offer) = named {
            return Some(Self::Offer(offer.key.clone()));
        }
        if is_no(whole) {
            return Some(Self::Declined);
        }
        if is_yes(whole) {
            let mut recommended = question.options.iter().filter(|offer| offer.recommended);
            if let (Some(only), None) = (recommended.next(), recommended.next()) {
                return Some(Self::Offer(only.key.clone()));
            }
        }
        None
    }

    /// The one bounded reading of `line` as the answer to `question`: the question in its own
    /// words, its offers by key (the recommended one said), the line verbatim, and one
    /// instruction — answer one offered key, `DELEGATE` or `NONE`.
    #[must_use]
    pub fn prompt(question: &AskedQuestion, line: &str) -> String {
        let offers: Vec<String> = (question.options.iter())
            .map(|offer| {
                let label = offer.label.trim();
                let recommended = if offer.recommended {
                    ", recommended"
                } else {
                    ""
                };
                format!("«{}» ({label}{recommended})", offer.key)
            })
            .collect();
        let (offered, instruction) = if offers.is_empty() {
            (
                "It offered no answers.".to_owned(),
                "Answer DELEGATE if the reply leaves the value to Nika or declines to give it \
                 again; NONE otherwise.",
            )
        } else {
            (
                format!("The offered answers are exactly: {}.", offers.join(" · ")),
                "Answer the key of the one offered answer the reply chooses or accepts (a plain \
                 acceptance accepts the recommended answer); DELEGATE if the reply leaves the \
                 choice to Nika or declines to choose again without rejecting the offers; NONE \
                 if it rejects them, gives another value, asks something or chooses several.",
            )
        };
        format!(
            "Nika, an automation tool, asked a human: «{asked}».\n{offered}\nThe human replied: \
             «{reply}».\n{instruction} One word only, nothing else.\nAnswer:",
            asked = question.question.trim(),
            reply = line.trim(),
        )
    }

    /// The pick a reading's reply names: an offered key exactly, `DELEGATE` or `NONE`, on one
    /// line, its `Answer:` cue and one pair of quotes aside. Any other reply picks nothing.
    #[must_use]
    pub fn read(reply: &str, question: &AskedQuestion) -> Self {
        let mut lines = reply.lines().map(str::trim).filter(|line| !line.is_empty());
        let (Some(first), None) = (lines.next(), lines.next()) else {
            return Self::Unread;
        };
        let word = first.strip_prefix("Answer:").map_or(first, str::trim);
        let word = unquoted(word);
        if let Some(offer) = question.options.iter().find(|offer| offer.key == word) {
            return Self::Offer(offer.key.clone());
        }
        match word {
            "DELEGATE" => Self::Delegated,
            "NONE" => Self::Nothing,
            _ => Self::Unread,
        }
    }
}

/// One pair of quotes around a word, removed.
fn unquoted(word: &str) -> &str {
    for (open, close) in [('"', '"'), ('\'', '\''), ('`', '`'), ('«', '»')] {
        if let Some(inner) = word.strip_prefix(open).and_then(|w| w.strip_suffix(close)) {
            return inner.trim();
        }
    }
    word
}

#[cfg(test)]
mod tests;
