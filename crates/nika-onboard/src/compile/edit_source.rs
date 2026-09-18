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
    let options = LoaderOptions::default()
        .error_on_duplicate_keys(true)
        .prevent_coercion(true);
    let tree = parse_yaml_with_options(0, source, options).ok()?;
    let consts = tree.as_mapping()?.get_node("const")?.as_mapping()?;
    let mut node = consts.get_node(name)?;
    let mut parent = tree.as_mapping()?.get_node("const")?;
    let declaration = before.get("const")?.get(name)?;
    let mut replacement = after.get("const")?.get(name)?;
    if declaration.get("type").is_some() && declaration.get("value").is_some() {
        parent = node;
        node = node.as_mapping()?.get_node("value")?;
        replacement = replacement.get("value")?;
    }
    let start = byte_offset(source, node.span().start()?.character())?;
    let parent_start = byte_offset(source, parent.span().start()?.character())?;
    let flow = source.as_bytes().get(parent_start) == Some(&b'{');
    let end = match node {
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
    let prefix = source.get(..start)?;
    let suffix = source.get(end..)?;
    // YAML 1.1 folds a raw NEL into a space even inside JSON quotes. Escape it
    // so the next EDIT's secondary decoder reads the same value, including keys.
    let replacement = replacement.to_string().replace('\u{85}', "\\u0085");
    let candidate = format!("{prefix}{replacement}{suffix}");
    // Validate both readers on the OUTPUT too: acceptance must not manufacture
    // decoder drift that prevents a later unrelated edit of this candidate.
    (serde_yaml_bw::from_str::<Value>(&candidate).ok().as_ref() == Some(after)
        && super::edit::literal_projection(&candidate).as_ref() == Some(after))
    .then_some(candidate)
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
