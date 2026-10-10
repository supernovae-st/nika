// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The child run a task of the run in view called, from the relation that
//! task's settle frame named, and its journal opened one level down.
//!
//! The relation is an observation of the parent's frame: never proof that
//! the parent's journal includes it, nor that the journal it names is that
//! child's. The host reads the journal by the whole name the frame gave,
//! the verifier judges those bytes, and each engagement the frame made is
//! compared and said: the head the verifier computes, the source its one
//! start names, the outcome its terminal says. The frame names no child
//! execution and no length, so those stay not compared and the relation is
//! never called bound whole. A verified journal is not a business success;
//! sealed is not verified.
//!
//! The parent stays the run in view: its frames keep folding, its pick,
//! face, scroll and draft are kept, and leaving the child (`Backspace`)
//! reads nothing again. A relation that changed since the view opened (a
//! new attempt, none any more, or the same one emitted again) leaves the
//! view without a reading. One level only: the child's own tasks are
//! listed, never opened from here.

use nika_display::run_story::{ChildOutcome, ChildRun};
use nika_display::state::TaskRow;
use nika_display::theme::Role;
use nika_tui_view::Canvas;
use ratatui::text::{Line, Span};

use super::LiveRun;
use super::faces::{lines_of, verdict};
use crate::session::acquire::ChildRead;
use crate::visual::{role, state};
use crate::workspace::text::{fit_head, marks};

/// The child journal opened from the picked task's detail.
#[derive(Clone, Debug)]
pub(crate) struct ChildView {
    /// The task whose settle frame named the relation.
    task: String,
    /// The relation as the fold had it when the view opened.
    relation: ChildRun,
    /// How many times the task's relation had changed then.
    epoch: u64,
    /// The detail's scroll to return to.
    scroll: usize,
    /// Which opening this is: closing and opening the same child again is
    /// another one, whose answer is never the earlier one's.
    opening: u64,
    /// What the host read, and for which reading of the leg.
    read: Option<(u64, ChildRead)>,
}

/// Whether a relation's journal can be opened: it names one, and an outcome
/// the reader recognises; otherwise why not.
pub(super) fn openable(relation: &ChildRun) -> Result<(), &'static str> {
    if relation.trace_id.as_deref().is_none_or(str::is_empty) {
        return Err("the frame names no journal: nothing to open");
    }
    if relation.outcome.is_none() {
        return Err("outcome not recognized: nothing is opened on it");
    }
    Ok(())
}

impl ChildView {
    pub(super) fn new(
        task: String,
        relation: ChildRun,
        (epoch, opening): (u64, u64),
        scroll: usize,
    ) -> Self {
        Self {
            task,
            relation,
            epoch,
            scroll,
            opening,
            read: None,
        }
    }

    /// Which opening this is.
    pub(super) fn opening(&self) -> u64 {
        self.opening
    }

    /// The detail's scroll to return to.
    pub(super) fn scroll(&self) -> usize {
        self.scroll
    }

    /// Whether the relation is still the one the view opened on: the same
    /// value, never changed in between.
    fn current(&self, leg: &LiveRun) -> bool {
        leg.relation_epoch(&self.task) == self.epoch
            && leg.child(&self.task) == Some(&self.relation)
    }

    /// The reading this leg's current reading holds, if any.
    fn reading(&self, leg: &LiveRun) -> Option<&ChildRead> {
        let (generation, read) = self.read.as_ref()?;
        (*generation == leg.generation() && self.current(leg)).then_some(read)
    }

    /// What still needs reading: the task and relation, while current.
    pub(super) fn wanted(&self, leg: &LiveRun) -> Option<(String, ChildRun)> {
        (self.current(leg) && self.reading(leg).is_none())
            .then(|| (self.task.clone(), self.relation.clone()))
    }

    /// Keep `read` when it answers this very view (asked by this opening,
    /// for its task and relation, still current); `false` when dropped.
    pub(super) fn keep(
        &mut self,
        leg: &LiveRun,
        (opening, task, relation): (Option<u64>, &str, &ChildRun),
        read: ChildRead,
    ) -> bool {
        if opening != Some(self.opening)
            || task != self.task
            || *relation != self.relation
            || !self.current(leg)
        {
            return false;
        }
        self.read = Some((leg.generation(), read));
        true
    }
}

/// A hash, short.
fn short(hash: Option<&str>) -> String {
    hash.map_or_else(|| "none".to_owned(), |h| h.chars().take(12).collect())
}

