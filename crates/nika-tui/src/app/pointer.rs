// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pointer navigation and input while a turn runs. All coordinates are judged
//! against the current frame geometry (the desk's arrangement); no click, no
//! drag becomes a submitted line, a consent or a Run. The object's own action
//! expands or restores it as `F4` does, a region the folded header names takes
//! the keys as `F6` moves them, a separator follows the pointer while its
//! button is held (within its bounds), and the marker of a transcript scrolled
//! back returns it to its latest row as `End` does. While the full diagnostic
//! covers the frame, nothing under it takes the pointer. Hold Shift for the
//! terminal's own selection/copy where supported; F4, F6, the separator keys,
//! arrows, PageUp/PageDown, End and Enter remain the complete keyboard path.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use nika_tui_view::Face;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use unicode_width::UnicodeWidthStr;

use super::{
    Busy, Composer, ComposerAction, Conversation, Desk, Exit, Heard, LEAVE_WAITS, Presentation,
    Route, Shell, Signal, UiEvent, UiState, busy_key, is_ctrl_c,
};
use crate::workspace::screen;
use crate::workspace::{
    aside, focus::Region, geometry::Geometry, live::RunFace, object::Object, project::Target,
};

const WHEEL_ROWS: usize = 3;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// What a held separator's move or release asks.
const fn held(moved: bool) -> Route {
    if moved {
        Route::Repaint
    } else {
        Route::Nothing
    }
}

/// Only real painted regions take the pointer. Inline leaves selection and
/// scrollback to the terminal; a workspace too small follows its focus fallback.
fn route(state: &mut UiState, desk: &mut Desk, composer: &Composer, mouse: MouseEvent) -> Route {
    let point = Position::new(mouse.column, mouse.row);
    match mouse.kind {
        // A held separator follows the pointer; letting go keeps it there. A
        // move without the button means it came up out of sight.
        MouseEventKind::Drag(MouseButton::Left)
            if state.presentation == Presentation::Workspace =>
        {
            return held(desk.drag_to(point, state.size));
        }
        MouseEventKind::Up(MouseButton::Left) | MouseEventKind::Moved => {
            return held(desk.release());
        }
        _ => {}
    }
    if state.presentation == Presentation::Inline || mouse.modifiers != KeyModifiers::NONE {
        return Route::Nothing;
    }
    let area = Rect::new(0, 0, state.size.0, state.size.1);
    if !area.contains(point) {
        return Route::Nothing;
    }
    let older = match mouse.kind {
        MouseEventKind::ScrollUp => Some(true),
        MouseEventKind::ScrollDown => Some(false),
        MouseEventKind::Down(MouseButton::Left) => None,
        _ => return Route::Nothing,
    };
    let geometry = (state.presentation == Presentation::Workspace)
        .then(|| desk.geometry(state.size))
        .flatten();
    let Some(geometry) = geometry else {
        if let Some(older) = older {
            crate::scroll::rows(state, desk, composer, older, WHEEL_ROWS);
            return Route::Repaint;
        }
        return Route::Nothing;
    };
    if older.is_none()
        && let Some(route) = chrome_pointer(state, desk, composer, &geometry, point)
    {
        return route;
    }
    let aside = geometry
        .aside
        .map(|mut area| {
            area.width = area.width.saturating_sub(1);
            area
        })
        .or_else(|| (desk.focus.region == Region::Aside).then_some(geometry.object));
    if let Some(area) = aside.filter(|area| area.contains(point)) {
        return aside_pointer(state, desk, area, mouse, older);
    }
    if geometry.conversation.contains(point) {
        if let Some(older) = older {
            crate::scroll::rows(state, desk, composer, older, WHEEL_ROWS);
        } else {
            desk.focus.region = Region::Conversation;
        }
        return Route::Repaint;
    }
    if geometry.object.contains(point) {
        if let Some(older) = older {
            if let Some(extent) = desk.extent(state.size) {
                desk.focus.scroll_rows(older, WHEEL_ROWS, extent);
            }
        } else {
            desk.focus.region = Region::Object;
            // The faces stand where the object paints its title: its body.
            let body = screen::object_body(&geometry);
            if mouse.row == body.y
                && let Some(column) = mouse.column.checked_sub(body.x)
            {
                return face_pointer(state, desk, column, body.width);
            }
        }
        return Route::Repaint;
    }
    Route::Nothing
}

