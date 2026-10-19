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
use nika_session_change::change::Witness;
use nika_session_change::outcome::QuestionId;
use nika_session_change::work::{
    AskedQuestion, AskedState, Binding, Offer, ProvenanceKind, ValueRole,
};
use serde_json::{Value, json};

use super::answer::{Answered, recommended};
use super::{
    Citations, Conversation, Facts, Mint, Pick, bare, in_line, resolution_role, same_literal,
};

/// The same offer shape can ask for missing information or show a candidate's default.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum QuestionUse {
    Ask,
    Propose,
}

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
    /// The person dropped it (« annuler »): asked again, it is refused.
    Dropped,
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
            Self::Dropped => "dropped",
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
            // A time zone is the schedule's, never the workflow's: told to write it in the
            // candidate, the author tried for ever (live gauntlet r05, 2026-10-16: 35 proposals
            // refused for a `schedule_zone` choice).
            if zone_like(&choice.key, &v.value) {
                return format!(
                    "`{}` is a time zone the candidate does not carry. The schedule's zone is no choice shown beside the candidate: give it as the schedule's `timezone` when the person's words or an option they picked name it, or ask it with `ask`, and leave it out of `open`. A zone the workflow itself uses is written in the candidate",
                    v.value
                );
            }
            format!(
                "the candidate does not carry `{}`, its default: write it in the candidate, or ask the choice with `ask`",
                v.value
            )
        })
}

/// Whether the choice `shown` covers the question `asked`: the same unresolved choice the person
/// was asked, now shown with its default. An open question of the same key asked in the same
/// words, its answer playing the same part, with the same single or multiple pick, free words and
/// prerequisites, and the same options, each by key in the same words with the same values: the
/// default the choice recommends may only add the values the candidate carries. A key reused
/// alone, or the same options asked another way, covers nothing.
fn covers(shown: &AskedQuestion, asked: &AskedQuestion) -> bool {
    let same_offer = |old: &Offer| {
        (shown.options.iter()).any(|new| {
            new.key == old.key
                && new.label.trim() == old.label.trim()
                && new.exclusive == old.exclusive
                && !old.recommended
                && if new.recommended {
                    old.values.iter().all(|value| new.values.contains(value))
                } else {
                    new.values == old.values
                }
        })
    };
    asked.state == AskedState::Open
        && shown.key == asked.key
        && shown.question.trim() == asked.question.trim()
        && shown.role == asked.role
        && shown.multi_select == asked.multi_select
        && shown.free_text == asked.free_text
        && shown.after == asked.after
        && shown.options.len() == asked.options.len()
        && asked.options.iter().all(same_offer)
}

