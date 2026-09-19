// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bounded source edits. The YAML node locates the value; canonical literal
//! projection validates the entire candidate. No scalar semantics are decoded here.

use marked_yaml::{LoaderOptions, Node, parse_yaml_with_options};
use serde_json::Value;

/// Called only after the canonical parser's resource guards and pure Check.
/// A no-op preserves all bytes. Otherwise only a single-line scalar or a flow
/// collection is replaced; ambiguous/block presentation keeps the original.
pub(super) fn emit(source: &str, before: &Value, after: &Value, name: &str) -> Option<String> {
    if super::edit::literal_projection(source).as_ref() != Some(before) {
        return None;
    }
    if before == after {
        return Some(source.to_owned());
    }
    let declaration = before.get("const")?.get(name)?;
    let mut replacement = after.get("const")?.get(name)?;
    let typed = declaration.get("type").is_some() && declaration.get("value").is_some();
    if typed {
        replacement = replacement.get("value")?;
    }
    let (start, end) = literal_range(source, name, typed)?;
    let prefix = source.get(..start)?;
    let suffix = source.get(end..)?;
    let replacement = yaml_safe_json(&replacement.to_string());
    let candidate = format!("{prefix}{replacement}{suffix}");
    // Validate both readers on the OUTPUT too: acceptance must not manufacture
    // decoder drift that prevents a later unrelated edit of this candidate.
    (serde_yaml_bw::from_str::<Value>(&candidate).ok().as_ref() == Some(after)
        && super::edit::literal_projection(&candidate).as_ref() == Some(after))
    .then_some(candidate)
}

/// Compact JSON is ASCII outside its string literals, so these code points can
/// only sit inside a string or key, where `\uXXXX` means the same to JSON and
/// to a YAML double-quoted scalar. A YAML 1.1 reader rejects DEL, the C1
/// controls and both BMP noncharacters when raw, and folds a raw NEL into a
/// space. The escape is a proposal: `emit` still proves it with both readers.
fn yaml_safe_json(token: &str) -> String {
    token
        .chars()
        .map(|c| match c {
            '\u{7f}'..='\u{9f}' | '\u{fffe}' | '\u{ffff}' => format!("\\u{:04x}", u32::from(c)),
            _ => c.to_string(),
        })
        .collect()
}

/// Byte range of the constant's literal (the `value` of a typed declaration),
/// or `None` when its presentation is outside this bounded slice.
fn literal_range(source: &str, name: &str, typed: bool) -> Option<(usize, usize)> {
    let options = LoaderOptions::default()
        .error_on_duplicate_keys(true)
        .prevent_coercion(true);
    let tree = parse_yaml_with_options(0, source, options).ok()?;
    let mut parent = tree.as_mapping()?.get_node("const")?;
    let mut node = parent.as_mapping()?.get_node(name)?;
    if typed {
        parent = node;
        node = node.as_mapping()?.get_node("value")?;
    }
    let start = byte_offset(source, node.span().start()?.character())?;
    let parent_start = byte_offset(source, parent.span().start()?.character())?;
    let flow = source.as_bytes().get(parent_start) == Some(&b'{');
    let end = match node {
        // An omitted value (`key:`) is an empty PLAIN scalar that the parser
        // marks at the NEXT token: that range is never the target. No written
        // plain scalar is empty, and `prevent_coercion` keeps `''`/`""` apart.
        Node::Scalar(scalar) if scalar.may_coerce() && scalar.as_str().is_empty() => return None,
        Node::Scalar(_) => scalar_end(source, start, flow)?,
        Node::Mapping(_) | Node::Sequence(_) => {
            let closing = match source.as_bytes().get(start)? {
                b'{' => b'}',
                b'[' => b']',
                _ => return None,
            };
            let end = byte_offset(source, node.span().end()?.character())?;
            if source.as_bytes().get(end) != Some(&closing) {
                return None;
            }
            end.checked_add(1)?
        }
    };
    Some((start, end))
}

fn byte_offset(source: &str, character: usize) -> Option<usize> {
    source
        .char_indices()
        .map(|(byte, _)| byte)
        .chain([source.len()])
        .nth(character)
}

