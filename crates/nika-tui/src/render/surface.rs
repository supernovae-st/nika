// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The continuous surface on screen: one opaque band above the composer,
//! whose rows stay where they stand ([`super::band`]). It holds what is
//! chosen now, one thing at a time: the commands the chooser lists (its
//! palette or the slash list of a command being typed), else the offers of
//! the typed choice waiting. The band's plan is the viewer's
//! ([`nika_tui_view::workspace::surface`]): a title, a head (the Session's
//! status facts and what waits, or the question's own words), the entries or
//! offers on whole pages, and the selected command's scope and help. The
//! composer's own row carries the palette's search or the typed choice's own
//! reply, and the hint row its keys. Painting and the pointer ([`hit`]) read
//! one plan; nothing here sends, answers or chooses.

use std::fmt::Write as _;
use std::ops::Range;

use nika_display::theme::Role;
use nika_tui_view::workspace::surface::{self, Ask, Mark, Plan};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use unicode_width::UnicodeWidthStr;

use super::chooser::{aside_line, context, fit};
use super::question::{REQUIRED, WHOLE, hang, word_cap};
use super::{Consent, STANDING, cut, fits_row, own, status_line};
use crate::composer::Composer;
use crate::composer::answer::choice;
use crate::composer::chooser::{Door, Listing};
use crate::lang::Voice;
use crate::model::{Asked, Committed, Kind, Offer, Presentation, Shape, UiState, Waiting};
use crate::visual::role;
use crate::workspace::cards::review::Review;

mod wrapped;

/// At most this many rows carry the selected command's scope and help.
const DETAIL_ROWS: u16 = 2;

/// At most this many entries or offers an inline band grows for; the rest
/// page.
pub(crate) const SHOWN: usize = 8;

/// The command names' column: the widest name, between these bounds.
const NAME_COLUMN: (usize, usize) = (6, 14);

/// The offers' key column: the widest key, up to this; a longer key pushes
/// its label along.
const KEY_COLUMN: usize = 16;

/// The labels' column before a note (a model's facts): the widest marked
/// label, up to this; a longer label pushes its note along.
const LABEL_COLUMN: usize = 30;

/// What the band holds now.
enum Held<'a> {
    /// The commands the chooser lists, words the palette set aside, what
    /// waits beside them and the selected entry's scope and help.
    Commands {
        listing: Listing<'a>,
        aside: Option<&'a str>,
        context: Option<&'static str>,
        detail: String,
    },
    /// The typed choice waiting: the question, its offers, the rows shown
    /// (each offer's own index, then a multiple choice's Submit row), the
    /// words a settings list filters them by, the one selected, if any, and
    /// whether the offers take keys now.
    Choice {
        asked: &'a Asked,
        offers: &'a [Offer],
        rows: Vec<usize>,
        filter: Option<String>,
        selected: Option<usize>,
        checked: Vec<usize>,
        armed: bool,
        /// The own reply holds words: `Enter` sends them, never a row.
        worded: bool,
        /// The offer those words name: `Enter` picks it.
        picked: Option<usize>,
    },
}

impl<'a> Held<'a> {
    /// What a surface shows on `state` with `composer`: the commands while
    /// the chooser lists (they suspend a typed choice), else the typed choice
    /// waiting while no turn works.
    fn of(state: &'a UiState, composer: &'a Composer) -> Option<Self> {
        if let Some(listing) = composer.listing() {
            let detail = (listing.current()).map_or_else(String::new, |entry| {
                own(&format!("{} · {}", entry.scope, entry.help), state.ascii)
            });
            return Some(Self::Commands {
                listing,
                aside: composer.aside(),
                context: context(state),
                detail,
            });
        }
        if state.busy.is_some() {
            return None;
        }
        let (_, asked, offers) = choice(&state.waiting)?;
        Some(Self::Choice {
            asked,
            offers,
            rows: composer.offer_rows(&state.waiting),
            filter: composer.offer_filter(&state.waiting),
            selected: composer.offer_selected(&state.waiting),
            checked: composer.offers_checked(&state.waiting),
            armed: composer.offers_armed(&state.waiting),
            worded: !composer.own_reply().trim().is_empty(),
            picked: composer.offer_named(&state.waiting),
        })
    }

    /// The rows each listed item takes at `width` cells: one for a command, an offer's own
    /// (wrapped under a question or a decision where its words do not fit one row).
    fn heights(&self, width: u16, state: &UiState) -> Vec<usize> {
        match (self, Listed::of(self, state)) {
            (Self::Choice { rows, .. }, Some(listed)) => listed.heights(rows, width),
            _ => vec![1; self.count()],
        }
    }

    /// The entries or rows listed, without the row a search finding nothing
    /// paints.
    fn count(&self) -> usize {
        match self {
            Self::Commands { listing, .. } => listing.entries.len(),
            Self::Choice { rows, .. } => rows.len(),
        }
    }

    /// The selected item's place in the list, if any.
    fn selected(&self) -> Option<usize> {
        match self {
            Self::Commands { listing, .. } => Some(listing.selected),
            Self::Choice { rows, selected, .. } => {
                selected.and_then(|at| rows.iter().position(|row| *row == at))
            }
        }
    }

    /// The item a place in the list stands for: an entry's own place, or a
    /// row's offer index (or a multiple choice's Submit row).
    fn item(&self, place: usize) -> Option<usize> {
        match self {
            Self::Commands { .. } => Some(place),
            Self::Choice { rows, .. } => rows.get(place).copied(),
        }
    }
}

