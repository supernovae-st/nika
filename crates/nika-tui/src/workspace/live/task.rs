// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The tasks of the run in view, one of them picked and read in detail.
//!
//! A task is picked by its run's execution and its id, never by the line it
//! is painted on: a wrap, a resize or a face turned away and back finds the
//! same task. The list follows the graph only when the run names the bytes
//! it was drawn from (`Binding::Same`): its tasks in the projection's order,
//! then its cleanup units, then what only the stream named. Otherwise the
//! stream's own order stands and no static fact is lent. A task the graph
//! declares and no frame named is listed as not observed, never in a state.
//!
//! The detail reads the fold's row as it is: its state, its failure, what
//! was measured and its output through the viewers, each absence named for
//! what it is (not finished, never ended, failed, carried none). Nothing here
//! reads a file, starts, answers or saves anything.

use nika_display::state::{AgentProgress, TaskRow, TaskState};
use nika_display::theme::Role;
use nika_tui_view::{Availability, Canvas, Content, Meta};
use ratatui::text::{Line, Span};

use super::child::{ChildView, openable};
use super::faces::lines_of;
use super::{Binding, LiveRun};
use crate::session::acquire::ChildRead;
use crate::visual::{role, state};
use crate::workspace::text::{fit_head, marks};
use nika_display::run_story::ChildRun;
use nika_display::run_story::ExecutionId;

/// The task picked in the run in view, and whether its detail is open.
#[derive(Clone, Debug, Default)]
pub(crate) struct Pick {
    /// The run and the task a key picked; until one does, the first listed.
    task: Option<(ExecutionId, String)>,
    /// The detail is open; the list's scroll to return to.
    open: Option<usize>,
    /// The pick moved: the scroll follows it once.
    follow: bool,
    /// The child journal opened from the detail, one level down.
    child: Option<ChildView>,
    /// How many child views were opened: each opening's own name.
    openings: u64,
}

/// Where a listed task comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// A task of the bytes the run names.
    Declared,
    /// A cleanup unit of the bytes the run names (`finally`).
    Cleanup,
    /// A task only the stream named.
    Observed,
}

impl Pick {
    /// Nothing picked by a key, no detail open.
    pub(crate) const fn new() -> Self {
        Self {
            task: None,
            open: None,
            follow: false,
            child: None,
            openings: 0,
        }
    }

    /// The task picked in `leg`: the one a key picked when it is this leg's
    /// and listed, else the first listed.
    pub(crate) fn current<'a>(&'a self, leg: &'a LiveRun) -> Option<&'a str> {
        let listed = leg.listed();
        if let (Some((execution, id)), Some(now)) = (&self.task, leg.execution())
            && *execution == now
            && listed.iter().any(|(listed, _)| listed == id)
        {
            return Some(id);
        }
        listed.first().map(|(id, _)| *id)
    }

    /// Pick the next listed task (or the previous one); `false` at an end.
    pub(crate) fn step(&mut self, leg: &LiveRun, down: bool) -> bool {
        let listed = leg.listed();
        let at = (self.current(leg)).and_then(|id| listed.iter().position(|(l, _)| *l == id));
        let (Some(execution), Some(at)) = (leg.execution(), at) else {
            return false;
        };
        let next = if down {
            at.checked_add(1)
        } else {
            at.checked_sub(1)
        };
        let Some((id, _)) = next.and_then(|n| listed.get(n)) else {
            return false;
        };
        self.task = Some((execution, (*id).to_owned()));
        self.follow = true;
        true
    }

    /// Open the picked task's detail, keeping the list's `scroll` for the
    /// return; `false` when no task is listed.
    pub(crate) fn open(&mut self, leg: &LiveRun, scroll: usize) -> bool {
        let id = self.current(leg).map(str::to_owned);
        let (Some(execution), Some(id)) = (leg.execution(), id) else {
            return false;
        };
        self.task = Some((execution, id));
        self.open = Some(scroll);
        true
    }

    /// Close the detail: the list's scroll to return to, when one was open.
    pub(crate) fn close(&mut self) -> Option<usize> {
        self.open.take()
    }

    /// Whether the picked task's detail is open.
    pub(crate) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Whether the scroll still has to follow a moved pick (once).
    pub(crate) fn follows(&mut self) -> bool {
        std::mem::take(&mut self.follow)
    }

