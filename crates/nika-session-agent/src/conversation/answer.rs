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

use super::{Conversation, Pick, Slot, chosen_as, options_answered, single, stands_apart};

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
        let Some(option) = option.filter(|option| self.pick.includes(option)) else {
            return false;
        };
        self.cite == message
            && selected(&self.question, &self.pick).is_some_and(|offers| {
                offers.iter().any(|offer| {
                    offer.key == option
                        && offer
                            .values
                            .iter()
                            .any(|v| v.role == role && same_literal(&v.value, value))
                })
            })
    }

    /// What the person's line picked, in the author's words.
    fn said(&self) -> String {
        match &self.pick {
            Pick::Offer(key) => format!("picked `{key}`"),
            Pick::Offers(keys) => format!(
                "picked {}",
                keys.iter()
                    .map(|key| format!("`{key}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Pick::Delegated => "left the choice to Nika".to_owned(),
            Pick::Declined => "declined the offers".to_owned(),
            Pick::Nothing => "picked none of the offers".to_owned(),
            _ => "could not be read as one of the offers".to_owned(),
        }
    }

    /// Why the proposal that shows this choice cannot be saved as it is: the person picked an
    /// option other than the default it carries.
    fn instead(&self) -> Option<String> {
        let offers = selected(&self.question, &self.pick)?;
        let default = recommended(&self.question).map(|offer| offer.key.as_str());
        let is_default = offers.len() == 1 && default == Some(offers[0].key.as_str());
        (!is_default).then(|| {
            format!(
                "the person {} for `{}` with {}, and this proposal carries its default",
                self.said(),
                self.question.key,
                self.cite
            )
        })
    }
}

/// Resolve every selected offer before changing bindings. A multiple choice
/// cannot smuggle unknown/duplicate offers or conflicting single-valued roles.
pub(super) fn selected<'a>(question: &'a AskedQuestion, pick: &Pick) -> Option<Vec<&'a Offer>> {
    let keys: Vec<&str> = match pick {
        Pick::Offer(key) => vec![key.as_str()],
        Pick::Offers(keys) if question.multi_select && !keys.is_empty() => {
            keys.iter().map(String::as_str).collect()
        }
        _ => return None,
    };
    if keys
        .iter()
        .enumerate()
        .any(|(at, key)| keys[..at].contains(key))
    {
        return None;
    }
    let offers = keys
        .iter()
        .map(|key| question.options.iter().find(|offer| offer.key == *key))
        .collect::<Option<Vec<_>>>()?;
    let values = offers
        .iter()
        .flat_map(|offer| &offer.values)
        .collect::<Vec<_>>();
    if values.iter().enumerate().any(|(at, value)| {
        single(value.role)
            && values[..at].iter().any(|before| {
                before.role == value.role && !same_literal(&before.value, &value.value)
            })
    }) {
        return None;
    }
    Some(offers)
}

