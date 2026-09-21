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

mod authoring;
mod durable;
#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod durable_tests;
mod history;

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
}

/// The help card — the few survivors, and the law that everything
/// meaningful is reachable in words.
pub const HELP: &str = "text                 describe work to build (« read ./notes, draft a summary, write ./out/summary.md ») · Nika compiles it,
                     asks what it cannot invent, shows the workflow, and writes it only when you say yes · consent is never a run
run …                run the workflow you accepted, or one you name (« run brief.nika with a ceiling of 0.05 ») · a paused run asks you
text                 ask, in words · these answer from the engine, no AI asked: your workflows · a file's verdict (« is X valid »)
                     · the builtins · the providers · an example or template for a job · a code (« explain NIKA-… »)
                     · what Nika calls a node, step, trigger, secret, action · the rest goes to your chosen intelligence, in words
/intelligence        the AI this session reasons with · asks the first screen again, the next line is your answer
/show                while a proposal waits: print its exact bytes (the review shows the boundary)
/help                this card
/quit                close the session
Name a workflow file in your question to let the session read it (only files under the root are ever read).";

/// How many recent turns ride the next prompt.
const RECENT_TURNS: usize = 8;

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

/// How a door builds the reasoner for a resolved choice.
pub type ReasonerFactory = Box<dyn Fn(&ResolvedSessionIntelligence) -> Box<dyn SessionReasoner>>;

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
    pending_gate: Option<PendingGate>,
    decided: Option<ProposalId>,
    answered: Option<GateId>,
    last_run: Option<(u8, String)>,
    last_workflow: Option<PathBuf>,
    /// The authoring round whose question the next line answers.
    authoring: Option<AuthoringRound>,
    /// The cognition the compiler may use, derived from the reasoner.
    seat: AuthoringSeat,
    /// Where a truthful progress line goes while the compiler works
    /// (presentation only: it never carries workflow meaning).
    progress: Option<ProgressHook>,
    /// A run request waiting on the values of the workflow's declared inputs.
    run_inputs: Option<authoring::RunInputs>,
}

