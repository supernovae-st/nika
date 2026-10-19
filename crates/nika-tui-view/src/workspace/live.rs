// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run the conversation drives, as the shell observed it while it ran:
//! the request (the workflow, and the look of the exact bytes at that path
//! when it was asked), then the typed frames of the child's stream folded by
//! the canonical [`RunView`]. Nothing here starts, answers or reads anything.
//!
//! One run is one leg: the first frame binds its execution, and a frame of
//! another execution never reaches the fold (it is counted, quarantined); a
//! resume is a new leg with its own identity. A frame after the settlement is
//! quarantined too, and a task that ends without having started is counted:
//! either leaves the stream incomplete. A runner that tells only the story
//! leaves the leg unfollowed, said so. The graph is the request's own
//! only when the run's start names those very bytes (`workflow_sha256`):
//! other bytes, no hash, or an unseen start leave it unbound, said so.
//!
//! Four things are kept apart, never folded into one verdict: the state the
//! settlement reports, whether the stream arrived whole (frames lost, lines
//! unread, a fold past its bound, no settlement), the evidence the producer
//! declared, and the proof, which only the trace's own verification judges.
//!
//! No frame outlives the turn that drove the leg: once that turn ended, a leg
//! still without its settlement never receives one, and neither it nor a task
//! it saw start reads as running ([`LiveRun::ended`]). Once settled, no task
//! runs either: one the fold still has running reads closed, ok when the
//! settlement counts every task ok ([`Closed`]).
//!
//! A leg kept from an earlier session shows what HOME history recorded of it
//! (its execution, workflow and exit as observed then): nothing replays, and
//! its proof is read again from its journal when that face opens.

use std::collections::{BTreeMap, BTreeSet};

use jiff::tz::TimeZone;
use nika_display::run_story::{
    ChildRun, Event, EventKind, Evidence, ExecutionId, RunFrame, RunSettlement, RunState, Settled,
    started_on,
};
use nika_display::state::{RunView, TaskRow, TaskState};
use nika_display::theme::Role;
use ratatui::text::{Line, Span};

use super::inspect::Inspected;
pub use acquired::{ChildRead, Fetched, Proven};
use nika_session::KeptRun;
use nika_session::world::{Reach, World};

pub mod acquired;
mod child;
mod faces;
mod task;
use super::aside::Verdict;
use super::pinned::Pinned;
use super::text::{fit_head, marks, twins, wrap};
use crate::lang::{Lang, Voice};
use crate::visual::icon::Icon;
use crate::visual::{role, state};
pub use faces::{FILES_READ, RunFace, Want};
pub use task::Pick;

/// The most events one leg's fold takes, and the most ids it remembers to
/// count a repeat; past it an event is only counted and the stream is
/// incomplete.
pub const EVENTS_KEPT: usize = 100_000;

/// Why a task the fold still has running does not read running: its leg
/// ended or settled without the task's own end frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closed {
    /// No end of it was received before the leg ended or settled.
    NoEnd,
    /// The settlement counts every task ok, this one too.
    SettledOk,
}

impl Closed {
    /// What its row, its card and its detail read.
    #[must_use]
    pub const fn words(self, detail: bool) -> &'static str {
        match (self, detail) {
            (Self::NoEnd, false) => "no end received",
            (Self::NoEnd, true) => "started, no end of it received before its run ended",
            (Self::SettledOk, false) => "settled ok",
            (Self::SettledOk, true) => {
                "succeeded, as the run's settlement counts every task ok; its own end frame was not received"
            }
        }
    }
}

/// Whether the graph shown is the one the run executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Binding {
    /// No frame of the run yet.
    Waiting,
    /// The run's start names the bytes shown when it was asked.
    Same,
    /// The run's start names other bytes: the file changed since the look.
    Other,
    /// The run's start names no source hash, or no bytes were read when asked.
    Unnamed,
    /// The run's first frame was not its start: the start was not observed.
    Unseen,
}