/// The rows of a head, top first, `width` cells wide, and how many of the
/// first ones show whole or not at all (the rest give way first): beside the
/// commands, the Session's status facts, the words the palette set aside and
/// what waits; under a typed choice, what the Session keeps of the request
/// (its goal, the questions still open), then the question's own words, cut
/// at the frame's cap with a last row saying where the whole question is
/// read.
fn head_rows(
    held: &Held<'_>,
    state: &UiState,
    width: u16,
    room: Option<u16>,
) -> (Vec<Line<'static>>, u16) {
    let (ascii, color) = (state.ascii, state.color);
    match held {
        Held::Commands { aside, context, .. } => commands_head(*aside, *context, state, width),
        Held::Choice { asked, .. } => {
            if !asked.why.is_empty()
                && matches!(&state.waiting, Waiting::Action { key, .. } if matches!(key.as_str(), "gate" | "gate_choice"))
            {
                return expired_gate_head(asked, width, (ascii, color));
            }
            let selection = matches!(state.waiting, Waiting::Selection { .. });
            // The block the head reads beside the question: a selection reads Nika's last
            // words, an act the block that asks it.
            let read = if selection {
                (state.transcript.iter()).rposition(|block| {
                    matches!(
                        block.kind,
                        Kind::Question | Kind::Reply | Kind::Refusal | Kind::Notice
                    )
                })
            } else if matches!(&state.waiting, Waiting::Action { key, .. } if key != "save") {
                super::question::action_block(state)
            } else {
                None
            };
            let block = read.and_then(|index| state.transcript.get(index));
            let text = match (asked.guide.as_deref(), block) {
                // The list's own guide, which the conversation never keeps.
                (Some(guide), _) if selection => format!("{}\n{guide}", asked.label),
                (None, Some(block)) if selection => format!("{}\n{}", asked.label, block.text),
                (_, Some(block)) => said_once(&asked.label, &block.text),
                // One transcript block can contain a whole group. These offers
                // answer only the active question; F2 keeps the complete block.
                (_, None) => asked.label.clone(),
            };
            let mut words: Vec<Line<'static>> = (hang(&text, usize::from(width)).into_iter())
                .map(Line::raw)
                .collect();
            let cap = word_cap(state);
            if words.len() > cap + 1 {
                words.truncate(cap);
                words.push(whole_row(width, (state.voice(), color)));
            }
            // A settings guide must keep its feedback above the catalogue: the title
            // and the same two detail rows used by a selected command. Models page
            // before these rows disappear. Ordinary questions retain their first row.
            let readable = if matches!(
                &state.waiting,
                Waiting::Action { key, .. } if matches!(key.as_str(), "gate" | "gate_choice")
            ) {
                // Keep the active gate's question when its run context cannot fit.
                // Further words yield to the offers and remain available through F2.
                1
            } else if matches!(state.waiting, Waiting::Action { .. }) {
                // The offered act must retain the cost facts it decides.
                words.len()
            } else if matches!(state.waiting, Waiting::Selection { .. }) {
                words.len().min(1 + usize::from(DETAIL_ROWS))
            } else {
                1
            };
            let retained = retained_rows(asked, width, (state.voice(), color));
            // What Nika said since the person's last line first, in the rows the question
            // spares: the surface covers the conversation where it stands. A selection reads
            // Nika's last words below, and its earlier lines were the picker's own.
            let said = if selection {
                Vec::new()
            } else {
                said_since(state, read, width)
            };
            let room = room.map_or(usize::MAX, usize::from);
            let spare = room.saturating_sub(retained.len() + readable);
            let mut rows = said_rows(&said, width, said_room(room).min(spare), ascii);
            rows.extend(retained);
            let whole = u16::try_from(rows.len() + readable).unwrap_or(u16::MAX);
            rows.extend(words);
            (rows, whole)
        }
    }
}

/// The head above the commands the chooser lists: the status, the words the palette set
/// aside and what waits beside them, in the accent while nothing waits, else as a warning.
fn commands_head(
    aside: Option<&str>,
    context: Option<&'static str>,
    state: &UiState,
    width: u16,
) -> (Vec<Line<'static>>, u16) {
    let (ascii, color) = (state.ascii, state.color);
    let mut rows = Vec::new();
    let status = status_line(state, true, width, None);
    if status.width() > 0 {
        rows.push(status);
    }
    if let Some(aside) = aside {
        rows.push(aside_line(aside, width, state.voice(), color));
    }
    if let Some(text) = context {
        let tone = if state.busy.is_some() || state.waiting == Waiting::Free {
            Role::Accent
        } else {
            Role::Warn
        };
        let words = fit(&own(text, ascii), usize::from(width), ascii);
        rows.push(Line::styled(words, role::style(tone, color)));
    }
    let whole = u16::try_from(rows.len()).unwrap_or(u16::MAX);
    (rows, whole)
}

/// Keep the exact prompt and expiry notice before offers, even in the compact
/// workspace. Full context remains in the gate's F2 reader; no clock or I/O here.
fn expired_gate_head(
    asked: &Asked,
    width: u16,
    (ascii, color): (bool, bool),
) -> (Vec<Line<'static>>, u16) {
    let cue = own(" … F2", ascii);
    let room = usize::from(width).saturating_sub(cue.width());
    let prompt = asked.label.split_whitespace().collect::<Vec<_>>().join(" ");
    let question = Line::from(vec![
        Span::raw(fit(&prompt, room, ascii)),
        Span::styled(cue, role::style(Role::Dim, color)),
    ]);
    let notice = Line::styled(
        fit(&asked.why, usize::from(width), ascii),
        role::style(Role::Warn, color),
    );
    (vec![question, notice], 2)
}

/// A decision's `label` above the words of its block, said once: an activation's question and
/// a run's ceiling are their block's own first line, so the words alone read it. A block that
/// says what landed before asking (a trigger's decision after its declaration) keeps the label
/// on top and its own words without that line (live gauntlet wave 8, 2026-10-19: « Installer le
/// déclencheur sur ce Mac ? » twice in one card, r01 and r04).
fn said_once(label: &str, words: &str) -> String {
    let asked = label.trim();
    if words.lines().next().map(str::trim) == Some(asked) {
        return words.to_owned();
    }
    if asked.is_empty() || !words.lines().any(|line| line.trim() == asked) {
        return format!("{label}\n{words}");
    }
    let rest: Vec<&str> = (words.lines())
        .filter(|line| line.trim() != asked)
        .collect();
    format!("{label}\n{}", rest.join("\n"))
}

/// What the Session keeps of the request beside a typed choice, one quiet
/// labelled row each, labelled in the person's language: its goal, then the questions still
/// open.
fn retained_rows(asked: &Asked, width: u16, (voice, color): (Voice, bool)) -> Vec<Line<'static>> {
    let ascii = voice.ascii;
    let Some(retained) = &asked.retained else {
        return Vec::new();
    };
    let width = usize::from(width);
    let quiet = role::style(Role::Dim, color);
    let row = |label: &str, words: &str| {
        let label = own(label, ascii);
        let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
        let words = fit(&words, width.saturating_sub(label.width()), ascii);
        Line::from(vec![Span::styled(label, quiet), Span::raw(words)])
    };
    let mut rows = Vec::new();
    if let Some(goal) = &retained.goal {
        rows.push(row(voice.say("Request · "), goal.as_str()));
    }
    if !retained.open.is_empty() {
        let open = retained.open.join(" · ");
        rows.push(row(voice.say("Open · "), open.as_str()));
    }
    rows
}

