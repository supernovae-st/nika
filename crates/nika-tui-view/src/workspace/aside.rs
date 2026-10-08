// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project aside: what the project named in the header holds, in two
//! projections, **Nika** (conversations, workflows, runs, activations) and
//! **Files**. It lists only entries the Session exposes, a workflow with the
//! installed checker's verdict in words; an inventory the Session marks partial
//! says so on its last row instead of pretending to show the whole disk, and
//! what a projection cannot list is said in a note, never left blank. When the
//! selection slides the list, the rows it hides are counted where they are,
//! above or below. Opening an entry changes the object in view, never the
//! conversation, and never attaches its content to the next message.

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

/// The listed window `[start, end)` of `count` entries in `room` rows, the
/// selected entry always inside it: a row says how many entries are hidden
/// above when the list slid, another how many below.
fn window(count: usize, room: usize, selected: Option<usize>) -> (usize, usize) {
    if count <= room {
        return (0, count);
    }
    let at = selected.map_or(0, |s| s.min(count - 1));
    // One row counts what is hidden: below at the top, above at the end.
    let one = room.saturating_sub(1).max(1);
    if at < one {
        return (0, one);
    }
    if at >= count - one {
        return (count - one, count);
    }
    // In the middle both rows count, and the selection stands last.
    let both = room.saturating_sub(2).max(1);
    let start = at + 1 - both;
    (start, start + both)
}

/// The aside lines with the entry at `selected` reversed (the keyboard is in
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
    let (width, height) = (usize::from(width), usize::from(height));
    let (sep, cut) = marks(ascii);
    let dim = role::style(Role::Dim, color);
    let strong = role::style(Role::Strong, color);
    let chosen = strong.add_modifier(Modifier::UNDERLINED);
    let scope = if aside.project.is_empty() {
        "no project".to_owned()
    } else {
        format!("in {}", aside.project)
    };
    let mut out = vec![Line::from(Span::styled(fit_head(&scope, width, cut), dim))];
    let (nika, files) = match aside.tab {
        Tab::Nika => (chosen, dim),
        Tab::Files => (dim, chosen),
    };
    out.push(Line::from(vec![
        Span::styled("Nika", nika),
        Span::styled(sep.trim_end().to_owned() + " ", dim),
        Span::styled("Files", files),
    ]));
    let note = aside
        .note
        .as_deref()
        .map(|n| wrap(n, width, cut))
        .unwrap_or_default();
    let count = aside.entries.len();
    let (start, end) = entry_window(aside, width, height, ascii, anchor);
    if start > 0 {
        out.push(Line::from(Span::styled(format!("  +{start} above"), dim)));
    }
    for (index, entry) in aside.entries.iter().enumerate().take(end).skip(start) {
        let row = entry_row(entry, width, ascii, color);
        // A weight, never a hue: the selection reads without colour too.
        out.push(if selected == Some(index) {
            row.patch_style(Style::default().add_modifier(Modifier::REVERSED))
        } else {
            row
        });
    }
    if end < count {
        out.push(Line::from(Span::styled(
            format!("  +{} below", count - end),
            dim,
        )));
    }
    out.extend(
        note.into_iter()
            .map(|row| Line::from(Span::styled(row, dim))),
    );
    if !aside.complete {
        out.push(Line::from(Span::styled(
            fit_head("partial listing", width, cut),
            dim,
        )));
    }
    out.truncate(height);
    out
}

/// The renderer and pointer share the exact listing window, including note rows.
fn entry_window(
    aside: &Aside,
    width: usize,
    height: usize,
    ascii: bool,
    anchor: usize,
) -> (usize, usize) {
    let (_, cut) = marks(ascii);
    let note_rows = aside
        .note
        .as_deref()
        .map_or(0, |note| wrap(note, width, cut).len());
    let footer = usize::from(!aside.complete) + note_rows;
    window(
        aside.entries.len(),
        height.saturating_sub(2 + footer),
        Some(anchor),
    )
}

/// The index of the entry [`lines_anchored`] paints at `row` (counted from the
/// aside's top) for the same `width`, `height`, glyph column and `anchor`.
/// Headers, omitted-count rows and footers never alias a file. The right edge
/// is excluded by the caller.
#[must_use]
pub fn entry_at(
    aside: &Aside,
    width: u16,
    height: u16,
    ascii: bool,
    anchor: usize,
    row: u16,
) -> Option<usize> {
    if row >= height {
        return None;
    }
    let (start, end) = entry_window(
        aside,
        usize::from(width),
        usize::from(height),
        ascii,
        anchor,
    );
    let first = 2 + usize::from(start > 0);
    let index = start.checked_add(usize::from(row).checked_sub(first)?)?;
    (index < end).then_some(index)
}