/// One leg of a run, as observed.
#[derive(Debug)]
#[non_exhaustive]
pub struct LiveRun {
    workflow: String,
    resume: bool,
    typed: bool,
    look: Option<Inspected>,
    execution: Option<ExecutionId>,
    binding: Binding,
    view: RunView,
    seen: BTreeSet<String>,
    events: usize,
    foreign: usize,
    repeated: usize,
    beyond: usize,
    late: usize,
    disorder: usize,
    dropped: usize,
    unread: usize,
    settled: Option<Box<Settled>>,
    /// The turn that drove the leg ended before its settlement arrived.
    over: bool,
    /// The run was refused before its first frame, and why, as the stream said it.
    refused: Option<String>,
    /// What the stream said of a frame it could not carry (a step's output over its bound),
    /// when it said it: the reason a journal it never kept was lost.
    drained: Option<&'static str>,
    /// The run this leg continued, as its start named it (`resumed_from`): known before its
    /// settlement says it again.
    started_from: Option<String>,
    /// The execution of the launch this leg continues (its first leg's), once found among the
    /// legs observed before it: the leg is named by when that launch began.
    launched: Option<ExecutionId>,
    /// The gates the person refused in this leg, by task (`approval_decided` with any decision
    /// but `allow`): a run they stopped there reads stopped, never succeeded, though its
    /// settlement says so.
    refused_gates: BTreeSet<String>,
    /// The zone whose clock names the leg: the machine's, where the person reads it.
    zone: TimeZone,
    fetched: Vec<Fetched>,
    proven: Option<Proven>,
    kept: Option<KeptRun>,
    /// Where the asked bytes reach, as the Session declared it (never observed).
    world: Option<World>,
    /// The last run of the same workflow observed here: what names it and its standing words.
    earlier: Option<(Naming, String)>,
    /// How many times what was acquired was forgotten: a reading's key.
    generation: u64,
    /// How many times each task's child relation changed (set, replaced or
    /// dropped): a child read is applied only for the relation it was
    /// opened on, never one that left and came back equal.
    relations: BTreeMap<String, u64>,
    revision: usize,
}

impl PartialEq for LiveRun {
    /// Two observations of a leg are the same when they name the same run of
    /// the same workflow at the same revision of what was observed.
    fn eq(&self, other: &Self) -> bool {
        self.workflow == other.workflow
            && self.resume == other.resume
            && self.execution == other.execution
            && self.revision == other.revision
    }
}

impl Eq for LiveRun {}

impl LiveRun {
    /// A run of `workflow` asked over the bytes `look` (a resume when
    /// `resume`), told by a runner that types its frames when `typed`.
    #[must_use]
    pub fn asked(workflow: String, resume: bool, typed: bool, look: Option<Inspected>) -> Self {
        Self {
            workflow,
            resume,
            typed,
            look,
            execution: None,
            binding: Binding::Waiting,
            view: RunView::new(),
            seen: BTreeSet::new(),
            events: 0,
            foreign: 0,
            repeated: 0,
            beyond: 0,
            late: 0,
            disorder: 0,
            dropped: 0,
            unread: 0,
            settled: None,
            over: false,
            refused: None,
            drained: None,
            started_from: None,
            launched: None,
            refused_gates: BTreeSet::new(),
            zone: TimeZone::UTC,
            fetched: Vec::new(),
            proven: None,
            kept: None,
            world: None,
            earlier: None,
            generation: 0,
            relations: BTreeMap::new(),
            revision: 0,
        }
    }

    /// The same leg, with where its bytes reach as the Session declared it.
    #[must_use]
    pub fn reaching(mut self, world: Option<World>) -> Self {
        self.world = world;
        self
    }

    /// The same leg, after `earlier`: the last run of the same workflow observed here.
    #[must_use]
    pub fn after(mut self, earlier: Option<(Naming, String)>) -> Self {
        self.earlier = earlier;
        self
    }

    /// The same leg, named on the clock of `zone` (the machine's): UTC until the shell says it.
    #[must_use]
    pub fn zoned(mut self, zone: TimeZone) -> Self {
        self.zone = zone;
        self
    }

    /// What names it and its standing words, as a later run of the same workflow names it.
    #[must_use]
    pub fn named(&self) -> (Naming, String) {
        (self.naming(), self.standing_in(Voice::default()).1)
    }

    /// What names this leg: the launch it continues, else its own execution.
    fn naming(&self) -> Naming {
        Naming {
            execution: self.launched.or(self.execution),
            refused: self.refused.is_some(),
        }
    }

    /// The last run HOME history kept from an earlier session, as the leg of
    /// `execution`: evidence of what was observed then, nothing replayed.
    #[must_use]
    pub fn kept(run: KeptRun, execution: ExecutionId) -> Self {
        let workflow = run
            .workflow
            .clone()
            .unwrap_or_else(|| "(not recorded)".to_owned());
        let mut leg = Self::asked(workflow, false, true, None);
        leg.execution = Some(execution);
        leg.binding = Binding::Unseen;
        leg.kept = Some(run);
        leg
    }

    /// Fold one frame: the first binds the leg's execution, another's is
    /// quarantined, a repeated event is counted once, a second settlement is
    /// counted, never applied, and a frame after the settlement is
    /// quarantined: the settlement is the run's last word.
    pub fn apply(&mut self, frame: RunFrame) {
        self.revision += 1;
        let Some(execution) = frame.execution() else {
            self.foreign += 1;
            return;
        };
        if *self.execution.get_or_insert(execution) != execution {
            self.foreign += 1;
            return;
        }
        match frame {
            RunFrame::Settled(settled) if self.settled.is_none() => {
                if self.binding == Binding::Waiting {
                    // The settlement came first: the start was never observed.
                    self.binding = Binding::Unseen;
                }
                self.settled = Some(settled);
            }
            RunFrame::Settled(_) => self.repeated += 1,
            RunFrame::Event(_) if self.settled.is_some() => self.late += 1,
            RunFrame::Event(event) => self.event(&event),
            _ => self.foreign += 1,
        }
    }

