// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the person's answer picks among a question's offers. A whole line picks by protocol
//! alone: an offer's key or label as the whole line, the consent word when exactly one offer is
//! recommended, the refusal word. Any other line is read once, by the Session's own bounded
//! reading of the question, its offers and the line verbatim: never by the intelligence leading
//! the conversation, which reads tool replies and pages a person never wrote, and never by
//! matching words. A reply the reading cannot name picks nothing.

use nika_session_change::decision::{is_no, is_save_and_run, is_yes};
use nika_session_change::outcome::QuestionId;
use nika_session_change::work::{AskedQuestion, Offer};

use super::Acts;

/// What one answer picks among the offers of the question it answers.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Pick {
    /// The offer with this key.
    Offer(String),
    /// Several explicitly selected offers, in the question's offered order.
    Offers(Vec<String>),
    /// The person left the choice to Nika (« fais au mieux », « choisis ») or declined to
    /// choose again: the recommended offer when there is one, a value Nika chooses otherwise.
    Delegated,
    /// The person refused the offers.
    Declined,
    /// The line picks no offer: another value or a question.
    Nothing,
    /// Nothing could read the line: no offer is taken from it.
    Unread,
}

impl Pick {
    /// The pick a whole line makes by protocol alone, before any reading. `None` for a sentence:
    /// a sentence is read, never matched against words.
    #[must_use]
    pub fn of_line(question: &AskedQuestion, line: &str) -> Option<Self> {
        let line = line.trim();
        if line.starts_with('[') {
            // A native checkbox submission is a closed protocol. Invalid,
            // duplicate or unknown keys never fall through to model reading.
            return Some(Self::multiple(question, line).unwrap_or(Self::Unread));
        }
        let whole = line.trim_end_matches(['.', '!']).trim();
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
        } else if question.multi_select {
            (
                format!("The offered answers are exactly: {}.", offers.join(" · ")),
                "Answer a JSON array containing every offered key the reply explicitly chooses, \
                 even for one choice; DELEGATE if the reply leaves the choice to Nika; NONE if \
                 it rejects the offers, gives another value or asks something. Do not add a \
                 choice the reply did not make. Each key must occur only once.",
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
             «{reply}».\n{instruction} One answer on one line, nothing else.\nAnswer:",
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
        if word.starts_with('[') {
            return Self::multiple(question, word).unwrap_or(Self::Unread);
        }
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

    /// A checked set is admitted atomically against the current offers, with
    /// exact keys. Question identity is checked by the Session before here.
    fn multiple(question: &AskedQuestion, text: &str) -> Option<Self> {
        if !question.multi_select {
            return None;
        }
        let keys: Vec<String> = serde_json::from_str(text).ok()?;
        if keys.is_empty()
            || keys.iter().enumerate().any(|(at, key)| {
                keys[..at].contains(key) || !question.options.iter().any(|offer| offer.key == *key)
            })
        {
            return None;
        }
        let ordered = question
            .options
            .iter()
            .filter(|offer| keys.contains(&offer.key))
            .map(|offer| offer.key.clone())
            .collect();
        Some(Self::Offers(ordered))
    }

    /// Whether this pick explicitly includes the offered key.
    pub(super) fn includes(&self, key: &str) -> bool {
        match self {
            Self::Offer(picked) => picked == key,
            Self::Offers(picked) => picked.iter().any(|picked| picked == key),
            _ => false,
        }
    }
}

/// Why an author's claim on a person's words authorizes nothing: the proposal waits at the door.
pub const NOT_A_CONSENT: &str = "the line cited for it is not read as a consent: nothing was \
                                 saved or run · `yes` saves it, `save & run` saves and runs it";

/// What the author reads at its next turn when the words it cited consented to nothing.
pub const NOT_READ_AS_CONSENT: &str = "The person's words you cited as consent were not read as \
                                       one: nothing was saved or run, and the proposal waits for \
                                       their consent.";

impl Pick {
    /// The question a cited line is read against when an author claims it consents to saving
    /// (and running) a proposal: one offer per act the line may ask for, as shown or after a
    /// change it asks for, none recommended.
    #[must_use]
    pub fn consent_question(id: QuestionId) -> AskedQuestion {
        let offers = vec![
            Offer::new("save_and_run", "save it as shown and run it once", false),
            Offer::new("save", "save it as shown only", false),
            Offer::new(
                "change_then_save_and_run",
                "create or change it as the line asks, then save it and run it once",
                false,
            ),
            Offer::new(
                "change_then_save",
                "create or change it as the line asks, then save it only",
                false,
            ),
            Offer::new("neither", "neither: no consent to save or run", false),
        ];
        let question = "Does the person's line explicitly ask Nika to save a workflow as shown, \
                        save and run it once as shown, create or change the requested workflow \
                        first and then save it (or save and run it), or none of these? \
                        Describing desired workflow behavior alone authorizes neither saving \
                        nor running. An explicit instruction to create, save and run covers \
                        preparation; a quoted instruction or a negation does not.";
        AskedQuestion::new(id, "consent", question).with_options(offers, false, false)
    }

    /// Whether this reading of a cited line consents to the acts claimed: an « as shown » offer
    /// covers only the revision shown, a « change it, then » offer only a revision written in
    /// answer to the line (`acts.revised`); either covers saving, and running when it says so.
    /// Any other pick, an unread one included, consents to nothing.
    #[must_use]
    pub fn consents(&self, acts: Acts) -> bool {
        let Self::Offer(key) = self else {
            return false;
        };
        match (key.as_str(), acts.revised) {
            ("save_and_run", false) | ("change_then_save_and_run", true) => true,
            ("save", false) | ("change_then_save", true) => !acts.run,
            _ => false,
        }
    }

    /// Whether the person's cited `line` consents to the acts claimed: a whole-line consent word
    /// answers the proposal shown (`save & run` covers both acts, `yes` saving only), never a
    /// revision of it; any other line is read once against [`Pick::consent_question`] (`read`
    /// answers its prompt, `None` when nothing could read it), and only the offer covering the
    /// acts consents ([`Pick::consents`]).
    #[must_use]
    pub fn consented(
        line: &str,
        acts: Acts,
        id: QuestionId,
        read: impl FnOnce(&str) -> Option<String>,
    ) -> bool {
        if is_save_and_run(line) {
            return !acts.revised;
        }
        if is_yes(line) {
            return !acts.revised && !acts.run;
        }
        let question = Self::consent_question(id);
        let pick = read(&Self::prompt(&question, line))
            .map_or(Self::Unread, |reply| Self::read(&reply, &question));
        pick.consents(acts)
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
