// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One continuous conversation, with bounded decision and diagnostic cards.
//! Labels follow typed blocks, never parsed prose. Clipping can start inside
//! a message without losing wrapped text.
//! A refusal recognised by the Session's exact sentence reads first as a short
//! summary ([`diagnostics`]); the block keeps the Session's words whole, and
//! measuring and painting use the same lines.

// The presenter's file sits beside the root modules; the cards that paint it
// own the module.
#[path = "../diagnostics.rs"]
pub(crate) mod diagnostics;

use nika_display::theme::Role;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use self::diagnostics::{Shown, Summary};
use super::text::fit_head;

use crate::model::{Committed, Kind, UiState};
use crate::render::{block_lines, content_rows, window};
use crate::visual::role;

fn heading(kind: Kind) -> (&'static str, Role) {
    match kind {
        Kind::Human => ("You", Role::Dim),
        Kind::Question => ("Question", Role::Warn),
        Kind::Proposal => ("Review before saving", Role::Warn),
        Kind::Gate => ("Approval needed", Role::Warn),
        Kind::Refusal => ("Could not continue", Role::Bad),
        Kind::Run => ("Run", Role::VerbInvoke),
        Kind::Result => ("Result", Role::Good),
        Kind::Report => ("Report", Role::Accent),
        Kind::Activity => ("Activity", Role::Accent),
        Kind::Banner => ("Nika", Role::VerbAgent),
        Kind::Notice | Kind::Reply => ("Nika", Role::Accent),
    }
}

/// The lines one card paints: the Session's words, or the summary of a
/// recognised refusal; the block itself is never rewritten.
fn card_lines(block: &Committed, color: bool, ascii: bool) -> Vec<Line<'static>> {
    match diagnostics::shown(block) {
        Shown::Said => block_lines(block, color, ascii),
        Shown::Banner(text) => block_lines(&Committed::new(block.kind, text), color, ascii),
        Shown::Refusal(summary) => summary_lines(block.kind, summary, color, ascii),
    }
}

/// The visible speaker label replaces the compact sent-message marker.
fn message_lines(block: &Committed, color: bool, ascii: bool) -> Vec<Line<'static>> {
    if block.kind == Kind::Human {
        block_lines(
            &Committed::new(Kind::Reply, block.text.clone()),
            color,
            ascii,
        )
    } else {
        card_lines(block, color, ascii)
    }
}

/// Only a decision or failure needs a bounded surface in the transcript.
fn framed(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Question | Kind::Proposal | Kind::Gate | Kind::Refusal
    )
}

fn message_width(kind: Kind, width: u16) -> u16 {
    if framed(kind) {
        width.saturating_sub(4)
    } else {
        width
    }
}

fn message_height(kind: Kind, rows: usize) -> usize {
    rows.saturating_add(if framed(kind) {
        3
    } else if kind == Kind::Banner {
        1
    } else {
        2
    })
}

/// A summary under the block's own glyph and tone: the cause first, then the
/// scope, the way on (strong) and where the details are read (dim), each
/// aligned under the cause.
fn summary_lines(kind: Kind, summary: &Summary, color: bool, ascii: bool) -> Vec<Line<'static>> {
    let mut lines = block_lines(&Committed::new(kind, summary.cause), color, ascii);
    let indent = (lines.first())
        .and_then(|line| line.spans.first())
        .map_or(0, Span::width);
    for (text, tone) in [
        (summary.scope, None),
        (summary.next, Some(Role::Strong)),
        (summary.details, Some(Role::Dim)),
    ] {
        let style = tone.map_or_else(Style::default, |tone| role::style(tone, color));
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(indent)),
            Span::styled(text, style),
        ]));
    }
    lines
}

