// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session runtime — the loop a terminal drives: one turn in, one
//! outcome out. A turn is a slash command, a Nika fact (answered from
//! the engine, zero tokens), the answer to an open authoring question, an
//! explicit run line, work to build (the ONE compiler reads it ·
//! [`crate::authoring`]), or a free-text question the selected
//! intelligence reasons over through the broker's bundle and under the
//! guard's reading — in words only: no reply ever becomes a file. No
//! temporary workflow, no trace for a chat turn, no hidden shell.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nika_onboard::compile::{CompileOutcome, TriggerRequirement};

use crate::authoring::{AuthoringRound, AuthoringSeat};
use crate::broker::ContextBroker;
use crate::change::{Applied, PendingGate, ProjectChangeSet, RunRequest, check_on_disk};
use crate::guard::KnownWorld;
use crate::intelligence::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference,
};
use crate::outcome::{GateId, ProposalId, Refusal, RefusalClass};
use crate::reasoner::{ReasonError, SessionReasoner};
use crate::snapshot::ProjectSnapshot;

mod answer;
mod aside;
mod authoring;
mod decision;
mod details;
mod draft;
mod durable;
#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod durable_tests;
mod history;
mod inference;
mod money_gate;
mod money_parse;
mod question;
mod recovery;
mod restore;
mod route;
mod run_budget;
mod unknown_cost;

pub use decision::{DecisionAnswer, decision_answer};
use decision::{is_gate_token, is_no, is_yes, local_command_of};
use run_budget::ceiling_in;
mod schedule;

/// The durable half of the conversation — decisions, not chat.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct IntentDraft {
    /// The goal, as first stated.
    pub goal: Option<String>,
    /// Decisions the human made in words.
    pub decisions: Vec<String>,
    /// Questions still open.
    pub unresolved: Vec<String>,
}

/// What one turn produced.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum TurnOutcome {
    /// The intelligence's reply, read by the guard.
    Reply(String),
    /// A fact from the engine (no model asked).
    Facts(String),
    /// The help card.
    Help(String),
    /// The human closed the session.
    Quit,
    /// The turn was refused: the class a host acts on, and the sentence
    /// that names the fix.
    Refusal(Refusal),
    /// The session asks the first screen again — the NEXT line is the
    /// answer ([`SessionRuntime::choose`]).
    Ask(String),
    /// A question at the consent prompt, answered; the proposal still
    /// waits and the NEXT line is still the consent.
    Held {
        /// The proposal that still waits.
        id: ProposalId,
        /// The answer, and the reminder that the proposal waits.
        preview: String,
    },
    /// The compiler needs one authoring value it cannot invent: the
    /// question, in its words. The NEXT line answers THIS key
    /// ([`SessionRuntime::pending_question`]); a `cancel` drops the round.
    Question {
        /// The stable semantic hole the answer fills (`model` · `const.x`).
        key: String,
        /// The question as the human reads it.
        question: String,
    },
    /// The compiler's Ready candidate, proposed: the review, then the
    /// exact preview of the bytes the apply would land. The NEXT line is
    /// the human's consent ([`SessionRuntime::consent`]); nothing is
    /// written before it, and consent is never a run.
    Proposal {
        /// The identity a consent names ([`SessionRuntime::consent_to`]).
        id: ProposalId,
        /// The exact preview.
        preview: String,
    },
    /// The set landed and its on-disk check is clean: the door runs the
    /// workflow once through the SAME run path as `nika run` and reports
    /// what it observed ([`SessionRuntime::observe_run`]). The apply and
    /// check report rides along.
    RunRequested {
        /// What apply and the check said.
        report: String,
        /// The run the human asked for.
        run: RunRequest,
    },
    /// The run paused at a human gate: the question, asked to the human.
    /// The NEXT line is their answer ([`SessionRuntime::answer_gate`]);
    /// nothing answers for them.
    GateAsk {
        /// The gate an answer names ([`SessionRuntime::answer_gate_for`]).
        id: GateId,
        /// The observation and the question.
        question: String,
    },
    /// The human answered the gate: the door resumes the SAME run
    /// (`--resume <trace> --answer <task>=<value>`) and reports again.
    ResumeRequested {
        /// The workflow, relative to the root.
        workflow: PathBuf,
        /// The paused trace.
        trace: PathBuf,
        /// `task=value`, as the human's line became it.
        answer: String,
    },
    /// An answer BESIDE what waits (« why? » under a question or a gate):
    /// said from the machine's own state; the question or the gate keeps
    /// waiting, nothing is consumed, decided or applied.
    Aside(String),
    /// The intelligence was chosen in the middle of a request: the
    /// choice's own fact, then the outcome of the line that waited for it,
    /// resumed exactly as the human typed it — never re-asked.
    Resumed {
        /// What the choice settled: the path, where the context goes, the seat.
        notice: String,
        /// The waiting line's outcome under the chosen intelligence.
        outcome: Box<TurnOutcome>,
    },
}

/// Why a turn needs the human's intelligence choice now — the reason the
/// contextual first screen names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Need {
    /// A free-text line only an intelligence answers, in words.
    Conversation,
    /// Work the deterministic reader could not settle alone.
    Authoring,
}

/// The help card — the few survivors, and the law that everything
/// meaningful is reachable in words.
pub const HELP: &str = "text                 describe work to build (« read ./notes, draft a summary, write ./out/summary.md ») · Nika compiles it,
                     asks what it cannot invent, shows the workflow, and writes it only when you say yes · consent is never a run
run …                run the workflow you accepted, or one you name (« run brief.nika with a ceiling of 0.05 ») · a paused run asks you
activate             declare the schedule your request asked for in nika.yaml (Nika asks the time zone, the missed policy, the ceiling) · declared is not active: a firer must run
text                 ask, in words · these answer from the engine, no AI asked: your workflows · a file's verdict (« is X valid »)
                     · the builtins · the providers · an example or template for a job · a code (« explain NIKA-… »)
                     · what Nika calls a node, step, trigger, secret, action · the rest goes to your chosen intelligence, in words
/intelligence        the AI this session reasons with · asks the first screen again, the next line is your answer
/status              where you are: the project root, the intelligence and where your context goes, the authoring seat
/why                 beside a question or a gate: what the answer is for, what it lets happen · nothing is consumed
/meaning             what Nika kept of your request, clause by clause, from the compiler's own ledger · a proposal still waits
/proof               after a run: what its trace records (chain · seal · boundary · task hashes) and what it does not prove · judged by `nika trace verify`, never a second walker
/details             how the last workflow was built: the authoring backend and model, calls, tokens and time, the strategy, the decision seat, the engine and spec identity · advanced, on demand
/show                while a proposal waits: print its exact bytes (the review shows the boundary)
/help                this card
/quit                close the session
Name a workflow file in your question to let the session read it (only files under the root are ever read).";

