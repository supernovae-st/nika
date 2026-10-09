// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The header answers « where am I », and it is the project's one home: the
//! active project's name, strong, then in a quieter voice what it is
//! (`local project`), its location and the facts about that location the
//! Session observed. The aside and the conversation do not repeat the name,
//! and no chevron offers a project menu the workspace does not open. Git and
//! a `nika.yaml` are both optional: a known absence is said, a `nika.yaml`
//! that governs from an ancestor is named by its real path, one the Session
//! refused says so in the warning role beside its words, and an unobserved
//! fact is not invented. When the row is too narrow the location gives way
//! from its start, then the Git fact, the governing file's words last; the
//! project's name is cut only when it alone is wider than the row.

use nika_display::theme::Role;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use super::text::{fit_head, fit_tail, marks};
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
    /// What `nika.yaml` governs the project, when observed.
    pub manifest: Option<Manifest>,
}

/// What governs a location, as the Session's discovery found it: the first
/// `nika.yaml` up from the location, none, or one it refused to read.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Manifest {
    /// A `nika.yaml` in the location itself.
    Here,
    /// A `nika.yaml` in an ancestor, by its path from the location
    /// (`../../nika.yaml`): a parent's file governs, and the header says whose.
    Above(String),
    /// No `nika.yaml` governs the location.
    Absent,
    /// A `nika.yaml` governs, and the Session refused it (unreadable or
    /// malformed): nothing it declares applies until it is corrected.
    Refused,
}

/// What a refused `nika.yaml` reads; these words wear the warning role.
const REFUSED: &str = "nika.yaml refused";

/// The narrowest cut location worth showing: the cut mark and seven cells
/// of its end.
const MIN_LOCATION: usize = 8;

impl Manifest {
    /// The fact in words, the same in both glyph columns.
    fn words(&self) -> String {
        match self {
            Self::Here => "nika.yaml".to_owned(),
            Self::Above(path) => path.clone(),
            Self::Absent => "no nika.yaml".to_owned(),
            Self::Refused => REFUSED.to_owned(),
        }
    }
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

    /// This place with the observed Git fact and a `nika.yaml` present in the
    /// location (`true`) or absent everywhere up from it (`false`).
    #[must_use]
    pub fn observed(mut self, git: bool, manifest: bool) -> Self {
        self.git = Some(git);
        self.manifest = Some(if manifest {
            Manifest::Here
        } else {
            Manifest::Absent
        });
        self
    }

    /// This place governed by `manifest`, as the Session found it.
    #[must_use]
    pub fn governed(mut self, manifest: Manifest) -> Self {
        self.manifest = Some(manifest);
        self
    }

    /// The observed facts about the location, in words; unobserved ones are absent.
    fn facts(&self) -> Vec<String> {
        let git = self
            .git
            .map(|g| if g { "git" } else { "no git" }.to_owned());
        git.into_iter()
            .chain(self.manifest.as_ref().map(Manifest::words))
            .collect()
    }

    /// What follows the name: the kind of place a project is (`local
    /// project`), or the host alone where no project is active.
    fn kind(&self, sep: &str) -> String {
        match (&self.project, self.host.is_empty()) {
            (Some(_), true) => "  project".to_owned(),
            (Some(_), false) => format!("  {} project", self.host),
            (None, true) => String::new(),
            (None, false) => format!("{sep}{}", self.host),
        }
    }
}

/// The header lines for a `width`-wide header of `rows` rows (one or two):
/// the identity first; on one row the location and the facts follow it, on
/// two rows they stand on the second.
#[must_use]
pub fn lines(place: &Place, width: u16, rows: u16, ascii: bool, color: bool) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let (sep, cut) = marks(ascii);
    let location = place.location.as_deref().unwrap_or_default();
    let facts = place.facts();
    let refused = place.manifest == Some(Manifest::Refused);
    let mut first = identity(place, width, ascii, color);
    if rows >= 2 {
        let second = trail("  ", location, &facts, (sep, cut), width);
        return vec![first, Line::from(tinted(second, refused, color))];
    }
    let room = width.saturating_sub(first.width());
    let tail = trail(sep, location, &facts, (sep, cut), room);
    first.spans.extend(tinted(tail, refused, color));
    vec![first]
}

