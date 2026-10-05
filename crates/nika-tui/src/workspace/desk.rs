// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workspace the shell keeps between frames: the project view the
//! conversation last lent, the keyboard focus, the workflow opened as the
//! object in view, the look its Session took of that workflow and the face in
//! view. It assembles one [`Screen`] from them and routes a key by a fixed
//! precedence (the shell decides `Ctrl+C` and `Ctrl+T` before it):
//!
//! | key                        | a region other than the composer   | the composer's region           |
//! |----------------------------|------------------------------------|---------------------------------|
//! | `F6` · `Shift+F6`          | next · previous region             | next · previous region          |
//! | `Esc`                      | back to the composer               | leaves to inline, draft intact  |
//! | `PgUp` · `PgDn`            | the object scrolls (object region) | the transcript scrolls          |
//! | arrows, `Home` · `End`     | aside selection · object scroll    | the composer (history, cursor)  |
//! | `Up` · `Down` (run tasks)  | the previous · next task picked    | the composer (history)          |
//! | `Left` · `Right` (aside)   | Nika · Files                       | the composer (cursor)           |
//! | `Left` · `Right` (object)  | the previous · next face           | the composer (cursor)           |
//! | `r` (object)               | the Session reads the file again   | the composer                    |
//! | `Enter`                    | opens the aside entry · the task   | sends the line                  |
//! | `Backspace` (task detail)  | back to the run's tasks            | the composer (erases)           |
//! | `Tab` and any other key    | ignored                            | the composer (`Tab` completes)  |
//!
//! On the run face of the run in view, the task list takes `Up`/`Down` (the
//! page keys still scroll) and `Enter` opens the picked task's detail, where
//! the arrows scroll; the pick is the task's id in that run's execution,
//! never a painted line, and the list's scroll comes back with `Backspace`.
//!
//! Opening a workflow asks its Session for one look ([`Route::Inspect`]): the
//! shell takes it from the conversation, never while drawing, and hands it
//! back ([`Desk::took`]); while a turn holds the conversation the look waits
//! for the turn's end. A look is shown only for the workflow it was taken of.
//!
//! Below [`super::geometry::MIN_SIZE`] the shell draws the focus view: every
//! key then behaves as in the composer's region, and the focus kept here
//! returns with the workspace when the size allows. Nothing here reads a
//! file, a clock or the environment.

use crossterm::event::{KeyCode, KeyEvent};
use nika_tui_view::Face;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;

use super::candidate::Proposed;
use super::live::{LiveRun, Pick, RunFace, Want};
use crate::model::Conversation;
use crate::session::acquire::{ChildRead, Fetched, Proven};
use crate::session::feed::{Gap, Observed};
use nika_display::run_story::ExecutionId;
use nika_session::KeptRun;

/// How many past legs of runs the desk keeps beside the one in flight.
const PAST_LEGS: usize = 4;
use super::focus::{Action, Extent, Focus, Region};
use super::geometry::Geometry;
use super::inspect::Inspected;
use super::object::{Object, Paint};
use super::project::{self, Opened, ProjectView, Target};
use super::screen::{self, Screen};
use crate::composer::Composer;
use crate::model::UiState;

/// What a key asks of the shell once the workspace has read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum Route {
    /// The key belongs to the composer: hand it over unchanged.
    Compose,
    /// Leave the workspace for inline, the draft intact.
    Leave,
    /// Scroll the conversation's transcript one page back.
    Older,
    /// Scroll the conversation's transcript one page forward.
    Newer,
    /// The focus, a selection, a scroll or the object in view changed.
    Repaint,
    /// Nothing changes.
    Nothing,
    /// The opened workflow needs a look from its Session: the shell takes
    /// one and hands it back ([`Desk::took`]).
    Inspect,
}

/// The workspace's own state.
#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct Desk {
    /// The project the conversation last lent; `None` when it lends none.
    pub(crate) view: Option<ProjectView>,
    /// The keyboard focus, the aside selection and projection, the object scroll.
    pub(crate) focus: Focus,
    /// What the aside opened as the object in view (a workflow, the
    /// pinned run); the welcome while `None` or no longer listed.
    pub(crate) opened: Option<Target>,
    /// The look the Session took of the opened workflow, when it took one.
    pub(crate) look: Option<Inspected>,
    /// The face of the look in view.
    pub(crate) face: Face,
    /// A look was asked while a turn held the conversation: it is taken when
    /// the turn ends.
    pub(crate) wants_look: bool,
    /// The candidate the conversation proposes, as it lent it last.
    pub(crate) candidate: Option<Proposed>,
    /// The run leg the shell observes (the latest asked), and the legs before it.
    pub(crate) live: Option<LiveRun>,
    pub(crate) past: Vec<LiveRun>,
    /// The face of the run in view.
    pub(crate) run_face: RunFace,
    /// The task picked in the run in view, and whether its detail is open.
    pick: Pick,
    /// The run an earlier session kept was offered once, at the opening.
    kept_seen: bool,
    /// The face of the look as the viewers last rendered it, and for what.
    drawn: Option<Drawn>,
}

/// The reading a request was made for: the leg's reading, and the child
/// view opening it asked for when it asks a child's journal. An answer is
/// applied only to the same reading, and a child's only to that opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reading {
    generation: u64,
    opening: Option<u64>,
}

/// One thing acquired for a run's face.
#[derive(Debug)]
pub(crate) enum Got {
    /// A file the run reported writing, as read now.
    File(Fetched),
    /// The run's journal, captured and verified.
    Proof(Proven),
    /// A child run's journal, read for the relation it was asked by.
    Child {
        task: String,
        relation: nika_display::run_story::ChildRun,
        read: Box<ChildRead>,
    },
}

/// Ask `conversation`'s host for each of `wants` of the leg `execution` (on
/// the shell's worker, never while drawing); what it cannot acquire is a
/// refusal with its reason, never an absence.
pub(crate) fn acquire_all<C: Conversation + ?Sized>(
    conversation: &mut C,
    execution: &ExecutionId,
    wants: Vec<Want>,
) -> Vec<Got> {
    (wants.into_iter())
        .map(|want| match want {
            Want::File(path) => Got::File(
                (conversation.fetch(execution, &path))
                    .unwrap_or_else(|| Fetched::refused(&path, "this conversation reads no file")),
            ),
            Want::Proof => {
                Got::Proof((conversation.prove(execution)).unwrap_or_else(|| {
                    Proven::refused("", "this conversation verifies no journal")
                }))
            }
            Want::Child { task, relation } => {
                let read = (conversation.child(execution, &task, &relation)).unwrap_or_else(|| {
                    ChildRead::refused("", "this conversation reads no child journal")
                });
                Got::Child {
                    task,
                    relation,
                    read: Box::new(read),
                }
            }
        })
        .collect()
}