    /// One event of the bound execution, before its settlement.
    fn event(&mut self, event: &Event) {
        if event.kind == EventKind::WorkflowStarted && self.started_from.is_none() {
            self.started_from = event.str_field("resumed_from").map(str::to_owned);
        }
        // A gate the person refused: the run stopped at their answer (the trace's own reading:
        // any decision but `allow`).
        if event.kind == EventKind::ApprovalDecided
            && event
                .str_field("decision")
                .is_some_and(|decision| decision != "allow")
            && let Some(task) = event.str_field("task")
        {
            self.refused_gates.insert(task.to_owned());
        }
        if self.binding == Binding::Waiting {
            self.binding = if event.kind == EventKind::WorkflowStarted {
                let bytes = self.look.as_ref().map(|l| l.source().as_bytes());
                match bytes.and_then(|b| started_on(event, b)) {
                    Some(true) => Binding::Same,
                    Some(false) => Binding::Other,
                    None => Binding::Unnamed,
                }
            } else {
                Binding::Unseen
            };
        }
        if self.beyond > 0 || self.events >= EVENTS_KEPT {
            // Past the window nothing is kept, not even the event's id.
            self.beyond += 1;
        } else if !self.seen.insert(event.id.uuid.to_string()) {
            self.repeated += 1;
        } else {
            // A task's success always follows its start (a cache hit is its
            // own frame): one without it is out of order, folded and counted.
            let started = event.str_field("task").and_then(|id| self.task(id));
            if event.kind == EventKind::TaskCompleted
                && !matches!(started, Some(TaskState::Running | TaskState::Retrying))
            {
                self.disorder += 1;
            }
            self.events += 1;
            let task = event.str_field("task");
            let before = task.and_then(|t| self.view.child(t).cloned());
            self.view.apply(event);
            if let Some(task) = task
                && before.as_ref() != self.view.child(task)
            {
                let epoch = self.relations.entry(task.to_owned()).or_default();
                *epoch = epoch.saturating_add(1);
            }
        }
    }

    /// The turn that drove the leg ended: without its settlement by then, it
    /// never receives one, so it and the tasks it saw start no longer read as
    /// running. A settled leg keeps its settlement.
    pub fn ended(&mut self) {
        if self.settled.is_none() && !self.over {
            self.over = true;
            self.revision += 1;
        }
    }

    /// The state a task of this leg shows for `state`, and why it is closed when
    /// the fold still has it running: settled with every task ok, it reads ok;
    /// settled otherwise, or ended without a settlement, no end of it was
    /// received. Nothing runs once the leg ended or settled.
    #[must_use]
    pub fn shown(&self, state: TaskState) -> (TaskState, Option<Closed>) {
        if !matches!(state, TaskState::Running | TaskState::Retrying) {
            return (state, None);
        }
        let tally = self.settlement().and_then(|settlement| settlement.tasks);
        let all_ok = tally.is_some_and(|tasks| tasks.total > 0 && tasks.ok == tasks.total);
        match (self.settled.is_some(), all_ok) {
            (true, true) => (TaskState::Ok, Some(Closed::SettledOk)),
            (true, false) => (TaskState::Pending, Some(Closed::NoEnd)),
            (false, _) if self.over => (TaskState::Pending, Some(Closed::NoEnd)),
            (false, _) => (state, None),
        }
    }

    /// What the stream said of a frame it could not carry during the turn, the
    /// first said kept.
    /// The run was refused before its first frame, for `why`: the leg settles
    /// as refused, with that reason. A leg whose run already started keeps
    /// its own frames' word.
    pub fn refuse(&mut self, why: String) {
        if self.execution.is_none() && self.refused.is_none() {
            self.refused = Some(why);
            self.revision += 1;
        }
    }

    pub fn told(&mut self, why: &'static str) {
        if self.drained.is_none() {
            self.drained = Some(why);
            self.revision += 1;
        }
    }

    /// What could not reach the fold during the turn (the queue's counters).
    pub fn lost(&mut self, dropped: usize, unread: usize) {
        self.revision += 1;
        self.dropped += dropped;
        self.unread += unread;
    }

    /// How many observations changed this leg (a rendering's key).
    #[must_use]
    pub fn revision(&self) -> usize {
        self.revision
    }

    /// The workflow the run was asked of.
    #[must_use]
    pub fn workflow(&self) -> &str {
        &self.workflow
    }

    /// The execution the leg's first frame bound, when one arrived.
    #[must_use]
    pub fn execution(&self) -> Option<ExecutionId> {
        self.execution
    }

