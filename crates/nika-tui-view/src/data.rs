// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! YAML and TOML, line by line. Neither is parsed into a tree here: each
//! line is split into what its syntax shows (indent, list marks, key,
//! value, comment); the key wears the accent, punctuation and comments are
//! dim, literals stand out, and the facts say the syntax was shown, not
//! validated.

use nika_display::theme::Role;
use ratatui::text::Span;

use super::cells::{self, Sheet, Tally, paint, plain};

/// A YAML line split into what its syntax shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Parts<'a> {
    /// Leading spaces.
    pub(crate) indent: &'a str,
    /// Block sequence marks (`- `, possibly repeated).
    pub(crate) dashes: &'a str,
    /// The mapping key, when the line has one.
    pub(crate) key: Option<&'a str>,
    /// The `:` after the key and the spaces that follow it.
    pub(crate) colon: &'a str,
    /// The value (the whole rest when there is no key).
    pub(crate) value: &'a str,
    /// A trailing comment, `#` included.
    pub(crate) comment: &'a str,
}

/// The byte where a comment starts: a `#` at the start or after
/// whitespace, outside a quote that opened at a token boundary.
pub(crate) fn comment_at(text: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut before: Option<char> = None;
    for (at, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            None if c == '#' && before.is_none_or(char::is_whitespace) => return Some(at),
            None if matches!(c, '"' | '\'')
                && before.is_none_or(|b| {
                    b.is_whitespace() || matches!(b, ':' | '[' | '{' | ',' | '=')
                }) =>
            {
                quote = Some(c);
            }
            Some(_) | None => {}
        }
        before = Some(c);
    }
    None
}

/// Where a mapping key ends in `rest` and where its value starts.
fn key_end(rest: &str) -> Option<(usize, usize)> {
    let first = rest.chars().next()?;
    if matches!(
        first,
        '[' | '{' | '#' | '&' | '*' | '!' | '|' | '>' | '%' | '@' | '`' | ':'
    ) {
        return None;
    }
    let key = if first == '"' || first == '\'' {
        rest[1..].find(first)? + 2
    } else {
        rest.char_indices()
            .find(|&(at, c)| {
                c == ':' && (rest[at + 1..].is_empty() || rest[at + 1..].starts_with(' '))
            })
            .map(|(at, _)| at)?
    };
    let after = rest[key..].strip_prefix(':')?;
    if !(after.is_empty() || after.starts_with(' ')) {
        return None;
    }
    Some((
        key,
        key + 1 + (after.len() - after.trim_start_matches(' ').len()),
    ))
}

/// Split one YAML line.
pub(crate) fn split(line: &str) -> Parts<'_> {
    let body = line.trim_start_matches(' ');
    let indent = &line[..line.len() - body.len()];
    let (body, comment) = match comment_at(body) {
        Some(at) => (&body[..at], &body[at..]),
        None => (body, ""),
    };
    let mut at = 0;
    while let Some(after) = body[at..].strip_prefix('-') {
        if !(after.is_empty() || after.starts_with(' ')) {
            break;
        }
        at += 1 + (after.len() - after.trim_start_matches(' ').len());
    }
    let (dashes, rest) = body.split_at(at);
    match key_end(rest) {
        Some((key, value)) => Parts {
            indent,
            dashes,
            key: Some(&rest[..key]),
            colon: &rest[key..value],
            value: &rest[value..],
            comment,
        },
        None => Parts {
            indent,
            dashes,
            key: None,
            colon: "",
            value: rest,
            comment,
        },
    }
}

