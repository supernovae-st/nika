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
//! back through `observe_run`. The optional reviewed runner retains the live
//! child and its frames across a distinct fresh Run cost question. That
//! question and the Session's one-time unknown-cost choice are both fresh
//! spending questions: typeahead from before they were painted is discarded,
//! an interruption cancels them without sending anything, and `details` reads
//! the same review's evidence without answering it.
//!
//! Without a kept intelligence choice the runtime opens all the same
//! (`open_unchosen`): the first screen is asked through the composer under
//! the `›` prompt the first time a turn needs an intelligence, by the
//! runtime's own `choose` law, and the line that waited resumes after it.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use nika_cli_host::lane::{PendingRun, RunProgress};
use nika_session::RunRequest;

/// A fresh Run may suspend at a child-owned cost question.
pub type RunReviewed = Box<dyn Fn(&Path, &RunRequest, &Sender<String>) -> RunProgress + Send>;
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

/// A run driven INSIDE the turn, the terminal never handed back: the
/// door executes through its own machine lane and hands each line of
/// the run's story to the busy sink as it happens; the exit code, the
/// trace the run left and the whole story come back for the observation
/// and the transcript. (root, work, busy sink).
pub type RunTapped =
    Box<dyn Fn(&Path, &Work, &Sender<String>) -> (u8, Option<PathBuf>, Vec<String>) + Send>;

/// The runners the CLI lends to the session. The two plain-path runners
/// print through the terminal the shell has handed back; the tapped
/// runner, when lent, keeps the terminal and the viewport shows the run.
pub struct Runners {
    /// `nika run <workflow>` once, under the request's ceiling.
    pub run_once: RunOnce,
    /// `nika run --resume <trace> --answer <answer>` on the same workflow.
    pub run_resume: RunResume,
    /// The same two runs inside the turn (the renderer's way when lent).
    pub run_tapped: Option<RunTapped>,
}

impl std::fmt::Debug for Runners {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.run_tapped.is_some() {
            "Runners { run_once, run_resume, run_tapped }"
        } else {
            "Runners { run_once, run_resume }"
        })
    }
}