    /// Whether the graph shown is the one the run executes.
    #[must_use]
    pub fn binding(&self) -> Binding {
        self.binding
    }

    /// The state the settlement reports, when one arrived.
    #[must_use]
    pub fn reported(&self) -> Option<RunState> {
        self.settled.as_ref().map(|s| s.settlement.state)
    }

    /// Whether the stream arrived whole: a settlement, and nothing lost,
    /// unread, quarantined or folded past the bound.
    #[must_use]
    pub fn whole(&self) -> bool {
        self.settled.is_some()
            && self.dropped == 0
            && self.unread == 0
            && self.foreign == 0
            && self.beyond == 0
            && self.late == 0
            && self.disorder == 0
    }

    /// The run's frames, as the canonical fold has them.
    #[must_use]
    pub fn run_view(&self) -> &RunView {
        &self.view
    }

    /// The settlement the run's last frame carried, when one arrived: its
    /// state, time, task tally and spend, as the runtime declared them.
    #[must_use]
    pub fn settlement(&self) -> Option<&RunSettlement> {
        self.settled.as_ref().map(|settled| &settled.settlement)
    }

    /// The run this leg continued (a trace id), as its settlement names it, else as its start
    /// did while it runs: the leg a gate paused before this one resumed it.
    #[must_use]
    pub fn resumed_from(&self) -> Option<&str> {
        (self.settled.as_ref())
            .and_then(|settled| settled.resumed_from.as_deref())
            .or(self.started_from.as_deref())
    }

    /// Whether the person refused a gate in this leg: the run stopped at their answer.
    #[must_use]
    pub fn refused_gate(&self) -> bool {
        !self.refused_gates.is_empty()
    }

    /// Whether the person refused the gate of task `id` in this leg.
    #[must_use]
    pub fn refused_at(&self, id: &str) -> bool {
        self.refused_gates.contains(id)
    }

    /// The state task `id` reads in, `state` as the fold has it: a gate the person refused
    /// reads as where the run stopped (the stop glyph), never as done (`✔`), though the task
    /// itself ended: it asked, and their « no » stopped the run there.
    #[must_use]
    pub fn reads_as(&self, id: &str, state: TaskState) -> TaskState {
        if state == TaskState::Ok && self.refused_at(id) {
            TaskState::Cancelled
        } else {
            state
        }
    }

    /// This leg's run as a continuation names it: its execution's trace id.
    #[must_use]
    pub fn trace_id(&self) -> Option<String> {
        self.execution
            .map(|execution| execution.uuid.simple().to_string())
    }

    /// The task state of `id`, as the fold has it.
    #[must_use]
    pub fn task(&self, id: &str) -> Option<TaskState> {
        self.row(id).map(|r| r.state)
    }

    /// The fold's row of task `id`, borrowed.
    #[must_use]
    pub fn row(&self, id: &str) -> Option<&TaskRow> {
        self.view.rows().iter().find(|r| r.id == id)
    }

    /// The child run task `id` called, as its latest settle frame named it.
    #[must_use]
    pub fn child(&self, id: &str) -> Option<&ChildRun> {
        self.view.child(id)
    }

    /// How many times task `id`'s child relation changed.
    #[must_use]
    pub fn relation_epoch(&self, id: &str) -> u64 {
        self.relations.get(id).copied().unwrap_or(0)
    }

    /// The run in the pinned row's words: its state glyph's state, and its
    /// words as a phrase whose one value (an exit code, a state) is set in
    /// after it is said.
    fn standing(&self) -> (TaskState, &'static str, String) {
        if let Some(kept) = &self.kept {
            let exit = kept
                .exit
                .map_or_else(|| "not recorded".to_owned(), |e| e.to_string());
            return (TaskState::Pending, "earlier session · exit {exit}", exit);
        }
        if let (Some(why), None) = (&self.refused, self.execution) {
            return (TaskState::Failed, "refused · {why}", why.clone());
        }
        let (state, words) = match (self.reported(), self.execution) {
            // A settlement says succeeded when the person's « no » stopped the run at a gate:
            // a stop is theirs, never a success (live gauntlet 5, r04 B9).
            (Some(RunState::Succeeded), _) if self.refused_gate() => {
                (TaskState::Cancelled, "settled · stopped at your no")
            }
            (Some(RunState::Succeeded), _) => (TaskState::Ok, "settled · succeeded"),
            (Some(RunState::Failed), _) => (TaskState::Failed, "settled · failed"),
            (Some(RunState::Paused), _) => (TaskState::Paused, "paused · a gate asks you"),
            (Some(RunState::Cancelled), _) => (TaskState::Cancelled, "settled · cancelled"),
            (Some(other), _) => {
                let state = other.as_str().to_owned();
                return (TaskState::Pending, "settled · {state}", state);
            }
            (None, None) if self.over => (TaskState::Pending, "ended · no frame received"),
            (None, Some(_)) if self.over => (TaskState::Pending, "ended · no settlement received"),
            (None, None) => (TaskState::Pending, "asked · waiting for its first frame"),
            (None, Some(_)) => (TaskState::Running, "running"),
        };
        (state, words, String::new())
    }

