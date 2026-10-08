// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One frame of the workspace screen: the header, the project aside, the
//! object in view, the conversation with its composer and the pinned run, each
//! in the region [`Geometry`] gives it. The facts come from the view the
//! caller passes; the transcript, the status, the composer and the hint are
//! painted by the same functions as the focus presentation, so typing, pasting
//! and history behave the same in every presentation.
//!
//! The chrome is painted once: the header's right end names the two layouts,
//! the one in view in brackets, and the key between them (`F4`); a
//! transcript scrolled back carries one marker back to its latest row; a
//! separator the pointer holds is shown reversed. Each is a plain word or a
//! weight, never a hue alone, and none of them moves.

use nika_display::theme::Role;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use super::aside::{self, Aside};
use super::conversation::{self, Thread};
use super::focus::{Extent, Focus, Region};
use super::geometry::{self, Arrangement, Geometry, Separator};
use super::header::{self, Place};
use super::object::{self, Object, Paint};
use super::pinned::{self, Pinned};
use super::text::marks;
use crate::composer::Composer;
use crate::model::UiState;
use crate::render::{activity_marker, live_rows, render_live, render_transcript, wrapped_rows};
use crate::visual::role;

/// Everything one workspace frame shows, as the Session projects it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Screen {
    /// Where the human stands (the header).
    pub place: Place,
    /// What the active project holds (the aside).
    pub aside: Aside,
    /// What the preview on the right shows.
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

/// What the regions hold on a frame of `area` in the Session layout, for
/// [`Focus::handle`]; none when the terminal is below
/// [`super::geometry::MIN_SIZE`]. The aside is always reachable: where the
/// width folds it, it is drawn over the object while it holds the keys, so
/// a workflow can be chosen at every size the workspace fits.
#[must_use]
pub fn extent(screen: &Screen, area: Rect) -> Option<Extent> {
    let geometry = Geometry::of(area, screen.pinned.is_some())?;
    Some(extent_in(screen, &geometry))
}

/// What the regions hold in `geometry`, whatever its arrangement.
#[must_use]
pub(crate) fn extent_in(screen: &Screen, geometry: &Geometry) -> Extent {
    Extent {
        aside_shown: true,
        aside_entries: screen.aside.entries.len(),
        object_lines: object::length(&screen.object),
        // The title row stays; the rest scrolls.
        object_rows: geometry.object.height.saturating_sub(1),
    }
}

/// How a frame is arranged beyond its facts: the layout and the separators'
/// shares, and the separator the pointer holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Chrome {
    /// The layout in view and the separators' shares.
    pub(crate) arrangement: Arrangement,
    /// The separator the pointer is moving, shown reversed.
    pub(crate) dragging: Option<Separator>,
}

/// Draw the workspace on the whole frame in the Session layout, the aside
/// selection and the object scroll following `focus`. Returns `false`,
/// drawing nothing, when the terminal is below [`super::geometry::MIN_SIZE`]:
/// the caller draws the focus view there.
pub fn draw(
    frame: &mut Frame<'_>,
    screen: &Screen,
    paint: Paint,
    focus: &Focus,
    state: &UiState,
    composer: &Composer,
) -> bool {
    let chrome = Chrome {
        arrangement: Arrangement::of(geometry::Layout::Session),
        dragging: None,
    };
    draw_in(frame, screen, paint, focus, state, composer, chrome)
}

