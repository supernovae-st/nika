// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation panel's own rows: who the next message goes to and with
//! what. The title names the thread and its project; the composer repeats the
//! full recipient as its placeholder; the context row keeps apart what is
//! only on screen (consulted, never sent) and what is attached to the next
//! message (joined explicitly). Opening an object changes what is on screen,
//! never the thread, and never attaches it.

use nika_display::theme::Role;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::text::{fit_head, marks};
use crate::visual::icon::Icon;
use crate::visual::role;

/// The conversation the composer writes to, as the Session projects it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Thread {
    /// The project that owns the conversation.
    pub project: String,
    /// The conversation's name.
    pub name: String,
    /// The object in view, consulted and not attached.
    pub on_screen: Option<String>,
    /// The references attached to the next message, in the order joined.
    pub attached: Vec<String>,
}

impl Thread {
    /// The conversation `name` of `project`, with nothing on screen or attached.
    #[must_use]
    pub fn new(project: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            name: name.into(),
            on_screen: None,
            attached: Vec::new(),
        }
    }

    /// This conversation with `object` in view.
    #[must_use]
    pub fn viewing(mut self, object: impl Into<String>) -> Self {
        self.on_screen = Some(object.into());
        self
    }

    /// This conversation with `reference` attached to the next message.
    #[must_use]
    pub fn attaching(mut self, reference: impl Into<String>) -> Self {
        self.attached.push(reference.into());
        self
    }
}

/// The composer's placeholder: the full recipient of the next message (the
/// thread alone when no project is known).
#[must_use]
pub fn placeholder(thread: &Thread) -> String {
    if thread.project.is_empty() {
        format!("Message to {}", thread.name)
    } else {
        format!("Message to {} / {}", thread.project, thread.name)
    }
}

/// The title row, `width` cells: the thread, then its project. A narrow row
/// cuts the thread's name before the project. As a `rule` (the panel under the
/// object on a narrow terminal) the row is drawn across as a separator.
#[must_use]
pub fn title(thread: &Thread, width: u16, ascii: bool, color: bool, rule: bool) -> Line<'static> {
    let width = usize::from(width);
    let (sep, cut) = marks(ascii);
    let dim = role::style(Role::Dim, color);
    let line = if ascii { "-" } else { "─" };
    let lead = if rule {
        format!("{line}{line} ")
    } else {
        String::new()
    };
    let head = format!("{lead}{} ", Icon::Conversation.glyph(ascii));
    let project = if thread.project.is_empty() {
        String::new()
    } else {
        format!("{sep}{}", thread.project)
    };
    let room = width.saturating_sub(head.width() + project.width());
    let name = fit_head(&thread.name, room, cut);
    let project = fit_head(
        &project,
        width.saturating_sub(head.width() + name.width()),
        cut,
    );
    let used = head.width() + name.width() + project.width();
    let tail = if rule && used < width {
        format!(" {}", line.repeat(width - used - 1))
    } else {
        String::new()
    };
    Line::from(vec![
        Span::styled(head, role::style(Role::Accent, color)),
        Span::styled(
            name,
            role::style(Role::Accent, color).patch(role::style(Role::Strong, color)),
        ),
        Span::styled(project, dim),
        Span::styled(tail, dim),
    ])
}

/// The narrowest on-screen part worth keeping: its label and a few characters.
const MIN_SEEN: usize = 16;

/// The context row, `width` cells: what is on screen, then what is attached,
/// or that nothing is. What the next message carries keeps priority: on a
/// narrow panel the on-screen part is cut first, then dropped.
#[must_use]
pub fn context(thread: &Thread, width: u16, ascii: bool, color: bool) -> Line<'static> {
    let width = usize::from(width);
    let (sep, cut) = marks(ascii);
    let sent = if thread.attached.is_empty() {
        "nothing attached".to_owned()
    } else {
        format!("with {}", thread.attached.join(", "))
    };
    let text = match &thread.on_screen {
        Some(object) => {
            let seen = format!("on screen: {object}");
            let room = width.saturating_sub(sep.width() + sent.width());
            if seen.width() <= room {
                format!("{seen}{sep}{sent}")
            } else if room >= MIN_SEEN {
                format!("{}{sep}{sent}", fit_head(&seen, room, cut))
            } else {
                sent
            }
        }
        None => sent,
    };
    Line::from(Span::styled(
        fit_head(&text, width, cut),
        role::style(Role::Dim, color),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn release() -> Thread {
        Thread::new("studio", "release checklist").viewing("release.nika")
    }

    #[test]
    fn the_panel_names_the_thread_its_project_and_the_full_recipient() {
        assert_eq!(
            text(&title(&release(), 60, false, false, false)),
            "◌ release checklist · studio"
        );
        assert_eq!(
            placeholder(&release()),
            "Message to studio / release checklist"
        );
        let narrow = text(&title(&release(), 20, false, false, false));
        assert_eq!(narrow, "◌ release … · studio");
        assert!(narrow.width() <= 20);
    }

    /// With no project known, the thread alone is the recipient.
    #[test]
    fn a_thread_outside_any_project_is_named_alone() {
        let bare = Thread::new("", "this conversation");
        assert_eq!(placeholder(&bare), "Message to this conversation");
        assert_eq!(
            text(&title(&bare, 40, false, false, false)),
            "◌ this conversation"
        );
    }

    #[test]
    fn under_the_object_the_title_is_a_rule_across_the_panel() {
        let row = text(&title(&release(), 40, true, false, true));
        assert_eq!(row, "-- [C] release checklist - studio ------");
        assert_eq!(row.width(), 40);
        let unicode = text(&title(&release(), 40, false, false, true));
        assert!(unicode.starts_with("── ◌ release checklist · studio ─"));
        assert!(unicode.width() <= 40);
    }

    #[test]
    fn what_is_on_screen_is_never_shown_as_attached() {
        assert_eq!(
            text(&context(&release(), 80, false, false)),
            "on screen: release.nika · nothing attached"
        );
        let joined = release().attaching("notes.md").attaching("orders.csv");
        assert_eq!(
            text(&context(&joined, 80, true, false)),
            "on screen: release.nika - with notes.md, orders.csv"
        );
        let bare = Thread::new("studio", "weekly report");
        assert_eq!(text(&context(&bare, 80, false, false)), "nothing attached");
        assert!(text(&context(&joined, 24, false, false)).width() <= 24);
    }

    #[test]
    fn a_narrow_row_cuts_what_is_on_screen_before_what_is_sent() {
        assert_eq!(
            text(&context(&release(), 36, false, false)),
            "on screen: relea… · nothing attached"
        );
        assert_eq!(
            text(&context(&release(), 30, false, false)),
            "nothing attached"
        );
        let joined = release().attaching("orders.csv");
        assert_eq!(text(&context(&joined, 20, true, false)), "with orders.csv");
    }
}