    /// [`Self::standing`] said in `voice`, its value set in after.
    fn standing_in(&self, voice: Voice) -> (TaskState, String) {
        let (state, words, value) = self.standing();
        let said = voice.say(words);
        let said = said.replace("{exit}", &value).replace("{state}", &value);
        (state, said.replace("{why}", &value))
    }

    /// How the leg is named, in English ([`Self::label_in`]).
    #[must_use]
    pub fn label(&self) -> String {
        self.label_in(Lang::En)
    }

    /// How the leg is named in `lang`: by when the launch it continues began, once that was found
    /// among the legs observed here ([`Self::continuing`]), else by when its own execution began,
    /// to the second, on the clock of the machine's zone (live gauntlet 5: r04 B10, runs named
    /// by hex; section F, times read in UTC), else by its first twelve hex digits, or by nothing
    /// yet (no identity is invented before the first frame). The hex stays in the detail
    /// ([`Self::hex`]).
    #[must_use]
    pub fn label_in(&self, lang: Lang) -> String {
        self.naming().name((&self.zone, lang))
    }

    /// The leg's execution in its first twelve hex digits, as its trace names it: the detail
    /// beside the time it is named by.
    pub fn hex(&self) -> Option<String> {
        self.execution.as_ref().map(hex12)
    }

    /// Name this leg as the launch it continues, once its start or its settlement names the run
    /// it resumed and that run is among the legs observed before it (`past`): one launch a gate
    /// split keeps one name, its first leg's (live gauntlet 5, r02 B13).
    pub fn continuing(&mut self, past: &[Self]) {
        if self.launched.is_some() {
            return;
        }
        let Some(from) = self.resumed_from() else {
            return;
        };
        let resumed = past
            .iter()
            .find(|leg| leg.trace_id().as_deref() == Some(from));
        self.launched = resumed.and_then(|leg| leg.launched.or(leg.execution));
    }

    /// How the leg ended or stands, as its row says it: succeeded, failed, parked at a gate,
    /// running. Nothing while it waits for its first frame, after a stream that ended with no
    /// settlement, or for an earlier session's run: no outcome is claimed that was not observed.
    #[must_use]
    pub fn outcome(&self) -> Option<Verdict> {
        let (task_state, _, _) = self.standing();
        (task_state != TaskState::Pending).then_some(Verdict::Run(task_state))
    }

    /// The pinned row of this leg in `project`.
    #[must_use]
    pub fn pinned(&self, project: &str) -> Pinned {
        self.pinned_in(project, Voice::default())
    }

    /// [`Self::pinned`], its standing said in `voice`.
    #[must_use]
    pub fn pinned_in(&self, project: &str, voice: Voice) -> Pinned {
        let (task_state, words) = self.standing_in(voice);
        Pinned::new(
            project,
            &self.workflow,
            self.label_in(voice.lang),
            task_state,
            words,
        )
    }

