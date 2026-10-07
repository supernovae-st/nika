// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The JSON objects of a seat's text: the one answer a seat wrapped in prose or a fence, the
//! competing answers beside it, and the group a syntax diagnostic targets. Owned with the seats
//! that answer in text since 2026-10-07 (the ADR-146 descent); the seats' doors keep these
//! names at their historical paths.

use nika_compile::{CompileOutcome, surface::sha256};
use serde_json::{Map, Value, json};

/// The first complete JSON object of a seat's text — the text itself when it is one, else
/// the balanced `{…}` it carries (a seat that wraps its answer in prose or a fence is not a
/// lost call). None when the text carries no balanced object.
#[must_use]
pub fn first_json_object(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed);
    }
    balanced_object(text, 0)?.ok().map(|range| &text[range])
}

/// The unclosed braces a scan retries after, before it stops judging.
pub const UNCLOSED_RETRIES: usize = 64;

/// What the complete JSON objects of a seat's text come to, judged by the answer's own shape.
pub enum Objects<'a> {
    /// No complete, non-empty JSON object: the text keeps its syntax path.
    None,
    /// The one answer to read (the first object when none is an answer), and the objects
    /// beside it that cannot be answers: kept by digest, never read.
    One {
        answer: &'a str,
        unread: Vec<&'a str>,
    },
    /// Two or more different answers, which the journal keeps by digest: none is read.
    Two(Vec<&'a str>),
    /// An object that never closes beside a complete one, or unclosed braces past the retry
    /// bound: undecided, never read as one answer. The complete objects, by digest.
    Undecided(Vec<&'a str>),
}

/// Every complete JSON object of a seat's text, an identical repetition once; prose, template
/// braces and empty objects are skipped. `is_answer` is the answer's own shape: two answers are
/// never resolved by reading the first, and an example that cannot be one never kills it.
#[must_use]
pub fn answer_objects(text: &str, is_answer: impl Fn(&str) -> bool) -> Objects<'_> {
    let mut objects: Vec<&str> = Vec::new();
    let (mut from, mut retries, mut undecided) = (0, 0, false);
    while let Some(group) = balanced_object(text, from) {
        match group {
            Ok(range) => {
                let object = &text[range.clone()];
                let empty = object[1..object.len() - 1].trim().is_empty();
                if !empty
                    && !objects.contains(&object)
                    && serde_json::from_str::<serde::de::IgnoredAny>(object).is_ok()
                {
                    objects.push(object);
                }
                from = range.end;
            }
            Err(start) => {
                // A brace that opens like an object (`{` then `"`) and never closes may be a
                // cut answer.
                undecided |= text[start + 1..].trim_start().starts_with('"');
                retries += 1;
                if retries > UNCLOSED_RETRIES {
                    undecided = true;
                    break;
                }
                from = start + 1;
            }
        }
    }
    let Some(&first) = objects.first() else {
        return Objects::None;
    };
    if undecided {
        return Objects::Undecided(objects);
    }
    let answers: Vec<&str> = objects
        .iter()
        .copied()
        .filter(|object| is_answer(object))
        .collect();
    let answer = match answers.as_slice() {
        [] => first,
        [answer] => answer,
        _ => return Objects::Two(answers),
    };
    let unread = objects
        .into_iter()
        .filter(|object| *object != answer)
        .collect();
    Objects::One { answer, unread }
}

/// Whether an object is shaped like an answer of type `T`: it decodes as one, or it carries one
/// of the `keys` only such an answer carries. A defect (an unknown key, a null field) never
/// turns a competing answer into an example.
#[must_use]
pub fn answer_shaped<T: serde::de::DeserializeOwned>(object: &str, keys: &[&str]) -> bool {
    serde_json::from_str::<T>(object).is_ok()
        || serde_json::from_str::<Map<String, Value>>(object)
            .is_ok_and(|map| keys.iter().any(|key| map.contains_key(*key)))
}

/// The group the syntax path judges when no complete object is JSON: the first closed brace
/// group that opens like a JSON object (`{` then `"`), so template or prose braces beside a
/// broken answer are never the target of its diagnostic. None when no group opens so.
#[must_use]
pub fn syntax_target(text: &str) -> Option<&str> {
    let (mut from, mut retries) = (0, 0);
    while let Some(group) = balanced_object(text, from) {
        match group {
            Ok(range) if text[range.start + 1..].trim_start().starts_with('"') => {
                return Some(&text[range]);
            }
            Ok(range) => from = range.end,
            Err(start) if retries < UNCLOSED_RETRIES => {
                retries += 1;
                from = start + 1;
            }
            Err(_) => return None,
        }
    }
    None
}

/// The balanced `{…}` opening at the first `{` at or after byte `from` (braces inside JSON
/// strings ignored), as a byte range; `Err(start)` when that brace never closes, None when no
/// brace opens.
fn balanced_object(text: &str, from: usize) -> Option<Result<std::ops::Range<usize>, usize>> {
    let start = from + text.get(from..)?.find('{')?;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in text[start..].char_indices() {
        if in_string {
            match ch {
                '\\' if !escaped => {
                    escaped = true;
                    continue;
                }
                '"' if !escaped => in_string = false,
                _ => {}
            }
            escaped = false;
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(Ok(start..start + i + ch.len_utf8()));
                }
            }
            _ => {}
        }
    }
    Some(Err(start))
}

/// Objects by digest and length, as the journals keep them.
#[must_use]
pub fn digests(objects: &[&str]) -> Value {
    objects
        .iter()
        .map(|object| json!({"sha256": sha256(object), "bytes": object.len()}))
        .collect()
}

/// Objects of a seat's text recorded on the call that returned them, under `field`: the unread
/// ones beside an answer, the competitors of a refused text. Never read, never silent.
pub fn record_objects(out: &mut CompileOutcome, field: &str, objects: &[&str]) {
    if objects.is_empty() {
        return;
    }
    if let Some(call) = out
        .provenance
        .authoring
        .as_mut()
        .and_then(|receipt| receipt.context.last_mut())
    {
        call[field] = digests(objects);
    }
}
