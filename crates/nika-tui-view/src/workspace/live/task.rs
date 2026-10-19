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

use crate::{Availability, Canvas, Content, Meta};
use nika_display::state::{AgentProgress, TaskRow, TaskState};
use nika_display::theme::Role;
use ratatui::text::{Line, Span};

use super::acquired::ChildRead;
use super::child::{ChildView, openable};
use super::faces::lines_of;
use super::{Binding, LiveRun};
use crate::lang::{Voice, fill};
use crate::visual::{role, state};
use crate::workspace::text::{fit_head, marks};
use nika_display::run_story::ChildRun;
use nika_display::run_story::ExecutionId;

/// The task picked in the run in view, and whether its detail is open.
#[derive(Clone, Debug, Default)]
pub struct Pick {
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
pub enum Origin {
    /// A task of the bytes the run names.
    Declared,
    /// A cleanup unit of the bytes the run names (`finally`).
    Cleanup,
    /// A task only the stream named.
    Observed,
}

impl Pick {
    /// Nothing picked by a key, no detail open.
    #[must_use]
    pub const fn new() -> Self {
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
    #[must_use]
    pub fn current<'a>(&'a self, leg: &'a LiveRun) -> Option<&'a str> {
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
    pub fn step(&mut self, leg: &LiveRun, down: bool) -> bool {
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
    pub fn open(&mut self, leg: &LiveRun, scroll: usize) -> bool {
        let id = self.current(leg).map(str::to_owned);
        let (Some(execution), Some(id)) = (leg.execution(), id) else {
            return false;
        };
        self.task = Some((execution, id));
        self.open = Some(scroll);
        true
    }

    /// Close the detail: the list's scroll to return to, when one was open.
    pub fn close(&mut self) -> Option<usize> {
        self.open.take()
    }

    /// Whether the picked task's detail is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Whether the scroll still has to follow a moved pick (once).
    pub fn follows(&mut self) -> bool {
        std::mem::take(&mut self.follow)
    }

    /// Open the child journal the picked task's relation names, from its
    /// open detail, keeping the detail's `scroll`; `false` when there is
    /// none to open.
    pub fn open_child(&mut self, leg: &LiveRun, scroll: usize) -> bool {
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
    pub fn close_child(&mut self) -> Option<usize> {
        self.child.take().map(|view| view.scroll())
    }

    /// Whether a child journal is in view.
    #[must_use]
    pub fn child_open(&self) -> bool {
        self.child.is_some()
    }

    /// The child view, when one is open.
    #[must_use]
    pub fn child_view(&self) -> Option<&ChildView> {
        self.child.as_ref()
    }

    /// Which opening the open child view is, when one is open.
    pub fn child_opening(&self) -> Option<u64> {
        self.child.as_ref().map(ChildView::opening)
    }

    /// What the open child view still needs read (its task and relation),
    /// while that relation is still the task's own and unread this reading.
    #[must_use]
    pub fn child_wanted(&self, leg: &LiveRun) -> Option<(String, ChildRun)> {
        self.child.as_ref()?.wanted(leg)
    }

    /// Keep `read` for the open child view when it answers that very view
    /// (asked by this opening, for its task and relation, still current);
    /// `false` when it is dropped.
    pub fn child_read(
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
    /// The fold's rows in the order the run executes them: bound to the bytes
    /// it names, the graph's order (its waves, then the declaration order
    /// within each, cleanup units last), the rows it does not name after; else
    /// the order the rows started, a row that never started keeping its
    /// first-seen place after them. Never the frames' scheduling order, which
    /// is alphabetical (live gauntlet 4, r06 B12, r08 B20).
    #[must_use]
    pub fn rows_in_order(&self) -> Vec<&TaskRow> {
        let rows = self.view.rows();
        if self.bound_graph().is_some() {
            return (self.listed().into_iter())
                .filter_map(|(id, _)| rows.iter().find(|row| row.id == id))
                .collect();
        }
        let mut started: Vec<(usize, &TaskRow)> = rows.iter().enumerate().collect();
        // Stable: equal starts, and the rows that never started, keep their first-seen order.
        started.sort_by_key(|(_, row)| (row.started_ms.is_none(), row.started_ms));
        started.into_iter().map(|(_, row)| row).collect()
    }

    /// The tasks of the run in list order, each with where it comes from.
    #[must_use]
    pub fn listed(&self) -> Vec<(&str, Origin)> {
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
    #[must_use]
    pub fn bound_graph(&self) -> Option<(&nika_display::dag_art::GraphDoc, &[Vec<usize>])> {
        if self.binding != Binding::Same {
            return None;
        }
        self.look.as_ref()?.graph()
    }

    /// One row of the task list: the pick mark, then the task as the fold
    /// has it, or not observed when no frame named it.
    #[must_use]
    pub fn list_row(
        &self,
        (id, origin): (&str, Origin),
        picked: bool,
        canvas: Canvas,
        voice: Voice,
    ) -> Line<'static> {
        let (sep, cut) = marks(canvas.ascii);
        let mark = match (picked, canvas.ascii) {
            (false, _) => "  ",
            (true, false) => "› ",
            (true, true) => "* ",
        };
        let unit = if origin == Origin::Cleanup {
            format!("{sep}{}", voice.say("cleanup (finally)"))
        } else {
            String::new()
        };
        let (text, tone) = if let Some(row) = self.row(id) {
            let (shown, closed) = self.shown(row.state);
            let reads = self.reads_as(id, shown);
            let (glyph, tone) = state::cell(reads, canvas.ascii);
            let ms = row
                .wall_ms()
                .map_or(String::new(), |ms| format!("{sep}{ms} ms"));
            let open = closed.map_or(String::new(), |closed| {
                format!("{sep}{}", voice.say(closed.words(false)))
            });
            // The stop glyph alone would not say whose stop: the person's « no » is named.
            let refused = if reads == shown {
                String::new()
            } else {
                format!("{sep}{}", voice.say("answered no"))
            };
            (format!("{mark}{glyph} {id}{ms}{open}{refused}{unit}"), tone)
        } else {
            let unseen = voice.say("not observed in this run");
            (format!("{mark}{id}{unit}{sep}{unseen}"), Role::Dim)
        };
        Line::from(Span::styled(
            fit_head(&text, usize::from(canvas.width), cut),
            role::style(tone, canvas.color),
        ))
    }

    /// The detail of task `id`: the title names it and the way back, the
    /// body reads its row and, when bound, its node, each phrase said in
    /// `voice` with its values set in after.
    #[must_use]
    pub fn detail(
        &self,
        id: &str,
        canvas: Canvas,
        voice: Voice,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        let (sep, cut) = marks(canvas.ascii);
        let cells = usize::from(canvas.width);
        let back = voice.say("task {id}{sep}Backspace: back to the tasks");
        let head = format!(
            "{}{sep}{}",
            self.head(voice),
            fill(back, &[("{id}", id), ("{sep}", sep)])
        );
        let title = Line::from(Span::styled(
            fit_head(&head, cells, cut),
            role::style(Role::Strong, canvas.color),
        ));
        let mut rows = self.standing_of(id, sep, voice);
        rows.extend(self.statics(id, sep, voice));
        let Some(row) = self.row(id) else {
            return (title, lines_of(&rows, cells, canvas.ascii, canvas.color));
        };
        rows.push(measured(row, sep, voice));
        rows.extend(said(row, sep, voice));
        rows.extend(harness_media(self.view.harness_media(id), voice));
        rows.extend(self.child_section(id, sep, voice));
        let mut body = lines_of(&rows, cells, canvas.ascii, canvas.color);
        body.push(Line::default());
        body.extend(self.task_output(id, row, canvas, voice));
        (title, body)
    }

    /// The task's state as the fold has it, or why it has none, in `voice`.
    fn standing_of(&self, id: &str, sep: &str, voice: Voice) -> Vec<(String, Role)> {
        let Some(row) = self.row(id) else {
            let words = voice.say(
                "{id}{sep}not observed in this run: the bound graph declares it, no frame of this run named it",
            );
            return vec![(fill(words, &[("{id}", id), ("{sep}", sep)]), Role::Dim)];
        };
        let (shown, closed) = self.shown(row.state);
        let words = match row.state {
            _ if closed.is_some() => closed.map_or("", |closed| closed.words(true)),
            TaskState::Pending => "scheduled, not started",
            TaskState::Running => "running, not finished",
            TaskState::Ok if row.cached => "succeeded from the cache of an earlier leg (resume)",
            TaskState::Ok if self.refused_at(id) => "answered no: the run stopped there",
            TaskState::Ok => "succeeded",
            TaskState::Failed => "failed",
            TaskState::Retrying => "an attempt failed, a retry is scheduled",
            TaskState::Skipped => "skipped: its condition was false",
            TaskState::Cancelled => "cancelled: it never ran to its end",
            TaskState::Paused => "paused: a gate asks a human",
        };
        let words = voice.say(words);
        let repaired = if row.recovered {
            format!("{sep}{}", voice.say("repaired by its on_error recovery"))
        } else {
            String::new()
        };
        let (glyph, tone) = state::cell(self.reads_as(id, shown), voice.ascii);
        vec![(
            format!(
                "{glyph} {id}{sep}{words}{repaired}{sep}{}",
                voice.say(self.source_words())
            ),
            tone,
        )]
    }

    /// What the bound bytes declare of task `id`, or why nothing is lent, in `voice`: the
    /// language's own words (`when`, `for_each`, `retry`, …) stay as written.
    fn statics(&self, id: &str, sep: &str, voice: Voice) -> Vec<(String, Role)> {
        let unknown = |why: &str| vec![(fill(voice.say(why), &[("{sep}", sep)]), Role::Dim)];
        let Some((doc, _)) = self.bound_graph() else {
            return unknown("static facts unknown{sep}the graph is not bound to this run");
        };
        let Some(node) = doc.nodes.iter().find(|n| n.id == id) else {
            return unknown("static facts unknown{sep}the bound graph does not declare it");
        };
        let mut rows = Vec::new();
        if node.kind == "finally" {
            let parents: Vec<&str> = (doc.edges.iter())
                .filter(|e| e.kind == "finally" && e.to == id)
                .map(|e| e.from.as_str())
                .collect();
            let words = voice
                .say("cleanup (finally){sep}runs when {parents} unwinds, never as a main task");
            let parents = parents.join(", ");
            rows.push((
                fill(words, &[("{sep}", sep), ("{parents}", &parents)]),
                Role::Dim,
            ));
        }
        let mut facts = vec![node.verb.to_owned()];
        facts.extend(node.tool.clone());
        facts.extend(node.model.clone());
        facts.extend(node.when.as_ref().map(|w| format!("when {w}")));
        facts.extend(node.fan_out.as_ref().map(|f| match f.count {
            Some(n) => fill(
                voice.say("for_each {kind} of {n}"),
                &[("{kind}", f.kind), ("{n}", &n.to_string())],
            ),
            None => format!("for_each {}", f.kind),
        }));
        facts.extend((node.retry_max_attempts).map(|n| {
            voice
                .say("retry {n} attempts")
                .replace("{n}", &n.to_string())
        }));
        facts.extend(node.timeout_ms.map(|ms| format!("timeout {ms} ms")));
        facts.extend(node.on_error.map(|e| format!("on_error {e}")));
        if !node.outputs.is_empty() {
            facts.push(format!("outputs {}", node.outputs.join(", ")));
        }
        if !node.permits.is_empty() {
            facts.push(format!("permits {}", node.permits.join(", ")));
        }
        let facts = facts.join(sep);
        let words = voice.say("declared{sep}{facts}{sep}the bytes the run names");
        rows.push((
            fill(words, &[("{sep}", sep), ("{facts}", &facts)]),
            Role::Dim,
        ));
        rows
    }

    /// The output the stream (a kept leg: its journal) carried, through the
    /// viewers, or why there is none; then a fan-out's items the same way.
    fn task_output(
        &self,
        id: &str,
        row: &TaskRow,
        canvas: Canvas,
        voice: Voice,
    ) -> Vec<Line<'static>> {
        let (sep, _) = marks(canvas.ascii);
        let cells = usize::from(canvas.width);
        let Some(json) = row.output_json.as_deref() else {
            let medium = voice.say(self.medium());
            let why = match row.state {
                TaskState::Pending
                | TaskState::Running
                | TaskState::Retrying
                | TaskState::Paused => "not finished: no output yet",
                TaskState::Failed => "no output {medium}: the task failed",
                TaskState::Skipped | TaskState::Cancelled => {
                    "no output {medium}: it never ran to its end"
                }
                TaskState::Ok => {
                    "no output {medium}: its frame carried none (a value withheld as secret, or an older engine)"
                }
            };
            let why = voice.say(why).replace("{medium}", medium);
            let said = fill(
                voice.say("output{sep}{why}"),
                &[("{sep}", sep), ("{why}", &why)],
            );
            return lines_of(&[(said, Role::Dim)], cells, canvas.ascii, canvas.color);
        };
        let mut lines = self.task_value("output", id, json, (canvas, voice));
        if let Some(items) = row.items_json.as_deref() {
            lines.push(Line::default());
            lines.extend(self.task_value("items", id, items, (canvas, voice)));
        }
        lines
    }

    /// One JSON value the stream (or a kept leg's journal) carried for task
    /// `id`, by the viewers.
    fn task_value(
        &self,
        name: &str,
        id: &str,
        json: &str,
        (canvas, voice): (Canvas, Voice),
    ) -> Vec<Line<'static>> {
        let (sep, _) = marks(canvas.ascii);
        let name = voice.say(name);
        let mut meta = Meta::new(name);
        meta.format = Some("json".to_owned());
        let label = self.label_in(voice.lang);
        let whence = voice.say("{label}{sep}task {id}");
        meta.provenance = Some(fill(
            whence,
            &[("{label}", &label), ("{sep}", sep), ("{id}", id)],
        ));
        meta.protected = true;
        meta.availability = Availability::Present;
        let content = if json.trim() == "null" {
            Content::Null
        } else {
            Content::Text(json)
        };
        let rendered = crate::artifact(content, &meta, canvas);
        let size = json.len().to_string();
        let words = fill(
            voice.say("{name}{sep}{n} bytes {medium}"),
            &[
                ("{name}", name),
                ("{sep}", sep),
                ("{n}", &size),
                ("{medium}", voice.say(self.medium())),
            ],
        );
        let mut lines = lines_of(
            &[(words, Role::Strong)],
            usize::from(canvas.width),
            canvas.ascii,
            canvas.color,
        );
        lines.extend(rendered.head(canvas));
        lines.extend(rendered.lines);
        lines
    }
}

/// What the stream measured of a task, each absent meter left out and said, in `voice`.
fn measured(row: &TaskRow, sep: &str, voice: Voice) -> (String, Role) {
    let mut parts = Vec::new();
    parts.extend(row.wall_ms().map(|ms| format!("{ms} ms")));
    parts.push(row.cost_usd.map_or_else(
        || voice.say("spend not recorded on its frames").to_owned(),
        |usd| {
            let usd = nika_display::format::fmt_cost_usd(usd);
            voice.say("spend {usd}").replace("{usd}", &usd)
        },
    ));
    parts.extend(
        row.model
            .as_ref()
            .map(|m| voice.say("model {m}").replace("{m}", m)),
    );
    parts.extend(
        row.tokens
            .map(|t| voice.say("{t} tokens").replace("{t}", &t.to_string())),
    );
    let meters: Vec<String> = (row.meters().iter())
        .map(|(name, n)| format!("{} {n}", voice.say(name)))
        .collect();
    if !meters.is_empty() {
        parts.push(meters.join(", "));
    }
    parts.extend((row.attempts).map(|n| {
        voice
            .say("{n} wire attempts")
            .replace("{n}", &n.to_string())
    }));
    if let Some(ms) = row.waited_ms {
        let on = row
            .retried_on
            .as_deref()
            .unwrap_or_else(|| voice.say("unknown"));
        let words = voice.say("waited {ms} ms after {on}");
        parts.push(fill(words, &[("{ms}", &ms.to_string()), ("{on}", on)]));
    }
    parts.extend(row.agent.as_ref().map(AgentProgress::describe));
    let parts = parts.join(sep);
    let words = voice.say("measured{sep}{parts}");
    (
        fill(words, &[("{sep}", sep), ("{parts}", &parts)]),
        Role::Dim,
    )
}

/// What the task's frames said beside its state: why it failed, its note,
/// a warning, where an untrusted value was born, its identity hashes; each
/// heading in `voice`, the frames' own words as they came.
fn said(row: &TaskRow, sep: &str, voice: Voice) -> Vec<(String, Role)> {
    let headed = |heading: &str, value: &str| {
        let words = voice.say(heading);
        fill(words, &[("{sep}", sep), ("{value}", value)])
    };
    let mut rows = Vec::new();
    if !row.detail.is_empty() {
        let tone = if row.state == TaskState::Failed {
            Role::Bad
        } else {
            Role::Dim
        };
        rows.push((headed("why{sep}{value}", &row.detail), tone));
    }
    if let Some(warning) = &row.warning {
        rows.push((headed("warning{sep}{value}", warning), Role::Warn));
    }
    if let Some(source) = &row.integrity_source {
        let words = headed("untrusted value{sep}born at {value}", source);
        rows.push((words, Role::Warn));
    }
    let started = row.started_note.as_deref().filter(|n| *n != row.note);
    if let Some(note) = started {
        rows.push((headed("started as{sep}{value}", note), Role::Dim));
    }
    if !row.note.is_empty() {
        rows.push((headed("note{sep}{value}", &row.note), Role::Dim));
    }
    let short = |h: &str| h.chars().take(12).collect::<String>();
    if let (Some(def), Some(input)) = (&row.def_hash, &row.input_hash) {
        let words = voice.say("definition {def}{sep}inputs {input}");
        let (def, input) = (short(def), short(input));
        let words = fill(
            words,
            &[("{def}", &def), ("{sep}", sep), ("{input}", &input)],
        );
        rows.push((words, Role::Dim));
    }
    rows
}

/// Describe received evidence only; a peer path never becomes a clickable/readable file here.
/// The fold keeps a sample: the exact total and any missing frame are always named.
fn harness_media(raw: Option<&str>, voice: Voice) -> Vec<(String, Role)> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let value = serde_json::from_str::<serde_json::Value>(raw).unwrap_or_default();
    let (Some(items), Some(observed)) = (value["images"].as_array(), value["observed"].as_u64())
    else {
        let unreadable = voice.say("Image report unreadable; no file verified");
        return vec![(unreadable.to_owned(), Role::Warn)];
    };
    let mut rows: Vec<_> = items.iter().map(|item| image_row(item, voice)).collect();
    let observed_n = observed.to_string();
    if observed > rows.len() as u64 {
        let shown = rows.len().to_string();
        let words = voice
            .say("Showing {shown} of {observed} image observations; the rest are in the trace");
        let words = fill(words, &[("{shown}", &shown), ("{observed}", &observed_n)]);
        rows.push((words, Role::Dim));
    }
    if value["complete"] != true {
        let expected = (value["expected"].as_u64())
            .map_or_else(|| voice.say("no count").to_owned(), |n| n.to_string());
        let words = voice
            .say("Image evidence incomplete: observed {observed}, terminal reports {expected}");
        let words = fill(
            words,
            &[("{observed}", &observed_n), ("{expected}", &expected)],
        );
        rows.push((words, Role::Warn));
    }
    rows
}

fn image_row(item: &serde_json::Value, voice: Voice) -> (String, Role) {
    let image = &item["image"];
    let mime = (image["mime_type"].as_str()).unwrap_or_else(|| voice.say("MIME unknown"));
    let received = match (
        image["received_bytes"].as_u64(),
        image["blob"]["hash"].as_str(),
    ) {
        (Some(size), Some(hash)) => fill(
            voice.say("{size} bytes received · stored blob {hash}"),
            &[("{size}", &size.to_string()), ("{hash}", hash)],
        ),
        (Some(size), None) if image["storage"] == "failed" => voice
            .say("{size} bytes received · storage failed")
            .replace("{size}", &size.to_string()),
        (Some(size), None) => voice
            .say("{size} bytes received · storage unconfirmed (no store answer observed)")
            .replace("{size}", &size.to_string()),
        (None, _) => voice
            .say("reported path only; no image data received")
            .to_owned(),
    };
    let path = image["reported_saved_path"]
        .as_str()
        .map_or(String::new(), |p| {
            let path = p.escape_default().to_string();
            voice
                .say(" · reported path {path}")
                .replace("{path}", &path)
        });
    let words = voice.say(
        "Image reported · {mime} · harness source · {received}{path} · file not verified; permission evidence is separate",
    );
    let text = fill(
        words,
        &[
            ("{mime}", mime),
            ("{received}", &received),
            ("{path}", &path),
        ],
    );
    (text, Role::Dim)
}

#[cfg(test)]
mod media_detail_tests {
    use super::*;

    fn text(raw: &serde_json::Value) -> String {
        let rows = harness_media(Some(&raw.to_string()), Voice::default());
        rows.into_iter()
            .map(|row| row.0)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn media_detail_distinguishes_bytes_from_a_path_and_never_claims_file_proof() {
        for (data, blob, expected) in [
            (
                serde_json::json!(68),
                serde_json::json!({"hash": "blake3:ab"}),
                "stored blob",
            ),
            (
                serde_json::json!(68),
                serde_json::Value::Null,
                "storage unconfirmed",
            ),
            (
                serde_json::json!(68),
                serde_json::json!("failed"),
                "storage failed",
            ),
            (
                serde_json::Value::Null,
                serde_json::Value::Null,
                "reported path only",
            ),
        ] {
            let (blob, storage) = if blob == "failed" {
                (serde_json::Value::Null, "failed")
            } else {
                (blob, "")
            };
            let image = serde_json::json!({"mime_type": "image/png", "received_bytes": data,
                "blob": blob, "storage": storage, "reported_saved_path": "/unverified/file.png"});
            let raw = serde_json::json!({"images": [{"image": image}], "observed": 1,
                "expected": 1, "complete": true});
            let text = text(&raw);
            assert!(text.contains(expected), "{text}");
            assert!(text.contains("file not verified"));
            assert!(text.contains("image/png"));
            assert!(text.contains("harness source"));
            assert!(!text.contains("incomplete"));
        }
        assert!(
            harness_media(Some("broken"), Voice::default())[0]
                .0
                .contains("unreadable")
        );
        assert!(
            harness_media(Some("[]"), Voice::default())[0]
                .0
                .contains("unreadable")
        );
    }

    #[test]
    fn media_sample_names_its_total_and_never_hides_missing_frames() {
        let row = serde_json::json!({"image": {"reported_saved_path": "/a.png"}});
        let raw = serde_json::json!({"images": [row.clone(), row.clone(), row.clone(), row],
            "observed": 255, "expected": 256, "complete": false});
        let rendered = text(&raw);
        assert!(rendered.contains("Showing 4 of 255"), "{rendered}");
        assert!(rendered.contains("incomplete: observed 255, terminal reports 256"));
        let raw = serde_json::json!({"images": [], "observed": 1, "expected": null,
            "complete": false});
        assert!(text(&raw).contains("terminal reports no count"));
    }
}