    /// The facts above the graph: identity, binding, report, stream, evidence,
    /// each phrase said in `voice` with its values set in after.
    fn facts(&self, voice: Voice) -> Vec<(String, Role)> {
        let ascii = voice.ascii;
        let (sep, _) = marks(ascii);
        let said = |en: &str| voice.say(en).replace("{sep}", sep);
        let leg = voice.say(if self.resume {
            "a resumed leg"
        } else {
            "a fresh run"
        });
        // The state's one home, in the glyph column in use.
        let words = twins(&self.standing_in(voice).1, ascii);
        let mut rows = vec![(
            format!("{}{sep}{leg}{sep}{words}", self.workflow),
            Role::Strong,
        )];
        if let Some(world) = &self.world {
            let leaves = !matches!(world.reach, Reach::Local | Reach::LocalServices);
            let role = if leaves { Role::Warn } else { Role::Dim };
            let reaches = voice.say("reaches, as declared");
            rows.push((format!("{reaches}{sep}{}", world.summary()), role));
        }
        if let Some((naming, words)) = &self.earlier {
            let label = naming.name((&self.zone, voice.lang));
            // Until a run can tell an effect already done, a repeated run does it again: a
            // warning wherever the bytes may touch a service (or nobody audited them).
            let services = (self.world.as_ref()).is_none_or(|w| w.reach != Reach::Local);
            let role = if services { Role::Warn } else { Role::Dim };
            let again = said(
                "again{sep}after {label} of this workflow ({words}){sep}a run does every effect it declares again",
            );
            let again = again.replace("{words}", voice.say(words));
            rows.push((again.replace("{label}", &label), role));
        }
        let witness: String = (self.look.as_ref())
            .and_then(Inspected::witness)
            .unwrap_or("")
            .chars()
            .take(12)
            .collect();
        let graph = said(match self.binding {
            Binding::Waiting => {
                "graph{sep}the bytes {witness} shown when it was asked, until the run names its own"
            }
            Binding::Same => {
                "graph{sep}the bytes {witness} it was asked over, as the run names them"
            }
            Binding::Other => {
                "graph not bound{sep}the run names other bytes than {witness} (the file changed since)"
            }
            Binding::Unnamed => {
                "graph not bound{sep}the run named no source hash, or nothing was read when it was asked"
            }
            Binding::Unseen => "graph not bound{sep}the run's start was not observed",
        });
        rows.push((graph.replace("{witness}", &witness), Role::Dim));
        if !self.typed {
            rows.push((
                said(
                    "story only{sep}this runner tells the run's story, never its frames: the run is not followed here, the transcript keeps the story",
                ),
                Role::Warn,
            ));
        }
        rows.push((self.report(sep, voice), Role::Accent));
        rows.push((
            self.stream(sep, voice),
            if self.whole() { Role::Dim } else { Role::Warn },
        ));
        let evidence = match self.settled.as_ref().and_then(|s| s.evidence) {
            Some(Evidence::Sealed) => "sealed",
            Some(Evidence::Unsealed) => "unsealed",
            Some(Evidence::Lost) => "lost",
            Some(Evidence::NoJournal) => "no journal",
            _ => "not declared yet",
        };
        let declared = if self.settled.is_some() {
            voice.say(", as the run declared it")
        } else {
            ""
        };
        // A journal lost says why when the stream told it, and that nothing told it otherwise.
        let lost = evidence == "lost";
        let why = match (lost, self.drained) {
            (true, Some(why)) => format!("{sep}{why}"),
            (true, None) => format!("{sep}{}", voice.say("reason not reported")),
            (false, _) => String::new(),
        };
        let proof = said(
            "evidence{sep}{evidence}{declared}{why}{sep}the proof is the trace's own verification",
        );
        let proof = (proof.replace("{evidence}", voice.say(evidence)))
            .replace("{declared}", declared)
            .replace("{why}", &why);
        rows.push((proof, if lost { Role::Warn } else { Role::Dim }));
        rows
    }

    /// The settlement's report, or what is known without one. Its state has
    /// one home, the standing on the header row ([`Self::standing`]): the
    /// settlement row says its cause, its tasks and its time.
    fn report(&self, sep: &str, voice: Voice) -> String {
        let said = |en: &str| voice.say(en).replace("{sep}", sep);
        if self.kept.is_some() {
            return said(
                "kept from an earlier session{sep}HOME history recorded it{sep}nothing was replayed",
            );
        }
        let seen = self.view.rows().len().to_string();
        if let (Some(why), None) = (&self.refused, self.execution) {
            let refused = said("refused before its start{sep}{why}{sep}nothing ran");
            return refused.replace("{why}", why);
        }
        let Some(settled) = &self.settled else {
            return match (self.execution, self.view.verdict) {
                (None, _) if self.over => said("asked{sep}no frame received before its turn ended"),
                (None, _) => said("asked{sep}no frame yet{sep}no run identity yet"),
                (Some(_), Some(_)) => said(
                    "no settlement received{sep}the runtime's own end frame is not a settlement",
                ),
                (Some(_), None) if self.over => {
                    said("no settlement received{sep}its turn ended{sep}{n} task(s) seen")
                        .replace("{n}", &seen)
                }
                (Some(_), None) => said("running{sep}{n} task(s) seen").replace("{n}", &seen),
            };
        };
        let s = &settled.settlement;
        let tasks = s.tasks.map_or(String::new(), |t| {
            let ok = voice.say("{ok}/{total} tasks ok");
            let ok = ok.replace("{ok}", &t.ok.to_string());
            format!("{sep}{}", ok.replace("{total}", &t.total.to_string()))
        });
        let elapsed = s
            .elapsed_ms
            .map_or(String::new(), |ms| format!("{sep}{ms} ms"));
        let settlement = said("settlement{sep}{cause}").replace("{cause}", s.cause.as_str());
        format!("{settlement}{tasks}{elapsed}")
    }

