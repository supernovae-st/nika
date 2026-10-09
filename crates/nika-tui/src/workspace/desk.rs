// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workspace the shell keeps between frames: the project view the
//! conversation last lent, the keyboard focus, the workflow opened as the
//! object in view, the look its Session took of that workflow, the face in
//! view and the arrangement of all of it (the object restored or expanded,
//! the separators' shares: the private `layout` module). It assembles
//! one [`Screen`] from them and routes a key by a fixed precedence (the shell
//! decides `Ctrl+C` and `Ctrl+T` before it):
//!
//! | key                        | a region other than the composer   | the composer's region           |
//! |----------------------------|------------------------------------|---------------------------------|
//! | `F4`                       | the object expanded (where it grows) · restored | the same          |
//! | `F6` · `Shift+F6`          | next · previous region             | next · previous region          |
//! | `Esc`                      | back to the composer               | leaves to inline, draft intact  |
//! | `PgUp` · `PgDn`            | the object scrolls (object region) | the transcript scrolls          |
//! | arrows, `Home` · `End`     | aside selection · object scroll    | the composer (history, cursor)  |
//! | `Up` · `Down` (run tasks)  | the previous · next task picked    | the composer (history)          |
//! | `Left` · `Right` (aside)   | Nika · Files                       | the composer (cursor)           |
//! | `Left` · `Right` (object)  | the previous · next face           | the composer (cursor)           |
//! | `+` · `-` · `0`            | the region's separator moves · is automatic again | the composer (types them) |
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
//! key then behaves as in the composer's region (`F4` changes nothing there:
//! no object is drawn), and the focus kept here returns with the workspace
//! when the size allows. Nothing here reads a file, a clock or the
//! environment.

use crossterm::event::{KeyCode, KeyEvent};
use nika_tui_view::Face;
use ratatui::Frame;
use ratatui::text::Line;

use super::candidate::Proposed;
use super::cards::review::{self, Review};
use super::live::{LiveRun, Pick, RunFace, Want};
use crate::model::Conversation;
use crate::session::acquire::{ChildRead, Fetched, Proven};
use crate::session::feed::{Gap, Observed};
use nika_display::run_story::{ExecutionId, RunState};
use nika_session::KeptRun;

/// How many past legs of runs the desk keeps beside the one in flight.
const PAST_LEGS: usize = 4;
use super::focus::{Action, Extent, Focus, Region};
use super::geometry::{Arrangement, Layout};
use super::inspect::Inspected;
use super::object::{Object, Paint};
use super::project::{self, Opened, ProjectView, Target};
use super::screen::{self, Screen};
use crate::composer::Composer;
use crate::model::{Presentation, UiState};

