// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The source facts a semantic record's candidate is recorded to rely on (C9 · F4): a bounded
//! slice, never a dataflow analysis. A semantic candidate's programs are the seat's own jq
//! (`expression` fills), so no typed rule grounds its keys; what can be recorded is narrower:
//! each key the host observed in a source the sketch declares it reads (one row, by its exact
//! path or as the one file a bare stated name places) that a jq fill names as a literal path
//! segment (`.key` or `."key"` begun by no other path, or an object's shorthand entry `{key}`,
//! outside strings and comments), graded by the one grounding law over the observation the round
//! read.
//!
//! What it is not: a key read through a computed access (`.[$k]`, `.["key"]`, `getpath`), a
//! key no observation shows, a source read through a glob, an unplaced or ambiguous name or
//! several rows, are not recorded, and a candidate none of whose reads is recognized records
//! nothing (its basis stays unjudged by name, never presented as fresh). A literal segment is
//! recorded for every read source that shows the key, whichever the program reads it from, and
//! an output name that equals an observed key is recorded too: extra facts that can only
//! withdraw a proposal whose source moved, never admit one.

use serde_json::Value;

use super::{Entry, grade, row, seen_in};

/// What binds a recorded semantic fact: the candidate reads the key literally.
pub const BOUND_BY: &str = "semantic_reads";

/// The rule a recorded semantic fact names: the candidate's own programs.
const RULE: &str = "the semantic candidate's jq";

/// The grounding entries (`decision.grounding`) of the keys `record`'s jq fills read literally
/// from the sources its sketch declares, over `world`, the observation its round read; empty
/// when none is recognized.
#[must_use]
pub fn facts(record: &Value, world: Option<&Value>) -> Vec<Value> {
    let mut read: Vec<String> = Vec::new();
    let fills = record["fills"].as_array().into_iter().flatten();
    for jq in fills
        .filter(|fill| fill["field"] == "expression")
        .filter_map(|fill| fill["value"].as_str())
    {
        for key in literal_keys(jq) {
            if !read.contains(&key) {
                read.push(key);
            }
        }
    }
    let mut facts = Vec::new();
    for source in sources(record, world) {
        let row = row(world, &source);
        let Some(seen) = seen_in(world, row) else {
            continue;
        };
        for key in seen.all.iter().filter(|key| read.contains(key)) {
            let (grade, everywhere) = grade(key, Some(&seen), &[]);
            let entry = Entry {
                rule: RULE,
                key,
                source: &source,
                row,
                grade,
                everywhere,
                bound_by: Some(BOUND_BY),
            };
            facts.push(entry.to_json());
        }
    }
    facts
}

/// The sources the sketch declares its tasks read, each the path of its one observation row:
/// the exact path, or the one file a bare stated name places; a glob, an unplaced or ambiguous
/// name, or several rows, none.
fn sources(record: &Value, world: Option<&Value>) -> Vec<String> {
    let tasks = record["sketch"]["tasks"].as_array().into_iter().flatten();
    let reads = tasks.flat_map(|task| task["reads"].as_array().into_iter().flatten());
    let mut found: Vec<String> = Vec::new();
    for read in reads.filter_map(Value::as_str) {
        let path = row(world, read)
            .and_then(|row| row["path"].as_str().map(str::to_owned))
            .or_else(|| crate::fidelity::placed(world, read));
        if let Some(path) = path
            && !found.contains(&path)
        {
            found.push(path);
        }
    }
    found
}

/// The keys a jq program names as literal path segments that no other path begins: `.key`,
/// `."key"` or an object's shorthand entry, outside string literals (whose interpolations are code) and comments. A segment
/// after a key, an index, a call, a variable or `?` is a nested path, and a computed access is
/// not read; no jq is parsed.
pub(crate) fn literal_keys(jq: &str) -> Vec<String> {
    let chars: Vec<char> = jq.chars().collect();
    let mut keys = Vec::new();
    code(&chars, 0, false, &mut keys);
    keys
}

