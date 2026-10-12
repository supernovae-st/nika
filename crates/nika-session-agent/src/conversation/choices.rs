// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The choices a proposal shows with their defaults (`propose`'s `open`, ADR-153): each in
//! `ask`'s shape, with exactly one recommended offer whose values the candidate already carries,
//! so the person reads the candidate at once instead of after a round of questions. Their next
//! line decides. A consent keeps every default shown, bound as the offer picked with that line; a
//! pick of another offer binds it and sends the author to revise; while a question or a choice
//! the proposal does not show is open, no consent saves it. A question a default can stand for
//! is never asked: the ask door refuses it ([`defaulted`]) and the proposal shows it here.

use std::fmt::Write as _;

use nika_compile_fidelity::fidelity::resolution::{Resolution, ResolutionKind};
use nika_session_change::work::{AskedQuestion, Binding, Offer, ValueRole};
use serde_json::{Value, json};

use super::answer::{Answered, recommended};
use super::{Citations, Conversation, Facts, Mint, Pick, bare, in_line, resolution_role};

/// What the author reads when it asks a question that recommends an option: a default can stand
/// for it, so the proposal shows it in `open`.
const DEFAULT_NOT_ASKED: &str = "it has a default, its recommended option, so it is not asked: \
                                 if the request can be done, write the default in the candidate \
                                 and propose with this choice in `open`; if no default can stand \
                                 for it (a fact only the person has, readings whose effects \
                                 differ materially, an authority only they can grant), ask it \
                                 with no recommended option; if the request cannot be done here, \
                                 say so and why, and propose nothing";

/// What the author reads when it asks for an output in the project, which always has a name it
/// can derive.
const OUTPUT_NOT_ASKED: &str = "an output in the project always has a name you can derive, so \
                                it is not asked: write one in the candidate and propose with the \
                                choice in `open`; ask it only when the person's later words ask \
                                to change it, cited in `reopens`; if the request cannot be done \
                                here, say so and why, and propose nothing";

/// Why the ask door does not ask a question, as the same word on every route: the person settled
/// it, left it to the author, or bound every value it offers; or a default can stand for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NotAsked {
    /// The person settled it.
    Settled,
    /// The person left it to the author.
    Delegated,
    /// Every value it offers is already bound.
    Bound,
    /// It recommends an option: the proposal shows it in `open` with that default.
    Default,
    /// It names an output in the project, which always has a name the author derives.
    Output,
}

impl NotAsked {
    /// The machine word a refused question carries.
    pub(super) const fn word(self) -> &'static str {
        match self {
            Self::Settled => "settled",
            Self::Delegated => "delegated",
            Self::Bound => "bound",
            Self::Default => "default",
            Self::Output => "output",
        }
    }

    /// The refusal the author reads for the question `key` that a default can stand for.
    pub(super) fn refusal(self, key: &str) -> Value {
        let why = match self {
            Self::Output => OUTPUT_NOT_ASKED,
            _ => DEFAULT_NOT_ASKED,
        };
        json!({"key": key, "reason": self.word(), "why": why})
    }
}

/// Whether a default can stand for the question `asked`, so the ask door does not ask it: it
/// recommends an option, or it names an output in the project (as its role or a value it offers)
/// and `reopens` does not cite the person's words asking to change it.
pub(super) fn defaulted(
    asked: &AskedQuestion,
    reopens: &Value,
    citations: &Citations,
) -> Option<NotAsked> {
    let output = asked.role == Some(ValueRole::OutputPath)
        || (asked.options.iter())
            .flat_map(|offer| &offer.values)
            .any(|value| value.role == ValueRole::OutputPath);
    if output && !cites_person(citations, reopens) {
        return Some(NotAsked::Output);
    }
    (asked.options.iter())
        .any(|offer| offer.recommended)
        .then_some(NotAsked::Default)
}

/// Whether `reopens` cites a line of the person's that holds the words it quotes.
fn cites_person(citations: &Citations, reopens: &Value) -> bool {
    let (Some(message), Some(excerpt)) = (reopens["message"].as_str(), reopens["excerpt"].as_str())
    else {
        return false;
    };
    (citations.get(message)).is_some_and(|line| in_line(line, excerpt, None))
}

/// Why `choice` cannot be shown beside the candidate `source`, after the choices `shown`.
fn unshowable(choice: &AskedQuestion, source: &str, shown: &[AskedQuestion]) -> Option<String> {
    if shown.iter().any(|q| q.key == choice.key) {
        return Some("it is shown twice".to_owned());
    }
    let Some(default) = recommended(choice) else {
        return Some(
            "a choice shown recommends exactly one option, its default; ask it with `ask`"
                .to_owned(),
        );
    };
    if default.values.is_empty() {
        return Some(format!("its default `{}` offers no value", default.key));
    }
    (default.values.iter())
        .find(|v| bare(&v.value).is_empty() || !source.contains(bare(&v.value)))
        .map(|v| {
            format!(
                "the candidate does not carry `{}`, its default: write it in the candidate, or ask the choice with `ask`",
                v.value
            )
        })
}

