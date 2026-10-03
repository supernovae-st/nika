// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A workflow's source, verb-aware. The nine envelope keys stand out, a
//! task id is strong, each of the four verbs wears its own hue with its
//! glyph in the gutter beside the line that names it (the theme seam's
//! table, its ASCII twin in the ASCII column), a `${{ }}` expression wears
//! the accent, a builtin or MCP tool name wears the invoke hue and a
//! comment is dim. The highlighting reads lines and judges nothing: the
//! check face says whether the bytes are a valid workflow.

use nika_display::theme::{Role, Theme};
use ratatui::text::Span;

use super::cells::{self, Sheet, Tally, paint, plain};
use super::data::{Parts, line_count, opens_block, scalar_role, split};

/// The nine keys of the envelope.
const ENVELOPE: [&str; 9] = [
    "nika", "model", "inputs", "const", "secrets", "permits", "run", "tasks", "outputs",
];

/// The four verbs.
const VERBS: [&str; 4] = ["infer", "exec", "invoke", "agent"];

/// The start of the next tool name in `text` (`nika:read`, `mcp:srv/x`):
/// a scheme after a non-word character, followed by a lower-case letter.
fn tool_at(text: &str) -> Option<usize> {
    ["nika:", "mcp:"]
        .iter()
        .filter_map(|scheme| {
            text.match_indices(scheme).map(|(at, _)| at).find(|&at| {
                let before = text[..at].chars().next_back();
                let after = text[at + scheme.len()..].chars().next();
                !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                    && after.is_some_and(|c| c.is_ascii_lowercase())
            })
        })
        .min()
}

/// The spans of a value: expressions in the accent, tool names in the
/// invoke hue, a lone literal in its role, the rest plain.
pub(crate) fn value_spans(value: &str, color: bool) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    let mut rest = value;
    loop {
        let expression = rest.find("${{");
        let tool = tool_at(rest);
        let next = match (expression, tool) {
            (Some(e), Some(t)) => Some(e.min(t)),
            (e, t) => e.or(t),
        };
        let Some(at) = next else {
            if out.is_empty()
                && let Some(role) = scalar_role(rest)
            {
                out.push(paint(rest, role, color));
            } else if !rest.is_empty() {
                out.push(plain(rest));
            }
            return out;
        };
        if at > 0 {
            out.push(plain(&rest[..at]));
        }
        let (end, role) = if Some(at) == expression {
            (
                rest[at..].find("}}").map_or(rest.len(), |e| at + e + 2),
                Role::Accent,
            )
        } else {
            let length = rest[at..]
                .find(|c: char| {
                    !(c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '/' | '.' | '-'))
                })
                .unwrap_or(rest.len() - at);
            (at + length, Role::VerbInvoke)
        };
        out.push(paint(&rest[at..end], role, color));
        rest = &rest[end..];
    }
}

/// Where the lines are in the workflow's shape.
#[derive(Debug, Default)]
struct Place {
    /// Inside the `tasks:` mapping.
    in_tasks: bool,
    /// The indent of a task id line, once seen.
    task_indent: Option<usize>,
    /// Inside a block scalar opened at this indent.
    block: Option<usize>,
}

/// The verb a line's key names inside a task, if it names one.
fn verb_of<'a>(parts: &Parts<'a>, indent: usize, place: &Place) -> Option<&'a str> {
    let key = parts.key?;
    let inside = place.in_tasks && place.task_indent.is_some_and(|t| indent > t);
    (inside && VERBS.contains(&key)).then_some(key)
}

/// The spans of one line's content (after the gutter), the place moved on.
fn content(
    line: &str,
    place: &mut Place,
    color: bool,
) -> (Vec<Span<'static>>, Option<&'static str>) {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if let Some(depth) = place.block {
        if line.trim().is_empty() || indent > depth {
            return (value_spans(line, color), None);
        }
        place.block = None;
    }
    let parts = split(line);
    if opens_block(&parts) {
        place.block = Some(indent);
    }
    if indent == 0 && parts.key.is_some() {
        place.in_tasks = parts.key == Some("tasks");
        place.task_indent = None;
    } else if place.in_tasks && parts.key.is_some() && place.task_indent.is_none() && indent > 0 {
        place.task_indent = Some(indent);
    }
    let verb = verb_of(&parts, indent, place);
    let mut out = vec![plain(parts.indent)];
    if !parts.dashes.is_empty() {
        out.push(paint(parts.dashes, Role::Dim, color));
    }
    if let Some(key) = parts.key {
        let envelope = indent == 0 && ENVELOPE.contains(&key);
        let task = place.in_tasks && place.task_indent == Some(indent) && parts.dashes.is_empty();
        let role = if envelope || task {
            Some(Role::Strong)
        } else {
            verb.and_then(Role::for_verb)
        };
        out.push(match role {
            Some(role) => paint(key, role, color),
            None => plain(key),
        });
        out.push(paint(parts.colon, Role::Dim, color));
    }
    if indent == 0 && parts.key == Some("nika") {
        out.push(paint(parts.value, Role::Strong, color));
    } else {
        out.extend(value_spans(parts.value, color));
    }
    if !parts.comment.is_empty() {
        out.push(paint(parts.comment, Role::Dim, color));
    }
    let verb = verb.and_then(|v| VERBS.iter().copied().find(|w| *w == v));
    (out, verb)
}