/// The role a scalar's words take: literals stand out, `null` recedes.
pub(crate) fn scalar_role(value: &str) -> Option<Role> {
    let word = value.trim();
    let literal = matches!(
        word,
        "true" | "false" | "True" | "False" | "TRUE" | "FALSE" | "yes" | "no" | "on" | "off"
    ) || (!word.is_empty() && word.parse::<f64>().is_ok());
    let quiet = matches!(word, "null" | "Null" | "NULL" | "~")
        || (word.starts_with(['|', '>']) && word.len() <= 3)
        || word.starts_with(['&', '*', '!']);
    if literal {
        Some(Role::Strong)
    } else if quiet {
        Some(Role::Dim)
    } else {
        None
    }
}

/// A scalar as one span in its role.
fn scalar(value: &str, color: bool) -> Span<'static> {
    match scalar_role(value) {
        Some(role) => paint(value, role, color),
        None => plain(value),
    }
}

/// The spans of one split YAML line.
pub(crate) fn yaml_spans(parts: Parts<'_>, color: bool) -> Vec<Span<'static>> {
    let mut out = vec![plain(parts.indent)];
    if !parts.dashes.is_empty() {
        out.push(paint(parts.dashes, Role::Dim, color));
    }
    if let Some(key) = parts.key {
        out.push(paint(key, Role::Accent, color));
        out.push(paint(parts.colon, Role::Dim, color));
    }
    if !parts.value.is_empty() {
        out.push(scalar(parts.value, color));
    }
    if !parts.comment.is_empty() {
        out.push(paint(parts.comment, Role::Dim, color));
    }
    out
}

/// Whether a split line opens a block scalar (`key: |`, `- >-`).
pub(crate) fn opens_block(parts: &Parts<'_>) -> bool {
    let value = parts.value.trim();
    !value.is_empty() && value.starts_with(['|', '>']) && value.len() <= 3
}

/// The number of lines of `text` (a final newline closes the last).
pub(crate) fn line_count(text: &str) -> usize {
    let text = text.strip_suffix('\n').unwrap_or(text);
    text.bytes().filter(|b| *b == b'\n').count() + 1
}

/// Show YAML text, line by line.
pub(crate) fn yaml(sheet: &mut Sheet, text: &str, protected: bool) {
    let canvas = sheet.body.canvas();
    let mut tally = Tally::default();
    let mut block: Option<usize> = None;
    for (raw, more) in cells::lines(text, canvas.limits.line_bytes) {
        let line = tally.line(raw, canvas, protected);
        let indent = line.len() - line.trim_start_matches(' ').len();
        if let Some(depth) = block {
            if line.trim().is_empty() || indent > depth {
                if !sheet.body.push(vec![plain(line)], more) {
                    break;
                }
                continue;
            }
            block = None;
        }
        let parts = split(&line);
        if opens_block(&parts) {
            block = Some(indent);
        }
        if !sheet.body.push(yaml_spans(parts, canvas.color), more) {
            break;
        }
    }
    tally.report(&mut sheet.notes);
    sheet.facts.push(cells::count(line_count(text), "line"));
    sheet.facts.push("syntax shown, not validated".to_owned());
}

/// The byte of the first `=` outside quotes.
fn equals_at(text: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (at, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            None if c == '=' => return Some(at),
            None if matches!(c, '"' | '\'') => quote = Some(c),
            Some(_) | None => {}
        }
    }
    None
}

/// The spans of one TOML line.
fn toml_spans(line: &str, color: bool) -> Vec<Span<'static>> {
    let body = line.trim_start();
    let indent = &line[..line.len() - body.len()];
    let (body, comment) = match comment_at(body) {
        Some(at) => (&body[..at], &body[at..]),
        None => (body, ""),
    };
    let mut out = vec![plain(indent)];
    if body.starts_with('[') {
        let open = body.len() - body.trim_start_matches('[').len();
        let name_end = body
            .rfind(']')
            .map_or(body.len(), |at| body[..=at].trim_end_matches(']').len());
        out.push(paint(&body[..open], Role::Dim, color));
        out.push(paint(&body[open..name_end], Role::Strong, color));
        out.push(paint(&body[name_end..], Role::Dim, color));
    } else if let Some(at) = equals_at(body) {
        let key = body[..at].trim_end();
        out.push(paint(key, Role::Accent, color));
        out.push(paint(&body[key.len()..=at], Role::Dim, color));
        out.push(scalar(&body[at + 1..], color));
    } else {
        out.push(plain(body));
    }
    if !comment.is_empty() {
        out.push(paint(comment, Role::Dim, color));
    }
    out
}

