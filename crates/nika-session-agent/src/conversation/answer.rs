// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The person's answer to the questions asked: what it picks among their offers ([`Pick`]),
//! bound by the Session itself — the offer the person picked, or the recommended offer for an
//! answer that leaves the choice to Nika, visibly delegated — and what the author reads of it.
//! An `offered` value the author states is admitted only from the offer the person picked.

use std::collections::BTreeMap;

use nika_compile_fidelity::fidelity::resolution::same_literal;
use nika_session_change::work::{
    AskedQuestion, Binding, Delegation, Offer, Provenance, ProvenanceKind, ValueRole,
};

use super::{Conversation, Pick, chosen_as, single};

/// A question the person's line answered, with what the Session read it to pick.
#[derive(Clone, Debug)]
pub(super) struct Answered {
    pub(super) question: AskedQuestion,
    pub(super) cite: String,
    pub(super) pick: Pick,
}

impl Answered {
    /// Whether `value` (in `role`) belongs to the offer `option`, and the person picked that
    /// offer with `message`.
    pub(super) fn offers(
        &self,
        message: &str,
        option: Option<&str>,
        (role, value): (ValueRole, &str),
    ) -> bool {
        let Pick::Offer(picked) = &self.pick else {
            return false;
        };
        self.cite == message
            && option == Some(picked.as_str())
            && self.question.options.iter().any(|offer| {
                offer.key == *picked
                    && (offer.values.iter())
                        .any(|v| v.role == role && same_literal(&v.value, value))
            })
    }

    /// What the person's line picked, in the author's words.
    fn said(&self) -> String {
        match &self.pick {
            Pick::Offer(key) => format!("picked `{key}`"),
            Pick::Delegated => "left the choice to Nika".to_owned(),
            Pick::Declined => "declined the offers".to_owned(),
            Pick::Nothing => "picked none of the offers".to_owned(),
            _ => "could not be read as one of the offers".to_owned(),
        }
    }
}