/// Show a workflow's source: a gutter (line number and verb glyph) and
/// the verb-aware line, what looks like a secret masked when `protected`.
pub(crate) fn show(sheet: &mut Sheet, text: &str, protected: bool) {
    let canvas = sheet.body.canvas();
    let theme = Theme::new(false, canvas.ascii, false);
    let total = line_count(text);
    let digits = total.min(canvas.limits.lines).to_string().len();
    let mut tally = Tally::default();
    let mut place = Place::default();
    for (number, (raw, more)) in cells::lines(text, canvas.limits.line_bytes).enumerate() {
        let line = tally.line(raw, canvas, protected);
        let (spans, verb) = content(&line, &mut place, canvas.color);
        let glyph = match verb {
            Some(verb) => paint(
                format!("{} ", theme.verb_glyph_bare(Some(verb))),
                Role::for_verb(verb).unwrap_or(Role::Dim),
                canvas.color,
            ),
            None => plain("  "),
        };
        let mut row = vec![
            paint(format!("{:>digits$} ", number + 1), Role::Dim, canvas.color),
            glyph,
        ];
        row.extend(spans);
        if !sheet.body.push(row, more) {
            break;
        }
    }
    tally.report(&mut sheet.notes);
    sheet.facts.push(cells::count(total, "line"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, Format};

    const FLOW: &str = "nika: brief # the id\nmodel: mistral/mistral-small-latest\ntasks:\n  read:\n    invoke:\n      tool: \"nika:read\"\n  draft:\n    with: { brief: \"${{ tasks.read.output }}\" }\n    infer:\n      prompt: |\n        Summarise ${{ with.brief }}\n        infer: not a key\n";

    fn rows(sheet: Sheet) -> Vec<Vec<(String, ratatui::style::Style)>> {
        let mut notes = Vec::new();
        sheet
            .body
            .finish(&mut notes)
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| (s.content.to_string(), s.style))
                    .collect()
            })
            .collect()
    }

    fn find<'a>(
        row: &'a [(String, ratatui::style::Style)],
        text: &str,
    ) -> Option<&'a ratatui::style::Style> {
        row.iter().find(|(t, _)| t == text).map(|(_, s)| s)
    }

    #[test]
    fn verbs_wear_their_hue_and_their_glyph_in_the_gutter() {
        let mut sheet = Sheet::new(Canvas::new(80, false, true), Format::Workflow);
        show(&mut sheet, FLOW, false);
        let rows = rows(sheet);
        let invoke = crate::role::style(Role::VerbInvoke, true);
        let infer = crate::role::style(Role::VerbInfer, true);
        assert_eq!(rows[4][1].0, "◆ ", "the invoke line carries its glyph");
        assert_eq!(find(&rows[4], "invoke"), Some(&invoke));
        assert_eq!(
            find(&rows[5], "\"nika:read\"").or_else(|| find(&rows[5], "nika:read")),
            Some(&invoke)
        );
        assert_eq!(rows[8][1].0, "◇ ");
        assert_eq!(find(&rows[8], "infer"), Some(&infer));
        let accent = crate::role::style(Role::Accent, true);
        assert_eq!(find(&rows[10], "${{ with.brief }}"), Some(&accent));
        assert_eq!(rows[11][1].0, "  ", "a block scalar line names no verb");
        let strong = crate::role::style(Role::Strong, true);
        assert_eq!(find(&rows[3], "read"), Some(&strong), "a task id is strong");
        assert_eq!(find(&rows[0], "nika"), Some(&strong));
    }

    #[test]
    fn the_ascii_column_draws_the_twins() {
        let mut sheet = Sheet::new(Canvas::new(80, true, false), Format::Workflow);
        show(&mut sheet, FLOW, false);
        let rows = rows(sheet);
        assert_eq!(rows[4][1].0, "@ ");
        assert_eq!(rows[8][1].0, "i ");
        assert!(rows[0][0].0.starts_with(" 1 "), "{:?}", rows[0][0]);
    }

    #[test]
    fn tool_names_and_expressions_are_found_in_values() {
        let spans = value_spans(
            "\"nika:fetch\" then mcp:github/search and ${{ inputs.x }}",
            false,
        );
        let words: Vec<&str> = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(
            words,
            [
                "\"",
                "nika:fetch",
                "\" then ",
                "mcp:github/search",
                " and ",
                "${{ inputs.x }}"
            ]
        );
        assert_eq!(
            tool_at("nika: my-id"),
            None,
            "the envelope key is not a tool"
        );
    }
}
