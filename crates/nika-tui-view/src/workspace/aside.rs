// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project aside: what the project the header names holds, in two
//! projections, **Nika** (conversations, workflows, runs, activations) and
//! **Files**. The header is the project's one home, so the aside's first row
//! names its region, never the project again. It lists only entries the
//! Session exposes, in its order, each with its identity and depth, a
//! workflow with the installed checker's verdict in words. Where the list
//! holds more than one kind, a quiet heading names each kind above its first
//! listed entry, a nested entry staying under its parent; a heading only
//! groups and opens nothing. An inventory the Session marks partial says so
//! on its last row instead of pretending to show the whole disk, and what a
//! projection cannot list is said in a note, never left blank. When the
//! selection slides the list, the rows it hides are counted where they are,
//! above or below. The object in view is marked `›` and set in weight, and
//! with colour its whole row stands on the selection fill wherever the keys
//! are; the entry the keys select wears a bar, `▐`, the accent and an
//! underline, on the same fill. The marks tell the two apart, so both read
//! without colour and no row is inverted. Opening an entry changes the
//! object in view, never the conversation, and never attaches its content to
//! the next message.

use nika_display::theme::Role;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::text::{fit_head, marks, wrap};
use crate::visual::icon::Icon;
use crate::visual::role;

/// The two projections of the project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Tab {
    /// Conversations, workflows, runs and activations.
    Nika,
    /// The project's files.
    Files,
}

/// The installed checker's verdict on a workflow, as the Session lends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verdict {
    /// The check found nothing.
    Clean,
    /// The check did not pass, with the findings it counted (none counted
    /// when the file did not read or parse: « not clean »).
    Findings(usize),
}

impl Verdict {
    /// The verdict in words and the role they wear: a hue accompanies the
    /// words, never replaces them.
    #[must_use]
    pub fn words(self) -> String {
        match self {
            Self::Clean => "ok".to_owned(),
            Self::Findings(0) => "not clean".to_owned(),
            Self::Findings(1) => "1 finding".to_owned(),
            Self::Findings(n) => format!("{n} findings"),
        }
    }

    fn role(self) -> Role {
        match self {
            Self::Clean => Role::Good,
            Self::Findings(_) => Role::Warn,
        }
    }
}

/// One entry of the aside, as the Session lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Entry {
    /// What kind of object it is.
    pub icon: Icon,
    /// Its name.
    pub label: String,
    /// Its nesting depth under the projection's root.
    pub depth: u8,
    /// Whether it is the object in view.
    pub open: bool,
    /// The checker's verdict, for a workflow the Session judged.
    pub verdict: Option<Verdict>,
}

impl Entry {
    /// A top-level entry, not open, not judged.
    #[must_use]
    pub fn new(icon: Icon, label: impl Into<String>) -> Self {
        Self {
            icon,
            label: label.into(),
            depth: 0,
            open: false,
            verdict: None,
        }
    }

    /// This entry `depth` levels deep.
    #[must_use]
    pub fn at(mut self, depth: u8) -> Self {
        self.depth = depth;
        self
    }

    /// This entry as the object in view.
    #[must_use]
    pub fn opened(mut self) -> Self {
        self.open = true;
        self
    }

    /// This entry with the checker's verdict beside it.
    #[must_use]
    pub fn judged(mut self, verdict: Verdict) -> Self {
        self.verdict = Some(verdict);
        self
    }
}

/// The aside's content, as the Session projects it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Aside {
    /// The project named in the header.
    pub project: String,
    /// The projection shown.
    pub tab: Tab,
    /// The entries, in the Session's order.
    pub entries: Vec<Entry>,
    /// Whether the listing is complete; `false` says it is partial.
    pub complete: bool,
    /// What the projection cannot list, in words, under its entries.
    pub note: Option<String>,
}

impl Aside {
    /// The `tab` projection of `project` listing `entries`, `complete` or
    /// partial, with no note.
    #[must_use]
    pub fn new(project: impl Into<String>, tab: Tab, entries: Vec<Entry>, complete: bool) -> Self {
        Self {
            project: project.into(),
            tab,
            entries,
            complete,
            note: None,
        }
    }

