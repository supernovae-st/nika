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
use nika_display::run_story::RunSink;
use nika_session::RunRequest;

use acquire::{Fetched, Proven};
use feed::{Feed, Seen};
use legs::{Leg, Legs};
use nika_display::run_story::ExecutionId;
use nika_session::KeptRun;

/// A fresh Run may suspend at a child-owned cost question. Only the run's
/// story reaches the sender: no frame is typed, so the workspace cannot follow
/// the run, and its leg says so ([`RunReviewedObserved`] tells the frames).
pub type RunReviewed = Box<dyn Fn(&Path, &RunRequest, &Sender<String>) -> RunProgress + Send>;
/// [`RunReviewed`], the run told to the sink: its story, its frames typed
/// (`run_story::RunSink`).
pub type RunReviewedObserved = Box<dyn Fn(&Path, &RunRequest, &dyn RunSink) -> RunProgress + Send>;
use nika_session::intelligence::{IntelligenceCensus, UserIntelligencePreference};
use nika_session::runtime::{ReasonerFactory, SessionRuntime, TurnOutcome};

use crate::model::{Beat, Committed, Conversation, Handoff, Kind, Turn, Waiting};
use crate::workspace::candidate::Proposed;
use crate::workspace::header::Manifest;
use crate::workspace::inspect::Inspected;
use crate::workspace::project::{ProjectView, WorkflowView};
use nika_session::ProjectSnapshot;

/// The exit code and the trace a run left.
pub type RunOutcome = (u8, Option<PathBuf>);
/// `nika run <workflow>` once, under the request's ceiling: (root, request).
pub type RunOnce = Box<dyn Fn(&Path, &RunRequest) -> RunOutcome + Send>;
/// `nika run --resume <trace> --answer <answer>`: (root, workflow, trace, answer).
pub type RunResume = Box<dyn Fn(&Path, &Path, &Path, &str) -> RunOutcome + Send>;

/// Where the runtime's progress lines go while a turn runs: the shell's
/// sink for the duration of one `submit_with`, nothing between turns.
type BusySlot = Arc<Mutex<Option<Feed>>>;

/// A run driven INSIDE the turn, the terminal never handed back: the
/// door executes through its own machine lane and hands each line of
/// the run's story to the busy sink as it happens; the exit code, the
/// trace the run left and the whole story come back for the observation
/// and the transcript. (root, work, sink). Only the story reaches the
/// sender: the workspace cannot follow the run, and its leg says so.
pub type RunTapped =
    Box<dyn Fn(&Path, &Work, &Sender<String>) -> (u8, Option<PathBuf>, Vec<String>) + Send>;
/// [`RunTapped`], the run told to the sink: its story, its frames typed.
pub type RunTappedObserved =
    Box<dyn Fn(&Path, &Work, &dyn RunSink) -> (u8, Option<PathBuf>, Vec<String>) + Send>;

