// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The choices a proposal shows with their defaults (`propose`'s `open`, ADR-153): each in
//! `ask`'s shape, with exactly one recommended offer whose values the candidate already carries,
//! so the person reads the candidate at once instead of after a round of questions. Their next
//! line decides. A consent keeps every default shown, bound as the offer picked with that line; a
//! pick of another offer binds it and sends the author to revise; while a question or a choice
//! the proposal does not show is open, no consent saves it.

use std::fmt::Write as _;

use nika_compile_fidelity::fidelity::resolution::{Resolution, ResolutionKind};
use nika_session_change::work::{AskedQuestion, Binding, Offer};
use serde_json::Value;

use super::answer::{Answered, recommended};
use super::{Citations, Conversation, Facts, Mint, Pick, bare, resolution_role};

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