/// Lexical boundary only, never a second YAML scalar decoder. Multi-line
/// quoted/block scalars are outside this slice. A folded plain continuation
/// cannot pass the whole-document projection after a one-line replacement.
fn scalar_end(source: &str, start: usize, flow: bool) -> Option<usize> {
    let tail = source.get(start..)?;
    let bytes = tail.as_bytes();
    let first = *bytes.first()?;
    if matches!(first, b'"' | b'\'') {
        let mut i = 1;
        while let Some(&byte) = bytes.get(i) {
            if matches!(byte, b'\r' | b'\n') {
                return None;
            }
            if first == b'"' && byte == b'\\' {
                i += 2;
            } else if byte == first {
                if first == b'\'' && bytes.get(i + 1) == Some(&first) {
                    i += 2;
                } else {
                    return start.checked_add(i + 1);
                }
            } else {
                i += 1;
            }
        }
        return None;
    }
    if matches!(first, b'|' | b'>' | b'#' | b' ' | b'\t' | b'\r' | b'\n') {
        return None;
    }
    let end = bytes
        .iter()
        .enumerate()
        .find_map(|(i, &byte)| {
            (matches!(byte, b'\r' | b'\n')
                || (flow && matches!(byte, b',' | b']' | b'}'))
                || (byte == b'#' && i > 0 && matches!(bytes[i - 1], b' ' | b'\t')))
            .then_some(i)
        })
        .unwrap_or(tail.len());
    let token = tail.get(..end)?.trim_end_matches([' ', '\t']);
    (!token.is_empty()).then_some(start + token.len())
}

#[cfg(test)]
mod tests {
    use super::{literal_range, yaml_safe_json};

    fn located(consts: &str, typed: bool) -> Option<String> {
        let source = format!("nika: x\nconst:\n{consts}\ntasks: {{}}\n");
        let (start, end) = literal_range(&source, "payload", typed)?;
        source.get(start..end).map(str::to_owned)
    }

    /// The public door cannot tell this guard from the whole-document
    /// comparison behind it; without the guard these return the NEXT token.
    #[test]
    fn an_omitted_value_has_no_range() {
        for omitted in [
            "  payload:\n  other: true",
            "  payload: # only a comment\n  \"other\": true",
            "  payload:\n  ~: true",
            "  other: true\n  payload:",
        ] {
            assert_eq!(located(omitted, false), None, "{omitted}");
        }
        let typed = "  payload:\n    type: string\n    value:\n  other: true";
        assert_eq!(located(typed, true), None);
    }

    #[test]
    fn written_nulls_and_empty_quotes_keep_their_exact_range() {
        for token in ["~", "null", "''", "\"\""] {
            let consts = format!("  payload: {token} # inline\n  other: true");
            assert_eq!(located(&consts, false).as_deref(), Some(token));
            let typed = format!("  payload:\n    type: string\n    value: {token}\n  other: 1");
            assert_eq!(located(&typed, true).as_deref(), Some(token));
        }
    }

    #[test]
    fn only_reader_hostile_code_points_are_escaped() {
        // An escaped backslash before the point must not swallow the new escape.
        for (c, hex) in [
            ('\u{7f}', "007f"),
            ('\u{80}', "0080"),
            ('\u{85}', "0085"),
            ('\u{9f}', "009f"),
            ('\u{fffe}', "fffe"),
            ('\u{ffff}', "ffff"),
        ] {
            let raw = format!("{{\"k{c}\":[\"\\\\{c}\"]}}");
            let safe = format!("{{\"k\\u{hex}\":[\"\\\\\\u{hex}\"]}}");
            assert_eq!(yaml_safe_json(&raw), safe);
            let decoded = serde_json::from_str::<serde_json::Value>(&raw).ok();
            assert!(decoded.is_some(), "{raw}");
            assert_eq!(serde_json::from_str(&safe).ok(), decoded);
        }
        // Neighbours of each range, BOM, a line separator and non-BMP stay raw.
        let kept = "[\"~\u{a0}\u{2028}\u{fffd}\u{feff}\u{10000}\",\"\\n\\u0001\"]";
        assert_eq!(yaml_safe_json(kept), kept);
    }
}