/// [`draw`] in the arrangement `chrome` names.
pub(crate) fn draw_in(
    frame: &mut Frame<'_>,
    screen: &Screen,
    paint: Paint,
    focus: &Focus,
    state: &UiState,
    composer: &Composer,
    chrome: Chrome,
) -> bool {
    let area = frame.area();
    let pinned = screen.pinned.is_some();
    let Some(geometry) = Geometry::arranged(area, pinned, &chrome.arrangement) else {
        return false;
    };
    let (ascii, color) = (paint.ascii, paint.color);
    frame
        .buffer_mut()
        .set_style(area, role::surface(color, false));
    frame
        .buffer_mut()
        .set_style(geometry.header, role::surface(color, true));
    header_row(
        &screen.place,
        geometry.header,
        frame.buffer_mut(),
        chrome.arrangement.layout,
        (ascii, color),
    );
    aside_and_object(frame, screen, &geometry, paint, focus);
    panel(
        frame,
        screen,
        &geometry,
        paint,
        state,
        composer,
        focus.region,
    );
    if let (Some(run), Some(area)) = (&screen.pinned, geometry.pinned) {
        let row = pinned::line(run, area.width, ascii, color);
        frame.render_widget(Paragraph::new(row), area);
    }
    if let Some(cells) = chrome.dragging.and_then(|held| geometry.handle(held)) {
        // The separator the pointer holds, reversed: a weight, not a hue.
        let held = Style::default().add_modifier(Modifier::REVERSED);
        frame.buffer_mut().set_style(cells, held);
    }
    true
}

/// The project aside (or, where the width folds it, the aside over the
/// object while it holds the keys) and the object in view, its title row
/// underlined while it holds the keys.
fn aside_and_object(
    frame: &mut Frame<'_>,
    screen: &Screen,
    geometry: &Geometry,
    paint: Paint,
    focus: &Focus,
) {
    let (ascii, color) = (paint.ascii, paint.color);
    let selected = (focus.region == Region::Aside).then_some(focus.selected);
    if let Some(area) = geometry.aside {
        frame
            .buffer_mut()
            .set_style(area, role::surface(color, true));
        let [list, edge] =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        let rows = aside::lines_anchored(
            &screen.aside,
            list.width,
            list.height,
            ascii,
            color,
            selected,
            focus.selected,
        );
        frame.render_widget(Paragraph::new(rows), list);
        rule_column(
            edge,
            ascii,
            color,
            focus.region == Region::Aside,
            frame.buffer_mut(),
        );
    } else if selected.is_some() {
        // The width folds the aside: while it holds the keys it stands over
        // the object, which returns as soon as the keys leave it.
        let area = geometry.object;
        let rows = aside::lines_anchored(
            &screen.aside,
            area.width,
            area.height,
            ascii,
            color,
            selected,
            focus.selected,
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
    if focus.region == Region::Object {
        frame.buffer_mut().set_style(
            Rect::new(
                geometry.object.x,
                geometry.object.y,
                geometry.object.width,
                1,
            ),
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::UNDERLINED),
        );
    }
}

/// What a click on the layout switch asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Switch {
    /// Show this layout.
    To(geometry::Layout),
    /// Show the other one (the switch's key, `F4`).
    Toggle,
}

/// The layout switch: both layouts, the one in view in brackets and strong,
/// then the key between them; each part with what a click on it asks. Its
/// width does not change with the layout in view.
fn switch_parts(
    layout: geometry::Layout,
    ascii: bool,
    color: bool,
) -> Vec<(Span<'static>, Option<Switch>)> {
    let dim = role::style(Role::Dim, color);
    let mut parts = Vec::new();
    for (index, each) in geometry::Layout::ALL.into_iter().enumerate() {
        if index > 0 {
            parts.push((Span::raw(" "), None));
        }
        let part = if each == layout {
            Span::styled(
                format!("[{}]", each.label()),
                role::style(Role::Strong, color),
            )
        } else {
            Span::styled(each.label(), dim)
        };
        parts.push((part, Some(Switch::To(each))));
    }
    parts.push((Span::styled(marks(ascii).0, dim), None));
    parts.push((Span::styled("F4", dim), Some(Switch::Toggle)));
    parts
}

