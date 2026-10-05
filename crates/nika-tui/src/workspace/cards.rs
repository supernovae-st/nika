// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Conversation cards for the workspace. Labels follow typed blocks, never
//! parsed prose. Clipping can start inside a card without losing wrapped text.

use nika_display::theme::Role;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use crate::model::{Committed, Kind, UiState};
use crate::render::{block_lines, wrapped_rows};
use crate::visual::role;

fn heading(kind: Kind) -> (&'static str, Role) {
    match kind {
        Kind::Human => ("You", Role::Strong),
        Kind::Question => ("Question", Role::Warn),
        Kind::Proposal => ("Review before saving", Role::Warn),
        Kind::Gate => ("Approval needed", Role::Warn),
        Kind::Refusal => ("Could not continue", Role::Bad),
        Kind::Run => ("Run", Role::Accent),
        Kind::Result => ("Result", Role::Accent),
        Kind::Report => ("Report", Role::Accent),
        Kind::Banner | Kind::Notice | Kind::Reply => ("Nika", Role::Accent),
    }
}

/// Paint the visible end of the conversation without allocating off-screen cells.
pub(crate) fn render(frame: &mut Frame<'_>, state: &UiState, area: Rect) {
    if area.is_empty() {
        return;
    }
    if area.height < 6 || area.width < 4 {
        compact(frame, state, area);
        return;
    }
    let width = area.width.saturating_sub(3).max(1);
    let cards: Vec<_> = state
        .transcript
        .iter()
        .map(|block| {
            let lines = block_lines(block, state.color, state.ascii);
            let rows = wrapped_rows(&lines, width);
            (block, lines, rows)
        })
        .collect();
    let total: usize = cards
        .iter()
        .map(|(_, _, rows)| usize::from(*rows) + 3)
        .sum();
    let mut skip = total
        .saturating_sub(usize::from(area.height))
        .saturating_sub(state.focus_scroll);
    let mut y = area.y;
    for (block, lines, rows) in cards {
        let height = usize::from(rows) + 3;
        if skip >= height {
            skip -= height;
            continue;
        }
        let visible = height
            .saturating_sub(skip)
            .min(usize::from(area.bottom() - y));
        let visible = u16::try_from(visible).unwrap_or(area.height);
        let offset = u16::try_from(skip).unwrap_or(u16::MAX);
        paint(
            frame,
            block,
            lines,
            rows,
            Rect::new(area.x, y, area.width, visible),
            offset,
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
    if area.height < 6 || area.width < 4 {
        let lines: Vec<_> = state
            .transcript
            .iter()
            .flat_map(|block| block_lines(block, state.color, state.ascii))
            .collect();
        return usize::from(wrapped_rows(&lines, area.width));
    }
    state
        .transcript
        .iter()
        .map(|block| {
            usize::from(wrapped_rows(
                &block_lines(block, state.color, state.ascii),
                area.width.saturating_sub(3),
            )) + 3
        })
        .sum()
}

/// In a short split, every row belongs to the question or answer, not chrome.
fn compact(frame: &mut Frame<'_>, state: &UiState, area: Rect) {
    let lines: Vec<_> = state
        .transcript
        .iter()
        .flat_map(|block| block_lines(block, state.color, state.ascii))
        .collect();
    let skip = wrapped_rows(&lines, area.width)
        .saturating_sub(area.height)
        .saturating_sub(u16::try_from(state.focus_scroll).unwrap_or(u16::MAX));
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((skip, 0)),
        area,
    );
}

fn paint(
    frame: &mut Frame<'_>,
    block: &Committed,
    lines: Vec<Line<'static>>,
    rows: u16,
    area: Rect,
    offset: u16,
    state: &UiState,
) {
    let painted = area
        .height
        .min(rows.saturating_add(2).saturating_sub(offset));
    frame.buffer_mut().set_style(
        Rect::new(area.x, area.y, area.width, painted),
        role::surface(state.color, true),
    );
    let (label, tone) = heading(block.kind);
    let style = role::style(tone, state.color);
    let (top, edge, bottom, rule) = if state.ascii {
        ("+-", "|", "+-", "-")
    } else {
        ("╭─", "│", "╰─", "─")
    };
    if offset == 0 {
        let title = format!(
            "{top} {label} {}",
            rule.repeat(usize::from(area.width).saturating_sub(label.len() + 4))
        );
        frame.render_widget(
            Paragraph::new(Line::styled(title, style)),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }
    let first = offset.max(1);
    let last = (offset.saturating_add(area.height)).min(rows.saturating_add(1));
    if last > first {
        let body = Rect::new(
            area.x + 2,
            area.y + first - offset,
            area.width.saturating_sub(3),
            last - first,
        );
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((first - 1, 0)),
            body,
        );
        for y in body.y..body.bottom() {
            frame.buffer_mut().set_string(area.x, y, edge, style);
        }
    }
    let foot = rows.saturating_add(1);
    if foot >= offset && foot < offset.saturating_add(area.height) {
        let line = format!(
            "{bottom}{}",
            rule.repeat(usize::from(area.width).saturating_sub(2))
        );
        frame.render_widget(
            Paragraph::new(Line::styled(line, style)),
            Rect::new(area.x, area.y + foot - offset, area.width, 1),
        );
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::model::Presentation;
    use ratatui::{Terminal, backend::TestBackend};

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
                .any(|cell| cell.fg != ratatui::style::Color::Reset)
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