/// The slash commands the session answers, in the help card's order
/// (the most used first, never alphabetical): a door completes them.
pub const SLASH_COMMANDS: &[&str] = &[
    "/help",
    "/status",
    "/why",
    "/meaning",
    "/proof",
    "/details",
    "/show",
    "/intelligence",
    "/quit",
];

/// How many recent turns ride the next prompt.
const RECENT_TURNS: usize = 8;

/// A trace path as the door gave it, resolved under the root when relative.
fn under(root: &Path, trace: &Path) -> PathBuf {
    if trace.is_absolute() {
        trace.to_path_buf()
    } else {
        root.join(trace)
    }
}

/// A byte count a human reads (`1.2 KB`, `340 B`).
#[allow(clippy::cast_precision_loss)] // display-only: a size shown to a human, never computed with
fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// How a door builds the reasoner for a resolved choice (`Send`: a host may
/// hold the runtime on a worker thread while its terminal stays live).
pub type ReasonerFactory =
    Box<dyn Fn(&ResolvedSessionIntelligence) -> Box<dyn SessionReasoner> + Send>;

/// The session over one project, one intelligence, one reasoner.
pub struct SessionRuntime {
    /// The project as observed at open.
    pub snapshot: ProjectSnapshot,
    /// The intelligence the human chose, judged against this machine.
    pub intelligence: ResolvedSessionIntelligence,
    /// The durable intent.
    pub intent: IntentDraft,
    reasoner: Box<dyn SessionReasoner>,
    broker: ContextBroker,
    known: KnownWorld,
    recent: Vec<(String, String)>,
    history: history::HistoryMode,
    census: Option<IntelligenceCensus>,
    home: Option<PathBuf>,
    factory: Option<ReasonerFactory>,
    pending: Option<ProjectChangeSet>,
    /// The proposal pending when an earlier session closed: evidence, never authority.
    restored_draft: Option<draft::Restored>,
    money: money_gate::MoneyState,
    unknown_cost: unknown_cost::UnknownCostState,
    pending_gate: Option<PendingGate>,
    decided: Option<ProposalId>,
    answered: Option<GateId>,
    last_run: Option<(u8, String)>,
    last_workflow: Option<PathBuf>,
    /// The check at the consent that saved the last workflow: clean, or
    /// findings (the rail's Checked field says which).
    last_check_clean: Option<bool>,
    /// The trace the last observed run left (`/proof` reads it).
    last_trace: Option<PathBuf>,
    /// The authoring round whose question the next line answers.
    authoring: Option<AuthoringRound>,
    /// The proposal a revision's question set aside, with the reading it came from: it waits
    /// again unchanged unless the revised proposal replaces it (`keep_revising`).
    revising: Option<(ProjectChangeSet, Option<CompileOutcome>)>,
    /// Who asks that question and which ones were answered (memory only).
    questions: question::Identities,
    /// The cognition the compiler may use, derived from the reasoner.
    seat: AuthoringSeat,
    /// The strategy and the knowledge snapshot a provider seat authors
    /// under: read once when a host door opens the session, or set by it.
    authoring_context: crate::authoring::AuthoringContext,
    /// Where a truthful progress line goes while the compiler works
    /// (presentation only: it never carries workflow meaning).
    progress: Option<ProgressHook>,
    /// A run request waiting on the values of the workflow's declared inputs.
    run_inputs: Option<authoring::RunInputs>,
    /// Whether the human chose (or kept) an intelligence. Opened without one,
    /// the session works from the engine's facts and the deterministic
    /// compiler, and asks the first screen only when a turn needs more.
    chosen: bool,
    /// The line that waits for the intelligence choice: resumed as typed
    /// once the choice is made, dropped on `cancel`.
    interrupted: Option<String>,
    /// The first screen is on the table: the NEXT line is a choice.
    pending_choice: bool,
    /// The last recovery card (a turn that could not be finished), kept so
    /// « what happened? » repeats it without a call.
    last_recovery: Option<String>,
    /// The compiler's last reading of the request (its ledger is the
    /// Meaning view), kept while its question or proposal waits.
    last_outcome: Option<CompileOutcome>,
    /// The schedule the pending proposal asked for (kept beside the
    /// program); becomes `last_trigger` when the human saves that program.
    pending_trigger: Option<TriggerRequirement>,
    /// The schedule of the last saved workflow: what « activate » declares.
    last_trigger: Option<TriggerRequirement>,
    /// The activation under way: its questions own the next lines.
    activation: Option<schedule::Activation>,
    /// Identifies the schedule declaration solely to discard it on revision, never to grant it.
    activation_proposal: Option<ProposalId>,
    /// The bounded classifier of open language, when a door injects one
    /// (a decision seat, a scripted one in tests); otherwise the session's
    /// own intelligence answers the routing prompt, or the fallback.
    classifier: Option<Box<dyn crate::turn::TurnClassifier>>,
    /// Every non-trivial route decided in this session (`/details`).
    routes: Vec<crate::turn::RouteRecord>,
}

/// A door's sink for progress lines (« Working through this workflow… »);
/// `Send` so the runtime may run a turn on a worker thread.
pub type ProgressHook = Box<dyn Fn(&str) + Send>;

impl std::fmt::Debug for SessionRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionRuntime")
            .field("root", &self.snapshot.root)
            .field("intelligence", &self.intelligence.kind)
            .field("reasoner", &self.reasoner.name())
            .finish_non_exhaustive()
    }
}

impl SessionRuntime {
    /// Open a session in `cwd` with the chosen intelligence and its reasoner.
    #[must_use]
    pub fn open(
        cwd: &Path,
        intelligence: ResolvedSessionIntelligence,
        reasoner: Box<dyn SessionReasoner>,
    ) -> Self {
        let snapshot = ProjectSnapshot::observe(cwd);
        let broker = ContextBroker::new(snapshot.root.clone());
        let known = KnownWorld::installed(&snapshot.root);
        let mut session = Self {
            snapshot,
            intelligence,
            intent: IntentDraft::default(),
            reasoner,
            broker,
            known,
            recent: Vec::new(),
            history: history::HistoryMode::Ephemeral,
            census: None,
            home: None,
            factory: None,
            pending: None,
            restored_draft: None,
            money: money_gate::MoneyState::default(),
            unknown_cost: unknown_cost::UnknownCostState::default(),
            pending_gate: None,
            decided: None,
            answered: None,
            last_run: None,
            last_workflow: None,
            last_check_clean: None,
            last_trace: None,
            authoring: None,
            revising: None,
            questions: question::Identities::default(),
            seat: AuthoringSeat::Deterministic { why: None },
            authoring_context: crate::authoring::AuthoringContext::default(),
            progress: None,
            run_inputs: None,
            chosen: true,
            interrupted: None,
            pending_choice: false,
            last_recovery: None,
            classifier: None,
            routes: Vec::new(),
            last_outcome: None,
            pending_trigger: None,
            last_trigger: None,
            activation: None,
            activation_proposal: None,
        };
        session.refresh_seat();
        session
    }