/// Whether a choice keyed `key` offering `value` is a time zone: its key says so, or the value
/// is spelled as a zone of the time zone database (`Europe/Paris`, `UTC`).
fn zone_like(key: &str, value: &str) -> bool {
    let key = key.to_lowercase();
    let named = ["zone", "fuseau", "timezone", "tz"]
        .iter()
        .any(|w| key.contains(w));
    let spelled = value == "UTC"
        || value.split_once('/').is_some_and(|(area, place)| {
            area.chars().next().is_some_and(char::is_uppercase)
                && area.chars().all(char::is_alphabetic)
                && !place.is_empty()
                && place
                    .chars()
                    .all(|c| c.is_alphanumeric() || "_-+/".contains(c))
        });
    named || spelled
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
    /// A routine output derived for this candidate is a recommendation, not a human pick.
    /// A still-unanswered choice can retain its same default across another choice's answer.
    /// Otherwise an output a proposal kept, or the person delegated, remains protected.
    /// The ask door still refuses it; only a carried recommended default can be shown.
    pub(super) fn proposable_output_default(
        &self,
        binding: &Binding,
        key: &str,
        options: &[Offer],
    ) -> bool {
        let pending_default = self.open.iter().any(|question| {
            question.key == key
                && recommended(question).is_some_and(|offer| {
                    offer.values.iter().any(|value| {
                        value.role == binding.role && same_literal(&value.value, &binding.value)
                    })
                })
        });
        if binding.role != ValueRole::OutputPath
            || binding.provenance.kind != ProvenanceKind::Derived
            || binding.key.is_some()
            || self.accepted.iter().any(|kept| {
                kept.role == binding.role
                    && !(pending_default
                        && kept.provenance.kind == ProvenanceKind::Derived
                        && kept.key.is_none()
                        && same_literal(&kept.value, &binding.value))
            })
            || self
                .delegations
                .iter()
                .any(|choice| choice.scope == binding.role)
        {
            return false;
        }
        let Some(candidate) = self.candidate.as_ref() else {
            return false;
        };
        if !candidate.source.contains(bare(&binding.value))
            || !candidate.rows.iter().any(|row| {
                (row.kind == ResolutionKind::Derived
                    || (pending_default && row.kind == ResolutionKind::Retained))
                    && row.role == resolution_role(binding.role)
                    && same_literal(&row.value, &binding.value)
            })
        {
            return false;
        }
        let mut defaults = options.iter().filter(|offer| offer.recommended);
        let Some(default) = defaults.next() else {
            return false;
        };
        defaults.next().is_none()
            && default.values.iter().any(|value| {
                value.role == binding.role && same_literal(&value.value, &binding.value)
            })
    }

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
    /// unsettled, shown once, with exactly one recommended offer whose values the candidate (or
    /// the schedule beside it) already carries. `mint` gives each its identity, as it does a
    /// question's.
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
        // A schedule's zone rides beside the candidate, never in it: a choice may offer it.
        let carried = match args["schedule"]["timezone"].as_str() {
            Some(zone) => format!("{source}\n{zone}"),
            None => source.to_owned(),
        };
        let mut shown: Vec<AskedQuestion> = Vec::new();
        let mut refused = Vec::new();
        let choices = args["open"].as_array().into_iter().flatten();
        for (ordinal, choice) in choices.enumerate() {
            let key = choice["key"].as_str().unwrap_or_default();
            match self.question(
                choice,
                citations,
                (call, ordinal),
                mint,
                facts,
                QuestionUse::Propose,
            ) {
                Ok(question) => match unshowable(&question, &carried, &shown) {
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

    /// The proposal admitted shows `open`: each asked question a choice of it covers ([`covers`])
    /// is that choice now and no longer waits as a question. Nothing is answered or bound here: the
    /// person answers it under the choice's identity, or their consent keeps its default.
    pub(super) fn supersede_covered(&mut self, open: &[AskedQuestion]) {
        (self.asked).retain(|asked| !open.iter().any(|shown| covers(shown, asked)));
    }

    /// The asked question `key` was answered while the proposal shown is the candidate: that
    /// proposal does not carry the answer, so it is not saved as it is until the author proposes
    /// again.
    pub(super) fn answered_after_shown(&mut self, key: &str) {
        let shown = self.shown.as_ref().map(|shown| shown.number);
        if shown.is_some() && shown == self.candidate.as_ref().map(|c| c.number) {
            (self.picked_other).push(format!("`{key}` was answered after this proposal"));
        }
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

    /// Whether the person's `line` settles the shown choice `id` with the default the candidate
    /// already carries, so the proposal shown waits on unrevised (its next choice, or its save
    /// decision once none remain) with no author round trip: the whole line picks that choice's
    /// one recommended offer by protocol (its key or label, or a consent word), and nothing else
    /// waits or changed. No question is asked, no choice is open that its card (`shown`, by key)
    /// does not show, no other option was picked, the candidate is the revision shown with its
    /// bytes, and every default the proposal shows is still in them (or in the schedule's zone
    /// `beside` them). The last choice of a proposal that deferred the person's words to save or
    /// run to these picks stays the author's: it proposes those bytes again, and the mandate
    /// continues ([`Self::propose`]).
    #[must_use]
    pub fn keeps_default(
        &self,
        id: &QuestionId,
        line: &str,
        (shown, beside): (&[String], Option<&str>),
    ) -> bool {
        let Some(question) = self.open.iter().find(|question| question.id == *id) else {
            return false;
        };
        let Some(default) = recommended(question) else {
            return false;
        };
        if Pick::of_line(question, line) != Some(Pick::Offer(default.key.clone()))
            || self.unsaved(shown).is_some()
        {
            return false;
        }
        let (Some(candidate), Some(seen)) = (self.candidate.as_ref(), self.shown.as_ref()) else {
            return false;
        };
        if seen.number != candidate.number
            || seen.witness != Witness::of(candidate.source.as_bytes()).0
        {
            return false;
        }
        let carried = match beside {
            Some(zone) => format!("{}\n{zone}", candidate.source),
            None => candidate.source.clone(),
        };
        if (self.open.iter()).any(|choice| unshowable(choice, &carried, &[]).is_some()) {
            return false;
        }
        let mandate = (seen.mandate).is_some_and(|(_, acts)| acts.save || acts.run);
        !(mandate && self.open.len() == 1)
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "choices/derived_tests.rs"]
mod derived_tests;
