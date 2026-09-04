// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session runtime — the loop a terminal drives: one turn in, one
//! outcome out. A turn is a slash command, a Nika fact (answered from
//! the engine, zero tokens), or a free-text question the selected
//! intelligence reasons over through the broker's bundle and under the
//! guard's reading. No temporary workflow, no trace for a chat turn, no
//! hidden shell.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::broker::ContextBroker;
use crate::change::{
    PendingGate, ProjectChangeSet, RunRequest, check_on_disk, prose_outside_blocks,
};
use crate::guard::KnownWorld;
use crate::intelligence::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference,
};
use crate::outcome::{GateId, ProposalId, Refusal, RefusalClass};
use crate::reasoner::{ReasonError, SessionReasoner};
use crate::snapshot::ProjectSnapshot;

/// The durable half of the conversation — decisions, not chat.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// The reply proposed a change to the project: the exact preview of
    /// the bytes the apply would land. The NEXT line is the human's
    /// consent ([`SessionRuntime::consent`]); nothing is written before it.
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
pub const HELP: &str = "text                 ask, in words · these answer from the engine, no AI asked: your workflows · a file's verdict (« is X valid »)
                     · the builtins · the providers · an example or template for a job · a code (« explain NIKA-… »)
                     · what Nika calls a node, step, trigger, secret, action · the rest goes to your chosen intelligence
/intelligence        the AI this session reasons with · asks the first screen again, the next line is your answer
/help                this card
/quit                close the session
Name a workflow file in your question to let the session read it (only files under the root are ever read).";

