// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::expect_used)]

use super::*;
use ratatui::style::{Color, Modifier};
use ratatui::widgets::{Paragraph, Widget, Wrap};

#[test]
fn windows_match_paragraph_word_wrapping_unicode_styles_and_alignment() {
    let samples = [
        "",
        "plain text with words",
        "     indented  and trailing   ",
        "abcdefghijklmnopqrstuvwxyz",
        "a\tb\tend",
        "日本語 広い言葉 界",
        "e\u{301} 👩‍💻 🇫🇷 👨‍👩‍👧‍👦 end",
        "a\u{00a0}b\u{200b}c\u{200b}",
        "   a  b      c   d",
        "\u{200b}\u{200b} abc",
        "x  界  y",
    ];
    for sample in samples {
        for alignment in [Alignment::Left, Alignment::Center, Alignment::Right] {
            let lines = vec![
                Line::default(),
                Line::from(vec![
                    Span::styled(sample, Style::new().fg(Color::Cyan)),
                    Span::styled(" τέλος 界", Style::new().add_modifier(Modifier::BOLD)),
                ])
                .style(Style::new().bg(Color::Blue))
                .alignment(alignment),
                Line::raw(sample),
                Line::raw("last row"),
            ];
            for width in [1, 2, 3, 4, 7, 12, 23] {
                let total = Paragraph::new(lines.clone())
                    .wrap(Wrap { trim: false })
                    .line_count(width);
                let mut actual_rows = 0;
                visit(&lines, width, |_, _| {
                    actual_rows += 1;
                    true
                });
                assert_eq!(actual_rows, total, "{sample:?} width {width}");
                let mut offset = 0u16;
                pages(&lines, width, 5, |page| {
                    let area = Rect::new(2, 1, width, u16::try_from(page.len()).expect("page"));
                    let mut actual = Buffer::empty(area);
                    let mut expected = actual.clone();
                    paint_page(page, &mut actual);
                    Paragraph::new(lines.clone())
                        .wrap(Wrap { trim: false })
                        .scroll((offset, 0))
                        .render(area, &mut expected);
                    assert_eq!(
                        actual, expected,
                        "inline {sample:?}, width {width}, offset {offset}"
                    );
                    offset += area.height;
                    Ok::<_, ()>(())
                })
                .expect("inline parity");
                assert_eq!(usize::from(offset), total);
                for skip in 0..=total {
                    let area = Rect::new(2, 1, width, 5);
                    let mut actual = Buffer::empty(Rect::new(0, 0, width + 4, 8));
                    let mut expected = actual.clone();
                    window(&lines, area, skip, &mut actual);
                    Paragraph::new(lines.clone())
                        .wrap(Wrap { trim: false })
                        .scroll((u16::try_from(skip).expect("small parity case"), 0))
                        .render(area, &mut expected);
                    assert_eq!(actual, expected, "{sample:?}, width {width}, offset {skip}");
                }
            }
        }
    }
}

#[test]
fn one_logical_line_can_wrap_past_the_terminal_coordinate_range() {
    let text = "abc ".repeat(140_000) + "FINAL";
    let lines = vec![Line::styled(text, Style::new().fg(Color::Cyan))];
    let width = 8;
    let total = height(&lines, width);
    assert!(total > usize::from(u16::MAX));
    let area = Rect::new(0, 0, width, 3);
    let mut buffer = Buffer::empty(area);
    window(&lines, area, total - 3, &mut buffer);
    let shown = buffer
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(shown.contains("FINAL"), "{shown}");
    assert!(
        buffer
            .content()
            .iter()
            .any(|c| c.symbol() == "F" && c.fg == Color::Cyan)
    );
}

#[test]
fn inline_pages_preserve_every_row_and_stop_after_the_first_write_error() {
    let text = (0..70_000)
        .map(|n| format!("row {n:05}"))
        .collect::<Vec<_>>()
        .join("\n");
    let lines = text.lines().map(Line::raw).collect::<Vec<_>>();
    let mut seen = 0usize;
    pages(&lines, 20, 17, |page| {
        assert!(page.len() <= 17);
        for line in page {
            assert_eq!(line.to_string(), format!("row {seen:05}"));
            seen += 1;
        }
        Ok::<_, ()>(())
    })
    .expect("all pages");
    assert_eq!(seen, 70_000);
    let mut calls = 0;
    let result = pages(&lines, 20, 17, |_| {
        calls += 1;
        Err("write refused")
    });
    assert_eq!(result, Err("write refused"));
    assert_eq!(calls, 1, "a failed terminal write is not retried");
}
