// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation panel's own rows: who the next message goes to and with
//! what. The title names the thread by its own name: the header is the one
//! home of the project, so the title does not repeat it, and the composer's
//! placeholder ([`placeholder`]) names the full recipient. The empty composer
//! invites the work and names the door to the commands ([`invitation`]). The
//! context row says what is attached to the next message (joined
//! explicitly), then apart and quieter what is only on screen (consulted,
//! never sent); with nothing attached it stays silent, the object's own title
//! naming what is on screen. Opening an object changes what is on screen,
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
    /// The selected intelligence, as observed by Session.
    pub intelligence: Option<String>,
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
            intelligence: None,
        }
    }

    /// The selected model and access, without changing this conversation's recipient.
    #[must_use]
    pub fn seated(mut self, intelligence: Option<String>) -> Self {
        self.intelligence = intelligence;
        self
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

/// What the empty composer of the workspace shows, in the glyph column in
/// use: the work it takes and the door to the commands (`/` opens their list
/// under the line), the door kept visible down to a 24-cell composer. The
/// recipient is the panel's title row; the palette key is the hint row's.
#[must_use]
pub fn invitation(ascii: bool) -> &'static str {
    if ascii {
        "Ask, change, or run... / commands"
    } else {
        "Ask, change, or run… / commands"
    }
}

/// The full recipient of the next message (the thread alone when no project
/// is known), in words a placeholder can carry.
#[must_use]
pub fn placeholder(thread: &Thread) -> String {
    if thread.project.is_empty() {
        format!("Message to {}", thread.name)
    } else {
        format!("Message to {} / {}", thread.project, thread.name)
    }
}

/// The title row, `width` cells: the conversation's glyph, then the thread's
/// own name, cut at its end on a narrow row. As a `rule` (the panel under the
/// object on a narrow terminal) the row is drawn across as a separator.
#[must_use]
pub fn title(thread: &Thread, width: u16, ascii: bool, color: bool, rule: bool) -> Line<'static> {
    let width = usize::from(width);
    let (_, cut) = marks(ascii);
    let line = if ascii { "-" } else { "─" };
    let lead = if rule {
        format!("{line}{line} ")
    } else {
        String::new()
    };
    let head = fit_head(
        &format!("{lead}{} ", Icon::Conversation.glyph(ascii)),
        width,
        "",
    );
    let name = fit_head(&thread.name, width - head.width(), cut);
    let used = head.width() + name.width();
    let tail = if rule && used < width {
        format!(" {}", line.repeat(width - used - 1))
    } else {
        String::new()
    };
    Line::from(vec![
        Span::styled(head, role::style(Role::Accent, color)),
        Span::styled(name, role::style(Role::Strong, color)),
        Span::styled(tail, role::style(Role::Dim, color)),
    ])
}

/// The word the context row puts before what is attached.
const ATTACHED: &str = "Attached: ";

/// The narrowest viewed part worth keeping after what is attached: the
/// separator, its word and a few characters of the object.
const MIN_SEEN: usize = 16;

/// The narrowest cut reference worth keeping, its cut mark included.
const MIN_REFERENCE: usize = 4;

/// The context row, `width` cells: what the next message carries, strong,
/// then apart and quieter the object in view, never counted as attached.
/// With nothing attached the row is empty. What is attached keeps priority:
/// on a narrow panel the viewed part is cut first, then dropped, then whole
/// references give way to how many more there are.
#[must_use]
pub fn context(thread: &Thread, width: u16, ascii: bool, color: bool) -> Line<'static> {
    if thread.attached.is_empty() {
        return Line::default();
    }
    let width = usize::from(width);
    let (sep, cut) = marks(ascii);
    let (sent, whole) = attached(&thread.attached, width, cut);
    let room = width.saturating_sub(sent.width());
    let seen = thread
        .on_screen
        .as_ref()
        .filter(|object| whole && !thread.attached.contains(object))
        .map(|object| format!("{sep}viewing {object}"))
        .filter(|seen| seen.width() <= room || room >= MIN_SEEN);
    let dim = role::style(Role::Dim, color);
    let mut spans = vec![Span::styled(sent, role::style(Role::Strong, color))];
    spans.extend(seen.map(|seen| Span::styled(fit_head(&seen, room, cut), dim)));
    Line::from(spans)
}