/// The rows of a head and how many show whole ([`head_rows`]); while the Session waits on the
/// save decision of the proposal the panel reviews (`consent`), what that consent decides,
/// fitted to `room` rows ([`consent_head`]).
fn head_of(
    held: &Held<'_>,
    state: &UiState,
    width: u16,
    consent: Consent<'_>,
    room: Option<u16>,
) -> (Vec<Line<'static>>, u16) {
    match (held, &state.waiting, consent) {
        (Held::Choice { asked, .. }, Waiting::Action { key, .. }, Some(review))
            if key == "save" =>
        {
            consent_head(asked, state, review, width, room)
        }
        _ => head_rows(held, state, width, room),
    }
}

/// The head of the save decision `asked` on the reviewed proposal `review`, `width` cells wide,
/// every row whole and at most `room` rows (none: every row): what the Session said above the
/// proposal ([`leading_notes`]: how it read an answer, a model it chose for the person), the
/// question, the name of the proposal a yes answers beside the key that reads the Session's
/// whole words (on rows of its own where even its shortest form is wider: never cropped), the
/// standing, then the card's own facts in its order, as a cost decision keeps the cost facts it
/// decides. A short band that grants its head at least one row (five rows or more: the title,
/// the list's least three, the head) keeps the name and the standing first (on one row where
/// two would cost a fact, the name in the longest whole form that leaves the standing its
/// place), then each fact on a row of its own, cut with a visible mark where it
/// says more, the last facts giving way to one row of those notes (from three rows on); the
/// rest of the notes, then the question, take only the rows left, a cut note marked. A
/// band of four rows grants its head none: there F2 alone reads the proposal.
fn consent_head(
    asked: &Asked,
    state: &UiState,
    review: &Review,
    width: u16,
    room: Option<u16>,
) -> (Vec<Line<'static>>, u16) {
    let (ascii, color) = (state.ascii, state.color);
    let label: Vec<Line<'static>> = (hang(&asked.label, usize::from(width)).into_iter())
        .map(Line::raw)
        .collect();
    let room = room.map_or(usize::MAX, usize::from);
    // What Nika said since the person's last line first, then that the proposal shown is the one
    // from before it: the band covers the conversation where they stand.
    let unchanged = unchanged_since_you(state);
    let same = state.voice().say(UNCHANGED);
    let mut notes = Vec::new();
    notes.extend(unchanged.then_some(same));
    notes.extend(leading_notes(state));
    // The note that the proposal is unchanged keeps its rows whole, another note one row, and
    // Nika's words the rows they can take beside them, before the last facts, where the head
    // holds three rows or more.
    let kept = if unchanged {
        note_rows(&[same], width, usize::MAX, ascii).len()
    } else {
        usize::from(!notes.is_empty())
    };
    let room_said = said_room(room).min(room.saturating_sub(2 + kept));
    let said = said_rows(&said_since(state, None, width), width, room_said, ascii);
    let reserve = if room > 2 {
        (said.len() + kept).min(room - 2)
    } else {
        0
    };
    let card = room.saturating_sub(reserve);
    let foot = review.foot(usize::from(width), ascii);
    let strong = role::style(Role::Strong, color);
    let identity: Vec<Line<'static>> = (hang(&foot, usize::from(width)).into_iter())
        .map(|row| Line::styled(row, strong))
        .collect();
    let kept = identity.len() + 1;
    let mut facts = review.lines(color, ascii, width);
    if kept + facts.len() > card {
        // The card's rows unwrapped, one row each, cut where a fact says more.
        facts = (review.lines(color, ascii, u16::MAX).into_iter())
            .map(|line| {
                let style =
                    (line.spans.first()).map_or(line.style, |span| line.style.patch(span.style));
                Line::styled(cut(&line.to_string(), width, ascii), style)
            })
            .collect();
    }
    let warn = role::style(Role::Warn, color).add_modifier(Modifier::BOLD);
    let sep = own(" · ", ascii);
    let voice = state.voice();
    let joined = (STANDING.into_iter()).find_map(|words| {
        let words = own(voice.say(words), ascii);
        // The foot in its longest whole form that leaves the standing its place beside it:
        // the proposal's name can be longer than a row's share.
        let room = usize::from(width).checked_sub(sep.width() + words.width())?;
        let foot = review.foot(room, ascii);
        (foot.width() <= room).then(|| {
            Line::from(vec![
                Span::styled(foot, strong),
                Span::raw(sep.clone()),
                Span::styled(words, warn),
            ])
        })
    });
    let mut rows = match joined {
        Some(joined) if kept + facts.len() > card => vec![joined],
        _ => {
            let standing = (STANDING.into_iter())
                .map(|words| voice.say(words))
                .find(|words| fits_row(words, ascii, width))
                .unwrap_or_else(|| voice.say(STANDING[1]));
            let standing = Line::styled(cut(&own(standing, ascii), width, ascii), warn);
            identity.into_iter().chain([standing]).collect()
        }
    };
    let shown = card.saturating_sub(rows.len());
    rows.extend(facts.into_iter().take(shown));
    let spare = room.saturating_sub(rows.len());
    let mut head = said;
    head.truncate(spare);
    let notes = note_rows(&notes, width, spare.saturating_sub(head.len()), ascii);
    head.extend(notes);
    if label.len() <= spare.saturating_sub(head.len()) {
        head.extend(label);
    }
    head.extend(rows);
    // A band that grants its head no row (four rows: the title and the list's least three)
    // shows none of it; F2 reads the whole proposal.
    head.truncate(room);
    let whole = u16::try_from(head.len()).unwrap_or(u16::MAX);
    (head, whole)
}

/// The fewest rows a card's head gives what Nika said since the person's last line, where its
/// room holds them; half the head's room where that is more ([`said_room`]).
const SAID_ROWS: usize = 3;

/// The rows a head of `room` rows gives what Nika said since the person's last line: half of
/// them, and never fewer than [`SAID_ROWS`]; the question and its facts keep the rest.
const fn said_room(room: usize) -> usize {
    let half = room / 2;
    if half < SAID_ROWS { SAID_ROWS } else { half }
}

/// The note a decision on a proposal shown before the person's last line carries.
const UNCHANGED: &str = "this proposal is unchanged since your last message";

