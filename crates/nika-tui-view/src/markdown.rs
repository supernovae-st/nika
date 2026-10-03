// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Markdown, line by line. Headings, lists, quotes, rules and fenced code
//! keep their shape; a quote keeps its `>` marks, so it never reads as the
//! bar of a code block or a table. Strong and emphasis lose their markers
//! where a weight or a slant carries them; a code span keeps its
//! backticks, the only sign of code once colour is gone. A link reads as
//! its text and its target, never as a hidden action. Prose wraps to the
//! width; code never does.

use nika_display::theme::Role;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use super::cells::{self, Sheet, Tally, paint, plain};

/// The index of the next `close` at or after `from`.
fn find(chars: &[char], from: usize, close: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == close)
}

/// The index of the next doubled `close` (`**`, `__`) at or after `from`.
fn find_pair(chars: &[char], from: usize, close: char) -> Option<usize> {
    (from..chars.len().saturating_sub(1)).find(|&i| chars[i] == close && chars[i + 1] == close)
}

/// A link or an image at `at`: its label, its target and the index after it.
fn link(chars: &[char], at: usize) -> Option<(String, String, usize)> {
    let open = if chars[at] == '!' { at + 1 } else { at };
    if chars.get(open) != Some(&'[') {
        return None;
    }
    let close = find(chars, open + 1, ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = find(chars, close + 2, ')')?;
    let label: String = chars[open + 1..close].iter().collect();
    let target: String = chars[close + 2..end].iter().collect();
    Some((label, target, end + 1))
}

/// The spans of one line's inline markdown.
pub(crate) fn inline(text: &str, color: bool) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut i = 0;
    let flush = |run: &mut String, out: &mut Vec<Span<'static>>| {
        if !run.is_empty() {
            out.push(plain(std::mem::take(run)));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        let before = i.checked_sub(1).map(|p| chars[p]);
        let word_before = before.is_some_and(char::is_alphanumeric);
        if c == '`'
            && let Some(end) = find(&chars, i + 1, '`')
        {
            flush(&mut run, &mut out);
            out.push(paint("`", Role::Dim, color));
            out.push(plain(chars[i + 1..end].iter().collect::<String>()));
            out.push(paint("`", Role::Dim, color));
            i = end + 1;
            continue;
        }
        if matches!(c, '*' | '_')
            && chars.get(i + 1) == Some(&c)
            && !word_before
            && let Some(end) = find_pair(&chars, i + 2, c).filter(|e| *e > i + 2)
        {
            flush(&mut run, &mut out);
            let strong: String = chars[i + 2..end].iter().collect();
            out.push(paint(strong, Role::Strong, color));
            i = end + 2;
            continue;
        }
        if matches!(c, '*' | '_')
            && !word_before
            && chars
                .get(i + 1)
                .is_some_and(|n| !n.is_whitespace() && *n != c)
            && let Some(end) = find(&chars, i + 1, c)
        {
            flush(&mut run, &mut out);
            let slanted: String = chars[i + 1..end].iter().collect();
            out.push(Span::styled(
                slanted,
                Style::default().add_modifier(Modifier::ITALIC),
            ));
            i = end + 1;
            continue;
        }
        if matches!(c, '[' | '!')
            && let Some((label, target, end)) = link(&chars, i)
        {
            flush(&mut run, &mut out);
            if c == '!' {
                out.push(paint(format!("[image: {label}]"), Role::Dim, color));
            } else {
                out.push(Span::styled(
                    label,
                    Style::default().add_modifier(Modifier::UNDERLINED),
                ));
            }
            out.push(paint(format!(" ({target})"), Role::Dim, color));
            i = end;
            continue;
        }
        run.push(c);
        i += 1;
    }
    flush(&mut run, &mut out);
    out
}

/// The fence a line opens (its character and length), if it opens one.
fn opens_fence(trimmed: &str) -> Option<(char, usize)> {
    let c = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let n = trimmed.chars().take_while(|x| *x == c).count();
    (n >= 3).then_some((c, n))
}

/// Whether a line is a thematic break (`---`, `* * *`, `___`).
fn is_rule(trimmed: &str) -> bool {
    let marks: Vec<char> = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    marks.len() >= 3 && matches!(marks[0], '-' | '*' | '_') && marks.iter().all(|m| *m == marks[0])
}