    /// Where the automation stands, in one line the doors show beside the
    /// prompt — the state that explains the next gesture, compiled from
    /// the machine's own facts, never a concatenation of flags.
    #[must_use]
    pub fn status_line(&self) -> String {
        if self.waiting_cost_choice() {
            return "Waiting for a one-time unknown-cost decision · nothing sent".into();
        }
        if self.pending_choice {
            return "Needs your choice of intelligence · the request waits".to_owned();
        }
        if let Some(gate) = &self.pending_gate {
            return format!(
                "Waiting for your answer · `{}` paused at `{}`",
                gate.workflow.display(),
                gate.task
            );
        }
        if let Some(set) = &self.pending {
            let files: Vec<String> = set
                .changes
                .iter()
                .map(|c| format!("`{}`", c.path().display()))
                .collect();
            return format!(
                "Ready for review · {} · nothing saved, nothing run",
                files.join(" · ")
            );
        }
        if let Some(question) = self.pending_question() {
            return format!("Needs one answer · {}", question.label);
        }
        if let Some(name) = self.pending_input() {
            return format!("Needs one value before it runs · `{name}`");
        }
        if let Some(key) = self.pending_activation() {
            return format!("Needs one value to declare the schedule · `{key}`");
        }
        if let Some((exit, _)) = &self.last_run {
            let word = match exit {
                0 => "Done · the run succeeded",
                1 => "Done · the run failed",
                2 => "Not run · the check refused",
                3 => "Not run · the environment refused",
                4 => "Paused · a gate waits",
                130 => "Stopped · the run was interrupted",
                _ => "Done · an unknown code",
            };
            return match &self.last_workflow {
                Some(w) => format!("{word} · `{}`", w.display()),
                None => word.to_owned(),
            };
        }
        if let Some(w) = &self.last_workflow {
            if let Some(declared) = schedule::declared_state(&self.snapshot.root, w) {
                return declared;
            }
            return format!(
                "Saved · checked · not active · nothing has run · `{}`",
                w.display()
            );
        }
        String::new()
    }

    /// Where the automation stands as separate facts — the rail the
    /// renderer keeps above the status row: DECLARED is never ACTIVE,
    /// SAVED is never RUN, findings are not a clean check.
    #[must_use]
    pub fn lifecycle(&self) -> crate::lifecycle::Lifecycle {
        let declared_active = self
            .last_workflow
            .as_ref()
            .and_then(|w| schedule::declared_entry(&self.snapshot.root, w))
            .map(|(_, active, _)| active);
        crate::lifecycle::Lifecycle::from_facts(&crate::lifecycle::LifecycleFacts {
            proposal_waits: self.pending.is_some(),
            composing: self.pending_question().is_some()
                || self.pending_input().is_some()
                || self.pending_activation().is_some(),
            saved: self.last_workflow.is_some(),
            check_clean: self.last_check_clean,
            declared_active,
            run: if self.pending_gate.is_some() {
                crate::lifecycle::RunFact::GateWaits
            } else {
                self.last_run
                    .as_ref()
                    .map_or(crate::lifecycle::RunFact::Nothing, |(exit, _)| {
                        crate::lifecycle::RunFact::Exit(*exit)
                    })
            },
        })
    }

    /// `/meaning` — the compiler's reading of the request, clause by
    /// clause, from its own ledger: beside a proposal it HOLDS it (a
    /// `Held`, the consent still waits); beside a question or after an
    /// incomplete it is an aside; never a score, never invented.
    fn meaning_unrecorded(&mut self) -> TurnOutcome {
        let view = if let Some(out) = &self.last_outcome {
            crate::meaning::render(out).unwrap_or_else(|| crate::meaning::UNAVAILABLE.to_owned())
        } else {
            if self.money.current.is_some() {
                return TurnOutcome::Aside(self.money_line());
            }
            return TurnOutcome::Facts(
                "nothing to read yet · describe work and Nika compiles it; `/meaning` then lists what it kept of your request"
                    .to_owned(),
            );
        };
        let mut view = format!("{view}\n{}", self.money_line());
        if let Some(receipt) = self
            .last_outcome
            .as_ref()
            .and_then(|out| out.provenance.authoring.as_ref())
            .filter(|receipt| {
                receipt
                    .backend
                    .as_ref()
                    .is_some_and(|b| b["kind"] == "harness_infer")
            })
        {
            details::receipt_lines(receipt, &mut view);
        }
        match &self.pending {
            Some(set) => TurnOutcome::Held {
                id: self.proposal_id(set),
                preview: format!(
                    "{view}\n(the proposal still waits · `yes` applies it · `no` discards it)"
                ),
            },
            None => TurnOutcome::Aside(view),
        }
    }

    /// Open a session before any intelligence is chosen (the first run):
    /// the facts answer and the deterministic compiler reads work at once;
    /// the first screen is asked in context, the first time a turn needs an
    /// intelligence, and the line that needed it resumes after the choice.
    /// The census, the home and the factory are the ones `open_with` takes.
    #[must_use]
    pub fn open_unchosen(
        cwd: &Path,
        census: IntelligenceCensus,
        home: Option<&Path>,
        factory: ReasonerFactory,
    ) -> Self {
        let none = UserIntelligencePreference::new(IntelligenceKind::None, None);
        let mut session = Self::open_with(cwd, census, &none, home, factory);
        session.chosen = false;
        session
    }

    /// Whether the first screen waits for its answer: the NEXT line is a
    /// choice ([`SessionRuntime::choose`]), whatever else may wait behind it.
    #[must_use]
    pub fn pending_choice(&self) -> bool {
        self.pending_choice
    }