/// What Nika said since the person's last line, line by line as the conversation reads it in
/// rows `width` cells wide: the replies and refusals committed after it, which an open surface
/// covers where they stand, but the block `read` the head already reads. None inline, where
/// they stand in the scrollback right above the band, and none before the person said anything.
fn said_since(state: &UiState, read: Option<usize>, width: u16) -> Vec<String> {
    if state.presentation == Presentation::Inline {
        return Vec::new();
    }
    let Some(last) = (state.transcript.iter()).rposition(|block| block.kind == Kind::Human) else {
        return Vec::new();
    };
    (state.transcript.iter().enumerate().skip(last + 1))
        .filter(|(index, block)| {
            Some(*index) != read && matches!(block.kind, Kind::Reply | Kind::Refusal)
        })
        .flat_map(|(_, block)| said_lines(block, state.ascii, width))
        .collect()
}

/// The lines of `block` as the conversation reads them in rows `width` cells wide, without
/// their weights: a reply's Markdown marks give way to the words they carry and its tables fit
/// the rows ([`nika_tui_view::reply::lines_within`]).
fn said_lines(block: &Committed, ascii: bool, width: u16) -> Vec<String> {
    let lines: Vec<String> = if block.kind == Kind::Reply {
        let room = usize::from(width);
        (nika_tui_view::reply::lines_within(&block.text, false, ascii, room).into_iter())
            .map(|spans| spans.iter().map(|span| span.content.as_ref()).collect())
            .collect()
    } else {
        block.text.lines().map(str::to_owned).collect()
    };
    (lines.into_iter())
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

/// The rows of what Nika said ([`said_since`]), `width` cells wide, at most `room`: where they
/// hold more, the first row, marked as cut, then the last ones, so its latest words always read
/// above the card; the conversation reads every word once the card closes.
fn said_rows(said: &[String], width: u16, room: usize, ascii: bool) -> Vec<Line<'static>> {
    let cells = usize::from(width);
    let lines: Vec<Vec<String>> = said.iter().map(|line| hang(line, cells)).collect();
    if lines.iter().map(Vec::len).sum::<usize>() <= room {
        return lines.into_iter().flatten().map(Line::raw).collect();
    }
    let mark = if ascii { " ..." } else { " …" };
    let first = (lines.first().and_then(|rows| rows.first()))
        .map(|first| fit(&format!("{first}{mark}"), cells, ascii));
    // Under it, the latest lines that fit whole, or the last rows of the last line where even
    // it holds more.
    let rest = room.saturating_sub(1);
    let mut whole: Vec<&Vec<String>> = Vec::new();
    let mut used = 0;
    for rows in lines.iter().skip(1).rev() {
        if used + rows.len() > rest {
            break;
        }
        used += rows.len();
        whole.push(rows);
    }
    let tail: Vec<String> = if whole.is_empty() {
        (lines.last())
            .map(|rows| (rows.iter().skip(rows.len().saturating_sub(rest)).cloned()).collect())
            .unwrap_or_default()
    } else {
        whole.into_iter().rev().flatten().cloned().collect()
    };
    (first.into_iter().take(room))
        .chain(tail)
        .map(Line::raw)
        .collect()
}

/// Whether the proposal a decision reviews was shown before the person's last words: their words
/// since changed nothing in it, and the card says so rather than re-offer it as new. An act the
/// decision offers, picked (« Inspect the exact changes », echoed by its label), asked for no
/// change: it is no such line.
fn unchanged_since_you(state: &UiState) -> bool {
    let reviewed = crate::workspace::cards::review::pending(state);
    let offers = choice(&state.waiting).map_or(&[][..], |(_, _, offers)| offers);
    let picked = |said: &str| {
        let said = said.trim();
        (offers.iter()).any(|offer| offer.label.trim() == said || offer.key.trim() == said)
    };
    let last = (state.transcript.iter())
        .rposition(|block| block.kind == Kind::Human && !picked(&block.text));
    matches!((reviewed, last), (Some(reviewed), Some(last)) if reviewed < last)
}

/// The first line of a proposal's own words ([`leading_notes`]): its review (« Nika proposes
/// `compiled-workflow.nika`: », what it does, runs and can touch), or a change set shown alone.
const PROPOSED: [&str; 2] = ["Nika proposes ", "proposed change · "];

/// What the Session said above the proposal the consent reviews, line by line: the lines of the
/// pending proposal block before the proposal's own first line ([`PROPOSED`]), such as how it
/// read an answer or which model it chose for the person. None where that line is not found:
/// nothing there tells a note from the proposal.
fn leading_notes(state: &UiState) -> Vec<&str> {
    let block =
        crate::workspace::cards::review::pending(state).and_then(|at| state.transcript.get(at));
    let Some(block) = block else {
        return Vec::new();
    };
    let Some(first) =
        (block.text.lines()).position(|line| PROPOSED.iter().any(|lead| line.starts_with(lead)))
    else {
        return Vec::new();
    };
    (block.text.lines().take(first))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

/// The rows of `notes`, `width` cells wide, at most `room`: the last row kept marks a cut.
fn note_rows(notes: &[&str], width: u16, room: usize, ascii: bool) -> Vec<Line<'static>> {
    let mut rows: Vec<String> = (notes.iter())
        .flat_map(|note| hang(note, usize::from(width)))
        .collect();
    if rows.len() > room {
        rows.truncate(room);
        if let Some(last) = rows.last_mut() {
            let mark = if ascii { " ..." } else { " …" };
            *last = fit(&format!("{last}{mark}"), usize::from(width), ascii);
        }
    }
    rows.into_iter().map(Line::raw).collect()
}

/// The rows a head shows whole in `band` above `held`'s items: the most the band's plan grants
/// one ([`surface::plan`]) under its title, the list's least rows kept.
fn head_room(items: usize, band: Rect) -> u16 {
    (1..=band.height)
        .rev()
        .find(|rows| {
            let ask = Ask::new(items).headed(*rows, 0);
            surface::plan(band, ask, false).head.height == *rows
        })
        .unwrap_or(0)
}

/// The row that says where the whole question is read.
fn whole_row(width: u16, (voice, color): (Voice, bool)) -> Line<'static> {
    let ascii = voice.ascii;
    let words = fit(&own(voice.say(WHOLE), ascii), usize::from(width), ascii);
    Line::styled(words, role::style(Role::Dim, color))
}