    /// Whether the stream arrived whole, and what is missing when not.
    fn stream(&self, sep: &str, voice: Voice) -> String {
        let said = |en: &str| voice.say(en).replace("{sep}", sep);
        let events = self.events.to_string();
        if self.kept.is_some() {
            return said("stream{sep}not followed in this session");
        }
        if self.refused.is_some() && self.execution.is_none() {
            return said("stream{sep}its refusal alone, before any frame");
        }
        if self.whole() {
            return said("stream{sep}{n} events and the settlement, whole").replace("{n}", &events);
        }
        let mut missing = Vec::new();
        if self.settled.is_none() {
            missing.push(voice.say("no settlement").to_owned());
        }
        for (n, what) in [
            (self.dropped, "{n} lost on the way"),
            (self.unread, "{n} unread"),
            (self.foreign, "{n} of another run, set aside"),
            (self.beyond, "{n} past the fold's bound"),
            (self.late, "{n} after the settlement, set aside"),
            (self.disorder, "{n} out of order"),
        ] {
            if n > 0 {
                missing.push(voice.say(what).replace("{n}", &n.to_string()));
            }
        }
        let incomplete = said("stream incomplete{sep}{missing}{sep}{n} events folded");
        (incomplete.replace("{n}", &events)).replace("{missing}", &missing.join(sep))
    }

    /// The leg's `face` as the object in view on a region `width` cells
    /// wide: the title (the run, its standing, the faces with the one in
    /// view marked), then that face, its first listed task picked. Pure:
    /// what a face needs from the disk was acquired before ([`Self::wants`]).
    #[must_use]
    pub fn lines(
        &self,
        face: RunFace,
        width: u16,
        ascii: bool,
        color: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        let voice = Voice {
            lang: Lang::En,
            ascii,
        };
        self.view(face, &Pick::new(), width, (voice, true), color)
    }

    /// The run's icon, label and standing glyph, in `voice`: the head of every title.
    fn head(&self, voice: Voice) -> String {
        let (task_state, _, _) = self.standing();
        let (glyph, _) = state::cell(task_state, voice.ascii);
        let run = Icon::Run.glyph(voice.ascii);
        format!("{run} {} {glyph}", self.label_in(voice.lang))
    }

    /// [`Self::lines`] with the desk's `pick`, in `voice`: on the run face,
    /// the list marks the picked task, or the picked task's detail stands in
    /// its place while it is open. The faces' names and the run face are said
    /// in `voice`; the other faces and a task's detail keep English.
    #[must_use]
    pub fn view(
        &self,
        face: RunFace,
        pick: &Pick,
        width: u16,
        (voice, here): (Voice, bool),
        color: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        let ascii = voice.ascii;
        // The viewers the faces show values through speak the person's language too.
        let canvas = crate::Canvas::new(width, ascii, color).speaking(voice.lang);
        if face == RunFace::Run
            && pick.is_open()
            && let Some(id) = pick.current(self)
        {
            return match pick.child_view() {
                Some(child) => self.child_lines(child, (canvas, voice)),
                None => self.detail(id, canvas, voice),
            };
        }
        let (sep, cut) = marks(ascii);
        let cells = usize::from(width);
        let tabs: Vec<String> = (RunFace::ALL.iter())
            .map(|f| {
                if *f == face {
                    format!("[{}]", f.label_in(voice))
                } else {
                    f.label_in(voice).to_owned()
                }
            })
            .collect();
        let head = format!("{}{sep}{}", self.head(voice), tabs.join(" "));
        let title = Line::from(Span::styled(
            fit_head(&head, cells, cut),
            role::style(Role::Strong, color),
        ));
        let body = match face {
            RunFace::Outputs => self.outputs_body(canvas, voice),
            RunFace::Files => self.files_body(canvas, voice),
            RunFace::Proof => self.proof_body(cells, voice, color),
            _ => self.run_body(canvas, pick.current(self), (voice, here)),
        };
        (title, body)
    }