/// Paint the visible end of the conversation without allocating off-screen cells.
pub(crate) fn render(frame: &mut Frame<'_>, state: &UiState, area: Rect) {
    if area.is_empty() {
        return;
    }
    if !room_for_labels(area) {
        compact(frame, state, area);
        return;
    }
    let cards: Vec<_> = state
        .transcript
        .iter()
        .map(|block| {
            let lines = message_lines(block, state.color, state.ascii);
            let rows = content_rows(&lines, message_width(block.kind, area.width));
            (block, lines, rows)
        })
        .collect();
    let total: usize = cards
        .iter()
        .map(|(block, _, rows)| message_height(block.kind, *rows))
        .sum();
    let mut skip = total
        .saturating_sub(usize::from(area.height))
        .saturating_sub(state.focus_scroll);
    let mut y = area.y;
    for (block, lines, rows) in cards {
        let height = message_height(block.kind, rows);
        if skip >= height {
            skip -= height;
            continue;
        }
        let visible = height
            .saturating_sub(skip)
            .min(usize::from(area.bottom() - y));
        let visible = u16::try_from(visible).unwrap_or(area.height);
        paint(
            frame,
            block,
            &lines,
            rows,
            Rect::new(area.x, y, area.width, visible),
            skip,
            state,
        );
        y += visible;
        skip = 0;
        if y == area.bottom() {
            break;
        }
    }
}

/// Total rendered rows, using the same widths and compact fallback as painting.
pub(crate) fn height(state: &UiState, area: Rect) -> usize {
    if !room_for_labels(area) {
        let lines: Vec<_> = state
            .transcript
            .iter()
            .flat_map(|block| card_lines(block, state.color, state.ascii))
            .collect();
        return content_rows(&lines, area.width);
    }
    state
        .transcript
        .iter()
        .map(|block| {
            message_height(
                block.kind,
                content_rows(
                    &message_lines(block, state.color, state.ascii),
                    message_width(block.kind, area.width),
                ),
            )
        })
        .sum()
}

/// Very small viewports spend their cells on message content first.
fn room_for_labels(area: Rect) -> bool {
    area.height >= 6 && area.width >= 6
}

/// In a short split, every row belongs to the question or answer, not chrome.
fn compact(frame: &mut Frame<'_>, state: &UiState, area: Rect) {
    let lines: Vec<_> = state
        .transcript
        .iter()
        .flat_map(|block| card_lines(block, state.color, state.ascii))
        .collect();
    let skip = content_rows(&lines, area.width)
        .saturating_sub(usize::from(area.height))
        .saturating_sub(state.focus_scroll);
    window(&lines, area, skip, frame.buffer_mut());
}