/// What the band asks at `width` cells under its title, its list `items` rows tall.
fn ask(
    held: &Held<'_>,
    state: &UiState,
    (width, items): (u16, usize),
    consent: Consent<'_>,
    room: Option<u16>,
) -> Ask {
    let (rows, whole) = head_of(held, state, width, consent, room);
    let height = u16::try_from(rows.len()).unwrap_or(u16::MAX);
    let tail = match held {
        Held::Commands {
            listing, detail, ..
        } if !listing.entries.is_empty() => {
            let rows = super::content_rows(&[Line::raw(detail.as_str())], width);
            u16::try_from(rows)
                .unwrap_or(DETAIL_ROWS)
                .clamp(1, DETAIL_ROWS)
        }
        Held::Commands { .. } | Held::Choice { .. } => 0,
    };
    Ask::new(items)
        .headed(whole.min(height), height.saturating_sub(whole))
        .tailed(tail)
}

/// The rows a list of items `heights` rows tall takes: every item's, or the one row saying a
/// search or a filter found nothing.
fn listed_rows(heights: &[usize]) -> usize {
    heights.iter().sum::<usize>().max(1)
}

/// Whether a surface is open on `state` with `composer`.
pub(crate) fn open(state: &UiState, composer: &Composer) -> bool {
    shown(state, composer).is_some()
}

/// What the band shows on `state` ([`Held::of`]): nothing of a typed choice while the person
/// reads the conversation behind its card ([`UiState::peek`]).
fn shown<'a>(state: &'a UiState, composer: &'a Composer) -> Option<Held<'a>> {
    Held::of(state, composer).filter(|held| !state.peek || matches!(held, Held::Commands { .. }))
}

/// How to read the conversation a card covers, on the hint row ([`covers`]).
pub(crate) const PEEK_CUE: &str = "PgUp: history";

/// Whether the card of the typed choice waiting covers the conversation: a question's or a
/// decision's, on a full screen, where the band hides the transcript down to the composer
/// (inline, the terminal keeps the history above it). There `PgUp` reads the conversation
/// behind it ([`UiState::peek`]; live gauntlet 5, r06 B9: the mandatory card hid the
/// conversation, with no way to read it before answering).
pub(crate) fn covers(state: &UiState) -> bool {
    state.presentation != Presentation::Inline
        && matches!(Asker::of(&state.waiting), Asker::Author | Asker::Decision)
        && choice(&state.waiting).is_some()
}

/// Whether the typed choice waiting holds the surface (choice mode): its own
/// reply's field then stands in the line's place.
pub(crate) fn choosing(state: &UiState, composer: &Composer) -> bool {
    matches!(Held::of(state, composer), Some(Held::Choice { .. }))
}

/// The rows an inline band asks for at `width` cells (a full screen paints
/// its band over the transcript and asks for none): the title, the head, at
/// most [`SHOWN`] items and the detail, the head as the panel's `consent` reads it.
pub(crate) fn demand(
    state: &UiState,
    composer: &Composer,
    width: u16,
    consent: Consent<'_>,
) -> u16 {
    shown(state, composer).map_or(0, |held| {
        let items = listed_rows(&held.heights(width, state));
        let ask = ask(&held, state, (width, items), consent, None);
        let shown = u16::try_from(items.min(SHOWN)).unwrap_or(1);
        1 + ask.head + ask.more + shown + ask.tail
    })
}

/// The width a band's rows take inside its frame.
const fn inner(band: Rect, framed: bool) -> u16 {
    if framed {
        band.width.saturating_sub(4)
    } else {
        band.width
    }
}

/// The plan of `held` in `band`, `framed` or not, its head as `consent` reads it in the rows
/// the band grants a head ([`head_room`]), the page it shows and the rows each item takes.
fn plan_of(
    held: &Held<'_>,
    state: &UiState,
    (band, framed): (Rect, bool),
    consent: Consent<'_>,
) -> (Plan, Range<usize>, Vec<usize>) {
    let width = inner(band, framed);
    let heights = held.heights(width, state);
    let items = listed_rows(&heights);
    let room = Some(head_room(items, band));
    let ask = ask(held, state, (width, items), consent, room);
    let plan = surface::plan(band, ask, framed);
    let page = wrapped::page(held.selected(), &heights, usize::from(plan.list.height));
    (plan, page, heights)
}

/// Where a press at a point lands on the open surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    /// On this listed command, or this offer of the typed choice.
    Item(usize),
    /// On the band, away from any item: it takes the press and does nothing.
    Band,
}

/// What a press at `point` lands on, for the surface open in `band` with the head `consent`
/// reads: the same plan and page the band paints. `None` outside the band.
pub(crate) fn hit(
    state: &UiState,
    composer: &Composer,
    band: (Rect, bool),
    point: Position,
    consent: Consent<'_>,
) -> Option<Hit> {
    let held = shown(state, composer)?;
    if !band.0.contains(point) {
        return None;
    }
    let (plan, page, heights) = plan_of(&held, state, band, consent);
    let item = wrapped::item_at(&plan, &page, &heights, point.y).and_then(|place| held.item(place));
    Some(item.map_or(Hit::Band, Hit::Item))
}

/// Paint the surface open in `band`, `framed` or not: cleared, its frame and
/// title, its head (as `consent` reads it), the page of items, the selected
/// command's detail.
pub(crate) fn render(
    state: &UiState,
    composer: &Composer,
    band: (Rect, bool),
    buf: &mut Buffer,
    consent: Consent<'_>,
) {
    let Some(held) = shown(state, composer) else {
        return;
    };
    if band.0.height == 0 || band.0.width == 0 {
        return;
    }
    let glyphs = (state.ascii, state.color);
    let (plan, page, heights) = plan_of(&held, state, band, consent);
    surface::clear(&plan, buf, glyphs);
    let title = title(&held, &page, inner(band.0, band.1), state);
    surface::paint_title(&plan, &title, buf);
    head(&held, &plan, state, buf, consent);
    match &held {
        Held::Commands {
            listing, detail, ..
        } => {
            commands(listing, (&plan, page), state, buf);
            if plan.tail.height > 0 {
                let words = Line::styled(detail.clone(), role::style(Role::Dim, state.color));
                Paragraph::new(words)
                    .wrap(Wrap { trim: false })
                    .render(plan.tail, buf);
            }
        }
        Held::Choice { .. } => choice_list(&held, (&plan, page, &heights), state, buf),
    }
}

