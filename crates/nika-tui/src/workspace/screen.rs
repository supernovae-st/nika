// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One frame of the workspace screen: the header, the project aside, the
//! object in view, the conversation with its composer and the pinned run, each
//! in the region [`Geometry`] gives it. The facts come from the view the
//! caller passes; the transcript, the status, the composer and the hint are
//! painted by the same functions as the focus presentation, so typing, pasting
//! and history behave the same in every presentation.
//!
//! The chrome is painted once. The header is one quiet row: the brand, the
//! project and what it is, then the intelligence selected for preparation
//! (its one home), the region names first at its right end while the project
//! list is folded, the one holding the keys in brackets. The conversation's
//! title is its own name over a thin rule; its composer is one box under a
//! slim caption where the panel has the rows for it. The object's title row
//! ends with its own action (`[+] Expand` only where expanding enlarges the
//! object, `[-] Restore` on every expansion, and its key `F4`), the welcome
//! giving it its first row; a transcript scrolled back carries one marker
//! back to its latest row; a separator the pointer holds is shown reversed.
//! Each is a plain word or a weight, never a hue alone, and none of them
//! moves; the separators between regions keep the border's quiet weight, and
//! the object, as the header and the aside, stands on the raised surface.

use nika_display::theme::Role;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use super::aside::{self, Aside};
use super::cards::{self, review::Review};
use super::conversation::{self, Thread};
use super::focus::{Extent, Focus, Region};
use super::geometry::{self, Arrangement, Geometry, Separator};
use super::header::Place;
use super::object::{self, Object, Paint};
use super::pinned::{self, Pinned};
use crate::composer::Composer;
use crate::model::UiState;
use crate::render::{
    activity_marker, boxed_live_rows, live_rows, render_boxed_live, render_live, rest_rows,
};
use crate::visual::role;

mod masthead;

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
    /// The candidate a consent can name, as the conversation reviews it: the
    /// same review its scroll bounds read.
    review: Option<Review>,
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
            review: None,
        }
    }

    /// This screen with `run` pinned.
    #[must_use]
    pub fn pinning(mut self, run: Pinned) -> Self {
        self.pinned = Some(run);
        self
    }

    /// This screen with the candidate a consent can name (`None`: none, and
    /// every block keeps its words as said).
    #[must_use]
    pub(crate) fn reviewing(mut self, review: Option<Review>) -> Self {
        self.review = review;
        self
    }
}

/// What the regions hold on a frame of `area` with the object restored, for
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
        // The title and, when needed, continuation cue stay; content scrolls.
        object_rows: object::content_rows(object::length(&screen.object), geometry.object.height),
    }
}

/// How a frame is arranged beyond its facts: the object restored or expanded
/// and the separators' shares, the separator the pointer holds, and whether
/// a current decision folds the stacked object ([`folded`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Chrome {
    /// The layout in view and the separators' shares.
    pub(crate) arrangement: Arrangement,
    /// The separator the pointer is moving, shown reversed.
    pub(crate) dragging: Option<Separator>,
    /// The stacked object folds to its strip ([`crate::workspace::desk::Desk::folds`]).
    pub(crate) folded: bool,
}

/// The rows a folded object keeps: its title, one row and the continuation cue.
const FOLDED_ROWS: u16 = 3;

/// `geometry` with its stacked object folded to [`FOLDED_ROWS`], the
/// conversation under it taking the rows given up; the header, the aside, the
/// pinned row and the arrangement stay. A decision the restored split cannot
/// show folds a frame; its every reader takes this one geometry.
#[must_use]
pub(crate) fn folded(mut geometry: Geometry) -> Geometry {
    if geometry.stacked {
        let given = geometry.object.height.saturating_sub(FOLDED_ROWS);
        geometry.object.height -= given;
        geometry.conversation.y -= given;
        geometry.conversation.height += given;
    }
    geometry
}

