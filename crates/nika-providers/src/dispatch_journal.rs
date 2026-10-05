// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The physical requests of one dispatch, kept where the dispatch's own
//! future cannot take them down (B7 · E17-F1 · E13-F4).
//!
//! A dispatch future can be dropped in the middle of a request: a
//! `fail_fast` fan-out aborts its in-flight siblings (spec 03), and a task
//! `timeout:` drops the attempt it bounds. The request has already crossed the
//! transport, and the [`crate::retry::TransportReport`] being built dies with
//! the future. Inside [`DispatchJournal::observe`], each request the registry
//! makes is recorded in explicit states: pre-send when the registry is about to
//! call its wire, sent when the wire hands it to the transport, returned with
//! its [`InferenceCall`] evidence, or withdrawn when the wire refused first. A
//! dispatch dropped first hands its caller every sent or returned request,
//! once. A pre-send request never left and is never reported as sent. Outside
//! `observe`, nothing is recorded.

use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};

use nika_types::cost::InferenceCall;

/// One request's state in the journal.
#[derive(Debug, Clone)]
enum Recorded {
    /// The registry is about to call its wire: reserved at most, never sent.
    PreSend,
    /// Handed to the transport, not answered: its charge is unknown.
    Sent(InferenceCall),
    /// Answered, with its evidence (which may itself leave the charge unknown).
    Returned(InferenceCall),
    /// The wire refused before anything crossed the transport.
    Withdrawn,
}

#[derive(Debug, Default)]
struct Requests {
    recorded: Vec<Recorded>,
    /// The request the registry is handing to its wire now.
    current: Option<usize>,
}

tokio::task_local! {
    /// The journal of the dispatch being polled.
    static JOURNAL: DispatchJournal;
}

/// The physical requests of one dispatch, shared with the caller that scoped
/// it (see the module documentation).
#[derive(Debug, Clone, Default)]
pub struct DispatchJournal(Arc<Mutex<Requests>>);

impl DispatchJournal {
    /// Run `dispatch` under a fresh journal. A returned dispatch carries its
    /// own evidence and `lost` is never called. A dispatch dropped first calls
    /// `lost` once with every request it sent: unanswered ones bare (charge
    /// unknown), answered ones with their evidence. Nothing is reported for a
    /// dispatch that sent nothing.
    ///
    /// CANCEL SAFETY: cancel-safe; dropping it is the case it exists for.
    pub async fn observe<F, L>(dispatch: F, lost: L) -> F::Output
    where
        F: Future,
        L: FnOnce(Vec<InferenceCall>),
    {
        struct OnDrop<L: FnOnce(Vec<InferenceCall>)>(Option<(DispatchJournal, L)>);
        impl<L: FnOnce(Vec<InferenceCall>)> Drop for OnDrop<L> {
            fn drop(&mut self) {
                if let Some((journal, lost)) = self.0.take() {
                    let crossed = journal.crossed();
                    if !crossed.is_empty() {
                        lost(crossed);
                    }
                }
            }
        }
        let journal = Self::default();
        let mut on_drop = OnDrop(Some((journal.clone(), lost)));
        let output = JOURNAL.scope(journal, dispatch).await;
        on_drop.0 = None;
        output
    }

    /// Keep every physical request while this future runs, including successful completion.
    /// The caller retains this journal across a dropped future; no admission is inferred.
    pub async fn capture<F: Future>(&self, dispatch: F) -> F::Output {
        JOURNAL.scope(self.clone(), dispatch).await
    }

    /// Every sent request so far, preserving incomplete usage after cancellation.
    #[must_use]
    pub fn snapshot(&self) -> Vec<InferenceCall> {
        self.crossed()
    }

    /// A request crossed transport without a recorded response. Its charge stays uncertain.
    #[must_use]
    pub fn unfinished(&self) -> bool {
        self.lock()
            .recorded
            .iter()
            .any(|r| matches!(r, Recorded::Sent(_)))
    }

    /// A poisoned lock still holds a plain record: recover it, never lose it.
    fn lock(&self) -> MutexGuard<'_, Requests> {
        match self.0.lock() {
            Ok(requests) => requests,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Every request that crossed the transport, in order.
    fn crossed(&self) -> Vec<InferenceCall> {
        self.lock()
            .recorded
            .iter()
            .filter_map(|recorded| match recorded {
                Recorded::Sent(call) | Recorded::Returned(call) => Some(call.clone()),
                Recorded::PreSend | Recorded::Withdrawn => None,
            })
            .collect()
    }
}

/// One request the registry is about to hand to its wire, inside a journal.
pub(crate) struct Entry {
    journal: DispatchJournal,
    index: usize,
}

/// Record a request the registry is about to hand to its wire: pre-send.
pub(crate) fn open() -> Option<Entry> {
    JOURNAL
        .try_with(|journal| {
            let mut requests = journal.lock();
            requests.recorded.push(Recorded::PreSend);
            let index = requests.recorded.len() - 1;
            requests.current = Some(index);
            Entry {
                journal: journal.clone(),
                index,
            }
        })
        .ok()
}

/// The wire hands the current request to the transport: pre-send becomes
/// sent, with the bare call the wire has just opened. Every wire calls this at
/// its send point, and nothing awaits between it and the post.
pub(crate) fn sent(call: Option<&InferenceCall>) {
    let _ = JOURNAL.try_with(|journal| {
        let mut requests = journal.lock();
        let Some(index) = requests.current else {
            return;
        };
        if let Some(slot) = requests.recorded.get_mut(index)
            && matches!(slot, Recorded::PreSend)
        {
            *slot = Recorded::Sent(call.cloned().unwrap_or_default());
        }
    });
}

impl Entry {
    /// The wire returned: keep the request's evidence, or withdraw it when it
    /// never crossed the transport. A sent request that returned no evidence
    /// stays sent: its charge is unknown.
    pub(crate) fn settle(self, call: Option<&InferenceCall>) {
        let mut requests = self.journal.lock();
        if requests.current == Some(self.index) {
            requests.current = None;
        }
        if let Some(slot) = requests.recorded.get_mut(self.index) {
            match (call, &*slot) {
                (Some(call), _) => *slot = Recorded::Returned(call.clone()),
                (None, Recorded::PreSend) => *slot = Recorded::Withdrawn,
                (None, _) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests;
