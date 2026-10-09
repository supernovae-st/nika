// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The text a new or replaced value takes. Each function proposes; the
//! strict parser decides: an edit keeps a candidate only when the reparsed
//! document reads exactly the value it was asked to write.

use serde_json::Value;

use super::Style;

/// Compact JSON, valid YAML flow, with the code points a YAML reader refuses
/// raw (DEL, the C1 controls, U+FFFE, U+FFFF) escaped. Compact JSON is ASCII
/// outside its strings, so these can only sit inside a string or a key, where
/// `\uXXXX` means the same to JSON and to a YAML double-quoted scalar.
pub(super) fn flow(value: &Value) -> String {
    value
        .to_string()
        .chars()
        .map(|c| match c {
            '\u{7f}'..='\u{9f}' | '\u{fffe}' | '\u{ffff}' => format!("\\u{:04x}", u32::from(c)),
            _ => c.to_string(),
        })
        .collect()
}

/// A double-quoted scalar of `text` (JSON's escapes are YAML's).
pub(super) fn double_quoted(text: &str) -> String {
    flow(&Value::String(text.to_owned()))
}

/// A single-quoted scalar of `text`, when one line of printable text holds it.
fn single_quoted(text: &str) -> Option<String> {
    printable(text, false).then(|| format!("'{}'", text.replace('\'', "''")))
}

/// Whether every character of `text` may stand raw in a YAML scalar (a tab
/// only when `tabs`); line breaks are never raw here.
fn printable(text: &str, tabs: bool) -> bool {
    text.chars().all(|c| {
        (tabs && c == '\t')
            || !(c.is_control()
                || matches!(
                    c,
                    '\u{feff}' | '\u{fffe}' | '\u{ffff}' | '\u{2028}' | '\u{2029}'
                ))
    })
}

/// The parser's own numeric gate (`parser::value::looks_like_number`): a text
/// it would try to read as a number is never written plain.
fn numeric(text: &str) -> bool {
    let mut chars = text.chars();
    let first = match chars.next() {
        Some(c) if c.is_ascii_digit() => true,
        Some('-' | '+') => chars.next().is_some_and(|c| c.is_ascii_digit()),
        _ => false,
    };
    first
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
}

/// Whether `text` can be written as a plain scalar that every reader takes
/// for exactly this string: no indicator first, no `: ` or ` #`, no word a
/// YAML 1.1 or 1.2 reader resolves to another type, no flow indicator inside
/// a flow collection.
pub(super) fn plain_safe(text: &str, in_flow: bool) -> bool {
    let Some(first) = text.chars().next() else {
        return false;
    };
    let reserved = [
        "~", "null", "true", "false", "yes", "no", "on", "off", "y", "n",
    ];
    let indicator = "-?:,[]{}#&*!|>'\"%@`".contains(first);
    let flow_hazard = in_flow && text.contains([',', '[', ']', '{', '}', ':']);
    printable(text, false)
        && !indicator
        && !flow_hazard
        && !text.starts_with(' ')
        && !text.ends_with([' ', ':'])
        && !text.contains(": ")
        && !text.contains(" #")
        && !numeric(text)
        && !reserved.iter().any(|r| r.eq_ignore_ascii_case(text))
}

/// A literal block (`|` or `|-`) holding `text` exactly, its content at
/// `indent`; `None` when no literal block states it (leading space, a raw
/// control, several trailing line breaks, an empty text).
pub(super) fn literal(text: &str, indent: usize, nl: &str) -> Option<String> {
    let (body, chomp) = match text.strip_suffix('\n') {
        Some(body) if !body.ends_with('\n') => (body, ""),
        Some(_) => return None,
        None => (text, "-"),
    };
    if body.is_empty()
        || body.starts_with([' ', '\t'])
        || body.contains('\r')
        || !body.split('\n').all(|line| printable(line, true))
    {
        return None;
    }
    let pad = " ".repeat(indent);
    let lines: Vec<String> = body
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect();
    Some(format!("|{chomp}{nl}{}", lines.join(nl)))
}

/// A scalar token for a non-string literal (`null`, a boolean, a number).
fn token(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some("null".to_owned()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::String(_) | Value::Array(_) | Value::Object(_) => None,
    }
}

/// The texts a scalar `value` may take where a scalar of style `old` stood,
/// in order of preference: the old presentation first, double quotes last.
pub(super) fn scalars(
    value: &Value,
    old: Style,
    in_flow: bool,
    indent: usize,
    nl: &str,
) -> Vec<String> {
    let Value::String(text) = value else {
        return token(value).into_iter().collect();
    };
    let quoted = double_quoted(text);
    let mut out = Vec::new();
    match old {
        Style::DoubleQuoted => {}
        Style::SingleQuoted => out.extend(single_quoted(text)),
        Style::Literal | Style::Folded if !in_flow => out.extend(literal(text, indent, nl)),
        _ if plain_safe(text, in_flow) => out.push(text.clone()),
        _ => {}
    }
    out.push(quoted);
    out
}

/// A mapping key: plain when it is a simple name, double-quoted otherwise.
pub(super) fn key(text: &str, in_flow: bool) -> String {
    let simple = text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'));
    if simple && plain_safe(text, in_flow) {
        text.to_owned()
    } else {
        double_quoted(text)
    }
}