/// Draw the workspace on the whole frame with the object restored, the aside
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
        folded: false,
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
    let Some(mut geometry) = Geometry::arranged(area, pinned, &chrome.arrangement) else {
        return false;
    };
    if chrome.folded {
        geometry = folded(geometry);
    }
    let (ascii, color) = (paint.ascii, paint.color);
    frame
        .buffer_mut()
        .set_style(area, role::surface(color, false));
    frame
        .buffer_mut()
        .set_style(geometry.header, role::surface(color, true));
    header_row(
        (&screen.place, screen.thread.intelligence.as_deref()),
        geometry.header,
        frame.buffer_mut(),
        geometry.aside.is_none().then_some(focus.region),
        (ascii, color),
    );
    let next = toggled(area, pinned, &chrome.arrangement, chrome.folded);
    let action = object_action(&screen.object, &geometry, focus.region, next, ascii);
    aside_and_object(frame, screen, &geometry, paint, focus, action.is_some());
    if let Some((cells, words)) = action {
        // A secondary view action: the object and selected face lead.
        let line = Line::styled(words, role::style(Role::Dim, color));
        frame.render_widget(Paragraph::new(line), cells);
    }
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
/// lifted while it holds the keys (underlined without colour). The welcome, which has no title,
/// leaves its first row to the object's action while one is offered
/// (`acting`): its own fitting lays the mark and the words out below.
fn aside_and_object(
    frame: &mut Frame<'_>,
    screen: &Screen,
    geometry: &Geometry,
    paint: Paint,
    focus: &Focus,
    acting: bool,
) {
    let (ascii, color) = (paint.ascii, paint.color);
    let selected = (focus.region == Region::Aside).then_some(focus.selected);
    // The object stands on the raised surface, as the header and the aside.
    frame
        .buffer_mut()
        .set_style(geometry.object, role::surface(color, true));
    if let Some(area) = geometry.aside {
        frame
            .buffer_mut()
            .set_style(area, role::surface(color, true));
        // Every cell of the narrow list is the names': the rows lay out
        // their own marker column and cuts.
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
        rule_column(edge, ascii, color, frame.buffer_mut());
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
    let body = object_body(geometry);
    if geometry.aside.is_some() || selected.is_none() {
        let mut area = body;
        if acting && matches!(screen.object, Object::Welcome { .. }) {
            area.y += 1;
            area.height = area.height.saturating_sub(1);
        }
        object::render_from(
            &screen.object,
            area,
            frame.buffer_mut(),
            paint,
            focus.scroll,
        );
    }
    if focus.region == Region::Object {
        frame.buffer_mut().set_style(
            Rect::new(body.x, body.y, body.width, 1),
            focused_title(color),
        );
    }
}

/// The object's cells inside its region: beside the conversation, one cell
/// of air after the separator; above the conversation (the narrow stack),
/// the whole region. Painting, the object's action, a press on its faces
/// and the width its face is rendered for read this one rectangle.
#[must_use]
pub(crate) fn object_body(geometry: &Geometry) -> Rect {
    let area = geometry.object;
    if geometry.stacked || area.width < 2 {
        area
    } else {
        Rect::new(area.x + 1, area.y, area.width - 1, area.height)
    }
}

/// A focused region's title keeps its text hierarchy on a quiet selection
/// surface. Without colour an underline still names where the keys go.
fn focused_title(color: bool) -> Style {
    if color {
        role::selection(true)
    } else {
        Style::default().add_modifier(Modifier::UNDERLINED)
    }
}

/// The regions a folded header names, in reading order.
const REGIONS: [(Region, &str); 3] = [
    (Region::Aside, "Project"),
    (Region::Conversation, "Conversation"),
    (Region::Object, "Object"),
];

/// While the project list is folded, the header's names of the three
/// regions: the one holding the keys (`focused`) in brackets and strong, each
/// part with the region a press on it gives the keys to. Its width does not
/// change with the region focused.
fn region_parts(focused: Region, color: bool) -> Vec<(Span<'static>, Option<Region>)> {
    let mut parts = Vec::new();
    for (index, (region, name)) in REGIONS.into_iter().enumerate() {
        if index > 0 {
            parts.push((Span::raw(" "), None));
        }
        let part = if region == focused {
            Span::styled(format!("[{name}]"), role::style(Role::Strong, color))
        } else {
            Span::styled(name, role::style(Role::Dim, color))
        };
        parts.push((part, Some(region)));
    }
    parts
}

/// Where the region names stand in `header`: the right end of its first
/// row, one blank cell after the place, while that row keeps room for the
/// place's glyph and the start of its name (everything else on the row gives
/// way first, [`masthead`]).
#[must_use]
pub(crate) fn regions_area(place: &Place, header: Rect, ascii: bool) -> Option<Rect> {
    let parts = region_parts(Region::Conversation, false);
    let width: usize = parts.iter().map(|(span, _)| span.content.width()).sum();
    let width = u16::try_from(width).ok()?;
    let room = header.width.checked_sub(width + 1)?;
    let fits = usize::from(room) >= masthead::least_place(place, ascii);
    (fits && header.height > 0).then(|| Rect::new(header.right() - width, header.y, width, 1))
}

/// The region a press at `column` of the header's first row names.
#[must_use]
pub(crate) fn region_at(
    place: &Place,
    header: Rect,
    focused: Region,
    ascii: bool,
    column: u16,
) -> Option<Region> {
    let area = regions_area(place, header, ascii)?;
    let offset = usize::from(column.checked_sub(area.x)?);
    let mut start = 0;
    for (span, region) in region_parts(focused, false) {
        let end = start + span.content.width();
        if (start..end).contains(&offset) {
            return region;
        }
        start = end;
    }
    None
}

/// What expanding or restoring the object shows on a frame of `area`
/// arranged as `arrangement`, a run pinned when `pinned`, the object folded
/// when `fold` ([`folded`]): the one answer the object's action, a press on
/// it, `F4` and the palette read. A kept expansion always restores; the
/// restored object expands only where the expanded one holds strictly more
/// cells, its shares as chosen. `None` where nothing would change (below the
/// minimum, or no room to grow).
#[must_use]
pub(crate) fn toggled(
    area: Rect,
    pinned: bool,
    arrangement: &Arrangement,
    fold: bool,
) -> Option<geometry::Layout> {
    let mut now = Geometry::arranged(area, pinned, arrangement)?;
    if fold {
        now = folded(now);
    }
    let next = arrangement.layout.toggled();
    if next == geometry::Layout::Session {
        return Some(next);
    }
    let grown = Geometry::arranged(area, pinned, &arrangement.with_layout(next))?;
    (grown.object.area() > now.object.area()).then_some(next)
}

/// The cells the widest continuation cue an object paints on its last row
/// takes (`↑ Above · ↓ Below · scroll`, in either glyph column).
const CUE_CELLS: usize = 26;

/// The object's own action for `next` ([`toggled`]) and the cells it takes
/// on a frame of `geometry`: the right end of the object's first row, one
/// blank cell after its title (the welcome has none: it gives the action its
/// first row), in the longest form that fits (`[+] Expand · F4`, then
/// `[+] F4`; `[-]` restores); with no room there, the right end of the
/// continuation cue's row, one blank cell after its widest words, or with no
/// cue the right end of a last row the object's lines leave free (strictly
/// fewer body lines than rows under the title), so neither a face, a line
/// nor the cue is ever covered. `None` where `next` is, under the folded
/// project list, or where no form fits: `F4` and the palette still act.
/// Drawing and the pointer read this one answer.
#[must_use]
pub(crate) fn object_action(
    shown: &Object,
    geometry: &Geometry,
    focused: Region,
    next: Option<geometry::Layout>,
    ascii: bool,
) -> Option<(Rect, &'static str)> {
    let area = object_body(geometry);
    let next = next?;
    if geometry.aside.is_none() && focused == Region::Aside {
        return None;
    }
    let title = if let Object::Workflow { title, .. } = shown {
        // A rendered face: its title row alone, never its body.
        title.width()
    } else if matches!(shown, Object::Welcome { .. }) {
        0
    } else {
        let plain = Paint {
            ascii,
            color: false,
            elapsed: std::time::Duration::ZERO,
            reduced_motion: true,
        };
        let lines = object::lines(shown, area.width, area.height, plain);
        lines.first().map_or(0, Line::width)
    };
    let expanded = next == geometry::Layout::Session;
    let forms = match (expanded, ascii) {
        (false, false) => ["[+] Expand · F4", "[+] F4"],
        (false, true) => ["[+] Expand - F4", "[+] F4"],
        (true, false) => ["[-] Restore · F4", "[-] F4"],
        (true, true) => ["[-] Restore - F4", "[-] F4"],
    };
    let body = object::length(shown);
    let cue = area.height >= 3 && body > usize::from(area.height - 1);
    // No cue: the last row is free only while the body lines are strictly
    // fewer than the rows under the title, never on an equal count.
    let free = !cue && body < usize::from(area.height.saturating_sub(1));
    let last = if cue { CUE_CELLS } else { 0 };
    let rows = [(area.y, title), (area.bottom().saturating_sub(1), last)];
    rows.into_iter()
        .take(1 + usize::from(cue || free))
        .find_map(|(y, used)| {
            forms.into_iter().find_map(|words| {
                let cells = u16::try_from(words.width()).ok()?;
                let fits = used + 1 + usize::from(cells) <= usize::from(area.width);
                fits.then(|| (Rect::new(area.right() - cells, y, cells, 1), words))
            })
        })
}

/// The header ([`masthead`]) of the place and the intelligence selected for
/// preparation, with the region names at the right end of its first row
/// while the project list is folded (`folded` holds the region with the
/// keys) and that row has room for them.
fn header_row(
    (place, seat): (&Place, Option<&str>),
    area: Rect,
    buf: &mut Buffer,
    folded: Option<Region>,
    (ascii, color): (bool, bool),
) {
    let names = folded.and_then(|focused| Some((focused, regions_area(place, area, ascii)?)));
    let cells = names.map(|(_, cells)| cells);
    masthead::render((place, seat), area, cells, buf, (ascii, color));
    if let Some((focused, cells)) = names {
        let parts = region_parts(focused, color);
        let line = Line::from(parts.into_iter().map(|(span, _)| span).collect::<Vec<_>>());
        Paragraph::new(line).render(cells, buf);
    }
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

/// The conversation panel: its title (beside the object, a thin rule under
/// it), the transcript, the attachments while there are some, then the live
/// area (status, prompt and composer, hint), its composer boxed under a slim
/// caption where the panel has the rows ([`boxed`]). Beside the object a
/// quiet rule column separates it; under the object its title is the
/// separator.
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
        rule_column(edge, ascii, color, frame.buffer_mut());
    }
    let [title, transcript, context, bottom] =
        panel_areas(geometry, state, composer, &screen.thread);
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
    let mut title_lines = vec![heading];
    if title.height > 1 {
        let rule = if ascii { "-" } else { "─" };
        let rule = rule.repeat(usize::from(title.width));
        title_lines.push(Line::styled(rule, role::border(color)));
    }
    frame.render_widget(Paragraph::new(title_lines), title);
    if focused == Region::Conversation {
        frame.buffer_mut().set_style(
            Rect::new(title.x, title.y, title.width, 1),
            focused_title(color),
        );
    }
    // The live question card's words, read where its block stood as one row:
    // the same projection the scroll bounds measure ([`carried`]).
    let carried = crate::render::question::carried(state, composer, bottom, boxed(geometry));
    cards::render(frame, state, transcript, (screen.review.as_ref(), carried));
    if state.focus_scroll > 0
        && let Some(marker) = latest_area(transcript, ascii)
    {
        // Scrolled back, new activity keeps the reading place; this marker
        // (or `End`) is the way back, never an automatic jump.
        frame.render_widget(Paragraph::new(latest_line(ascii, color)), marker);
    }
    if context.height > 0 {
        let with = conversation::context(&screen.thread, context.width, ascii, color);
        frame.render_widget(Paragraph::new(with), context);
    }
    if boxed(geometry) {
        render_boxed_live(frame, state, composer, bottom);
    } else {
        render_live(frame, state, composer, bottom);
    }
}