    /// Open the child journal the picked task's relation names, from its
    /// open detail, keeping the detail's `scroll`; `false` when there is
    /// none to open.
    pub(crate) fn open_child(&mut self, leg: &LiveRun, scroll: usize) -> bool {
        if !self.is_open() || self.child.is_some() {
            return false;
        }
        let Some(id) = self.current(leg).map(str::to_owned) else {
            return false;
        };
        let Some(relation) = leg.child(&id).filter(|r| openable(r).is_ok()).cloned() else {
            return false;
        };
        let epoch = leg.relation_epoch(&id);
        self.openings = self.openings.wrapping_add(1);
        let opened = (epoch, self.openings);
        self.child = Some(ChildView::new(id, relation, opened, scroll));
        true
    }

    /// Close the child view: the detail's scroll to return to, when one was open.
    pub(crate) fn close_child(&mut self) -> Option<usize> {
        self.child.take().map(|view| view.scroll())
    }

    /// Whether a child journal is in view.
    pub(crate) fn child_open(&self) -> bool {
        self.child.is_some()
    }

    /// The child view, when one is open.
    pub(crate) fn child_view(&self) -> Option<&ChildView> {
        self.child.as_ref()
    }

    /// Which opening the open child view is, when one is open.
    pub(crate) fn child_opening(&self) -> Option<u64> {
        self.child.as_ref().map(ChildView::opening)
    }

    /// What the open child view still needs read (its task and relation),
    /// while that relation is still the task's own and unread this reading.
    pub(crate) fn child_wanted(&self, leg: &LiveRun) -> Option<(String, ChildRun)> {
        self.child.as_ref()?.wanted(leg)
    }

    /// Keep `read` for the open child view when it answers that very view
    /// (asked by this opening, for its task and relation, still current);
    /// `false` when it is dropped.
    pub(crate) fn child_read(
        &mut self,
        leg: &LiveRun,
        asked: (Option<u64>, &str, &ChildRun),
        read: ChildRead,
    ) -> bool {
        self.child
            .as_mut()
            .is_some_and(|view| view.keep(leg, asked, read))
    }
}

impl LiveRun {
    /// The tasks of the run in list order, each with where it comes from.
    pub(crate) fn listed(&self) -> Vec<(&str, Origin)> {
        let rows = self.view.rows();
        let Some((doc, _)) = self.bound_graph() else {
            return rows
                .iter()
                .map(|r| (r.id.as_str(), Origin::Observed))
                .collect();
        };
        let cleanup = |kind: &str| kind == "finally";
        let declared = (doc.nodes.iter().filter(|n| !cleanup(n.kind)))
            .map(|n| (n.id.as_str(), Origin::Declared));
        let units = (doc.nodes.iter().filter(|n| cleanup(n.kind)))
            .map(|n| (n.id.as_str(), Origin::Cleanup));
        let mut listed: Vec<(&str, Origin)> = declared.chain(units).collect();
        for row in rows {
            if !listed.iter().any(|(id, _)| *id == row.id) {
                listed.push((row.id.as_str(), Origin::Observed));
            }
        }
        listed
    }

    /// The graph of the bytes the run names, when it names those shown.
    pub(super) fn bound_graph(&self) -> Option<(&nika_display::dag_art::GraphDoc, &[Vec<usize>])> {
        if self.binding != Binding::Same {
            return None;
        }
        self.look.as_ref()?.graph()
    }