/// Scan code from `at`; inside an interpolation (`nested`), up to its closing parenthesis.
/// Returns the index after what was scanned. An object's shorthand entry (`{sku, stock}`) reads
/// its key as `.key` does.
fn code(chars: &[char], mut at: usize, nested: bool, keys: &mut Vec<String>) -> usize {
    let mut open: Vec<char> = Vec::new();
    let mut before: Option<char> = None;
    while let Some(&c) = chars.get(at) {
        match c {
            '#' => {
                while chars.get(at).is_some_and(|&c| c != '\n') {
                    at += 1;
                }
                continue;
            }
            '"' => {
                at = string(chars, at + 1, keys).0;
                before = Some('"');
                continue;
            }
            '(' | '[' | '{' => open.push(c),
            ')' if nested && open.is_empty() => return at + 1,
            ')' | ']' | '}' => {
                open.pop();
            }
            '.' if begins(before) => {
                if let Some((key, next)) = segment(chars, at + 1, keys) {
                    keys.push(key);
                    at = next;
                    before = Some('k');
                    continue;
                }
            }
            c if (c.is_ascii_alphabetic() || c == '_')
                && open.last() == Some(&'{')
                && matches!(before, Some('{' | ',')) =>
            {
                let (word, next) = identifier(chars, at);
                let after = (next..chars.len()).find(|&i| !chars[i].is_whitespace());
                if after.is_some_and(|i| matches!(chars[i], ',' | '}')) {
                    keys.push(word);
                }
                at = next;
                before = Some('k');
                continue;
            }
            _ => {}
        }
        if !c.is_whitespace() {
            before = Some(c);
        }
        at += 1;
    }
    at
}

/// Whether a `.` after `before` begins a path: nothing, an operator or an opening bracket; never
/// after a key, an index, a call, a string, a variable, a format, a number or `?`.
fn begins(before: Option<char>) -> bool {
    before.is_none_or(|c| {
        !(c.is_alphanumeric() || matches!(c, '_' | '.' | ')' | ']' | '}' | '"' | '?' | '$' | '@'))
    })
}

/// The key a path segment starting at `at` names: an identifier, or a string without
/// interpolation; with the index after it.
fn segment(chars: &[char], at: usize, keys: &mut Vec<String>) -> Option<(String, usize)> {
    match chars.get(at) {
        Some(&c) if c.is_ascii_alphabetic() || c == '_' => Some(identifier(chars, at)),
        Some('"') => match string(chars, at + 1, keys) {
            (end, Some(text)) => Some((text, end)),
            (_, None) => None,
        },
        _ => None,
    }
}

/// The identifier starting at `at`, and the index after it.
fn identifier(chars: &[char], at: usize) -> (String, usize) {
    let end = (at..chars.len())
        .find(|&i| !(chars[i].is_ascii_alphanumeric() || chars[i] == '_'))
        .unwrap_or(chars.len());
    (chars[at..end].iter().collect(), end)
}

/// Skip a string literal whose body starts at `at`: the index after its closing quote, and its
/// text when it holds no interpolation (whose code is scanned) and is closed.
fn string(chars: &[char], mut at: usize, keys: &mut Vec<String>) -> (usize, Option<String>) {
    let mut text = Some(String::new());
    while let Some(&c) = chars.get(at) {
        match c {
            '"' => return (at + 1, text),
            '\\' if chars.get(at + 1) == Some(&'(') => {
                at = code(chars, at + 2, true, keys);
                text = None;
                continue;
            }
            '\\' => {
                let escaped = chars.get(at + 1).copied();
                if let (Some(text), Some(e)) = (text.as_mut(), escaped) {
                    match e {
                        '"' | '\\' | '/' => text.push(e),
                        _ => {
                            // Another escape spells a key this law does not decode.
                            at += 2;
                            return (skip_rest(chars, at, keys), None);
                        }
                    }
                }
                at += 2;
                continue;
            }
            _ => {
                if let Some(text) = text.as_mut() {
                    text.push(c);
                }
            }
        }
        at += 1;
    }
    (at, None)
}

/// The rest of a string whose text is not kept.
fn skip_rest(chars: &[char], at: usize, keys: &mut Vec<String>) -> usize {
    let (end, _) = string(chars, at, keys);
    end
}