/// Where the layout switch stands in `header`: the right end of its first
/// row, one blank cell after the place, when that row has room for both;
/// `None` otherwise (`F4` still switches).
#[must_use]
pub(crate) fn switch_area(place: &Place, header: Rect, ascii: bool) -> Option<Rect> {
    let parts = switch_parts(geometry::Layout::Session, ascii, false);
    let width: usize = parts.iter().map(|(span, _)| span.content.width()).sum();
    let width = u16::try_from(width).ok()?;
    let room = header.width.checked_sub(width + 1)?;
    let first = header::lines(place, room, header.height, ascii, false);
    let fits = first
        .first()
        .is_some_and(|line| line.width() <= usize::from(room));
    (fits && header.height > 0).then(|| Rect::new(header.right() - width, header.y, width, 1))
}

/// What a click at `column` of the header row asks of the layout switch.
#[must_use]
pub(crate) fn switch_at(
    place: &Place,
    header: Rect,
    layout: geometry::Layout,
    ascii: bool,
    column: u16,
) -> Option<Switch> {
    let area = switch_area(place, header, ascii)?;
    let offset = usize::from(column.checked_sub(area.x)?);
    let mut start = 0;
    for (span, asks) in switch_parts(layout, ascii, false) {
        let end = start + span.content.width();
        if (start..end).contains(&offset) {
            return asks;
        }
        start = end;
    }
    None
}

/// The header with the layout switch at the right end of its first row; the
/// place alone where the row has no room for both.
fn header_row(
    place: &Place,
    area: Rect,
    buf: &mut Buffer,
    layout: geometry::Layout,
    (ascii, color): (bool, bool),
) {
    let Some(switch) = switch_area(place, area, ascii) else {
        header::render(place, area, buf, ascii, color);
        return;
    };
    let room = switch.x - area.x - 1;
    let first = header::lines(place, room, area.height, ascii, color);
    let full = header::lines(place, area.width, area.height, ascii, color);
    let mut rows = first.into_iter().take(1).chain(full.into_iter().skip(1));
    if let Some(line) = rows.next() {
        Paragraph::new(line).render(Rect::new(area.x, area.y, room, 1), buf);
    }
    for (offset, line) in (1..area.height).zip(rows) {
        Paragraph::new(line).render(Rect::new(area.x, area.y + offset, area.width, 1), buf);
    }
    let parts = switch_parts(layout, ascii, color);
    let line = Line::from(parts.into_iter().map(|(span, _)| span).collect::<Vec<_>>());
    Paragraph::new(line).render(switch, buf);
}

/// The marker that returns a scrolled transcript to its latest row.
fn latest_line(ascii: bool, color: bool) -> Line<'static> {
    let words = if ascii { " v latest " } else { " ↓ latest " };
    let style = role::style(Role::Accent, color).add_modifier(Modifier::REVERSED);
    Line::from(Span::styled(words, style))
}

/// Where the marker back to the latest row stands on a `transcript` scrolled
/// back: the right end of its last row; `None` where the row cannot hold it.
#[must_use]
pub(crate) fn latest_area(transcript: Rect, ascii: bool) -> Option<Rect> {
    let width = u16::try_from(latest_line(ascii, false).width()).ok()?;
    (transcript.height > 0 && transcript.width >= width).then(|| {
        Rect::new(
            transcript.right() - width,
            transcript.bottom() - 1,
            width,
            1,
        )
    })
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
    focused: Region,
) {
    let (ascii, color) = (paint.ascii, paint.color);
    let area = geometry.conversation;
    if !geometry.stacked {
        // The rule follows a free gutter at the right edge, beside the preview.
        let [_, _, edge] = Layout::horizontal([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);
        rule_column(
            edge,
            ascii,
            color,
            focused != Region::Aside,
            frame.buffer_mut(),
        );
    }
    let [title, transcript, context, bottom] = panel_areas(
        geometry,
        state,
        composer,
        screen.thread.intelligence.as_deref(),
    );
    let marker = activity_marker(state);
    let prefix = marker.as_ref().map_or(0, |mark| {
        u16::try_from(mark.width() + 1).unwrap_or(u16::MAX)
    });
    let mut heading = conversation::title(
        &screen.thread,
        title.width.saturating_sub(prefix),
        ascii,
        color,
        geometry.stacked,
    );
    if let Some(marker) = marker {
        heading.spans.splice(0..0, [marker, Span::raw(" ")]);
    }
    let heading = if focused == Region::Conversation {
        heading.style(
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::UNDERLINED),
        )
    } else {
        heading
    };
    let mut title_lines = vec![heading];
    if title.height > 1 {
        title_lines.push(preparation_line(
            screen.thread.intelligence.as_deref(),
            color,
        ));
    }
    frame.render_widget(
        Paragraph::new(title_lines).wrap(ratatui::widgets::Wrap { trim: false }),
        title,
    );
    render_transcript(frame, state, transcript);
    if state.focus_scroll > 0
        && let Some(marker) = latest_area(transcript, ascii)
    {
        // Scrolled back, new activity keeps the reading place; this marker
        // (or `End`) is the way back, never an automatic jump.
        frame.render_widget(Paragraph::new(latest_line(ascii, color)), marker);
    }
    let with = conversation::context(&screen.thread, context.width, ascii, color);
    frame.render_widget(Paragraph::new(with), context);
    render_live(frame, state, composer, bottom);
}