    /// This aside with `note` under its entries.
    #[must_use]
    pub fn noting(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// The aside's first row: the region it is. The header names the project.
const REGION: &str = "Project";

/// The list rows a grouped window needs at least: a count of what is hidden
/// above, a heading, the selected entry and a count of what is hidden below.
/// With less room the list is not grouped, so the selection keeps its row.
const GROUPED_ROOM: usize = 4;

/// The marker of the entry the keys select: its Unicode face, its ASCII twin.
const CURSOR: (&str, &str) = ("▐", ">");

/// The marker of the object in view: its Unicode face, its ASCII twin.
const OPEN: (&str, &str) = ("›", "*");

/// The heading a top-level entry of this kind opens; `None` for a kind no
/// heading names.
const fn heading(icon: Icon) -> Option<&'static str> {
    match icon {
        Icon::Conversation => Some("CONVERSATIONS"),
        Icon::Workflow => Some("WORKFLOWS"),
        Icon::Run | Icon::Pinned => Some("RUNS"),
        Icon::Activation => Some("ACTIVATIONS"),
        Icon::File => Some("FILES"),
        Icon::Memory => Some("MEMORY"),
        Icon::Connection => Some("CONNECTIONS"),
        Icon::Settings => Some("SETTINGS"),
        Icon::Project | Icon::Search | Icon::Choice => None,
    }
}

/// What one row of the aside shows: the plan the frame paints and the
/// pointer reads, so a click lands on the entry that was painted.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    /// The region's name.
    Region,
    /// The two projections.
    Tabs,
    /// The heading of the kind the entries under it are.
    Heading(&'static str),
    /// The entry at this index.
    Entry(usize),
    /// How many entries the slid list hides above.
    Above(usize),
    /// How many it hides below.
    Below(usize),
    /// One wrapped row of the note.
    Note(String),
    /// The listing is partial.
    Partial,
}

/// The aside lines for a `width` × `height` region, nothing selected.
#[must_use]
pub fn lines(
    aside: &Aside,
    width: u16,
    height: u16,
    ascii: bool,
    color: bool,
) -> Vec<Line<'static>> {
    lines_selecting(aside, width, height, ascii, color, None)
}

/// The aside lines with the entry at `selected` marked (the keyboard is in
/// the aside); the listing slides so the selected entry is always shown.
#[must_use]
pub fn lines_selecting(
    aside: &Aside,
    width: u16,
    height: u16,
    ascii: bool,
    color: bool,
    selected: Option<usize>,
) -> Vec<Line<'static>> {
    lines_anchored(
        aside,
        width,
        height,
        ascii,
        color,
        selected,
        selected.unwrap_or(0),
    )
}

/// The aside's rows, `width` × `height` cells, with `selected` marked when the
/// aside holds the keys, the list kept at its scroll `anchor` even while
/// another region has the keyboard. The renderer's frame and its pointer read
/// the same rows, so a click lands on the entry that was painted.
#[must_use]
pub fn lines_anchored(
    aside: &Aside,
    width: u16,
    height: u16,
    ascii: bool,
    color: bool,
    selected: Option<usize>,
    anchor: usize,
) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let (_, cut) = marks(ascii);
    let dim = role::style(Role::Dim, color);
    let quiet = |words: &str| Line::from(Span::styled(fit_head(words, width, cut), dim));
    plan(aside, width, usize::from(height), ascii, anchor)
        .into_iter()
        .map(|row| match row {
            Row::Region => quiet(REGION),
            Row::Tabs => tabs(aside.tab, width, ascii, color),
            Row::Heading(name) => quiet(name),
            Row::Entry(index) => aside
                .entries
                .get(index)
                .map_or_else(Line::default, |entry| {
                    entry_row(entry, width, ascii, color, selected == Some(index))
                }),
            Row::Above(hidden) => quiet(&format!("  +{hidden} above")),
            Row::Below(hidden) => quiet(&format!("  +{hidden} below")),
            Row::Note(words) => quiet(&words),
            Row::Partial => quiet("partial listing"),
        })
        .collect()
}

/// The index of the entry [`lines_anchored`] paints at `row` (counted from the
/// aside's top) for the same `width`, `height`, glyph column and `anchor`.
/// The region, projections, headings, omitted-count rows and footers never
/// alias an entry. The right edge is excluded by the caller.
#[must_use]
pub fn entry_at(
    aside: &Aside,
    width: u16,
    height: u16,
    ascii: bool,
    anchor: usize,
    row: u16,
) -> Option<usize> {
    let rows = plan(
        aside,
        usize::from(width),
        usize::from(height),
        ascii,
        anchor,
    );
    match rows.get(usize::from(row)) {
        Some(Row::Entry(index)) => Some(*index),
        _ => None,
    }
}

/// The rows of a `width` × `height` aside whose list is kept at `anchor`,
/// cut to its height: the region, the projections, the listed window with
/// its headings and hidden counts, the note and the partial listing.
fn plan(aside: &Aside, width: usize, height: usize, ascii: bool, anchor: usize) -> Vec<Row> {
    let (_, cut) = marks(ascii);
    let note = aside
        .note
        .as_deref()
        .map(|note| wrap(note, width, cut))
        .unwrap_or_default();
    let footer = note.len() + usize::from(!aside.complete);
    let room = height.saturating_sub(2 + footer);
    let under = headings(&aside.entries, room);
    let (start, end) = window(&under, room, anchor);
    let mut rows = vec![Row::Region, Row::Tabs];
    if start > 0 {
        rows.push(Row::Above(start));
    }
    for index in start..end {
        rows.extend(opens(&under, start, index).map(Row::Heading));
        rows.push(Row::Entry(index));
    }
    if end < under.len() {
        rows.push(Row::Below(under.len() - end));
    }
    rows.extend(note.into_iter().map(Row::Note));
    if !aside.complete {
        rows.push(Row::Partial);
    }
    rows.truncate(height);
    rows
}

