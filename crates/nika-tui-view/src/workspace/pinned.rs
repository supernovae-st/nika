// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pinned activity row: the run that asks for attention, named by its own
//! identity (its owning project, its workflow, its run), its state as the
//! theme's glyph with the Session's words beside it, and the one useful action
//! when the Session offers one. The row leads with the pinned icon (`⌖`, `^`
//! in ASCII), never a `>` a reader could take for a prompt marker. Pinning
//! never moves a run nor changes the revision it runs; the row keeps naming
//! the owning project while another project is in view. When the row is narrow
//! the action goes first, then the workflow's end; the run and its state words
//! stay.

use nika_display::state::TaskState;
use nika_display::theme::Role;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::text::{fit_head, marks};
use crate::visual::icon::Icon;
use crate::visual::{role, state};

/// A run pinned in view, as the Session projects it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Pinned {
    /// The project that owns the run.
    pub project: String,
    /// The workflow the run executes.
    pub workflow: String,
    /// The run as shown (`#043`).
    pub run: String,
    /// The run's state.
    pub state: TaskState,
    /// The state in the Session's words (`waiting for your approval`).
    pub words: String,
    /// The one useful action the Session offers, if any.
    pub action: Option<String>,
}

impl Pinned {
    /// A pinned run with no action offered.
    #[must_use]
    pub fn new(
        project: impl Into<String>,
        workflow: impl Into<String>,
        run: impl Into<String>,
        state: TaskState,
        words: impl Into<String>,
    ) -> Self {
        Self {
            project: project.into(),
            workflow: workflow.into(),
            run: run.into(),
            state,
            words: words.into(),
            action: None,
        }
    }

    /// This pinned run with the action the Session offers.
    #[must_use]
    pub fn offering(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }
}

/// The pinned row fitted to `width` cells.
#[must_use]
pub fn line(pinned: &Pinned, width: u16, ascii: bool, color: bool) -> Line<'static> {
    let width = usize::from(width);
    let (sep, cut) = marks(ascii);
    let dim = role::style(Role::Dim, color);
    let (glyph, state_role) = state::cell(pinned.state, ascii);
    let head = format!("{} {} ", Icon::Pinned.glyph(ascii), pinned.project);
    let run = format!(" {}", pinned.run);
    let status = format!("{sep}{glyph} {}", pinned.words);
    let action = pinned
        .action
        .as_ref()
        .map(|a| format!("{sep}{a}"))
        .unwrap_or_default();
    let fixed = head.width() + run.width() + status.width();
    let mut workflow = format!("/ {}", pinned.workflow);
    let mut action_shown = action.clone();
    if fixed + workflow.width() + action_shown.width() > width {
        action_shown.clear();
    }
    if fixed + workflow.width() > width {
        workflow = fit_head(&workflow, width.saturating_sub(fixed), cut);
    }
    Line::from(vec![
        Span::styled(head, dim),
        Span::styled(workflow, dim),
        Span::styled(run, role::style(Role::Strong, color)),
        Span::styled(status, role::style(state_role, color)),
        Span::styled(action_shown, dim),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn gate() -> Pinned {
        Pinned::new(
            "studio",
            "release.nika",
            "#043",
            TaskState::Paused,
            "waiting for your approval",
        )
        .offering("answer the gate")
    }

    #[test]
    fn the_row_names_the_owning_project_the_run_its_state_and_the_action() {
        // The pinned icon leads, never a `>` that reads as a prompt marker.
        let row = text(&line(&gate(), 120, false, false));
        assert_eq!(
            row,
            "⌖ studio / release.nika #043 · ◇ waiting for your approval · answer the gate"
        );
    }

    #[test]
    fn a_narrow_row_drops_the_action_then_cuts_the_workflow_never_the_run() {
        let row = text(&line(&gate(), 60, false, false));
        assert!(!row.contains("answer the gate"), "{row}");
        assert!(
            row.contains("#043") && row.contains("waiting for your approval"),
            "{row}"
        );
        let tight = text(&line(&gate(), 52, false, false));
        assert!(tight.contains('…') && tight.contains("#043"), "{tight}");
        assert!(tight.width() <= 52, "{tight}");
    }

    #[test]
    fn the_state_wears_its_theme_role_and_ascii_twin() {
        let row = line(&gate(), 120, false, true);
        assert_eq!(row.spans[3].style, role::style(Role::Warn, true));
        let ascii = text(&line(&gate(), 120, true, false));
        assert_eq!(
            ascii,
            "^ studio / release.nika #043 - ? waiting for your approval - answer the gate"
        );
        let failed = Pinned::new(
            "studio",
            "enrich.nika",
            "#044",
            TaskState::Failed,
            "failed at summarise",
        );
        assert_eq!(
            line(&failed, 120, false, true).spans[3].style,
            role::style(Role::Bad, true)
        );
    }
}
