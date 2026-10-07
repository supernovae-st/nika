// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Session's typed activity, drained from the turn's queue on the
//! shell's thread: to the workspace card as a typed update, timed by the
//! shell's own clock since the turn began, and to the busy row as the
//! Session's own line. Every run observation still goes to the desk.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use nika_display::activity::Activity;
use nika_display::activity_card::Update;

use super::Shell;
use crate::composer::Composer;
use crate::model::{Conversation, UiState};
use crate::session::feed::Observed;

/// The card's update for one activity the shell saw `at` since the turn
/// began: a call when the Session reported one, its phase otherwise.
fn update(activity: &Activity, at: Duration) -> Update<'_> {
    match &activity.call {
        Some(call) => Update::call(
            call.ordinal,
            &call.role,
            &call.model,
            activity.phase,
            call.state,
            at,
        ),
        None => Update::phase(activity.phase, &activity.note, activity.done),
    }
}

impl<C: Conversation + 'static> Shell<C> {
    /// Change the state and the draft while a scrolled transcript keeps its
    /// reading position: the rows they add or remove are below it.
    pub(super) fn keep_reading<T>(
        &mut self,
        update: impl FnOnce(&mut UiState, &mut Composer) -> T,
    ) -> T {
        let (state, desk, composer) = (&mut self.state, &self.desk, &mut self.composer);
        crate::scroll::preserve_reading_and_draft(state, desk, composer, update)
    }

    /// Show `busy` in the busy row, whose rows are the live area's.
    pub(super) fn set_busy(&mut self, busy: String) {
        self.keep_reading(|state, _| state.busy = Some(busy));
    }

    /// Drain the turn's queue: each typed activity to the card and the busy
    /// row (a finished one beside the current one), every run observation to
    /// the desk. `true` when anything arrived.
    pub(super) fn drain(
        &mut self,
        queue: &Receiver<Observed>,
        started: Instant,
        base: &mut Option<String>,
        last_done: &mut Option<String>,
    ) -> bool {
        let mut runs = Vec::new();
        let mut any = false;
        for observed in queue.try_iter() {
            any = true;
            let Observed::Activity(activity) = observed else {
                runs.push(observed);
                continue;
            };
            let at = started.elapsed();
            crate::scroll::preserve_reading(&mut self.state, &self.desk, &self.composer, |state| {
                state.observe(update(&activity, at));
            });
            let line = activity.line();
            if activity.done {
                *last_done = Some(line);
            } else {
                *base = Some(line);
            }
        }
        if !runs.is_empty() {
            self.desk.observe(runs.into_iter());
        }
        any
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use nika_display::activity::Phase;

    /// A phase activity becomes a phase update with the Session's words; the
    /// shell's time rides only on a call.
    #[test]
    fn a_phase_activity_keeps_its_phase_and_words() {
        let activity = Activity::now(Phase::Checking, "checking the proposal");
        match update(&activity, Duration::from_secs(9)) {
            Update::Phase { phase, note, done } => {
                assert_eq!(phase, Phase::Checking);
                assert_eq!(note, "checking the proposal");
                assert!(!done);
            }
            other => panic!("not a phase: {other:?}"),
        }
        let done = Activity::done(Phase::Understanding, "recorded 2 requirements");
        assert!(matches!(
            update(&done, Duration::ZERO),
            Update::Phase { done: true, .. }
        ));
    }
}