    /// One row of the task list: the pick mark, then the task as the fold
    /// has it, or not observed when no frame named it.
    pub(super) fn list_row(
        &self,
        (id, origin): (&str, Origin),
        picked: bool,
        canvas: Canvas,
    ) -> Line<'static> {
        let (sep, cut) = marks(canvas.ascii);
        let mark = match (picked, canvas.ascii) {
            (false, _) => "  ",
            (true, false) => "› ",
            (true, true) => "* ",
        };
        let unit = if origin == Origin::Cleanup {
            format!("{sep}cleanup (finally)")
        } else {
            String::new()
        };
        let (text, tone) = match self.row(id) {
            Some(row) => {
                let (glyph, tone) = state::cell(row.state, canvas.ascii);
                let ms = row
                    .wall_ms()
                    .map_or(String::new(), |ms| format!("{sep}{ms} ms"));
                (format!("{mark}{glyph} {id}{ms}{unit}"), tone)
            }
            None => (
                format!("{mark}{id}{unit}{sep}not observed in this run"),
                Role::Dim,
            ),
        };
        Line::from(Span::styled(
            fit_head(&text, usize::from(canvas.width), cut),
            role::style(tone, canvas.color),
        ))
    }

    /// The detail of task `id`: the title names it and the way back, the
    /// body reads its row and, when bound, its node.
    pub(super) fn detail(&self, id: &str, canvas: Canvas) -> (Line<'static>, Vec<Line<'static>>) {
        let (sep, cut) = marks(canvas.ascii);
        let cells = usize::from(canvas.width);
        let head = format!(
            "{}{sep}task {id}{sep}Backspace: back to the tasks",
            self.head(canvas.ascii)
        );
        let title = Line::from(Span::styled(
            fit_head(&head, cells, cut),
            role::style(Role::Strong, canvas.color),
        ));
        let mut rows = self.standing_of(id, sep, canvas.ascii);
        rows.extend(self.statics(id, sep));
        let Some(row) = self.row(id) else {
            return (title, lines_of(&rows, cells, canvas.ascii, canvas.color));
        };
        rows.push(measured(row, sep));
        rows.extend(said(row, sep));
        rows.extend(self.child_section(id, sep));
        let mut body = lines_of(&rows, cells, canvas.ascii, canvas.color);
        body.push(Line::default());
        body.extend(self.task_output(id, row, canvas));
        (title, body)
    }

    /// The task's state as the fold has it, or why it has none.
    fn standing_of(&self, id: &str, sep: &str, ascii: bool) -> Vec<(String, Role)> {
        let Some(row) = self.row(id) else {
            return vec![(
                format!(
                    "{id}{sep}not observed in this run: the bound graph declares it, no frame of this run named it"
                ),
                Role::Dim,
            )];
        };
        let words = match row.state {
            TaskState::Pending => "scheduled, not started",
            TaskState::Running => "running, not finished",
            TaskState::Ok if row.cached => "succeeded from the cache of an earlier leg (resume)",
            TaskState::Ok => "succeeded",
            TaskState::Failed => "failed",
            TaskState::Retrying => "an attempt failed, a retry is scheduled",
            TaskState::Skipped => "skipped: its condition was false",
            TaskState::Cancelled => "cancelled: it never ran to its end",
            TaskState::Paused => "paused: a gate asks a human",
        };
        let repaired = if row.recovered {
            format!("{sep}repaired by its on_error recovery")
        } else {
            String::new()
        };
        let (glyph, tone) = state::cell(row.state, ascii);
        vec![(
            format!("{glyph} {id}{sep}{words}{repaired}{sep}as the stream folded it"),
            tone,
        )]
    }

    /// What the bound bytes declare of task `id`, or why nothing is lent.
    fn statics(&self, id: &str, sep: &str) -> Vec<(String, Role)> {
        let Some((doc, _)) = self.bound_graph() else {
            return vec![(
                format!("static facts unknown{sep}the graph is not bound to this run"),
                Role::Dim,
            )];
        };
        let Some(node) = doc.nodes.iter().find(|n| n.id == id) else {
            return vec![(
                format!("static facts unknown{sep}the bound graph does not declare it"),
                Role::Dim,
            )];
        };
        let mut rows = Vec::new();
        if node.kind == "finally" {
            let parents: Vec<&str> = (doc.edges.iter())
                .filter(|e| e.kind == "finally" && e.to == id)
                .map(|e| e.from.as_str())
                .collect();
            rows.push((
                format!(
                    "cleanup (finally){sep}runs when {} unwinds, never as a main task",
                    parents.join(", ")
                ),
                Role::Dim,
            ));
        }
        let mut facts = vec![node.verb.to_owned()];
        facts.extend(node.tool.clone());
        facts.extend(node.model.clone());
        facts.extend(node.when.as_ref().map(|w| format!("when {w}")));
        facts.extend(node.fan_out.as_ref().map(|f| match f.count {
            Some(n) => format!("for_each {} of {n}", f.kind),
            None => format!("for_each {}", f.kind),
        }));
        facts.extend(
            node.retry_max_attempts
                .map(|n| format!("retry {n} attempts")),
        );
        facts.extend(node.timeout_ms.map(|ms| format!("timeout {ms} ms")));
        facts.extend(node.on_error.map(|e| format!("on_error {e}")));
        if !node.outputs.is_empty() {
            facts.push(format!("outputs {}", node.outputs.join(", ")));
        }
        if !node.permits.is_empty() {
            facts.push(format!("permits {}", node.permits.join(", ")));
        }
        rows.push((
            format!(
                "declared{sep}{}{sep}the bytes the run names",
                facts.join(sep)
            ),
            Role::Dim,
        ));
        rows
    }

    /// The output the stream carried, through the viewers, or why there is
    /// none; then a fan-out's items the same way.
    fn task_output(&self, id: &str, row: &TaskRow, canvas: Canvas) -> Vec<Line<'static>> {
        let (sep, _) = marks(canvas.ascii);
        let cells = usize::from(canvas.width);
        let Some(json) = row.output_json.as_deref() else {
            let why = match row.state {
                TaskState::Pending
                | TaskState::Running
                | TaskState::Retrying
                | TaskState::Paused => "not finished: no output yet",
                TaskState::Failed => "no output on the stream: the task failed",
                TaskState::Skipped | TaskState::Cancelled => {
                    "no output on the stream: it never ran to its end"
                }
                TaskState::Ok => {
                    "no output on the stream: its frame carried none (a value withheld as secret, or an older engine)"
                }
            };
            return lines_of(
                &[(format!("output{sep}{why}"), Role::Dim)],
                cells,
                canvas.ascii,
                canvas.color,
            );
        };
        let mut lines = self.task_value("output", id, json, canvas);
        if let Some(items) = row.items_json.as_deref() {
            lines.push(Line::default());
            lines.extend(self.task_value("items", id, items, canvas));
        }
        lines
    }

    /// One JSON value the stream carried for task `id`, by the viewers.
    fn task_value(&self, name: &str, id: &str, json: &str, canvas: Canvas) -> Vec<Line<'static>> {
        let (sep, _) = marks(canvas.ascii);
        let mut meta = Meta::new(name);
        meta.format = Some("json".to_owned());
        meta.provenance = Some(format!("{}{sep}task {id}", self.label()));
        meta.protected = true;
        meta.availability = Availability::Present;
        let content = if json.trim() == "null" {
            Content::Null
        } else {
            Content::Text(json)
        };
        let rendered = nika_tui_view::artifact(content, &meta, canvas);
        let mut lines = lines_of(
            &[(
                format!("{name}{sep}{} bytes on the stream", json.len()),
                Role::Strong,
            )],
            usize::from(canvas.width),
            canvas.ascii,
            canvas.color,
        );
        lines.extend(rendered.head(canvas));
        lines.extend(rendered.lines);
        lines
    }
}