/// Show TOML text, line by line; a multi-line string's lines stay plain.
pub(crate) fn toml(sheet: &mut Sheet, text: &str, protected: bool) {
    let canvas = sheet.body.canvas();
    let mut tally = Tally::default();
    let mut inside: Option<&str> = None;
    for (raw, more) in cells::lines(text, canvas.limits.line_bytes) {
        let line = tally.line(raw, canvas, protected);
        let spans = if let Some(fence) = inside {
            if line.contains(fence) {
                inside = None;
            }
            vec![plain(line)]
        } else {
            for fence in ["\"\"\"", "'''"] {
                if line.matches(fence).count() % 2 == 1 {
                    inside = Some(fence);
                }
            }
            toml_spans(&line, canvas.color)
        };
        if !sheet.body.push(spans, more) {
            break;
        }
    }
    tally.report(&mut sheet.notes);
    sheet.facts.push(cells::count(line_count(text), "line"));
    sheet.facts.push("syntax shown, not validated".to_owned());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, Format};

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
    fn a_yaml_line_splits_into_what_its_syntax_shows() {
        let parts = split("  - name: \"a # b\"  # the name");
        assert_eq!(parts.indent, "  ");
        assert_eq!(parts.dashes, "- ");
        assert_eq!(parts.key, Some("name"));
        assert_eq!(parts.colon, ": ");
        assert_eq!(parts.value, "\"a # b\"  ");
        assert_eq!(parts.comment, "# the name");
        assert_eq!(split("url: http://x:80").key, Some("url"));
        assert_eq!(split("- plain item").key, None);
        assert_eq!(split("don't: stop # here").comment, "# here");
        assert_eq!(split("\"quoted key\": 1").key, Some("\"quoted key\""));
        assert_eq!(
            split("-1").dashes,
            "",
            "a negative number is not a list mark"
        );
    }

    #[test]
    fn yaml_keeps_block_scalars_as_text_and_every_line_whole() {
        let mut sheet = Sheet::new(Canvas::new(60, true, false), Format::Yaml);
        yaml(
            &mut sheet,
            "prompt: |\n  Say: hello\n  # not a comment\nnext: 1\n",
            false,
        );
        assert_eq!(sheet.facts[0], "4 lines");
        assert_eq!(
            texts(sheet),
            ["prompt: |", "  Say: hello", "  # not a comment", "next: 1"]
        );
    }

    #[test]
    fn toml_tables_keys_and_multiline_strings() {
        let mut sheet = Sheet::new(Canvas::new(60, true, false), Format::Toml);
        toml(
            &mut sheet,
            "[package]\nname = \"nika\" # id\ntext = \"\"\"\na = b\n\"\"\"\n",
            false,
        );
        let rows = texts(sheet);
        assert_eq!(rows[0], "[package]");
        assert_eq!(rows[1], "name = \"nika\" # id");
        assert_eq!(rows[3], "a = b");
        let spans = toml_spans("[[bin]]", false);
        let parts: Vec<&str> = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(parts, ["", "[[", "bin", "]]"]);
    }

    #[test]
    fn a_protected_yaml_masks_its_credentials() {
        let mut sheet = Sheet::new(Canvas::new(60, true, false), Format::Yaml);
        yaml(&mut sheet, "user: ann\npassword: hunter2\n", true);
        assert!(sheet.notes.contains(&crate::Note::Masked { count: 1 }));
        let rows = texts(sheet);
        assert_eq!(rows[1], "password: ******");
    }
}
