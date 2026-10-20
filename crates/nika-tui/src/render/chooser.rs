// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The chooser's words around its surface (`super::surface` paints its list
//! above the line it fills): what waits, when something does (whatever is
//! chosen, none of it answers), the hint row's keys in a form that fits the
//! row, the palette's search in the composer's own row, the draft waiting out
//! of view, and a draft the palette set aside, named on a row of its own
//! until it returns.

use nika_display::theme::Role;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::own;
use crate::composer::Composer;
use crate::composer::chooser::{Door, Listing};
use crate::lang::Voice;
use crate::model::{UiState, Waiting};
use crate::visual::role;

/// The prompt of the palette's search, in place of the waiting prompt.
pub(crate) const SEARCH_PROMPT: &str = "commands › ";

/// What waits, said above the list when something does; `None` at a free
/// prompt. Every claim is the Session's own law: a command-shaped line is
/// never read as an answer, a consent or a gate's answer, and while Nika
/// works a command waits in the box (it is never a correction).
pub(crate) fn context(state: &UiState) -> Option<&'static str> {
    let voice = state.voice();
    waits(state).map(|words| voice.say(words))
}

/// What waits, in English ([`context`]).
fn waits(state: &UiState) -> Option<&'static str> {
    if state.busy.is_some() {
        return Some("Nika is working · a command waits in the box until your turn");
    }
    match &state.waiting {
        Waiting::Free => None,
        Waiting::Action { .. } => Some("A decision waits · choose an action with the arrow keys"),
        Waiting::Proposal => Some("A proposal waits · no command answers it · yes + Enter: Save"),
        Waiting::Gate => Some("A gate waits · no command answers it · approve or refuse"),
        Waiting::Choosing | Waiting::Selection { .. } => {
            Some("The intelligence choice waits · no command answers it")
        }
        Waiting::Knowledge { .. } => {
            Some("Your message waits · /knowledge embedded resumes it · cancel drops it")
        }
        Waiting::Question { key } | Waiting::QuestionDocument { key, .. }
            if key == "unknown_cost" || key == "run_cost" =>
        {
            Some("A cost decision waits · no command approves it · only your yes does")
        }
        Waiting::Question { .. }
        | Waiting::QuestionDocument { .. }
        | Waiting::ProposalChoice { .. } => {
            Some("A question waits · no command answers it · your reply does")
        }
    }
}

/// The hint row while the chooser shows, in the longest form a row `width`
/// cells wide holds (the row keeps its height at rest): how to choose, that
/// `Enter` runs the selected command once and `Tab` only inserts it, or,
/// while Nika works, that the command waits in the box for the human's turn.
pub(crate) fn hint(listing: &Listing<'_>, busy: bool, voice: Voice, width: u16) -> String {
    let ascii = voice.ascii;
    let say = |en: &'static str| voice.say(en).to_owned();
    // A command's name goes in as typed, after the words are said.
    let named = |en: &'static str, name: &str| voice.say(en).replace("{name}", name);
    let forms = match (listing.door, listing.current()) {
        (Door::Palette, _) if busy => vec![
            say("↑↓ choose · Enter keeps it for your turn · Esc: back to your draft"),
            say("Enter keeps it for your turn · Esc: your draft"),
        ],
        (Door::Palette, _) => vec![
            say("↑↓ choose · Enter runs · Tab inserts · Esc: back to your draft"),
            say("↑↓ choose · Enter runs · Tab inserts · Esc"),
            say("Enter runs · Tab inserts · Esc"),
        ],
        (Door::Slash, Some(entry)) if listing.whole && busy => vec![
            named(
                "{name} waits for your turn · ↑↓ choose · Esc hides the list",
                &entry.name,
            ),
            named("{name} waits for your turn · Esc hides", &entry.name),
        ],
        (Door::Slash, Some(entry)) if listing.whole => vec![
            named(
                "Enter sends {name} · ↑↓ choose · Esc hides the list",
                &entry.name,
            ),
            named("Enter sends {name} · Esc hides", &entry.name),
        ],
        (Door::Slash, _) if busy => vec![
            say("↑↓ choose · Enter keeps it for your turn · Esc hides the list"),
            say("Enter keeps it for your turn · Esc hides"),
        ],
        (Door::Slash, _) => vec![
            say("↑↓ choose · Enter runs · Tab inserts · Esc hides the list"),
            say("Enter runs · Tab inserts · Esc hides"),
        ],
    };
    let shown: Vec<String> = (forms.iter())
        .map(|form| arrows(&own(form, ascii), ascii))
        .collect();
    let last = shown.last().cloned().unwrap_or_default();
    (shown.into_iter())
        .find(|form| form.width() <= usize::from(width))
        .unwrap_or(last)
}

/// The renderer's arrows in the glyph column in use.
fn arrows(text: &str, ascii: bool) -> String {
    if ascii {
        text.replace("↑↓", "Up/Down")
    } else {
        text.to_owned()
    }
}

/// `text` cut to `width` cells, the cut marked.
pub(super) fn fit(text: &str, width: usize, ascii: bool) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let cut = if ascii { "..." } else { "…" };
    let room = width.saturating_sub(cut.width());
    let mut kept = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let cells = ch.width().unwrap_or(0);
        if used + cells > room {
            break;
        }
        used += cells;
        kept.push(ch);
    }
    kept.push_str(cut);
    kept
}

/// The palette's search row: what was typed, then the cursor.
pub(crate) fn search_line(composer: &Composer, color: bool) -> Line<'static> {
    let query = composer
        .listing()
        .and_then(|listing| listing.query.map(str::to_owned))
        .unwrap_or_default();
    Line::from(vec![
        Span::styled(query, role::style(Role::Strong, color)),
        Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)),
    ])
}

/// The row naming a draft the palette set aside: its start, and how it
/// comes back.
pub(crate) fn aside_line(aside: &str, width: u16, voice: Voice, color: bool) -> Line<'static> {
    const SHOWN_CHARS: usize = 24;
    let ascii = voice.ascii;
    let words = aside.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut start: String = words.chars().take(SHOWN_CHARS).collect();
    let (open, close, cut) = if ascii {
        ("\"", "\"", "...")
    } else {
        ("« ", " »", "…")
    };
    if words.chars().count() > SHOWN_CHARS {
        start = format!("{}{cut}", start.trim_end());
    }
    // The person's words go in last, as typed.
    let words =
        voice.say("set aside: {open}{start}{close} · back once this line is sent · Esc: now");
    let text = own(
        &(words.replace("{open}", open).replace("{close}", close)).replace("{start}", &start),
        ascii,
    );
    Line::styled(
        fit(&text, usize::from(width), ascii),
        role::style(Role::Dim, color),
    )
}