/// The text after `key:` for `value` in a block mapping whose entries sit at
/// column `indent` (`step` deeper for a nested collection): ` scalar`,
/// ` {}`/` []`, or the nested block on the following lines.
pub(super) fn tail(value: &Value, indent: usize, step: usize, nl: &str) -> String {
    let deeper = indent + step;
    match value {
        Value::Object(map) if !map.is_empty() => {
            format!(
                "{nl}{}{}",
                " ".repeat(deeper),
                block(value, deeper, step, nl)
            )
        }
        Value::Array(items) if !items.is_empty() => {
            format!(
                "{nl}{}{}",
                " ".repeat(deeper),
                block(value, deeper, step, nl)
            )
        }
        Value::String(text) if text.contains('\n') => {
            let literal = literal(text, deeper, nl);
            format!(" {}", literal.unwrap_or_else(|| double_quoted(text)))
        }
        _ => format!(" {}", inline(value)),
    }
}

/// A value on one line inside a flow collection: a plain scalar when one
/// is safe there, quoted otherwise, a collection as compact flow JSON.
pub(super) fn flow_item(value: &Value) -> String {
    match value {
        Value::String(text) if plain_safe(text, true) => text.clone(),
        Value::String(text) => double_quoted(text),
        _ => inline(value),
    }
}

/// A value on one line: a plain or quoted scalar, or compact flow JSON.
pub(super) fn inline(value: &Value) -> String {
    match value {
        Value::String(text) if plain_safe(text, false) => text.clone(),
        Value::String(text) => double_quoted(text),
        Value::Array(_) | Value::Object(_) => flow(value),
        _ => token(value).unwrap_or_else(|| flow(value)),
    }
}

/// A block collection placed at column `indent`; its first line carries no
/// indentation (the caller already stands there).
pub(super) fn block(value: &Value, indent: usize, step: usize, nl: &str) -> String {
    let pad = format!("{nl}{}", " ".repeat(indent));
    match value {
        Value::Object(map) if !map.is_empty() => map
            .iter()
            .map(|(k, v)| format!("{}:{}", key(k, false), tail(v, indent, step, nl)))
            .collect::<Vec<_>>()
            .join(&pad),
        Value::Array(items) if !items.is_empty() => items
            .iter()
            .map(|item| format!("- {}", item_text(item, indent + 2, step, nl)))
            .collect::<Vec<_>>()
            .join(&pad),
        _ => inline(value),
    }
}

/// A block sequence item after its `- `, the item's own lines at `indent`.
pub(super) fn item_text(item: &Value, indent: usize, step: usize, nl: &str) -> String {
    match item {
        Value::Object(map) if !map.is_empty() => block(item, indent, step, nl),
        Value::String(text) if text.contains('\n') => {
            literal(text, indent, nl).unwrap_or_else(|| double_quoted(text))
        }
        _ => inline(item),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Style, block, flow, key, literal, plain_safe, scalars};

    #[test]
    fn plain_is_offered_only_for_text_every_reader_keeps_a_string() {
        for safe in ["hello", "out/report.md", "${{ tasks.a.output }}", "a-b c"] {
            assert!(plain_safe(safe, false), "{safe}");
        }
        for unsafe_text in [
            "",
            "true",
            "No",
            "null",
            "~",
            "42",
            "-1",
            "1e5",
            "a: b",
            "a #b",
            "- x",
            "{x}",
            " lead",
            "trail ",
            "two\nlines",
            "x:",
            "'q",
            "\"q",
            "*a",
            "&a",
            "!t",
            "%d",
            "@x",
        ] {
            assert!(!plain_safe(unsafe_text, false), "{unsafe_text:?}");
        }
        assert!(plain_safe("a,b", false));
        assert!(!plain_safe("a,b", true));
        assert!(!plain_safe("http://x", true));
    }

    #[test]
    fn the_old_presentation_comes_first_and_double_quotes_last() {
        let text = json!("it's");
        assert_eq!(
            scalars(&text, Style::SingleQuoted, false, 2, "\n"),
            ["'it''s'", "\"it's\""]
        );
        assert_eq!(
            scalars(&json!("x"), Style::Plain, false, 2, "\n"),
            ["x", "\"x\""]
        );
        assert_eq!(
            scalars(&json!(72), Style::DoubleQuoted, false, 2, "\n"),
            ["72"]
        );
        assert_eq!(
            scalars(&json!("a\nb\n"), Style::Literal, false, 4, "\n"),
            ["|\n    a\n    b", "\"a\\nb\\n\""]
        );
    }

    #[test]
    fn a_literal_block_states_its_text_exactly_or_not_at_all() {
        assert_eq!(
            literal("a\n\nb", 2, "\n").as_deref(),
            Some("|-\n  a\n\n  b")
        );
        assert_eq!(literal("x\n", 2, "\n").as_deref(), Some("|\n  x"));
        assert_eq!(literal("x\n\n", 2, "\n"), None);
        assert_eq!(literal(" lead", 2, "\n"), None);
        assert_eq!(literal("", 2, "\n"), None);
    }

    #[test]
    fn hostile_code_points_are_escaped_and_keys_quoted_when_not_simple() {
        assert_eq!(flow(&json!("\u{85}")), "\"\\u0085\"");
        assert_eq!(key("max_items", false), "max_items");
        assert_eq!(key("Content Type", false), "\"Content Type\"");
        assert_eq!(key("true", false), "\"true\"");
    }

    #[test]
    fn block_emission_nests_by_step() {
        let value = json!({"retry": {"max_attempts": 3}, "tools": ["nika:log"]});
        assert_eq!(
            block(&value, 4, 2, "\n"),
            "retry:\n      max_attempts: 3\n    tools:\n      - nika:log"
        );
    }
}
