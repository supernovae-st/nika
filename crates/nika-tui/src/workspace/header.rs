// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The header answers « where am I »: the active project (a selector, marked by
//! its chevron, never a hidden button), its location and the host, then the
//! facts about that location the Session observed. Git and a `nika.yaml` are
//! both optional: a known absence is said, an unobserved fact is not invented.
//! When the row is too narrow the location gives way from its start, never the
//! project name.

use nika_display::theme::Role;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::visual::icon::Icon;
use crate::visual::role;

/// Where the human stands, as the Session projects it; every field is a fact
/// it observed or `None`.
// No `Default`: on Linux a Default type gains blanket zvariant rows in the public API,
// and a place always names its host (`Place::on`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Place {
    /// The host or workspace the project lives on (`local`, a server name).
    pub host: String,
    /// The active project's name; `None` when no project is active.
    pub project: Option<String>,
    /// The project's location as shown to the human (home-relative).
    pub location: Option<String>,
    /// Whether the location is inside a Git work tree, when observed.
    pub git: Option<bool>,
    /// Whether a `nika.yaml` governs the project, when observed.
    pub manifest: Option<bool>,
}

impl Place {
    /// A place on `host` with no active project.
    #[must_use]
    pub fn on(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            project: None,
            location: None,
            git: None,
            manifest: None,
        }
    }

    /// This place with `project` active at `location`.
    #[must_use]
    pub fn with_project(mut self, project: impl Into<String>, location: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self.location = Some(location.into());
        self
    }

    /// This place with the observed Git and manifest facts.
    #[must_use]
    pub fn observed(mut self, git: bool, manifest: bool) -> Self {
        self.git = Some(git);
        self.manifest = Some(manifest);
        self
    }

    /// The observed facts about the location, in words; unobserved ones are absent.
    fn facts(&self) -> Vec<&'static str> {
        let git = self.git.map(|g| if g { "git" } else { "no git" });
        let manifest = self
            .manifest
            .map(|m| if m { "nika.yaml" } else { "no nika.yaml" });
        git.into_iter().chain(manifest).collect()
    }
}

/// The header lines for a `width`-wide header of `rows` rows (one or two).
#[must_use]
pub fn lines(place: &Place, width: u16, rows: u16, ascii: bool, color: bool) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let (sep, cut) = if ascii {
        (" - ", "...")
    } else {
        (" · ", "…")
    };
    let dim = role::style(Role::Dim, color);
    let strong = role::style(Role::Strong, color);
    let icon = Icon::Project.glyph(ascii);
    let name = place
        .project
        .clone()
        .unwrap_or_else(|| "no project".to_owned());
    let chevron = Icon::Choice.glyph(ascii);
    let head_width = icon.width() + 1 + name.width() + 1 + chevron.width();
    let mut first = vec![
        Span::styled(format!("{icon} "), dim),
        Span::styled(name, strong),
        Span::styled(format!(" {chevron}"), dim),
    ];
    let facts = place.facts().join(sep);
    let location = place.location.clone().unwrap_or_default();
    if rows >= 2 {
        first.push(Span::styled(format!("  {}", place.host), dim));
        let mut second = format!("  {location}");
        if !facts.is_empty() {
            second.push_str(sep);
            second.push_str(&facts);
        }
        let second = fit_tail(&second, width, 2, cut);
        return vec![Line::from(first), Line::from(Span::styled(second, dim))];
    }
    let mut tail = String::new();
    for part in [location.as_str(), facts.as_str(), place.host.as_str()] {
        if !part.is_empty() {
            tail.push_str(sep);
            tail.push_str(part);
        }
    }
    let room = width.saturating_sub(head_width);
    first.push(Span::styled(
        fit_tail(&tail, room, sep.chars().count(), cut),
        dim,
    ));
    vec![Line::from(first)]
}

/// Draw the header into `area`.
pub fn render(place: &Place, area: Rect, buf: &mut Buffer, ascii: bool, color: bool) {
    Paragraph::new(lines(place, area.width, area.height, ascii, color)).render(area, buf);
}

/// `text` fitted to `width` cells: its first `keep` characters stay, then the
/// `cut` mark, then as much of its end as fits; empty when not even the kept
/// start and the mark fit.
fn fit_tail(text: &str, width: usize, keep: usize, cut: &str) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let head: String = text.chars().take(keep).collect();
    if width < head.width() + cut.width() {
        return String::new();
    }
    let budget = width - head.width() - cut.width();
    let mut tail: Vec<char> = Vec::new();
    let mut used = 0;
    for c in text
        .chars()
        .skip(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > budget {
            break;
        }
        used += w;
        tail.push(c);
    }
    tail.reverse();
    format!("{head}{cut}{}", tail.into_iter().collect::<String>())
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

    fn studio() -> Place {
        Place::on("local")
            .with_project("studio", "~/Projects/studio")
            .observed(true, false)
    }

    #[test]
    fn one_row_names_the_project_its_location_facts_and_host() {
        let rows = text(&lines(&studio(), 80, 1, false, false));
        assert_eq!(
            rows,
            ["▱ studio ⌄ · ~/Projects/studio · git · no nika.yaml · local"]
        );
    }

    #[test]
    fn two_rows_put_the_location_and_its_facts_under_the_selector() {
        let rows = text(&lines(&studio(), 120, 2, false, false));
        assert_eq!(rows[0], "▱ studio ⌄  local");
        assert_eq!(rows[1], "  ~/Projects/studio · git · no nika.yaml");
    }

    #[test]
    fn unobserved_facts_are_never_invented_and_no_project_is_said() {
        let place = Place::on("local").with_project("veille", "~/veille");
        assert_eq!(
            text(&lines(&place, 80, 1, false, false)),
            ["▱ veille ⌄ · ~/veille · local"]
        );
        let none = Place::on("local");
        assert_eq!(
            text(&lines(&none, 80, 1, false, false)),
            ["▱ no project ⌄ · local"]
        );
    }

    #[test]
    fn a_narrow_row_cuts_the_location_from_its_start_never_the_name() {
        let place = Place::on("local")
            .with_project("studio", "~/Projects/clients/acme/campaigns/2026/studio")
            .observed(true, true);
        for width in [60_u16, 48, 40] {
            let rows = text(&lines(&place, width, 1, false, false));
            assert!(rows[0].starts_with("▱ studio ⌄ · …"), "{rows:?}");
            assert!(rows[0].width() <= usize::from(width), "{width}: {rows:?}");
            assert!(rows[0].ends_with("local"), "{rows:?}");
        }
        let two = text(&lines(&place, 30, 2, false, false));
        assert!(two[1].starts_with("  …") && two[1].width() <= 30, "{two:?}");
    }

    #[test]
    fn the_ascii_column_replaces_every_glyph_and_separator() {
        let rows = text(&lines(&studio(), 80, 1, true, false));
        assert_eq!(
            rows,
            ["[P] studio v - ~/Projects/studio - git - no nika.yaml - local"]
        );
        let place =
            Place::on("local").with_project("studio", "~/a/very/long/location/for/a/narrow/row");
        let cut = text(&lines(&place, 40, 1, true, false));
        assert!(
            cut[0].is_ascii() && cut[0].contains("...") && cut[0].len() <= 40,
            "{cut:?}"
        );
    }

    #[test]
    fn colour_off_keeps_the_project_name_strong() {
        let lines = lines(&studio(), 80, 1, false, false);
        let name = &lines[0].spans[1];
        assert_eq!(name.content, "studio");
        assert_eq!(name.style, role::style(Role::Strong, false));
    }
}