/// The work a run asks of the door: a run once, or a resume with the
/// human's answer to a gate.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Work {
    /// `nika run <workflow>` once, under the request's ceiling.
    Run(RunRequest),
    /// `nika run --resume <trace> --answer <answer>` on the same workflow.
    Resume {
        /// The workflow, relative to the root.
        workflow: PathBuf,
        /// The paused trace.
        trace: PathBuf,
        /// `task=value`, the human's own.
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
    run_review: Option<RunReviewed>,
    pending_run: Option<Box<PendingRun>>,
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
            run_review: None,
            pending_run: None,
            pending: None,
            next_id: 1,
            busy: Arc::new(Mutex::new(None)),
        };
        live.open_runtime(kept);
        live
    }

    /// Supply actual monetary scope evidence for this host, before opening beats.
    #[must_use]
    pub fn with_cost_host_evidence(mut self, evidence: nika_session::CostHostEvidence) -> Self {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.set_cost_host_evidence(evidence);
        }
        self
    }

    /// Keep a real child alive across a distinct fresh Run cost decision.
    #[must_use]
    pub fn with_run_review(mut self, runner: RunReviewed) -> Self {
        self.run_review = Some(runner);
        self
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
        beats.push(Beat::Rail(runtime.lifecycle().rail()));
        beats.push(Beat::Status(runtime.status_line()));
        beats.push(Beat::Wait(self.waiting()));
        beats
    }

    /// What the runtime waits for, by the same reading as the plain loop.
    fn waiting(&self) -> Waiting {
        if self.pending_run.is_some() {
            return Waiting::Question {
                key: "run_cost".into(),
            };
        }
        let Some(runtime) = self.runtime.as_ref() else {
            return Waiting::Free;
        };
        if runtime.waiting_cost_choice() {
            Waiting::Question {
                key: "unknown_cost".into(),
            }
        } else if runtime.pending_choice() {
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
        } else if let Some(key) = runtime.pending_activation() {
            Waiting::Question {
                key: key.to_owned(),
            }
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
                if self.runners.run_tapped.is_some() || self.run_review.is_some() {
                    beats.extend(self.run_inline(&Work::Run(run)));
                    return (beats, None);
                }
                handoff = Some(self.keep(Work::Run(run), label));
            }
            TurnOutcome::Question { key, question, .. } if key == "unknown_cost" => {
                beats.push(Beat::Say(Committed::new(
                    Kind::Question,
                    authoring_cost_question(&question),
                )));
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
                let work = Work::Resume {
                    workflow,
                    trace,
                    answer,
                };
                if self.runners.run_tapped.is_some() || self.run_review.is_some() {
                    beats.extend(self.run_inline(&work));
                    return (beats, None);
                }
                handoff = Some(self.keep(work, label));
            }
            TurnOutcome::Refusal(why) => {
                beats.push(Beat::Say(Committed::new(Kind::Refusal, why.to_string())));
            }
            _ => {}
        }
        if handoff.is_none() {
            if let Some(runtime) = self.runtime.as_ref() {
                beats.push(Beat::Rail(runtime.lifecycle().rail()));
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

    /// The run inside the turn: the tapped runner executes while the busy
    /// row shows each line of the run's story; the story is then
    /// committed as one block, the observation follows (a result, a
    /// failure, or a gate that waits for the human), the prompt returns.
    fn run_inline(&mut self, work: &Work) -> Vec<Beat> {
        let root = self
            .runtime
            .as_ref()
            .map_or_else(|| self.cwd.clone(), |r| r.snapshot.root.clone());
        let busy = self
            .busy
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .unwrap_or_else(|| std::sync::mpsc::channel().0);
        if let Work::Run(run) = work
            && let Some(review) = self.run_review.as_ref()
        {
            return match review(&root, run, &busy) {
                RunProgress::Complete(result) => self.run_result(result),
                RunProgress::Review(pending) => {
                    let question = pending.question();
                    self.pending_run = Some(pending);
                    vec![
                        Beat::Say(Committed::new(Kind::Question, question)),
                        Beat::Wait(self.waiting()),
                    ]
                }
                _ => vec![Beat::Say(Committed::new(
                    Kind::Refusal,
                    "unsupported Run review response",
                ))],
            };
        }
        let Some(tap) = self.runners.run_tapped.as_ref() else {
            return Vec::new();
        };
        let result = tap(&root, work, &busy);
        self.run_result(result)
    }

    fn run_result(
        &mut self,
        (code, trace, story): (u8, Option<PathBuf>, Vec<String>),
    ) -> Vec<Beat> {
        let mut beats = Vec::new();
        if !story.is_empty() {
            beats.push(Beat::Say(Committed::new(Kind::Run, story.join("\n"))));
        }
        let Some(runtime) = self.runtime.as_mut() else {
            beats.push(Beat::Quit);
            return beats;
        };
        let outcome = runtime.observe_run(code, trace.as_deref());
        let (more, again) = self.map(outcome);
        beats.extend(more);
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

    /// `details` under the Session's cost question: the same review's
    /// evidence, read without a turn (nothing recorded, answered or reviewed
    /// again); `None` when no such question waits.
    fn cost_details(&self, line: &str) -> Option<Vec<Beat>> {
        if !line.trim().eq_ignore_ascii_case("details") {
            return None;
        }
        let details = self.runtime.as_ref()?.cost_choice_details()?;
        Some(vec![
            Beat::Say(Committed::new(
                Kind::Question,
                format!(
                    "Authoring cost decision details · the same review; reading them approves nothing\n{details}\n{REVIEW_CHOICE}"
                ),
            )),
            Beat::Wait(self.waiting()),
        ])
    }
}

/// The review's own closing line, and the line that also names `details`:
/// the same words as the first screen of a fresh Run cost question.
const REVIEW_CHOICE: &str = "Continue once? yes / no";
const CHOICES: &str = "Continue once? yes / no / details";

/// The Session's one-time unknown-cost question, told apart from Save and
/// Run and closing on the three choices; the Session's sentences stay whole.
fn authoring_cost_question(question: &str) -> String {
    let body = question
        .strip_suffix(REVIEW_CHOICE)
        .map_or(question, str::trim_end);
    format!(
        "Fresh authoring cost decision · this request only; approving it never saves or runs anything\n{body}\n{CHOICES}"
    )
}

impl Conversation for Live {
    fn commands(&self) -> Vec<String> {
        nika_session::runtime::SLASH_COMMANDS
            .iter()
            .map(|c| (*c).to_owned())
            .collect()
    }

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
        if let Some(pending) = self.pending_run.take() {
            let beats = if line.trim().eq_ignore_ascii_case("details") {
                let details = pending.details();
                self.pending_run = Some(pending);
                vec![
                    Beat::Say(Committed::new(Kind::Question, details)),
                    Beat::Wait(self.waiting()),
                ]
            } else if line.trim().eq_ignore_ascii_case("yes") {
                let busy = self
                    .busy
                    .lock()
                    .ok()
                    .and_then(|g| g.clone())
                    .unwrap_or_else(|| std::sync::mpsc::channel().0);
                self.run_result((*pending).answer(true, &busy))
            } else {
                drop(pending);
                let mut beats = self.run_result((130, None, vec![
                    "Run cost decision cancelled; nothing sent. Request Run again for a fresh review.".into(),
                ]));
                if line.trim() == "/quit" {
                    beats.push(Beat::Quit);
                }
                beats
            };
            return Turn {
                beats,
                handoff: None,
            };
        }
        if let Some(beats) = self.cost_details(line) {
            return Turn {
                beats,
                handoff: None,
            };
        }
        let outcome = {
            let Some(runtime) = self.runtime.as_mut() else {
                return Turn {
                    beats: vec![Beat::Quit],
                    handoff: None,
                };
            };
            if runtime.waiting_cost_choice() {
                runtime.turn(line)
            } else if runtime.pending_choice() {
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

    /// Both fresh spending questions: the retained Run review and the
    /// Session's one-time unknown-cost choice. Any other waiting state keeps
    /// its typeahead and grants nothing here.
    fn fresh_input_required(&self) -> bool {
        self.pending_run.is_some()
            || self
                .runtime
                .as_ref()
                .is_some_and(SessionRuntime::waiting_cost_choice)
    }

    fn cancel_pending(&mut self) -> Vec<Beat> {
        if self.pending_run.take().is_some() {
            return self.run_result((
                130,
                None,
                vec!["Run cost decision cancelled; nothing sent".into()],
            ));
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return Vec::new();
        };
        if !runtime.waiting_cost_choice() {
            return Vec::new();
        }
        // The Session's own answer path: every answer but yes cancels the
        // review and sends nothing; the interruption never becomes a yes.
        let outcome = runtime.turn("cancel");
        self.map(outcome).0
    }

    fn busy_label(&self, line: &str) -> Option<String> {
        if self.pending_run.is_some() {
            return Some("answering the fresh Run cost question".into());
        }
        let runtime = self.runtime.as_ref()?;
        if runtime.waiting_cost_choice() {
            return Some("answering the fresh authoring cost question".into());
        }
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

#[cfg(all(test, unix))]
mod tests;