mod layout;
use layout::Drag;

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
    /// The layout in view and the shares the separators were moved to.
    arrangement: Arrangement,
    /// The separator the pointer is moving, while its button is held.
    drag: Option<Drag>,
    /// The arrangement changed since the shell last took it to be kept.
    unsettled: bool,
    /// The frame a current decision folds the stacked object on: the size,
    /// the arrangement and the pin decided for, once per frame
    /// ([`Desk::prepare_for`], read through [`Desk::folds`]); another
    /// candidate drops it.
    fold: Option<((u16, u16), Arrangement, bool)>,
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
    /// witness of its bytes, the face, the region's width, the glyph column,
    /// the colour, the short graph format, and whether the conversation
    /// reviews the candidate shown (its facts then live there alone).
    key: (
        String,
        Option<String>,
        Face,
        RunFace,
        u16,
        bool,
        bool,
        bool,
        bool,
    ),
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
    /// No project known yet, the composer has the keys, nothing opened, the
    /// Session layout with automatic shares.
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
            arrangement: Arrangement::of(Layout::Session),
            drag: None,
            unsettled: false,
            fold: None,
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
                    world,
                } => {
                    if let Some(leg) = self.live.take() {
                        self.past.insert(0, leg);
                        self.past.truncate(PAST_LEGS);
                    }
                    // The last run of the same workflow observed here: a fresh run does every
                    // effect it declares again (a resumed leg continues its own run).
                    let earlier = (self.past.iter())
                        .find(|leg| !resume && leg.workflow() == workflow)
                        .map(LiveRun::named);
                    let leg = LiveRun::asked(workflow, resume, typed, look.map(|l| *l));
                    self.live = Some(leg.reaching(world.map(|w| *w)).after(earlier));
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
                // The shell's card reads the Session's activity; no run leg does.
                Observed::Activity(_) => {}
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

    /// What a run's story said now folds under: the label and state word of
    /// the leg this turn observed, once it came typed, whole and settled; a
    /// paused, partial or untyped leg folds nothing and its story stays whole.
    pub(crate) fn folded(&self) -> Option<(String, &'static str)> {
        let leg = self.live.as_ref().filter(|leg| leg.whole())?;
        let state = leg.reported().filter(|state| *state != RunState::Paused)?;
        Some((leg.label(), state.as_str()))
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

    /// Render the face in view for the frame of `state`, before it: the
    /// candidate shown leaves its facts to the conversation while the
    /// conversation reviews it ([`review::summarized`]), the same decision
    /// the transcript paints with, cached with the face so painting, the
    /// extent, the pointer and scrolling read one body. First, once per
    /// frame, whether that decision or the live question folds the stacked
    /// object ([`Desk::folds`]): the face is rendered for the cells it gets.
    pub(crate) fn prepare_for(&mut self, state: &UiState, ascii: bool, color: bool) {
        let review = self.review(ascii);
        let reviewed = state.presentation == Presentation::Workspace
            && review::summarized(state, review.as_ref()).is_some();
        let folds = self.overflowed(state, review.as_ref());
        self.fold = folds.then_some((state.size, self.arrangement, self.pins()));
        self.prepare_reviewed(state.size, ascii, color, reviewed);
    }

    /// [`Self::prepare_for`] with no review in the conversation.
    #[cfg(test)]
    pub(crate) fn prepare(&mut self, size: (u16, u16), ascii: bool, color: bool) {
        self.prepare_reviewed(size, ascii, color, false);
    }

    /// Render the face in view for a terminal of `size`, before the frame and
    /// only when the look or the candidate, the face, the region's width, the
    /// glyph column, colour, short graph format or review changed: drawing
    /// paints these lines and calls no viewer.
    fn prepare_reviewed(&mut self, size: (u16, u16), ascii: bool, color: bool, reviewed: bool) {
        let (Some((from, witness)), Some(geometry)) = (self.rendered_from(), self.geometry(size))
        else {
            return;
        };
        let reviewed = reviewed && matches!(self.shown(), Some(Opened::Candidate(_)));
        // The face is rendered for the cells the object paints it in.
        let width = screen::object_body(&geometry).width;
        let height = geometry.object.height;
        let compact = height < 24
            && self.face == Face::Graph
            && matches!(
                self.shown(),
                Some(Opened::Candidate(_) | Opened::Workflow(_))
            );
        let key = (
            from,
            witness,
            self.face,
            self.run_face,
            width,
            ascii,
            color,
            compact,
            reviewed,
        );
        if let Some(drawn) = self.drawn.as_mut().filter(|d| d.key == key) {
            // The same lines in another viewport: a picked task stays in view.
            let rows = usize::from(super::object::content_rows(drawn.body.len(), height)).max(1);
            if drawn.rows != rows {
                drawn.rows = rows;
                if let Some(line) = drawn.picked {
                    follow(&mut self.focus.scroll, line, rows);
                }
            }
            return;
        }
        // Compact rows and cards have different line identities, and so do a
        // face with and without its facts. Start the reading at its top when
        // either changes; keep source and focus.
        if self.drawn.as_ref().is_some_and(|drawn| {
            drawn.key.0 == key.0
                && drawn.key.1 == key.1
                && drawn.key.2 == key.2
                && (drawn.key.7 != compact || drawn.key.8 != reviewed)
        }) {
            self.focus.scroll = 0;
        }
        let mut picked_line = None;
        let lines = match (self.shown(), &self.look) {
            (Some(Opened::Candidate(c)), _) if reviewed => {
                c.reviewed_face_lines(self.face, width, ascii, color, compact)
            }
            (Some(Opened::Candidate(c)), _) => {
                c.face_lines_in(self.face, width, ascii, color, compact)
            }
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
            (_, Some(look)) => look.face_lines_in(self.face, width, ascii, color, compact),
            (_, None) => return,
        };
        let rows = usize::from(super::object::content_rows(lines.1.len(), height)).max(1);
        let resized = (self.drawn.as_ref()).is_some_and(|d| d.key.4 != width || d.rows != rows);
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
        // The leg the reading was asked for, the one in flight or an earlier one in view.
        let Some(leg) = viewed_mut(self.opened.as_ref(), &mut self.live, &mut self.past)
            .filter(|l| l.execution() == Some(execution) && l.generation() == reading.generation)
        else {
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
        // Another candidate is another decision: the next frame decides the fold.
        self.fold = None;
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
            (self.live.as_ref(), &self.past),
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
            project::aside(
                view,
                self.focus.tab,
                opened,
                candidate,
                (self.live.as_ref(), &self.past),
            ),
            object,
            project::thread(view, label.as_deref()),
        );
        let project = view.map_or("", |v| v.name.as_str());
        let pinned = (self.live.as_ref().map(|leg| leg.pinned(project)))
            .or_else(|| view.and_then(|v| v.pinned.clone()));
        let screen = screen.reviewing(self.review(ascii));
        match pinned {
            Some(run) => screen.pinning(run),
            None => screen,
        }
    }

    /// The candidate a consent can name, as the conversation reviews it: the
    /// one review the transcript's paint ([`Self::screen`]), its scroll bounds
    /// and the full-words reader read.
    #[must_use]
    pub(crate) fn review(&self, ascii: bool) -> Option<Review> {
        self.candidate
            .as_ref()
            .and_then(|candidate| candidate.review(ascii))
    }

    /// What the regions hold on a terminal of `size`, as the arrangement lays
    /// them out; `None` below the minimum.
    #[must_use]
    pub(crate) fn extent(&self, size: (u16, u16)) -> Option<Extent> {
        let geometry = self.geometry(size)?;
        // The counts the keys move over are the same in both glyph columns.
        Some(screen::extent_in(&self.screen(false), &geometry))
    }

    /// Route one key on a terminal of `size` (the shell has already taken
    /// `Ctrl+C` and `Ctrl+T`).
    pub(crate) fn route(&mut self, key: KeyEvent, size: (u16, u16)) -> Route {
        let Some(extent) = self.extent(size) else {
            // The focus view stands in: the composer's region, whatever the
            // focus kept for the workspace's return.
            return composer_route(key);
        };
        if key.code == KeyCode::F(4) {
            // The object expanded or restored, from every region: the view
            // changes, the draft, the focus, the object and the runs do not.
            // Where the object cannot grow the arrangement stays as it is.
            return if self.toggle_layout(size) {
                Route::Repaint
            } else {
                Route::Nothing
            };
        }
        if let Some(route) = self.separator_key(key, size) {
            return route;
        }
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
                let folded = self.geometry(size).is_some_and(|g| g.aside.is_none());
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
                if let Some(leg) = viewed_mut(self.opened.as_ref(), &mut self.live, &mut self.past)
                {
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
        let leg = viewed(self.opened.as_ref(), self.live.as_ref(), &self.past)?;
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
        let (candidate, legs) = (
            self.candidate.as_ref(),
            (self.live.as_ref(), &self.past[..]),
        );
        match project::target(self.view.as_ref(), self.focus.tab, index, candidate, legs) {
            Some(Target::Conversation) => {
                self.focus.region = Region::Conversation;
                Route::Repaint
            }
            Some(target) => {
                if self.opened.as_ref() != Some(&target) {
                    self.look = None;
                    self.face = Face::Source;
                    self.focus.scroll = 0;
                    // Another run in view: its own task list, from its run face.
                    if matches!(target, Target::Live | Target::Past(_)) {
                        self.pick = Pick::new();
                        self.run_face = RunFace::Run;
                        self.drawn = None;
                    }
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

    /// A fresh entry into the workspace: the composer has the keys, no
    /// separator is held; the object, the selection, the projection and the
    /// arrangement stay as they were left.
    pub(crate) fn enter(&mut self) {
        self.focus.region = Region::Conversation;
        self.release();
    }
}

/// A key in the composer's region of a full screen (the workspace's, or the
/// focus view's whole screen): `Esc` leaves, the page keys scroll the
/// transcript, `F4` changes nothing (no object is drawn there), the composer
/// takes everything else.
pub(crate) fn composer_route(key: KeyEvent) -> Route {
    match key.code {
        KeyCode::Esc => Route::Leave,
        KeyCode::PageUp => Route::Older,
        KeyCode::PageDown => Route::Newer,
        KeyCode::F(4) => Route::Nothing,
        _ => Route::Compose,
    }
}

/// Draw the workspace on the whole frame, in the desk's arrangement; `false`,
/// drawing nothing, below the minimum (the caller draws the focus view there).
pub(crate) fn draw(
    frame: &mut Frame<'_>,
    desk: &Desk,
    paint: Paint,
    state: &UiState,
    composer: &Composer,
) -> bool {
    let area = frame.area();
    screen::draw_in(
        frame,
        &desk.screen(paint.ascii),
        paint,
        &desk.focus,
        state,
        composer,
        screen::Chrome {
            arrangement: desk.arrangement,
            dragging: desk.dragging(),
            folded: desk.folds((area.width, area.height)),
        },
    )
}

/// The run leg in view: the leg in flight, or an earlier one opened by its execution.
fn viewed<'a>(
    opened: Option<&Target>,
    live: Option<&'a LiveRun>,
    past: &'a [LiveRun],
) -> Option<&'a LiveRun> {
    match opened? {
        Target::Live => live,
        Target::Past(execution) => past.iter().find(|l| l.execution() == Some(*execution)),
        _ => None,
    }
}

/// [`viewed`], to apply what was read for it or forget what it acquired.
fn viewed_mut<'a>(
    opened: Option<&Target>,
    live: &'a mut Option<LiveRun>,
    past: &'a mut [LiveRun],
) -> Option<&'a mut LiveRun> {
    match opened? {
        Target::Live => live.as_mut(),
        Target::Past(execution) => past.iter_mut().find(|l| l.execution() == Some(*execution)),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod layout_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod fold_tests;