/// The identity row in at most `width` cells: the project's glyph, its name
/// (strong; `no project` quiet) and what it is. What it is gives way first;
/// the name is cut only when it alone is wider than the row.
fn identity(place: &Place, width: usize, ascii: bool, color: bool) -> Line<'static> {
    let (sep, cut) = marks(ascii);
    let glyph = fit_head(&format!("{} ", Icon::Project.glyph(ascii)), width, "");
    let (name, style) = match &place.project {
        Some(name) => (name.as_str(), role::style(Role::Strong, color)),
        None => ("no project", role::style(Role::Dim, color)),
    };
    let name = fit_head(name, width - glyph.width(), cut);
    let kind = place.kind(sep);
    let fits = glyph.width() + name.width() + kind.width() <= width;
    let mut spans = vec![
        Span::styled(glyph, role::style(Role::Accent, color)),
        Span::styled(name, style),
    ];
    if fits {
        spans.push(Span::styled(kind, role::style(Role::Dim, color)));
    }
    Line::from(spans)
}

/// The location and the observed facts after `lead`, in at most `room`
/// cells: whole when they fit; else the location cut from its start while
/// at least [`MIN_LOCATION`] cells of it remain; else the facts alone, the
/// Git fact going before the governing file's words. Empty when nothing fits.
fn trail(
    lead: &str,
    location: &str,
    facts: &[String],
    (sep, cut): (&str, &str),
    room: usize,
) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if !location.is_empty() {
        parts.push(location);
    }
    parts.extend(facts.iter().map(String::as_str));
    if parts.is_empty() {
        return String::new();
    }
    let whole = format!("{lead}{}", parts.join(sep));
    if whole.width() <= room {
        return whole;
    }
    let after = if facts.is_empty() {
        String::new()
    } else {
        format!("{sep}{}", facts.join(sep))
    };
    let budget = room.saturating_sub(lead.width() + after.width());
    if !location.is_empty() && budget >= MIN_LOCATION {
        return format!("{lead}{}{after}", fit_tail(location, budget, 0, cut));
    }
    (0..facts.len())
        .map(|first| format!("{lead}{}", facts[first..].join(sep)))
        .find(|kept| kept.width() <= room)
        .unwrap_or_default()
}

/// `trail` in the quiet role, a refused `nika.yaml` at its end in the
/// warning role: a hue beside the words, never instead of them.
fn tinted(trail: String, refused: bool, color: bool) -> Vec<Span<'static>> {
    let dim = role::style(Role::Dim, color);
    if refused && let Some(before) = trail.strip_suffix(REFUSED) {
        return vec![
            Span::styled(before.to_owned(), dim),
            Span::styled(REFUSED, role::style(Role::Warn, color)),
        ];
    }
    vec![Span::styled(trail, dim)]
}