/// A press on the chrome rather than on a region's content: a region the
/// folded header names (it takes the keys, as `F6` moves them), the object's
/// own action (`F4`), a separator (held until the button comes up, the
/// keyboard focus unchanged), or the marker back to the transcript's latest
/// row (the conversation takes the keys, as a click on it does, and `End`
/// applies). `None` leaves the press to the regions. Every one of them is
/// view only.
fn chrome_pointer(
    state: &mut UiState,
    desk: &mut Desk,
    composer: &Composer,
    geometry: &Geometry,
    point: Position,
) -> Option<Route> {
    desk.release();
    let shown = desk.screen(state.ascii);
    let focused = desk.focus.region;
    if point.y == geometry.header.y && geometry.aside.is_none() {
        let named = screen::region_at(&shown.place, geometry.header, focused, state.ascii, point.x);
        if let Some(region) = named {
            desk.focus.region = region;
            return Some(held(region != focused));
        }
    }
    let next = desk.toggled(state.size);
    let action = screen::object_action(&shown.object, geometry, focused, next, state.ascii);
    if action.is_some_and(|(cells, _)| cells.contains(point)) {
        // The object's own action is `F4`: the view changes, the keys stay.
        desk.toggle_layout(state.size);
        return Some(Route::Repaint);
    }
    if desk.press(point, state.size).is_some() {
        return Some(Route::Repaint);
    }
    if state.focus_scroll > 0 {
        let transcript = screen::panel_areas(geometry, state, composer, &shown.thread)[1];
        let marker = screen::latest_area(transcript, state.ascii);
        if marker.is_some_and(|marker| marker.contains(point)) {
            desk.focus.region = Region::Conversation;
            crate::scroll::end(state, desk, key(KeyCode::End));
            return Some(Route::Repaint);
        }
    }
    None
}

fn aside_pointer(
    state: &UiState,
    desk: &mut Desk,
    area: Rect,
    mouse: MouseEvent,
    older: Option<bool>,
) -> Route {
    let shown = desk.screen(state.ascii);
    if let Some(older) = older {
        desk.focus
            .scroll_aside(older, WHEEL_ROWS, shown.aside.entries.len());
        return Route::Repaint;
    }
    let row = mouse.row - area.y;
    let column = mouse.column - area.x;
    let entry = aside::entry_at(
        &shown.aside,
        area.width,
        area.height,
        state.ascii,
        desk.focus.selected,
        row,
    );
    desk.focus.region = Region::Aside;
    if row == 1 {
        // Match the rendered tab spans, including their ASCII/Unicode separator.
        let rows = aside::lines_anchored(
            &shown.aside,
            area.width,
            area.height,
            state.ascii,
            state.color,
            None,
            desk.focus.selected,
        );
        if let Some(line) = rows.get(1) {
            return match span_at(line, column, area.width) {
                Some("Nika") => desk.route(key(KeyCode::Left), state.size),
                Some("Files") => desk.route(key(KeyCode::Right), state.size),
                _ => Route::Repaint,
            };
        }
    }
    if let Some(index) = entry {
        desk.focus.selected = index;
        // Exactly the keyboard path: inspection only, never Save, consent or Run.
        return desk.route(key(KeyCode::Enter), state.size);
    }
    Route::Repaint
}

fn span_at<'a>(line: &'a Line<'_>, column: u16, width: u16) -> Option<&'a str> {
    let mut start = 0;
    for span in &line.spans {
        let end = start + span.content.width();
        if end > usize::from(width) {
            return None;
        }
        if (start..end).contains(&usize::from(column)) {
            return Some(&span.content);
        }
        start = end;
    }
    None
}

/// Click only whole, visible tab labels from the existing title. Truncated
/// labels and task-detail titles remain keyboard-only, never guessed hit boxes.
fn face_pointer(state: &UiState, desk: &mut Desk, column: u16, width: u16) -> Route {
    let shown = desk.screen(state.ascii);
    let Object::Workflow { title, .. } = &shown.object else {
        return Route::Repaint;
    };
    let steps = if matches!(desk.opened, Some(Target::Live | Target::Past(_))) {
        let labels: Vec<_> = RunFace::ALL
            .iter()
            .map(|face| {
                if *face == desk.run_face {
                    format!("[{}]", face.label())
                } else {
                    face.label().to_owned()
                }
            })
            .collect();
        let Some(index) = suffix_tab_at(title, &labels, column, width) else {
            return Route::Repaint;
        };
        let current = RunFace::ALL
            .iter()
            .position(|face| *face == desk.run_face)
            .unwrap_or(0);
        (index + RunFace::ALL.len() - current) % RunFace::ALL.len()
    } else {
        let Some(label) = span_at(title, column, width) else {
            return Route::Repaint;
        };
        let label = label.trim_start_matches('[').trim_end_matches(']');
        let Some(index) = Face::ALL.iter().position(|face| face.label() == label) else {
            return Route::Repaint;
        };
        let current = Face::ALL
            .iter()
            .position(|face| *face == desk.face)
            .unwrap_or(0);
        (index + Face::ALL.len() - current) % Face::ALL.len()
    };
    let mut route = Route::Repaint;
    for _ in 0..steps {
        route = desk.route(key(KeyCode::Right), state.size);
    }
    route
}