/// The page of a typed choice's rows on `plan`'s list, items `heights` rows tall: each offer
/// with its glyph and quiet facts (a question's or a decision's wrapped where its words do not
/// fit), a multiple choice's Submit, « Let Nika choose »; a settings filter matching nothing
/// says what `Enter` does instead.
fn choice_list(
    held: &Held<'_>,
    (plan, page, heights): (&Plan, Range<usize>, &[usize]),
    state: &UiState,
    buf: &mut Buffer,
) {
    let (
        Held::Choice {
            asked,
            rows,
            filter,
            selected,
            armed,
            ..
        },
        Some(listed),
    ) = (held, Listed::of(held, state))
    else {
        return;
    };
    let width = plan.list.width;
    if rows.is_empty() {
        let query = filter.as_deref().unwrap_or_default();
        let line = unmatched(query, asked.mandatory, width, state);
        buf.set_line(plan.list.x, plan.list.y, &line, width);
        return;
    }
    // A decision's selected act is filled whether or not its list holds the keys: the row
    // `Enter` acts on is never only a mark beside it.
    let mut top = plan.list.y;
    for place in page {
        let height = heights.get(place).copied().unwrap_or(1);
        if let Some(&index) = rows.get(place) {
            let decision = listed.asker == Asker::Decision;
            let mark = Mark::new(*selected == Some(index), *armed || decision);
            let lines = listed.lines(index, width, mark);
            for (y, line) in (top..plan.list.bottom()).zip(lines.iter().take(height)) {
                buf.set_line(plan.list.x, y, line, width);
            }
        }
        top = top.saturating_add(u16::try_from(height).unwrap_or(u16::MAX));
        if top >= plan.list.bottom() {
            break;
        }
    }
}

/// Who asks what a typed choice lists: it decides how the rows mark, number and wrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asker {
    /// A question the author asked (a question document, a proposal choice).
    Author,
    /// A decision the Session waits on.
    Decision,
    /// The knowledge decision's two acts, in a key column.
    Knowledge,
    /// A settings list: its radio is the value in use.
    Settings,
    /// Any other typed choice.
    Other,
}

impl Asker {
    /// Who asks the typed choice `waiting` holds.
    const fn of(waiting: &Waiting) -> Self {
        match waiting {
            Waiting::QuestionDocument { .. } | Waiting::ProposalChoice { .. } => Self::Author,
            Waiting::Action { .. } => Self::Decision,
            Waiting::Knowledge { .. } => Self::Knowledge,
            Waiting::Selection { .. } => Self::Settings,
            _ => Self::Other,
        }
    }

    /// Whether a label's source names it (a question the author asked) and whether the
    /// recommendation is said (that question, a decision): a decision names the act the Session
    /// recommends, as a question does, by a word beside it; it still opens on no act
    /// ([`crate::composer::answer`]), so `Enter` never takes the recommendation by itself.
    const fn marks(self) -> (bool, bool) {
        (
            matches!(self, Self::Author),
            matches!(self, Self::Author | Self::Decision),
        )
    }

    /// Whether each of the first nine offers wears the digit that selects it
    /// ([`crate::composer::answer`]), and an offer wraps the words a row cannot hold: a
    /// question's or a decision's, never a settings list's (one row per model or connection).
    const fn numbered(self) -> bool {
        matches!(self, Self::Author | Self::Decision)
    }
}

/// What every row of a typed choice's list shares, read once for measuring and painting: what
/// its rows mark and number, the column their facts align on, what its radios and boxes read.
struct Listed<'h> {
    offers: &'h [Offer],
    checked: &'h [usize],
    multiple: bool,
    asker: Asker,
    /// The knowledge choice's key column.
    column: usize,
    /// The column quiet facts align on after the names.
    named: usize,
    worded: bool,
    picked: Option<usize>,
    voice: Voice,
    color: bool,
}

impl<'h> Listed<'h> {
    /// The list of the typed choice `held` holds on `state`; `None` beside the commands.
    fn of(held: &'h Held<'_>, state: &UiState) -> Option<Self> {
        let Held::Choice {
            offers,
            checked,
            worded,
            picked,
            asked,
            ..
        } = held
        else {
            return None;
        };
        let asker = Asker::of(&state.waiting);
        let (marks, numbered) = (asker.marks(), asker.numbered());
        // The knowledge choice's two acts read their exact lines in a key column.
        let column = (offers.iter().filter(|_| asker == Asker::Knowledge))
            .map(|offer| offer.key.width())
            .max()
            .unwrap_or(0)
            .clamp(1, KEY_COLUMN);
        // Offers with quiet facts read them in one column after their names, aligned over the
        // whole list (never only the page shown): every name is measured. A settings list builds
        // only the rows it shows; a question's or a decision's few rows are built whole once to
        // measure their heights ([`Listed::heights`]).
        let named = (offers.iter())
            .filter_map(|offer| {
                let (name, facts) = name_of(offer, marks);
                facts.then(|| name.width() + 4 + if numbered { 2 } else { 0 })
            })
            .max()
            .map_or(0, |widest| widest.min(LABEL_COLUMN));
        Some(Self {
            offers,
            checked,
            multiple: matches!(asked.shape, Shape::Multiple(_)),
            asker,
            column,
            named,
            worded: *worded,
            picked: *picked,
            voice: state.voice(),
            color: state.color,
        })
    }

    /// Whether this list's offers wrap the words a row cannot hold ([`Asker::numbered`]).
    const fn wraps(&self) -> bool {
        self.asker.numbered()
    }

    /// The rows each of `rows` takes at `width` cells: one, or a wrapped offer's.
    fn heights(&self, rows: &[usize], width: u16) -> Vec<usize> {
        let none = Mark::new(false, false);
        if !self.wraps() {
            return vec![1; rows.len()];
        }
        (rows.iter())
            .map(|index| self.lines(*index, width, none).len().max(1))
            .collect()
    }

    /// The digit that selects the offer at `index`, where the list wears them.
    fn number(&self, index: usize) -> String {
        match index {
            0..9 if self.asker.numbered() => format!("{} ", index + 1),
            _ if self.asker.numbered() => "  ".to_owned(),
            _ => String::new(),
        }
    }

