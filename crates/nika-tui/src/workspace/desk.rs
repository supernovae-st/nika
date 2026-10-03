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
//! | `Left` · `Right` (aside)   | Nika · Files                       | the composer (cursor)           |
//! | `Left` · `Right` (object)  | the previous · next face           | the composer (cursor)           |
//! | `r` (object)               | the Session reads the file again   | the composer                    |
//! | `Enter`                    | opens the aside entry              | sends the line                  |
//! | `Tab` and any other key    | ignored                            | the composer (`Tab` completes)  |
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
    /// Scroll the conversation's transcript one block back.
    Older,
    /// Scroll the conversation's transcript one block forward.
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// The face of the look as the viewers last rendered it, and for what.
    drawn: Option<Drawn>,
}

/// One face of one look rendered for one region: what the frame paints.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Drawn {
    /// The look's path and witness, the face, the region's width, the glyph
    /// column and the colour it was rendered for.
    key: (String, Option<String>, Face, u16, bool, bool),
    title: Line<'static>,
    body: Vec<Line<'static>>,
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
            drawn: None,
        }
    }

    /// Render the face in view for a terminal of `size`, before the frame and
    /// only when the look, the face, the region's width, the glyph column or
    /// the colour changed: drawing paints these lines and calls no viewer.
    pub(crate) fn prepare(&mut self, size: (u16, u16), ascii: bool, color: bool) {
        let pinned = self.view.as_ref().is_some_and(|v| v.pinned.is_some());
        let area = Rect::new(0, 0, size.0, size.1);
        let (Some(look), Some(geometry)) = (&self.look, Geometry::of(area, pinned)) else {
            return;
        };
        let width = geometry.object.width;
        let witness = look.witness().map(str::to_owned);
        let key = (
            look.path().to_owned(),
            witness,
            self.face,
            width,
            ascii,
            color,
        );
        if self.drawn.as_ref().is_some_and(|d| d.key == key) {
            return;
        }
        let (title, body) = look.face_lines(self.face, width, ascii, color);
        self.drawn = Some(Drawn { key, title, body });
    }

    /// The path of the opened workflow, while the view still lists it.
    #[must_use]
    pub(crate) fn opened_workflow(&self) -> Option<&str> {
        match self.shown()? {
            Opened::Workflow(workflow) => Some(&workflow.path),
            Opened::Run(_) => None,
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

    /// The object in view for `opened`: a workflow's look when its Session
    /// took one of it, else what the listing judged of it.
    fn object_of(&self, opened: Opened<'_>, ascii: bool) -> Object {
        let drawn = self.drawn.as_ref().filter(|d| {
            let current = self.look.as_ref().map(|l| (l.path(), l.witness()));
            current == Some((d.key.0.as_str(), d.key.1.as_deref())) && d.key.2 == self.face
        });
        match (opened, drawn) {
            (Opened::Workflow(workflow), Some(drawn)) if drawn.key.0 == workflow.path => {
                Object::Workflow {
                    title: drawn.title.clone(),
                    body: drawn.body.clone(),
                }
            }
            _ => opened.object(ascii),
        }
    }

    /// The object opened, when the current view still lists it.
    fn shown(&self) -> Option<Opened<'_>> {
        project::resolve(self.view.as_ref(), self.opened.as_ref())
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
        let screen = Screen::new(
            project::place(view),
            project::aside(view, self.focus.tab, opened),
            object,
            project::thread(view, label.as_deref()),
        );
        match view.and_then(|v| v.pinned.clone()) {
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
        match self.focus.handle(key, extent) {
            Action::Compose => composer_route(key),
            Action::Moved => Route::Repaint,
            Action::Open(index) => {
                let route = self.open(index);
                let pinned = self.view.as_ref().is_some_and(|v| v.pinned.is_some());
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
            _ => Route::Nothing,
        }
    }

    /// Show the next face of the look in view (or the previous one), from
    /// its top; nothing turns while no look is in view.
    fn turn(&mut self, next: bool) -> Route {
        if self.look.is_none() || self.opened_workflow().is_none() {
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
        match project::target(self.view.as_ref(), self.focus.tab, index) {
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
        assert_eq!(desk.route(key(KeyCode::F(6)), WIDE), Route::Repaint);
        assert_eq!(desk.focus.region, Region::Aside);
        assert_eq!(desk.route(key(KeyCode::Char('x')), WIDE), Route::Nothing);
        assert_eq!(desk.route(key(KeyCode::Esc), WIDE), Route::Repaint);
        assert_eq!(desk.focus.region, Region::Conversation);
        assert_eq!(desk.route(key(KeyCode::Esc), WIDE), Route::Leave);
        // At 80 columns the aside is folded, yet reachable: F6 reaches it
        // (drawn over the object), then the object.
        desk.route(key(KeyCode::F(6)), SMALL);
        assert_eq!(desk.focus.region, Region::Aside);
        desk.route(key(KeyCode::F(6)), SMALL);
        assert_eq!(desk.focus.region, Region::Object);
        assert_eq!(desk.route(key(KeyCode::PageUp), SMALL), Route::Nothing);
    }

    /// Enter on a workflow opens it as the object and attaches nothing; Enter
    /// on this conversation gives the keys back to its composer.
    #[test]
    fn the_aside_opens_a_workflow_without_attaching_it() {
        let mut desk = demo();
        assert!(desk.welcoming());
        desk.route(key(KeyCode::F(6)), WIDE);
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
        desk.route(key(KeyCode::F(6)), WIDE);
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
        desk.route(key(KeyCode::F(6)), WIDE);
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
        desk.route(key(KeyCode::F(6)), WIDE);
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
        desk.route(key(KeyCode::F(6)), WIDE);
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
        let wide = desk.drawn.as_ref().map(|d| d.key.3);
        desk.prepare((60, 18), true, false);
        let narrow = desk.drawn.as_ref().map(|d| d.key.3);
        assert_eq!((wide, narrow), (Some(82), Some(60)));
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
        desk.route(key(KeyCode::F(6)), SMALL);
        assert_eq!(desk.focus.region, Region::Aside);
        desk.route(key(KeyCode::Down), SMALL);
        assert_eq!(desk.route(key(KeyCode::Enter), SMALL), Route::Inspect);
        assert_eq!(desk.focus.region, Region::Object);
        let mut wide = demo();
        wide.route(key(KeyCode::F(6)), WIDE);
        wide.route(key(KeyCode::Down), WIDE);
        wide.route(key(KeyCode::Enter), WIDE);
        assert_eq!(
            wide.focus.region,
            Region::Aside,
            "a shown aside keeps the keys"
        );
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
}