/// The selection is for preparation, not evidence that this model answered a turn.
fn preparation_line(intelligence: Option<&str>, color: bool) -> Line<'static> {
    let seat = intelligence.unwrap_or("not selected - /intelligence to choose; asked when needed");
    Line::from(vec![
        Span::styled("Prepare with: ", role::style(Role::VerbAgent, color)),
        Span::styled(seat.to_owned(), role::style(Role::Dim, color)),
    ])
}

/// The exact conversation rectangles, shared by painting and scroll bounds.
pub(crate) fn panel_areas(
    geometry: &Geometry,
    state: &UiState,
    composer: &Composer,
    intelligence: Option<&str>,
) -> [Rect; 4] {
    let area = if geometry.stacked {
        geometry.conversation
    } else {
        // One left inset, then the content, a free gutter and the separator.
        let region = geometry.conversation;
        Rect::new(
            region.x + 1,
            region.y,
            region.width.saturating_sub(3),
            region.height,
        )
    };
    let base_heading = if area.height >= 12 { 3 } else { 1 };
    // Keep the existing live budget: a model label must not displace activity or typing.
    let live = live_rows(
        state,
        composer,
        area.width,
        area.height.saturating_sub(base_heading + 1),
    );
    let heading = intelligence.map_or(base_heading, |_| {
        let needed = wrapped_rows(&[preparation_line(intelligence, false)], area.width) + 1;
        // Leave the context row and at least one transcript row beside the live area.
        needed
            .max(base_heading)
            .min(area.height.saturating_sub(live + 2).max(1))
    });
    Layout::vertical([
        Constraint::Length(heading),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(live),
    ])
    .areas(area)
}

