// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One frame of the workspace screen: the header, the project aside, the
//! object in view, the conversation with its composer and the pinned run, each
//! in the region [`Geometry`] gives it. The facts come from the view the
//! caller passes; the transcript, the status, the composer and the hint are
//! painted by the same functions as the focus presentation, so typing, pasting
//! and history behave the same in every presentation.

use nika_display::theme::Role;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use super::aside::{self, Aside};
use super::conversation::{self, Thread};
use super::focus::{Extent, Focus, Region};
use super::geometry::Geometry;
use super::header::{self, Place};
use super::object::{self, Object, Paint};
use super::pinned::{self, Pinned};
use crate::composer::Composer;
use crate::model::UiState;
use crate::render::{live_rows, render_live, render_transcript};
use crate::visual::role;

/// Everything one workspace frame shows, as the Session projects it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Screen {
    /// Where the human stands (the header).
    pub place: Place,
    /// What the active project holds (the aside).
    pub aside: Aside,
    /// What the centre shows.
    pub object: Object,
    /// The conversation the composer writes to.
    pub thread: Thread,
    /// The run pinned in view, if any.
    pub pinned: Option<Pinned>,
}

impl Screen {
    /// A screen of these regions, no run pinned.
    #[must_use]
    pub fn new(place: Place, aside: Aside, object: Object, thread: Thread) -> Self {
        Self {
            place,
            aside,
            object,
            thread,
            pinned: None,
        }
    }

    /// This screen with `run` pinned.
    #[must_use]
    pub fn pinning(mut self, run: Pinned) -> Self {
        self.pinned = Some(run);
        self
    }
}

/// What the regions hold on a frame of `area`, for [`Focus::handle`]; none
/// when the terminal is below
/// [`super::geometry::MIN_SIZE`]. The aside is always reachable: where the
/// width folds it, it is drawn over the object while it holds the keys, so
/// a workflow can be chosen at every size the workspace fits.
#[must_use]
pub fn extent(screen: &Screen, area: Rect) -> Option<Extent> {
    let geometry = Geometry::of(area, screen.pinned.is_some())?;
    Some(Extent {
        aside_shown: true,
        aside_entries: screen.aside.entries.len(),
        object_lines: object::length(&screen.object),
        // The title row stays; the rest scrolls.
        object_rows: geometry.object.height.saturating_sub(1),
    })
}

/// Draw the workspace on the whole frame, the aside selection and the object
/// scroll following `focus`. Returns `false`, drawing nothing, when the
/// terminal is below [`super::geometry::MIN_SIZE`]: the caller draws the focus
/// view there.
pub fn draw(
    frame: &mut Frame<'_>,
    screen: &Screen,
    paint: Paint,
    focus: &Focus,
    state: &UiState,
    composer: &Composer,
) -> bool {
    let Some(geometry) = Geometry::of(frame.area(), screen.pinned.is_some()) else {
        return false;
    };
    let (ascii, color) = (paint.ascii, paint.color);
    header::render(
        &screen.place,
        geometry.header,
        frame.buffer_mut(),
        ascii,
        color,
    );
    let selected = (focus.region == Region::Aside).then_some(focus.selected);
    if let Some(area) = geometry.aside {
        let [list, edge] =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        let rows = aside::lines_selecting(
            &screen.aside,
            list.width,
            list.height,
            ascii,
            color,
            selected,
        );
        frame.render_widget(Paragraph::new(rows), list);
        rule_column(edge, ascii, color, frame.buffer_mut());
    } else if selected.is_some() {
        // The width folds the aside: while it holds the keys it stands over
        // the object, which returns as soon as the keys leave it.
        let area = geometry.object;
        let rows = aside::lines_selecting(
            &screen.aside,
            area.width,
            area.height,
            ascii,
            color,
            selected,
        );
        frame.render_widget(Paragraph::new(rows), area);
    }
    if geometry.aside.is_some() || selected.is_none() {
        object::render_from(
            &screen.object,
            geometry.object,
            frame.buffer_mut(),
            paint,
            focus.scroll,
        );
    }
    panel(frame, screen, &geometry, paint, state, composer);
    if let (Some(run), Some(area)) = (&screen.pinned, geometry.pinned) {
        let row = pinned::line(run, area.width, ascii, color);
        frame.render_widget(Paragraph::new(row), area);
    }
    true
}

