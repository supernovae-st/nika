// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project aside: what the project named in the header holds, in two
//! projections, **Nika** (conversations, workflows, runs, activations) and
//! **Files**. It lists only entries the Session exposes; an inventory the
//! Session marks partial says so on its last row instead of pretending to show
//! the whole disk. Opening an entry changes the object in view, never the
//! conversation, and never attaches its content to the next message.

use nika_display::theme::Role;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::text::{fit_head, marks};
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
}

impl Entry {
    /// A top-level entry, not open.
    #[must_use]
    pub fn new(icon: Icon, label: impl Into<String>) -> Self {
        Self {
            icon,
            label: label.into(),
            depth: 0,
            open: false,
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
    let (width, height) = (usize::from(width), usize::from(height));
    let (sep, cut) = marks(ascii);
    let dim = role::style(Role::Dim, color);
    let strong = role::style(Role::Strong, color);
    let chosen = strong.add_modifier(Modifier::UNDERLINED);
    let mut out = vec![Line::from(Span::styled(
        fit_head(&format!("in {}", aside.project), width, cut),
        dim,
    ))];
    let (nika, files) = match aside.tab {
        Tab::Nika => (chosen, dim),
        Tab::Files => (dim, chosen),
    };
    out.push(Line::from(vec![
        Span::styled("Nika", nika),
        Span::styled(sep.trim_end().to_owned() + " ", dim),
        Span::styled("Files", files),
    ]));
    let footer = usize::from(!aside.complete);
    let room = height.saturating_sub(out.len() + footer);
    let hidden = aside.entries.len().saturating_sub(room);
    // When entries are hidden, the last listed row says how many.
    let listed = if hidden > 0 {
        room.saturating_sub(1)
    } else {
        room
    };
    let start = match selected {
        Some(at) if listed > 0 && at >= listed => at + 1 - listed,
        _ => 0,
    };
    for (index, entry) in aside.entries.iter().enumerate().skip(start).take(listed) {
        let indent = "  ".repeat(usize::from(entry.depth));
        let glyph = entry.icon.glyph(ascii);
        let marker = if entry.open {
            if ascii { ">" } else { "›" }
        } else {
            " "
        };
        let head = format!("{marker}{indent}{glyph} ");
        let label = fit_head(&entry.label, width.saturating_sub(head.width()), cut);
        let style = if entry.open { strong } else { Style::default() };
        let row = Line::from(vec![Span::styled(head, dim), Span::styled(label, style)]);
        // A weight, never a hue: the selection reads without colour too.
        out.push(if selected == Some(index) {
            row.patch_style(Style::default().add_modifier(Modifier::REVERSED))
        } else {
            row
        });
    }
    if hidden > 0 && room > 0 {
        let more = aside.entries.len() - listed;
        out.push(Line::from(Span::styled(format!("  +{more} more"), dim)));
    }
    if !aside.complete {
        out.push(Line::from(Span::styled(
            fit_head("partial listing", width, cut),
            dim,
        )));
    }
    out.truncate(height);
    out
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
        Aside {
            project: "studio".into(),
            tab: Tab::Nika,
            entries: vec![
                Entry::new(Icon::Conversation, "Prepare the release"),
                Entry::new(Icon::Workflow, "workflows"),
                Entry::new(Icon::Workflow, "release.nika").at(1).opened(),
                Entry::new(Icon::Workflow, "enrich.nika").at(1),
                Entry::new(Icon::Activation, "weekly digest"),
            ],
            complete: true,
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
        assert_eq!(rows[4], "  +3 more");
        assert_eq!(rows[5], "partial listing");
        let full = text(&lines(&studio(), 28, 7, false, false));
        assert!(!full.iter().any(|r| r.contains("more")), "{full:?}");
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

    #[test]
    fn the_selected_entry_is_reversed_and_always_listed() {
        let reversed = |line: &Line<'_>| line.style.add_modifier.contains(Modifier::REVERSED);
        let all = lines_selecting(&studio(), 28, 20, false, false, Some(1));
        assert!(reversed(&all[3]) && !reversed(&all[2]), "{all:?}");
        // Five entries in three listed rows: selecting the last slides the list.
        let tight = lines_selecting(&studio(), 28, 6, false, false, Some(4));
        let rows = text(&tight);
        assert_eq!(rows[5], "  +2 more");
        assert!(rows[4].ends_with("weekly digest"), "{rows:?}");
        assert!(reversed(&tight[4]) && !reversed(&tight[3]));
    }
}