/// The panel rows from which its title takes a thin rule under it.
const RULED_ROWS: u16 = 12;

/// The panel rows from which the composer stands boxed under its caption:
/// the transcript keeps ten rows above it.
const BOXED_ROWS: u16 = 18;

/// The conversation's cells inside its region: beside the object, one cell
/// of air, the content, a free gutter and the separator; under the object,
/// the whole region (its title is the separator).
fn panel_content(geometry: &Geometry) -> Rect {
    let region = geometry.conversation;
    if geometry.stacked {
        region
    } else {
        Rect::new(
            region.x + 1,
            region.y,
            region.width.saturating_sub(3),
            region.height,
        )
    }
}

/// Whether the composer stands boxed under its caption: beside the object,
/// where the panel has [`BOXED_ROWS`]; the compact conversation under the
/// object keeps the plain composer. Painting and the pointer read it.
pub(crate) fn boxed(geometry: &Geometry) -> bool {
    !geometry.stacked && panel_content(geometry).height >= BOXED_ROWS
}

/// The exact conversation rectangles, shared by painting, the scroll bounds
/// and the pointer: the title, the transcript, the attachments of `thread`
/// (no row while nothing is attached) and the live area.
pub(crate) fn panel_areas(
    geometry: &Geometry,
    state: &UiState,
    composer: &Composer,
    thread: &Thread,
) -> [Rect; 4] {
    split_panel(geometry, thread, |width, room| {
        if boxed(geometry) {
            boxed_live_rows(state, composer, width, room)
        } else {
            live_rows(state, composer, width, room)
        }
    })
}

