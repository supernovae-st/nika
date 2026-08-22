// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `jq-as-map` — `. as $c` then a bare `map(` maps the CURRENT value.
//!
//! Split from `hints.rs` at the 1500-LOC ceiling so the field-pipe
//! admission (issue 1038 remainder) has somewhere to live.

use super::{Hint, hint};
use nika_schema::raw::RawInvokeAction;

/// `. as $c` then a bare `map(` maps the CURRENT value (often a pair),
/// not `$c`. The jury-jq class: `($c | map(...))`.
pub(super) fn push_jq_as_map_hint(hints: &mut Vec<Hint>, id: &str, a: &RawInvokeAction) {
    let Some(tool) = a.tool() else {
        return;
    };
    if tool.value != "nika:jq" {
        return;
    }
    let Some(expr) = a
        .args
        .as_ref()
        .and_then(|args| args.value.get("expression"))
        .and_then(|v| v.as_str())
    else {
        return;
    };
    if !jq_maps_the_current_after_bind(expr) {
        return;
    }
    hints.push(hint(
        "jq-as-map",
        id,
        format!(
            "`nika:jq` on `{id}` binds `. as $name` then calls `map(` on the current \
             value — after a later construct the current value is often a pair, not \
             the bound array. Write `($name | map(...))`"
        ),
    ));
}

/// `. as $c | map(...)` maps the *current* value. `($c | map(...))` is
/// the one-way. A one-liner used to slip past the line-start detector.
fn jq_maps_the_current_after_bind(expr: &str) -> bool {
    // The DOT binding is what puts the current value in question, so it stays
    // the trigger: no `. as $name`, no hint.
    if bound_jq_names(expr).is_empty() || !expr.contains("map(") {
        return false;
    }
    // ANY bound name names its input out loud. A law that walks several
    // bindings (`($entries | map(...)) as $rows | ($rows | map(...))`) already
    // does exactly what the advice prescribes, and stripping only the
    // dot-bound name reported it as a defect (measured 2026-08-19).
    let mut rest = squash_ws(expr);
    for name in &all_bound_jq_names(expr) {
        rest = rest.replace(&format!("(${name} | map("), "");
        rest = rest.replace(&format!("(${name}|map("), "");
    }
    // A map piped from a field access on the current value (`.paths | map(`)
    // re-navigates, so map receives that array — the same class as a map
    // piped from a bound name (issue 1038 remainder).
    rest = erase_field_piped_maps(&rest);
    rest.contains("map(")
}

/// Drop `.IDENT | map(` / `(.IDENT | map(` so a remaining `map(` is bare.
fn erase_field_piped_maps(s: &str) -> String {
    let mut rest = s.to_owned();
    while let Some((start, end)) = find_field_piped_map(&rest) {
        rest.replace_range(start..end, "");
    }
    rest
}

fn find_field_piped_map(s: &str) -> Option<(usize, usize)> {
    let mut i = 0usize;
    while i < s.len() {
        let rest = &s[i..];
        let paren = rest.starts_with('(');
        let body = if paren { &rest[1..] } else { rest };
        if let Some(after_dot) = body.strip_prefix('.') {
            let name_len = after_dot
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .count();
            if name_len > 0 {
                let after_name = &after_dot[name_len..];
                let tail = if after_name.starts_with(" | map(") {
                    " | map(".len()
                } else if after_name.starts_with("|map(") {
                    "|map(".len()
                } else {
                    0
                };
                if tail > 0 {
                    let consumed = usize::from(paren) + 1 + name_len + tail;
                    return Some((i, i + consumed));
                }
            }
        }
        i += s[i..].chars().next()?.len_utf8();
    }
    None
}

/// Every `as $NAME` binding, dot-bound or not. A `map(` piped from one of
/// them is explicit; only a BARE `map(` rides the current value.
fn all_bound_jq_names(expr: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = expr;
    while let Some(at) = rest.find(" as $") {
        let after = &rest[at + 5..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            rest = after;
            continue;
        }
        let skip = name.len();
        names.push(name);
        rest = after.get(skip..).unwrap_or("");
    }
    names
}

fn bound_jq_names(expr: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = expr;
    while let Some(at) = rest.find(". as $") {
        let after = &rest[at + 6..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            rest = after;
            continue;
        }
        let skip = name.len();
        names.push(name);
        rest = after.get(skip..).unwrap_or("");
    }
    names
}

fn squash_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut gap = false;
    for c in s.chars() {
        if c.is_whitespace() {
            gap = true;
        } else {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(c);
        }
    }
    out
}