/// The heading each entry stands under: a top-level entry opens its kind's,
/// a nested one stays under its parent's. None at all where the list holds
/// a single kind, or where `room` cannot hold a grouped window.
fn headings(entries: &[Entry], room: usize) -> Vec<Option<&'static str>> {
    let mut current = None;
    let under: Vec<Option<&'static str>> = entries
        .iter()
        .map(|entry| {
            if entry.depth == 0 || current.is_none() {
                current = heading(entry.icon).or(current);
            }
            current
        })
        .collect();
    let first = under.iter().flatten().next();
    let mixed = under.iter().flatten().any(|name| Some(name) != first);
    if mixed && room >= GROUPED_ROOM {
        under
    } else {
        vec![None; entries.len()]
    }
}

/// The heading row before the entry at `index` of a window opening at
/// `start`: where the entry opens its heading, or opens the window.
fn opens(under: &[Option<&'static str>], start: usize, index: usize) -> Option<&'static str> {
    let name = under.get(index).copied().flatten()?;
    let continues = index > start && under.get(index - 1).copied().flatten() == Some(name);
    (!continues).then_some(name)
}

/// The listed window `[start, end)` of the entries in `room` rows, the entry
/// at `anchor` always inside it: a row says how many entries are hidden
/// above when the list slid, another how many below, and the heading of a
/// listed entry takes its own row.
fn window(under: &[Option<&'static str>], room: usize, anchor: usize) -> (usize, usize) {
    let count = under.len();
    let rows = |start: usize, end: usize| {
        (start..end)
            .filter(|&index| opens(under, start, index).is_some())
            .count()
            + (end - start)
            + usize::from(start > 0)
            + usize::from(end < count)
    };
    if rows(0, count) <= room {
        return (0, count);
    }
    let at = anchor.min(count - 1);
    // From the top, one row counts what is hidden below.
    let mut end = 1;
    while end < count && rows(0, end + 1) <= room {
        end += 1;
    }
    if at < end {
        return (0, end);
    }
    // At the end, one row counts what is hidden above.
    let mut start = count - 1;
    while start > 0 && rows(start - 1, count) <= room {
        start -= 1;
    }
    if at >= start {
        return (start, count);
    }
    // In the middle both rows count, and the selection stands last.
    let mut start = at;
    while start > 0 && rows(start - 1, at + 1) <= room {
        start -= 1;
    }
    (start, at + 1)
}

/// The projections' row: the chosen one in the accent, set in weight and
/// underlined, each name its own span (the spans the pointer reads), cut at
/// the edge like any row.
fn tabs(tab: Tab, width: usize, ascii: bool, color: bool) -> Line<'static> {
    let (sep, cut) = marks(ascii);
    let dim = role::style(Role::Dim, color);
    let chosen =
        role::style(Role::Accent, color).add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let (nika, files) = match tab {
        Tab::Nika => (chosen, dim),
        Tab::Files => (dim, chosen),
    };
    let spans = vec![
        Span::styled("Nika", nika),
        Span::styled(sep, dim),
        Span::styled("Files", files),
    ];
    clip(spans, width, cut)
}

/// `spans` within `width` cells: those that fit whole, then the first that
/// does not, cut with `cut`; the rest are dropped.
fn clip(spans: Vec<Span<'static>>, width: usize, cut: &str) -> Line<'static> {
    let mut used = 0;
    let mut kept = Vec::with_capacity(spans.len());
    for span in spans {
        let cells = span.content.width();
        if used + cells > width {
            let rest = fit_head(&span.content, width - used, cut);
            if !rest.is_empty() {
                kept.push(Span::styled(rest, span.style));
            }
            break;
        }
        used += cells;
        kept.push(span);
    }
    Line::from(kept)
}

/// A marker in the glyph column in use: its Unicode face only where that
/// cell is one wide in both width tables, its ASCII twin otherwise.
fn face((unicode, twin): (&'static str, &'static str), ascii: bool) -> &'static str {
    if !ascii && unicode.width() == 1 && unicode.width_cjk() == 1 {
        unicode
    } else {
        twin
    }
}

/// One entry's row, `width` cells: its marker (`▐` where the keys select
/// it, `›` where it is the object in view), its indent, glyph and label, and
/// the verdict at the row's end. A narrow row cuts the label before the
/// verdict, and drops the verdict only when no label would remain. With
/// colour, the object in view stands on the selection fill wherever the keys
/// are, and so does the row the keys select: the marker, the underline and
/// the weight tell the two apart.
fn entry_row(entry: &Entry, width: usize, ascii: bool, color: bool, cursor: bool) -> Line<'static> {
    let (_, cut) = marks(ascii);
    let accent = role::style(Role::Accent, color);
    let marker = match (cursor, entry.open) {
        (true, _) => face(CURSOR, ascii),
        (false, true) => face(OPEN, ascii),
        (false, false) => " ",
    };
    // A weight for the object in view, the accent and an underline for the
    // selection: each reads without colour, together as well.
    let mut style = if entry.open {
        role::style(Role::Strong, color)
    } else {
        Style::default()
    };
    if cursor {
        style = style.patch(accent).add_modifier(Modifier::UNDERLINED);
    }
    let indent = "  ".repeat(usize::from(entry.depth));
    let head = format!("{indent}{} ", entry.icon.glyph(ascii));
    let room = width.saturating_sub(marker.width() + head.width());
    let mut spans = vec![
        Span::styled(marker, accent),
        Span::styled(head, role::style(Role::Dim, color)),
    ];
    // The verdict, a space, and at least one character of the label and the cut.
    let verdict = entry
        .verdict
        .map(|verdict| (verdict.words(), verdict.role()))
        .filter(|(words, _)| room > words.width() + 1 + cut.width());
    if let Some((words, verdict_role)) = verdict {
        let label = fit_head(&entry.label, room - words.width() - 1, cut);
        let pad = room - label.width() - words.width();
        spans.extend([
            Span::styled(label, style),
            Span::raw(" ".repeat(pad)),
            Span::styled(words, role::style(verdict_role, color)),
        ]);
    } else {
        spans.push(Span::styled(fit_head(&entry.label, room, cut), style));
    }
    let row = clip(spans, width, "");
    if color && (cursor || entry.open) {
        lifted(row, width)
    } else {
        row
    }
}

/// A lifted `row` (the object in view, the entry the keys select) on the
/// selection fill across its whole `width`, the blank cells after its words
/// included: every hue and weight it wore stays, and no other row is touched.
/// The caller lifts only with colour: without it no row is filled or padded.
fn lifted(mut row: Line<'static>, width: usize) -> Line<'static> {
    let blank = width.saturating_sub(row.width());
    if blank > 0 {
        row.spans.push(Span::raw(" ".repeat(blank)));
    }
    let fill = role::selection(true);
    for span in &mut row.spans {
        span.style = span.style.patch(fill);
    }
    row
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::widgets::{Paragraph, Widget};

    use super::*;

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    /// The Nika projection as the Session lists it: this conversation and the
    /// draft it proposes, the judged workflows (one in view), a run.
    fn project() -> Aside {
        Aside::new(
            "studio",
            Tab::Nika,
            vec![
                Entry::new(Icon::Conversation, "this conversation"),
                Entry::new(Icon::Workflow, "draft: weekly report").at(1),
                Entry::new(Icon::Workflow, "release.nika")
                    .judged(Verdict::Clean)
                    .opened(),
                Entry::new(Icon::Workflow, "enrich.nika").judged(Verdict::Findings(2)),
                Entry::new(Icon::Run, "#043 release.nika"),
            ],
            true,
        )
    }

    /// Sixteen entries of three kinds, to slide.
    fn long() -> Aside {
        let workflows = (1..=12).map(|n| Entry::new(Icon::Workflow, format!("flow-{n:02}.nika")));
        let runs = (1..=3).map(|n| Entry::new(Icon::Run, format!("#00{n} flow-{n:02}.nika")));
        let entries = std::iter::once(Entry::new(Icon::Conversation, "this conversation"))
            .chain(workflows)
            .chain(runs)
            .collect();
        Aside::new("studio", Tab::Nika, entries, true)
    }

    /// Thirty entries of four kinds, some nested, the listing partial and
    /// noted; each label carries its own `eNN-` token.
    fn mixed() -> Aside {
        let kinds = [
            (Icon::Conversation, 2),
            (Icon::Workflow, 14),
            (Icon::Run, 8),
            (Icon::Activation, 6),
        ];
        let entries = kinds
            .into_iter()
            .flat_map(|(icon, count)| std::iter::repeat_n(icon, count))
            .enumerate()
            .map(|(n, icon)| Entry::new(icon, format!("e{n:02}-unique")).at(u8::from(n % 4 == 1)))
            .collect();
        Aside::new("mixed", Tab::Nika, entries, false).noting("one note that wraps across rows")
    }

    /// The entries a painted row names, by their `eNN-` tokens.
    fn named(row: &str) -> Vec<usize> {
        row.match_indices('e')
            .filter_map(|(at, _)| {
                let digits = row.get(at + 1..at + 3)?;
                (row.get(at + 3..at + 4)? == "-")
                    .then_some(digits)?
                    .parse()
                    .ok()
            })
            .collect()
    }

    #[test]
    fn pointer_rows_follow_the_rendered_window_without_opening_headers_or_notes() {
        let aside = Aside::new(
            "many",
            Tab::Files,
            (0..30)
                .map(|n| Entry::new(Icon::File, format!("unique-{n:02}")))
                .collect(),
            false,
        )
        .noting("one note");
        for height in [0, 1, 2, 3, 4, 8, 20] {
            for anchor in [0, 7, 29] {
                let rows = lines_anchored(&aside, 24, height, false, false, None, anchor);
                for row in 0..height {
                    let hit = entry_at(&aside, 24, height, false, anchor, row);
                    let text = rows
                        .get(usize::from(row))
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    if let Some(index) = hit {
                        assert!(
                            text.contains(&format!("unique-{index:02}")),
                            "{row}: {text}"
                        );
                    } else {
                        assert!(!text.contains("unique-"), "visible entry missed: {text}");
                    }
                }
                assert_eq!(entry_at(&aside, 24, height, false, anchor, height), None);
            }
        }
    }

    /// Mixed kinds at every size and scroll anchor the pointer meets: a
    /// painted entry row maps to that entry alone; the region, projections,
    /// group, hidden-count, note and partial rows map to none; the selected
    /// entry stays in view wherever four rows of list remain.
    #[test]
    fn every_painted_row_maps_to_its_own_entry_and_every_other_row_is_inert() {
        let aside = mixed();
        for ascii in [false, true] {
            let (_, cut) = marks(ascii);
            for width in [16_u16, 24, 32] {
                let note = aside.note.as_deref().unwrap_or_default();
                let footer = wrap(note, usize::from(width), cut).len() + 1;
                for height in 0..=24_u16 {
                    for anchor in [0, 1, 7, 15, 23, 29] {
                        let rows = lines_anchored(
                            &aside,
                            width,
                            height,
                            ascii,
                            false,
                            Some(anchor),
                            anchor,
                        );
                        assert!(rows.len() <= usize::from(height), "{rows:?}");
                        let mut hits = Vec::new();
                        for row in 0..height {
                            let shown: String = rows
                                .get(usize::from(row))
                                .map(ToString::to_string)
                                .unwrap_or_default();
                            assert!(shown.width() <= usize::from(width), "{width}: {shown}");
                            let hit = entry_at(&aside, width, height, ascii, anchor, row);
                            assert_eq!(
                                named(&shown),
                                hit.into_iter().collect::<Vec<_>>(),
                                "{width}x{height} at {anchor}, row {row}: {shown}"
                            );
                            hits.extend(hit);
                        }
                        assert_eq!(entry_at(&aside, width, height, ascii, anchor, height), None);
                        assert!(
                            hits.windows(2).all(|pair| pair[1] == pair[0] + 1),
                            "{hits:?}"
                        );
                        if usize::from(height) >= 2 + footer + 4 {
                            assert!(hits.contains(&anchor), "{width}x{height}: {rows:?}");
                        }
                    }
                }
            }
        }
    }

    /// The header is the project's home: the aside names its region, then the
    /// projections, then each kind of entry under its own quiet heading, a
    /// draft staying under the conversation that proposes it.
    #[test]
    fn the_aside_names_its_region_and_groups_entries_by_their_kind() {
        let pad = |cells: usize| " ".repeat(cells);
        assert_eq!(
            text(&lines(&project(), 28, 20, false, false)),
            [
                "Project".to_owned(),
                "Nika · Files".to_owned(),
                "CONVERSATIONS".to_owned(),
                " ◌ this conversation".to_owned(),
                "   ⑂ draft: weekly report".to_owned(),
                "WORKFLOWS".to_owned(),
                format!("›⑂ release.nika{}ok", pad(11)),
                format!(" ⑂ enrich.nika{}2 findings", pad(4)),
                "RUNS".to_owned(),
                " > #043 release.nika".to_owned(),
            ]
        );
    }

    /// Group headings come from the entries' own kinds and never take an
    /// entry's place: each entry keeps its index and depth, and a heading row
    /// opens nothing. A single kind needs no heading.
    #[test]
    fn group_rows_are_inert_and_every_entry_keeps_its_index_and_depth() {
        let aside = project();
        for ascii in [false, true] {
            let rows = text(&lines(&aside, 28, 20, ascii, false));
            let hits: Vec<(u16, usize)> = (0..20_u16)
                .filter_map(|row| entry_at(&aside, 28, 20, ascii, 0, row).map(|at| (row, at)))
                .collect();
            assert_eq!(hits, [(3, 0), (4, 1), (6, 2), (7, 3), (9, 4)], "{rows:?}");
            for (row, heading) in [(2_u16, "CONVERSATIONS"), (5, "WORKFLOWS"), (8, "RUNS")] {
                assert_eq!(rows[usize::from(row)], heading, "{rows:?}");
                assert_eq!(entry_at(&aside, 28, 20, ascii, 0, row), None);
            }
            assert!(rows[4].starts_with("   ") && rows[4].ends_with("draft: weekly report"));
        }
        let files = (0..3)
            .map(|n| Entry::new(Icon::File, format!("notes-{n}.md")))
            .collect();
        let files = Aside::new("studio", Tab::Files, files, true);
        assert_eq!(
            text(&lines(&files, 28, 10, false, false))[2..],
            [" [F] notes-0.md", " [F] notes-1.md", " [F] notes-2.md"]
        );
    }

    #[test]
    fn the_chosen_projection_is_underlined_strong_and_accented() {
        let plain = lines(&project(), 28, 20, false, false);
        let chosen = Modifier::UNDERLINED | Modifier::BOLD;
        assert!(plain[1].spans[0].style.add_modifier.contains(chosen));
        assert!(
            !plain[1].spans[2]
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
        let accent = role::style(Role::Accent, true).fg;
        assert_eq!(
            lines(&project(), 28, 20, false, true)[1].spans[0].style.fg,
            accent
        );
        let files = Aside::new("studio", Tab::Files, Vec::new(), true);
        let colored = lines(&files, 28, 20, false, true);
        assert_eq!(colored[1].spans[2].style.fg, accent);
        assert_eq!(
            colored[1].spans[0].style.fg,
            role::style(Role::Dim, true).fg
        );
    }

    #[test]
    fn overflow_is_counted_and_a_partial_listing_says_so() {
        let mut aside = project();
        aside.complete = false;
        assert_eq!(
            text(&lines(&aside, 28, 7, false, false)),
            [
                "Project",
                "Nika · Files",
                "CONVERSATIONS",
                " ◌ this conversation",
                "   ⑂ draft: weekly report",
                "  +3 below",
                "partial listing",
            ]
        );
        let full = text(&lines(&project(), 28, 10, false, false));
        assert_eq!(full.len(), 10, "{full:?}");
        assert!(
            !full
                .iter()
                .any(|r| r.contains("above") || r.contains("below")),
            "{full:?}"
        );
    }

    #[test]
    fn narrow_labels_are_cut_and_ascii_replaces_every_glyph() {
        let rows = text(&lines(&project(), 14, 20, true, false));
        assert_eq!(rows[1], "Nika - Files");
        assert!(
            rows.iter().all(|r| r.is_ascii() && r.width() <= 14),
            "{rows:?}"
        );
        assert_eq!(rows[6], "*[W] rel... ok");
    }

    /// Every row keeps within its cells at any width, in both glyph columns:
    /// the projections, a deep entry and a wide label are cut, never spilled.
    /// With colour the object in view and the selection fill exactly their
    /// row, the rows the pointer maps to those entries.
    #[test]
    fn every_row_keeps_within_its_width_in_both_glyph_columns() {
        let mut aside = project();
        aside.entries.push(
            Entry::new(Icon::Workflow, "日本語のワークフロー.nika")
                .at(3)
                .judged(Verdict::Findings(12)),
        );
        aside.complete = false;
        let aside = aside.noting("the file listing needs a Session contract");
        for width in 0..=40_u16 {
            for ascii in [false, true] {
                for color in [false, true] {
                    for selected in [None, Some(2), Some(5)] {
                        let anchor = selected.unwrap_or(0);
                        let rows = text(&lines_anchored(
                            &aside, width, 30, ascii, color, selected, anchor,
                        ));
                        for (at, row) in (0_u16..).zip(&rows) {
                            assert!(row.width() <= usize::from(width), "{width}: {rows:?}");
                            if ascii {
                                assert!(!row.contains(['›', '▐', '·', '…', '◌', '⑂']), "{rows:?}");
                            }
                            let hit = entry_at(&aside, width, 30, ascii, anchor, at);
                            let lifted = hit == Some(2) || (hit.is_some() && hit == selected);
                            if color && lifted {
                                assert_eq!(row.width(), usize::from(width), "{width}: {rows:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    /// The entry the keys select wears a bar, the accent and an underline;
    /// the object in view is marked and set in weight; both read without
    /// colour, apart or together, and no row is ever inverted.
    #[test]
    fn the_selection_and_the_object_in_view_read_apart_without_inverting_a_row() {
        let label = |line: &Line<'_>, words: &str| {
            line.spans
                .iter()
                .find(|span| span.content == words)
                .map(|span| span.style)
        };
        let pad = |cells: usize| " ".repeat(cells);
        let plain = lines_selecting(&project(), 28, 20, false, false, Some(3));
        let rows = text(&plain);
        assert_eq!(rows[6], format!("›⑂ release.nika{}ok", pad(11)));
        assert_eq!(rows[7], format!("▐⑂ enrich.nika{}2 findings", pad(4)));
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let underlined = Style::default().add_modifier(Modifier::UNDERLINED);
        assert_eq!(label(&plain[6], "release.nika"), Some(bold));
        assert_eq!(label(&plain[7], "enrich.nika"), Some(underlined));
        let both = lines_selecting(&project(), 28, 20, false, false, Some(2));
        assert!(text(&both)[6].starts_with("▐⑂ release.nika"), "{both:?}");
        assert_eq!(
            label(&both[6], "release.nika"),
            Some(bold.patch(underlined))
        );
        let ascii = text(&lines_selecting(&project(), 28, 20, true, false, Some(3)));
        assert_eq!(ascii[6], format!("*[W] release.nika{}ok", pad(9)));
        assert_eq!(ascii[7], format!(">[W] enrich.nika{}2 findings", pad(2)));
        let colored = lines_selecting(&project(), 28, 20, false, true, Some(3));
        let accent = role::style(Role::Accent, true).fg;
        assert_eq!(label(&colored[7], "enrich.nika").and_then(|s| s.fg), accent);
        assert_eq!(label(&colored[7], "▐").and_then(|s| s.fg), accent);
        assert_eq!(
            label(&colored[6], "release.nika").and_then(|s| s.fg),
            role::style(Role::Strong, true).fg
        );
        for line in plain.iter().chain(&both).chain(&colored) {
            let inverted = std::iter::once(line.style)
                .chain(line.spans.iter().map(|span| span.style))
                .any(|style| style.add_modifier.contains(Modifier::REVERSED));
            assert!(!inverted, "{line:?}");
        }
    }

    /// A slid list counts what it hides where it is hidden, names the group
    /// of its first listed entry again, and keeps the selection in view (the
    /// last listed entry in the middle of the list). Below four rows of list
    /// it is not grouped, so the selection keeps its row.
    #[test]
    fn a_slid_list_counts_hidden_entries_and_heads_its_first_group() {
        let at = |anchor: usize, height: u16| {
            text(&lines_anchored(
                &long(),
                24,
                height,
                false,
                false,
                Some(anchor),
                anchor,
            ))
        };
        assert_eq!(
            at(0, 10),
            [
                "Project",
                "Nika · Files",
                "CONVERSATIONS",
                "▐◌ this conversation",
                "WORKFLOWS",
                " ⑂ flow-01.nika",
                " ⑂ flow-02.nika",
                " ⑂ flow-03.nika",
                " ⑂ flow-04.nika",
                "  +11 below",
            ]
        );
        assert_eq!(
            at(8, 10),
            [
                "Project",
                "Nika · Files",
                "  +4 above",
                "WORKFLOWS",
                " ⑂ flow-04.nika",
                " ⑂ flow-05.nika",
                " ⑂ flow-06.nika",
                " ⑂ flow-07.nika",
                "▐⑂ flow-08.nika",
                "  +7 below",
            ]
        );
        assert_eq!(
            at(15, 10),
            [
                "Project",
                "Nika · Files",
                "  +11 above",
                "WORKFLOWS",
                " ⑂ flow-11.nika",
                " ⑂ flow-12.nika",
                "RUNS",
                " > #001 flow-01.nika",
                " > #002 flow-02.nika",
                "▐> #003 flow-03.nika",
            ]
        );
        assert_eq!(
            at(8, 5),
            [
                "Project",
                "Nika · Files",
                "  +8 above",
                "▐⑂ flow-08.nika",
                "  +7 below"
            ]
        );
    }

    /// A workflow wears the checker's verdict in words at the row's end; a
    /// narrow row cuts the label first and keeps the verdict.
    #[test]
    fn a_workflow_wears_its_verdict_in_words_at_the_row_end() {
        let aside = Aside::new(
            "demo",
            Tab::Nika,
            vec![
                Entry::new(Icon::Workflow, "release.nika").judged(Verdict::Clean),
                Entry::new(Icon::Workflow, "enrich.nika").judged(Verdict::Findings(2)),
                Entry::new(Icon::Workflow, "broken.nika").judged(Verdict::Findings(0)),
            ],
            true,
        );
        let rows = text(&lines(&aside, 28, 10, false, false));
        let row =
            |label: &str, gap: usize, words: &str| format!(" ⑂ {label}{}{words}", " ".repeat(gap));
        assert_eq!(rows[2], row("release.nika", 11, "ok"));
        assert_eq!(rows[3], row("enrich.nika", 4, "2 findings"));
        assert_eq!(rows[4], row("broken.nika", 5, "not clean"));
        assert!(rows[2..5].iter().all(|r| r.width() == 28), "{rows:?}");
        let narrow = text(&lines(&aside, 20, 10, true, false));
        assert_eq!(narrow[3], " [W] e... 2 findings");
        assert!(narrow.iter().all(|r| r.width() <= 20), "{narrow:?}");
        let colored = lines(&aside, 28, 10, false, true);
        let verdict = |line: &Line<'_>| line.spans.last().map(|span| span.style);
        assert_eq!(verdict(&colored[2]), Some(role::style(Role::Good, true)));
        assert_eq!(verdict(&colored[3]), Some(role::style(Role::Warn, true)));
        assert_eq!(Verdict::Findings(1).words(), "1 finding");
    }

    /// What a projection cannot list is said in words, wrapped to the width.
    #[test]
    fn a_note_says_what_the_projection_cannot_list() {
        let files = Aside::new("demo", Tab::Files, Vec::new(), true)
            .noting("the file listing needs a Session contract");
        let rows = text(&lines(&files, 20, 10, false, false));
        assert_eq!(
            rows[2..],
            ["the file listing", "needs a Session", "contract"]
        );
        let partial = Aside::new("demo", Tab::Nika, Vec::new(), false).noting("no workflow");
        assert_eq!(
            text(&lines(&partial, 20, 10, false, false))[2..],
            ["no workflow", "partial listing"]
        );
    }

    /// `lines` painted into a buffer `width` cells wide and 20 rows tall, as
    /// the renderer paints the aside: one paragraph, no wrapping.
    fn painted(lines: Vec<Line<'static>>, width: u16) -> Buffer {
        let area = Rect::new(0, 0, width, 20);
        let mut buffer = Buffer::empty(area);
        Paragraph::new(lines).render(area, &mut buffer);
        buffer
    }

    /// As the renderer paints it, the object in view stands on the selection
    /// fill across its whole width wherever the keys are (none selected: they
    /// are in the conversation or the object), and so does the entry the keys
    /// select, the blank cells after its words included. No other row takes
    /// the fill; the selection keeps its bar, accent and underline, the object
    /// in view its mark and weight, and a verdict its hue.
    #[test]
    fn the_open_and_selected_rows_are_lifted_across_their_width_and_no_other_row_is() {
        let fill = role::selection(true).bg;
        let accent = role::style(Role::Accent, true).fg;
        let strong = role::style(Role::Strong, true).fg;
        let warn = role::style(Role::Warn, true).fg;
        // release.nika, the object in view, is painted on this row.
        let open = 6_u16;
        for width in [16_u16, 32] {
            for ascii in [false, true] {
                let (bar, mark, label) = if ascii {
                    (">", "*", 5)
                } else {
                    ("▐", "›", 3)
                };
                // The keys elsewhere; enrich.nika, its verdict at the row's end;
                // this conversation, blank cells after its words at 32 columns;
                // the object in view itself.
                for (selected, at) in [
                    (None, None),
                    (Some(3), Some(7_u16)),
                    (Some(0), Some(3)),
                    (Some(2), Some(open)),
                ] {
                    let lines = lines_selecting(&project(), width, 20, ascii, true, selected);
                    let buffer = painted(lines, width);
                    for y in 0..20_u16 {
                        let filled = (0..width)
                            .filter(|&x| Some(buffer[(x, y)].bg) == fill)
                            .count();
                        let lifted = y == open || Some(y) == at;
                        let expected = if lifted { usize::from(width) } else { 0 };
                        assert_eq!(filled, expected, "{width} {ascii} {selected:?}: row {y}");
                    }
                    if let Some(at) = at {
                        assert_eq!(buffer[(0, at)].symbol(), bar);
                        assert_eq!(Some(buffer[(0, at)].fg), accent);
                        assert!(buffer[(label, at)].modifier.contains(Modifier::UNDERLINED));
                        assert_eq!(Some(buffer[(label, at)].fg), accent);
                    }
                    if at != Some(open) {
                        // The object in view: its mark and weight, no underline.
                        let cell = &buffer[(label, open)];
                        assert_eq!(buffer[(0, open)].symbol(), mark);
                        assert_eq!(Some(cell.fg), strong);
                        assert!(cell.modifier.contains(Modifier::BOLD));
                        assert!(!cell.modifier.contains(Modifier::UNDERLINED));
                    }
                }
            }
            // The verdict keeps its words and its hue on the fill.
            let buffer = painted(
                lines_selecting(&project(), width, 20, false, true, Some(3)),
                width,
            );
            let mut verdict = width - 10..width;
            let words: String = verdict.clone().map(|x| buffer[(x, 7)].symbol()).collect();
            assert_eq!(words, "2 findings", "{width}");
            assert!(verdict.clone().all(|x| Some(buffer[(x, 7)].fg) == warn));
            assert!(verdict.all(|x| Some(buffer[(x, 7)].bg) == fill));
        }
    }

    /// Without colour no cell of the aside has a background or a hue wherever
    /// the keys are, no row is padded, the selection keeps its bar and
    /// underline and the object in view its mark and weight.
    #[test]
    fn without_colour_no_row_has_a_background_and_the_selection_keeps_its_marks() {
        for width in [16_u16, 32] {
            for (selected, at) in [(None, None), (Some(3), Some(7_u16)), (Some(0), Some(3))] {
                let lines = lines_selecting(&project(), width, 20, false, false, selected);
                let buffer = painted(lines, width);
                for y in 0..20_u16 {
                    for x in 0..width {
                        let cell = &buffer[(x, y)];
                        assert_eq!((cell.bg, cell.fg), (Color::Reset, Color::Reset), "{x},{y}");
                    }
                }
                if let Some(at) = at {
                    assert_eq!(buffer[(0, at)].symbol(), "▐");
                    assert!(buffer[(3, at)].modifier.contains(Modifier::UNDERLINED));
                }
                assert_eq!(buffer[(0, 6)].symbol(), "›");
                assert!(buffer[(3, 6)].modifier.contains(Modifier::BOLD));
            }
        }
        // The row ends with its words: nothing is added where no fill shows.
        let plain = lines_selecting(&project(), 32, 20, false, false, Some(0));
        assert_eq!(plain[3].width(), "▐◌ this conversation".width());
        let colored = lines_selecting(&project(), 32, 20, false, true, Some(0));
        assert_eq!(colored[3].width(), 32);
        // So for the object in view without a verdict, the keys elsewhere.
        let mut talk = project();
        talk.entries[2].open = false;
        talk.entries[0].open = true;
        let open = |color| lines(&talk, 32, 20, false, color)[3].width();
        assert_eq!(open(false), "›◌ this conversation".width());
        assert_eq!(open(true), 32);
    }
}
