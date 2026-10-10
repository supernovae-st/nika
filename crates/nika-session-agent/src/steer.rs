// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The person's lines while a run is under way. A steering line enters the conversation after
//! the current calls, and the calls not yet run are skipped so the model reads it first; a
//! follow-up line waits until the model would end the run. Stop returns every queued line
//! unsent. The handle is shared: a host queues from its own thread while the run takes.
//!
//! Each line gets an identity (`l1`, `l2`, … within the conversation) and a state a host shows:
//! waiting, entered as the person's cited line, or returned unsent. The queue takes lines only
//! while a run reads it — the run opens it when it starts and closes it when it ends, returning
//! the lines it did not read — so a line is never left for a later run to read.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

pub use nika_session_change::work::{QueueMode, Queued, QueuedState};

/// The lines one run takes at most, whatever became of them: a further line is refused, so
/// neither the queue nor the snapshot that shows it grows without bound.
pub const MAX_QUEUED: usize = 32;

/// Why a line was not queued: a receipt a host shows, never an error of the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum QueueRefused {
    /// A blank line is no line.
    Blank,
    /// No run reads the queue now: nothing is under way, or what runs is not a conversation's
    /// run; the line is sent as the next turn instead.
    NotReading,
    /// The run took as many lines as it takes ([`MAX_QUEUED`]): the line is sent once it ends.
    Full,
}

impl QueueRefused {
    /// The receipt's word on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::NotReading => "not_reading",
            Self::Full => "full",
        }
    }
}

/// One line in the ledger, and whether a run has taken it to record.
#[derive(Debug)]
struct Slot {
    queued: Queued,
    taken: bool,
}

#[derive(Debug, Default)]
struct Ledger {
    open: bool,
    minted: u64,
    slots: Vec<Slot>,
}

/// The lines queued for one conversation, shared by the host and the run.
#[derive(Clone, Debug, Default)]
pub struct Steering {
    ledger: Arc<Mutex<Ledger>>,
}

impl Steering {
    /// An empty, closed queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a line that enters after the current calls.
    ///
    /// # Errors
    ///
    /// [`QueueRefused`]: the line is blank, no run reads the queue now, or the run took
    /// [`MAX_QUEUED`] lines.
    pub fn steer(&self, line: impl Into<String>) -> Result<Queued, QueueRefused> {
        self.push(QueueMode::Steer, line.into())
    }

    /// Queue a line that enters when the run would end.
    ///
    /// # Errors
    ///
    /// [`QueueRefused`]: the line is blank, no run reads the queue now, or the run took
    /// [`MAX_QUEUED`] lines.
    pub fn follow_up(&self, line: impl Into<String>) -> Result<Queued, QueueRefused> {
        self.push(QueueMode::FollowUp, line.into())
    }

    /// A run starts reading the queue: the lines of the run before it are forgotten.
    pub fn open(&self) {
        let mut ledger = self.lock();
        ledger.open = true;
        ledger.slots.clear();
    }

    /// The run ended: the queue takes no more lines, and every line it did not read comes back
    /// unsent (`returned` in [`Steering::records`]).
    pub fn close(&self) {
        let mut ledger = self.lock();
        ledger.open = false;
        return_waiting(&mut ledger);
    }

    /// Whether a run reads the queue now.
    #[must_use]
    pub fn reading(&self) -> bool {
        self.lock().open
    }

    /// Whether a line of `mode` waits.
    #[must_use]
    pub fn pending(&self, mode: QueueMode) -> bool {
        self.lock().slots.iter().any(|slot| fresh(slot, mode))
    }

    /// Take the waiting lines of `mode`, oldest first, for the run to record; the others wait.
    #[must_use]
    pub fn take(&self, mode: QueueMode) -> Vec<Queued> {
        let mut ledger = self.lock();
        let slots = ledger.slots.iter_mut().filter(|slot| fresh(slot, mode));
        slots
            .map(|slot| {
                slot.taken = true;
                slot.queued.clone()
            })
            .collect()
    }

    /// The line `id` entered the conversation as the person's line `cite`.
    pub fn entered(&self, id: &str, cite: &str) {
        let mut ledger = self.lock();
        if let Some(slot) = ledger.slots.iter_mut().find(|s| s.queued.id == id) {
            slot.queued.state = QueuedState::Entered {
                cite: cite.to_owned(),
            };
        }
    }

    /// Lines taken that never entered wait again, in their place.
    pub fn requeue(&self, ids: &[String]) {
        let mut ledger = self.lock();
        for slot in &mut ledger.slots {
            if ids.contains(&slot.queued.id) {
                slot.taken = false;
                slot.queued.state = QueuedState::Waiting;
            }
        }
    }

    /// Every waiting line comes back unsent, oldest first: what Stop and a parked run return.
    #[must_use]
    pub fn drain(&self) -> Vec<Queued> {
        return_waiting(&mut self.lock())
    }

    /// The lines of the current or last run, with their states, in the order they came.
    #[must_use]
    pub fn records(&self) -> Vec<Queued> {
        let ledger = self.lock();
        ledger
            .slots
            .iter()
            .map(|slot| slot.queued.clone())
            .collect()
    }

    fn push(&self, mode: QueueMode, line: String) -> Result<Queued, QueueRefused> {
        if line.trim().is_empty() {
            return Err(QueueRefused::Blank);
        }
        let mut ledger = self.lock();
        if !ledger.open {
            return Err(QueueRefused::NotReading);
        }
        if ledger.slots.len() >= MAX_QUEUED {
            return Err(QueueRefused::Full);
        }
        ledger.minted += 1;
        let queued = Queued::new(format!("l{}", ledger.minted), mode, line);
        ledger.slots.push(Slot {
            queued: queued.clone(),
            taken: false,
        });
        Ok(queued)
    }

    fn lock(&self) -> MutexGuard<'_, Ledger> {
        self.ledger.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A line of `mode` that waits and no run has taken.
fn fresh(slot: &Slot, mode: QueueMode) -> bool {
    !slot.taken && slot.queued.mode == mode && slot.queued.state == QueuedState::Waiting
}

/// Every waiting line returned unsent, oldest first.
fn return_waiting(ledger: &mut Ledger) -> Vec<Queued> {
    let waiting = ledger.slots.iter_mut();
    let waiting = waiting.filter(|slot| slot.queued.state == QueuedState::Waiting);
    waiting
        .map(|slot| {
            slot.taken = false;
            slot.queued.state = QueuedState::Returned;
            slot.queued.clone()
        })
        .collect()
}

#[cfg(test)]
mod tests;
