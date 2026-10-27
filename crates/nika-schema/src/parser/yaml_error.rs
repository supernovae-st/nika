// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A YAML loader refusal (`NIKA-PARSE-001`) at the site the scanner stopped
//! at, with its usual cause when the line shows one (#1608).
//!
//! Split from `parser/mod.rs` under its line budget: none of it reads parser
//! state, only the loader error and the source text.

use marked_yaml::{LoadError, Marker};

use super::CharToByte;
use crate::error::SchemaError;
use crate::source::{ByteOffset, FileId, Span};

/// The longest authored key a cause names before it says `key` instead.
const NAMED_KEY_CHARS: usize = 40;

/// A loader refusal at the site the scanner stopped at: the span every other
/// PARSE code carries, so the frame and `--json` name the line, plus its usual
/// cause when the line shows one.
///
/// A scanner that ran out of input names the stream end, often a line past the
/// last one; the site is then where the content ended, the place a missing
/// closer belongs, and the cause says the file ended there.
pub(super) fn scanner_refusal(
    err: &LoadError,
    yaml: &str,
    file_id: FileId,
    char_to_byte: &CharToByte,
) -> SchemaError {
    let raw = err.to_string();
    let Some(marker) = load_error_marker(err) else {
        return SchemaError::YamlSyntax {
            message: raw,
            span: None,
        };
    };
    let content_end = yaml.trim_end().len();
    let stopped = usize::try_from(char_to_byte.marker(marker)).unwrap_or(usize::MAX);
    let at_end = stopped >= content_end;
    let site = stopped.min(content_end);
    let cause = yaml_syntax_cause(&raw, line_at(yaml, site), at_end);
    SchemaError::YamlSyntax {
        message: match cause {
            Some(cause) => format!("{raw} — {cause}"),
            None => raw,
        },
        span: Some(Span::point(
            file_id,
            ByteOffset::new(u32::try_from(site).unwrap_or(u32::MAX)),
        )),
    }
}

/// The position a loader refusal names; a duplicate key carries its own span instead.
fn load_error_marker(err: &LoadError) -> Option<&Marker> {
    match err {
        LoadError::TopLevelMustBeMapping(m)
        | LoadError::TopLevelMustBeSequence(m)
        | LoadError::UnexpectedAnchor(m)
        | LoadError::MappingKeyMustBeScalar(m)
        | LoadError::UnexpectedTag(m)
        | LoadError::ScanError(m, _) => Some(m),
        LoadError::DuplicateKey(_) => None,
    }
}

/// The line holding byte `at`, without its break.
fn line_at(yaml: &str, at: usize) -> &str {
    let before = yaml.get(..at).unwrap_or(yaml);
    let start = before.rfind(['\n', '\r']).map_or(0, |i| i + 1);
    yaml.get(start..)
        .and_then(|rest| rest.split(['\n', '\r']).next())
        .unwrap_or("")
}

/// The cause behind the YAML errors a newcomer meets most, read on the line the
/// scanner stopped at: a `: ` inside a plain value (or a key that meant to nest),
/// an unquoted `${{ … }}` inside a flow mapping, and a flow collection the file
/// never closed. `None` when the line shows none of them.
fn yaml_syntax_cause(raw: &str, line: &str, at_end: bool) -> Option<String> {
    if raw.contains("mapping values are not allowed")
        && let Some(cause) = colon_in_plain_value(line)
    {
        return Some(cause);
    }
    if raw.contains("flow mapping") && holds_unquoted_expression(line) {
        return Some(
            "inside a flow mapping `{ … }` an expression must be quoted: \
             `{ prompt: \"${{ … }}\" }`"
                .to_owned(),
        );
    }
    if at_end && raw.contains("flow mapping") {
        return Some("the file ends inside an open `{ … }`: close it with `}`".to_owned());
    }
    if at_end && raw.contains("flow sequence") {
        return Some("the file ends inside an open `[ … ]`: close it with `]`".to_owned());
    }
    None
}

/// `outer: inner: rest` read as a plain value holding `: `. The same line is a
/// key that meant to nest (`infer: prompt: hi`) when `inner` is a bare key, so
/// the cause names both repairs rather than guessing one.
fn colon_in_plain_value(line: &str) -> Option<String> {
    let entry = line.trim_start().trim_start_matches("- ");
    let (outer, value) = entry.split_once(": ")?;
    let (inner, _) = value.split_once(": ")?;
    let outer = if is_bare_key(outer) { outer } else { "key" };
    let shown = if inner.chars().count() <= NAMED_KEY_CHARS {
        inner
    } else {
        "…"
    };
    let nested = if is_bare_key(inner) {
        format!(
            "; if `{inner}:` is a key of its own, put it on its own line, indented under `{outer}:`"
        )
    } else {
        String::new()
    };
    Some(format!(
        "a plain value cannot contain `: `, so quote the whole value \
         (`{outer}: \"{shown}: …\"`) or write it as a block scalar after `{outer}: |`{nested}"
    ))
}

/// A key short and plain enough to name back: letters, digits, `_` and `-`.
fn is_bare_key(text: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= NAMED_KEY_CHARS
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Whether `line` holds a `${{` outside a quoted scalar. A quote opens a scalar
/// only where a scalar starts (after `{`, `[`, `,`, `:` or at the line's start),
/// so the apostrophe in a plain `it's` opens nothing.
fn holds_unquoted_expression(line: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut prev: Option<char> = None;
    let mut chars = line.char_indices();
    while let Some((at, c)) = chars.next() {
        match quote {
            Some('"') if c == '\\' => {
                chars.next();
            }
            // `''` is the single-quoted scalar's own quote, not its end.
            Some('\'') if c == '\'' && line.get(at + 1..).is_some_and(|r| r.starts_with('\'')) => {
                chars.next();
            }
            Some(open) if c == open => quote = None,
            None if (c == '"' || c == '\'')
                && matches!(prev, None | Some('{' | '[' | ',' | ':' | '-')) =>
            {
                quote = Some(c);
            }
            None if line.get(at..).is_some_and(|rest| rest.starts_with("${{")) => return true,
            Some(_) | None => {}
        }
        if !c.is_whitespace() {
            prev = Some(c);
        }
    }
    false
}
