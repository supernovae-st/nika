// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Literal conservation: whether a literal answer carries an expression island, an integer token
//! JSON cannot hold exactly, the literal a dotted path names in a document, and how an answer
//! fills a literal slot. Moved unchanged from the compile core's edit door (slice C, crate wall),
//! where the answer laws, the slot and operation choice, emission, Check and grants stay;
//! `inexact_integer` reads JSON already validated, it is not a parser.

/// Pure projection of permits needed by exact answered endpoint literals.
pub mod answered;

use serde_json::Value;

/// Literal answers cannot insert expression islands or author implicit references.
#[must_use]
pub fn has_expression(value: &Value) -> bool {
    match value {
        Value::String(s) => s.contains("${{"),
        Value::Array(values) => values.iter().any(has_expression),
        Value::Object(values) => values
            .iter()
            .any(|(k, v)| k.contains("${{") || has_expression(v)),
        _ => false,
    }
}

/// The first integer token the canonical reader cannot hold exactly. Its integer
/// domain is `i64`; past it that reader rounds to f64, as `serde_json` does past
/// `u64`. `json` was already accepted as JSON, so outside a string a number can only
/// begin at `-` or a digit. Lexical only: fraction and exponent tokens are floats
/// and keep their f64 contract; digits inside strings and keys are text.
#[must_use]
pub fn inexact_integer(json: &str) -> Option<&str> {
    let mut rest = json;
    loop {
        let at = rest.find(|c: char| c == '"' || c == '-' || c.is_ascii_digit())?;
        let (_, tail) = rest.split_at(at);
        if let Some(body) = tail.strip_prefix('"') {
            rest = after_string(body);
            continue;
        }
        let end = tail
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
            .unwrap_or(tail.len());
        let (token, after) = tail.split_at(end);
        if !token.contains(['.', 'e', 'E']) && token.parse::<i64>().is_err() {
            return Some(token);
        }
        rest = after;
    }
}

/// The text after a JSON string's closing quote; an escape consumes one character.
fn after_string(body: &str) -> &str {
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '"' => break,
            _ => {}
        }
    }
    chars.as_str()
}

/// Locate the existing node only. Typed constants retain their declaration.
pub fn literal_at<'a>(doc: &'a mut Value, path: &str) -> Option<&'a mut Value> {
    let mut node = doc;
    for key in path.split('.') {
        node = node.get_mut(key)?;
    }
    if node.get("type").is_some()
        && (path.starts_with("inputs.")
            || (path.starts_with("const.") && node.get("value").is_some()))
    {
        let key = if path.starts_with("const.") {
            "value"
        } else {
            "default"
        };
        return node.get_mut(key);
    }
    Some(node)
}

/// Fill a marker line without discarding the surrounding prompt or system text.
pub fn fill_slot(node: &mut Value, answer: Value) -> bool {
    let Some(text) = node.as_str() else {
        return false;
    };
    let holes = text
        .lines()
        .filter(|line| {
            let line = line.trim();
            line.starts_with("<SLOT:") && line.ends_with('>')
        })
        .count();
    if holes != 1 {
        // One answer cannot stand in for several independent authoring choices.
        return false;
    }
    if text.trim().starts_with("<SLOT:") && text.trim().ends_with('>') && text.lines().count() == 1
    {
        *node = answer;
        return true;
    }
    let Some(replacement) = answer.as_str() else {
        return false;
    };
    let filled: Vec<_> = text
        .lines()
        .map(|line| {
            let line_trimmed = line.trim();
            if line_trimmed.starts_with("<SLOT:") && line_trimmed.ends_with('>') {
                replacement
            } else {
                line
            }
        })
        .collect();
    let suffix = if text.ends_with('\n') { "\n" } else { "" };
    *node = Value::String(format!("{}{suffix}", filled.join("\n")));
    true
}