/// What the stream measured of a task, each absent meter left out and said.
fn measured(row: &TaskRow, sep: &str) -> (String, Role) {
    let mut parts = Vec::new();
    parts.extend(row.wall_ms().map(|ms| format!("{ms} ms")));
    parts.push(row.cost_usd.map_or_else(
        || "spend not recorded on its frames".to_owned(),
        |usd| format!("spend {}", nika_display::format::fmt_cost_usd(usd)),
    ));
    parts.extend(row.model.as_ref().map(|m| format!("model {m}")));
    parts.extend(row.tokens.map(|t| format!("{t} tokens")));
    let meters: Vec<String> = (row.meters().iter())
        .map(|(name, n)| format!("{name} {n}"))
        .collect();
    if !meters.is_empty() {
        parts.push(meters.join(", "));
    }
    parts.extend(row.attempts.map(|n| format!("{n} wire attempts")));
    if let Some(ms) = row.waited_ms {
        let on = row.retried_on.as_deref().unwrap_or("unknown");
        parts.push(format!("waited {ms} ms after {on}"));
    }
    parts.extend(row.agent.as_ref().map(AgentProgress::describe));
    (format!("measured{sep}{}", parts.join(sep)), Role::Dim)
}

/// What the task's frames said beside its state: why it failed, its note,
/// a warning, where an untrusted value was born, its identity hashes.
fn said(row: &TaskRow, sep: &str) -> Vec<(String, Role)> {
    let mut rows = Vec::new();
    if !row.detail.is_empty() {
        let tone = if row.state == TaskState::Failed {
            Role::Bad
        } else {
            Role::Dim
        };
        rows.push((format!("why{sep}{}", row.detail), tone));
    }
    if let Some(warning) = &row.warning {
        rows.push((format!("warning{sep}{warning}"), Role::Warn));
    }
    if let Some(source) = &row.integrity_source {
        rows.push((format!("untrusted value{sep}born at {source}"), Role::Warn));
    }
    let started = row.started_note.as_deref().filter(|n| *n != row.note);
    if let Some(note) = started {
        rows.push((format!("started as{sep}{note}"), Role::Dim));
    }
    if !row.note.is_empty() {
        rows.push((format!("note{sep}{}", row.note), Role::Dim));
    }
    let short = |h: &str| h.chars().take(12).collect::<String>();
    if let (Some(def), Some(input)) = (&row.def_hash, &row.input_hash) {
        rows.push((
            format!("definition {}{sep}inputs {}", short(def), short(input)),
            Role::Dim,
        ));
    }
    rows
}