    /// The rows the item at `index` paints, `width` cells wide, marked `mark`: an offer (wrapped
    /// where it wraps and its words do not fit one row), a multiple choice's Submit, « Let Nika
    /// choose »; nothing for a place no item holds.
    fn lines(&self, index: usize, width: u16, mark: Mark) -> Vec<Line<'static>> {
        let (ascii, glyphs) = (self.voice.ascii, (self.voice.ascii, self.color));
        let Some(offer) = self.offers.get(index) else {
            let voice = self.voice;
            return if self.multiple && index == self.offers.len() {
                let detail = voice.say(submit_detail(self.checked.len()));
                vec![surface::row(
                    (voice.say("Submit"), detail),
                    6,
                    width,
                    mark,
                    glyphs,
                )]
            } else if index == self.offers.len() + 1 {
                let words = own(voice.say(delegated(self.offers)), ascii);
                vec![surface::row(
                    (voice.say(DELEGATE), &words),
                    0,
                    width,
                    mark,
                    glyphs,
                )]
            } else {
                Vec::new()
            };
        };
        if self.asker == Asker::Knowledge {
            let line = surface::row((&offer.key, &offer.label), self.column, width, mark, glyphs);
            return vec![line];
        }
        // A radio is filled only where `Enter` applies it: once words are typed, on the offer
        // they name, or none when `Enter` sends them. A settings list's radio is the value in
        // use instead, wherever the cursor stands: the cursor alone says where `Enter` goes
        // (its words only filter it).
        let applies = if self.asker == Asker::Settings {
            offer.is_in_use()
        } else if !self.worded {
            mark.selected
        } else {
            self.picked == Some(index)
        };
        let glyph = match (self.multiple, self.checked.contains(&index), applies, ascii) {
            (true, true, _, _) => "[x]",
            (true, false, _, _) => "[ ]",
            (false, _, true, true) => "(*)",
            (false, _, true, false) => "(●)",
            (false, _, false, _) => "( )",
        };
        let (name, _) = name_of(offer, self.asker.marks());
        let lead = format!("{}{glyph} ", self.number(index));
        let words = format!("{lead}{name}");
        let facts = quiet_facts(offer, self.asker.marks(), self.voice);
        let column = if facts.is_empty() { 0 } else { self.named };
        if !self.wraps() || whole_in_one((&words, &facts), column, width, ascii) {
            return vec![surface::row((&words, &facts), column, width, mark, glyphs)];
        }
        wrapped::rows((&lead, name, &facts), width, mark, glyphs)
    }
}

/// Whether one row of `width` cells holds an offer's `words` and `facts` whole, as
/// [`surface::row`] lays them: the selection glyph, the words padded to `column`, then the facts
/// after their separator.
fn whole_in_one((words, facts): (&str, &str), column: usize, width: u16, ascii: bool) -> bool {
    let (separator, _) = nika_tui_view::workspace::text::marks(ascii);
    let head = 2 + words.width().max(column);
    let rest = if facts.trim().is_empty() || facts == words {
        0
    } else {
        separator.width() + facts.width()
    };
    head + rest <= usize::from(width)
}

/// The row of a settings list whose filter found no row, `{query}` the
/// person's words as typed and `{then}` what `Enter` does with them
/// ([`unmatched`]).
const UNMATCHED: &str = "no row matches {open}{query}{close} · {then}";

/// The row that leaves a question's choice to Nika
/// ([`crate::composer::answer::DELEGATION`]).
const DELEGATE: &str = "Let Nika choose";

/// What leaving the choice to Nika does with `offers`: their one
/// recommendation is taken; with several, Nika chooses and says so.
fn delegated(offers: &[Offer]) -> &'static str {
    if offers.iter().filter(|offer| offer.recommended).count() == 1 {
        "Nika takes the recommended answer"
    } else {
        "Nika chooses and says what it chose"
    }
}

/// The name an offer's row reads first, and whether quiet facts follow it.
/// Under a question the author asked (`asks`), a label holding « · » names
/// its source before it and gives the rest to the quiet column
/// (`TechCrunch · techcrunch.com/feed`); any other label is the name whole.
/// Where the recommendation is said (`recommends`: a question the author
/// asked, a decision), a recommended offer has a fact to read.
fn name_of(offer: &Offer, (asks, recommends): (bool, bool)) -> (&str, bool) {
    let label = if offer.label.trim().is_empty() {
        offer.key.as_str()
    } else {
        offer.label.as_str()
    };
    let (name, detail) = match label.split_once(" · ") {
        Some((name, detail)) if asks && !name.trim().is_empty() => (name, detail),
        _ => (label, ""),
    };
    let facts = !detail.is_empty() || !offer.note.is_empty() || (recommends && offer.recommended);
    (name, facts)
}

/// Whether `words` already say an offer is recommended, in English or French.
fn says_recommended(words: &str) -> bool {
    let words = words.to_lowercase();
    ["recommended", "recommandé", "recommande"]
        .iter()
        .any(|word| words.contains(word))
}

/// The quiet facts read beside an offer's name ([`name_of`]), dimmed and cut
/// to the row: the rest of a question's label after « · », its note, then,
/// under a question the author asked or a decision (`recommends`), whether
/// the author or the Session recommends it, unless its own words already say
/// so.
fn quiet_facts(offer: &Offer, (asks, recommends): (bool, bool), voice: Voice) -> String {
    let (name, _) = name_of(offer, (asks, recommends));
    let label = if offer.label.trim().is_empty() {
        &offer.key
    } else {
        &offer.label
    };
    let detail = label
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix(" · "))
        .unwrap_or_default();
    let said = says_recommended(label) || says_recommended(&offer.note);
    let recommended = if recommends && offer.recommended && !said {
        voice.say("recommended")
    } else {
        ""
    };
    let parts = [detail, offer.note.as_str(), recommended];
    let kept: Vec<&str> = parts.into_iter().filter(|part| !part.is_empty()).collect();
    own(&kept.join(" · "), voice.ascii)
}

/// What the Submit row of a multiple choice says `Enter` does with the `checked` answers.
/// How many are checked has its one home, the title.
const fn submit_detail(checked: usize) -> &'static str {
    if checked == 0 {
        "Space checks answers, Enter validates them"
    } else {
        "Enter validates the checked answers"
    }
}

/// The one row of a settings list whose filter found no row: what `Enter`
/// does with the words then. An expert route goes as typed; other words are
/// a message, sent over a usable choice it keeps, or kept until a `required`
/// one is made.
fn unmatched(query: &str, required: bool, width: u16, state: &UiState) -> Line<'static> {
    let (open, close) = if state.ascii {
        ("\"", "\"")
    } else {
        ("« ", " »")
    };
    let voice = state.voice();
    let then = voice.say(if crate::composer::answer::expert_route(query) {
        "Enter sends it as typed"
    } else if required {
        "Enter keeps it as your message until you choose"
    } else {
        "Enter sends it as a message · intelligence unchanged"
    });
    // The person's words go in as typed, after the TUI's own are said.
    let said = (voice.say(UNMATCHED).replace("{then}", then))
        .replace("{open}", open)
        .replace("{close}", close)
        .replace("{query}", query);
    let said = own(&said, state.ascii);
    let words = fit(&said, usize::from(width), state.ascii);
    Line::styled(words, role::style(Role::Dim, state.color))
}