/// The transcript of `geometry`'s conversation at rest, its live area as
/// [`rest_rows`] asks, whatever is typed, listed or busy: where a decision's
/// demand is measured.
pub(crate) fn rest_transcript(geometry: &Geometry, state: &UiState, thread: &Thread) -> Rect {
    split_panel(geometry, thread, |width, room| {
        rest_rows(state, width).min(room)
    })[1]
}

/// The panel cut into its title, transcript, attachments of `thread` and the
/// live area `live` sizes for its width and the rows left.
fn split_panel(
    geometry: &Geometry,
    thread: &Thread,
    live: impl FnOnce(u16, u16) -> u16,
) -> [Rect; 4] {
    let area = panel_content(geometry);
    let heading = if !geometry.stacked && area.height >= RULED_ROWS {
        2
    } else {
        1
    };
    let context = u16::from(!thread.attached.is_empty());
    let room = area.height.saturating_sub(heading + context);
    let live = live(area.width, room);
    Layout::vertical([
        Constraint::Length(heading),
        Constraint::Min(0),
        Constraint::Length(context),
        Constraint::Length(live),
    ])
    .areas(area)
}

/// A quiet vertical rule filling the one-column `area`: a separator keeps
/// the border's weight, whichever region holds the keys.
fn rule_column(area: Rect, ascii: bool, color: bool, buf: &mut Buffer) {
    let glyph = if ascii { "|" } else { "│" };
    let style = role::border(color);
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
            // The thread by its own name: the header is the project's one home.
            let title = find(&rows, "◌ release checklist").expect("thread title");
            assert!(!rows[title].contains("· studio"), "{}", rows[title]);
            // Nothing attached: no context row says so.
            assert!(
                find(&rows, "nothing attached").is_none(),
                "{width}x{height}"
            );
            // The placeholder names the recipient; a narrow composer cuts its end,
            // and the title row above still names the thread in full.
            let composer = find(&rows, "Message to studio / rel");
            assert!(
                composer.is_some_and(|composer| title < composer),
                "{width}x{height}: the composer names the recipient"
            );
            let aside = Geometry::of(Rect::new(0, 0, width, height), true)
                .expect("fits")
                .aside;
            assert_eq!(aside.is_some(), width >= 120, "{width}x{height}");
            if let Some(aside) = aside {
                let row = &rows[usize::from(aside.y)];
                assert!(row.starts_with("Project"), "{width}x{height}: {row}");
            }
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
        // The composer's box closes against the gutter, never across it.
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
        let title = panel_areas(&geometry, &state, &composer, &view.thread)[0];
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
        assert!(
            narrow[title].starts_with("── ◌ release checklist ─"),
            "{}",
            narrow[title]
        );
        let (_, wide) = draw_at(&screen, 120, 40, paint(false));
        let title = find(&wide, "◌ release checklist").expect("title");
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
        let title = find(&rows, "[C] release checklist").expect("title");
        let object = find(&rows, "[W] release.nika").expect("object title");
        let prompt = find(&rows, "reply > ").expect("the prompt marker twin");
        for y in [0, 1, title, object, prompt, prompt + 1, rows.len() - 1] {
            assert!(rows[y].is_ascii(), "row {y}: {}", rows[y]);
        }
        assert!(rows[2].contains('|'), "the aside edge: {}", rows[2]);
    }

    /// The selected intelligence has one home, the header's first row, in
    /// both glyph columns: the renderer's own words take their twins, the
    /// Session's words keep their bytes.
    #[test]
    fn the_selected_intelligence_has_one_home_in_the_header() {
        let mut view = screen(welcome());
        view.thread.intelligence = Some("deepseek/chosen - deepseek API, metered".into());
        let (_, rows) = draw_at(&view, 120, 40, paint(true));
        let words = "Prepare with: deepseek/chosen - deepseek API, metered";
        // The words keep one blank cell before the edge.
        assert!(rows[0].ends_with(&format!("{words} ")), "{}", rows[0]);
        assert!(rows[0].is_ascii(), "{}", rows[0]);
        assert_eq!(rows.join("\n").matches("Prepare with").count(), 1);
        view.thread.intelligence = Some("private/été·beta - app account".into());
        let (_, rows) = draw_at(&view, 120, 40, paint(true));
        assert!(
            rows[0].contains("private/été·beta"),
            "model bytes stay intact: {}",
            rows[0]
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
                    let selected = panel_areas(&geometry, &state, &composer, &view.thread);
                    let unseated = view.thread.clone().seated(None);
                    let before = panel_areas(&geometry, &state, &composer, &unseated);
                    assert_eq!(
                        selected, before,
                        "the selection never moves the conversation's rows"
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
                        "checking files locally",
                        "draft stays here",
                    ] {
                        assert!(
                            rows.iter().any(|row| row.contains(visible)),
                            "{width}x{height} {visible}: {rows:#?}"
                        );
                    }
                    // A narrow one-row header cuts the selection, or points at
                    // its details; the conversation never repeats it.
                    let start = model.split('/').next().expect("a provider");
                    assert!(
                        rows[0].contains(start) || rows[0].contains("/status"),
                        "{width}x{height}: {}",
                        rows[0]
                    );
                    assert_eq!(rows.join("\n").matches("Prepare with").count(), {
                        usize::from(rows[0].contains("Prepare with"))
                    });
                    assert!(rows.last().expect("rows").contains("#043"));
                    assert!(!rows.iter().any(|row| row.contains("AI  ")));
                }
            }
        }
    }

    #[test]
    fn a_compact_preview_keeps_the_composer_under_a_long_selection() {
        // A selection the session projects: author, connection and a selected
        // decision seat. A one-row header too narrow for it cuts it beside
        // `/status`, which says it whole; the composer and the draft keep their rows.
        let line = "deepseek/deepseek-chat - deepseek API, metered; verifier: typesafe/jev-1.13.0 (selected)";
        for (width, height) in [(60, 18), (80, 24)] {
            let mut view = screen(welcome());
            view.thread.intelligence = Some(line.into());
            let mut state = UiState::new(Presentation::Workspace, false, (width, height));
            state.ascii = true;
            let mut composer = Composer::new();
            composer.paste("draft stays here");
            let geometry = Geometry::of(Rect::new(0, 0, width, height), true).expect("fits");
            let selected = panel_areas(&geometry, &state, &composer, &view.thread);
            let unseated = view.thread.clone().seated(None);
            let before = panel_areas(&geometry, &state, &composer, &unseated);
            assert_eq!(selected, before, "{width}x{height}: the rows stay");
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
            assert!(rows[0].contains("/status"), "{width}x{height}: {}", rows[0]);
            assert!(rows[0].is_ascii(), "{width}x{height}: {}", rows[0]);
            let shown = rows.join("\n");
            assert!(
                shown.contains("draft stays here"),
                "{width}x{height}\n{shown}"
            );
            assert!(!shown.contains("verifier:"), "{width}x{height}\n{shown}");
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
                        "Enter send"
                    };
                    for visible in [*lead, way, "keep my draft"] {
                        assert!(
                            shown.contains(visible),
                            "{width}x{height} {visible}: {shown}"
                        );
                    }
                    // The selection's one home is the header, whole where it fits.
                    let header = rows[..if height >= 30 { 2 } else { 1 }].join("\n");
                    let whole = "Prepare with: scaleway/chosen - scaleway API, metered; verifier: same model";
                    assert!(
                        header.contains(whole) || (height < 30 && header.contains("/status")),
                        "{width}x{height}: {header}"
                    );
                    assert_eq!(
                        shown.matches("verifier:").count(),
                        header.matches("verifier:").count()
                    );
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
        // The selection is the accent with an underline, never a reversed block.
        assert!(
            buffer[(x, y)].modifier.contains(Modifier::UNDERLINED),
            "{}",
            rows[at]
        );
        let opened = rows
            .iter()
            .position(|r| aside_part(r).contains("release checklist"))
            .expect("open row");
        let (ox, oy) = (x, u16::try_from(opened).expect("y"));
        assert!(!buffer[(ox, oy)].modifier.contains(Modifier::UNDERLINED));
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod composition_tests;
