// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The live conversation: the real session runtime behind the renderer's
//! beats (UX-2).
//!
//! Every `TurnOutcome` of [`nika_session::runtime::SessionRuntime`] maps to
//! the beats of [`crate::model`] one to one, and the composer's line goes to
//! `choose` · `consent` · `answer_gate` · `turn` by the same state the plain
//! loop of the CLI reads (a `yes` under `reply ›` is an answer, never a
//! consent). A run keeps the plain path: the session asks for a
//! [`Handoff`], the shell hands the terminal back, the caller's runners do
//! the run through the very path `nika run` owns, and the observation comes
//! back through `observe_run`. Capturing a run's frames inside the renderer
//! is a later wave.
//!
//! Without a kept intelligence choice the runtime opens all the same
//! (`open_unchosen`): the first screen is asked through the composer under
//! the `›` prompt the first time a turn needs an intelligence, by the
//! runtime's own `choose` law, and the line that waited resumes after it.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use nika_session::RunRequest;
use nika_session::intelligence::{IntelligenceCensus, UserIntelligencePreference};
use nika_session::runtime::{ReasonerFactory, SessionRuntime, TurnOutcome};

use crate::model::{Beat, Committed, Conversation, Handoff, Kind, Turn, Waiting};

/// The exit code and the trace a run left.
pub type RunOutcome = (u8, Option<PathBuf>);
/// `nika run <workflow>` once, under the request's ceiling: (root, request).
pub type RunOnce = Box<dyn Fn(&Path, &RunRequest) -> RunOutcome + Send>;
/// `nika run --resume <trace> --answer <answer>`: (root, workflow, trace, answer).
pub type RunResume = Box<dyn Fn(&Path, &Path, &Path, &str) -> RunOutcome + Send>;

/// Where the runtime's progress lines go while a turn runs: the shell's
/// sink for the duration of one `submit_with`, nothing between turns.
type BusySlot = Arc<Mutex<Option<Sender<String>>>>;

/// The two plain-path runners the CLI lends to the session: a run once, a
/// resume with the human's answer. Both print through the terminal the
/// shell has handed back.
pub struct Runners {
    /// `nika run <workflow>` once, under the request's ceiling.
    pub run_once: RunOnce,
    /// `nika run --resume <trace> --answer <answer>` on the same workflow.
    pub run_resume: RunResume,
}

impl std::fmt::Debug for Runners {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Runners { run_once, run_resume }")
    }
}

/// What a handoff must do, kept by the conversation until the shell asks.
#[derive(Debug, Clone)]
enum Work {
    Run(RunRequest),
    Resume {
        workflow: PathBuf,
        trace: PathBuf,
        answer: String,
    },
}

/// The live conversation.
pub struct Live {
    cwd: PathBuf,
    census: IntelligenceCensus,
    home: Option<PathBuf>,
    factory: Option<ReasonerFactory>,
    runtime: Option<SessionRuntime>,
    runners: Runners,
    pending: Option<(u64, Work)>,
    next_id: u64,
    busy: BusySlot,
}

impl std::fmt::Debug for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Live")
            .field("cwd", &self.cwd)
            .field("open", &self.runtime.is_some())
            .finish_non_exhaustive()
    }
}

impl Live {
    /// A conversation over `cwd`. `kept` is the intelligence choice found
    /// under the home, when one exists; without it the runtime opens
    /// unchosen and asks the first screen when a turn needs one.
    #[must_use]
    pub fn new(
        cwd: PathBuf,
        census: IntelligenceCensus,
        kept: Option<UserIntelligencePreference>,
        home: Option<PathBuf>,
        factory: ReasonerFactory,
        runners: Runners,
    ) -> Self {
        let mut live = Self {
            cwd,
            census,
            home,
            factory: Some(factory),
            runtime: None,
            runners,
            pending: None,
            next_id: 1,
            busy: Arc::new(Mutex::new(None)),
        };
        live.open_runtime(kept);
        live
    }