/// One face of one look rendered for one region: what the frame paints.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Drawn {
    /// What was rendered (a look's path, or a candidate's identity) and the
    /// witness of its bytes, the face, the region's width, the glyph column
    /// and the colour it was rendered for.
    key: (String, Option<String>, Face, RunFace, u16, bool, bool),
    title: Line<'static>,
    body: Vec<Line<'static>>,
    /// The body line of the picked task, when the run's list shows one.
    picked: Option<usize>,
    /// The object rows the body was last shown in (not part of the key: a
    /// new height shows the same lines in another viewport).
    rows: usize,
}

/// Move `scroll` the least that shows `line` among `rows` rows.
fn follow(scroll: &mut usize, line: usize, rows: usize) {
    if line < *scroll {
        *scroll = line;
    } else if line >= *scroll + rows {
        *scroll = line + 1 - rows;
    }
}

/// What a rendering of `candidate` is keyed by: its identity and standing.
fn candidate_key(candidate: &Proposed) -> String {
    format!("proposal {} {}", candidate.id().as_str(), candidate.aside())
}

impl Default for Desk {
    fn default() -> Self {
        Self::new()
    }
}

impl Desk {
    /// No project known yet, the composer has the keys, nothing opened.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            view: None,
            focus: Focus::composing(),
            opened: None,
            look: None,
            face: Face::Source,
            wants_look: false,
            candidate: None,
            live: None,
            past: Vec::new(),
            run_face: RunFace::Run,
            pick: Pick::new(),
            kept_seen: false,
            drawn: None,
        }
    }

    /// Whether a run is pinned: the leg the shell observes, else the view's.
    pub(crate) fn pins(&self) -> bool {
        self.live.is_some() || self.view.as_ref().is_some_and(|v| v.pinned.is_some())
    }

    /// What the shell observed of a run during a turn. A request starts a
    /// new leg (the leg before it is kept as history) and becomes the object
    /// in view; a frame folds into the leg in flight. `true` when anything
    /// was observed.
    pub(crate) fn observe(&mut self, seen: impl Iterator<Item = Observed>) -> bool {
        let mut any = false;
        for observed in seen {
            any = true;
            match observed {
                Observed::Asked {
                    workflow,
                    resume,
                    typed,
                    look,
                } => {
                    if let Some(leg) = self.live.take() {
                        self.past.insert(0, leg);
                        self.past.truncate(PAST_LEGS);
                    }
                    self.live = Some(LiveRun::asked(workflow, resume, typed, look.map(|l| *l)));
                    self.opened = Some(Target::Live);
                    self.run_face = RunFace::Run;
                    self.pick = Pick::new();
                    self.focus.scroll = 0;
                }
                Observed::Frame(frame) => {
                    if let Some(leg) = self.live.as_mut() {
                        leg.apply(frame);
                    }
                }
            }
        }
        any
    }

    /// The end of a turn: every observation still queued is folded before
    /// the turn's result is handled (a settlement already sent is never
    /// dropped), then what the queue could not carry is recorded.
    pub(crate) fn close_turn(&mut self, queued: &std::sync::mpsc::Receiver<Observed>, gap: &Gap) {
        self.observe(queued.try_iter());
        self.lost(gap.dropped(), gap.unread());
    }

    /// What the turn's queue could not carry, to the leg in flight.
    pub(crate) fn lost(&mut self, dropped: usize, unread: usize) {
        if let (Some(leg), true) = (self.live.as_mut(), dropped + unread > 0) {
            leg.lost(dropped, unread);
        }
    }

    /// What the object in view renders from, keyed: the candidate under
    /// review, or the look of the opened workflow when it is that file's.
    fn rendered_from(&self) -> Option<(String, Option<String>)> {
        match self.shown()? {
            Opened::Candidate(c) => Some((candidate_key(c), c.witness().map(str::to_owned))),
            Opened::Live(leg) => {
                let pick = (
                    self.pick.current(leg),
                    self.pick.is_open(),
                    self.pick.child_open(),
                );
                Some((format!("{} {} {pick:?}", leg.label(), leg.revision()), None))
            }
            Opened::Workflow(w) => (self.look.as_ref())
                .filter(|l| l.path() == w.path)
                .map(|l| (l.path().to_owned(), l.witness().map(str::to_owned))),
            _ => None,
        }
    }

    /// Render the face in view for a terminal of `size`, before the frame and
    /// only when the look or the candidate, the face, the region's width, the
    /// glyph column or the colour changed: drawing paints these lines and
    /// calls no viewer.
    pub(crate) fn prepare(&mut self, size: (u16, u16), ascii: bool, color: bool) {
        let pinned = self.pins();
        let area = Rect::new(0, 0, size.0, size.1);
        let (Some((from, witness)), Some(geometry)) =
            (self.rendered_from(), Geometry::of(area, pinned))
        else {
            return;
        };
        let width = geometry.object.width;
        let rows = usize::from(geometry.object.height.saturating_sub(1)).max(1);
        let key = (from, witness, self.face, self.run_face, width, ascii, color);
        if let Some(drawn) = self.drawn.as_mut().filter(|d| d.key == key) {
            // The same lines in another viewport: a picked task stays in view.
            if drawn.rows != rows {
                drawn.rows = rows;
                if let Some(line) = drawn.picked {
                    follow(&mut self.focus.scroll, line, rows);
                }
            }
            return;
        }
        let resized = (self.drawn.as_ref()).is_some_and(|d| d.key.4 != width || d.rows != rows);
        let mut picked_line = None;
        let lines = match (self.shown(), &self.look) {
            (Some(Opened::Candidate(c)), _) => c.face_lines(self.face, width, ascii, color),
            (Some(Opened::Live(leg)), _) => {
                let lines = leg.view(self.run_face, &self.pick, width, ascii, color);
                if self.run_face == RunFace::Run && !self.pick.is_open() {
                    // The list is the run face's last rows, one per task.
                    let listed = leg.listed();
                    let at = (self.pick.current(leg))
                        .and_then(|id| listed.iter().position(|(l, _)| *l == id));
                    picked_line = at.map(|at| lines.1.len().saturating_sub(listed.len()) + at);
                }
                lines
            }
            (_, Some(look)) => look.face_lines(self.face, width, ascii, color),
            (_, None) => return,
        };
        if let (Some(line), true) = (picked_line, self.pick.follows() || resized) {
            // A moved pick, or one a new size shows elsewhere, stays in
            // view: the scroll follows it once, never the page keys.
            follow(&mut self.focus.scroll, line, rows);
        }
        let (title, body) = lines;
        self.drawn = Some(Drawn {
            key,
            title,
            body,
            picked: picked_line,
            rows,
        });
    }

    /// The run in view, what its face still needs acquired, and the reading
    /// it is for (with the child opening it asks for), when it needs something.
    pub(crate) fn wanted(&self) -> Option<(ExecutionId, Vec<Want>, Reading)> {
        let Some(Opened::Live(leg)) = self.shown() else {
            return None;
        };
        let mut wants = leg.wants(self.run_face);
        let mut opening = None;
        if self.run_face == RunFace::Run
            && let Some((task, relation)) = self.pick.child_wanted(leg)
        {
            wants.push(Want::Child { task, relation });
            opening = self.pick.child_opening();
        }
        let reading = Reading {
            generation: leg.generation(),
            opening,
        };
        (!wants.is_empty()).then_some((leg.execution()?, wants, reading))
    }

    /// What was acquired for the leg `execution` at `reading`: applied only
    /// while that leg and that reading are still the ones in the desk (`false`
    /// when it came too late and was dropped); a child's journal only to the
    /// opening that asked it.
    pub(crate) fn acquired(
        &mut self,
        execution: ExecutionId,
        reading: Reading,
        got: Vec<Got>,
    ) -> bool {
        let current = |l: &&mut LiveRun| {
            l.execution() == Some(execution) && l.generation() == reading.generation
        };
        let Some(leg) = self.live.as_mut().filter(current) else {
            return false;
        };
        for item in got {
            match item {
                Got::File(fetched) => leg.fetched(fetched),
                Got::Proof(proven) => leg.proven(proven),
                // Applied only to the child in view, for that relation and
                // while it is still the task's own (never one re-emitted).
                Got::Child {
                    task,
                    relation,
                    read,
                } => {
                    let asked = (reading.opening, task.as_str(), &relation);
                    self.pick.child_read(leg, asked, *read);
                }
            }
        }
        self.drawn = None;
        true
    }

    /// The last run an earlier session kept, offered once at the opening:
    /// the object in view and the pinned leg while no run of this session
    /// is; never run, its proof read again only when that face opens. An
    /// unreadable record shows nothing here (the conversation says why).
    pub(crate) fn kept(&mut self, kept: Option<Result<KeptRun, String>>) {
        if std::mem::replace(&mut self.kept_seen, true) || self.live.is_some() {
            return;
        }
        let Some(Ok(run)) = kept else {
            return;
        };
        if let Some(execution) = crate::session::legs::execution_of(&run) {
            self.live = Some(LiveRun::kept(run, execution));
            self.opened = Some(Target::Live);
            self.run_face = RunFace::Run;
            self.pick = Pick::new();
            self.drawn = None;
        }
    }

    /// The candidate the conversation lends after a batch of beats. A new
    /// identity (a proposal, a revision, a set aside) becomes the object in
    /// view at its top, in the face already in view when a candidate was;
    /// the same candidate changes nothing. When the candidate in view leaves,
    /// the workflow the view now lists at its path is opened (a yes landed
    /// it: a fresh look reads the file), else the welcome returns.
    pub(crate) fn proposed(&mut self, next: Option<Proposed>) {
        if next == self.candidate {
            return;
        }
        // The same identity with other facts (its rehearsal words): rendered
        // anew where it is, never taken back from another object in view.
        let identity = |c: &Proposed| (c.id().clone(), c.aside());
        if next.as_ref().map(identity) == self.candidate.as_ref().map(identity) {
            self.drawn = None;
            self.candidate = next;
            return;
        }
        let showing = self.opened == Some(Target::Candidate);
        let left = self.candidate.take();
        if let Some(candidate) = &next {
            if !showing {
                self.face = candidate.initial_face();
            }
            self.opened = Some(Target::Candidate);
            self.look = None;
            self.focus.scroll = 0;
        } else if showing {
            let path = left.map(|c| c.path().to_owned());
            let listed = path.filter(|p| self.view.as_ref().and_then(|v| v.workflow(p)).is_some());
            self.wants_look = listed.is_some();
            self.opened = listed.map(Target::Workflow);
            self.look = None;
            self.face = Face::Source;
            self.focus.scroll = 0;
        }
        self.drawn = None;
        self.candidate = next;
    }

    /// The path of the opened workflow, while the view still lists it.
    #[must_use]
    pub(crate) fn opened_workflow(&self) -> Option<&str> {
        match self.shown()? {
            Opened::Workflow(workflow) => Some(&workflow.path),
            _ => None,
        }
    }

    /// The look the Session took for `path`: kept only while `path` is the
    /// opened workflow, so a look never stands for another file. `None` from
    /// a conversation that takes no look leaves the listing's facts in view.
    pub(crate) fn took(&mut self, path: &str, look: Option<Inspected>) {
        self.wants_look = false;
        if self.opened_workflow() != Some(path) {
            return;
        }
        if look.as_ref().map(Inspected::witness) != self.look.as_ref().map(Inspected::witness) {
            // Other bytes: the reading starts at their top.
            self.focus.scroll = 0;
        }
        // Every accepted look is rendered anew: two looks may share a key
        // (unread twice for different reasons, the same bytes judged anew).
        self.drawn = None;
        self.look = look.filter(|l| l.path() == path);
    }

    /// The object in view for `opened`: the faces rendered for it (a
    /// workflow's look, the candidate), else the facts the view holds of it.
    fn object_of(&self, opened: Opened<'_>, ascii: bool) -> Object {
        let current = self.rendered_from();
        let drawn = self.drawn.as_ref().filter(|d| {
            current.as_ref() == Some(&(d.key.0.clone(), d.key.1.clone())) && d.key.2 == self.face
        });
        match drawn {
            Some(drawn) => Object::Workflow {
                title: drawn.title.clone(),
                body: drawn.body.clone(),
            },
            None => opened.object(ascii),
        }
    }

    /// The object opened, when the current view or candidate still holds it.
    fn shown(&self) -> Option<Opened<'_>> {
        project::resolve(
            self.view.as_ref(),
            self.opened.as_ref(),
            self.candidate.as_ref(),
            self.live.as_ref(),
        )
    }

    /// Whether the object in view is the welcome (nothing opened).
    #[must_use]
    #[cfg(test)]
    pub(crate) fn welcoming(&self) -> bool {
        self.shown().is_none()
    }

    /// One frame's regions, from the view, the focus and the opened workflow;
    /// the words the workspace composes follow the glyph column (`ascii`).
    #[must_use]
    pub(crate) fn screen(&self, ascii: bool) -> Screen {
        let view = self.view.as_ref();
        let shown = self.shown();
        let object = shown.map_or_else(
            || project::welcome(view, ascii),
            |s| self.object_of(s, ascii),
        );
        let label = shown.map(Opened::label);
        let opened = shown.and(self.opened.as_ref());
        let candidate = self.candidate.as_ref();
        let screen = Screen::new(
            project::place(view),
            project::aside(view, self.focus.tab, opened, candidate, self.live.as_ref()),
            object,
            project::thread(view, label.as_deref()),
        );
        let project = view.map_or("", |v| v.name.as_str());
        let pinned = (self.live.as_ref().map(|leg| leg.pinned(project)))
            .or_else(|| view.and_then(|v| v.pinned.clone()));
        match pinned {
            Some(run) => screen.pinning(run),
            None => screen,
        }
    }

    /// What the regions hold on a terminal of `size`; `None` below the minimum.
    #[must_use]
    pub(crate) fn extent(&self, size: (u16, u16)) -> Option<Extent> {
        // The counts the keys move over are the same in both glyph columns.
        screen::extent(&self.screen(false), Rect::new(0, 0, size.0, size.1))
    }

    /// Route one key on a terminal of `size` (the shell has already taken
    /// `Ctrl+C` and `Ctrl+T`).
    pub(crate) fn route(&mut self, key: KeyEvent, size: (u16, u16)) -> Route {
        let Some(extent) = self.extent(size) else {
            // The focus view stands in: the composer's region, whatever the
            // focus kept for the workspace's return.
            return composer_route(key);
        };
        if self.focus.region == Region::Object
            && let Some(route) = self.task_key(key.code)
        {
            return route;
        }
        match self.focus.handle(key, extent) {
            Action::Compose => composer_route(key),
            Action::Moved => Route::Repaint,
            Action::Open(index) => {
                let route = self.open(index);
                let pinned = self.pins();
                let area = Rect::new(0, 0, size.0, size.1);
                let folded = Geometry::of(area, pinned).is_some_and(|g| g.aside.is_none());
                if folded && self.focus.region == Region::Aside && self.opened.is_some() {
                    // The folded aside stands over the object: the object it
                    // opened takes the keys, so it shows.
                    self.focus.region = Region::Object;
                }
                route
            }
            Action::Face(next) => self.turn(next),
            Action::Again if self.opened_workflow().is_some() => Route::Inspect,
            // A run in view is read again: its files and journal as they are now.
            Action::Again if matches!(self.shown(), Some(Opened::Live(_))) => {
                if let Some(leg) = self.live.as_mut() {
                    leg.forget();
                }
                self.drawn = None;
                if self.wanted().is_some() {
                    Route::Inspect
                } else {
                    Route::Repaint
                }
            }
            _ => Route::Nothing,
        }
    }

    /// A key of the run face's task list or of the picked task's detail;
    /// `None` leaves it to the focus (the page keys, the faces, `Esc`).
    fn task_key(&mut self, code: KeyCode) -> Option<Route> {
        if self.run_face != RunFace::Run || !matches!(self.shown(), Some(Opened::Live(_))) {
            return None;
        }
        let leg = self.live.as_ref()?;
        let open = self.pick.is_open();
        if !open && leg.listed().is_empty() {
            // No task to pick: the arrows scroll the facts as before.
            return None;
        }
        let moved = match code {
            KeyCode::Up | KeyCode::Down if !open => self.pick.step(leg, code == KeyCode::Down),
            KeyCode::Enter if !open => {
                let opened = self.pick.open(leg, self.focus.scroll);
                if opened {
                    self.focus.scroll = 0;
                }
                opened
            }
            KeyCode::Enter if !self.pick.child_open() => {
                if !self.pick.open_child(leg, self.focus.scroll) {
                    return Some(Route::Nothing);
                }
                self.focus.scroll = 0;
                // Its journal is read outside the frame, on the worker.
                return Some(Route::Inspect);
            }
            KeyCode::Enter => false,
            KeyCode::Backspace => match self.pick.close_child().or_else(|| self.pick.close()) {
                Some(scroll) => {
                    self.focus.scroll = scroll;
                    true
                }
                None => false,
            },
            _ => return None,
        };
        Some(if moved {
            Route::Repaint
        } else {
            Route::Nothing
        })
    }

    /// Show the next face of the look, the candidate or the run in view (or
    /// the previous one), from its top; nothing turns while none is in view.
    /// A run's face that needs acquiring asks for it (outside the frame).
    fn turn(&mut self, next: bool) -> Route {
        if let Some(Opened::Live(_)) = self.shown() {
            let at = RunFace::ALL
                .iter()
                .position(|f| *f == self.run_face)
                .unwrap_or(0);
            let step = if next { 1 } else { RunFace::ALL.len() - 1 };
            self.run_face = RunFace::ALL[(at + step) % RunFace::ALL.len()];
            self.focus.scroll = 0;
            return if self.wanted().is_some() {
                Route::Inspect
            } else {
                Route::Repaint
            };
        }
        let candidate = matches!(self.shown(), Some(Opened::Candidate(_)));
        if !candidate && (self.look.is_none() || self.opened_workflow().is_none()) {
            return Route::Nothing;
        }
        let at = Face::ALL.iter().position(|f| *f == self.face).unwrap_or(0);
        let step = if next { 1 } else { Face::ALL.len() - 1 };
        self.face = Face::ALL[(at + step) % Face::ALL.len()];
        self.focus.scroll = 0;
        Route::Repaint
    }

    /// Open the aside entry at `index`: a workflow or the pinned run becomes
    /// the object in view (scrolled to its top), this conversation gives the
    /// keys back to its composer. A workflow asks its Session for a look
    /// (opened again, it is read again). Nothing is attached to the next
    /// message.
    fn open(&mut self, index: usize) -> Route {
        let (candidate, live) = (self.candidate.as_ref(), self.live.as_ref());
        match project::target(self.view.as_ref(), self.focus.tab, index, candidate, live) {
            Some(Target::Conversation) => {
                self.focus.region = Region::Conversation;
                Route::Repaint
            }
            Some(target) => {
                if self.opened.as_ref() != Some(&target) {
                    self.look = None;
                    self.face = Face::Source;
                    self.focus.scroll = 0;
                }
                let workflow = matches!(target, Target::Workflow(_));
                self.opened = Some(target);
                if workflow {
                    Route::Inspect
                } else {
                    Route::Repaint
                }
            }
            None => Route::Nothing,
        }
    }

    /// A fresh entry into the workspace: the composer has the keys; the
    /// object, the selection and the projection stay as they were left.
    pub(crate) fn enter(&mut self) {
        self.focus.region = Region::Conversation;
    }
}

