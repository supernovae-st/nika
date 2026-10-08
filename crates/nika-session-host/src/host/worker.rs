// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The one thread that owns the runtime: a turn through [`SessionRuntime::submit`] with the
//! published `Waiting`, the Session's Stop token armed before it, the withdrawal of a stopped
//! preparation's late result before anything is published, then what the turn asked of the run
//! door (nothing once a Stop won: its run is not started), then one publication.

use std::sync::mpsc;

use nika_cli_host::lane::RunSink;
use nika_session::SessionRuntime;
use nika_session::outcome::ReviewId;
use nika_session::work::Waiting;

use super::{Job, Shared, publish};
use crate::run::{RunDoor, RunStep};
use crate::wire::{ActivityWire, Effect, Outcome, TurnPhase, project};

/// Why a run a stopped turn requested was not started.
const STOPPED: &str =
    "the Stop accepted while this turn prepared ended it before its run was admitted · nothing ran";

/// Closes the log when the worker ends, however it ends.
struct Finish<'a>(&'a Shared);

impl Drop for Finish<'_> {
    fn drop(&mut self) {
        self.0.finish();
    }
}

/// Serve turns until the Session closes or quits; then release the runtime (and its history)
/// and the run door (a held review's child ends, nothing is sent) before the log closes.
pub(super) fn serve(
    mut runtime: SessionRuntime,
    mut door: Box<dyn RunDoor>,
    shared: &Shared,
    jobs: &mpsc::Receiver<Job>,
) {
    let finish = Finish(shared);
    let mut held: Option<ReviewId> = None;
    while let Ok(Job::Submit {
        command,
        line,
        shown,
    }) = jobs.recv()
    {
        let turn = Turn {
            shared,
            command: &command,
        };
        if turn.run(&mut runtime, door.as_mut(), &mut held, &line, &shown) {
            break;
        }
    }
    drop(door);
    drop(runtime);
    drop(finish);
}

/// One turn of one command.
struct Turn<'a> {
    shared: &'a Shared,
    command: &'a str,
}

impl RunSink for Turn<'_> {
    fn said(&self, line: String) {
        self.shared.activity(ActivityWire::run(line));
    }
}

impl Turn<'_> {
    /// The turn, settled and published; `true` when the Session quit.
    fn run(
        &self,
        runtime: &mut SessionRuntime,
        door: &mut dyn RunDoor,
        held: &mut Option<ReviewId>,
        line: &str,
        shown: &Waiting,
    ) -> bool {
        // A review answer continues a held run and arms no preparation, as the terminal doors.
        if !matches!(shown, Waiting::RunReview { .. }) {
            let token = runtime.begin_preparation_turn();
            self.shared.arm(self.command, token);
        }
        let outcome = runtime.submit(line, shown);
        self.shared.checkpoint("returned");
        let stopped = self.shared.phase(self.command, TurnPhase::Settling);
        self.shared.checkpoint("settling");
        let mut wire = Vec::new();
        let mut effects = Vec::new();
        project(outcome, &mut wire, &mut effects);
        if stopped && let Some(note) = runtime.withdraw_cancelled_preparation() {
            // The Stop won: what the stopped preparation announced is withdrawn history, never a
            // current proposal or question.
            let (withdrawn, kept): (Vec<_>, Vec<_>) =
                wire.into_iter().partition(Outcome::withdrawable);
            wire = kept;
            wire.push(Outcome::Cancelled {
                text: note,
                withdrawn,
            });
        }
        for effect in effects {
            // A Stop accepted while the turn prepared ends it before any run door is asked: the
            // run or resume it requested starts nothing (a review answer arms no preparation).
            if stopped && !matches!(effect, Effect::Reviewed { .. }) {
                wire.push(Outcome::RunNotStarted {
                    text: STOPPED.to_owned(),
                });
                continue;
            }
            self.perform(runtime, door, held, effect, &mut wire);
        }
        let quit = wire.iter().any(|outcome| matches!(outcome, Outcome::Quit));
        let published = publish(runtime, &self.shared.session);
        self.shared.settle(self.command, wire, published, quit);
        quit
    }

    /// What one outcome asked of the run door, observed through the Session's own door.
    fn perform(
        &self,
        runtime: &mut SessionRuntime,
        door: &mut dyn RunDoor,
        held: &mut Option<ReviewId>,
        effect: Effect,
        wire: &mut Vec<Outcome>,
    ) {
        let root = runtime.snapshot.root.clone();
        let step = match effect {
            Effect::Run(run) => {
                *held = None;
                self.shared.phase(self.command, TurnPhase::Running);
                door.run(&root, &run, self)
            }
            Effect::Resume {
                workflow,
                trace,
                answer,
            } => {
                self.shared.phase(self.command, TurnPhase::Running);
                door.resume(&root, &workflow, &trace, &answer, self)
            }
            Effect::Reviewed { review, approve } => {
                // Only the review this door holds is answered, once.
                if held.as_ref() != Some(&review) {
                    return;
                }
                *held = None;
                door.answer_review(approve, self)
            }
        };
        self.shared.phase(self.command, TurnPhase::Settling);
        match step {
            RunStep::Observed { exit, trace } => {
                let observed = runtime.observe_run(exit, trace.as_deref());
                let mut again = Vec::new();
                project(observed, wire, &mut again);
                if !again.is_empty() {
                    wire.push(Outcome::RunNotStarted {
                        text: "the observation asked for another run; it was not started · request it again".to_owned(),
                    });
                }
            }
            RunStep::Review { question, details } => {
                let review = runtime.run_review_asked(&question, &details);
                wire.push(Outcome::RunReview {
                    review: review.as_str().to_owned(),
                    text: question,
                });
                *held = Some(review);
            }
            RunStep::NotStarted { why } => wire.push(Outcome::RunNotStarted { text: why }),
            RunStep::Unobserved { why } => wire.push(Outcome::RunUnobserved { text: why }),
            _ => {}
        }
    }
}