/// The runner a run inside the turn goes to, in precedence order: a typed
/// review, a story-only review (fresh runs only), a typed tap, a story tap.
enum Runner<'a> {
    ReviewTyped(&'a RunReviewedObserved),
    Review(&'a RunReviewed),
    TapTyped(&'a RunTappedObserved),
    Tap(&'a RunTapped),
}

impl Runner<'_> {
    /// Whether this runner tells the run's frames, typed.
    fn typed(&self) -> bool {
        matches!(self, Self::ReviewTyped(_) | Self::TapTyped(_))
    }
}

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
    run_review_observed: Option<RunReviewedObserved>,
    run_tapped_observed: Option<RunTappedObserved>,
    /// The child waiting at its cost question, and whether its runner tells
    /// the frames (its answer is told the same way).
    pending_run: Option<(Box<PendingRun>, bool)>,
    pending: Option<(u64, Work)>,
    next_id: u64,
    busy: BusySlot,
    /// The candidate under review, folded when the last turn ended: what the
    /// workspace shows, and the identity a consent typed here answers.
    candidate: Option<Proposed>,
    /// What this host relayed of each run leg: the only identities, paths
    /// and journals its fetch and proof may reach.
    legs: Arc<Mutex<Legs>>,
    /// The last run an earlier session kept (HOME history), as read at open.
    kept: Option<Result<KeptRun, String>>,
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
            run_review_observed: None,
            run_tapped_observed: None,
            pending_run: None,
            pending: None,
            next_id: 1,
            busy: Arc::new(Mutex::new(None)),
            candidate: None,
            legs: Arc::default(),
            kept: None,
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

    /// [`Self::with_run_review`], the run's frames told typed: it takes
    /// precedence over a story-only review.
    #[must_use]
    pub fn with_run_review_observed(mut self, runner: RunReviewedObserved) -> Self {
        self.run_review_observed = Some(runner);
        self
    }

    /// The run inside the turn, its frames told typed: it takes precedence
    /// over `Runners::run_tapped` (a review still goes first for a fresh run).
    #[must_use]
    pub fn with_run_tapped_observed(mut self, runner: RunTappedObserved) -> Self {
        self.run_tapped_observed = Some(runner);
        self
    }

    /// The runner `work` goes to inside the turn, when one is lent.
    fn runner(&self, work: &Work) -> Option<Runner<'_>> {
        let fresh = matches!(work, Work::Run(_));
        let review = (self.run_review_observed.as_ref().filter(|_| fresh))
            .map(Runner::ReviewTyped)
            .or_else(|| (self.run_review.as_ref().filter(|_| fresh)).map(Runner::Review));
        review
            .or_else(|| self.run_tapped_observed.as_ref().map(Runner::TapTyped))
            .or_else(|| self.runners.run_tapped.as_ref().map(Runner::Tap))
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
                && let Some(feed) = guard.as_ref()
            {
                feed.said(line.to_owned());
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
        beats.extend(earlier(runtime.kept_turns()));
        let kept = runtime.kept_run();
        if let Some(line) = kept_line(kept.as_ref()) {
            beats.push(Beat::Say(Committed::new(Kind::Notice, line)));
        }
        if let (Some(Ok(run)), Ok(mut legs)) = (&kept, self.legs.lock()) {
            legs.kept(run);
        }
        self.kept = kept;
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
                let work = Work::Run(run);
                if self.runner(&work).is_some() {
                    beats.extend(self.run_inline(&work));
                    return (beats, None);
                }
                handoff = Some(self.keep(work, label));
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
                if self.runner(&work).is_some() {
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
        let feed = self.feed();
        // The request, before its child exists: the shell learns the workflow
        // and the exact bytes at that path now, never a run identity.
        let (workflow, resume) = match work {
            Work::Run(run) => (run.workflow.display().to_string(), false),
            Work::Resume { workflow, .. } => (workflow.display().to_string(), true),
        };
        let look = (self.runtime.as_ref()).and_then(|r| look::take(&r.snapshot, &workflow));
        let Some(runner) = self.runner(work) else {
            return Vec::new();
        };
        let typed = runner.typed();
        feed.asked(workflow, resume, typed, look);
        let progress = match (runner, work) {
            (Runner::ReviewTyped(review), Work::Run(run)) => review(&root, run, &feed),
            (Runner::Review(review), Work::Run(run)) => review(&root, run, feed.busy()),
            (Runner::TapTyped(tap), _) => RunProgress::Complete(tap(&root, work, &feed)),
            (Runner::Tap(tap), _) => RunProgress::Complete(tap(&root, work, feed.busy())),
            _ => return Vec::new(),
        };
        match progress {
            RunProgress::Complete(result) => self.run_result(result),
            RunProgress::Review(pending) => {
                let question = pending.question();
                self.pending_run = Some((pending, typed));
                vec![
                    Beat::Say(Committed::new(Kind::Question, question)),
                    Beat::Wait(self.waiting()),
                ]
            }
            _ => vec![Beat::Say(Committed::new(
                Kind::Refusal,
                "unsupported Run review response",
            ))],
        }
    }

    /// The sink of the turn under way: the shell's busy row and queue, or a
    /// row nobody reads between turns.
    fn feed(&self) -> Feed {
        (self.busy.lock().ok())
            .and_then(|guard| guard.clone())
            .unwrap_or_else(|| Feed::new(std::sync::mpsc::channel().0, None))
    }

    /// A declined (or interrupted) Run review: the child got no answer,
    /// nothing was sent, nothing ran — a typed « not run », never an
    /// observed exit code, so the status keeps the last real run.
    fn declined_run(&mut self, story: &str) -> Vec<Beat> {
        let mut beats = vec![Beat::Say(Committed::new(Kind::Run, story))];
        let Some(runtime) = self.runtime.as_mut() else {
            beats.push(Beat::Quit);
            return beats;
        };
        let outcome = runtime.observe_declined_run();
        let (more, _) = self.map(outcome);
        beats.extend(more);
        beats
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
        // The leg this host relayed for this request, when frames came: its
        // identity rides the observation into HOME history.
        let leg = (self.legs.lock().ok()).and_then(|legs| legs.newest().map(Leg::kept_run));
        let outcome = match leg {
            Some(leg) => runtime.observe_run_leg(code, trace.as_deref(), leg),
            None => runtime.observe_run(code, trace.as_deref()),
        };
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
/// The recent turns HOME history kept, repainted as history: what was said
/// in an earlier session, never replayed.
fn earlier(turns: &[(String, String)]) -> Vec<Beat> {
    if turns.is_empty() {
        return Vec::new();
    }
    let mut beats = vec![Beat::Say(Committed::new(
        Kind::Notice,
        "earlier in this conversation · kept in your history · nothing is replayed",
    ))];
    for (said, reply) in turns {
        beats.push(Beat::Say(Committed::new(Kind::Human, said.clone())));
        beats.push(Beat::Say(Committed::new(Kind::Reply, reply.clone())));
    }
    beats
}

/// The last run an earlier session kept, in one line: evidence of what was
/// observed then; its journal is verified again only when its proof opens.
fn kept_line(kept: Option<&Result<KeptRun, String>>) -> Option<String> {
    Some(match kept? {
        Ok(run) => {
            let id: String = run
                .execution
                .as_deref()
                .unwrap_or("unrecorded")
                .chars()
                .take(13)
                .collect();
            let workflow = run.workflow.as_deref().unwrap_or("(not recorded)");
            let exit = run
                .exit
                .map_or_else(|| "not recorded".to_owned(), |e| e.to_string());
            format!(
                "last run, observed in an earlier session · {id} of `{workflow}` · exit {exit} · its proof is read again from its journal when you open it · nothing replays"
            )
        }
        Err(why) => format!(
            "the last run's record is unreadable ({why}) · kept unchanged · nothing replays"
        ),
    })
}

/// A consent line while no candidate is on screen.
const NOTHING_SHOWN: &str = "no proposal is on screen to answer · nothing was applied · the proposal is shown again after this line";

/// A line that leaves or declines: it applies nothing, shown or not.
fn declines(line: &str) -> bool {
    use nika_session::runtime::{DecisionAnswer, decision_answer};
    matches!(line.trim(), "/quit" | "/exit") || decision_answer(line) == DecisionAnswer::Decline
}

/// The Run review, still waiting after a local command or an unknown line.
const RUN_STILL_WAITS: &str = "the fresh Run cost decision still waits · `yes`/`oui` runs it once · `no`/`non` cancels · `details` shows the evidence";

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
        // `/restore` completes only while a kept draft can be proposed again.
        self.runtime
            .as_ref()
            .map_or_else(
                || nika_session::runtime::SLASH_COMMANDS.to_vec(),
                SessionRuntime::slash_commands,
            )
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn open(&mut self) -> Vec<Beat> {
        let beats = self.opening_beats();
        self.fold_candidate();
        beats
    }

    fn submit_with(&mut self, line: &str, busy: &Sender<String>) -> Turn {
        self.lent(Feed::new(busy.clone(), None), line)
    }

    fn submit_observed(&mut self, line: &str, busy: &Sender<String>, seen: &Seen) -> Turn {
        self.lent(Feed::new(busy.clone(), Some(seen.clone())), line)
    }

    fn submit(&mut self, line: &str) -> Turn {
        let turn = self.answer(line);
        self.fold_candidate();
        turn
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
        let beats = self.cancelled();
        self.fold_candidate();
        beats
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
        let beats = self.performed(handoff);
        self.fold_candidate();
        beats
    }

    fn project(&self) -> Option<ProjectView> {
        let runtime = self.runtime.as_ref()?;
        Some(project_view(runtime, self.home.as_deref()))
    }

    /// The look (the crate-private `look::take`): only a workflow the runtime's snapshot
    /// lists, read once below its root. Nothing here joins the look to the
    /// conversation, its facts or a consent.
    fn inspect(&mut self, path: &str) -> Option<Inspected> {
        look::take(&self.runtime.as_ref()?.snapshot, path)
    }

    /// The candidate folded when the last turn ended (`candidate::take`).
    fn candidate(&self) -> Option<Proposed> {
        self.candidate.clone()
    }

    /// A file the leg `execution` reported writing (as this host relayed
    /// it), read now below the root; any other path is refused unread.
    fn fetch(&mut self, execution: &ExecutionId, path: &str) -> Option<Fetched> {
        let root = &self.runtime.as_ref()?.snapshot.root;
        let legs = self.legs.lock().ok()?;
        let leg = legs.find(execution);
        Some(match leg {
            Some(leg) if leg.written.iter().any(|w| w == path) => acquire::fetch(root, path),
            _ => Fetched::refused(path, "not a file this run reported writing"),
        })
    }

    /// The Proof of the journal the leg `execution` settled with, bound to
    /// what this host relayed of it (never a path the renderer names).
    fn prove(&mut self, execution: &ExecutionId) -> Option<Proven> {
        let root = &self.runtime.as_ref()?.snapshot.root;
        let legs = self.legs.lock().ok()?;
        Some(match legs.find(execution) {
            Some(leg) => match leg.trace.as_deref() {
                Some(trace) => acquire::prove(root, trace, &leg.expect()),
                None => Proven::refused("", "its settlement named no journal"),
            },
            None => Proven::refused("", "this run's settlement was not observed here"),
        })
    }

    /// The last run an earlier session kept, as read at open.
    fn kept_run(&self) -> Option<Result<KeptRun, String>> {
        self.kept.clone()
    }
}

impl Live {
    /// One submitted line with `feed` lent to the runtime for the turn.
    fn lent(&mut self, feed: Feed, line: &str) -> Turn {
        if let Ok(mut guard) = self.busy.lock() {
            *guard = Some(feed.with_legs(Arc::clone(&self.legs)));
        }
        let turn = self.submit(line);
        if let Ok(mut guard) = self.busy.lock() {
            *guard = None;
        }
        turn
    }

    /// Fold the Session's candidate once a turn, a performed work, a
    /// cancellation or the opening ended: on that turn's thread, never while
    /// the shell draws.
    fn fold_candidate(&mut self) {
        self.candidate = (self.runtime.as_ref())
            .and_then(|runtime| candidate::take(runtime, self.candidate.as_ref()));
    }

    /// A line under the retained Run review. The review's own answer grammar
    /// is the Session's (EN/FR); a local command answers from the session's
    /// facts; an unknown line is asked again. Only an approval answers the
    /// child, once.
    fn answer_run_review(
        &mut self,
        (pending, typed): (Box<PendingRun>, bool),
        line: &str,
    ) -> Vec<Beat> {
        use nika_session::runtime::{DecisionAnswer, decision_answer};
        let trimmed = line.trim();
        if matches!(trimmed, "/help" | "/status") {
            let facts = match (trimmed, self.runtime.as_ref()) {
                ("/help", Some(runtime)) => runtime.help_card(),
                ("/help", None) => nika_session::runtime::HELP.to_owned(),
                (_, Some(runtime)) => runtime.status(),
                (_, None) => String::new(),
            };
            self.pending_run = Some((pending, typed));
            return vec![
                Beat::Say(Committed::new(Kind::Notice, facts)),
                Beat::Say(Committed::new(Kind::Question, RUN_STILL_WAITS)),
                Beat::Wait(self.waiting()),
            ];
        }
        if trimmed == "/quit" {
            drop(pending);
            let mut beats = self.declined_run("Run cost decision cancelled; nothing sent.");
            beats.push(Beat::Quit);
            return beats;
        }
        match decision_answer(trimmed) {
            DecisionAnswer::Details => {
                let details = pending.details();
                self.pending_run = Some((pending, typed));
                vec![
                    Beat::Say(Committed::new(Kind::Question, details)),
                    Beat::Wait(self.waiting()),
                ]
            }
            DecisionAnswer::Approve => {
                let feed = self.feed();
                let result = if typed {
                    (*pending).answer_observed(true, &feed)
                } else {
                    (*pending).answer(true, feed.busy())
                };
                self.run_result(result)
            }
            DecisionAnswer::Decline => {
                drop(pending);
                self.declined_run(
                    "Run cost decision cancelled; nothing sent. Request Run again for a fresh review.",
                )
            }
            // Unknown, and any answer this door does not know yet
            // (the grammar is non-exhaustive): asked again, never a yes.
            _ => {
                self.pending_run = Some((pending, typed));
                vec![
                    Beat::Say(Committed::new(
                        Kind::Question,
                        format!(
                            "« {trimmed} » is not a yes or a no · nothing was sent\n{RUN_STILL_WAITS}"
                        ),
                    )),
                    Beat::Wait(self.waiting()),
                ]
            }
        }
    }

    /// One submitted line to the state that waits for it.
    fn answer(&mut self, line: &str) -> Turn {
        if let Some(pending) = self.pending_run.take() {
            return Turn {
                beats: self.answer_run_review(pending, line),
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
                // The line answers the candidate on screen, by its identity: a
                // proposal that is not the one shown is refused as stale, and
                // with none shown nothing is consented (leaving and declining
                // apply nothing, and still go through).
                match self.candidate.as_ref().filter(|shown| !shown.aside()) {
                    Some(shown) => runtime.consent_to(shown.id(), line.trim()),
                    None if declines(line) => runtime.consent(line.trim()),
                    None => TurnOutcome::Refusal(nika_session::Refusal::new(
                        nika_session::RefusalClass::WrongState,
                        NOTHING_SHOWN,
                    )),
                }
            } else if runtime.waiting_gate().is_some() {
                runtime.answer_gate(line.trim())
            } else {
                runtime.turn(line)
            }
        };
        let (beats, handoff) = self.map(outcome);
        Turn { beats, handoff }
    }

    /// An interruption: a fresh spending question it cancels, nothing else.
    fn cancelled(&mut self) -> Vec<Beat> {
        if self.pending_run.take().is_some() {
            return self.declined_run("Run cost decision cancelled; nothing sent");
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

    /// The handed-off work, performed with the terminal handed back.
    fn performed(&mut self, handoff: &Handoff) -> Vec<Beat> {
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

/// The project as this session observed it when it opened, lent read-only to
/// the workspace: the runtime's own snapshot in words, never a new walk of
/// the disk. The session runs in this process, so its host is `local`. No
/// run is pinned: the Session lends no typed identity of a run in flight or
/// paused (the gate's id names its trace and task, not its workflow).
fn project_view(runtime: &SessionRuntime, home: Option<&Path>) -> ProjectView {
    let snapshot = &runtime.snapshot;
    let root = &snapshot.root;
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| root.display().to_string());
    let workflows = snapshot
        .workflows
        .iter()
        .map(|w| WorkflowView::new(&w.path, w.name.as_deref(), w.clean, w.findings, w.tasks))
        .collect();
    let complete = !(snapshot.truncated || snapshot.walk_truncated);
    let view = ProjectView::new("local", name, shown_path(root, home))
        .with_git(snapshot.git_root.is_some())
        .governed(governing(snapshot, home))
        .listing(workflows, complete);
    match seat(runtime) {
        Some(seat) => view.seated(seat),
        None => view,
    }
}

/// A path as the human reads it: home-relative under the home.
fn shown_path(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// What governs the root: the project file the Session's discovery found (in
/// the root, or in an ancestor named by its path from the root), none, or
/// one it refused.
fn governing(snapshot: &ProjectSnapshot, home: Option<&Path>) -> Manifest {
    if snapshot.project_error.is_some() {
        return Manifest::Refused;
    }
    let Some(file) = snapshot.project_file.as_deref() else {
        return Manifest::Absent;
    };
    let up = file
        .parent()
        .and_then(|dir| snapshot.root.ancestors().position(|a| a == dir));
    match (up, file.file_name()) {
        (Some(0), _) => Manifest::Here,
        (Some(up), Some(name)) => {
            Manifest::Above(format!("{}{}", "../".repeat(up), name.to_string_lossy()))
        }
        _ => Manifest::Above(shown_path(file, home)),
    }
}

/// The intelligence the session reasons with, in words (the model when one
/// is named, and a choice this machine cannot serve now says so); `None`
/// while none was chosen.
fn seat(runtime: &SessionRuntime) -> Option<String> {
    use nika_session::intelligence::{DataLocus, IntelligenceKind};
    if !runtime.intelligence_chosen() {
        return None;
    }
    let chosen = &runtime.intelligence;
    let base = match (&chosen.kind, &chosen.locus) {
        (IntelligenceKind::None, _) => return Some("none, the engine facts answer".to_owned()),
        (IntelligenceKind::Harness { seat }, _) => format!("{seat}, through your account"),
        (IntelligenceKind::Api { provider }, DataLocus::Gateway { host, .. }) => {
            format!("{provider} API through {host}, metered")
        }
        (IntelligenceKind::Api { provider }, _) => format!("{provider} API, metered"),
        (IntelligenceKind::Local { provider }, _) => format!("{provider}, on this machine"),
        _ => "an intelligence this view cannot name".to_owned(),
    };
    let model = chosen
        .model
        .as_deref()
        .map_or_else(String::new, |model| format!(", model {model}"));
    let ready = if chosen.ready { "" } else { ", not ready here" };
    Some(format!("{base}{model}{ready}"))
}

pub mod acquire;
mod candidate;
pub mod feed;
pub(crate) mod legs;
mod look;

/// The one audit fold of a look, for the workspace's own tests.
#[cfg(test)]
pub(crate) fn judge_for_tests(path: &str, witness: String, source: &str) -> Inspected {
    look::judge(path.to_owned(), witness, source.to_owned())
}

#[cfg(all(test, unix))]
mod tests;

/// The project view a live session lends: its runtime's snapshot, in words.
#[cfg(all(test, unix))]
#[allow(clippy::expect_used, clippy::panic)]
mod project_view_tests {
    use super::*;
    use nika_session::ScriptedReasoner;

    /// A temporary directory, removed when the test ends.
    struct Room(PathBuf);

    impl Room {
        fn new(tag: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let path = std::env::temp_dir()
                .join(format!("nika-tui-ws-{tag}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&path).expect("room");
            Self(path)
        }
    }

    impl Drop for Room {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A live conversation over `root` with no intelligence chosen: nothing
    /// reasons and nothing runs.
    fn live(root: &Path, home: Option<PathBuf>) -> Live {
        Live::new(
            root.to_path_buf(),
            IntelligenceCensus::empty(),
            None,
            home,
            Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
            Runners {
                run_once: Box::new(|_, _| panic!("nothing runs here")),
                run_resume: Box::new(|_, _, _, _| panic!("nothing resumes here")),
                run_tapped: None,
            },
        )
    }

    #[test]
    fn the_live_view_projects_the_snapshot_the_runtime_holds() {
        let room = Room::new("project");
        std::fs::create_dir_all(room.0.join(".git")).expect("a git root");
        std::fs::write(room.0.join("nika.yaml"), "nika: demo\n").expect("manifest");
        let project = room.0.join("ventures").join("one");
        std::fs::create_dir_all(&project).expect("a project below it");
        std::fs::write(
            project.join("alpha.nika"),
            "nika: alpha\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
        )
        .expect("alpha");
        std::fs::write(
            project.join("beta.nika"),
            "nika: beta\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n",
        )
        .expect("beta");
        let view = live(&project, Some(room.0.clone()))
            .project()
            .expect("an open runtime lends its project");
        assert_eq!(
            (
                view.host.as_str(),
                view.name.as_str(),
                view.location.as_str()
            ),
            ("local", "one", "~/ventures/one")
        );
        assert_eq!(view.git, Some(true));
        assert_eq!(
            view.manifest,
            Some(Manifest::Above("../../nika.yaml".to_owned())),
            "the parent file governs, named where it is"
        );
        assert!(view.complete);
        let judged: Vec<(&str, Option<&str>, bool, usize)> = view
            .workflows
            .iter()
            .map(|w| (w.path.as_str(), w.name.as_deref(), w.clean, w.tasks))
            .collect();
        assert_eq!(
            judged,
            [
                ("alpha.nika", Some("alpha"), true, 1),
                ("beta.nika", Some("beta"), false, 1)
            ]
        );
        assert_eq!(view.seat, None, "no intelligence chosen, none named");
        assert!(view.pinned.is_none(), "no typed run identity is lent");
    }

    #[test]
    fn a_refused_project_file_and_an_absent_one_are_named() {
        let room = Room::new("refused");
        std::fs::write(room.0.join("nika.yaml"), "not: [valid\n").expect("a broken file");
        let view = live(&room.0, None).project().expect("view");
        assert_eq!(view.manifest, Some(Manifest::Refused));
        let bare = Room::new("bare");
        let view = live(&bare.0, None).project().expect("view");
        assert_eq!(view.manifest, Some(Manifest::Absent));
        assert_eq!(view.git, Some(false));
        assert!(
            view.location.starts_with('/'),
            "no home given: the path stays whole: {}",
            view.location
        );
    }
}
