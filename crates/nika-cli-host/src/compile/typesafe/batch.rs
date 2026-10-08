// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Several independent closed choices asked together (A1), sent through the seat's
//! single-attempt client. ONE request carries every item whose id the batch asks once; the
//! service's refusal for capacity alone asks a request's items again in its two halves, as the
//! partition law of [`system_one`] decides, which also reads each reply by id and keeps each
//! physical request's usage once. This driver sends each request, tells the [`Requests`] watching
//! it before the request can leave and once it settled, and sends nothing more once the operator
//! stopped preparation. An id the batch asks twice is never sent.

use std::{future::Future, pin::Pin};

use nika_onboard::compile::decide::{ChoiceAnswer, ChoiceBatch, DecisionError, system_one};
use nika_providers::authoring::preparation::PreparationCosts;
use serde_json::Value;

use super::{Delivery, TypesafeSeat, Usage};

/// What a batch's requests came to: each item's answer or why it has none, and what the wire
/// reported across its physical requests.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct BatchExchange {
    /// Each item's answer, or why it has none, in item order.
    pub answers: Vec<Result<ChoiceAnswer, DecisionError>>,
    /// Each item's outcome, in item order: `chosen` · `none` · `outside_options` ·
    /// `unanswered` · `repeated` · `malformed` · `not_sent` (its id asked twice) · `failed`
    /// (its request answered no item) · `over_capacity` (refused for capacity alone) · `stopped`.
    pub outcomes: Vec<&'static str>,
    /// How far its requests went: `Unknown` when one may have been received without a response
    /// read, else the last response's status; `NotSent` when none left.
    pub delivery: Delivery,
    /// The model the first response that named one named.
    pub model: Option<String>,
    /// What its requests reported they used, each request once; a count is unknown unless every
    /// request that left reported it.
    pub usage: Usage,
    /// Ids a response answered that its request did not ask: recorded, never assigned.
    pub unasked: Vec<String>,
    /// The first final reason items have no answer, when there is one.
    pub error: Option<DecisionError>,
}

/// The object-safe future of one batch exchange.
pub type BatchExchangeFuture<'a> = Pin<Box<dyn Future<Output = BatchExchange> + Send + 'a>>;

/// Why an item is withheld once the operator stopped preparation.
const STOPPED: &str = "not sent: preparation stopped before its request could leave";

/// What watches each physical request of a batch (the Session's journal): each record is
/// written before its request can leave and settled after it. A request it cannot record is not
/// sent, and nothing after it.
pub(super) trait Requests: Sync {
    /// Record a request in flight before it leaves: its slot, or why it cannot be recorded.
    fn sending(&self, record: Value) -> Result<usize, DecisionError>;
    /// Settle the request recorded at `slot`.
    fn settled(&self, slot: usize, record: Value);
    /// Record items no request carries: never sent.
    fn unsent(&self, record: Value);
}

/// No watcher: the seat's own door keeps nothing beyond its exchange.
struct Unrecorded;

impl Requests for Unrecorded {
    fn sending(&self, _: Value) -> Result<usize, DecisionError> {
        Ok(0)
    }
    fn settled(&self, _: usize, _: Value) {}
    fn unsent(&self, _: Value) {}
}

impl TypesafeSeat {
    /// The batch's requests, each one physical request with what the wire reported: no retry, no
    /// fallback; only a refusal for capacity of several items asks them again, in halves. Nothing
    /// is sent for an empty batch, nor when every id is asked twice.
    #[must_use]
    pub fn exchange_each<'a>(&'a self, batch: &'a ChoiceBatch) -> BatchExchangeFuture<'a> {
        Box::pin(self.exchanged(batch, &Unrecorded))
    }

    /// [`Self::exchange_each`], each physical request shown to `watch` before it can leave.
    pub(super) async fn exchanged(
        &self,
        batch: &ChoiceBatch,
        watch: &dyn Requests,
    ) -> BatchExchange {
        let mut partition = system_one::Partition::of(batch);
        if let Some(record) = partition.unsent(batch, "not_sent") {
            watch.unsent(record);
        }
        let mut slots = Vec::new();
        while partition.waiting() {
            if PreparationCosts::stopped() {
                partition.withhold("stopped", &DecisionError(STOPPED.to_owned()));
                if let Some(record) = partition.unsent(batch, "stopped") {
                    watch.unsent(record);
                }
                break;
            }
            let Some((at, body)) = partition.begin(batch, &self.model) else {
                break;
            };
            match watch.sending(partition.record(at, batch, &slots)) {
                Ok(slot) => slots.push(slot),
                Err(error) => {
                    partition.lost(at, false, error.clone());
                    partition.withhold("not_sent", &error);
                    break;
                }
            }
            match self.post(&body).await {
                Ok(response) => partition.responded(at, batch, response.status, &response.body),
                Err(failure) => {
                    partition.lost(at, failure.delivery != Delivery::NotSent, failure.error);
                }
            }
            watch.settled(slots[at], partition.record(at, batch, &slots));
        }
        BatchExchange::of(partition)
    }
}

impl BatchExchange {
    /// The exchange a settled partition makes: its items' answers and outcomes, and what its
    /// physical requests reported across them.
    fn of(partition: system_one::Partition) -> Self {
        let reach = |attempt: &system_one::Attempt| match (attempt.sent, attempt.status) {
            (false, _) => Delivery::NotSent,
            (true, Some(status)) => Delivery::Responded(status),
            (true, None) => Delivery::Unknown,
        };
        let reached: Vec<Delivery> = partition.attempts.iter().map(reach).collect();
        let last = reached.iter().rev().find(|d| **d != Delivery::NotSent);
        let uncertain = reached
            .contains(&Delivery::Unknown)
            .then_some(Delivery::Unknown);
        Self {
            delivery: uncertain.or(last.copied()).unwrap_or(Delivery::NotSent),
            model: partition.model(),
            usage: partition.usage(),
            unasked: partition.unasked(),
            error: partition.error(),
            answers: partition.answers,
            outcomes: partition.outcomes,
        }
    }
}

#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod tests;