/// One entry's row, `width` cells: the open marker, the indent, the glyph,
/// the label, and the verdict at the row's end. A narrow row cuts the label
/// before the verdict, and drops the verdict only when no label would remain.
fn entry_row(entry: &Entry, width: usize, ascii: bool, color: bool) -> Line<'static> {
    let (_, cut) = marks(ascii);
    let dim = role::style(Role::Dim, color);
    let strong = role::style(Role::Strong, color);
    let indent = "  ".repeat(usize::from(entry.depth));
    let glyph = entry.icon.glyph(ascii);
    let marker = if entry.open {
        if ascii { ">" } else { "›" }
    } else {
        " "
    };
    let head = format!("{marker}{indent}{glyph} ");
    let style = if entry.open { strong } else { Style::default() };
    let verdict = entry.verdict.map(|v| (v.words(), v.role()));
    let room = width.saturating_sub(head.width());
    // The verdict, a space, and at least one character of the label and the cut.
    let with_verdict = verdict
        .as_ref()
        .filter(|(words, _)| room > words.width() + 1 + cut.width());
    let Some((words, verdict_role)) = with_verdict else {
        let label = fit_head(&entry.label, room, cut);
        return Line::from(vec![Span::styled(head, dim), Span::styled(label, style)]);
    };
    let label = fit_head(&entry.label, room - words.width() - 1, cut);
    let pad = room - label.width() - words.width();
    Line::from(vec![
        Span::styled(head, dim),
        Span::styled(label, style),
        Span::raw(" ".repeat(pad)),
        Span::styled(words.clone(), role::style(*verdict_role, color)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    fn studio() -> Aside {
        Aside::new(
            "studio",
            Tab::Nika,
            vec![
                Entry::new(Icon::Conversation, "Prepare the release"),
                Entry::new(Icon::Workflow, "workflows"),
                Entry::new(Icon::Workflow, "release.nika").at(1).opened(),
                Entry::new(Icon::Workflow, "enrich.nika").at(1),
                Entry::new(Icon::Activation, "weekly digest"),
            ],
            true,
        )
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

    #[test]
    fn the_aside_names_its_project_the_projections_and_the_open_entry() {
        let rows = text(&lines(&studio(), 28, 20, false, false));
        assert_eq!(rows[0], "in studio");
        assert_eq!(rows[1], "Nika · Files");
        assert_eq!(rows[2], " ◌ Prepare the release");
        assert_eq!(rows[4], "›  ⑂ release.nika");
        assert_eq!(rows[5], "   ⑂ enrich.nika");
        assert_eq!(rows.len(), 7);
    }

    #[test]
    fn the_chosen_projection_is_underlined_and_strong() {
        let lines = lines(&studio(), 28, 20, false, false);
        assert!(
            lines[1].spans[0]
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
        assert!(
            !lines[1].spans[2]
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
    }

    #[test]
    fn overflow_is_counted_and_a_partial_listing_says_so() {
        let mut aside = studio();
        aside.complete = false;
        let rows = text(&lines(&aside, 28, 6, false, false));
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[4], "  +3 below");
        assert_eq!(rows[5], "partial listing");
        let full = text(&lines(&studio(), 28, 7, false, false));
        assert!(
            !full
                .iter()
                .any(|r| r.contains("above") || r.contains("below")),
            "{full:?}"
        );
    }

    #[test]
    fn narrow_labels_are_cut_and_ascii_replaces_every_glyph() {
        let rows = text(&lines(&studio(), 14, 20, true, false));
        assert_eq!(rows[1], "Nika - Files");
        assert!(
            rows.iter().all(|r| r.is_ascii() && r.width() <= 14),
            "{rows:?}"
        );
        assert_eq!(rows[4], ">  [W] rele...");
    }

    /// Sliding the list counts the hidden entries where they are: above when
    /// the selection went down, below and above in the middle of the list.
    #[test]
    fn the_selected_entry_is_reversed_and_the_hidden_ones_counted_where_they_are() {
        let reversed = |line: &Line<'_>| line.style.add_modifier.contains(Modifier::REVERSED);
        let all = lines_selecting(&studio(), 28, 20, false, false, Some(1));
        assert!(reversed(&all[3]) && !reversed(&all[2]), "{all:?}");
        // Five entries in four rows: selecting the last slides the list.
        let tight = lines_selecting(&studio(), 28, 6, false, false, Some(4));
        let rows = text(&tight);
        assert_eq!(rows[2], "  +2 above", "{rows:?}");
        assert!(rows[5].ends_with("weekly digest"), "{rows:?}");
        assert!(reversed(&tight[5]) && !reversed(&tight[4]));
        assert!(!rows.iter().any(|r| r.contains("below")), "{rows:?}");
        // Seven entries in four rows, the fourth selected: both sides counted,
        // the selection on the last listed row.
        let mut long = studio();
        long.entries.push(Entry::new(Icon::Run, "#043"));
        long.entries.push(Entry::new(Icon::Run, "#044"));
        let middle = lines_selecting(&long, 28, 6, false, false, Some(3));
        let rows = text(&middle);
        assert_eq!(rows.len(), 6, "{rows:?}");
        assert_eq!(rows[2], "  +2 above", "{rows:?}");
        assert!(
            rows[4].ends_with("enrich.nika") && reversed(&middle[4]),
            "{rows:?}"
        );
        assert_eq!(rows[5], "  +3 below", "{rows:?}");
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
        assert_eq!(colored[2].spans[3].style, role::style(Role::Good, true));
        assert_eq!(colored[3].spans[3].style, role::style(Role::Warn, true));
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
}