/// The one offer a question recommends, when it recommends exactly one.
pub(super) fn recommended(question: &AskedQuestion) -> Option<&Offer> {
    let mut recommended = question.options.iter().filter(|offer| offer.recommended);
    match (recommended.next(), recommended.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

impl Conversation {
    /// The questions the person's `line` answers that only a reading can settle, and the choices
    /// the proposal shows: the ones the whole line does not pick by protocol.
    #[must_use]
    pub fn to_read(&self, line: &str) -> Vec<AskedQuestion> {
        (self.asked.iter().chain(&self.open))
            .filter(|question| Pick::of_line(question, line).is_none())
            .cloned()
            .collect()
    }

    /// The person's line `cite` (its words, `line`) answered the questions asked now: they
    /// answer nothing again. What it picks — by protocol, else by `read` (the Session's reading
    /// of each question, by key) — is bound by the Session itself: the offer picked, or, for a
    /// line that leaves the choice to Nika, the recommended offer as delegated. A choice the
    /// proposal shows is bound only by such a pick; any other line leaves it open with its
    /// default. Returns what the author reads of it, one line per question.
    pub fn answered_by(
        &mut self,
        cite: &str,
        line: &str,
        read: &BTreeMap<String, Pick>,
    ) -> Vec<String> {
        self.answered_with(cite, line, |question| {
            Pick::of_line(question, line).or_else(|| read.get(&question.key).cloned())
        })
    }

    /// [`Self::answered_by`] with what each question waiting now picks given by `pick_of`.
    pub(super) fn answered_with(
        &mut self,
        cite: &str,
        line: &str,
        pick_of: impl Fn(&AskedQuestion) -> Option<Pick>,
    ) -> Vec<String> {
        let mut said = Vec::new();
        for question in std::mem::take(&mut self.asked) {
            let pick = pick_of(&question).unwrap_or(Pick::Unread);
            let answer = Answered {
                question,
                cite: cite.to_owned(),
                pick,
            };
            said.push(self.take(&answer, line));
            self.answered.insert(answer.question.key.clone(), answer);
        }
        for question in std::mem::take(&mut self.open) {
            let Some(pick @ (Pick::Offer(_) | Pick::Offers(_) | Pick::Delegated)) =
                pick_of(&question)
            else {
                self.open.push(question);
                continue;
            };
            let answer = Answered {
                question,
                cite: cite.to_owned(),
                pick,
            };
            let mut words = self.take(&answer, line);
            // Another option than the default the proposal carries: the author revises first.
            if let Some(why) = answer.instead() {
                self.picked_other.push(why);
                words.push_str(
                    ": the proposal shown carries its default, so write this pick in the candidate and propose again",
                );
            }
            said.push(words);
            self.answered.insert(answer.question.key.clone(), answer);
        }
        said
    }

    /// Bind what one answer picked; the author's line about it.
    pub(super) fn take(&mut self, answer: &Answered, line: &str) -> String {
        let (question, cite) = (&answer.question, answer.cite.as_str());
        let key = question.key.as_str();
        match &answer.pick {
            Pick::Offer(picked) => {
                let Some(offer) = question.options.iter().find(|o| o.key == *picked) else {
                    return format!("`{key}`: {cite} named no offer Nika showed; nothing is bound");
                };
                let provenance = Provenance::new(ProvenanceKind::Offered, cite)
                    .with_offer(key, Some(picked.clone()));
                self.bind_offer(question, offer, &provenance);
                self.remember_answer(question, (cite, line), &[offer], ProvenanceKind::Offered);
                format!(
                    "`{key}`: the person picked `{picked}` ({}) with {cite}; Nika bound its values",
                    offer.label
                )
            }
            Pick::Offers(_) => {
                let Some(offers) = selected(question, &answer.pick) else {
                    return format!(
                        "`{key}`: the selected offers are invalid or contain conflicting single values; nothing is bound"
                    );
                };
                // Remove the previous answer once, then bind the whole set.
                // Repeated calls to bind_offer would erase earlier selections.
                let slot = Slot::asked(&question.options);
                let (answered, recalled) = (&self.answered, &self.recalled);
                self.bindings.retain(|binding| {
                    let its = options_answered(answered, recalled, binding);
                    binding.key.as_deref() != Some(key)
                        && (stands_apart(binding, its, &slot)
                            || !(offers.iter().flat_map(|offer| &offer.values))
                                .any(|value| single(value.role) && value.role == binding.role))
                });
                for offer in &offers {
                    let provenance = Provenance::new(ProvenanceKind::Offered, cite)
                        .with_offer(key, Some(offer.key.clone()));
                    for value in &offer.values {
                        if !self.bindings.iter().any(|bound| {
                            bound.key.as_deref() == Some(key)
                                && bound.role == value.role
                                && same_literal(&bound.value, &value.value)
                                && bound.provenance == provenance
                        }) {
                            self.bindings.push(
                                Binding::new(value.role, &value.value, provenance.clone())
                                    .with_key(key),
                            );
                        }
                    }
                }
                self.remember_answer(question, (cite, line), &offers, ProvenanceKind::Offered);
                format!(
                    "`{key}`: the person {} with {cite}; Nika bound all selected values",
                    answer.said()
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
    /// with none, the choice is the author's to make, once; a run model, which only an offer
    /// binds, is offered again with one recommended, which Nika takes unasked (`left_model`).
    fn delegated(&mut self, question: &AskedQuestion, cite: &str, excerpt: &str) -> String {
        let key = question.key.as_str();
        let Some(offer) = recommended(question) else {
            let role = question.role.unwrap_or(ValueRole::Value);
            self.delegate(cite, excerpt, role);
            if role == ValueRole::RunModel {
                return format!("`{key}`: {}", super::left_model::offer_one_model(cite));
            }
            return format!(
                "`{key}`: the person left it to you with {cite}: choose it, bind it as {} citing their words, say what you chose, and never ask it again",
                chosen_as(question.role)
            );
        };
        let provenance = Provenance::new(ProvenanceKind::Delegated, cite)
            .with_excerpt(excerpt)
            .with_offer(key, Some(offer.key.clone()));
        self.bind_offer(question, offer, &provenance);
        self.remember_answer(
            question,
            (cite, excerpt),
            &[offer],
            ProvenanceKind::Delegated,
        );
        for value in &offer.values {
            self.delegate(cite, excerpt, value.role);
        }
        format!(
            "`{key}`: the person left the choice to Nika with {cite}; Nika took the recommended `{}` ({}) as delegated: say so, and that they can change it",
            offer.key, offer.label
        )
    }

    /// The values of `offer`, bound for `question` with `provenance`; a value of the same
    /// question, or of a single-valued role, is no longer bound, unless that output stands apart
    /// from the question (`stands_apart`), by the offers each question showed.
    pub(super) fn bind_offer(
        &mut self,
        question: &AskedQuestion,
        offer: &Offer,
        provenance: &Provenance,
    ) {
        let key = question.key.as_str();
        let slot = Slot::asked(&question.options);
        self.bindings.retain(|b| b.key.as_deref() != Some(key));
        for value in &offer.values {
            if single(value.role) {
                let (answered, recalled) = (&self.answered, &self.recalled);
                (self.bindings).retain(|b| {
                    let its = options_answered(answered, recalled, b);
                    b.role != value.role || stands_apart(b, its, &slot)
                });
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
            Some(answer) if answer.cite == message && answer.pick.includes(option) => {
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

    /// The labels of the offers the person's own answers in this request picked, each with the
    /// line that picked it: one offer, several checked together, or the default of a choice their
    /// consent kept. A line that left the choice to Nika, declined the offers or was not read
    /// picks none. A question answered before a reopen says what it picked from the kept record
    /// of that answer ([`Self::kept_picks`]).
    pub(super) fn picked(&self) -> impl Iterator<Item = (&str, &str)> {
        let live = (self.answered.values()).flat_map(|answer| {
            let offers = selected(&answer.question, &answer.pick).unwrap_or_default();
            (offers.into_iter()).map(move |offer| (answer.cite.as_str(), offer.label.as_str()))
        });
        live.chain(self.kept_picks())
    }

    /// Whether the question `key` was left to the author by the person.
    pub(super) fn left_to_author(&self, key: &str) -> Option<&str> {
        (self.answered.get(key))
            .filter(|answer| answer.pick == Pick::Delegated)
            .map(|answer| answer.cite.as_str())
    }
}

#[cfg(test)]
mod multi_tests;