/// A list item's marker (as drawn) and the text after it.
fn list_item(trimmed: &str, ascii: bool) -> Option<(String, &str)> {
    let bullet = if ascii { "-" } else { "•" };
    if let Some(rest) = trimmed.strip_prefix(['-', '*', '+'])
        && let Some(text) = rest.strip_prefix(' ')
    {
        return Some((bullet.to_owned(), text));
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    let rest = trimmed.get(digits..)?;
    if (1..=9).contains(&digits)
        && let Some(after) = rest.strip_prefix(['.', ')'])
        && let Some(text) = after.strip_prefix(' ')
    {
        return Some((trimmed[..=digits].to_owned(), text));
    }
    None
}

/// Keep one non-code line (heading, rule, quote, list item, paragraph).
fn block(sheet: &mut Sheet, line: &str, more: bool) -> bool {
    let canvas = sheet.body.canvas();
    let color = canvas.color;
    let indent = &line[..line.len() - line.trim_start().len()];
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && (trimmed.len() == hashes || trimmed[hashes..].starts_with(' '))
    {
        let marks = format!("{} ", &trimmed[..hashes]);
        let hang = [plain(" ".repeat(marks.len()))];
        let title = paint(trimmed[hashes..].trim(), Role::Strong, color);
        return sheet
            .body
            .wrap(&[title], &[paint(marks, Role::Dim, color)], &hang, more);
    }
    if is_rule(trimmed) {
        let bar = if canvas.ascii { "-" } else { "─" };
        let rule = bar.repeat(sheet.body.width().min(40));
        return sheet.body.push(vec![paint(rule, Role::Dim, color)], false);
    }
    if trimmed.starts_with('>') {
        let depth = trimmed
            .chars()
            .take_while(|c| *c == '>' || *c == ' ')
            .filter(|c| *c == '>')
            .count();
        let rest = trimmed.trim_start_matches(['>', ' ']);
        let lead = [paint("> ".repeat(depth), Role::Dim, color)];
        return sheet.body.wrap(&inline(rest, color), &lead, &lead, more);
    }
    if let Some((marker, text)) = list_item(trimmed, canvas.ascii) {
        let first = [
            plain(indent.to_owned()),
            paint(format!("{marker} "), Role::Dim, color),
        ];
        let hang = [plain(" ".repeat(indent.len() + cells::width(&marker) + 1))];
        return sheet.body.wrap(&inline(text, color), &first, &hang, more);
    }
    if trimmed.starts_with('|') {
        return sheet.body.push(vec![plain(line.to_owned())], more);
    }
    sheet.body.wrap(&inline(line, color), &[], &[], more)
}

/// Show Markdown text.
pub(crate) fn show(sheet: &mut Sheet, text: &str, protected: bool) {
    let canvas = sheet.body.canvas();
    let mut tally = Tally::default();
    let mut fence: Option<(char, usize)> = None;
    let mut blank = false;
    let (mut headings, mut blocks) = (0usize, 0usize);
    for (raw, more) in cells::lines(text, canvas.limits.line_bytes) {
        let line = tally.line(raw, canvas, protected);
        let trimmed = line.trim_start();
        let kept = if let Some((c, n)) = fence {
            if opens_fence(trimmed).is_some_and(|(x, m)| x == c && m >= n)
                && trimmed.trim_start_matches(c).trim().is_empty()
            {
                fence = None;
                sheet
                    .body
                    .push(vec![paint(line.clone(), Role::Dim, canvas.color)], more)
            } else {
                let bar = if canvas.ascii { "| " } else { "│ " };
                sheet.body.push(
                    vec![paint(bar, Role::Dim, canvas.color), plain(line.clone())],
                    more,
                )
            }
        } else if let Some(open) = opens_fence(trimmed) {
            fence = Some(open);
            blocks += 1;
            sheet
                .body
                .push(vec![paint(line.clone(), Role::Dim, canvas.color)], more)
        } else if trimmed.is_empty() {
            let keep = blank || sheet.body.push(Vec::new(), false);
            blank = true;
            keep
        } else {
            blank = false;
            if trimmed.starts_with('#') {
                headings += 1;
            }
            block(sheet, &line, more)
        };
        if !kept {
            sheet.body.overflow();
            break;
        }
    }
    tally.report(&mut sheet.notes);
    let counted = [
        cells::count(headings, "heading"),
        cells::count(blocks, "code block"),
    ];
    sheet.facts.push(counted.join(cells::sep(canvas.ascii)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, Format};

    fn words(spans: &[Span<'_>]) -> Vec<String> {
        spans.iter().map(|s| s.content.to_string()).collect()
    }

    fn texts(sheet: Sheet) -> Vec<String> {
        let mut notes = Vec::new();
        sheet
            .body
            .finish(&mut notes)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn inline_marks_become_weights_and_links_read_as_text() {
        let spans = inline("a **b** *c* `d` [e](http://f) snake_case", false);
        assert_eq!(
            words(&spans),
            [
                "a ",
                "b",
                " ",
                "c",
                " ",
                "`",
                "d",
                "`",
                " ",
                "e",
                " (http://f)",
                " snake_case"
            ]
        );
        assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
        assert!(spans[3].style.add_modifier.contains(Modifier::ITALIC));
        assert_eq!(
            words(&inline("![logo](a.png)", false)),
            ["[image: logo]", " (a.png)"]
        );
    }

    #[test]
    fn blocks_keep_their_shape_and_code_never_wraps() {
        let mut sheet = Sheet::new(Canvas::new(24, true, false), Format::Markdown);
        let doc = "# Release notes\n\n- first item that wraps around\n1. one\n> quoted\n\n\n```rust\nlet x = \"a long line of code\";\n```\n---\n";
        show(&mut sheet, doc, false);
        assert_eq!(sheet.facts, ["1 heading - 1 code block"]);
        assert_eq!(
            texts(sheet),
            [
                "# Release notes",
                "",
                "- first item that wraps",
                "  around",
                "1. one",
                "> quoted",
                "",
                "```rust",
                "| let x = \"a long lin...",
                "```",
                "------------------------"
            ]
        );
    }
}