/// How many recent turns ride the next prompt.
const RECENT_TURNS: usize = 8;

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
    census: Option<IntelligenceCensus>,
    home: Option<PathBuf>,
    factory: Option<ReasonerFactory>,
    pending: Option<ProjectChangeSet>,
    pending_gate: Option<PendingGate>,
    decided: Option<ProposalId>,
    answered: Option<GateId>,
    last_run: Option<(u8, String)>,
    last_workflow: Option<PathBuf>,
}

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
        Self {
            snapshot,
            intelligence,
            intent: IntentDraft {
                goal: None,
                decisions: Vec::new(),
                unresolved: Vec::new(),
            },
            reasoner,
            broker,
            known,
            recent: Vec::new(),
            census: None,
            home: None,
            factory: None,
            pending: None,
            pending_gate: None,
            decided: None,
            answered: None,
            last_run: None,
            last_workflow: None,
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
    pub fn choose(&mut self, answer: &str) -> TurnOutcome {
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
        TurnOutcome::Facts(format!("{} · {kept}", self.intelligence_line()))
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
            "nika · session\n  root: {}\n  {}{readiness}\n  /help for the card · /quit to close",
            self.snapshot.root.display(),
            self.intelligence_line()
        )
    }

    /// One turn.
    pub fn turn(&mut self, input: &str) -> TurnOutcome {
        // A new turn discards a pending proposal: consent is the NEXT line
        // and nothing else (the door routes that line to `consent`).
        self.pending = None;
        let input = input.trim();
        match input {
            "" => return TurnOutcome::Facts(String::new()),
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
        if !self.intelligence.ready {
            let why =
                self.intelligence.why.clone().unwrap_or_else(|| {
                    "this session has no conversational intelligence".to_owned()
                });
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::NoIntelligence,
                format!(
                    "{why} — the facts still answer (workflows · builtins · providers · check · explain)"
                ),
            ));
        }
        if self.intent.goal.is_none() {
            self.intent.goal = Some(input.to_owned());
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
            Ok(reply) => {
                let findings = self.known.audit(&reply.text);
                let shown = KnownWorld::correct(&reply.text, &findings);
                self.remember(input, &shown);
                self.propose_or_reply(input, &shown, &named)
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

    /// A reply that carries a file becomes a proposal: the typed change
    /// set is built from the exact bytes, previewed, and held for the
    /// human's consent. A reply without one is a reply.
    fn propose_or_reply(&mut self, input: &str, shown: &str, named: &[String]) -> TurnOutcome {
        let run = wants_run(input).then(|| RunRequest {
            workflow: PathBuf::new(),
            vars: Vec::new(),
            max_cost_usd: ceiling_in(input)
                .or(self.snapshot.ceiling)
                .unwrap_or(DEFAULT_CEILING_USD),
        });
        match ProjectChangeSet::from_reply(&self.snapshot.root, input, shown, named, run) {
            Ok(Some(set)) => {
                let bytes = set.preview();
                let id = ProposalId::of(&bytes);
                let preview = format!("{}{}", prose_outside_blocks(shown), bytes);
                self.pending = Some(set);
                TurnOutcome::Proposal { id, preview }
            }
            Ok(None) => TurnOutcome::Reply(shown.to_owned()),
            Err(e) => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::NotAllowed,
                format!("the reply proposed a file the session may not write — {e}"),
            )),
        }
    }

    /// The human's answer to a proposal: `yes` lands the set (every
    /// witness checked before the first write · atomic writes · nothing
    /// outside the set), the real check follows every workflow written,
    /// and a run the human asked for is requested ONLY when that check is
    /// clean. Anything else discards the set; nothing is written.
    pub fn consent(&mut self, answer: &str) -> TurnOutcome {
        let Some(set) = self.pending.take() else {
            return TurnOutcome::Refusal(self.nothing_pending());
        };
        let id = ProposalId::of(&set.preview());
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
        self.decided = Some(id);
        let applied = match set.apply() {
            Ok(applied) => applied,
            Err(e) => return TurnOutcome::Refusal(Refusal::from_change(&e)),
        };
        let written: Vec<String> = applied
            .written
            .iter()
            .map(|p| format!("`{}`", p.display()))
            .collect();
        let mut report = format!("applied · wrote {}", written.join(" · "));
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
            for h in &audit.hints {
                let _ = write!(report, "\n    · hint · {h}");
            }
        }
        self.snapshot = ProjectSnapshot::observe(&self.snapshot.cwd);
        self.remember("(consent)", &report);
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
            None => TurnOutcome::Refusal(self.nothing_pending()),
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
            None => TurnOutcome::Refusal(self.no_gate_waiting()),
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
    pub fn observe_run(&mut self, exit: u8, trace: Option<&Path>) -> TurnOutcome {
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
    pub fn answer_gate(&mut self, line: &str) -> TurnOutcome {
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
        self.last_run = Some((exit, line.clone()));
        self.remember("(run)", &line);
        line
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

/// « create it and run it » · « then run it once » — the human asked for
/// one run with the change.
fn wants_run(input: &str) -> bool {
    let lower = input.to_lowercase();
    [
        "and run",
        "then run",
        "run it",
        "run once",
        "run them",
        "run this",
        "and execute",
    ]
    .iter()
    .any(|p| lower.contains(p))
}

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
        .filter(|t| t.ends_with(".nika.yaml") || *t == "nika.yaml")
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::intelligence::{DataLocus, IntelligenceKind};
    use crate::reasoner::{NoReasoner, Reply, ScriptedReasoner};

    /// A seat reasoner whose name is the seat itself, as the harness one is.
    struct Seat(&'static str);

    impl SessionReasoner for Seat {
        fn name(&self) -> String {
            self.0.to_owned()
        }

        fn reason(&mut self, _prompt: &str) -> Result<Reply, crate::reasoner::ReasonError> {
            Ok(Reply {
                text: "seated".to_owned(),
                usage_observed: false,
            })
        }
    }

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("alpha.nika.yaml"),
            "nika: alpha\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: \"sk-live-ABCDEFGH123456\", max_tokens: 10 }\n",
        )
        .expect("a");
        dir
    }

    fn ready(kind: IntelligenceKind, locus: DataLocus) -> ResolvedSessionIntelligence {
        ResolvedSessionIntelligence {
            kind,
            model: None,
            locus,
            ready: true,
            why: None,
        }
    }

    /// A chat turn never writes a temp workflow nor a trace: the tree is
    /// untouched after three turns.
    #[test]
    fn a_turn_writes_nothing() {
        let dir = tree();
        let reasoner = ScriptedReasoner::new(vec!["Sure.".to_owned()]);
        let mut s = SessionRuntime::open(
            dir.path(),
            ready(
                IntelligenceKind::Local {
                    provider: "ollama".to_owned(),
                },
                DataLocus::Local,
            ),
            Box::new(reasoner),
        );
        let before: Vec<_> = std::fs::read_dir(dir.path())
            .expect("dir")
            .flatten()
            .map(|e| e.path())
            .collect();
        let _ = s.turn("what workflows are here?");
        let _ = s.turn("explain what alpha.nika.yaml does");
        assert!(
            matches!(s.turn("/help"), TurnOutcome::Help(ref card) if card.contains("no AI asked") && card.contains("what Nika calls")),
            "the card names the shapes that answer without a model"
        );
        let after: Vec<_> = std::fs::read_dir(dir.path())
            .expect("dir")
            .flatten()
            .map(|e| e.path())
            .collect();
        assert_eq!(before, after, "no temp file, no .nika/ tree");
        assert!(!dir.path().join(".nika").exists());
    }

    /// The reasoner receives only the bundle: the grounding, the facts,
    /// the file the human named (redacted), the turn — never the
    /// environment, never a file the human did not name.
    #[test]
    fn the_reasoner_receives_only_the_bundle() {
        let dir = tree();
        std::fs::write(
            dir.path().join("secret.nika.yaml"),
            "nika: hidden\ntasks: {}\n",
        )
        .expect("hidden");
        let mut s = SessionRuntime::open(
            dir.path(),
            ready(
                IntelligenceKind::Api {
                    provider: "mistral".to_owned(),
                },
                DataLocus::Metered {
                    provider: "mistral".to_owned(),
                },
            ),
            Box::new(ScriptedReasoner::new(vec!["It reads a file.".to_owned()])),
        );
        let out = s.turn("what does alpha.nika.yaml do?");
        assert!(
            matches!(out, TurnOutcome::Reply(ref t) if t.contains("It reads a file.")),
            "{out:?}"
        );
        // reach the scripted reasoner's record through a second session? the
        // reasoner is boxed: assert on the prompt shape via a fresh reasoner
        let mut probe = ScriptedReasoner::new(vec!["x".to_owned()]);
        let snapshot = ProjectSnapshot::observe(dir.path());
        let broker = ContextBroker::new(snapshot.root.clone());
        let bundle = broker.bundle(
            &snapshot,
            Some("goal"),
            &["alpha.nika.yaml".to_owned()],
            "metered",
        );
        let prompt = ContextBroker::prompt(&bundle, &[], "what does alpha.nika.yaml do?");
        let _ = probe.reason(&prompt);
        let seen = &probe.seen[0];
        assert!(
            seen.contains("Never invent Nika syntax"),
            "the identity core rides"
        );
        assert!(
            seen.contains("File `alpha.nika.yaml`"),
            "the named file rides"
        );
        assert!(
            !seen.contains("nika: hidden"),
            "an unnamed file never rides"
        );
        assert!(
            !seen.contains("sk-live-ABCDEFGH123456"),
            "the secret never rides"
        );
        assert!(
            !seen.contains("PATH=") && !seen.contains("OPENAI_API_KEY=") && !seen.contains("HOME="),
            "the environment never rides"
        );
    }

    /// An invented workflow language in the reply is corrected before the
    /// human sees it; a claim of ignorance too.
    #[test]
    fn an_invented_grammar_is_corrected_before_the_human_sees_it() {
        let dir = tree();
        let invented = "Here is your workflow:\n```yaml\nversion: 1\nsteps:\n  - fetch_internet: https://x\n```\nUse `nika:telegram` to notify. I don't know Nika's exact syntax.";
        let mut s = SessionRuntime::open(
            dir.path(),
            ready(
                IntelligenceKind::Local {
                    provider: "ollama".to_owned(),
                },
                DataLocus::Local,
            ),
            Box::new(ScriptedReasoner::new(vec![invented.to_owned()])),
        );
        let out = s.turn("make me a workflow that fetches a site and notifies telegram");
        let TurnOutcome::Reply(text) = out else {
            panic!("{out:?}");
        };
        assert!(
            text.contains("grounding (the installed engine disagrees"),
            "{text}"
        );
        assert!(text.contains("`steps` is not a workflow field"), "{text}");
        assert!(text.contains("`nika:telegram` is not a builtin"), "{text}");
        assert!(
            text.contains("the installed engine's canon is available"),
            "{text}"
        );
        assert_eq!(
            s.intent.goal.as_deref(),
            Some("make me a workflow that fetches a site and notifies telegram")
        );
    }

    /// Without conversational intelligence the facts still answer and a
    /// free-text turn is refused with the fix, never routed elsewhere.
    #[test]
    fn without_intelligence_the_facts_stay_and_free_text_is_refused() {
        let dir = tree();
        let mut s = SessionRuntime::open(
            dir.path(),
            ready(IntelligenceKind::None, DataLocus::None),
            Box::new(NoReasoner),
        );
        assert!(
            matches!(s.turn("which builtins exist?"), TurnOutcome::Facts(ref t) if t.contains("nika:read"))
        );
        assert!(
            matches!(s.turn("write a haiku"), TurnOutcome::Refusal(ref r) if r.text.contains("no conversational intelligence"))
        );
        assert!(matches!(s.turn("/quit"), TurnOutcome::Quit));
        assert!(s.banner().contains("no conversational AI"));
        assert_eq!(
            s.banner().matches("no conversational AI").count(),
            1,
            "the path is named once: {}",
            s.banner()
        );
    }

    const PROPOSED: &str = "Here it is.\n\n```yaml path=daily.nika.yaml\nnika: daily\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\noutputs:\n  said: ${{ tasks.t.output }}\n```\n";

    fn ready_with(dir: &Path, replies: Vec<&str>) -> SessionRuntime {
        let seated = ResolvedSessionIntelligence {
            kind: IntelligenceKind::Harness {
                seat: "codex".to_owned(),
            },
            model: None,
            locus: DataLocus::Remote {
                product: "codex".to_owned(),
            },
            ready: true,
            why: None,
        };
        SessionRuntime::open(
            dir,
            seated,
            Box::new(ScriptedReasoner::new(
                replies.into_iter().map(str::to_owned).collect(),
            )),
        )
    }

    /// A reply carrying a file is a proposal: nothing is written until the
    /// consent line says yes; `no` discards; the next yes lands the exact
    /// bytes and the real check follows; a new turn discards a pending set.
    #[test]
    fn a_proposal_lands_only_on_consent() {
        let dir = tree();
        let mut s = ready_with(dir.path(), vec![PROPOSED, PROPOSED, PROPOSED]);
        let TurnOutcome::Proposal { preview, .. } = s.turn("write me a daily digest workflow")
        else {
            panic!("a proposal");
        };
        assert!(
            preview.starts_with("Here it is.\n\n"),
            "the prose above: {preview}"
        );
        assert!(
            preview.contains("proposed change · write me a daily digest workflow"),
            "the header names this turn's request: {preview}"
        );
        assert!(
            preview.contains("creates `daily.nika.yaml`") && preview.contains("clean ✔"),
            "{preview}"
        );
        assert!(
            !dir.path().join("daily.nika.yaml").exists(),
            "nothing written before consent"
        );
        assert!(matches!(s.consent("no"), TurnOutcome::Facts(ref t) if t.contains("discarded")));
        assert!(
            matches!(s.turn("1"), TurnOutcome::Facts(ref t) if t.contains("already chosen")),
            "a bare digit is the first-screen reflex, never a message for the seat"
        );
        assert!(
            !dir.path().join("daily.nika.yaml").exists(),
            "no means nothing"
        );
        assert!(
            matches!(s.consent("yes"), TurnOutcome::Refusal(ref r) if r.text.contains("nothing is pending"))
        );
        assert!(matches!(
            s.turn("again please"),
            TurnOutcome::Proposal { .. }
        ));
        assert!(
            matches!(s.turn("what workflows are here?"), TurnOutcome::Facts(_)),
            "a new turn"
        );
        assert!(
            matches!(s.consent("yes"), TurnOutcome::Refusal(_)),
            "the new turn discarded the proposal"
        );
        assert!(matches!(s.turn("once more"), TurnOutcome::Proposal { .. }));
        let TurnOutcome::Facts(report) = s.consent("yes") else {
            panic!("applied");
        };
        assert!(
            report.contains("applied · wrote `daily.nika.yaml`") && report.contains("clean ✔"),
            "{report}"
        );
        let on_disk = std::fs::read_to_string(dir.path().join("daily.nika.yaml")).expect("landed");
        assert!(
            on_disk.starts_with("nika: daily\n") && on_disk.ends_with("${{ tasks.t.output }}\n"),
            "exact bytes"
        );
        assert!(
            s.snapshot
                .workflows
                .iter()
                .any(|w| w.path.ends_with("daily.nika.yaml")),
            "the snapshot sees it"
        );
    }

    /// A question at the consent prompt is answered and the proposal held;
    /// the next `yes` still lands it.
    #[test]
    fn a_question_at_the_consent_prompt_holds_the_proposal() {
        let dir = tree();
        let mut s = ready_with(dir.path(), vec![PROPOSED]);
        assert!(matches!(
            s.turn("write me a daily digest"),
            TurnOutcome::Proposal { .. }
        ));
        let TurnOutcome::Held { preview: text, .. } = s.consent("what is permits?") else {
            panic!("held");
        };
        assert!(text.contains("boundary"), "{text}");
        let TurnOutcome::Held { preview: text, .. } =
            s.consent("what will this read and write when it runs?")
        else {
            panic!("held");
        };
        assert!(
            text.contains("when it runs:") && text.contains("model mock/echo"),
            "the set's own effects: {text}"
        );
        let TurnOutcome::Held { preview: text, .. } = s.consent("hmm") else {
            panic!("held");
        };
        assert!(
            text.contains("not a consent") && text.contains("still waits"),
            "{text}"
        );
        assert!(!dir.path().join("daily.nika.yaml").exists());
        assert!(matches!(s.consent("yes"), TurnOutcome::Facts(ref t) if t.contains("applied")));
        assert!(dir.path().join("daily.nika.yaml").exists());
    }

    /// After a run in a git repository, the missing ignore line is named
    /// once; a `.gitignore` that keeps the traces out silences it.
    #[test]
    fn the_trace_hygiene_note_names_the_missing_ignore_line() {
        let dir = tree();
        std::fs::create_dir_all(dir.path().join(".git")).expect("a git root");
        let mut s = ready_with(dir.path(), vec![PROPOSED]);
        assert!(s.snapshot.git_root.is_some(), "a git root");
        let TurnOutcome::Facts(line) = s.observe_run(0, Some(Path::new(".nika/traces/t.ndjson")))
        else {
            panic!("an observation");
        };
        assert!(line.contains("not ignored by git here"), "{line}");
        std::fs::write(dir.path().join(".gitignore"), "target/\n.nika/traces/\n").expect("ignore");
        let TurnOutcome::Facts(line) = s.observe_run(0, Some(Path::new(".nika/traces/t.ndjson")))
        else {
            panic!("an observation");
        };
        assert!(!line.contains("not ignored"), "{line}");
    }

    /// « create and run it » requests the run ONLY after a clean on-disk
    /// check; findings stop it; the door's observation becomes a fact.
    #[test]
    fn a_run_is_requested_only_on_a_clean_check() {
        let dir = tree();
        let dirty = "```yaml path=bad.nika.yaml\nnika: bad\ntasks:\n  t:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n```\n";
        let mut s = ready_with(dir.path(), vec![PROPOSED, dirty]);
        assert!(
            matches!(s.turn("create a digest and run it once"), TurnOutcome::Proposal { ref preview, .. } if preview.contains("run `daily.nika.yaml` once"))
        );
        let TurnOutcome::RunRequested { report, run } = s.consent("yes") else {
            panic!("a clean check requests the run");
        };
        assert!(report.contains("clean ✔"), "{report}");
        assert_eq!(run.workflow, PathBuf::from("daily.nika.yaml"));
        assert!((run.max_cost_usd - DEFAULT_CEILING_USD).abs() < f64::EPSILON);
        assert_eq!(
            ceiling_in("create it and run it once with a ceiling of 0.05"),
            Some(0.05)
        );
        assert_eq!(ceiling_in("run it, cap $0.10 please"), Some(0.10));
        assert_eq!(ceiling_in("run it --max-cost-usd 1"), Some(1.0));
        assert_eq!(ceiling_in("run it --max-cost-usd=0.5"), Some(0.5));
        assert_eq!(ceiling_in("run it once"), None, "no number, the default");
        assert_eq!(
            ceiling_in("write 3 tasks and run it"),
            None,
            "a count is not a ceiling"
        );
        let TurnOutcome::Facts(observed) =
            s.observe_run(0, Some(Path::new(".nika/traces/t.ndjson")))
        else {
            panic!("an observation");
        };
        assert!(
            observed.contains("exit 0 · succeeded") && observed.contains("t.ndjson"),
            "{observed}"
        );
        assert!(matches!(
            s.turn("make a curl one and run it"),
            TurnOutcome::Proposal { .. }
        ));
        let TurnOutcome::Facts(report) = s.consent("yes") else {
            panic!("findings stop the run");
        };
        assert!(
            report.contains("findings ✖") && report.contains("the run was not started"),
            "{report}"
        );
        assert!(
            dir.path().join("bad.nika.yaml").exists(),
            "the bytes landed; the run did not start"
        );
    }

    /// A paused run returns to the session as a question; the human's line
    /// becomes the resume the door runs; nothing answers for them.
    #[test]
    fn a_paused_run_asks_the_human_and_the_answer_resumes_it() {
        let dir = tree();
        let mut s = ready_with(dir.path(), vec![PROPOSED]);
        assert!(matches!(
            s.turn("create a digest and run it"),
            TurnOutcome::Proposal { .. }
        ));
        assert!(matches!(s.consent("yes"), TurnOutcome::RunRequested { .. }));
        let store = dir.path().join(".nika").join("traces");
        std::fs::create_dir_all(&store).expect("store");
        let trace = store.join("paused.ndjson");
        std::fs::write(
            &trace,
            "{\"kind\":\"workflow_paused\",\"fields\":[{\"key\":\"task\",\"value\":\"gate\"},{\"key\":\"mode\",\"value\":\"confirm\"},{\"key\":\"message\",\"value\":\"Ship it?\"}]}\n",
        )
        .expect("trace");
        let TurnOutcome::GateAsk { question, .. } = s.observe_run(4, Some(&trace)) else {
            panic!("the gate is asked");
        };
        assert!(
            question.contains("paused for a human answer") && question.contains("Ship it?"),
            "{question}"
        );
        assert!(
            matches!(s.answer_gate(""), TurnOutcome::Refusal(ref r) if r.text.contains("nothing answers for you"))
        );
        let TurnOutcome::ResumeRequested {
            workflow,
            trace: t,
            answer,
        } = s.answer_gate("yes")
        else {
            panic!("the resume");
        };
        assert_eq!(workflow, PathBuf::from("daily.nika.yaml"));
        assert_eq!(t, trace);
        assert_eq!(answer, "gate=true");
        assert!(
            matches!(s.answer_gate("yes"), TurnOutcome::Refusal(_)),
            "answered once"
        );
        assert!(
            matches!(s.observe_run(0, Some(&trace)), TurnOutcome::Facts(_)),
            "a completed resume is a fact"
        );
    }

    /// The repair round: a dirty apply, then « fix it » — the reasoner's
    /// repaired file is a witnessed update, consented, checked clean.
    #[test]
    fn a_repair_round_updates_the_witnessed_file_to_clean() {
        let dir = tree();
        let dirty = "```yaml path=bad.nika.yaml\nnika: bad\ntasks:\n  t:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n```\n";
        let repaired = "Adding the boundary.\n\n```yaml path=bad.nika.yaml\nnika: bad\npermits: { exec: [\"curl\"], net: { http: [\"example.com\"] } }\ntasks:\n  t:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n```\n";
        let mut s = ready_with(dir.path(), vec![dirty, repaired]);
        assert!(matches!(
            s.turn("make a curl one"),
            TurnOutcome::Proposal { .. }
        ));
        let TurnOutcome::Facts(report) = s.consent("yes") else {
            panic!("applied");
        };
        assert!(
            report.contains("findings ✖") && report.contains("NIKA-AUTH-006"),
            "{report}"
        );
        let TurnOutcome::Proposal { preview, .. } = s.turn("fix it") else {
            panic!("a repair proposal");
        };
        assert!(
            preview.contains("replaces `bad.nika.yaml` whole") && preview.contains("clean ✔"),
            "{preview}"
        );
        let TurnOutcome::Facts(report) = s.consent("yes") else {
            panic!("applied");
        };
        assert!(
            report.contains("clean ✔") && !report.contains("findings ✖"),
            "{report}"
        );
        assert!(
            std::fs::read_to_string(dir.path().join("bad.nika.yaml"))
                .expect("landed")
                .contains("permits:")
        );
    }

    /// A reply proposing a path outside the root is refused before any preview.
    #[test]
    fn a_path_outside_the_root_is_refused_before_preview() {
        let dir = tree();
        let evil = "```yaml path=../evil.nika.yaml\nnika: evil\n```\n";
        let mut s = ready_with(dir.path(), vec![evil]);
        assert!(
            matches!(s.turn("write one"), TurnOutcome::Refusal(ref r) if r.text.contains("not a path inside the project root"))
        );
        assert!(
            matches!(s.consent("yes"), TurnOutcome::Refusal(_)),
            "nothing pending"
        );
    }

    /// `/intelligence` asks the first screen again in-session; the next
    /// line is the answer, kept under the home, the reasoner rebuilt; an
    /// unserved pick is refused and the previous choice stands.
    #[test]
    fn the_intelligence_can_be_rechosen_in_session() {
        let dir = tree();
        let home = tempfile::tempdir().expect("home");
        let census = IntelligenceCensus {
            seats: vec![crate::intelligence::SeatSeen {
                id: "codex".to_owned(),
                product_present: true,
                configured: true,
            }],
            api_keys: vec![],
            locals: vec![],
        };
        let pref = UserIntelligencePreference::new(IntelligenceKind::None, None);
        let factory: ReasonerFactory = Box::new(|resolved| match &resolved.kind {
            IntelligenceKind::None => Box::new(NoReasoner),
            _ => Box::new(ScriptedReasoner::new(vec!["seated".to_owned()])),
        });
        let mut s =
            SessionRuntime::open_with(dir.path(), census, &pref, Some(home.path()), factory);
        assert!(matches!(s.turn("hello"), TurnOutcome::Refusal(_)));
        let TurnOutcome::Ask(screen) = s.turn("/intelligence") else {
            panic!("asks");
        };
        assert!(screen.contains("Choose which AI"), "{screen}");
        assert!(
            matches!(s.choose("2"), TurnOutcome::Refusal(ref r) if r.text.contains("previous choice stands"))
        );
        assert!(
            matches!(s.turn("hello"), TurnOutcome::Refusal(_)),
            "still none"
        );
        assert!(
            matches!(s.choose("1"), TurnOutcome::Facts(ref t) if t.contains("codex") && t.contains("kept"))
        );
        assert!(matches!(s.turn("hello"), TurnOutcome::Reply(ref t) if t.contains("seated")));
        let back = UserIntelligencePreference::load(home.path()).expect("kept under the home");
        assert_eq!(
            back.kind,
            IntelligenceKind::Harness {
                seat: "codex".to_owned()
            }
        );
    }

    /// An explicit choice this machine cannot serve refuses every
    /// free-text turn with its fix — the facts still answer.
    #[test]
    fn an_unserved_choice_refuses_with_its_fix() {
        let dir = tree();
        let unserved = ResolvedSessionIntelligence {
            kind: IntelligenceKind::Harness {
                seat: "claude-code".to_owned(),
            },
            model: None,
            locus: DataLocus::Remote {
                product: "claude-code".to_owned(),
            },
            ready: false,
            why: Some("`claude-code` is not installed on this machine — install it".to_owned()),
        };
        let mut s = SessionRuntime::open(dir.path(), unserved, Box::new(Seat("claude-code")));
        assert!(s.banner().contains("⚠ `claude-code` is not installed"));
        assert!(
            s.banner().contains("intelligence: claude-code · uses")
                && !s.banner().contains("claude-code · claude-code"),
            "the seat is named once: {}",
            s.banner()
        );
        assert!(
            matches!(s.turn("hello"), TurnOutcome::Refusal(ref r) if r.text.contains("not installed"))
        );
        assert!(matches!(
            s.turn("what workflows are here?"),
            TurnOutcome::Facts(_)
        ));
    }

    /// A remote host judges by identity (ADR-133): a consent naming a
    /// proposal that is not the one waiting is stale and applies nothing;
    /// the same proposal consents once; the same gate answers once.
    #[test]
    fn a_remote_host_drives_the_machine_by_identity() {
        let dir = tree();
        let mut s = ready_with(dir.path(), vec![PROPOSED, PROPOSED]);
        let TurnOutcome::Proposal { id, .. } = s.turn("write me a daily digest workflow") else {
            panic!("a proposal");
        };
        assert_eq!(s.pending_proposal().as_ref(), Some(&id));
        let other = ProposalId::of("another preview");
        let TurnOutcome::Refusal(stale) = s.consent_to(&other, "yes") else {
            panic!("stale");
        };
        assert_eq!(stale.class, RefusalClass::StaleRevision, "{stale}");
        assert_eq!(
            s.pending_proposal().as_ref(),
            Some(&id),
            "a stale consent leaves the proposal waiting"
        );
        assert!(
            !dir.path().join("daily.nika.yaml").exists(),
            "nothing was applied"
        );
        assert!(matches!(
            s.consent_to(&id, "yes"),
            TurnOutcome::Facts(_) | TurnOutcome::RunRequested { .. }
        ));
        let TurnOutcome::Refusal(again) = s.consent_to(&id, "yes") else {
            panic!("consumed");
        };
        assert_eq!(again.class, RefusalClass::AlreadyConsumed, "{again}");
        assert!(s.pending_proposal().is_none());
        let TurnOutcome::Refusal(none) = s.consent("yes") else {
            panic!("nothing pending");
        };
        assert!(none.text.contains("nothing is pending"), "{none}");
        assert!(s.waiting_gate().is_none());
        let gate = GateId::new(Path::new("never.ndjson"), "gate");
        let TurnOutcome::Refusal(no_gate) = s.answer_gate_for(&gate, "yes") else {
            panic!("no gate");
        };
        assert_eq!(no_gate.class, RefusalClass::WrongState, "{no_gate}");
    }
}