/// The conversation panel: its title, the transcript, the context row, then
/// the live area (status, prompt and composer, hint). Beside the object a rule
/// column separates it; under the object its title is the separator.
fn panel(
    frame: &mut Frame<'_>,
    screen: &Screen,
    geometry: &Geometry,
    paint: Paint,
    state: &UiState,
    composer: &Composer,
) {
    let (ascii, color) = (paint.ascii, paint.color);
    let mut area = geometry.conversation;
    if !geometry.stacked {
        // The rule, then one blank column so no word touches it.
        let [edge, _, rest] = Layout::horizontal([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas(area);
        rule_column(edge, ascii, color, frame.buffer_mut());
        area = rest;
    }
    let live = live_rows(state, composer, area.width, area.height.saturating_sub(2));
    let [title, transcript, context, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(live),
    ])
    .areas(area);
    let heading = conversation::title(&screen.thread, title.width, ascii, color, geometry.stacked);
    frame.render_widget(Paragraph::new(heading), title);
    render_transcript(frame, state, transcript);
    let with = conversation::context(&screen.thread, context.width, ascii, color);
    frame.render_widget(Paragraph::new(with), context);
    render_live(frame, state, composer, bottom);
}

/// A dim vertical rule filling the one-column `area`.
fn rule_column(area: Rect, ascii: bool, color: bool, buf: &mut Buffer) {
    let glyph = if ascii { "|" } else { "│" };
    let style = role::style(Role::Dim, color);
    for y in area.top()..area.bottom() {
        buf.set_line(area.x, y, &Line::styled(glyph, style), area.width);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::time::Duration;

    use nika_display::state::TaskState;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::model::{Presentation, Script};
    use crate::visual::icon::Icon;
    use crate::visual::logomark::{REVEAL_ENDS, Size};
    use crate::workspace::aside::{Entry, Tab};

    /// The four sizes the terminal matrix qualifies.
    const SIZES: [(u16, u16); 4] = [(80, 24), (100, 32), (120, 40), (160, 48)];

    fn screen(object: Object) -> Screen {
        let place = Place::on("local")
            .with_project("studio", "~/Projects/studio")
            .observed(true, false);
        let aside = Aside::new(
            "studio",
            Tab::Nika,
            vec![
                Entry::new(Icon::Conversation, "release checklist").opened(),
                Entry::new(Icon::Workflow, "release.nika"),
                Entry::new(Icon::Run, "#043").at(1),
            ],
            true,
        );
        let thread = Thread::new("studio", "release checklist").viewing("release.nika");
        let run = Pinned::new(
            "studio",
            "release.nika",
            "#043",
            TaskState::Paused,
            "waiting for your approval",
        )
        .offering("answer the gate");
        Screen::new(place, aside, object, thread).pinning(run)
    }

    fn welcome() -> Object {
        Object::Welcome {
            words: vec!["Describe the work you want to automate.".to_owned()],
        }
    }

    fn paint(ascii: bool) -> Paint {
        Paint {
            ascii,
            color: false,
            elapsed: REVEAL_ENDS,
            reduced_motion: false,
        }
    }

    /// Draw `screen` at `width` × `height` after the demo exchange.
    fn draw_at(screen: &Screen, width: u16, height: u16, paint: Paint) -> (bool, Vec<String>) {
        let (drawn, rows, _) = draw_focused(screen, width, height, paint, &Focus::composing());
        (drawn, rows)
    }

    /// Draw `screen` with the keyboard at `focus`; the buffer keeps the styles.
    fn draw_focused(
        screen: &Screen,
        width: u16,
        height: u16,
        paint: Paint,
        focus: &Focus,
    ) -> (bool, Vec<String>, ratatui::buffer::Buffer) {
        let mut state = UiState::new(Presentation::Focus, false, (width, height));
        // One glyph column for the whole frame: the caller sets both from its theme.
        state.ascii = paint.ascii;
        let mut script = Script::demo();
        for beat in script.open() {
            state.apply(beat);
        }
        for beat in script.submit("digest my notes") {
            state.apply(beat);
        }
        let mut composer = Composer::new();
        composer.set_placeholder(&conversation::placeholder(&screen.thread));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        let mut drawn = false;
        terminal
            .draw(|frame| drawn = draw(frame, screen, paint, focus, &state, &composer))
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        let rows = (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_owned())
                    .collect()
            })
            .collect();
        (drawn, rows, buffer)
    }

    fn find(rows: &[String], needle: &str) -> Option<usize> {
        rows.iter().position(|row| row.contains(needle))
    }

    #[test]
    fn every_qualified_size_shows_the_five_regions() {
        let screen = screen(welcome());
        for (width, height) in SIZES {
            let (drawn, rows) = draw_at(&screen, width, height, paint(false));
            assert!(drawn, "{width}x{height}");
            assert!(rows[0].contains("studio"), "{width}x{height}: {}", rows[0]);
            let last = rows.last().expect("rows");
            assert!(last.contains("#043") && last.contains("waiting for your approval"));
            let title = find(&rows, "◌ release checklist · studio").expect("thread title");
            let context = find(&rows, "nothing attached").expect("context row");
            assert!(
                rows[context].contains("on screen: rel"),
                "{}",
                rows[context]
            );
            assert!(title < context, "{width}x{height}");
            // The placeholder names the recipient; a narrow composer cuts its end,
            // and the title row above still names the thread in full.
            assert!(
                find(&rows, "Message to studio / rel").is_some(),
                "{width}x{height}: the composer names the recipient"
            );
            assert_eq!(
                find(&rows, "in studio").is_some(),
                width >= 120,
                "{width}x{height}: the aside appears from 120 columns"
            );
        }
    }

    #[test]
    fn the_welcome_mark_leaves_the_composer_its_rows() {
        let screen = screen(welcome());
        for ((width, height), size) in [((80, 24), Size::Compact), ((120, 40), Size::Board)] {
            let (_, rows) = draw_at(&screen, width, height, paint(false));
            let mark = size.lines();
            let middle = mark[mark.len() / 2].trim();
            let at = find(&rows, middle).expect("mark drawn");
            let words = find(&rows, "Describe the work you want to automate.").expect("words");
            assert!(at < words, "{width}x{height}");
            let composer = find(&rows, "Message to studio").expect("composer");
            assert!(words < composer || width >= 100, "{width}x{height}");
        }
    }

    #[test]
    fn under_the_object_the_panel_title_is_a_rule_and_beside_it_a_column() {
        let screen = screen(welcome());
        let (_, narrow) = draw_at(&screen, 80, 24, paint(false));
        let title = find(&narrow, "◌ release checklist").expect("title");
        assert!(narrow[title].starts_with("── ◌ release checklist · studio ─"));
        let (_, wide) = draw_at(&screen, 120, 40, paint(false));
        let title = find(&wide, "◌ release checklist · studio").expect("title");
        assert!(
            wide[title].contains("│ ◌ release checklist"),
            "a rule column, then one blank column: {}",
            wide[title]
        );
    }

    #[test]
    fn the_ascii_column_keeps_the_chrome_ascii() {
        let object = Object::Shown {
            icon: Icon::Workflow,
            name: "release.nika".to_owned(),
            lines: vec!["nika: release".to_owned()],
        };
        let screen = screen(object);
        let (_, rows) = draw_at(&screen, 120, 40, paint(true));
        let title = find(&rows, "[C] release checklist - studio").expect("title");
        let context = find(&rows, "- nothing attached").expect("context row");
        let object = find(&rows, "[W] release.nika").expect("object title");
        let prompt = find(&rows, "reply > ").expect("the prompt marker twin");
        for y in [
            0,
            1,
            title,
            context,
            object,
            prompt,
            prompt + 1,
            rows.len() - 1,
        ] {
            assert!(rows[y].is_ascii(), "row {y}: {}", rows[y]);
        }
        assert!(rows[2].contains('|'), "the aside edge: {}", rows[2]);
    }

    #[test]
    fn the_keyboard_selects_in_the_aside_and_scrolls_the_object() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use ratatui::style::Modifier;
        let lines: Vec<String> = (1..=60).map(|n| format!("line {n}")).collect();
        let object = Object::Shown {
            icon: Icon::Workflow,
            name: "release.nika".to_owned(),
            lines,
        };
        let screen = screen(object);
        let area = Rect::new(0, 0, 120, 40);
        let room = extent(&screen, area).expect("fits");
        assert!(room.aside_shown && room.aside_entries == 3 && room.object_lines == 60);
        let mut focus = Focus::composing();
        let press = |code| KeyEvent::new(code, KeyModifiers::NONE);
        focus.handle(press(KeyCode::F(6)), room);
        focus.handle(press(KeyCode::Down), room);
        let (_, rows, buffer) = draw_focused(&screen, 120, 40, paint(false), &focus);
        // The aside holds the first 20 columns at 120 (its rule is the 21st).
        let aside_part = |row: &String| row.chars().take(20).collect::<String>();
        let at = rows
            .iter()
            .position(|r| aside_part(r).contains("release.nika"))
            .expect("aside row");
        let column = aside_part(&rows[at])
            .chars()
            .position(|c| c == 'r')
            .expect("label");
        let (x, y) = (
            u16::try_from(column).expect("x"),
            u16::try_from(at).expect("y"),
        );
        assert!(
            buffer[(x, y)].modifier.contains(Modifier::REVERSED),
            "{}",
            rows[at]
        );
        let opened = rows
            .iter()
            .position(|r| aside_part(r).contains("release checklist"))
            .expect("open row");
        let (ox, oy) = (x, u16::try_from(opened).expect("y"));
        assert!(!buffer[(ox, oy)].modifier.contains(Modifier::REVERSED));
        // The object scrolls when it has the keys; its title row stays.
        focus.handle(press(KeyCode::F(6)), room);
        focus.handle(press(KeyCode::PageDown), room);
        let (_, rows, _) = draw_focused(&screen, 120, 40, paint(false), &focus);
        assert!(find(&rows, "⑂ release.nika").is_some());
        assert!(
            find(&rows, "line 1 ").is_none() && find(&rows, "line 37").is_some(),
            "{rows:#?}"
        );
        assert!(extent(&screen, Rect::new(0, 0, 59, 40)).is_none());
    }

    #[test]
    fn a_terminal_below_the_minimum_draws_nothing() {
        let (drawn, rows) = draw_at(&screen(welcome()), 59, 40, paint(false));
        assert!(!drawn);
        assert!(rows.iter().all(|row| row.trim().is_empty()));
    }

    #[test]
    fn the_reveal_starts_sparse_and_ends_on_the_mark() {
        let screen = screen(welcome());
        let start = Paint {
            elapsed: Duration::ZERO,
            ..paint(false)
        };
        let (_, first) = draw_at(&screen, 120, 40, start);
        let (_, last) = draw_at(&screen, 120, 40, paint(false));
        assert_ne!(first, last);
        let reduced = Paint {
            elapsed: Duration::ZERO,
            reduced_motion: true,
            ..paint(false)
        };
        assert_eq!(draw_at(&screen, 120, 40, reduced).1, last);
    }
}