/// Draw the header into `area`.
pub fn render(place: &Place, area: Rect, buf: &mut Buffer, ascii: bool, color: bool) {
    Paragraph::new(lines(place, area.width, area.height, ascii, color)).render(area, buf);
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

    /// One row: the project's name is its one strong word; what it is and
    /// where it lives follow in a quieter voice, then the observed facts.
    #[test]
    fn one_row_names_the_project_once_then_its_kind_location_and_facts() {
        assert_eq!(
            text(&lines(&studio(), 80, 1, false, false)),
            ["▱ studio  local project · ~/Projects/studio · git · no nika.yaml"]
        );
    }

    #[test]
    fn two_rows_keep_the_identity_alone_above_its_location_and_facts() {
        assert_eq!(
            text(&lines(&studio(), 120, 2, false, false)),
            [
                "▱ studio  local project",
                "  ~/Projects/studio · git · no nika.yaml"
            ]
        );
    }

    #[test]
    fn unobserved_facts_are_never_invented_and_no_project_is_said() {
        let place = Place::on("local").with_project("veille", "~/veille");
        assert_eq!(
            text(&lines(&place, 80, 1, false, false)),
            ["▱ veille  local project · ~/veille"]
        );
        let none = Place::on("local");
        assert_eq!(
            text(&lines(&none, 80, 1, false, false)),
            ["▱ no project · local"]
        );
        assert_eq!(
            text(&lines(&none, 80, 2, false, false)),
            ["▱ no project · local", ""]
        );
    }

    /// A narrow row gives way from the location's start, then drops the
    /// location, then the Git fact: the governing file's words go last, and
    /// the project's name is never cut while anything else is shown.
    #[test]
    fn a_narrow_row_gives_way_from_the_location_never_the_name() {
        let place = Place::on("local")
            .with_project("studio", "~/Projects/clients/acme/campaigns/2026/studio")
            .observed(true, true);
        let row = |width| text(&lines(&place, width, 1, false, false));
        assert_eq!(
            row(60),
            ["▱ studio  local project · …gns/2026/studio · git · nika.yaml"]
        );
        assert_eq!(row(48), ["▱ studio  local project · git · nika.yaml"]);
        assert_eq!(row(40), ["▱ studio  local project · nika.yaml"]);
        assert_eq!(
            text(&lines(&place, 30, 2, false, false)),
            ["▱ studio  local project", "  …26/studio · git · nika.yaml"]
        );
    }

    #[test]
    fn the_ascii_column_replaces_every_glyph_and_separator() {
        assert_eq!(
            text(&lines(&studio(), 80, 1, true, false)),
            ["[P] studio  local project - ~/Projects/studio - git - no nika.yaml"]
        );
        let place =
            Place::on("local").with_project("studio", "~/a/very/long/location/for/a/narrow/row");
        assert_eq!(
            text(&lines(&place, 40, 1, true, false)),
            ["[P] studio  local project - ...arrow/row"]
        );
    }

    /// A parent's `nika.yaml` is named by its path, a refused one says so in
    /// the warning role beside its words, and a place with no host names none.
    #[test]
    fn the_governing_file_is_named_where_it_is_and_a_refusal_is_said() {
        let above = Place::on("local")
            .with_project("one", "~/repo/ventures/one")
            .governed(Manifest::Above("../../nika.yaml".to_owned()));
        assert_eq!(
            text(&lines(&above, 80, 1, false, false)),
            ["▱ one  local project · ~/repo/ventures/one · ../../nika.yaml"]
        );
        let refused = Place::on("local")
            .with_project("one", "~/one")
            .observed(false, true)
            .governed(Manifest::Refused);
        assert_eq!(
            text(&lines(&refused, 80, 1, true, false)),
            ["[P] one  local project - ~/one - no git - nika.yaml refused"]
        );
        let colored = lines(&refused, 80, 1, false, true);
        let warning = colored.first().and_then(|line| {
            line.spans
                .iter()
                .find(|span| span.content == "nika.yaml refused")
                .map(|span| span.style)
        });
        assert_eq!(warning, Some(role::style(Role::Warn, true)));
        let hostless = Place::on("");
        assert_eq!(
            text(&lines(&hostless, 80, 2, false, false)),
            ["▱ no project", ""]
        );
        assert_eq!(
            text(&lines(&hostless, 80, 1, false, false)),
            ["▱ no project"]
        );
    }

    #[test]
    fn colour_off_keeps_the_project_name_strong_and_an_absence_quiet() {
        let named = lines(&studio(), 80, 1, false, false);
        let name = &named[0].spans[1];
        assert_eq!(name.content, "studio");
        assert_eq!(name.style, role::style(Role::Strong, false));
        let none = lines(&Place::on("local"), 80, 1, false, false);
        let absence = &none[0].spans[1];
        assert_eq!(absence.content, "no project");
        assert_eq!(absence.style, role::style(Role::Dim, false));
    }

    /// No chevron offers a project menu the workspace does not open; every
    /// row keeps within its width in both glyph columns, and the name is cut
    /// only where nothing else is shown.
    #[test]
    fn every_row_fits_and_no_dead_selector_is_drawn() {
        let long = "a-project-name-far-longer-than-any-header-row";
        let places = [
            studio(),
            Place::on("local")
                .with_project(long, format!("~/work/{long}"))
                .observed(false, true),
            Place::on("serveur")
                .with_project("日本語のプロジェクト", "~/仕事/日本語のプロジェクト")
                .governed(Manifest::Refused),
            Place::on("local"),
            Place::on(""),
        ];
        for place in &places {
            for ascii in [false, true] {
                let (sep, _) = marks(ascii);
                let whole = place
                    .project
                    .as_ref()
                    .map(|name| format!("{} {name}", Icon::Project.glyph(ascii)));
                for rows in [1, 2] {
                    for width in 0..=100_u16 {
                        let shown = lines(place, width, rows, ascii, false);
                        for line in &shown {
                            let row: String =
                                line.spans.iter().map(|s| s.content.as_ref()).collect();
                            assert!(row.width() <= usize::from(width), "{width}: {row}");
                            assert!(
                                line.spans
                                    .iter()
                                    .all(|s| s.content.trim() != Icon::Choice.glyph(ascii)),
                                "{row}"
                            );
                            if ascii {
                                assert!(!row.contains(['▱', '·', '…', '⌄']), "{row}");
                            }
                        }
                        let first = text(&shown).into_iter().next().unwrap_or_default();
                        if let Some(whole) = &whole
                            && !first.starts_with(whole.as_str())
                        {
                            assert!(
                                !first.contains(" project") && !first.contains(sep),
                                "{width}: {first}"
                            );
                        }
                    }
                }
            }
        }
    }
}