    fn open_runtime(&mut self, pref: Option<UserIntelligencePreference>) {
        let Some(factory) = self.factory.take() else {
            return;
        };
        let mut runtime = match pref {
            Some(pref) => SessionRuntime::open_with(
                &self.cwd,
                self.census.clone(),
                &pref,
                self.home.as_deref(),
                factory,
            ),
            None => SessionRuntime::open_unchosen(
                &self.cwd,
                self.census.clone(),
                self.home.as_deref(),
                factory,
            ),
        };
        // The plain loop prints progress lines to stdout; here the viewport
        // owns stdout: a progress line becomes the busy label the shell
        // draws while the turn runs (`submit_with` arms the sink), and is
        // dropped between turns.
        let slot = Arc::clone(&self.busy);
        runtime.on_progress(Box::new(move |line| {
            if let Ok(guard) = slot.lock()
                && let Some(tx) = guard.as_ref()
            {
                let _ = tx.send(line.to_owned());
            }
        }));
        self.runtime = Some(runtime);
    }

    /// The beats that open a runtime: banner, history, restored state.
    fn opening_beats(&mut self) -> Vec<Beat> {
        let Some(runtime) = self.runtime.as_mut() else {
            return Vec::new();
        };
        let mut beats = vec![Beat::Say(Committed::new(Kind::Banner, runtime.banner()))];
        match self.home.as_deref() {
            Some(home) => match runtime.enable_history(home) {
                Ok(Some(notice)) => beats.push(Beat::Say(Committed::new(Kind::Notice, notice))),
                Ok(None) => {}
                Err(why) => {
                    beats.push(Beat::Say(Committed::new(Kind::Refusal, why.to_string())));
                    beats.push(Beat::Quit);
                    return beats;
                }
            },
            None => beats.push(Beat::Say(Committed::new(
                Kind::Notice,
                "conversation is temporary: no home directory is available",
            ))),
        }
        if let Some(notice) = runtime.restore_state() {
            beats.push(Beat::Say(Committed::new(Kind::Notice, notice)));
        }
        beats.push(Beat::Status(runtime.status_line()));
        beats.push(Beat::Wait(self.waiting()));
        beats
    }

    /// What the runtime waits for, by the same reading as the plain loop.
    fn waiting(&self) -> Waiting {
        let Some(runtime) = self.runtime.as_ref() else {
            return Waiting::Free;
        };
        if runtime.pending_choice() {
            Waiting::Choosing
        } else if runtime.pending_proposal().is_some() {
            Waiting::Proposal
        } else if runtime.waiting_gate().is_some() {
            Waiting::Gate
        } else if let Some(question) = runtime.pending_question() {
            Waiting::Question {
                key: question.key.clone(),
            }
        } else if runtime.pending_input().is_some() {
            Waiting::Question { key: String::new() }
        } else {
            Waiting::Free
        }
    }

    /// One outcome to beats, and the handoff it asks for.
    fn map(&mut self, outcome: TurnOutcome) -> (Vec<Beat>, Option<Handoff>) {
        let mut beats = Vec::new();
        let mut handoff = None;
        match outcome {
            TurnOutcome::Quit => return (vec![Beat::Quit], None),
            TurnOutcome::Reply(text)
            | TurnOutcome::Facts(text)
            | TurnOutcome::Help(text)
            | TurnOutcome::Aside(text) => {
                if !text.is_empty() {
                    beats.push(Beat::Say(Committed::new(Kind::Reply, text)));
                }
            }
            // The first screen, in context or on `/intelligence`: the runtime
            // now waits for the choice (`waiting()` reads it).
            TurnOutcome::Ask(screen) => {
                beats.push(Beat::Say(Committed::new(Kind::Reply, screen)));
            }
            // The choice landed and the waiting line resumed under it: the
            // choice's fact, then whatever that line became.
            TurnOutcome::Resumed { notice, outcome } => {
                beats.push(Beat::Say(Committed::new(Kind::Notice, notice)));
                let (rest, again) = self.map(*outcome);
                beats.extend(rest);
                return (beats, again);
            }
            TurnOutcome::Proposal { preview, .. } | TurnOutcome::Held { preview, .. } => {
                beats.push(Beat::Say(Committed::new(Kind::Proposal, preview)));
            }
            TurnOutcome::RunRequested { report, run } => {
                beats.push(Beat::Say(Committed::new(Kind::Report, report)));
                let label = format!(
                    "running `{}` once · ceiling ${:.2}",
                    run.workflow.display(),
                    run.max_cost_usd
                );
                beats.push(Beat::Say(Committed::new(Kind::Report, label.clone())));
                handoff = Some(self.keep(Work::Run(run), label));
            }
            TurnOutcome::Question { question, .. } => {
                beats.push(Beat::Say(Committed::new(Kind::Question, question)));
            }
            TurnOutcome::GateAsk { question, .. } => {
                beats.push(Beat::Say(Committed::new(Kind::Gate, question)));
            }
            TurnOutcome::ResumeRequested {
                workflow,
                trace,
                answer,
            } => {
                let label = format!("resuming `{}` with your answer", workflow.display());
                beats.push(Beat::Say(Committed::new(Kind::Report, label.clone())));
                handoff = Some(self.keep(
                    Work::Resume {
                        workflow,
                        trace,
                        answer,
                    },
                    label,
                ));
            }
            TurnOutcome::Refusal(why) => {
                beats.push(Beat::Say(Committed::new(Kind::Refusal, why.to_string())));
            }
            _ => {}
        }
        if handoff.is_none() {
            if let Some(runtime) = self.runtime.as_ref() {
                beats.push(Beat::Status(runtime.status_line()));
            }
            beats.push(Beat::Wait(self.waiting()));
        }
        (beats, handoff)
    }