    /// The key of the value an activation waits for (`project.timezone` ·
    /// `project.missed` · `project.ceiling`), when « activate » is under
    /// way: the next line answers it, on its own prompt.
    #[must_use]
    pub fn pending_activation(&self) -> Option<&'static str> {
        self.activation
            .as_ref()
            .and_then(schedule::Activation::current)
    }

    /// Whether an intelligence was chosen or kept for this session.
    #[must_use]
    pub fn intelligence_chosen(&self) -> bool {
        self.chosen
    }

    /// The first screen, in context: why this turn needs an intelligence,
    /// the options this machine holds, and the promise that the line is
    /// kept. The next line chooses; `cancel` continues without one.
    pub(crate) fn ask_for_intelligence(&mut self, line: &str, need: Need) -> TurnOutcome {
        let Some(census) = &self.census else {
            let why = self
                .intelligence
                .why
                .as_deref()
                .filter(|_| !self.intelligence.ready)
                .unwrap_or("no conversational intelligence");
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::NoIntelligence,
                format!(
                    "{why} — the facts still answer (workflows · builtins · providers · check · explain) · describe work to build and Nika compiles it"
                ),
            ));
        };
        let why = match need {
            Need::Conversation => "to answer this in words",
            Need::Authoring => "to finish reading this request — what it read on its own is kept",
        };
        // A kept choice this machine cannot serve is the problem, said
        // first in plain words (the ⚠ line of the banner), before the ways on.
        let unserved = self
            .intelligence
            .why
            .as_deref()
            .filter(|_| !self.intelligence.ready)
            .map_or(String::new(), |w| format!("  ⚠ {w}\n"));
        self.interrupted = Some(line.to_owned());
        self.pending_choice = true;
        TurnOutcome::Ask(format!(
            "Nika needs an intelligence for this part\n  {why}\n{unserved}  your request is kept and resumes after the choice · `cancel` continues without one\n\n{}",
            census.options_screen()
        ))
    }

    /// Where progress lines go while the compiler works under a seat: a
    /// door prints them; a remote host projects them. Presentation only.
    pub fn on_progress(&mut self, hook: ProgressHook) {
        self.progress = Some(hook);
    }

    /// One typed activity to the door, when one listens: the door prints
    /// its line (the plain loop) or draws it in the busy row (the renderer).
    pub(super) fn activity(&self, activity: &crate::activity::Activity) {
        self.progress(&activity.line());
    }

    /// One truthful progress line to the door, when one listens.
    pub(super) fn progress(&self, line: &str) {
        if let Some(hook) = &self.progress {
            hook(line);
        }
    }

    /// Open a session that can re-choose its intelligence in-session:
    /// the census it judges against, the home the choice is kept under,
    /// and the door's reasoner factory.
    #[must_use]
    pub fn open_with(
        cwd: &Path,
        census: IntelligenceCensus,
        pref: &UserIntelligencePreference,
        home: Option<&Path>,
        factory: ReasonerFactory,
    ) -> Self {
        let intelligence = ResolvedSessionIntelligence::resolve(pref, &census);
        let reasoner = factory(&intelligence);
        let mut session = Self::open(cwd, intelligence, reasoner);
        session.census = Some(census);
        session.home = home.map(Path::to_path_buf);
        session.factory = Some(factory);
        // The host door's authoring configuration, read once, here: the
        // names `nika compile` reads, through the same parser.
        session.authoring_context = crate::authoring::AuthoringContext::from_env();
        session
    }

    /// The answer to the first screen asked in-session (`/intelligence`):
    /// the choice is judged, kept under the home when one exists, and the
    /// reasoner rebuilt — refused with its fix when this machine cannot
    /// serve it, and the previous choice stands.
    fn choose_unrecorded(&mut self, answer: &str) -> TurnOutcome {
        let (Some(census), Some(factory)) = (&self.census, &self.factory) else {
            self.pending_choice = false;
            self.interrupted = None;
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "this session cannot re-choose its intelligence — quit and open `nika` again",
            ));
        };
        // Leaving is always one line away: no choice is made, the waiting
        // line is dropped (never sent anywhere).
        if is_quit(answer) {
            self.pending_choice = false;
            self.interrupted = None;
            return TurnOutcome::Quit;
        }
        // A cancel keeps going without a choice: the waiting line is dropped
        // (never sent anywhere), the previous choice stands.
        if crate::authoring::is_cancel(answer) {
            self.pending_choice = false;
            let text = match self.interrupted.take() {
                Some(_) => {
                    "no intelligence chosen · your request was not sent anywhere · the facts still answer (workflows · builtins · providers · check · explain) and work Nika can read on its own compiles · `/intelligence` chooses later"
                }
                None => "the choice stands · `/intelligence` asks again",
            };
            return TurnOutcome::Facts(text.to_owned());
        }
        // A local command beside the first screen answers from the session's
        // own facts; the choice keeps waiting (a slash line is never a choice).
        if let Some(command) = local_command_of(answer) {
            return self.answer_locally(command);
        }
        let pref = match census.choose(answer) {
            Ok(pref) => pref,
            // The screen stays on the table with the line it holds: the
            // next line is still a choice (a typo never loses a request).
            Err(why) => {
                return TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::IntelligenceRefused,
                    format!("{why} · the previous choice stands"),
                ));
            }
        };
        let resolved = ResolvedSessionIntelligence::resolve(&pref, census);
        let kept = match &self.home {
            Some(home) => pref
                .save(home)
                .map(|()| "kept")
                .unwrap_or("holds for this session only"),
            None => "holds for this session only",
        };
        self.reasoner = factory(&resolved);
        self.intelligence = resolved;
        self.refresh_seat();
        self.chosen = true;
        self.pending_choice = false;
        let notice = format!(
            "{} · {kept}\n  {}",
            self.intelligence_line(),
            self.seat.line()
        );
        // The line that waited resumes exactly as typed, under the choice.
        match self.interrupted.take() {
            Some(line) => {
                let outcome = self.turn_unrecorded(&line);
                TurnOutcome::Resumed {
                    notice,
                    outcome: Box::new(outcome),
                }
            }
            None => TurnOutcome::Facts(notice),
        }
    }

    /// The one line that names the path and where the context goes.
    fn intelligence_line(&self) -> String {
        let locus = self.intelligence.locus.line();
        let name = self.reasoner.name();
        if matches!(self.intelligence.kind, IntelligenceKind::None) || locus.starts_with(&name) {
            format!("intelligence: {locus}")
        } else {
            format!("intelligence: {name} · {locus}")
        }
    }

    /// The banner a terminal prints at open — the human's level: the
    /// project, the one question, how to go on. An explicit choice this
    /// machine cannot serve is the one warning that belongs here. The
    /// engine's own facts (the root, the path, the seat) are `/status`.
    #[must_use]
    pub fn banner(&self) -> String {
        let project = self
            .snapshot
            .root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| self.snapshot.root.display().to_string());
        let mut text = format!(
            "Nika · {project}\n\nWhat do you want to automate?\n  describe the outcome · Nika asks only for what's missing · /help · /status"
        );
        if let Some(why) = &self.intelligence.why {
            let _ = write!(text, "\n  ⚠ {why}");
        }
        // A named authoring configuration this session cannot honor is the
        // same kind of warning: said at open, refused at the first seated turn.
        if let Some(why) = self.authoring_context.refusal() {
            let _ = write!(text, "\n  ⚠ authoring knowledge: {why}");
        }
        text
    }

    /// Where the session stands, in the engine's words (`/status`): the
    /// root it observed, the intelligence and where the context goes, the
    /// seat authoring reasons with, the doors.
    #[must_use]
    pub fn status(&self) -> String {
        let readiness = match &self.intelligence.why {
            Some(why) => format!("\n  ⚠ {why}"),
            None => String::new(),
        };
        let chosen = if self.chosen {
            ""
        } else {
            " (not chosen yet · asked when a turn needs one · `/intelligence` chooses now)"
        };
        format!(
            "session\n  root: {}\n  {}{chosen}{readiness}\n  {}\n  {}\n  {}\n  /help for the card · /quit to close",
            self.snapshot.root.display(),
            self.intelligence_line(),
            self.seat.line(),
            self.authoring_context.line(),
            self.money_line()
        )
    }

    /// One turn.
    fn turn_unrecorded(&mut self, input: &str) -> TurnOutcome {
        // A new turn discards a pending proposal: consent is the NEXT line
        // and nothing else (the door routes that line to `consent`).
        self.pending = None;
        self.money.pending = None;
        let original = input;
        let input = input.trim();
        match input {
            "/quit" | "/exit" => return TurnOutcome::Quit,
            "/help" => return TurnOutcome::Help(self.help_card()),
            "/status" => return TurnOutcome::Facts(self.status()),
            "/why" => return self.explain_pending(),
            "/meaning" => return self.meaning_unrecorded(),
            "/proof" => return self.proof_unrecorded(),
            "/details" => return TurnOutcome::Facts(self.details()),
            "/intelligence" => {
                return match &self.census {
                    Some(census) => {
                        let screen =
                            format!("{}\n{}", self.intelligence_card(), census.first_screen());
                        self.pending_choice = true;
                        TurnOutcome::Ask(screen)
                    }
                    None => TurnOutcome::Facts(self.intelligence_card()),
                };
            }
            _ => {}
        }
        // « what happened? » repeats the last recovery card from memory,
        // whatever waits: it consumes nothing and calls nothing.
        if crate::authoring::is_what_happened(input)
            && let Some(card) = self.last_recovery()
        {
            return card;
        }
        // An open authoring question owns the next line — before any
        // fact, digit or model reads it (`./notes` answers « which folder »).
        if self.authoring.is_some() {
            if let Err(refusal) = self.admit_money(original, true) {
                return refusal;
            }
            let outcome = self.answer_question_unrecorded(input);
            return self.keep_revising(outcome);
        }
        // A run waiting on a declared input owns the next line the same way.
        if self.run_inputs.is_some() {
            return self.answer_input_unrecorded(input);
        }
        // An activation waiting on its values owns the next line the same way.
        if self.activation.is_some() {
            return self.answer_activation_unrecorded(input);
        }
        if input.is_empty() {
            return TurnOutcome::Facts(String::new());
        }
        // A consent word with nothing pending answers nothing: it is neither
        // work to build nor a question, and it never reaches a model.
        if is_yes(input) || is_no(input) {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "nothing waits for a yes or a no here — a proposal asks `apply? ›` first · describe the outcome you want, or `/help`",
            ));
        }
        if matches!(input, "1" | "2" | "3" | "4") {
            return TurnOutcome::Facts(format!(
                "{}\nthe intelligence is already chosen — `/intelligence` shows the first screen again and the next line picks",
                self.intelligence_line()
            ));
        }
        if let Some(fact) = crate::facts::answer(input, &self.snapshot, &self.snapshot.root) {
            self.remember(input, &fact);
            return TurnOutcome::Facts(fact);
        }
        if !self.local_run_line(original)
            && let Err(refusal) = self.admit_money(original, false)
        {
            return refusal;
        }
        if let Some(outcome) = self.run_turn(original) {
            return outcome;
        }
        // « activate »: the schedule the last saved workflow asked for
        // becomes a declaration to review — never by a `yes`, never by saving.
        if schedule::is_activate(input) {
            return self.activate_turn();
        }
        // Work to build reaches the ONE compiler; only a line that reads as
        // no work at all goes to the conversation.
        if let Some(outcome) = self.author_unrecorded(original) {
            return outcome;
        }
        // No intelligence chosen yet: this is the first turn that needs one.
        // The first screen is asked in context and this line waits for it.
        if !self.chosen {
            return self.ask_for_intelligence(input, Need::Conversation);
        }
        self.converse_unrecorded(input)
    }

    /// A free-text line the chosen intelligence answers, in words only,
    /// through the broker's bundle and under the guard's reading.
    fn converse_unrecorded(&mut self, input: &str) -> TurnOutcome {
        if self.money_blocks_cognition() {
            return self.cognition_money_refusal();
        }
        // A kept choice this machine cannot serve: the problem in plain
        // words, the ways on, and the line kept for the choice — never a
        // call on a path that cannot answer, never a silent replacement.
        if !self.intelligence.ready {
            return self.ask_for_intelligence(input, Need::Conversation);
        }
        let named = named_files(input);
        let bundle = self.broker.bundle(
            &self.snapshot,
            self.intent.goal.as_deref(),
            &named,
            &self.intelligence.locus.line(),
        );
        let prompt = ContextBroker::prompt(&bundle, &self.recent, input);
        match self.reason_with_money(&prompt, false) {
            // In words only: a reply never becomes a file (the compiler is
            // the ONE door to a workflow · ADR-125 wave 5 retired here).
            Ok(reply) => {
                let findings = self.known.audit(&reply.text);
                let shown = KnownWorld::correct(&reply.text, &findings);
                self.remember(input, &shown);
                TurnOutcome::Reply(shown)
            }
            Err(ReasonError::NoIntelligence) => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::NoIntelligence,
                "no conversational intelligence — the facts still answer (workflows · builtins · providers · check · explain) · `/intelligence` to choose a path",
            )),
            // The path did not answer: a recovery card (what is kept, what
            // did not happen, the ways on); the choice stands, nothing is
            // substituted.
            Err(e) => {
                let what = format!(
                    "I couldn't use {} (the conversational intelligence) for this part",
                    self.reasoner.name()
                );
                self.recovery_for(
                    Some(input),
                    Some(RefusalClass::IntelligenceRefused),
                    &what,
                    &e.to_string(),
                )
            }
        }
    }

    /// `/why` — the aside for whatever waits: an authoring question, a
    /// declared input, a gate, a proposal; a fact when nothing waits.
    fn explain_pending(&self) -> TurnOutcome {
        if let Some(round) = &self.authoring
            && let Some(question) = round.current()
        {
            return TurnOutcome::Aside(aside::explain_question(question, round));
        }
        if let Some(inputs) = &self.run_inputs
            && let Some(name) = inputs.first_needed()
        {
            return TurnOutcome::Aside(aside::explain_input(
                inputs.workflow(),
                name,
                inputs.remaining(),
            ));
        }
        if let Some(gate) = &self.pending_gate {
            return TurnOutcome::Aside(aside::explain_gate(gate, &self.snapshot.root));
        }
        if let Some(set) = &self.pending {
            return TurnOutcome::Aside(format!(
                "{}\n(the proposal still waits · `yes` applies it · `no` discards it)",
                set.effects_fact()
            ));
        }
        TurnOutcome::Facts(
            "nothing waits for you right now · describe work, ask a fact, or `run …` an accepted workflow"
                .to_owned(),
        )
    }

    /// The human's answer to a proposal: `yes` lands the set (every
    /// witness checked before the first write · atomic writes · nothing
    /// outside the set), the real check follows every workflow written,
    /// and a run the human asked for is requested ONLY when that check is
    /// clean. A no discards the set; other lines route without granting consent.
    fn consent_unrecorded(&mut self, answer: &str) -> TurnOutcome {
        let Some(set) = self.pending.take() else {
            return TurnOutcome::Refusal(self.nothing_pending());
        };
        // Leaving is always one line away: the proposal is dropped, nothing
        // is written (a consent is the next line, never a later session's).
        if is_quit(answer) {
            return TurnOutcome::Quit;
        }
        let id = self.proposal_id(&set);
        // The compiler's reading of the request, on request, the proposal
        // held: what it kept, clause by clause, is never a consent.
        if crate::authoring::is_meaning(answer) {
            self.pending = Some(set);
            return self.meaning_unrecorded();
        }
        // A local command beside the proposal answers from the session's own
        // facts, the proposal held: never the model, never a consent.
        if let Some(command) = local_command_of(answer) {
            self.pending = Some(set);
            return self.answer_locally(command);
        }
        // The exact bytes, on request, the proposal held: consent stays a yes.
        if matches!(answer.trim(), "/show" | "show") {
            let preview = self.proposal_preview(&set);
            self.pending = Some(set);
            return TurnOutcome::Held {
                id,
                preview: format!(
                    "{preview}(the proposal still waits · `yes` applies it · `no` discards it)"
                ),
            };
        }
        if is_no(answer) {
            self.decided = Some(id);
            return TurnOutcome::Facts(
                "discarded · nothing was written · ask again for the change when ready".to_owned(),
            );
        }
        if !is_yes(answer) {
            // Not a protocol token: open language. Its act is a bounded
            // decision over the typed state and the RAW line (the door's
            // classifier, the session's intelligence, else UNKNOWN) —
            // never a word list, never a consent.
            return self.consent_money_route(set, &id, answer);
        }
        let applied = match set.apply_attempt() {
            Ok(applied) => applied,
            // No write returned success; the failing target may have changed.
            // The proposal is neither pending nor decided, so retry by identity
            // reads `wrong_state`, never a false claim of a completed effect.
            Err(attempt) if attempt.written.is_empty() => {
                return TurnOutcome::Refusal(Refusal::from_change(&attempt.error));
            }
            // A later write failed after this call itself landed files.
            // The account is the write loop's record, not a tree scan;
            // the proposal stays undecided.
            Err(attempt) => {
                let evidence = self.evidence_partial(&set, &id, &attempt);
                let text = format!("{}{evidence}", attempt.refusal_text(&set));
                self.snapshot = ProjectSnapshot::observe(&self.snapshot.cwd);
                let class = Refusal::from_change(&attempt.error).class;
                return TurnOutcome::Refusal(Refusal::new(class, text));
            }
        };
        self.report_landed(set, &applied, id)
    }

    /// After a yes lands the set: mark decided, check every workflow,
    /// re-observe, remember, and request a run only when that check is
    /// clean. Empty-write and mid-set Io stay on `consent` so a refusal
    /// never becomes `already_consumed`.
    fn report_landed(
        &mut self,
        set: ProjectChangeSet,
        applied: &Applied,
        id: ProposalId,
    ) -> TurnOutcome {
        self.save_proposal_money(&set, &id);
        let evidence = self.evidence_applied(&set, &id, applied);
        self.decided = Some(id);
        let written: Vec<String> = applied
            .written
            .iter()
            .map(|p| format!("`{}`", p.display()))
            .collect();
        let mut report = format!("applied · wrote {}{evidence}", written.join(" · "));
        let mut all_clean = true;
        for wf in set.workflows() {
            let audit = check_on_disk(&set.root, &wf);
            all_clean &= audit.clean;
            let _ = write!(
                report,
                "\n  check · `{}` · {}",
                wf.display(),
                if audit.clean {
                    "clean ✔"
                } else {
                    "findings ✖"
                }
            );
            for f in &audit.findings {
                let _ = write!(report, "\n    · {f}");
            }
            if let Some(line) =
                crate::change::compact_hints(&audit.hints, &wf.display().to_string())
            {
                let _ = write!(report, "\n    · {line}");
            }
        }
        self.snapshot = ProjectSnapshot::observe(&self.snapshot.cwd);
        self.remember("(consent)", &report);
        // The workflow just accepted is the one « run it » names next —
        // an explicit line, never this consent — and the schedule its
        // request asked for is what « activate » declares.
        let landed_workflow = set.workflows().into_iter().next();
        if let Some(first) = landed_workflow.clone() {
            // Run evidence belongs to the previous saved bytes. A new Save is not a Run,
            // including when it replaces the workflow at the same path.
            self.last_run = None;
            self.last_workflow = Some(first);
            self.last_check_clean = Some(all_clean);
            self.last_trigger = self.pending_trigger.take();
        }
        let project_only = landed_workflow.is_none()
            && set
                .changes
                .iter()
                .any(|c| c.path() == std::path::Path::new("nika.yaml"));
        match set.run {
            Some(run) if all_clean => {
                self.last_workflow = Some(run.workflow.clone());
                TurnOutcome::RunRequested { report, run }
            }
            Some(_) => {
                report.push_str(
                    "\n  the run was not started: findings stop it — repair them, then ask to run",
                );
                TurnOutcome::Facts(report)
            }
            None if project_only => {
                report.push_str(
                    "\nDeclared in `nika.yaml` · not active: a firer must run on this machine\n  `nika serve` fires it while it runs · `nika arm --emit launchd --write` installs the OS unit · `nika arm` lists what is declared and proves what fired",
                );
                TurnOutcome::Facts(report)
            }
            None if all_clean => {
                report.push_str(
                    "\nSaved · checked · not active · nothing has run\n  say « run it » to run it once (a ceiling is announced first)",
                );
                if let Some(t) = &self.last_trigger
                    && t.status == nika_onboard::compile::TriggerStatus::RequiresBinding
                {
                    let _ = write!(
                        report,
                        "\n  say « activate » to declare « {} » in `nika.yaml` (Nika asks the time zone, the missed policy and the ceiling first) · saving activated nothing",
                        t.source_hint.as_deref().unwrap_or("the schedule")
                    );
                }
                TurnOutcome::Facts(report)
            }
            None => TurnOutcome::Facts(report),
        }
    }

    /// The proposal waiting for a consent, when one is (its identity: the
    /// witness of the preview's bytes).
    #[must_use]
    pub fn pending_proposal(&self) -> Option<ProposalId> {
        self.pending.as_ref().map(|set| self.proposal_id(set))
    }

    /// A consent that names the proposal it answers — a remote host, a
    /// reconnect (ADR-133): refused as stale when another proposal waits,
    /// as already consumed when that proposal was decided, as the wrong
    /// state when none is pending. Never applied twice.
    pub fn consent_to(&mut self, id: &ProposalId, answer: &str) -> TurnOutcome {
        if self.waiting_cost_choice() {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::StaleRevision,
                "a new cost review waits; old proposal consent cannot answer it",
            ));
        }
        match self.pending_proposal() {
            Some(waiting) if waiting != *id => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::StaleRevision,
                format!(
                    "the proposal {id} is not the one waiting ({waiting}) — read the preview again before consenting"
                ),
            )),
            Some(_) => self.consent(answer),
            None if self.decided.as_ref() == Some(id) => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::AlreadyConsumed,
                format!("the proposal {id} was already decided — nothing is pending"),
            )),
            None => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!(
                    "nothing is pending — the proposal {id} is neither waiting nor the last decided · ask for the change again"
                ),
            )),
        }
    }

    /// The refusal for a consent with no proposal: the last one was
    /// already decided, or none was ever proposed.
    fn nothing_pending(&self) -> Refusal {
        match &self.decided {
            Some(id) => Refusal::new(
                RefusalClass::AlreadyConsumed,
                format!(
                    "the proposal {id} was already decided — nothing is pending · ask again for the change"
                ),
            ),
            None => Refusal::new(
                RefusalClass::WrongState,
                "nothing is pending — ask for a change first",
            ),
        }
    }

    /// The gate waiting for an answer, when one is.
    #[must_use]
    pub fn waiting_gate(&self) -> Option<GateId> {
        self.pending_gate
            .as_ref()
            .map(|gate| GateId::new(&gate.trace, &gate.task))
    }

    /// An answer that names the gate it decides (ADR-133): refused as
    /// stale when another gate waits, as already consumed when that gate
    /// was answered, as the wrong state when none waits. The same gate
    /// answers once.
    pub fn answer_gate_for(&mut self, id: &GateId, line: &str) -> TurnOutcome {
        match self.waiting_gate() {
            Some(waiting) if waiting != *id => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::StaleRevision,
                format!("the gate {id} is not the one waiting ({waiting})"),
            )),
            Some(_) => self.answer_gate(line),
            None if self.answered.as_ref() == Some(id) => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::AlreadyConsumed,
                format!("the gate {id} was answered once — a decided gate stays decided"),
            )),
            None => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!(
                    "no run is waiting for an answer — the gate {id} is neither waiting nor the last answered"
                ),
            )),
        }
    }

    /// The refusal for an answer with no gate: the last one was answered,
    /// or no run ever paused.
    fn no_gate_waiting(&self) -> Refusal {
        match &self.answered {
            Some(id) => Refusal::new(
                RefusalClass::AlreadyConsumed,
                format!("the gate {id} was answered once — no run is waiting for an answer"),
            ),
            None => Refusal::new(RefusalClass::WrongState, "no run is waiting for an answer"),
        }
    }

    /// What the door observed of the run it started for the human: the
    /// exit code's meaning and the trace, remembered as a fact of this
    /// session — never re-run, never re-authorized (attaching is
    /// observation). A pause (exit 4) whose trace carries the gate
    /// becomes the question asked to the human.
    fn observe_run_unrecorded(&mut self, exit: u8, trace: Option<&Path>) -> TurnOutcome {
        let root = self.snapshot.root.clone();
        // The trace's own frames, when the door left one this session can
        // read: the views below say what they prove, the line stays the fact.
        let facts = trace.and_then(|t| crate::run_view::RunFacts::read(&under(&root, t)));
        let line = self.observation_line(exit, trace, facts.is_none());
        self.last_trace = trace.map(Path::to_path_buf);
        if exit == 4
            && let (Some(trace), Some(workflow)) = (trace, self.last_workflow.clone())
            && let Some(gate) = PendingGate::from_trace(&workflow, trace)
        {
            let gated = aside::gated_tasks(&root.join(&workflow), &gate.task);
            let view = facts.as_ref().map_or_else(
                || gate.question(),
                |f| f.gate(&workflow, &gate.message, &gate.mode, &gated),
            );
            let id = GateId::new(&gate.trace, &gate.task);
            self.pending_gate = Some(gate);
            return TurnOutcome::GateAsk {
                id,
                question: format!("{line}\n{view}"),
            };
        }
        match (exit, facts, self.last_workflow.clone()) {
            (0 | 1, Some(f), Some(workflow)) => {
                TurnOutcome::Facts(format!("{}\n  {line}", f.result(&root, &workflow)))
            }
            _ => TurnOutcome::Facts(line),
        }
    }

    /// `/proof` — what the last observed run's trace proves, through the
    /// ONE verify door; before any run, where a proof will come from.
    fn proof_unrecorded(&self) -> TurnOutcome {
        let Some(trace) = &self.last_trace else {
            return TurnOutcome::Facts(
                "No run observed in this session yet · « run it » runs the accepted workflow once · `/proof` then reads the trace it leaves (`nika trace ls` lists earlier ones)".to_owned(),
            );
        };
        match crate::run_view::RunFacts::read(&under(&self.snapshot.root, trace)) {
            Some(facts) => TurnOutcome::Facts(facts.proof(&self.snapshot.root)),
            None => TurnOutcome::Facts(format!(
                "the trace `{}` cannot be read now · `nika trace verify {}` judges it from the shell",
                trace.display(),
                trace.display()
            )),
        }
    }

    /// The human's answer to a pending gate: the resume the door runs.
    /// Nothing answers for the human; an empty line is not an answer.
    fn answer_gate_unrecorded(&mut self, line: &str) -> TurnOutcome {
        let Some(gate) = self.pending_gate.take() else {
            return TurnOutcome::Refusal(self.no_gate_waiting());
        };
        // Leaving is always one line away: the gate keeps waiting in its
        // paused trace (and in the record), nothing answers for the human.
        if is_quit(line) {
            self.pending_gate = Some(gate);
            return TurnOutcome::Quit;
        }
        // « why? » beside the gate: what the answer lets happen, from the
        // workflow's own bytes; the gate keeps waiting.
        if crate::authoring::is_why(line) {
            let text = aside::explain_gate(&gate, &self.snapshot.root);
            self.pending_gate = Some(gate);
            return TurnOutcome::Aside(text);
        }
        // A local command beside the gate answers from the session's own
        // facts, the gate kept: a slash line is never the gate's answer.
        if let Some(command) = local_command_of(line) {
            self.pending_gate = Some(gate);
            return self.answer_locally(command);
        }
        if line.trim().is_empty() {
            self.pending_gate = Some(gate);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::EmptyAnswer,
                "the gate needs an answer — nothing answers for you",
            ));
        }
        // A confirm gate takes its protocol tokens and nothing else: any
        // other line is open language — a question about the gate explains
        // it, a change belongs to the workflow (« no », then the change);
        // neither answers the gate. Authority never comes from a reading.
        if gate.mode == "confirm" && !is_gate_token(line) {
            if let Err(refusal) = self.admit_gate_money(line) {
                self.pending_gate = Some(gate);
                return refusal;
            }
            if self.money_blocks_cognition() {
                self.pending_gate = Some(gate);
                return self.cognition_money_refusal();
            }
            let decision = self.classify(crate::turn::SessionPhase::GatePending, line);
            let text = match decision.act {
                crate::turn::TurnAct::Modify | crate::turn::TurnAct::Mixed => {
                    "the gate takes a yes or a no — a change belongs to the workflow: answer `no`, then say the change".to_owned()
                }
                _ => format!(
                    "{}\n  the gate still waits · `yes` or `no` answers it",
                    aside::explain_gate(&gate, &self.snapshot.root)
                ),
            };
            self.pending_gate = Some(gate);
            return TurnOutcome::Aside(text);
        }
        self.answered = Some(GateId::new(&gate.trace, &gate.task));
        let answer = gate.answer_arg(line);
        self.finish_gate_money();
        self.remember("(gate)", &format!("{} answered: {answer}", gate.task));
        TurnOutcome::ResumeRequested {
            workflow: gate.workflow,
            trace: gate.trace,
            answer,
        }
    }

    fn observation_line(&mut self, exit: u8, trace: Option<&Path>, with_produced: bool) -> String {
        let meaning = match exit {
            0 => "succeeded",
            1 => "the workflow failed",
            2 => "refused before running (findings)",
            3 => "refused by the environment",
            4 => {
                "paused for a human answer — `nika run <file> --resume <trace> --answer <task>=<value>` continues it"
            }
            130 => "interrupted before it finished (Ctrl+C) — the trace shows what ran",
            _ => "ended with an unknown code",
        };
        let line = match trace {
            Some(t) => format!(
                "run observed · exit {exit} · {meaning} · trace `{}`",
                t.display()
            ),
            None => format!("run observed · exit {exit} · {meaning}"),
        };
        let line = match self.trace_hygiene_note() {
            Some(note) => format!("{line}\n  {note}"),
            None => line,
        };
        let line = match (exit, self.produced_line()) {
            (0, Some(produced)) if with_produced => format!("{line}\n  {produced}"),
            _ => line,
        };
        self.last_run = Some((exit, line.clone()));
        self.remember("(run)", &line);
        line
    }

    /// What a green run left behind: the files the workflow's own boundary
    /// lets it write (`permits.fs.write`, literal paths only) that exist
    /// under the root now, with their sizes. The boundary is the claim; the
    /// file on disk is the evidence; a glob is not a file.
    fn produced_line(&self) -> Option<String> {
        let workflow = self.last_workflow.as_ref()?;
        let root = &self.snapshot.root;
        let source = std::fs::read_to_string(root.join(workflow)).ok()?;
        let wf = nika_schema::parse(
            &source,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .ok()?;
        let writes = wf.permits.as_ref()?.value.fs.as_ref()?.write.clone();
        let mut produced = Vec::new();
        for path in writes {
            if path.contains(['*', '?', '[']) {
                continue;
            }
            let Ok(meta) = std::fs::metadata(root.join(&path)) else {
                continue;
            };
            if meta.is_file() {
                produced.push(format!("{path} ({})", human_size(meta.len())));
            }
        }
        (!produced.is_empty()).then(|| format!("produced · {}", produced.join(" · ")))
    }

    /// In a git repository whose `.gitignore` does not keep `.nika/traces/`
    /// out, a run's trace (model outputs · file contents · 0600) would be
    /// one `git add` away from a commit: say so once per run.
    fn trace_hygiene_note(&self) -> Option<String> {
        let root = self.snapshot.git_root.as_ref()?;
        let ignored = std::fs::read_to_string(root.join(".gitignore"))
            .map(|text| {
                text.lines().any(|l| {
                    l.trim().contains(".nika/traces") || l.trim() == ".nika" || l.trim() == ".nika/"
                })
            })
            .unwrap_or(false);
        (!ignored).then(|| {
            "runs write `.nika/traces/` (model outputs · file contents · mode 0600) — not ignored by git here · `nika init` adds the line, or add `.nika/traces/` to `.gitignore`".to_owned()
        })
    }

    fn intelligence_card(&self) -> String {
        format!(
            "{}\n{}",
            self.intelligence_line(),
            self.intelligence
                .why
                .as_deref()
                .map_or(String::new(), |w| format!("  ⚠ {w}\n"))
        )
    }

    fn remember(&mut self, user: &str, assistant: &str) {
        self.recent.push((user.to_owned(), assistant.to_owned()));
        if self.recent.len() > RECENT_TURNS {
            self.recent.remove(0);
        }
    }
}

/// Session's execution fallback when neither explicit money nor a project
/// default was supplied. This does not meter conversation or authoring.
const DEFAULT_CEILING_USD: f64 = 0.25;

/// The door out, from any prompt.
fn is_quit(answer: &str) -> bool {
    matches!(answer.trim(), "/quit" | "/exit")
}

fn named_files(input: &str) -> Vec<String> {
    input
        .split(|c: char| {
            c.is_whitespace()
                || c == '`'
                || c == '"'
                || c == '\''
                || c == ','
                || c == '?'
                || c == '('
                || c == ')'
        })
        .filter(|t| nika_source::is_canonical_program_file_name(t) || *t == "nika.yaml")
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod answer_tests;
#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod authoring_tests;
#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod choice_tests;
#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod restore_tests;
#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod route_tests;
#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;

#[cfg(test)]
pub(crate) mod inference_tests;