/// The one offer a question recommends, when it recommends exactly one.
fn recommended(question: &AskedQuestion) -> Option<&Offer> {
    let mut recommended = question.options.iter().filter(|offer| offer.recommended);
    match (recommended.next(), recommended.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

impl Conversation {
    /// The questions the person's `line` answers that only a reading can settle: the ones the
    /// whole line does not pick by protocol.
    #[must_use]
    pub fn to_read(&self, line: &str) -> Vec<AskedQuestion> {
        (self.asked.iter())
            .filter(|question| Pick::of_line(question, line).is_none())
            .cloned()
            .collect()
    }

    /// The person's line `cite` (its words, `line`) answered the questions asked now: they
    /// answer nothing again. What it picks — by protocol, else by `read` (the Session's reading
    /// of each question, by key) — is bound by the Session itself: the offer picked, or, for a
    /// line that leaves the choice to Nika, the recommended offer as delegated. Returns what the
    /// author reads of it, one line per question.
    pub fn answered_by(
        &mut self,
        cite: &str,
        line: &str,
        read: &BTreeMap<String, Pick>,
    ) -> Vec<String> {
        let mut said = Vec::new();
        for question in std::mem::take(&mut self.asked) {
            let pick = Pick::of_line(&question, line)
                .or_else(|| read.get(&question.key).cloned())
                .unwrap_or(Pick::Unread);
            let answer = Answered {
                question,
                cite: cite.to_owned(),
                pick,
            };
            said.push(self.take(&answer, line));
            self.answered.insert(answer.question.key.clone(), answer);
        }
        said
    }

    /// Bind what one answer picked; the author's line about it.
    fn take(&mut self, answer: &Answered, line: &str) -> String {
        let (question, cite) = (&answer.question, answer.cite.as_str());
        let key = question.key.as_str();
        match &answer.pick {
            Pick::Offer(picked) => {
                let Some(offer) = question.options.iter().find(|o| o.key == *picked) else {
                    return format!("`{key}`: {cite} named no offer Nika showed; nothing is bound");
                };
                let provenance = Provenance::new(ProvenanceKind::Offered, cite)
                    .with_offer(key, Some(picked.clone()));
                self.bind_offer(key, offer, &provenance);
                format!(
                    "`{key}`: the person picked `{picked}` ({}) with {cite}; Nika bound its values",
                    offer.label
                )
            }
            Pick::Delegated => self.delegated(question, cite, line.trim()),
            Pick::Declined => {
                format!("`{key}`: the person declined the offers with {cite}; nothing is bound")
            }
            Pick::Nothing => format!(
                "`{key}`: {cite} picks none of the offers; bind only what they typed, or ask differently"
            ),
            _ => format!(
                "`{key}`: Nika could not read which offer {cite} picks, so none is bound: bind only what they typed, or ask them to answer with one of the offers"
            ),
        }
    }

    /// A line that leaves the choice to Nika: the recommended offer is bound as delegated, or,
    /// with none, the choice is the author's to make, once.
    fn delegated(&mut self, question: &AskedQuestion, cite: &str, excerpt: &str) -> String {
        let key = question.key.as_str();
        let Some(offer) = recommended(question) else {
            self.delegate(cite, excerpt, question.role.unwrap_or(ValueRole::Value));
            return format!(
                "`{key}`: the person left it to you with {cite}: choose it, bind it as {} citing their words, say what you chose, and never ask it again",
                chosen_as(question.role)
            );
        };
        let provenance = Provenance::new(ProvenanceKind::Delegated, cite)
            .with_excerpt(excerpt)
            .with_offer(key, Some(offer.key.clone()));
        self.bind_offer(key, offer, &provenance);
        for value in &offer.values {
            self.delegate(cite, excerpt, value.role);
        }
        format!(
            "`{key}`: the person left the choice to Nika with {cite}; Nika took the recommended `{}` ({}) as delegated: say so, and that they can change it",
            offer.key, offer.label
        )
    }

    /// The values of `offer`, bound for the question `key` with `provenance`; a value of the
    /// same question, or of a single-valued role, is no longer bound.
    fn bind_offer(&mut self, key: &str, offer: &Offer, provenance: &Provenance) {
        self.bindings.retain(|b| b.key.as_deref() != Some(key));
        for value in &offer.values {
            if single(value.role) {
                self.bindings.retain(|b| b.role != value.role);
            }
            let binding = Binding::new(value.role, &value.value, provenance.clone());
            self.bindings.push(binding.with_key(key));
        }
    }

    /// The person delegated the choice of `role` with `cite`, their words `excerpt`.
    fn delegate(&mut self, cite: &str, excerpt: &str, role: ValueRole) {
        let known = (self.delegations.iter())
            .any(|d| d.message == cite && d.excerpt == excerpt && d.scope == role);
        if !known {
            self.delegations.push(Delegation::new(cite, excerpt, role));
        }
    }

    /// Why an `offered` claim is refused: what the person's answer actually picked.
    pub(super) fn not_picked(
        &self,
        question: Option<&str>,
        option: Option<&str>,
        (message, value): (&str, &str),
    ) -> String {
        let option = option.unwrap_or("?");
        match question.and_then(|key| self.answered.get(key)) {
            Some(answer) if answer.cite == message && answer.pick == Pick::Offer(option.into()) => {
                format!(
                    "`{value}` is stated as offered, but it is no value of `{option}`, the offer the person picked with {message}"
                )
            }
            Some(answer) if answer.cite == message => format!(
                "`{value}` is stated as offered with `{option}`, but the person's {message} {}: only the offer the person picked is theirs",
                answer.said()
            ),
            _ => format!(
                "`{value}` is stated as offered, but no question the person answered with {message} carries it"
            ),
        }
    }

    /// The question Nika asked about `value` once the person's line at `at` was written: those
    /// words did not settle it, so the value is never `named` from them.
    pub(super) fn asked_about(&self, value: &str, at: u64) -> Option<&str> {
        let questions = (self.asked.iter()).chain(self.answered.values().map(|a| &a.question));
        let mut about = questions.filter(|question| {
            self.asked_at
                .get(&question.key)
                .is_some_and(|asked| *asked >= at)
                && (question.options.iter().flat_map(|offer| &offer.values))
                    .any(|offered| same_literal(&offered.value, value))
        });
        about.next().map(|question| question.key.as_str())
    }

    /// Whether the question `key` was left to the author by the person.
    pub(super) fn left_to_author(&self, key: &str) -> Option<&str> {
        (self.answered.get(key))
            .filter(|answer| answer.pick == Pick::Delegated)
            .map(|answer| answer.cite.as_str())
    }
}
