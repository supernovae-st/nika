// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The person's lines while a run is under way. A steering line enters the conversation after
//! the current calls, and the calls not yet run are skipped so the model reads it first; a
//! follow-up line waits until the model would end the run. Stop returns every queued line
//! unsent. The handle is shared: a host queues from its own thread while the run takes.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

/// How a line waits for a run under way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum QueueMode {
    /// It enters after the current calls.
    Steer,
    /// It enters when the run would end.
    FollowUp,
}

/// The lines queued for one conversation, shared by the host and the run.
#[derive(Clone, Debug, Default)]
pub struct Steering {
    queue: Arc<Mutex<VecDeque<(QueueMode, String)>>>,
}

impl Steering {
    /// An empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a line that enters after the current calls. A blank line is no line.
    pub fn steer(&self, line: impl Into<String>) {
        self.push(QueueMode::Steer, line.into());
    }

    /// Queue a line that enters when the run would end. A blank line is no line.
    pub fn follow_up(&self, line: impl Into<String>) {
        self.push(QueueMode::FollowUp, line.into());
    }

    /// Whether a line of `mode` waits.
    #[must_use]
    pub fn pending(&self, mode: QueueMode) -> bool {
        self.lock().iter().any(|(queued, _)| *queued == mode)
    }

    /// Take the lines of `mode`, oldest first; the others stay queued.
    #[must_use]
    pub fn take(&self, mode: QueueMode) -> Vec<String> {
        let mut queue = self.lock();
        let (taken, kept): (VecDeque<_>, VecDeque<_>) =
            queue.drain(..).partition(|(queued, _)| *queued == mode);
        *queue = kept;
        taken.into_iter().map(|(_, line)| line).collect()
    }

    /// Take every queued line, oldest first: what Stop returns unsent.
    #[must_use]
    pub fn drain(&self) -> Vec<(QueueMode, String)> {
        self.lock().drain(..).collect()
    }

    /// Put lines back at the front, in their order: lines taken that never entered.
    pub fn requeue(&self, lines: Vec<(QueueMode, String)>) {
        let mut queue = self.lock();
        for line in lines.into_iter().rev() {
            queue.push_front(line);
        }
    }

    fn push(&self, mode: QueueMode, line: String) {
        if !line.trim().is_empty() {
            self.lock().push_back((mode, line));
        }
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<(QueueMode, String)>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mode_is_taken_in_order_and_leaves_the_other_queued() {
        let steering = Steering::new();
        steering.follow_up("then summarize");
        steering.steer("use Le Monde too");
        steering.steer("   ");
        steering.steer("and TechCrunch");
        assert!(steering.pending(QueueMode::Steer));
        assert_eq!(
            steering.take(QueueMode::Steer),
            ["use Le Monde too", "and TechCrunch"]
        );
        assert!(!steering.pending(QueueMode::Steer));
        assert!(steering.pending(QueueMode::FollowUp));
        assert_eq!(steering.take(QueueMode::FollowUp), ["then summarize"]);
        assert!(steering.drain().is_empty());
    }

    /// A host queues from its own thread; the run sees the line through its clone.
    #[test]
    fn a_line_queued_on_another_thread_reaches_the_run() {
        let steering = Steering::new();
        let host = steering.clone();
        std::thread::scope(|scope| {
            scope.spawn(move || host.steer("stop reading the RSS feed"));
        });
        assert_eq!(
            steering.drain(),
            [(QueueMode::Steer, "stop reading the RSS feed".to_owned())]
        );
    }

    #[test]
    fn lines_put_back_keep_their_place_before_newer_ones() {
        let steering = Steering::new();
        steering.steer("newer");
        steering.requeue(vec![
            (QueueMode::Steer, "first".to_owned()),
            (QueueMode::FollowUp, "second".to_owned()),
        ]);
        let drained: Vec<String> = steering.drain().into_iter().map(|(_, l)| l).collect();
        assert_eq!(drained, ["first", "second", "newer"]);
    }
}