    fn keep(&mut self, work: Work, label: String) -> Handoff {
        let id = self.next_id;
        self.next_id += 1;
        self.pending = Some((id, work));
        Handoff { id, label }
    }
}

impl Conversation for Live {
    fn open(&mut self) -> Vec<Beat> {
        self.opening_beats()
    }

    fn submit_with(&mut self, line: &str, busy: &Sender<String>) -> Turn {
        if let Ok(mut guard) = self.busy.lock() {
            *guard = Some(busy.clone());
        }
        let turn = self.submit(line);
        if let Ok(mut guard) = self.busy.lock() {
            *guard = None;
        }
        turn
    }

    fn submit(&mut self, line: &str) -> Turn {
        let outcome = {
            let Some(runtime) = self.runtime.as_mut() else {
                return Turn {
                    beats: vec![Beat::Quit],
                    handoff: None,
                };
            };
            if runtime.pending_choice() {
                runtime.choose(line.trim())
            } else if runtime.pending_proposal().is_some() {
                runtime.consent(line.trim())
            } else if runtime.waiting_gate().is_some() {
                runtime.answer_gate(line.trim())
            } else {
                runtime.turn(line)
            }
        };
        let (beats, handoff) = self.map(outcome);
        Turn { beats, handoff }
    }

    fn busy_label(&self, line: &str) -> Option<String> {
        let runtime = self.runtime.as_ref()?;
        let label = if runtime.pending_choice() {
            "seating the intelligence you chose"
        } else if runtime.pending_proposal().is_some() {
            if line.trim().is_empty() {
                return None;
            }
            "landing the exact bytes and checking them"
        } else if runtime.waiting_gate().is_some() {
            "answering the gate"
        } else if line.trim_start().starts_with('/') {
            return None;
        } else {
            "working through your words"
        };
        Some(label.to_owned())
    }

    fn perform(&mut self, handoff: &Handoff) -> Vec<Beat> {
        let Some((id, work)) = self.pending.take() else {
            return vec![Beat::Wait(self.waiting())];
        };
        if id != handoff.id {
            return vec![
                Beat::Say(Committed::new(
                    Kind::Refusal,
                    "the terminal was handed back for work the session no longer holds",
                )),
                Beat::Wait(self.waiting()),
            ];
        }
        let root = self
            .runtime
            .as_ref()
            .map_or_else(|| self.cwd.clone(), |r| r.snapshot.root.clone());
        let (code, trace) = match &work {
            Work::Run(run) => (self.runners.run_once)(&root, run),
            Work::Resume {
                workflow,
                trace,
                answer,
            } => (self.runners.run_resume)(&root, workflow, trace, answer),
        };
        let Some(runtime) = self.runtime.as_mut() else {
            return vec![Beat::Quit];
        };
        let outcome = runtime.observe_run(code, trace.as_deref());
        let (mut beats, again) = self.map(outcome);
        if again.is_some() {
            self.pending = None;
            beats.push(Beat::Say(Committed::new(
                Kind::Notice,
                "the observation asked for another run; say « run it » again when you want it",
            )));
            beats.push(Beat::Wait(self.waiting()));
        }
        beats
    }
}