/// One option of a choice shown, as the card says it: its key, its label, whether it is the
/// default, and the values it binds.
fn option_words(offer: &Offer) -> String {
    let mut words = format!("`{}`", offer.key);
    let default = if offer.recommended { "(default)" } else { "" };
    for part in [offer.label.trim(), default] {
        if !part.is_empty() {
            words.push(' ');
            words.push_str(part);
        }
    }
    let values: Vec<&str> = offer.values.iter().map(|v| v.value.as_str()).collect();
    if !values.is_empty() {
        let _ = write!(words, ": {}", values.join(", "));
    }
    words
}

impl Conversation {
    /// The goal a proposal states: the author's `summary`, the `doubt` the verifier stated, then
    /// each choice it shows with its options, the default said, so the preview and the identity
    /// the person consents to cover them.
    #[must_use]
    pub fn card(&self, summary: &str, doubt: Option<&str>) -> String {
        let mut goal = summary.to_owned();
        if let Some(doubt) = doubt {
            let _ = write!(goal, "\n  doubt · {}", doubt.trim());
        }
        if !self.open.is_empty() {
            goal.push_str(
                "\n  open choices · `yes` keeps every default · name another option to change it",
            );
        }
        for choice in &self.open {
            let _ = write!(goal, "\n    · {}", choice.question.trim());
            for offer in &choice.options {
                let _ = write!(goal, "\n      {}", option_words(offer));
            }
        }
        goal
    }

    /// The choices the last proposal shows with their defaults, open until the person's line
    /// picks one or consents to them.
    #[must_use]
    pub fn open(&self) -> &[AskedQuestion] {
        &self.open
    }

    /// `propose`'s `open`, read before the proposal is judged: each choice in `ask`'s shape,
    /// unsettled, shown once, with exactly one recommended offer whose values the candidate
    /// already carries. `mint` gives each its identity, as it does a question's.
    ///
    /// # Errors
    ///
    /// Each choice the Session does not show, with why: the author asks it instead.
    pub fn open_choices(
        &self,
        citations: &Citations,
        args: &Value,
        call: Option<&str>,
        (mint, facts): (Mint<'_>, Facts<'_>),
    ) -> Result<Vec<AskedQuestion>, String> {
        let source = (self.candidate.as_ref()).map_or("", |c| c.source.as_str());
        let mut shown: Vec<AskedQuestion> = Vec::new();
        let mut refused = Vec::new();
        let choices = args["open"].as_array().into_iter().flatten();
        for (ordinal, choice) in choices.enumerate() {
            let key = choice["key"].as_str().unwrap_or_default();
            match self.question(choice, citations, (call, ordinal), mint, facts) {
                Ok(question) => match unshowable(&question, source, &shown) {
                    Some(why) => refused.push(format!("`{key}`: {why}")),
                    None => shown.push(question),
                },
                Err(settled) => {
                    let why = settled["why"].as_str().unwrap_or("it cannot be shown");
                    refused.push(format!("`{key}`: {why}; leave it out of `open`"));
                }
            }
        }
        if refused.is_empty() {
            Ok(shown)
        } else {
            Err(refused.join("\n"))
        }
    }

    /// The values the choices `open` show as their defaults, as the Session states them to the
    /// judge while it shows them: offered for the person's consent, bound only by it.
    #[must_use]
    pub fn defaults(open: &[AskedQuestion]) -> Vec<Resolution> {
        (open.iter().filter_map(recommended))
            .flat_map(|offer| &offer.values)
            .map(|v| Resolution::new(&v.value, ResolutionKind::Offered, resolution_role(v.role)))
            .collect()
    }

    /// The proposal that shows the choices no longer waits: they close unanswered.
    pub fn close_choices(&mut self) {
        self.open.clear();
        self.picked_other.clear();
    }

    /// Why the person's consent cannot save the proposal whose card shows the choices `shown`
    /// (by key): a question asked, or a choice open, that it does not show; or an option of a
    /// shown choice the person picked instead of its default, which the author revises first.
    #[must_use]
    pub fn unsaved(&self, shown: &[String]) -> Option<String> {
        if let Some(question) = self.asked.first() {
            return Some(format!(
                "Nika asked `{}`, which this proposal does not show: saving waits for the answer",
                question.key
            ));
        }
        if let Some(choice) = self.open.iter().find(|q| !shown.contains(&q.key)) {
            return Some(format!(
                "`{}` is a choice this proposal does not show: Nika shows it before saving",
                choice.key
            ));
        }
        (self.picked_other.first()).map(|why| format!("{why}: Nika revises it before saving"))
    }

    /// The person's line `cite` consented to the proposal that shows the open choices: each
    /// default is bound as the offer they picked with that line, and is theirs as a proposal they
    /// saw binds it. Returns the keys it bound.
    pub fn consented(&mut self, cite: &str) -> Vec<String> {
        let mut keys = Vec::new();
        for question in std::mem::take(&mut self.open) {
            let Some(default) = recommended(&question).map(|offer| offer.key.clone()) else {
                continue;
            };
            let answer = Answered {
                question,
                cite: cite.to_owned(),
                pick: Pick::Offer(default),
            };
            self.take(&answer, "");
            keys.push(answer.question.key.clone());
            self.answered.insert(answer.question.key.clone(), answer);
        }
        let consented: Vec<Binding> = (self.bindings.iter())
            .filter(|b| b.provenance.message == cite)
            .cloned()
            .collect();
        self.accepted.extend(consented.iter().cloned());
        self.seen.extend(consented);
        keys
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