/// A dim vertical rule filling the one-column `area`.
fn rule_column(area: Rect, ascii: bool, color: bool, active: bool, buf: &mut Buffer) {
    let glyph = if ascii { "|" } else { "│" };
    let style = role::style(if active { Role::Accent } else { Role::Dim }, color);
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
        let mut state = UiState::new(Presentation::Workspace, false, (width, height));
        // One glyph column for the whole frame: the caller sets both from its theme.
        state.ascii = paint.ascii;
        state.color = paint.color;
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
    fn a_short_pinned_workspace_keeps_the_question_visible() {
        let (_, rows) = draw_at(&screen(welcome()), 60, 16, paint(false));
        assert!(
            rows.iter()
                .any(|row| row.contains("Which file holds the notes")),
            "{rows:#?}"
        );
        assert!(rows.iter().any(|row| row.contains("reply")), "{rows:#?}");
    }

    #[test]
    fn the_conversation_is_between_the_project_and_the_larger_preview() {
        let view = screen(Object::Shown {
            icon: Icon::Workflow,
            name: "release.nika".to_owned(),
            lines: vec!["preview content".to_owned()],
        });
        let (_, rows, buffer) = draw_focused(
            &view,
            120,
            40,
            Paint {
                color: true,
                ..paint(false)
            },
            &Focus::composing(),
        );
        let middle: String = rows[2].chars().skip(21).take(47).collect();
        let right: String = rows[2].chars().skip(68).collect();
        assert!(middle.contains("release checklist"), "{middle}");
        assert!(right.contains("release.nika"), "{right}");
        assert_eq!(
            buffer[(22, 2)].fg,
            role::style(Role::Accent, true)
                .fg
                .expect("accent foreground")
        );
        let geometry = Geometry::of(Rect::new(0, 0, 120, 40), true).expect("fits");
        let conversation = geometry.conversation;
        let gutter = geometry.object.x - 2;
        assert!(
            (conversation.y..conversation.bottom()).all(|y| buffer[(gutter, y)].symbol() == " ")
        );
        assert!(
            (conversation.y..conversation.bottom())
                .any(|y| buffer[(gutter - 1, y)].symbol() == "╯")
        );
        let (_, _, plain) = draw_focused(&view, 120, 40, paint(false), &Focus::composing());
        assert_eq!(plain[(22, 2)].fg, ratatui::style::Color::Reset);
    }

    #[test]
    fn a_busy_conversation_marks_its_title_and_returns_to_idle() {
        let view = screen(welcome());
        let mut state = UiState::new(Presentation::Workspace, true, (120, 40));
        let composer = Composer::new();
        let geometry = Geometry::of(Rect::new(0, 0, 120, 40), true).expect("fits");
        let title = panel_areas(
            &geometry,
            &state,
            &composer,
            view.thread.intelligence.as_deref(),
        )[0];
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
        let paint = Paint {
            color: true,
            ..paint(false)
        };
        for working in [true, false] {
            state.busy = working.then(|| "DeepSeek is working".to_owned());
            state.spinner = working.then_some(1);
            terminal
                .draw(|frame| {
                    draw(frame, &view, paint, &Focus::composing(), &state, &composer);
                })
                .expect("draw");
            let buffer = terminal.backend().buffer();
            let text: String = (title.x..title.right())
                .map(|x| buffer[(x, title.y)].symbol())
                .collect();
            assert!(text.contains("release checklist"), "{text}");
            if let Some(marker) = activity_marker(&state) {
                assert!(text.starts_with(marker.content.as_ref()), "{text}");
            } else {
                assert!(text.starts_with("◌ "), "{text}");
            }
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
            let words = find(&rows, "Describe the work you want to").expect("words");
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
    fn the_selected_model_stays_visible_in_ascii_chrome() {
        let mut view = screen(welcome());
        view.thread.intelligence = Some("deepseek/chosen - deepseek API, metered".into());
        let (_, rows) = draw_at(&view, 120, 40, paint(true));
        let model =
            find(&rows, "Prepare with: deepseek/chosen").expect("selected model is visible");
        assert!(rows[model].is_ascii(), "{}", rows[model]);
        assert!(rows[model + 1].is_ascii(), "{}", rows[model + 1]);
        view.thread.intelligence = Some("private/été·beta - app account".into());
        let (_, rows) = draw_at(&view, 120, 40, paint(true));
        assert!(
            rows.iter().any(|row| row.contains("private/été·beta")),
            "model bytes stay intact"
        );
    }

    #[test]
    fn a_compact_open_preview_keeps_the_exact_selection_activity_and_draft() {
        for (width, height) in [(60, 18), (80, 24)] {
            for ascii in [true, false] {
                for model in [
                    "claude-code/claude-fable-5-1[1m]",
                    "deepseek/private-été·beta",
                ] {
                    let mut view = screen(Object::Shown {
                        icon: Icon::Workflow,
                        name: "release.nika".into(),
                        lines: vec!["saved workflow".into()],
                    });
                    view.thread.intelligence = Some(format!("{model} - selected for preparation"));
                    let mut state = UiState::new(Presentation::Workspace, false, (width, height));
                    state.ascii = ascii;
                    state.busy = Some("checking files locally".into());
                    let mut composer = Composer::new();
                    composer.paste("draft stays here");
                    let geometry =
                        Geometry::of(Rect::new(0, 0, width, height), true).expect("fits");
                    let selected = panel_areas(
                        &geometry,
                        &state,
                        &composer,
                        view.thread.intelligence.as_deref(),
                    );
                    let before = panel_areas(&geometry, &state, &composer, None);
                    assert_eq!(
                        selected[3], before[3],
                        "the live area does not move or shrink"
                    );
                    assert!(
                        selected[1].height >= 1,
                        "the conversation keeps a visible row"
                    );
                    let mut terminal =
                        Terminal::new(TestBackend::new(width, height)).expect("terminal");
                    terminal
                        .draw(|frame| {
                            assert!(draw(
                                frame,
                                &view,
                                paint(ascii),
                                &Focus::composing(),
                                &state,
                                &composer
                            ));
                        })
                        .expect("draw");
                    let buffer = terminal.backend().buffer();
                    let rows: Vec<String> = (0..height)
                        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
                        .collect();
                    for visible in [
                        "studio",
                        "release.nika",
                        "Prepare with:",
                        model,
                        "checking files locally",
                        "draft stays here",
                    ] {
                        assert!(
                            rows.iter().any(|row| row.contains(visible)),
                            "{width}x{height} {visible}: {rows:#?}"
                        );
                    }
                    assert!(rows.last().expect("rows").contains("#043"));
                    assert!(!rows.iter().any(|row| row.contains("AI  ")));
                }
            }
        }
    }

    #[test]
    fn a_compact_preview_keeps_the_composer_under_a_wrapped_verifier_line() {
        // A selection the session projects: author, connection and a selected
        // decision seat. It wraps in the title; the live area and the draft keep their rows.
        let line = "deepseek/deepseek-chat - deepseek API, metered; verifier: typesafe/jev-1.13.0 (selected)";
        for (width, height) in [(60, 18), (80, 24)] {
            let mut view = screen(welcome());
            view.thread.intelligence = Some(line.into());
            let mut state = UiState::new(Presentation::Workspace, false, (width, height));
            state.ascii = true;
            let mut composer = Composer::new();
            composer.paste("draft stays here");
            let geometry = Geometry::of(Rect::new(0, 0, width, height), true).expect("fits");
            let selected = panel_areas(&geometry, &state, &composer, Some(line));
            let before = panel_areas(&geometry, &state, &composer, None);
            assert_eq!(
                selected[3], before[3],
                "{width}x{height}: the live area keeps its rows"
            );
            assert!(
                selected[1].height >= 1,
                "{width}x{height}: a transcript row remains"
            );
            let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
            terminal
                .draw(|frame| {
                    let focus = Focus::composing();
                    assert!(draw(frame, &view, paint(true), &focus, &state, &composer));
                })
                .expect("draw");
            let buffer = terminal.backend().buffer();
            let rows: Vec<String> = (0..height)
                .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
                .collect();
            for visible in [
                "Prepare with:",
                "verifier:",
                "typesafe/jev-1.13.0",
                "draft stays here",
            ] {
                assert!(
                    rows.iter().any(|row| row.contains(visible)),
                    "{width}x{height} {visible}: {rows:#?}"
                );
            }
        }
    }

    #[test]
    fn an_open_preview_keeps_the_status_navigation_selection_and_the_draft() {
        // The Session's leads, as a narrow status row must keep them.
        const FAILED: &str = "Last Run · Done · the run failed";
        const EARLIER: &str = "last run ✓ exit 0 in an earlier session";
        let cases = [
            (FAILED, format!("{FAILED} · `release.nika`"), 0),
            (
                EARLIER,
                format!("{EARLIER} · Saved · no current Run result · `release.nika`"),
                0,
            ),
            (FAILED, format!("{FAILED} · `release.nika`"), 3),
        ];
        for (width, height) in [(60, 18), (80, 24), (100, 32), (120, 40), (180, 48)] {
            for ascii in [false, true] {
                for (lead, status, scroll) in &cases {
                    let mut view = screen(Object::Shown {
                        icon: Icon::Workflow,
                        name: "release.nika".into(),
                        lines: vec!["saved workflow".into()],
                    });
                    // Idle: no paused run is pinned beside this frame.
                    view.pinned = None;
                    view.thread.intelligence = Some(
                        "scaleway/chosen - scaleway API, metered; verifier: same model".into(),
                    );
                    let mut state = UiState::new(Presentation::Workspace, false, (width, height));
                    state.ascii = ascii;
                    state.status.clone_from(status);
                    state.transcript.push(crate::model::Committed::new(
                        crate::model::Kind::Reply,
                        (0..80)
                            .map(|n| format!("reply line {n:03}"))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    ));
                    state.focus_scroll = *scroll;
                    let mut composer = Composer::new();
                    composer.paste("keep my draft");
                    let mut terminal =
                        Terminal::new(TestBackend::new(width, height)).expect("terminal");
                    terminal
                        .draw(|frame| {
                            assert!(draw(
                                frame,
                                &view,
                                paint(ascii),
                                &Focus::composing(),
                                &state,
                                &composer
                            ));
                        })
                        .expect("draw");
                    let buffer = terminal.backend().buffer();
                    let rows: Vec<String> = (0..height)
                        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
                        .collect();
                    let shown = rows.join("\n");
                    let way = if *scroll > 0 {
                        "click chat; End: latest"
                    } else {
                        "wheel"
                    };
                    for visible in [
                        "Prepare with:",
                        "scaleway/chosen",
                        "verifier:",
                        "same model",
                        *lead,
                        "/intelligence",
                        way,
                        "keep my draft",
                    ] {
                        assert!(
                            shown.contains(visible),
                            "{width}x{height} {visible}: {shown}"
                        );
                    }
                    assert!(!shown.contains("Idle"), "{width}x{height}: {shown}");
                }
            }
        }
    }

    #[test]
    fn intelligence_choices_remain_visible_below_a_long_scrolled_menu() {
        use crate::model::{Beat, Committed, Kind, Waiting};
        for (width, height) in [(60, 18), (80, 24), (120, 40)] {
            for ascii in [true, false] {
                let view = screen(welcome());
                let mut state = UiState::new(Presentation::Workspace, false, (width, height));
                state.ascii = ascii;
                state.waiting = Waiting::Choosing;
                let menu = nika_session::intelligence::IntelligenceCensus::empty().first_screen();
                state.apply(Beat::Say(Committed::new(Kind::Reply, menu)));
                let composer = Composer::new();
                let mut terminal =
                    Terminal::new(TestBackend::new(width, height)).expect("terminal");
                terminal
                    .draw(|frame| {
                        assert!(draw(
                            frame,
                            &view,
                            paint(ascii),
                            &Focus::composing(),
                            &state,
                            &composer
                        ));
                    })
                    .expect("draw");
                let buffer = terminal.backend().buffer();
                let rows: Vec<String> = (0..height)
                    .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
                    .collect();
                let choices = rows
                    .iter()
                    .find(|row| {
                        ["1 account", "2 API", "3 local", "4 no AI"]
                            .iter()
                            .all(|choice| row.contains(choice))
                    })
                    .expect("the fixed hint names all four choices together");
                assert!(
                    !choices.contains("Prepare with:"),
                    "the cue does not claim a model answered"
                );
                assert!(rows.iter().any(|row| row.contains("cancel")), "{rows:#?}");
                assert_eq!(state.focus_scroll, 0, "no manual scroll needed");
            }
        }
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
        focus.handle(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), room);
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
        // Backwards from the aside wraps to the preview; its title row stays.
        focus.handle(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT), room);
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
