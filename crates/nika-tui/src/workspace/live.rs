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
//! A leg kept from an earlier session shows what HOME history recorded of it
//! (its execution, workflow and exit as observed then): nothing replays, and
//! its proof is read again from its journal when that face opens.

use std::collections::BTreeSet;

use nika_display::run_story::{
    Event, EventKind, Evidence, ExecutionId, RunFrame, RunState, Settled, started_on,
};
use nika_display::state::{RunView, TaskState};
use nika_display::theme::{Role, Theme};
use ratatui::text::{Line, Span};

use super::inspect::Inspected;
use crate::session::acquire::{Fetched, Proven};
use nika_session::KeptRun;

mod faces;
use super::pinned::Pinned;
use super::text::{fit_head, marks, wrap};
use crate::visual::icon::Icon;
use crate::visual::{role, state};
pub use faces::{FILES_READ, RunFace, Want};

/// The most events one leg's fold takes, and the most ids it remembers to
/// count a repeat; past it an event is only counted and the stream is
/// incomplete.
pub const EVENTS_KEPT: usize = 100_000;

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
    fetched: Vec<Fetched>,
    proven: Option<Proven>,
    kept: Option<KeptRun>,
    /// How many times what was acquired was forgotten: a reading's key.
    generation: u64,
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
    pub(crate) fn asked(
        workflow: String,
        resume: bool,
        typed: bool,
        look: Option<Inspected>,
    ) -> Self {
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
            fetched: Vec::new(),
            proven: None,
            kept: None,
            generation: 0,
            revision: 0,
        }
    }

    /// The last run HOME history kept from an earlier session, as the leg of
    /// `execution`: evidence of what was observed then, nothing replayed.
    #[must_use]
    pub(crate) fn kept(run: KeptRun, execution: ExecutionId) -> Self {
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
    pub(crate) fn apply(&mut self, frame: RunFrame) {
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
            self.view.apply(event);
        }
    }

    /// What could not reach the fold during the turn (the queue's counters).
    pub(crate) fn lost(&mut self, dropped: usize, unread: usize) {
        self.revision += 1;
        self.dropped += dropped;
        self.unread += unread;
    }

    /// How many observations changed this leg (a rendering's key).
    pub(crate) fn revision(&self) -> usize {
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

    /// The task state of `id`, as the fold has it.
    #[must_use]
    pub fn task(&self, id: &str) -> Option<TaskState> {
        self.view
            .rows()
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.state)
    }

    /// The run in the pinned row's words: its state glyph's state and words.
    fn standing(&self) -> (TaskState, String) {
        if let Some(kept) = &self.kept {
            let exit = kept
                .exit
                .map_or_else(|| "not recorded".to_owned(), |e| e.to_string());
            return (TaskState::Pending, format!("earlier session · exit {exit}"));
        }
        match (self.reported(), self.execution) {
            (Some(RunState::Succeeded), _) => (TaskState::Ok, "settled · succeeded".to_owned()),
            (Some(RunState::Failed), _) => (TaskState::Failed, "settled · failed".to_owned()),
            (Some(RunState::Paused), _) => {
                (TaskState::Paused, "paused · a gate asks you".to_owned())
            }
            (Some(RunState::Cancelled), _) => {
                (TaskState::Cancelled, "settled · cancelled".to_owned())
            }
            (Some(other), _) => (TaskState::Pending, format!("settled · {}", other.as_str())),
            (None, None) => (
                TaskState::Pending,
                "asked · waiting for its first frame".to_owned(),
            ),
            (None, Some(_)) => (TaskState::Running, "running".to_owned()),
        }
    }

    /// How the leg is named: its execution's first twelve hex digits, or
    /// nothing yet (no identity is invented before the first frame).
    #[must_use]
    pub fn label(&self) -> String {
        match self.execution {
            Some(execution) => {
                let hex: String = execution
                    .uuid
                    .simple()
                    .to_string()
                    .chars()
                    .take(12)
                    .collect();
                format!("run {hex}")
            }
            None => "run (starting)".to_owned(),
        }
    }

    /// The pinned row of this leg in `project`.
    #[must_use]
    pub fn pinned(&self, project: &str) -> Pinned {
        let (task_state, words) = self.standing();
        Pinned::new(project, &self.workflow, self.label(), task_state, words)
    }

    /// The facts above the graph: identity, binding, report, stream, evidence.
    fn facts(&self, ascii: bool) -> Vec<(String, Role)> {
        let (sep, _) = marks(ascii);
        let leg = if self.resume {
            "a resumed leg"
        } else {
            "a fresh run"
        };
        let (_, words) = self.standing();
        let mut rows = vec![(
            format!("{}{sep}{leg}{sep}{words}", self.workflow),
            Role::Strong,
        )];
        let witness: String = (self.look.as_ref())
            .and_then(Inspected::witness)
            .unwrap_or("")
            .chars()
            .take(12)
            .collect();
        let graph = match self.binding {
            Binding::Waiting => format!(
                "graph{sep}the bytes {witness} shown when it was asked, until the run names its own"
            ),
            Binding::Same => {
                format!("graph{sep}the bytes {witness} it was asked over, as the run names them")
            }
            Binding::Other => format!(
                "graph not bound{sep}the run names other bytes than {witness} (the file changed since)"
            ),
            Binding::Unnamed => format!(
                "graph not bound{sep}the run named no source hash, or nothing was read when it was asked"
            ),
            Binding::Unseen => format!("graph not bound{sep}the run's start was not observed"),
        };
        rows.push((graph, Role::Dim));
        if !self.typed {
            rows.push((
                format!(
                    "story only{sep}this runner tells the run's story, never its frames: the run is not followed here, the transcript keeps the story"
                ),
                Role::Warn,
            ));
        }
        rows.push((self.report(sep), Role::Accent));
        rows.push((
            self.stream(sep),
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
            ", as the run declared it"
        } else {
            ""
        };
        rows.push((
            format!(
                "evidence{sep}{evidence}{declared}{sep}the proof is the trace's own verification"
            ),
            Role::Dim,
        ));
        rows
    }

    /// The settlement's report, or what is known without one.
    fn report(&self, sep: &str) -> String {
        if self.kept.is_some() {
            return format!(
                "kept from an earlier session{sep}HOME history recorded it{sep}nothing was replayed"
            );
        }
        let Some(settled) = &self.settled else {
            return match (self.execution, self.view.verdict) {
                (None, _) => format!("asked{sep}no frame yet{sep}no run identity yet"),
                (Some(_), Some(_)) => {
                    format!(
                        "no settlement received{sep}the runtime's own end frame is not a settlement"
                    )
                }
                (Some(_), None) => format!("running{sep}{} task(s) seen", self.view.rows().len()),
            };
        };
        let s = &settled.settlement;
        let tasks = s.tasks.map_or(String::new(), |t| {
            format!("{sep}{}/{} tasks ok", t.ok, t.total)
        });
        let elapsed = s
            .elapsed_ms
            .map_or(String::new(), |ms| format!("{sep}{ms} ms"));
        format!(
            "settled{sep}{}{sep}{}{tasks}{elapsed}",
            s.state.as_str(),
            s.cause.as_str()
        )
    }

    /// Whether the stream arrived whole, and what is missing when not.
    fn stream(&self, sep: &str) -> String {
        if self.kept.is_some() {
            return format!("stream{sep}not followed in this session");
        }
        if self.whole() {
            return format!(
                "stream{sep}{} events and the settlement, whole",
                self.events
            );
        }
        let mut missing = Vec::new();
        if self.settled.is_none() {
            missing.push("no settlement".to_owned());
        }
        for (n, what) in [
            (self.dropped, "lost on the way"),
            (self.unread, "unread"),
            (self.foreign, "of another run, set aside"),
            (self.beyond, "past the fold's bound"),
            (self.late, "after the settlement, set aside"),
            (self.disorder, "out of order"),
        ] {
            if n > 0 {
                missing.push(format!("{n} {what}"));
            }
        }
        format!(
            "stream incomplete{sep}{}{sep}{} events folded",
            missing.join(sep),
            self.events
        )
    }

    /// The leg's `face` as the object in view on a region `width` cells
    /// wide: the title (the run, its standing, the faces with the one in
    /// view marked), then that face. Pure: what a face needs from the disk
    /// was acquired before ([`Self::wants`]).
    #[must_use]
    pub fn lines(
        &self,
        face: RunFace,
        width: u16,
        ascii: bool,
        color: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        let (sep, cut) = marks(ascii);
        let cells = usize::from(width);
        let (task_state, _) = self.standing();
        let (glyph, _) = state::cell(task_state, ascii);
        let tabs: Vec<String> = (RunFace::ALL.iter())
            .map(|f| {
                if *f == face {
                    format!("[{}]", f.label())
                } else {
                    f.label().to_owned()
                }
            })
            .collect();
        let head = format!(
            "{} {} {glyph}{sep}{}",
            Icon::Run.glyph(ascii),
            self.label(),
            tabs.join(" ")
        );
        let title = Line::from(Span::styled(
            fit_head(&head, cells, cut),
            role::style(Role::Strong, color),
        ));
        let canvas = nika_tui_view::Canvas::new(width, ascii, color);
        let body = match face {
            RunFace::Outputs => self.outputs_body(canvas),
            RunFace::Files => self.files_body(canvas),
            RunFace::Proof => self.proof_body(cells, ascii, color),
            _ => self.run_body(cells, ascii, color),
        };
        (title, body)
    }

    /// The run face: the facts, the graph of the bound bytes with each node
    /// in its state (or why it is not drawn), then the tasks as the fold has them.
    fn run_body(&self, cells: usize, ascii: bool, color: bool) -> Vec<Line<'static>> {
        let (sep, cut) = marks(ascii);
        let mut body = Vec::new();
        for (row, tone) in self.facts(ascii) {
            for part in wrap(&row, cells, cut) {
                body.push(Line::from(Span::styled(part, role::style(tone, color))));
            }
        }
        body.push(Line::default());
        if self.binding == Binding::Same {
            body.extend(self.graph(cells, ascii, color));
            body.push(Line::default());
        }
        for row in self.view.rows() {
            let (glyph, tone) = state::cell(row.state, ascii);
            let ms = row
                .wall_ms()
                .map_or(String::new(), |ms| format!("{sep}{ms} ms"));
            let text = format!("{glyph} {}{ms}", row.id);
            body.push(Line::from(Span::styled(
                fit_head(&text, cells, cut),
                role::style(tone, color),
            )));
        }
        body
    }

    /// The bound bytes' graph, each node's chip its state as the fold has it.
    fn graph(&self, cells: usize, ascii: bool, color: bool) -> Vec<Line<'static>> {
        let Some((doc, waves)) = self.look.as_ref().and_then(Inspected::graph) else {
            return Vec::new();
        };
        let theme = Theme::new(false, ascii, false);
        let chip = |id: &str, _verb: &str| {
            let (glyph, _) = state::cell(self.task(id).unwrap_or(TaskState::Pending), ascii);
            (format!("{glyph:<2}"), id.to_owned())
        };
        let wires = nika_display::dag_art::wire_graph(doc, waves);
        let Some(art) = nika_display::wires::render_with(&wires, theme, &chip, None) else {
            return vec![Line::from(Span::styled(
                "the graph does not draw truthfully at this shape: the tasks are listed below",
                role::style(Role::Dim, color),
            ))];
        };
        let (_, cut) = marks(ascii);
        art.lines()
            .map(|l| {
                Line::from(Span::styled(
                    fit_head(l, cells, cut),
                    role::style(Role::Strong, color),
                ))
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod live_tests;