impl LiveRun {
    /// What the picked task's detail says of the child run it called: the
    /// relation its settle frame named and whether its journal opens.
    pub(super) fn child_section(&self, id: &str, sep: &str) -> Vec<(String, Role)> {
        let Some(relation) = self.child(id) else {
            let calls = (self.bound_graph())
                .and_then(|(doc, _)| doc.nodes.iter().find(|n| n.id == id))
                .and_then(|n| n.tool.as_deref())
                .is_some_and(|tool| tool.starts_with("workflow:"));
            return if calls {
                vec![(
                    format!(
                        "child run{sep}no relation on its frames: nothing to open (the producer keeps none when the child fails)"
                    ),
                    Role::Dim,
                )]
            } else {
                Vec::new()
            };
        };
        let named = match relation.outcome {
            Some(ChildOutcome::Success) => "success",
            Some(ChildOutcome::Failure) => "failure",
            _ => "none recognized",
        };
        let mut rows = vec![
            (
                format!(
                    "child run{sep}{}{sep}as this task's settle frame names it",
                    relation.target
                ),
                Role::Strong,
            ),
            (
                format!(
                    "named{sep}journal {}{sep}head {}{sep}source {}{sep}outcome {named}",
                    relation.trace_id.as_deref().unwrap_or("none"),
                    short(relation.chain_head.as_deref()),
                    short(relation.def_hash.as_deref()),
                ),
                Role::Dim,
            ),
        ];
        rows.push(match openable(relation) {
            Ok(()) => ("Enter: open its journal".to_owned(), Role::Accent),
            Err(why) => (why.to_owned(), Role::Warn),
        });
        rows
    }

    /// The child view: its title names the way back; its body the relation,
    /// the reading (or why there is none) and the child's own tasks.
    pub(super) fn child_lines(
        &self,
        view: &ChildView,
        canvas: Canvas,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        let (sep, cut) = marks(canvas.ascii);
        let cells = usize::from(canvas.width);
        let head = format!(
            "Backspace: back{sep}child {}{sep}{}",
            view.relation.target,
            self.head(canvas.ascii)
        );
        let title = Line::from(Span::styled(
            fit_head(&head, cells, cut),
            role::style(Role::Strong, canvas.color),
        ));
        let trace = view.relation.trace_id.as_deref().unwrap_or("none");
        let mut rows = vec![(
            format!(
                "relation{sep}task {}'s settle frame in this run names {} and journal {trace}",
                view.task, view.relation.target
            ),
            Role::Dim,
        )];
        if !view.current(self) {
            rows.push((
                "the task's relation changed since this view opened (a new attempt, or none now): its reading is set aside; Backspace returns to the task".to_owned(),
                Role::Warn,
            ));
            return (title, lines_of(&rows, cells, canvas.ascii, canvas.color));
        }
        let Some(read) = view.reading(self) else {
            rows.push((
                "its journal is captured and verified when this view opens".to_owned(),
                Role::Dim,
            ));
            return (title, lines_of(&rows, cells, canvas.ascii, canvas.color));
        };
        rows.extend(said(read, sep));
        let mut body = lines_of(&rows, cells, canvas.ascii, canvas.color);
        if read.proven().why().is_none() {
            body.push(Line::default());
            body.extend(lines_of(
                &[(
                    format!("its tasks{sep}folded from the same bytes{sep}one level: their children are not opened here"),
                    Role::Dim,
                )],
                cells,
                canvas.ascii,
                canvas.color,
            ));
            body.extend(read.rows().iter().map(|row| task_row(row, canvas)));
        }
        (title, body)
    }
}

/// What a reading says, in the order a reader checks it.
fn said(read: &ChildRead, sep: &str) -> Vec<(String, Role)> {
    let proven = read.proven();
    if let Some(why) = proven.why() {
        return vec![(format!("not read{sep}{why}"), Role::Warn)];
    }
    let mut rows = Vec::new();
    verdict(proven, sep, &mut rows);
    for (holds, words) in read.compared() {
        rows.push((words.clone(), if *holds { Role::Dim } else { Role::Warn }));
    }
    let terminal = proven
        .terminal()
        .unwrap_or("none: the journal holds no terminal frame");
    rows.push((format!("terminal frame{sep}{terminal}"), Role::Dim));
    rows.push((
        "a relation in the parent's frame is not proof the parent's journal holds it; a verified journal records what happened, never that the work was right".to_owned(),
        Role::Dim,
    ));
    rows
}

/// One of the child's tasks as its fold has it.
fn task_row(row: &TaskRow, canvas: Canvas) -> Line<'static> {
    let (sep, cut) = marks(canvas.ascii);
    let (glyph, tone) = state::cell(row.state, canvas.ascii);
    let ms = row
        .wall_ms()
        .map_or(String::new(), |ms| format!("{sep}{ms} ms"));
    Line::from(Span::styled(
        fit_head(
            &format!("  {glyph} {}{ms}", row.id),
            usize::from(canvas.width),
            cut,
        ),
        role::style(tone, canvas.color),
    ))
}