    /// The run face: the facts, the graph of the bound bytes with each node
    /// in its state (or why it is not drawn), then the tasks, `picked`
    /// marked. The list is the body's last rows, one row per task.
    fn run_body(
        &self,
        canvas: crate::Canvas,
        picked: Option<&str>,
        (voice, here): (Voice, bool),
    ) -> Vec<Line<'static>> {
        let (cells, ascii, color) = (usize::from(canvas.width), canvas.ascii, canvas.color);
        let (sep, cut) = marks(ascii);
        let mut body = Vec::new();
        for (row, tone) in self.facts(voice) {
            for part in wrap(&row, cells, cut) {
                body.push(Line::from(Span::styled(part, role::style(tone, color))));
            }
        }
        body.push(Line::default());
        if self.binding == Binding::Same {
            body.extend(self.graph(cells, voice, color));
            body.push(Line::default());
        }
        if let Some(journal) = self.history() {
            let short: String = journal.witness().unwrap_or("").chars().take(12).collect();
            let said = voice.say(
                "journal {trace}{sep}captured bytes {short}{sep}its tasks as it recorded them",
            );
            let said = (said.replace("{sep}", sep)).replace("{short}", &short);
            let said = said.replace("{trace}", journal.trace());
            body.extend(faces::lines_of(&[(said, Role::Dim)], cells, ascii, color));
        } else if let Some(why) = self.history_missing(voice) {
            body.extend(faces::lines_of(&[(why, Role::Dim)], cells, ascii, color));
        }
        let listed = self.listed();
        if listed.is_empty() {
            return body;
        }
        // The list's keys from where they are: the box needs `F6` first.
        let keys = voice.say(if here {
            "tasks · ↑↓ pick · Enter details"
        } else {
            "tasks · F6 then ↑↓ pick · Enter details"
        });
        let keys = super::text::own(keys, ascii);
        body.push(Line::from(Span::styled(
            fit_head(&keys, cells, cut),
            role::style(Role::Dim, color),
        )));
        for item in listed {
            body.push(self.list_row(item, picked == Some(item.0), canvas, voice));
        }
        body
    }

    /// What the leg's fold kept of its stream: the events it took, the ids it remembers, the
    /// events past its bound and the repeats it counted (the renderer's fold tests read them).
    #[doc(hidden)]
    #[must_use]
    pub fn kept_counts(&self) -> (usize, usize, usize, usize) {
        (self.events, self.seen.len(), self.beyond, self.repeated)
    }

    /// The bound bytes' graph, each node's chip its state as the fold has
    /// it, the chips and the graph's own words said in `voice`.
    #[must_use]
    pub fn graph(&self, cells: usize, voice: Voice, color: bool) -> Vec<Line<'static>> {
        let ascii = voice.ascii;
        let Some((doc, waves)) = self.look.as_ref().and_then(Inspected::graph) else {
            return Vec::new();
        };
        let observed = |id: &str| {
            let row = self.row(id)?;
            let (shown, closed) = self.shown(row.state);
            let (glyph, tone) = state::cell(self.reads_as(id, shown), ascii);
            let words = match shown {
                _ if closed.is_some() => closed.map_or("", |closed| closed.words(false)),
                TaskState::Pending => "scheduled",
                TaskState::Running => "running",
                TaskState::Ok if row.cached => "cache hit (resume)",
                TaskState::Ok if self.refused_at(id) => "answered no",
                TaskState::Ok if row.recovered => "succeeded after recovery",
                TaskState::Ok => "succeeded",
                TaskState::Failed => "failed",
                TaskState::Retrying => "retrying",
                TaskState::Skipped => "skipped",
                TaskState::Cancelled => "cancelled",
                TaskState::Paused => "paused",
            };
            Some((format!("{glyph} {}", voice.say(words)), tone))
        };
        let width = u16::try_from(cells).unwrap_or(u16::MAX);
        let canvas = crate::Canvas::new(width, ascii, color).speaking(voice.lang);
        let rendered = crate::graph_cards(doc, waves, canvas, &observed);
        let mut lines = rendered.head(canvas);
        lines.extend(rendered.lines);
        lines
    }
}

/// The first twelve hex digits of `execution`.
fn hex12(execution: &ExecutionId) -> String {
    execution
        .uuid
        .simple()
        .to_string()
        .chars()
        .take(12)
        .collect()
}

/// What names a leg, read again in the language a later screen speaks: the execution it is
/// named by (the launch it continues, else its own) and whether it was refused before one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Naming {
    execution: Option<ExecutionId>,
    refused: bool,
}

impl Naming {
    /// The name in `lang`, its time read on the clock of `zone`: when the execution began, else
    /// its hex; « run (refused) » or « run (starting) » before any execution.
    fn name(self, (zone, lang): (&TimeZone, Lang)) -> String {
        match self.execution {
            Some(execution) => match began(&execution, zone, lang) {
                Some(when) => format!("run {when}"),
                None => format!("run {}", hex12(&execution)),
            },
            None if self.refused => "run (refused)".to_owned(),
            None => "run (starting)".to_owned(),
        }
    }
}

/// When `execution` began, to the second, on the clock of `zone` (the machine's, where the
/// person reads it): month and day in English (`10-17 11:04:30`), day and month in French
/// (`17/10 11:04:30`). A time-ordered (version 7) identity holds the milliseconds since 1970 in
/// its first 48 bits; to the second, two runs in the same minute keep two names. `None` for an
/// identity that holds no time.
fn began(execution: &ExecutionId, zone: &TimeZone, lang: Lang) -> Option<String> {
    if execution.uuid.get_version_num() != 7 {
        return None;
    }
    let ms = (execution.uuid.as_bytes().iter().take(6))
        .fold(0_i64, |ms, byte| (ms << 8) | i64::from(*byte));
    let local = jiff::Timestamp::from_millisecond(ms)
        .ok()?
        .to_zoned(zone.clone());
    let (month, day) = (local.month(), local.day());
    let time = format!(
        "{:02}:{:02}:{:02}",
        local.hour(),
        local.minute(),
        local.second()
    );
    Some(match lang {
        Lang::Fr => format!("{day:02}/{month:02} {time}"),
        _ => format!("{month:02}-{day:02} {time}"),
    })
}
