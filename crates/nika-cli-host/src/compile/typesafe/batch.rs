// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Several independent closed choices in ONE System One request (A1), sent through the seat's
//! single-attempt client. The wire form (the body, the reading of the reply bound by id, the
//! usage once per request) is [`system_one`]'s; this exchange adds how far the ONE request went.
//! An id the batch asks twice is never sent.

use std::{future::Future, pin::Pin};

use nika_onboard::compile::decide::{
    ChoiceAnswer, ChoiceBatch, ChoiceQuestion, DecisionError, system_one,
};

use super::{Delivery, ExchangeError, TypesafeSeat, Usage};

/// One System One request for a batch: each item's answer or why it has none, and what the wire
/// reported, once, for the request.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct BatchExchange {
    /// Each item's answer, or why it has none, in item order.
    pub answers: Vec<Result<ChoiceAnswer, DecisionError>>,
    /// Each item's outcome, in item order: `chosen` · `none` · `outside_options` ·
    /// `unanswered` · `repeated` · `malformed` · `not_sent` (its id asked twice) · `failed`
    /// (the request answered no item).
    pub outcomes: Vec<&'static str>,
    /// How far the ONE request went; `NotSent` when no item could be sent.
    pub delivery: Delivery,
    /// The model the response named.
    pub model: Option<String>,
    /// What the response reported it used, once for the request, whatever its items came to.
    pub usage: Usage,
    /// Ids the response answered that no sent item asked: recorded, never assigned.
    pub unasked: Vec<String>,
    /// Why the request as a whole answered no item, when it did not.
    pub error: Option<DecisionError>,
}

/// The object-safe future of one batch exchange.
pub type BatchExchangeFuture<'a> = Pin<Box<dyn Future<Output = BatchExchange> + Send + 'a>>;

impl TypesafeSeat {
    /// Exactly one physical request for every item of `batch` whose id it asks once, with what
    /// the wire reported: no retry, no fallback, no second request. Nothing is sent for an empty
    /// batch, nor when every id is asked twice.
    #[must_use]
    pub fn exchange_each<'a>(&'a self, batch: &'a ChoiceBatch) -> BatchExchangeFuture<'a> {
        Box::pin(async move {
            let (mut exchange, sent) = BatchExchange::before(batch);
            if sent.is_empty() {
                return exchange;
            }
            let questions: Vec<&ChoiceQuestion> = (sent.iter())
                .filter_map(|at| batch.items.get(*at).map(|item| &item.question))
                .collect();
            match self
                .post(&system_one::request(&self.model, &questions))
                .await
            {
                Err(failure) => exchange.failed(&sent, failure),
                Ok(response) => {
                    let status = response.status;
                    exchange.delivery = Delivery::Responded(status);
                    match system_one::read(&questions, status, &response.body) {
                        (usage, Err(message)) => {
                            let (error, delivery) = (DecisionError(message), exchange.delivery);
                            exchange.failed(
                                &sent,
                                ExchangeError {
                                    error,
                                    delivery,
                                    usage,
                                },
                            );
                        }
                        (usage, Ok(reply)) => exchange.settle(&sent, usage, reply),
                    }
                }
            }
            exchange
        })
    }
}

impl BatchExchange {
    /// The exchange before its request, and the items it may send (their ids asked once): an
    /// item whose id the batch asks more than once is never sent.
    fn before(batch: &ChoiceBatch) -> (Self, Vec<usize>) {
        let repeated = batch.repeated();
        let sent: Vec<usize> = (batch.items.iter().enumerate())
            .filter(|(_, item)| !repeated.contains(item.question.id.as_str()))
            .map(|(at, _)| at)
            .collect();
        let unsent = |id: &str| {
            let why = "more than once: an answer keyed by id cannot tell them apart; not sent";
            Err(DecisionError(format!("the batch asks `{id}` {why}")))
        };
        let exchange = Self {
            answers: (batch.items.iter())
                .map(|item| unsent(&item.question.id))
                .collect(),
            outcomes: vec!["not_sent"; batch.items.len()],
            delivery: Delivery::NotSent,
            model: None,
            usage: Usage::default(),
            unasked: Vec::new(),
            error: (sent.is_empty() && !batch.items.is_empty()).then(|| {
                DecisionError("every id of this batch is asked more than once; nothing sent".into())
            }),
        };
        (exchange, sent)
    }

    /// The request answered no item: every sent item fails with its error.
    fn failed(&mut self, sent: &[usize], failure: ExchangeError) {
        for at in sent {
            if let (Some(answer), Some(outcome)) =
                (self.answers.get_mut(*at), self.outcomes.get_mut(*at))
            {
                *answer = Err(failure.error.clone());
                *outcome = "failed";
            }
        }
        self.delivery = failure.delivery;
        self.usage = failure.usage;
        self.error = Some(failure.error);
    }

    /// The reply to the sent items, each bound by id, with the request's usage once.
    fn settle(&mut self, sent: &[usize], usage: Usage, reply: system_one::Reply) {
        for (at, (outcome, answer)) in sent.iter().zip(reply.items) {
            if let (Some(slot), Some(word)) =
                (self.answers.get_mut(*at), self.outcomes.get_mut(*at))
            {
                *slot = answer;
                *word = outcome;
            }
        }
        self.model = Some(reply.model);
        self.usage = usage;
        self.unasked = reply.unasked;
    }
}

#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod tests;