/// The head's rows on `plan`: when the question's words do not all show,
/// the last row shown says where the whole question is read (or, with one
/// row only, its end does).
fn head(held: &Held<'_>, plan: &Plan, state: &UiState, buf: &mut Buffer, consent: Consent<'_>) {
    let items = listed_rows(&held.heights(plan.list.width, state));
    let room = Some(head_room(items, plan.band));
    let (mut rows, _) = head_of(held, state, plan.head.width, consent, room);
    let shown = usize::from(plan.head.height);
    if matches!(held, Held::Choice { .. }) && shown < rows.len() {
        rows.truncate(shown);
        if shown >= 2 {
            rows[shown - 1] = whole_row(plan.head.width, (state.voice(), state.color));
        } else if let Some(first) = rows.first_mut() {
            let cue = own(" … F2", state.ascii);
            let room = usize::from(plan.head.width).saturating_sub(cue.width());
            let kept = fit(&first.to_string(), room, state.ascii);
            *first = Line::from(vec![
                Span::raw(kept),
                Span::styled(cue, role::style(Role::Dim, state.color)),
            ]);
        }
    }
    for (y, row) in (plan.head.y..plan.head.bottom()).zip(rows) {
        buf.set_line(plan.head.x, y, &row, plan.head.width);
    }
}

/// The title row: what the surface holds and its facts, and what closes it.
fn title(held: &Held<'_>, page: &Range<usize>, width: u16, state: &UiState) -> Line<'static> {
    let glyphs = (state.ascii, state.color);
    match held {
        Held::Commands { listing, .. } => {
            let voice = state.voice();
            let facts = voice.say(match (listing.door, listing.entries.is_empty()) {
                (_, true) => " · nothing matches",
                (Door::Palette, false) => " · choose an action",
                (Door::Slash, false) => " · beginning with what you typed",
            });
            let name = voice.say("Commands");
            surface::title((name, &own(facts, state.ascii)), "Esc", width, glyphs)
        }
        Held::Choice {
            asked,
            offers,
            rows,
            filter,
            checked,
            ..
        } => {
            let voice = state.voice();
            let knowledge = matches!(state.waiting, Waiting::Knowledge { .. });
            let mut facts = String::new();
            if knowledge {
                facts.push_str(voice.say(" · your message waits"));
            } else if asked.mandatory {
                facts.push_str(voice.say(REQUIRED));
            }
            if matches!(asked.shape, Shape::Multiple(_)) {
                let selected = (voice.say(" · {checked} of {all} selected"))
                    .replace("{checked}", &checked.len().to_string())
                    .replace("{all}", &offers.len().to_string());
                facts.push_str(&selected);
            }
            // The offers listed, a multiple choice's Submit row aside, and so is a row that
            // moves within a settings list rather than choosing: the count is the choices'.
            let choice = |row: &usize| offers.get(*row).is_some_and(|offer| !offer.is_aside());
            let listed = rows.iter().filter(|row| choice(row)).count();
            let all = offers.iter().filter(|offer| !offer.is_aside()).count();
            if filter.is_some() {
                // Out of the rows the list shows; where a search also finds rows the list keeps
                // out of view (every model under every connection), the matches alone: « 1 of
                // 657 match » over 23 connections read as a count of nothing shown (live
                // gauntlet r08 B21).
                if offers.iter().any(Offer::hidden) {
                    let _ = write!(facts, " · {}", voice.count(listed, "match"));
                } else {
                    let matched = (voice.say(" · {listed} of {all} match"))
                        .replace("{listed}", &listed.to_string())
                        .replace("{all}", &all.to_string());
                    facts.push_str(&matched);
                }
            }
            // The page over every row the list keeps; its range and total over the choices.
            let kept = rows.iter().filter(|row| **row < offers.len()).count();
            if page.len() < kept {
                let shown = |end: usize| rows.iter().take(end).filter(|row| choice(row)).count();
                let paged = (voice.say(" · {from}-{to} of {listed}"))
                    .replace("{from}", &(shown(page.start) + 1).min(listed).to_string())
                    .replace("{to}", &shown(page.end).to_string())
                    .replace("{listed}", &listed.to_string());
                facts.push_str(&paged);
            }
            let name = voice.say(if knowledge {
                "Knowledge"
            } else if matches!(state.waiting, Waiting::Selection { .. }) {
                "Intelligence"
            } else if matches!(state.waiting, Waiting::Action { .. }) {
                "Decision"
            } else if matches!(state.waiting, Waiting::ProposalChoice { .. }) {
                "Proposal choice"
            } else {
                "Question"
            });
            surface::title((name, &own(&facts, state.ascii)), "", width, glyphs)
        }
    }
}

/// The page of commands on `plan`'s list rows; a search that finds nothing
/// says so on the first.
fn commands(
    listing: &Listing<'_>,
    (plan, page): (&Plan, Range<usize>),
    state: &UiState,
    buf: &mut Buffer,
) {
    let (ascii, color) = (state.ascii, state.color);
    let width = plan.list.width;
    if listing.entries.is_empty() {
        let query = listing.query.unwrap_or_default();
        let (open, close) = if ascii { ("\"", "\"") } else { ("« ", " »") };
        // The person's search goes in last, as typed.
        let words = (state.voice())
            .say("nothing matches {open}{query}{close} · Backspace edits the search")
            .replace("{open}", open)
            .replace("{close}", close);
        let said = own(&words.replace("{query}", query), ascii);
        let words = fit(&said, usize::from(width), ascii);
        let line = Line::styled(words, role::style(Role::Dim, color));
        buf.set_line(plan.list.x, plan.list.y, &line, width);
        return;
    }
    let column = (listing.entries.iter())
        .map(|entry| entry.name.width())
        .max()
        .unwrap_or(0)
        .clamp(NAME_COLUMN.0, NAME_COLUMN.1);
    for (y, index) in (plan.list.y..plan.list.bottom()).zip(page) {
        let Some(entry) = listing.entries.get(index) else {
            break;
        };
        let mark = Mark::new(index == listing.selected, true);
        let effect = own(&entry.effect, ascii);
        let words = (entry.name.as_str(), effect.as_str());
        let line = surface::row(words, column, width, mark, (ascii, color));
        buf.set_line(plan.list.x, y, &line, width);
    }
}