fn suffix_tab_at(title: &Line<'_>, labels: &[String], column: u16, width: u16) -> Option<usize> {
    let text = title
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    if text.width() > usize::from(width) {
        return None;
    }
    let tabs = labels.join(" ");
    let mut start = text.strip_suffix(&tabs)?.width();
    for (index, label) in labels.iter().enumerate() {
        let end = start + label.width();
        if (start..end).contains(&usize::from(column)) {
            return Some(index);
        }
        start = end + 1;
    }
    None
}

impl<C: Conversation + 'static> Shell<C> {
    /// Ratatui's clear preserves the cursor by querying the terminal even in
    /// fullscreen mode, and a terminal that never answers would end the
    /// session. A full screen owns every cell: it is cleared and wholly
    /// redrawn at the size the backend reports, with no query. The inline
    /// viewport keeps the clear; the sole reader is parked so it cannot
    /// consume that reply.
    pub(super) fn repaint(&mut self, broker: &super::Broker) -> std::io::Result<()> {
        if self.state.presentation != Presentation::Inline {
            let size = self.screen.size()?;
            return self.screen.resize(Rect::new(0, 0, size.width, size.height));
        }
        broker.pause();
        let cleared = self.screen.clear();
        broker.resume();
        cleared
    }

    /// One pointer event; what it changed (`Route::Nothing`: nothing to draw).
    pub(super) fn on_mouse(&mut self, mouse: MouseEvent) -> Route {
        if self.diagnostic.is_some() {
            // The full diagnostic covers every region: no target under it
            // takes the pointer, and a held separator is let go where it is.
            self.desk.release();
            return Route::Nothing;
        }
        let routed = route(&mut self.state, &mut self.desk, &self.composer, mouse);
        if self.composer.palette_open()
            && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            && self.desk.focus.region != Region::Conversation
        {
            // The click explicitly chose another panel. Restore the draft,
            // but let that new focus supersede where the palette opened.
            self.palette_from = None;
            self.keep_reading(|_, composer| composer.close_palette());
        }
        if routed != Route::Nothing {
            self.state.interrupt_armed = false;
            self.state.completion = None;
        }
        if routed == Route::Inspect {
            self.look();
        }
        routed
    }
    /// One event heard while a turn runs. An interruption acts now (the
    /// exit to leave with, once armed). The composer stays usable: words,
    /// pastes and edits land in the draft as they are typed, a bare `Enter`
    /// sends nothing and the hint row says when it will, and in a full
    /// screen the page keys scroll the transcript. Every other event waits
    /// for the turn. Once the turn ends on a decision, the typeahead law
    /// keeps the draft unsent ([`Self::set_aside_typeahead`]).
    pub(super) fn hear(&mut self, event: UiEvent, armed: &mut bool) -> Heard {
        let key = match event {
            UiEvent::Signal(Signal::Terminate) => return Heard::Leave(Exit::Terminated),
            UiEvent::Closed => return Heard::Leave(Exit::Closed),
            UiEvent::Signal(Signal::Interrupt) => {
                return self.stop_or_arm(armed).map_or(Heard::Nothing, Heard::Leave);
            }
            UiEvent::Key(key) if is_ctrl_c(&key) => {
                return self.stop_or_arm(armed).map_or(Heard::Nothing, Heard::Leave);
            }
            UiEvent::Paste(text) => {
                self.state.completion = None;
                self.composer.paste(&text);
                self.typed_live = true;
                return Heard::Redraw;
            }
            UiEvent::Mouse(mouse) => {
                // A pointer moving over the screen changes nothing: no frame.
                if self.on_mouse(mouse) == Route::Nothing {
                    return Heard::Nothing;
                }
                // The view changed: a requested stop's words come back once
                // the event is handled ([`Self::hear_first`]); the next
                // `Ctrl+C` leaves, it does not ask the stop again.
                return Heard::Redraw;
            }
            UiEvent::Key(key) => key,
            UiEvent::Resize(cols, rows) if self.state.presentation != Presentation::Inline => {
                // A full screen reads its size without asking the terminal:
                // the frames drawn while the turn runs, and the keys the
                // workspace routes meanwhile, follow the new size at once.
                // The resize is still replayed once the turn ends.
                self.state.size = (cols, rows);
                self.deferred.push_back(event);
                // A shrink followed by growth can leave the backend size
                // unchanged while the terminal has discarded old cells.
                return Heard::Repaint;
            }
            other => {
                self.deferred.push_back(other);
                return Heard::Nothing;
            }
        };
        if crate::scroll::end(&mut self.state, &self.desk, key) {
            return Heard::Redraw;
        }
        match busy_key(&self.state, &mut self.desk, key) {
            Busy::Edit => {
                self.state.completion = None;
                self.typed_live = true;
                if self.composer.handle(key) == ComposerAction::Complete
                    && let crate::composer::Completion::Several(list) =
                        self.composer.complete(&self.commands)
                {
                    self.state.completion = Some(list.join("  "));
                }
                Heard::Redraw
            }
            Busy::Hold => {
                self.queue_correction();
                Heard::Redraw
            }
            Busy::Older => {
                crate::scroll::page(&mut self.state, &self.desk, &self.composer, true);
                Heard::Redraw
            }
            Busy::Newer => {
                crate::scroll::page(&mut self.state, &self.desk, &self.composer, false);
                Heard::Redraw
            }
            Busy::Later => {
                self.deferred.push_back(UiEvent::Key(key));
                Heard::Nothing
            }
            Busy::Leave => {
                self.state.completion = Some(LEAVE_WAITS.to_owned());
                self.deferred.push_back(UiEvent::Key(key));
                Heard::Redraw
            }
            Busy::Region => Heard::Redraw,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::model::{Committed, Kind, demo_project};
    use crate::workspace::focus::Extent;

    fn mouse(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn setup(size: (u16, u16)) -> (UiState, Desk, Composer) {
        let mut state = UiState::new(Presentation::Workspace, false, size);
        state.transcript.push(Committed::new(
            Kind::Question,
            (0..150)
                .map(|n| format!("line {n}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
        let mut desk = Desk::new();
        desk.view = Some(demo_project());
        let mut composer = Composer::new();
        composer.paste("unsent yes");
        (state, desk, composer)
    }

    #[test]
    fn wheel_follows_the_point_without_taking_focus_or_the_draft() {
        let (mut state, mut desk, composer) = setup((120, 40));
        let g = Geometry::of(Rect::new(0, 0, 120, 40), desk.pins()).expect("geometry");
        desk.focus.region = Region::Object;
        assert_eq!(
            route(
                &mut state,
                &mut desk,
                &composer,
                mouse(
                    MouseEventKind::ScrollUp,
                    g.conversation.x + 3,
                    g.conversation.y + 5
                )
            ),
            Route::Repaint
        );
        assert_eq!(state.focus_scroll, WHEEL_ROWS);
        assert_eq!(desk.focus.region, Region::Object);
        assert_eq!(composer.text(), "unsent yes");
        let before = state.focus_scroll;
        let aside = g.aside.expect("wide aside");
        route(
            &mut state,
            &mut desk,
            &composer,
            mouse(MouseEventKind::ScrollDown, aside.x + 1, aside.y + 4),
        );
        assert!(desk.focus.selected > 0);
        assert_eq!(state.focus_scroll, before);
        assert_eq!(desk.focus.region, Region::Object);
    }

    #[test]
    fn clicks_open_exact_list_entries_through_enter_and_never_submit() {
        let (mut state, mut desk, composer) = setup((120, 40));
        state.busy = Some("observed work".into());
        let g = Geometry::of(Rect::new(0, 0, 120, 40), desk.pins()).expect("geometry");
        let area = g.aside.expect("aside");
        let shown = desk.screen(state.ascii);
        let (row, index) = (0..area.height)
            .find_map(|row| {
                aside::entry_at(
                    &shown.aside,
                    area.width - 1,
                    area.height,
                    state.ascii,
                    0,
                    row,
                )
                .filter(|index| *index > 0)
                .map(|index| (row, index))
            })
            .expect("listed entry");
        let mut keyboard = Desk::new();
        keyboard.view = desk.view.clone();
        keyboard.focus.region = Region::Aside;
        keyboard.focus.selected = index;
        let expected = keyboard.route(key(KeyCode::Enter), state.size);
        assert_eq!(
            route(
                &mut state,
                &mut desk,
                &composer,
                mouse(
                    MouseEventKind::Down(MouseButton::Left),
                    area.x + 2,
                    area.y + row
                )
            ),
            expected
        );
        assert_eq!(desk.opened, keyboard.opened);
        assert_eq!(composer.text(), "unsent yes");
        assert!(state.busy.is_some());
        route(
            &mut state,
            &mut desk,
            &composer,
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                g.conversation.x + 2,
                g.conversation.bottom() - 2,
            ),
        );
        assert_eq!(desk.focus.region, Region::Conversation);
        assert_eq!(
            composer.text(),
            "unsent yes",
            "a composer click is focus only"
        );
    }

    #[test]
    fn inline_shift_border_and_stale_coordinates_do_nothing() {
        let (mut state, mut desk, composer) = setup((120, 40));
        let mut shifted = mouse(MouseEventKind::ScrollUp, 30, 8);
        shifted.modifiers = KeyModifiers::SHIFT;
        for event in [
            shifted,
            mouse(MouseEventKind::Down(MouseButton::Right), 30, 8),
            mouse(MouseEventKind::ScrollUp, 120, 40),
            mouse(MouseEventKind::ScrollUp, 2, 0),
        ] {
            assert_eq!(
                route(&mut state, &mut desk, &composer, event),
                Route::Nothing
            );
        }
        assert_eq!(state.focus_scroll, 0);
        state.presentation = Presentation::Inline;
        assert_eq!(
            route(
                &mut state,
                &mut desk,
                &composer,
                mouse(MouseEventKind::ScrollUp, 30, 8)
            ),
            Route::Nothing
        );
        assert_eq!(state.focus_scroll, 0);
        assert_eq!(desk.focus.region, Region::Conversation);
    }

    #[test]
    fn resize_to_focus_fallback_keeps_scroll_bounded_and_focus_stored() {
        let (mut state, mut desk, composer) = setup((59, 15));
        desk.focus.region = Region::Aside;
        for _ in 0..150 {
            route(
                &mut state,
                &mut desk,
                &composer,
                mouse(MouseEventKind::ScrollUp, 2, 2),
            );
        }
        assert!(state.focus_scroll > 0 && state.focus_scroll < 1000);
        for _ in 0..150 {
            route(
                &mut state,
                &mut desk,
                &composer,
                mouse(MouseEventKind::ScrollDown, 2, 2),
            );
        }
        assert_eq!(state.focus_scroll, 0);
        assert_eq!(desk.focus.region, Region::Aside);
        state.size = (0, 0);
        assert_eq!(
            route(
                &mut state,
                &mut desk,
                &composer,
                mouse(MouseEventKind::ScrollUp, 0, 0)
            ),
            Route::Nothing
        );
    }

    #[test]
    fn object_wheel_is_rows_not_task_selection_and_never_changes_focus() {
        let mut focus = crate::workspace::focus::Focus::composing();
        let extent = Extent {
            aside_shown: true,
            aside_entries: 20,
            object_lines: 50,
            object_rows: 10,
        };
        focus.scroll_rows(false, 3, extent);
        assert_eq!(
            (focus.scroll, focus.selected, focus.region),
            (3, 0, Region::Conversation)
        );
        focus.scroll_rows(false, usize::MAX, extent);
        assert_eq!(focus.scroll, 40);
        focus.scroll_rows(true, usize::MAX, extent);
        assert_eq!(focus.scroll, 0);
    }

    /// A separator follows real pointer events from press to release, within
    /// its bounds; the keys stay in their region and nothing is sent.
    #[test]
    fn a_separator_follows_press_drag_and_release_and_never_submits() {
        use crate::workspace::geometry::Separator;
        let (mut state, mut desk, composer) = setup((120, 40));
        desk.focus.region = Region::Object;
        let g = desk.geometry(state.size).expect("geometry");
        let handle = g.handle(Separator::Beside).expect("beside");
        let (x, y) = (handle.x + 1, handle.y + 3);
        let mut at = |kind, x, y| route(&mut state, &mut desk, &composer, mouse(kind, x, y));
        assert_eq!(
            at(MouseEventKind::Down(MouseButton::Left), x, y),
            Route::Repaint
        );
        assert_eq!(
            at(MouseEventKind::Drag(MouseButton::Left), x + 6, y),
            Route::Repaint
        );
        assert_eq!(
            at(MouseEventKind::Drag(MouseButton::Left), x + 6, y + 2),
            Route::Nothing
        );
        assert_eq!(
            at(MouseEventKind::Up(MouseButton::Left), x + 6, y + 2),
            Route::Repaint
        );
        assert_eq!(
            at(MouseEventKind::Drag(MouseButton::Left), x + 30, y),
            Route::Nothing
        );
        assert_eq!(desk.dragging(), None);
        let moved = desk.geometry(state.size).expect("geometry");
        assert_eq!(moved.conversation.width, g.conversation.width + 6);
        assert_eq!(
            desk.focus.region,
            Region::Object,
            "a separator never takes the keys"
        );
        assert_eq!(composer.text(), "unsent yes");
        assert_eq!(state.focus_scroll, 0);
        // A release lost out of sight: the next move without a button lets go.
        let mut at = |kind, x, y| route(&mut state, &mut desk, &composer, mouse(kind, x, y));
        assert_eq!(
            at(MouseEventKind::Down(MouseButton::Left), x + 6, y),
            Route::Repaint
        );
        assert_eq!(at(MouseEventKind::Moved, x + 20, y), Route::Repaint);
        assert_eq!(at(MouseEventKind::Moved, x + 20, y), Route::Nothing);
        assert_eq!(desk.dragging(), None);
        state.presentation = Presentation::Inline;
        let inline = mouse(MouseEventKind::Down(MouseButton::Left), x + 6, y);
        assert_eq!(
            route(&mut state, &mut desk, &composer, inline),
            Route::Nothing
        );
        assert_eq!(desk.dragging(), None, "inline holds no separator");
    }

    /// The marker of a transcript scrolled back returns it to its latest
    /// row as `End` does, the conversation taking the keys; a click beside it
    /// only focuses; nothing is sent. In both layouts.
    #[test]
    fn the_latest_marker_returns_the_transcript_to_its_newest_row() {
        use crate::workspace::geometry::Layout;
        for layout in Layout::ALL {
            let (mut state, mut desk, composer) = setup((120, 40));
            desk.set_layout(layout);
            desk.focus.region = Region::Object;
            state.focus_scroll = 9;
            let g = desk.geometry(state.size).expect("geometry");
            let thread = desk.screen(state.ascii).thread;
            let transcript = screen::panel_areas(&g, &state, &composer, &thread)[1];
            let marker = screen::latest_area(transcript, state.ascii).expect("marker");
            let click = |x| mouse(MouseEventKind::Down(MouseButton::Left), x, marker.y);
            assert_eq!(
                route(&mut state, &mut desk, &composer, click(marker.x - 2)),
                Route::Repaint
            );
            assert_eq!(desk.focus.region, Region::Conversation);
            assert_eq!(
                state.focus_scroll, 9,
                "{layout:?}: a click beside it only focuses"
            );
            desk.focus.region = Region::Object;
            assert_eq!(
                route(&mut state, &mut desk, &composer, click(marker.x + 2)),
                Route::Repaint
            );
            assert_eq!(state.focus_scroll, 0, "{layout:?}: back to the latest row");
            assert_eq!(desk.focus.region, Region::Conversation);
            assert_eq!(
                route(&mut state, &mut desk, &composer, click(marker.x + 2)),
                Route::Repaint
            );
            assert_eq!(
                state.focus_scroll, 0,
                "at the latest row it is a focus click"
            );
            assert_eq!(composer.text(), "unsent yes");
        }
    }

    #[test]
    fn tabs_hit_only_visible_labels_in_the_painted_title() {
        let row = crate::workspace::inspect::title_row("日本.nika", Face::Source, 70, false, false);
        // The labels stand where the painted title puts them (after the
        // object's own name): measured on the row, never assumed.
        let plan = row.spans.iter().position(|span| span.content == "plan");
        let plan = plan.expect("the plan tab is painted");
        let start: usize = row.spans[..plan].iter().map(|s| s.content.width()).sum();
        let start = u16::try_from(start).expect("cells");
        assert_eq!(span_at(&row, start + 1, 70), Some("plan"));
        assert_eq!(span_at(&row, start - 1, 70), Some(" "));
        assert_eq!(
            span_at(&row, start + 1, start + 2),
            None,
            "partial labels never hit"
        );
        let labels = vec![
            "[run]".into(),
            "outputs".into(),
            "files".into(),
            "proof".into(),
        ];
        let row = Line::from("日本 · [run] outputs files proof");
        assert_eq!(suffix_tab_at(&row, &labels, 14, 80), Some(1));
        assert_eq!(
            suffix_tab_at(&Line::from("日本 · [run] out…"), &labels, 14, 80),
            None
        );
    }

    /// The frame the shell paints for `desk` at the state's size.
    fn painted(state: &UiState, desk: &Desk, composer: &Composer) -> ratatui::buffer::Buffer {
        let (width, height) = state.size;
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
        let paint = crate::workspace::object::Paint {
            ascii: state.ascii,
            color: false,
            elapsed: std::time::Duration::ZERO,
            reduced_motion: true,
        };
        terminal
            .draw(|frame| {
                assert!(crate::workspace::desk::draw(
                    frame, desk, paint, state, composer
                ));
            })
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    /// The text of the cells `from..to` of row `y`.
    fn cells(buffer: &ratatui::buffer::Buffer, from: u16, to: u16, y: u16) -> String {
        (from..to).map(|x| buffer[(x, y)].symbol()).collect()
    }

    /// A press on the object's own action is exactly `F4`: the object grows
    /// (beside the conversation from 100 columns, above it below) while the
    /// draft, the transcript, the reading position and the keys stay, the
    /// change settles once, and the restore form gives back the same frame.
    /// The blank cell before the action is the title row (a focus); under
    /// the folded project list nothing expands.
    #[test]
    fn a_press_on_the_object_action_is_f4_and_changes_only_the_view() {
        let press = |x, y| mouse(MouseEventKind::Down(MouseButton::Left), x, y);
        for size in [(80, 24), (120, 40), (180, 48)] {
            let (mut state, mut desk, composer) = setup(size);
            desk.opened = Some(Target::Workflow("release.nika".to_owned()));
            state.focus_scroll = 7;
            let blocks = state.transcript.len();
            let mut keyboard = Desk::new();
            keyboard.view = desk.view.clone();
            assert_eq!(keyboard.route(key(KeyCode::F(4)), size), Route::Repaint);
            let restored = desk.geometry(size).expect("geometry");
            let (x, y) = (restored.object.right() - 15, restored.object.y);
            let action = cells(&painted(&state, &desk, &composer), x, x + 15, y);
            assert_eq!(action, "[+] Expand · F4", "{size:?}: the object's action");
            assert_eq!(
                route(&mut state, &mut desk, &composer, press(x - 1, y)),
                Route::Repaint
            );
            assert_eq!(desk.arrangement(), Desk::new().arrangement(), "{size:?}");
            desk.focus.region = Region::Conversation;
            assert_eq!(
                route(&mut state, &mut desk, &composer, press(x, y)),
                Route::Repaint
            );
            assert_eq!(
                desk.arrangement(),
                keyboard.arrangement(),
                "{size:?}: not F4"
            );
            let expanded = desk.geometry(size).expect("geometry");
            let area = |r: Rect| u32::from(r.width) * u32::from(r.height);
            assert!(area(expanded.object) > area(restored.object), "{size:?}");
            assert_eq!(
                desk.focus.region,
                Region::Conversation,
                "{size:?}: the keys"
            );
            assert_eq!(composer.text(), "unsent yes");
            assert_eq!((state.transcript.len(), state.focus_scroll), (blocks, 7));
            assert_eq!(desk.take_settled(), Some(desk.arrangement()), "{size:?}");
            assert_eq!(desk.take_settled(), None, "{size:?}: settled once");
            let (x, y) = (expanded.object.right() - 16, expanded.object.y);
            let action = cells(&painted(&state, &desk, &composer), x, x + 16, y);
            assert_eq!(action, "[-] Restore · F4", "{size:?}");
            assert_eq!(
                route(&mut state, &mut desk, &composer, press(x + 15, y)),
                Route::Repaint
            );
            assert_eq!(
                desk.geometry(size),
                Some(restored),
                "{size:?}: not restored"
            );
        }
        let (mut state, mut desk, composer) = setup((80, 24));
        desk.opened = Some(Target::Workflow("release.nika".to_owned()));
        desk.focus.region = Region::Aside;
        let object = desk.geometry(state.size).expect("geometry").object;
        route(
            &mut state,
            &mut desk,
            &composer,
            press(object.right() - 2, object.y),
        );
        assert_eq!(
            desk.arrangement(),
            Desk::new().arrangement(),
            "under the list"
        );
    }

    /// While the project list is folded, a press on a region's name in the
    /// header gives that region the keys and changes nothing else, and the
    /// region that has them asks nothing; from 120 columns no name is drawn.
    #[test]
    fn a_press_on_a_folded_regions_name_moves_only_the_keys() {
        for size in [(80, 24), (100, 32)] {
            let (mut state, mut desk, composer) = setup(size);
            state.focus_scroll = 5;
            for (name, region) in [
                ("Project", Region::Aside),
                ("Object", Region::Object),
                ("Conversation", Region::Conversation),
                ("Conversation", Region::Conversation),
            ] {
                let row = cells(&painted(&state, &desk, &composer), 0, size.0, 0);
                let words = match desk.focus.region {
                    Region::Aside => "[Project] Conversation Object",
                    Region::Object => "Project Conversation [Object]",
                    Region::Conversation => "Project [Conversation] Object",
                };
                assert!(row.ends_with(words), "{size:?}: {row}");
                let start = size.0 - u16::try_from(words.len()).expect("cells");
                let at = start + u16::try_from(words.find(name).expect("named")).expect("cells");
                let moved = desk.focus.region != region;
                let press = mouse(MouseEventKind::Down(MouseButton::Left), at + 1, 0);
                let routed = route(&mut state, &mut desk, &composer, press);
                assert_eq!(routed == Route::Repaint, moved, "{size:?} {name}");
                assert_eq!(desk.focus.region, region, "{size:?} {name}");
                assert_eq!(desk.arrangement(), Desk::new().arrangement(), "{size:?}");
                assert_eq!(
                    (composer.text(), state.focus_scroll),
                    ("unsent yes".into(), 5)
                );
            }
        }
        let (state, desk, composer) = setup((120, 40));
        let row = cells(&painted(&state, &desk, &composer), 0, 120, 0);
        assert!(
            !row.contains("Conversation") && !row.contains("Workbench"),
            "{row}"
        );
    }

    /// The demo project with `opened` in view and, when `pinned`, a run pinned.
    fn opened(size: (u16, u16), pinned: bool) -> (UiState, Desk, Composer) {
        let (state, mut desk, composer) = setup(size);
        desk.opened = Some(Target::Workflow("release.nika".to_owned()));
        if pinned {
            let run = crate::workspace::pinned::Pinned::new(
                "demo",
                "release.nika",
                "#7",
                nika_display::state::TaskState::Paused,
                "waiting",
            );
            desk.view.as_mut().expect("the demo project").pinned = Some(run);
        }
        (state, desk, composer)
    }

    /// Where expanding would not give the object more cells, no `[+]` is
    /// painted and a press at the end of its first row expands nothing (the
    /// object takes the keys, as a press on its title does); a kept expansion
    /// there still paints its restore, and a press on it restores.
    #[test]
    fn a_press_never_expands_an_object_that_cannot_grow() {
        use crate::workspace::geometry::{Arrangement, Layout};
        let press = |x, y| mouse(MouseEventKind::Down(MouseButton::Left), x, y);
        for (size, pinned) in [
            ((60, 16), false),
            ((60, 16), true),
            ((80, 17), true),
            ((99, 16), false),
        ] {
            let (mut state, mut desk, composer) = opened(size, pinned);
            let at = format!("{size:?} pinned={pinned}");
            let object = desk.geometry(size).expect("geometry").object;
            let row = cells(
                &painted(&state, &desk, &composer),
                object.x,
                object.right(),
                object.y,
            );
            assert!(!row.contains("[+]"), "{at}: {row}");
            route(
                &mut state,
                &mut desk,
                &composer,
                press(object.right() - 2, object.y),
            );
            assert_eq!(desk.arrangement(), Desk::new().arrangement(), "{at}");
            assert_eq!(desk.take_settled(), None, "{at}");
            desk.arrange(Arrangement::of(Layout::Workbench));
            let object = desk.geometry(size).expect("geometry").object;
            let row = cells(
                &painted(&state, &desk, &composer),
                object.x,
                object.right(),
                object.y,
            );
            assert!(row.ends_with("[-] Restore · F4"), "{at}: {row}");
            route(
                &mut state,
                &mut desk,
                &composer,
                press(object.right() - 2, object.y),
            );
            assert_eq!(desk.arrangement().layout, Layout::Session, "{at}");
            assert_eq!(composer.text(), "unsent yes", "{at}");
        }
    }

    /// A long project name never hides the folded regions: a press on each
    /// region's name moves only the keys, with a one-row and a two-row
    /// header, wide glyphs and both glyph columns.
    #[test]
    fn a_long_project_name_keeps_each_folded_region_pressable() {
        use crate::workspace::project::ProjectView;
        for size in [(60, 16), (60, 30)] {
            for name in [
                "customer-onboarding-automation",
                "顧客オンボーディング-automation",
            ] {
                for ascii in [false, true] {
                    let (mut state, mut desk, composer) = setup(size);
                    state.ascii = ascii;
                    state.focus_scroll = 5;
                    let location = format!("~/Projects/{name}");
                    desk.view = Some(ProjectView::new("local", name, location));
                    for (target, region) in [
                        ("Project", Region::Aside),
                        ("Object", Region::Object),
                        ("Conversation", Region::Conversation),
                    ] {
                        let at = format!("{size:?} {name} ascii={ascii} {target}");
                        let words = match desk.focus.region {
                            Region::Aside => "[Project] Conversation Object",
                            Region::Object => "Project Conversation [Object]",
                            Region::Conversation => "Project [Conversation] Object",
                        };
                        let row = cells(&painted(&state, &desk, &composer), 0, size.0, 0);
                        assert!(row.ends_with(words), "{at}: {row}");
                        let start = size.0 - u16::try_from(words.len()).expect("cells");
                        let offset = words.find(target).expect("named");
                        let x = start + u16::try_from(offset).expect("cells") + 1;
                        let press = mouse(MouseEventKind::Down(MouseButton::Left), x, 0);
                        route(&mut state, &mut desk, &composer, press);
                        assert_eq!(desk.focus.region, region, "{at}");
                        assert_eq!(desk.arrangement(), Desk::new().arrangement(), "{at}");
                        assert_eq!(
                            (composer.text(), state.focus_scroll),
                            ("unsent yes".into(), 5),
                            "{at}"
                        );
                    }
                }
            }
        }
    }
}