/// The references the next message carries, in at most `width` cells, and
/// whether every one is shown whole: as many whole as fit and how many more,
/// else the first one cut and how many more, else their count alone.
fn attached(references: &[String], width: usize, cut: &str) -> (String, bool) {
    let count = references.len();
    let more = |shown: usize| {
        if shown < count {
            format!(" +{}", count - shown)
        } else {
            String::new()
        }
    };
    for shown in (1..=count).rev() {
        let listed = references[..shown].join(", ");
        let words = format!("{ATTACHED}{listed}{}", more(shown));
        if words.width() <= width {
            return (words, shown == count);
        }
    }
    let room = width.saturating_sub(ATTACHED.width() + more(1).width());
    match references.first() {
        Some(first) if room >= MIN_REFERENCE => (
            format!("{ATTACHED}{}{}", fit_head(first, room, cut), more(1)),
            false,
        ),
        _ => (fit_head(&format!("{count} attached"), width, cut), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::aside::{self, Aside, Entry, Tab};
    use crate::workspace::header::{self, Place};

    fn text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn release() -> Thread {
        Thread::new("studio", "release checklist").viewing("release.nika")
    }

    /// The title names the thread alone, the header being the project's one
    /// home; the placeholder still names the full recipient, and no width
    /// spills the title or brings the project back into it.
    #[test]
    fn the_title_names_the_thread_alone_and_the_placeholder_the_recipient() {
        assert_eq!(
            text(&title(&release(), 60, false, false, false)),
            "◌ release checklist"
        );
        assert_eq!(
            placeholder(&release()),
            "Message to studio / release checklist"
        );
        assert_eq!(
            text(&title(&release(), 12, false, false, false)),
            "◌ release c…"
        );
        for width in 0..=60 {
            for (ascii, rule) in [(false, false), (false, true), (true, false), (true, true)] {
                let row = text(&title(&release(), width, ascii, false, rule));
                assert!(row.width() <= usize::from(width), "{width}: {row}");
                assert!(!row.contains("studio"), "{row}");
            }
        }
    }

    /// The thread's name is the panel's strong word; its glyph keeps the
    /// accent, with and without colour.
    #[test]
    fn the_thread_name_is_strong_and_its_glyph_accented() {
        for color in [false, true] {
            let row = title(&release(), 40, false, color, false);
            let name = row
                .spans
                .iter()
                .find(|span| span.content == "release checklist")
                .map(|span| span.style);
            assert_eq!(name, Some(role::style(Role::Strong, color)));
            assert_eq!(
                row.spans.first().map(|span| span.style),
                Some(role::style(Role::Accent, color))
            );
        }
    }

    /// One home per fact: the header names the project; neither the aside's
    /// first rows nor the conversation's title repeat it.
    #[test]
    fn the_project_has_one_home_among_the_primary_rows() {
        let place = Place::on("local")
            .with_project("studio", "~/Projects/studio")
            .observed(true, true);
        let listing = Aside::new(
            "studio",
            Tab::Nika,
            vec![Entry::new(Icon::Conversation, "release checklist")],
            true,
        );
        let thread = Thread::new("studio", "release checklist");
        for ascii in [false, true] {
            let head = header::lines(&place, 80, 1, ascii, false)
                .first()
                .map(text)
                .unwrap_or_default();
            assert!(head.contains("studio"), "{head}");
            let top: Vec<String> = aside::lines(&listing, 24, 10, ascii, false)
                .iter()
                .take(2)
                .map(text)
                .collect();
            assert!(top.iter().all(|row| !row.contains("studio")), "{top:?}");
            let heading = text(&title(&thread, 40, ascii, false, false));
            assert!(heading.contains("release checklist"), "{heading}");
            assert!(!heading.contains("studio"), "{heading}");
        }
    }

    /// The empty box invites the work and the door to the commands, the door
    /// readable in a 24-cell composer, ASCII in the ASCII column; it does not
    /// repeat the recipient the title row names.
    #[test]
    fn the_empty_box_invites_the_work_and_the_slash_door() {
        for ascii in [false, true] {
            let words = invitation(ascii);
            assert!(words.starts_with("Ask, change, or run"), "{words}");
            let door = words.find('/').map(|at| words[..at].width());
            assert!(door.is_some_and(|at| at < 24), "{words}");
            assert!(words.width() <= 37, "{words}");
            assert!(!words.contains("Message to"), "{words}");
            assert_eq!(words.is_ascii(), ascii, "{words}");
        }
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
        assert_eq!(row, "-- [C] release checklist ---------------");
        assert_eq!(row.width(), 40);
        let unicode = text(&title(&release(), 40, false, false, true));
        assert!(unicode.starts_with("── ◌ release checklist ─"), "{unicode}");
        assert_eq!(unicode.width(), 40);
    }

    /// With nothing attached the row is silent whatever is in view (the
    /// object's title names it). Attached references are said, strong; what
    /// is only viewed follows apart and dim, never among them.
    #[test]
    fn an_empty_attachment_is_silent_and_what_is_viewed_is_never_attached() {
        assert_eq!(text(&context(&release(), 80, false, false)), "");
        let bare = Thread::new("studio", "weekly report");
        assert_eq!(text(&context(&bare, 80, true, false)), "");
        let joined = release().attaching("notes.md").attaching("orders.csv");
        assert_eq!(
            text(&context(&joined, 80, false, false)),
            "Attached: notes.md, orders.csv · viewing release.nika"
        );
        assert_eq!(
            text(&context(&joined, 80, true, false)),
            "Attached: notes.md, orders.csv - viewing release.nika"
        );
        let row = context(&joined, 80, false, false);
        let style = |words: &str| {
            row.spans
                .iter()
                .find(|span| span.content.contains(words))
                .map(|span| span.style)
        };
        assert_eq!(style("Attached"), Some(role::style(Role::Strong, false)));
        assert_eq!(style("viewing"), Some(role::style(Role::Dim, false)));
        // The object in view, attached explicitly, is listed once, as attached.
        let both = Thread::new("studio", "notes")
            .viewing("notes.md")
            .attaching("notes.md");
        assert_eq!(
            text(&context(&both, 80, false, false)),
            "Attached: notes.md"
        );
    }

    /// Under elision the attached references keep the row: what is viewed
    /// goes first, then whole references give way to a count, never to the
    /// object in view.
    #[test]
    fn attachments_survive_elision_and_the_viewed_object_goes_first() {
        let joined = release().attaching("notes.md").attaching("orders.csv");
        let row = |width| text(&context(&joined, width, false, false));
        assert_eq!(row(30), "Attached: notes.md, orders.csv");
        assert_eq!(row(24), "Attached: notes.md +1");
        assert_eq!(row(17), "Attached: not… +1");
        assert_eq!(row(14), "2 attached");
        for width in 0..=70 {
            for ascii in [false, true] {
                let shown = text(&context(&joined, width, ascii, false));
                let (sep, _) = marks(ascii);
                let sent = shown.split(sep).next().unwrap_or_default();
                assert!(shown.width() <= usize::from(width), "{width}: {shown}");
                assert!(!sent.contains("release"), "{width}: {shown}");
                if width >= 10 {
                    assert!(shown.contains("ttached"), "{width}: {shown}");
                }
            }
        }
    }
}
