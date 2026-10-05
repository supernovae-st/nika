// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pointer navigation and input while a turn runs. All coordinates are judged
//! against the current frame geometry; no click becomes a submitted line.
//! Hold Shift for the terminal's own selection/copy where supported; F6,
//! arrows, PageUp/PageDown and Enter remain the complete keyboard path.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use nika_tui_view::Face;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use unicode_width::UnicodeWidthStr;

use super::{
    Busy, Composer, ComposerAction, Conversation, Desk, ENTER_WAITS, Exit, Heard, LEAVE_WAITS,
    Presentation, Route, Shell, Signal, UiEvent, UiState, busy_key, is_ctrl_c,
};
use crate::workspace::{
    aside, focus::Region, geometry::Geometry, live::RunFace, object::Object, project::Target,
};

const WHEEL_ROWS: usize = 3;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Only real painted regions take the pointer. Inline leaves selection and
/// scrollback to the terminal; a workspace too small follows its focus fallback.
fn route(state: &mut UiState, desk: &mut Desk, composer: &Composer, mouse: MouseEvent) -> Route {
    if state.presentation == Presentation::Inline || mouse.modifiers != KeyModifiers::NONE {
        return Route::Nothing;
    }
    let area = Rect::new(0, 0, state.size.0, state.size.1);
    let point = Position::new(mouse.column, mouse.row);
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
        .then(|| Geometry::of(area, desk.pins()))
        .flatten();
    let Some(geometry) = geometry else {
        if let Some(older) = older {
            crate::scroll::rows(state, desk, composer, older, WHEEL_ROWS);
            return Route::Repaint;
        }
        return Route::Nothing;
    };
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
            if mouse.row == geometry.object.y {
                return face_pointer(
                    state,
                    desk,
                    mouse.column - geometry.object.x,
                    geometry.object.width,
                );
            }
        }
        return Route::Repaint;
    }
    Route::Nothing
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
    let steps = if desk.opened == Some(Target::Live) {
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

    pub(super) fn on_mouse(&mut self, mouse: MouseEvent) {
        let routed = route(&mut self.state, &mut self.desk, &self.composer, mouse);
        if routed != Route::Nothing {
            self.state.interrupt_armed = false;
            self.state.completion = None;
        }
        if routed == Route::Inspect {
            self.look();
        }
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
                return Self::arm(armed).map_or(Heard::Nothing, Heard::Leave);
            }
            UiEvent::Key(key) if is_ctrl_c(&key) => {
                return Self::arm(armed).map_or(Heard::Nothing, Heard::Leave);
            }
            UiEvent::Paste(text) => {
                self.state.completion = None;
                self.composer.paste(&text);
                self.typed_live = true;
                return Heard::Redraw;
            }
            UiEvent::Mouse(mouse) => {
                self.on_mouse(mouse);
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
                self.state.completion = Some(ENTER_WAITS.to_owned());
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

    #[test]
    fn tabs_hit_only_visible_labels_in_the_painted_title() {
        let row = crate::workspace::inspect::title_row("日本.nika", Face::Source, 70, false, false);
        assert_eq!(span_at(&row, 10, 70), Some("plan"));
        assert_eq!(span_at(&row, 8, 70), Some(" "));
        assert_eq!(span_at(&row, 10, 10), None, "partial labels never hit");
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
}