/// A door's sink for progress lines (« Working through this workflow… »).
pub type ProgressHook = Box<dyn Fn(&str)>;

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
            pending_gate: None,
            decided: None,
            answered: None,
            last_run: None,
            last_workflow: None,
            authoring: None,
            seat: AuthoringSeat::Deterministic { why: None },
            progress: None,
            run_inputs: None,
        };
        session.refresh_seat();
        session
    }

    /// Where progress lines go while the compiler works under a seat: a
    /// door prints them; a remote host projects them. Presentation only.
    pub fn on_progress(&mut self, hook: ProgressHook) {
        self.progress = Some(hook);
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
        session
    }

    /// The answer to the first screen asked in-session (`/intelligence`):
    /// the choice is judged, kept under the home when one exists, and the
    /// reasoner rebuilt — refused with its fix when this machine cannot
    /// serve it, and the previous choice stands.
    fn choose_unrecorded(&mut self, answer: &str) -> TurnOutcome {
        let (Some(census), Some(factory)) = (&self.census, &self.factory) else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "this session cannot re-choose its intelligence — quit and open `nika` again",
            ));
        };
        let pref = match census.choose(answer) {
            Ok(pref) => pref,
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
        TurnOutcome::Facts(format!(
            "{} · {kept}\n  {}",
            self.intelligence_line(),
            self.seat.line()
        ))
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

    /// The banner a terminal prints at open: the root, the path, the locus.
    #[must_use]
    pub fn banner(&self) -> String {
        let readiness = match &self.intelligence.why {
            Some(why) => format!("\n  ⚠ {why}"),
            None => String::new(),
        };
        format!(
            "nika · session\n  root: {}\n  {}{readiness}\n  {}\n  /help for the card · /quit to close",
            self.snapshot.root.display(),
            self.intelligence_line(),
            self.seat.line()
        )
    }

    /// One turn.
    fn turn_unrecorded(&mut self, input: &str) -> TurnOutcome {
        // A new turn discards a pending proposal: consent is the NEXT line
        // and nothing else (the door routes that line to `consent`).
        self.pending = None;
        let input = input.trim();
        match input {
            "/quit" | "/exit" => return TurnOutcome::Quit,
            "/help" => return TurnOutcome::Help(HELP.to_owned()),
            "/intelligence" => {
                return match &self.census {
                    Some(census) => TurnOutcome::Ask(format!(
                        "{}\n{}",
                        self.intelligence_card(),
                        census.first_screen()
                    )),
                    None => TurnOutcome::Facts(self.intelligence_card()),
                };
            }
            _ => {}
        }
        // An open authoring question owns the next line — before any
        // fact, digit or model reads it (`./notes` answers « which folder »).
        if self.authoring.is_some() {
            return self.answer_question_unrecorded(input);
        }
        // A run waiting on a declared input owns the next line the same way.
        if self.run_inputs.is_some() {
            return self.answer_input_unrecorded(input);
        }
        if input.is_empty() {
            return TurnOutcome::Facts(String::new());
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
        if let Some(outcome) = self.run_turn(input) {
            return outcome;
        }
        if self.intent.goal.is_none() {
            self.intent.goal = Some(input.to_owned());
        }
        // Work to build reaches the ONE compiler; only a line that reads as
        // no work at all goes to the conversation.
        if let Some(outcome) = self.author_unrecorded(input) {
            return outcome;
        }
        if !self.intelligence.ready {
            let why =
                self.intelligence.why.clone().unwrap_or_else(|| {
                    "this session has no conversational intelligence".to_owned()
                });
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::NoIntelligence,
                format!(
                    "{why} — the facts still answer (workflows · builtins · providers · check · explain) · describe work to build and Nika compiles it"
                ),
            ));
        }
        let named = named_files(input);
        let bundle = self.broker.bundle(
            &self.snapshot,
            self.intent.goal.as_deref(),
            &named,
            &self.intelligence.locus.line(),
        );
        let prompt = ContextBroker::prompt(&bundle, &self.recent, input);
        match self.reasoner.reason(&prompt) {
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
            Err(e) => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::IntelligenceRefused,
                format!(
                    "{e} — the choice stands (`/intelligence` to change it); nothing was substituted"
                ),
            )),
        }
    }

    /// The human's answer to a proposal: `yes` lands the set (every
    /// witness checked before the first write · atomic writes · nothing
    /// outside the set), the real check follows every workflow written,
    /// and a run the human asked for is requested ONLY when that check is
    /// clean. Anything else discards the set; nothing is written.
    fn consent_unrecorded(&mut self, answer: &str) -> TurnOutcome {
        let Some(set) = self.pending.take() else {
            return TurnOutcome::Refusal(self.nothing_pending());
        };
        let id = ProposalId::of(&set.preview());
        // The exact bytes, on request, the proposal held: consent stays a yes.
        if matches!(answer.trim(), "/show" | "show") {
            let preview = set.preview();
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
            // Anything that is neither a yes nor a no is a question about the
            // proposal: answered from the set itself (what it reaches) or from
            // the engine, and the proposal HELD — a newcomer who asks « what is
            // permits? » at the prompt must not lose the file.
            let lower = answer.to_lowercase();
            let about_effects = [
                "read",
                "write",
                "network",
                "reach",
                "when it runs",
                "effect",
                "spend",
                "cost",
                "touch",
            ]
            .iter()
            .any(|w| lower.contains(w));
            let text = if about_effects {
                set.effects_fact()
            } else {
                crate::facts::answer(answer, &self.snapshot, &self.snapshot.root).unwrap_or_else(|| {
                    "that line is not a consent — ask about the proposal (what it reads and writes · its check · a word) or answer".to_owned()
                })
            };
            self.pending = Some(set);
            return TurnOutcome::Held {
                id,
                preview: format!(
                    "{text}\n(the proposal still waits · `yes` applies it · `no` discards it)"
                ),
            };
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
        // an explicit line, never this consent.
        if let Some(first) = set.workflows().into_iter().next() {
            self.last_workflow = Some(first);
        }
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
            None if all_clean => {
                report.push_str("\n  say « run it » to run it once (a ceiling is announced first)");
                TurnOutcome::Facts(report)
            }
            None => TurnOutcome::Facts(report),
        }
    }

    /// The proposal waiting for a consent, when one is (its identity: the
    /// witness of the preview's bytes).
    #[must_use]
    pub fn pending_proposal(&self) -> Option<ProposalId> {
        self.pending
            .as_ref()
            .map(|set| ProposalId::of(&set.preview()))
    }

    /// A consent that names the proposal it answers — a remote host, a
    /// reconnect (ADR-133): refused as stale when another proposal waits,
    /// as already consumed when that proposal was decided, as the wrong
    /// state when none is pending. Never applied twice.
    pub fn consent_to(&mut self, id: &ProposalId, answer: &str) -> TurnOutcome {
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
                format!("the proposal {id} was already decided — its effect happened once"),
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
        let line = self.observation_line(exit, trace);
        if exit == 4
            && let (Some(trace), Some(workflow)) = (trace, self.last_workflow.clone())
            && let Some(gate) = PendingGate::from_trace(&workflow, trace)
        {
            let question = gate.question();
            let id = GateId::new(&gate.trace, &gate.task);
            self.pending_gate = Some(gate);
            return TurnOutcome::GateAsk {
                id,
                question: format!("{line}\n{question}"),
            };
        }
        TurnOutcome::Facts(line)
    }

    /// The human's answer to a pending gate: the resume the door runs.
    /// Nothing answers for the human; an empty line is not an answer.
    fn answer_gate_unrecorded(&mut self, line: &str) -> TurnOutcome {
        let Some(gate) = self.pending_gate.take() else {
            return TurnOutcome::Refusal(self.no_gate_waiting());
        };
        if line.trim().is_empty() {
            self.pending_gate = Some(gate);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::EmptyAnswer,
                "the gate needs an answer — nothing answers for you",
            ));
        }
        self.answered = Some(GateId::new(&gate.trace, &gate.task));
        let answer = gate.answer_arg(line);
        self.remember("(gate)", &format!("{} answered: {answer}", gate.task));
        TurnOutcome::ResumeRequested {
            workflow: gate.workflow,
            trace: gate.trace,
            answer,
        }
    }

    fn observation_line(&mut self, exit: u8, trace: Option<&Path>) -> String {
        let meaning = match exit {
            0 => "succeeded",
            1 => "the workflow failed",
            2 => "refused before running (findings)",
            3 => "refused by the environment",
            4 => {
                "paused for a human answer — `nika run <file> --resume <trace> --answer <task>=<value>` continues it"
            }
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
            (0, Some(produced)) => format!("{line}\n  {produced}"),
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

/// The workflow or project files an input names.
/// The ceiling a run from the session is announced with when the project
/// file declares none (the CLI's own default).
const DEFAULT_CEILING_USD: f64 = 0.25;

/// The ceiling the human named in their own words — « with a ceiling of
/// 0.05 » · « cap 0.10 » · « max cost 1 » · « $0.05 » · `--max-cost-usd 0.05`
/// — or none.
fn ceiling_in(input: &str) -> Option<f64> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    for (i, raw) in tokens.iter().enumerate() {
        let token = raw.trim_matches(|c: char| matches!(c, ',' | '(' | ')'));
        let token = token
            .strip_suffix('.')
            .filter(|t| t.parse::<f64>().is_ok())
            .unwrap_or(token);
        if let Some(dollars) = token.strip_prefix('$')
            && let Ok(v) = dollars.parse::<f64>()
        {
            return Some(v);
        }
        if let Some(v) = token
            .strip_prefix("--max-cost-usd=")
            .and_then(|v| v.parse::<f64>().ok())
        {
            return Some(v);
        }
        let previous = i.checked_sub(1).map(|p| tokens[p].to_lowercase());
        let after_a_ceiling_word = previous.as_deref().is_some_and(|p| {
            matches!(
                p,
                "ceiling"
                    | "cap"
                    | "cost"
                    | "usd"
                    | "--max-cost-usd"
                    | "of"
                    | "to"
                    | "at"
                    | "under"
            )
        });
        if after_a_ceiling_word
            && let Ok(v) = token.trim_start_matches('$').parse::<f64>()
            && v >= 0.0
        {
            return Some(v);
        }
    }
    None
}

/// The refusal line: `no` in the few words a human types for it.
fn is_no(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "no" | "n" | "non" | "discard" | "cancel" | "drop" | "nope" | "stop"
    )
}

/// The consent line, and nothing else: `yes` in the few words a human
/// types for it.
fn is_yes(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "yes" | "y" | "apply" | "ok" | "oui" | "go" | "do it"
    )
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
mod authoring_tests;
#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