/// A key in the composer's region of a full screen (the workspace's, or the
/// focus view's whole screen): `Esc` leaves, the page keys scroll the
/// transcript, the composer takes everything else.
pub(crate) fn composer_route(key: KeyEvent) -> Route {
    match key.code {
        KeyCode::Esc => Route::Leave,
        KeyCode::PageUp => Route::Older,
        KeyCode::PageDown => Route::Newer,
        _ => Route::Compose,
    }
}

/// Draw the workspace on the whole frame; `false`, drawing nothing, below
/// the minimum (the caller draws the focus view there).
pub(crate) fn draw(
    frame: &mut Frame<'_>,
    desk: &Desk,
    paint: Paint,
    state: &UiState,
    composer: &Composer,
) -> bool {
    screen::draw(
        frame,
        &desk.screen(paint.ascii),
        paint,
        &desk.focus,
        state,
        composer,
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;
    use crate::model::demo_project;
    use crate::workspace::aside::Tab;
    use crate::workspace::object::Object;
    use nika_display::run_story::RunFrame;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn demo() -> Desk {
        let mut desk = Desk::new();
        desk.view = Some(demo_project());
        desk
    }

    const WIDE: (u16, u16) = (120, 40);
    const SMALL: (u16, u16) = (80, 24);
    const TINY: (u16, u16) = (59, 20);

    /// The composer's region: Esc leaves, the page keys scroll the
    /// transcript, Tab and the rest are the composer's.
    #[test]
    fn the_composer_region_leaves_on_esc_and_scrolls_the_transcript() {
        let mut desk = demo();
        assert_eq!(desk.route(key(KeyCode::Esc), WIDE), Route::Leave);
        assert_eq!(desk.route(key(KeyCode::PageUp), WIDE), Route::Older);
        assert_eq!(desk.route(key(KeyCode::PageDown), WIDE), Route::Newer);
        for code in [
            KeyCode::Tab,
            KeyCode::Char('y'),
            KeyCode::Enter,
            KeyCode::Up,
        ] {
            assert_eq!(desk.route(key(code), WIDE), Route::Compose, "{code:?}");
        }
    }

    /// Esc in another region returns to the composer first; a second Esc
    /// leaves. The page keys scroll the object when it has the keys.
    #[test]
    fn esc_climbs_the_ladder_one_region_at_a_time() {
        let mut desk = demo();
        assert_eq!(
            desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE),
            Route::Repaint
        );
        assert_eq!(desk.focus.region, Region::Aside);
        assert_eq!(desk.route(key(KeyCode::Char('x')), WIDE), Route::Nothing);
        assert_eq!(desk.route(key(KeyCode::Esc), WIDE), Route::Repaint);
        assert_eq!(desk.focus.region, Region::Conversation);
        assert_eq!(desk.route(key(KeyCode::Esc), WIDE), Route::Leave);
        // At 80 columns the aside is folded, yet reachable: Shift+F6 reaches it
        // (drawn over the object), then the object.
        desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), SMALL);
        assert_eq!(desk.focus.region, Region::Aside);
        desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), SMALL);
        assert_eq!(desk.focus.region, Region::Object);
        assert_eq!(desk.route(key(KeyCode::PageUp), SMALL), Route::Nothing);
    }

    /// Enter on a workflow opens it as the object and attaches nothing; Enter
    /// on this conversation gives the keys back to its composer.
    #[test]
    fn the_aside_opens_a_workflow_without_attaching_it() {
        let mut desk = demo();
        assert!(desk.welcoming());
        desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
        desk.route(key(KeyCode::Down), WIDE);
        desk.route(key(KeyCode::Down), WIDE);
        assert_eq!(
            desk.route(key(KeyCode::Enter), WIDE),
            Route::Inspect,
            "a workflow asks its Session for a look"
        );
        assert_eq!(
            desk.opened,
            Some(Target::Workflow("enrich.nika".to_owned()))
        );
        assert!(!desk.welcoming());
        let screen = desk.screen(false);
        assert!(matches!(&screen.object, Object::Shown { name, .. } if name == "enrich"));
        assert_eq!(screen.thread.on_screen.as_deref(), Some("enrich.nika"));
        assert!(screen.thread.attached.is_empty());
        assert!(screen.aside.entries[2].open);
        assert_eq!(desk.focus.region, Region::Aside, "opening keeps the aside");
        desk.route(key(KeyCode::Home), WIDE);
        assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Repaint);
        assert_eq!(desk.focus.region, Region::Conversation);
        assert_eq!(
            desk.opened,
            Some(Target::Workflow("enrich.nika".to_owned())),
            "the object stays"
        );
    }

    /// A pinned run is the aside's last entry; Enter opens it as the object,
    /// and once the view no longer pins it the welcome returns.
    #[test]
    fn the_pinned_run_opens_as_the_object_while_it_is_pinned() {
        use crate::workspace::pinned::Pinned;
        use nika_display::state::TaskState;
        let mut desk = demo();
        let run = Pinned::new(
            "demo",
            "digest-notes.nika",
            "#1",
            TaskState::Paused,
            "waiting",
        );
        desk.view = desk.view.take().map(|view| view.pinning(run));
        desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
        desk.route(key(KeyCode::End), WIDE);
        assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Repaint);
        assert_eq!(desk.opened, Some(Target::Run("#1".to_owned())));
        let screen = desk.screen(false);
        assert!(
            matches!(&screen.object, Object::Shown { name, .. } if name == "#1 digest-notes.nika")
        );
        assert_eq!(
            screen.thread.on_screen.as_deref(),
            Some("#1 digest-notes.nika")
        );
        assert!(screen.aside.entries.last().is_some_and(|e| e.open));
        assert!(screen.pinned.is_some());
        desk.view = Some(demo_project());
        assert!(
            desk.welcoming(),
            "the run settled: nothing claims it on screen"
        );
    }

    /// The Files projection lists nothing and says why; Enter opens nothing.
    #[test]
    fn the_files_projection_opens_nothing() {
        let mut desk = demo();
        desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
        assert_eq!(desk.route(key(KeyCode::Right), WIDE), Route::Repaint);
        assert_eq!(desk.focus.tab, Tab::Files);
        assert!(desk.screen(false).aside.entries.is_empty());
        assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Nothing);
        assert!(desk.welcoming());
    }

    /// Below the minimum the focus view stands in: keys act as in the
    /// composer's region and the kept focus returns with the workspace.
    #[test]
    fn below_the_minimum_the_focus_view_keys_apply_and_the_focus_is_kept() {
        let mut desk = demo();
        desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
        assert_eq!(desk.focus.region, Region::Aside);
        assert_eq!(desk.extent(TINY), None);
        assert_eq!(desk.route(key(KeyCode::F(6)), TINY), Route::Compose);
        assert_eq!(desk.route(key(KeyCode::PageUp), TINY), Route::Older);
        assert_eq!(desk.route(key(KeyCode::Esc), TINY), Route::Leave);
        assert_eq!(desk.focus.region, Region::Aside, "kept for the return");
        desk.enter();
        assert_eq!(desk.focus.region, Region::Conversation);
    }

    /// A workflow the next view no longer lists falls back to the welcome,
    /// and the thread stops naming it as on screen.
    #[test]
    fn a_workflow_the_view_no_longer_lists_is_not_claimed_on_screen() {
        let mut desk = demo();
        desk.opened = Some(Target::Workflow("gone.nika".to_owned()));
        assert!(desk.welcoming());
        let screen = desk.screen(false);
        assert_eq!(screen.thread.on_screen, None);
        assert!(matches!(screen.object, Object::Welcome { .. }));
    }

    /// A look taken of another file than the opened one is never shown; the
    /// opened workflow shows its own look, face by face, from the cache the
    /// shell prepares before the frame.
    #[test]
    fn a_look_is_shown_only_for_the_workflow_it_was_taken_of() {
        let mut desk = demo();
        desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
        let other = refused("release.nika", "aaaa");
        desk.took("release.nika", Some(other));
        assert!(desk.look.is_none(), "not the opened workflow");
        let look = refused("enrich.nika", "bbbbbbbbbbbbcccc");
        desk.took("enrich.nika", Some(look));
        assert!(desk.look.is_some());
        // Not prepared yet: the listing's facts stand in, never a stale face.
        assert!(matches!(desk.screen(false).object, Object::Shown { .. }));
        desk.prepare(WIDE, true, false);
        let Object::Workflow { title, body } = desk.screen(false).object else {
            panic!("the look is in view");
        };
        assert!(title.to_string().contains("[source]"), "{title}");
        assert!(body.iter().any(|l| l.to_string().contains("bbbbbbbbbbbb")));
        // The face turns in the object region; the cache follows it.
        desk.focus.region = Region::Object;
        assert_eq!(desk.route(key(KeyCode::Right), WIDE), Route::Repaint);
        assert_eq!(desk.face, Face::Plan);
        desk.prepare(WIDE, true, false);
        let Object::Workflow { title, .. } = desk.screen(false).object else {
            panic!("the look is in view");
        };
        assert!(title.to_string().contains("[plan]"), "{title}");
        assert_eq!(desk.route(key(KeyCode::Left), WIDE), Route::Repaint);
        assert_eq!(desk.face, Face::Source);
        assert_eq!(desk.route(key(KeyCode::Left), WIDE), Route::Repaint);
        assert_eq!(desk.face, Face::Check);
        assert_eq!(desk.route(key(KeyCode::Char('r')), WIDE), Route::Inspect);
    }

    /// New bytes of the opened workflow replace the look and the cache: the
    /// frame never paints the old face for the new witness.
    #[test]
    fn new_bytes_replace_the_look_and_its_rendering() {
        let mut desk = demo();
        desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
        desk.took("enrich.nika", Some(refused("enrich.nika", "111111111111")));
        desk.prepare(WIDE, true, false);
        desk.took("enrich.nika", Some(refused("enrich.nika", "222222222222")));
        let shown = desk.screen(false).object;
        assert!(
            matches!(shown, Object::Shown { .. }),
            "the old rendering is not lent to the new bytes"
        );
        desk.prepare(WIDE, true, false);
        let Object::Workflow { body, .. } = desk.screen(false).object else {
            panic!("the new look is in view");
        };
        let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
        assert!(rows.iter().any(|r| r.contains("222222222222")), "{rows:?}");
        assert!(!rows.iter().any(|r| r.contains("111111111111")), "{rows:?}");
    }

    /// Opening another workflow drops the previous look and starts at its
    /// source; no face turns while no look is in view.
    #[test]
    fn opening_another_workflow_drops_the_previous_look() {
        let mut desk = demo();
        desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
        desk.route(key(KeyCode::Down), WIDE);
        assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Inspect);
        desk.took("release.nika", Some(refused("release.nika", "abc")));
        desk.face = Face::Graph;
        desk.route(key(KeyCode::Down), WIDE);
        assert_eq!(desk.route(key(KeyCode::Enter), WIDE), Route::Inspect);
        assert!(desk.look.is_none());
        assert_eq!(desk.face, Face::Source);
        desk.focus.region = Region::Object;
        assert_eq!(desk.route(key(KeyCode::Right), WIDE), Route::Nothing);
    }

    /// Two unread looks of the same path share every key the cache reads:
    /// the second one's reason is shown, never the first one's.
    #[test]
    fn a_new_unread_look_is_rendered_anew() {
        let mut desk = demo();
        desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
        desk.took(
            "enrich.nika",
            Some(Inspected::unread("enrich.nika", "not found")),
        );
        desk.prepare(WIDE, true, false);
        let reason = "larger than 1048576 bytes";
        desk.took(
            "enrich.nika",
            Some(Inspected::unread("enrich.nika", reason)),
        );
        desk.prepare(WIDE, true, false);
        let Object::Workflow { body, .. } = desk.screen(false).object else {
            panic!("the look is in view");
        };
        let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
        assert!(rows.iter().any(|r| r.contains(reason)), "{rows:?}");
        assert!(!rows.iter().any(|r| r.contains("not found")), "{rows:?}");
    }

    /// The face is rendered for the width the frame will have: a smaller
    /// terminal renders it again, never paints the wider lines.
    #[test]
    fn the_face_is_rendered_again_for_a_new_width() {
        let mut desk = demo();
        desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
        desk.took("enrich.nika", Some(refused("enrich.nika", "abcdef")));
        desk.prepare((160, 48), true, false);
        let wide = desk.drawn.as_ref().map(|d| d.key.4);
        desk.prepare((60, 18), true, false);
        let narrow = desk.drawn.as_ref().map(|d| d.key.4);
        assert_eq!((wide, narrow), (Some(69), Some(60)));
        let Object::Workflow { title, body } = desk.screen(false).object else {
            panic!("the look is in view");
        };
        for line in std::iter::once(&title).chain(body.iter()) {
            assert!(line.width() <= 60, "{line}");
        }
    }

    /// Where the width folds the aside, the object it opens takes the keys,
    /// so it is the object that shows, not the aside over it.
    #[test]
    fn opening_from_a_folded_aside_shows_the_object() {
        let mut desk = demo();
        desk.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), SMALL);
        assert_eq!(desk.focus.region, Region::Aside);
        desk.route(key(KeyCode::Down), SMALL);
        assert_eq!(desk.route(key(KeyCode::Enter), SMALL), Route::Inspect);
        assert_eq!(desk.focus.region, Region::Object);
        let mut wide = demo();
        wide.route(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), WIDE);
        wide.route(key(KeyCode::Down), WIDE);
        wide.route(key(KeyCode::Enter), WIDE);
        assert_eq!(
            wide.focus.region,
            Region::Aside,
            "a shown aside keeps the keys"
        );
    }

    /// A candidate `preview` names, over unjudged bytes landing at `path`.
    fn candidate(preview: &str, path: &str, aside: bool) -> Proposed {
        let look = Inspected::unjudged(path, format!("{preview}-bytes"), "nika: x\n".to_owned());
        Proposed::new(nika_session::ProposalId::of(preview), aside, look)
    }

    /// A proposal becomes the object in view, listed under this conversation;
    /// a revision (a new identity) replaces it in the face already in view,
    /// and the same candidate lent again changes nothing.
    #[test]
    fn a_new_candidate_becomes_the_object_and_a_revision_replaces_it() {
        let mut desk = demo();
        desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
        desk.proposed(Some(candidate("A", "compiled-workflow.nika", false)));
        assert_eq!(desk.opened, Some(Target::Candidate));
        assert_eq!(desk.face, Face::Source);
        let screen = desk.screen(false);
        assert_eq!(
            screen.aside.entries[1].label,
            "proposal compiled-workflow.nika"
        );
        assert!(screen.aside.entries[1].open);
        assert_eq!(
            screen.thread.on_screen.as_deref(),
            Some("proposal compiled-workflow.nika")
        );
        desk.prepare(WIDE, false, false);
        let Object::Workflow { body, .. } = desk.screen(false).object else {
            panic!("the candidate's face is in view");
        };
        let a = nika_session::ProposalId::of("A").to_string();
        assert!(body.iter().any(|l| l.to_string().contains(&a)));
        // The face turns on a candidate; a revision keeps it and shows B only.
        desk.focus.region = Region::Object;
        assert_eq!(desk.route(key(KeyCode::Right), WIDE), Route::Repaint);
        assert_eq!(desk.route(key(KeyCode::Char('r')), WIDE), Route::Nothing);
        desk.proposed(Some(candidate("B", "compiled-workflow.nika", false)));
        assert_eq!(desk.face, Face::Plan, "the face in view stays");
        desk.prepare(WIDE, false, false);
        let Object::Workflow { body, .. } = desk.screen(false).object else {
            panic!("B is in view");
        };
        let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
        let b = nika_session::ProposalId::of("B").to_string();
        assert!(rows.iter().any(|r| r.contains(&b)), "{rows:?}");
        assert!(!rows.iter().any(|r| r.contains(&a)), "A is gone: {rows:?}");
        let drawn = desk.drawn.clone();
        desk.proposed(Some(candidate("B", "compiled-workflow.nika", false)));
        assert_eq!(desk.drawn, drawn, "the same candidate changes nothing");
    }

    /// A candidate that leaves takes the object with it: the workflow the view
    /// now lists at its path is opened (and looked at), else the welcome.
    #[test]
    fn the_candidate_in_view_leaves_for_the_saved_workflow_or_the_welcome() {
        let mut desk = demo();
        desk.proposed(Some(candidate("A", "release.nika", false)));
        desk.proposed(None);
        assert_eq!(
            desk.opened,
            Some(Target::Workflow("release.nika".to_owned()))
        );
        assert!(desk.wants_look, "the saved file is looked at anew");
        let mut desk = demo();
        desk.proposed(Some(candidate("A", "compiled-workflow.nika", false)));
        desk.proposed(None);
        assert!(desk.welcoming(), "a discarded create leaves the welcome");
        assert!(!desk.wants_look);
        let mut desk = demo();
        desk.proposed(Some(candidate("A", "compiled-workflow.nika", false)));
        desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
        desk.proposed(None);
        assert_eq!(
            desk.opened,
            Some(Target::Workflow("enrich.nika".to_owned())),
            "another object in view stays"
        );
    }

    /// A run asked becomes the object in view, an aside entry and the pinned
    /// row, with no identity before its first frame; a second request (a
    /// resume) is a new leg and the first one is kept as history.
    #[test]
    fn an_asked_run_is_the_object_and_the_pinned_row() {
        let mut desk = demo();
        let asked = |resume| Observed::Asked {
            workflow: "release.nika".to_owned(),
            resume,
            typed: true,
            look: None,
        };
        assert!(!desk.observe(std::iter::empty()));
        assert!(desk.observe(std::iter::once(asked(false))));
        assert_eq!(desk.opened, Some(Target::Live));
        let screen = desk.screen(false);
        let pinned = screen.pinned.as_ref().expect("the run is pinned");
        assert_eq!(
            (pinned.run.as_str(), pinned.workflow.as_str()),
            ("run (starting)", "release.nika")
        );
        assert!(
            screen
                .aside
                .entries
                .iter()
                .any(|e| e.label == "run (starting) release.nika" && e.open)
        );
        desk.prepare(WIDE, false, false);
        let Object::Workflow { title, body } = desk.screen(false).object else {
            panic!("the run is in view");
        };
        assert!(title.to_string().contains("run (starting)"), "{title}");
        assert!(
            body.iter()
                .any(|l| l.to_string().contains("no run identity yet"))
        );
        desk.observe(std::iter::once(asked(true)));
        assert_eq!(desk.past.len(), 1, "the first leg is kept as history");
        desk.lost(2, 0);
        desk.prepare(WIDE, false, false);
        let Object::Workflow { body, .. } = desk.screen(false).object else {
            panic!("the resumed leg is in view");
        };
        let rows: Vec<String> = body.iter().map(ToString::to_string).collect();
        assert!(rows.iter().any(|r| r.contains("a resumed leg")), "{rows:?}");
        assert!(rows.join(" ").contains("2 lost on the way"), "{rows:?}");
    }

    /// The turn's result arrives after the child already queued its last
    /// frames: closing the turn folds every queued frame first, so the
    /// settlement already sent settles the leg, and the losses are recorded.
    #[test]
    fn closing_a_turn_folds_the_settlement_already_queued() {
        use nika_display::run_story::RunFrame;
        let mut desk = demo();
        let (tx, rx) = std::sync::mpsc::sync_channel(8);
        let gap = Gap::default();
        let frames = [
            r#"{"correlation":null,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"fields":[{"key":"workflow","value":"release"}],"id":{"uuid":"01a0ef11-03a1-73d9-a2bc-2548bdab1943"},"kind":"workflow_started","run":null,"timestamp":1}"#,
            r#"{"kind":"run_settled","status":"succeeded","cause":"normal","execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"spend":{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0},"evidence":"none"}"#,
        ];
        tx.send(Observed::Asked {
            workflow: "release.nika".to_owned(),
            resume: false,
            typed: true,
            look: None,
        })
        .expect("queued");
        for frame in frames {
            tx.send(Observed::Frame(RunFrame::decode(frame).expect("a frame")))
                .expect("queued");
        }
        drop(tx);
        desk.close_turn(&rx, &gap);
        let leg = desk.live.as_ref().expect("the leg");
        assert_eq!(
            leg.reported(),
            Some(nika_display::run_story::RunState::Succeeded)
        );
        assert!(leg.whole(), "nothing lost, settled");
    }

    /// The same candidate identity folded again with other facts is
    /// rendered anew where it is: it never takes the object back from another
    /// view the human opened. A new identity does.
    #[test]
    fn the_same_identity_with_other_facts_never_steals_the_view() {
        let mut desk = demo();
        desk.proposed(Some(candidate("A", "compiled-workflow.nika", false)));
        desk.opened = Some(Target::Workflow("enrich.nika".to_owned()));
        let renewed =
            candidate("A", "compiled-workflow.nika", false).rehearsed(Some("rehearsed".into()));
        desk.proposed(Some(renewed.clone()));
        assert_eq!(
            desk.opened,
            Some(Target::Workflow("enrich.nika".to_owned()))
        );
        assert_eq!(
            desk.candidate.as_ref(),
            Some(&renewed),
            "the facts are kept"
        );
        desk.proposed(Some(candidate("B", "compiled-workflow.nika", false)));
        assert_eq!(desk.opened, Some(Target::Candidate), "a new identity shows");
    }

    /// A conversation that counts what the desk asks it to acquire.
    #[derive(Default)]
    struct Lender {
        fetched: usize,
        proved: usize,
    }

    impl Conversation for Lender {
        fn open(&mut self) -> Vec<crate::model::Beat> {
            Vec::new()
        }
        fn submit(&mut self, _line: &str) -> crate::model::Turn {
            crate::model::Turn {
                beats: Vec::new(),
                handoff: None,
            }
        }
        fn perform(&mut self, _handoff: &crate::model::Handoff) -> Vec<crate::model::Beat> {
            Vec::new()
        }
        fn fetch(&mut self, _execution: &ExecutionId, path: &str) -> Option<Fetched> {
            self.fetched += 1;
            Some(Fetched::refused(path, "lent"))
        }
        fn prove(&mut self, _execution: &ExecutionId) -> Option<Proven> {
            self.proved += 1;
            Some(Proven::refused("", "lent"))
        }
    }

    /// A run's face turns in the object region; what it needs is acquired
    /// once, by the conversation, never while preparing the frame; `r` reads
    /// it again.
    #[test]
    fn a_run_face_is_acquired_outside_the_frame_and_read_again_on_demand() {
        let exec = r#""execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"}"#;
        let frame = |n: u32, kind: &str, fields: &str| {
            let line = format!(
                r#"{{"correlation":null,{exec},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
            );
            Observed::Frame(RunFrame::decode(&line).expect("frame"))
        };
        let settled = format!(
            r#"{{"kind":"run_settled","status":"succeeded","cause":"normal",{exec},"spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"evidence":"unsealed"}}"#
        );
        let starting =
            r#"{"key":"task","value":"save"},{"key":"note","value":"invoke · nika:write"}"#;
        let ending =
            r#"{"key":"task","value":"save"},{"key":"output","value":"\"./out/copy.md\""}"#;
        let mut desk = demo();
        desk.observe(
            [
                Observed::Asked {
                    workflow: "copy.nika".to_owned(),
                    resume: false,
                    typed: true,
                    look: None,
                },
                frame(1, "workflow_started", ""),
                frame(2, "task_started", starting),
                frame(3, "task_completed", ending),
                Observed::Frame(RunFrame::decode(&settled).expect("settled")),
            ]
            .into_iter(),
        );
        desk.focus.region = Region::Object;
        let mut lender = Lender::default();
        assert_eq!(
            desk.route(key(KeyCode::Right), WIDE),
            Route::Repaint,
            "outputs: held"
        );
        assert_eq!(
            desk.route(key(KeyCode::Right), WIDE),
            Route::Inspect,
            "files: to read"
        );
        desk.prepare(WIDE, false, false);
        assert_eq!(
            (lender.fetched, lender.proved),
            (0, 0),
            "drawing reads nothing"
        );
        assert!(acquire(&mut desk, &mut lender));
        assert!(!acquire(&mut desk, &mut lender), "read once");
        assert_eq!(desk.route(key(KeyCode::Char('r')), WIDE), Route::Inspect);
        // A reading asked before `r` arrives late: dropped, asked again.
        let (execution, wants, stale) = desk.wanted().expect("read again");
        let late = acquire_all(&mut lender, &execution, wants);
        assert_eq!(desk.route(key(KeyCode::Char('r')), WIDE), Route::Inspect);
        assert!(
            !desk.acquired(execution, stale, late),
            "a late reading is dropped"
        );
        assert!(acquire(&mut desk, &mut lender));
        assert_eq!(
            desk.route(key(KeyCode::Right), WIDE),
            Route::Inspect,
            "proof: to verify"
        );
        assert!(acquire(&mut desk, &mut lender));
        assert_eq!((lender.fetched, lender.proved), (3, 1));
    }

    /// What the shell's worker does, inline: acquire what the face wants,
    /// then hand it to the desk for the reading it was asked for.
    fn acquire(desk: &mut Desk, lender: &mut Lender) -> bool {
        let Some((execution, wants, generation)) = desk.wanted() else {
            return false;
        };
        let got = acquire_all(lender, &execution, wants);
        desk.acquired(execution, generation, got)
    }

    /// A refused look of `path`, read with `witness`.
    fn refused(path: &str, witness: &str) -> Inspected {
        Inspected::read(
            path,
            witness.to_owned(),
            "nika: x\nbogus: 1\n".to_owned(),
            Err(("NIKA-PARSE-005".to_owned(), "unknown key bogus".to_owned())),
        )
    }
    /// A run of thirty scheduled tasks, its last one picked, the object
    /// holding the keys.
    fn long_list() -> Desk {
        let exec = r#""execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"}"#;
        let frame = |n: u32, kind: &str, fields: &str| {
            let line = format!(
                r#"{{"correlation":null,{exec},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
            );
            Observed::Frame(RunFrame::decode(&line).expect("frame"))
        };
        let mut seen = vec![
            Observed::Asked {
                workflow: "long.nika".to_owned(),
                resume: false,
                typed: true,
                look: None,
            },
            frame(1, "workflow_started", ""),
        ];
        for n in 0..30 {
            let task = format!(r#"{{"key":"task","value":"step_{n:02}"}}"#);
            seen.push(frame(2 + n, "task_scheduled", &task));
        }
        let mut desk = demo();
        desk.observe(seen.into_iter());
        desk.focus.region = Region::Object;
        desk
    }

    /// The painted line of the picked task, and the object rows at `size`.
    fn picked_line(desk: &mut Desk, size: (u16, u16)) -> (usize, usize) {
        desk.prepare(size, false, false);
        let Object::Workflow { body, .. } = desk.screen(false).object else {
            panic!("the run is in view");
        };
        let at =
            (body.iter().position(|l| l.to_string().starts_with("› "))).expect("a task is picked");
        let rows = usize::from(desk.extent(size).expect("the workspace").object_rows);
        (at, rows)
    }

    /// Only the height changes: the cached lines stay the same, the viewport
    /// does not, and the picked task stays in view; a page key's scroll at
    /// an unchanged size is never pulled back.
    #[test]
    fn a_height_only_resize_keeps_the_picked_task_in_view() {
        const TALL: (u16, u16) = (120, 40);
        const SHORT: (u16, u16) = (120, 24);
        let mut desk = long_list();
        for _ in 0..29 {
            assert_eq!(desk.route(key(KeyCode::Down), TALL), Route::Repaint);
            desk.prepare(TALL, false, false);
        }
        let (at, rows) = picked_line(&mut desk, TALL);
        let scroll = desk.focus.scroll;
        assert!(
            scroll <= at && at < scroll + rows,
            "{at} in {scroll}+{rows}"
        );
        let key_before = desk.drawn.as_ref().map(|d| d.key.clone());
        let (at, rows) = picked_line(&mut desk, SHORT);
        assert_eq!(
            desk.drawn.as_ref().map(|d| d.key.clone()),
            key_before,
            "the same rendering, kept"
        );
        let scroll = desk.focus.scroll;
        assert!(
            scroll <= at && at < scroll + rows,
            "{at} in {scroll}+{rows}"
        );
        assert_eq!(desk.route(key(KeyCode::Home), SHORT), Route::Repaint);
        let (_, _) = picked_line(&mut desk, SHORT);
        assert_eq!(desk.focus.scroll, 0, "the page keys are not pulled back");
        let (at, rows) = picked_line(&mut desk, TALL);
        let scroll = desk.focus.scroll;
        assert!(
            scroll <= at && at < scroll + rows,
            "back: {at} in {scroll}+{rows}"
        );
    }
}