fn paint(
    frame: &mut Frame<'_>,
    block: &Committed,
    lines: &[Line<'static>],
    rows: usize,
    area: Rect,
    offset: usize,
    state: &UiState,
) {
    if !framed(block.kind) {
        paint_message(frame, block, lines, rows, area, offset, state);
        return;
    }
    let height = usize::from(area.height);
    let painted = height.min(rows.saturating_add(2).saturating_sub(offset));
    let painted = u16::try_from(painted).unwrap_or(area.height);
    frame.buffer_mut().set_style(
        Rect::new(area.x, area.y, area.width, painted),
        role::surface(state.color, true),
    );
    let (label, tone) = heading(block.kind);
    let style = role::style(tone, state.color);
    let (top, top_end, edge, bottom, bottom_end, rule, cut) = if state.ascii {
        ("+-", "+", "|", "+", "+", "-", "...")
    } else {
        ("╭─", "╮", "│", "╰", "╯", "─", "…")
    };
    if offset == 0 {
        let label = fit_head(label, usize::from(area.width).saturating_sub(5), cut);
        let title = format!(
            "{top} {label} {}{top_end}",
            rule.repeat(usize::from(area.width).saturating_sub(label.width() + 5))
        );
        frame.render_widget(
            Paragraph::new(Line::styled(title, style)),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }
    let first = offset.max(1);
    let last = offset.saturating_add(height).min(rows.saturating_add(1));
    if last > first {
        let body = Rect::new(
            area.x + 2,
            area.y + u16::try_from(first - offset).unwrap_or(area.height),
            area.width.saturating_sub(4),
            u16::try_from(last - first).unwrap_or(area.height),
        );
        window(lines, body, first - 1, frame.buffer_mut());
        for y in body.y..body.bottom() {
            frame.buffer_mut().set_string(area.x, y, edge, style);
            frame
                .buffer_mut()
                .set_string(area.right() - 1, y, edge, style);
        }
    }
    let foot = rows.saturating_add(1);
    if foot >= offset && foot < offset.saturating_add(height) {
        let line = format!(
            "{bottom}{}{bottom_end}",
            rule.repeat(usize::from(area.width).saturating_sub(2))
        );
        frame.render_widget(
            Paragraph::new(Line::styled(line, style)),
            Rect::new(
                area.x,
                area.y + u16::try_from(foot - offset).unwrap_or(area.height),
                area.width,
                1,
            ),
        );
    }
}

/// Speaker, content, then one breathing row. A banner already introduces
/// itself and needs no additional speaker. Measuring and painting use
/// the complete pane width, including when a scrolled frame starts mid-turn.
fn paint_message(
    frame: &mut Frame<'_>,
    block: &Committed,
    lines: &[Line<'static>],
    rows: usize,
    area: Rect,
    offset: usize,
    state: &UiState,
) {
    let label_rows = usize::from(block.kind != Kind::Banner);
    if offset == 0 && label_rows > 0 {
        let (label, tone) = heading(block.kind);
        let cut = if state.ascii { "..." } else { "…" };
        let label = fit_head(label, usize::from(area.width), cut);
        frame.render_widget(
            Paragraph::new(Line::styled(label, role::style(tone, state.color))),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }
    let first = offset.max(label_rows);
    let last = offset
        .saturating_add(usize::from(area.height))
        .min(rows.saturating_add(label_rows));
    if last > first {
        let body = Rect::new(
            area.x,
            area.y + u16::try_from(first - offset).unwrap_or(area.height),
            area.width,
            u16::try_from(last - first).unwrap_or(area.height),
        );
        window(lines, body, first - label_rows, frame.buffer_mut());
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::diagnostics::tests::{
        ROOT, WARNING_LINE, knowledge_refusal, opening_banner, provider_failure,
    };
    use super::*;
    use crate::model::Presentation;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    /// Every row the cards paint at `width` with the whole conversation in
    /// view, the measured height exactly.
    fn painted(state: &UiState, width: u16) -> (Vec<String>, Buffer) {
        let rows = u16::try_from(height(state, Rect::new(0, 0, width, 6))).expect("rows");
        let mut terminal = Terminal::new(TestBackend::new(width, rows)).expect("terminal");
        terminal
            .draw(|frame| render(frame, state, frame.area()))
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        let text = (0..rows)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect();
        (text, buffer)
    }

    /// Body words from either the open conversation or a bounded decision.
    fn words(rows: &[String]) -> String {
        let inner = |row: &String| {
            let cells: Vec<char> = row.chars().collect();
            if cells.len() > 2 && matches!(cells[0], '│' | '|') {
                Some(cells[1..cells.len() - 1].iter().collect::<String>())
            } else if row.starts_with(['╭', '╰', '+'])
                || matches!(
                    row.trim(),
                    "You" | "Nika" | "Activity" | "Run" | "Result" | "Report"
                )
            {
                None
            } else {
                Some(row.clone())
            }
        };
        let words: Vec<String> = (rows.iter().filter_map(inner))
            .flat_map(|row| {
                row.split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect();
        words.join(" ")
    }

    /// Ordinary conversation flows through one surface. Only the actual
    /// message and its speaker need space; repeated frames add no meaning.
    #[test]
    fn ordinary_turns_share_one_quiet_conversation_surface() {
        for ascii in [false, true] {
            for color in [false, true] {
                let mut state = UiState::new(Presentation::Workspace, color, (40, 12));
                state.ascii = ascii;
                state
                    .transcript
                    .push(Committed::new(Kind::Human, "Keep a compact brief."));
                state.transcript.push(Committed::new(
                    Kind::Reply,
                    "I will keep each update concise.",
                ));
                let before = state.transcript.clone();
                let (rows, buffer) = painted(&state, 40);
                let text: Vec<_> = rows.iter().map(|row| row.trim_end()).collect();
                assert_eq!(
                    text,
                    [
                        "You",
                        "Keep a compact brief.",
                        "",
                        "Nika",
                        "I will keep each update concise.",
                        "",
                    ]
                );
                assert!(
                    buffer
                        .content()
                        .iter()
                        .all(|cell| cell.bg == ratatui::style::Color::Reset)
                );
                assert_eq!(state.transcript, before);
                if !color {
                    assert!(
                        buffer
                            .content()
                            .iter()
                            .all(|cell| cell.fg == ratatui::style::Color::Reset)
                    );
                }
            }
        }
    }

    /// Short panes still distinguish a sent message from the assistant reply.
    #[test]
    fn a_compact_conversation_keeps_the_human_speaker() {
        for ascii in [false, true] {
            let area = Rect::new(0, 0, 40, 4);
            let mut state = UiState::new(Presentation::Workspace, false, (40, 4));
            state.ascii = ascii;
            state
                .transcript
                .push(Committed::new(Kind::Human, "Keep a compact brief."));
            state.transcript.push(Committed::new(
                Kind::Reply,
                "I will keep each update concise.",
            ));
            let before = state.transcript.clone();
            assert_eq!(height(&state, area), 2);
            let mut terminal = Terminal::new(TestBackend::new(40, 4)).expect("terminal");
            terminal
                .draw(|frame| render(frame, &state, area))
                .expect("draw");
            let buffer = terminal.backend().buffer();
            let rows: Vec<String> = (0..4)
                .map(|y| {
                    (0..40)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                        .trim_end()
                        .to_owned()
                })
                .collect();
            assert_eq!(
                rows,
                [
                    format!("{} Keep a compact brief.", if ascii { ">" } else { "›" }),
                    "I will keep each update concise.".to_owned(),
                    String::new(),
                    String::new()
                ]
            );
            assert_eq!(state.transcript, before);
        }
    }

    /// The knowledge refusal reads as its four sentences at every width, glyph
    /// column and colour mode, never as its root or its codes; the measured
    /// height ends on the bottom border, and the transcript keeps its words.
    #[test]
    fn a_recognised_refusal_reads_as_its_summary_at_every_width() {
        let refusal = knowledge_refusal(ROOT);
        for ascii in [false, true] {
            for color in [false, true] {
                for width in [24, 44, 72, 120] {
                    let mut state = UiState::new(Presentation::Workspace, color, (width, 40));
                    state.ascii = ascii;
                    state.transcript.push(Committed::new(
                        Kind::Human,
                        "read notes.md and write digest.md",
                    ));
                    state
                        .transcript
                        .push(Committed::new(Kind::Refusal, refusal.clone()));
                    let (rows, buffer) = painted(&state, width);
                    let glyph = if ascii { "x" } else { "✖" };
                    let summary = format!(
                        "{glyph} Nika cannot verify the knowledge release named by NIKA_KNOWLEDGE. This authoring request was not sent; no write. Earlier routing may have reached the model. Next: quit and restart Nika with NIKA_KNOWLEDGE unset (built-in knowledge) or NIKA_KNOWLEDGE=off. Details: F2"
                    );
                    let case = format!("width {width} ascii {ascii} color {color}");
                    assert!(words(&rows).ends_with(&summary), "{case}: {rows:#?}");
                    let all = rows.join("\n");
                    for wall in [ROOT, "ADMISSION_UNTRUSTED", "NIKA_AUTHORING_STRATEGY"] {
                        assert!(!all.contains(wall), "{case}: {wall} in {all}");
                    }
                    assert!(all.contains("Could not continue"), "{case}: {all}");
                    // A card is its title, body and bottom border, then one gap row.
                    assert!(rows.len() >= 4, "{case}");
                    let (foot, gap) = (&rows[rows.len() - 2], &rows[rows.len() - 1]);
                    assert!(foot.starts_with(if ascii { "+" } else { "╰" }), "{case}");
                    assert!(gap.trim().is_empty(), "{case}: {gap:?}");
                    assert!(!ascii || all.is_ascii(), "{case}: {all}");
                    assert!(
                        color
                            || buffer
                                .content()
                                .iter()
                                .all(|cell| cell.fg == ratatui::style::Color::Reset),
                        "{case}"
                    );
                    assert_eq!(state.transcript[1].text, refusal, "{case}");
                }
            }
        }
    }

    /// In a split too short for cards, the summary's end is what shows, and
    /// the measure counts the same lines.
    #[test]
    fn a_short_split_shows_the_summary_end() {
        let area = Rect::new(0, 0, 40, 5);
        let mut state = UiState::new(Presentation::Workspace, false, (40, 5));
        let block = Committed::new(Kind::Refusal, knowledge_refusal(ROOT));
        state.transcript.push(block.clone());
        assert_eq!(
            height(&state, area),
            content_rows(&card_lines(&block, false, false), 40)
        );
        let mut terminal = Terminal::new(TestBackend::new(40, 5)).expect("terminal");
        terminal
            .draw(|frame| render(frame, &state, area))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let last: String = (0..40).map(|x| buffer[(x, 4)].symbol()).collect();
        assert_eq!(last.trim_end(), "  Details: F2");
    }

    /// A provider failure may have sent its call: its card paints the
    /// Session's words exactly, the uncertain scope included, and claims
    /// nothing more.
    #[test]
    fn a_provider_failure_card_paints_the_session_words() {
        let block = Committed::new(Kind::Refusal, provider_failure());
        for (color, ascii) in [(false, false), (true, true)] {
            assert_eq!(
                card_lines(&block, color, ascii),
                block_lines(&block, color, ascii)
            );
        }
        let mut state = UiState::new(Presentation::Workspace, false, (60, 40));
        state.transcript.push(block.clone());
        let (rows, _) = painted(&state, 60);
        let words = words(&rows);
        assert!(
            words.starts_with("✖ I couldn't use the authoring seat for this part — "),
            "{words}"
        );
        assert!(
            words.contains("a failed call can still have been sent."),
            "{words}"
        );
        assert!(
            !words.to_lowercase().contains("nothing was sent"),
            "{words}"
        );
        assert_eq!(state.transcript[0], block);
    }

    /// The opening banner warns in one short line; its other lines, and the
    /// block, stay as the Session wrote them.
    #[test]
    fn the_banner_card_warns_in_one_short_line() {
        let said = opening_banner(ROOT);
        let mut state = UiState::new(Presentation::Workspace, false, (72, 40));
        state
            .transcript
            .push(Committed::new(Kind::Banner, said.clone()));
        let (rows, _) = painted(&state, 72);
        let all = words(&rows);
        let warning = WARNING_LINE
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            all.starts_with("Nika ·") && all.ends_with(&warning),
            "{all}"
        );
        assert!(
            !all.contains(ROOT) && !all.contains("ADMISSION_UNTRUSTED"),
            "{all}"
        );
        assert_eq!(state.transcript[0].text, said);
    }

    #[test]
    fn compact_view_keeps_the_tail_beyond_u16_rows() {
        let area = Rect::new(0, 0, 30, 5);
        let mut state = UiState::new(Presentation::Workspace, false, (30, 5));
        state.transcript.push(Committed::new(
            Kind::Reply,
            "row\n".repeat(70_000) + "final detail",
        ));
        assert!(height(&state, area) > usize::from(u16::MAX));
        let mut terminal = Terminal::new(TestBackend::new(30, 5)).expect("terminal");
        terminal
            .draw(|frame| render(frame, &state, area))
            .expect("compact draws");
        let shown = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(shown.contains("final detail"), "{shown}");
    }

    #[test]
    fn complete_cards_keep_both_borders_and_padding_at_every_width() {
        for ascii in [false, true] {
            for width in [6, 12, 22, 44, 72] {
                let mut state = UiState::new(Presentation::Workspace, true, (width, 8));
                state.ascii = ascii;
                state.transcript.push(Committed::new(Kind::Proposal, "ok"));
                let mut terminal = Terminal::new(TestBackend::new(width, 8)).expect("terminal");
                terminal
                    .draw(|frame| render(frame, &state, frame.area()))
                    .expect("draw");
                let buffer = terminal.backend().buffer();
                assert_eq!(
                    buffer[(width - 1, 0)].symbol(),
                    if ascii { "+" } else { "╮" }
                );
                assert_eq!(
                    buffer[(width - 1, 1)].symbol(),
                    if ascii { "|" } else { "│" }
                );
                assert_eq!(
                    buffer[(width - 1, 2)].symbol(),
                    if ascii { "+" } else { "╯" }
                );
                assert_eq!(buffer[(1, 1)].symbol(), " ");
                assert_eq!(buffer[(width - 2, 1)].symbol(), " ");
                assert_eq!(buffer[(2, 1)].symbol(), "o");
                assert_eq!(buffer[(3, 1)].symbol(), "k");
            }
        }
    }

    #[test]
    fn wrapped_message_rows_remain_reachable_without_repeated_frames() {
        let area = Rect::new(0, 0, 20, 6);
        let mut state = UiState::new(Presentation::Workspace, false, (20, 6));
        state.transcript.push(Committed::new(
            Kind::Reply,
            "12345678901234567\n".repeat(8) + "last row",
        ));
        assert_eq!(
            height(&state, area),
            11,
            "17 cells fit in the full 20-cell conversation body"
        );
        let mut terminal = Terminal::new(TestBackend::new(20, 6)).expect("terminal");
        let mut saw_first = false;
        let mut saw_last = false;
        for scroll in 0..=height(&state, area) - usize::from(area.height) {
            state.focus_scroll = scroll;
            terminal
                .draw(|frame| render(frame, &state, area))
                .expect("draw");
            let buffer = terminal.backend().buffer();
            let rows: Vec<String> = (0..6)
                .map(|y| (0..20).map(|x| buffer[(x, y)].symbol()).collect())
                .collect();
            saw_first |= rows[0].contains("Nika");
            saw_last |= rows.iter().any(|row| row.contains("last row"));
            assert!(!rows.iter().any(|row| row.contains(['╭', '╰', '│'])));
        }
        assert!(saw_first && saw_last, "both ends stay reachable");
    }

    #[test]
    fn a_clipped_card_keeps_the_end_and_scrolling_reveals_the_previous_turn() {
        let mut state = UiState::new(Presentation::Workspace, true, (35, 12));
        state
            .transcript
            .push(Committed::new(Kind::Human, "earlier intent"));
        state.transcript.push(Committed::new(
            Kind::Reply,
            "a long reply\n".repeat(30) + "the final detail",
        ));
        let mut terminal = Terminal::new(TestBackend::new(35, 12)).expect("test terminal");
        let text = |buffer: &ratatui::buffer::Buffer| {
            (0..12)
                .map(|y| (0..35).map(|x| buffer[(x, y)].symbol()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        };
        terminal
            .draw(|frame| render(frame, &state, frame.area()))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        assert!(text(buffer).contains("the final detail"));
        assert!(!text(buffer).contains("earlier intent"));
        assert!(
            buffer
                .content()
                .iter()
                .all(|cell| cell.bg == ratatui::style::Color::Reset),
            "a scrolled reply remains on the conversation surface"
        );
        state.focus_scroll = usize::MAX;
        state.color = false;
        state.ascii = true;
        terminal
            .draw(|frame| render(frame, &state, frame.area()))
            .expect("draw older");
        let buffer = terminal.backend().buffer();
        assert!(text(buffer).contains("earlier intent"));
        assert!(text(buffer).is_ascii());
        assert!(
            buffer
                .content()
                .iter()
                .all(|cell| cell.fg == ratatui::style::Color::Reset)
        );
    }
}
