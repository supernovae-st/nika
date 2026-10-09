// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Renaming a named entity together with every reference its syntactic
//! owners hold: the identifier tokens of the `${{ }}` islands that read it,
//! the `after:` keys that order on a task, the `lift.from` bindings that
//! name it. Text outside an island is never touched, so prose, a command or
//! data that merely spells the name stays as written. Each rewritten island
//! is parsed again and must be the old expression with exactly the renamed
//! chain changed; anything else is refused.

use nika_tmpl::expression::{Expr, Literal, parse_expression};
use serde_json::Value;

use super::{Path, Refusal};

/// The rewrites a rename makes: string values first (their paths still
/// read the old names), then mapping keys, the entity's own key last.
#[derive(Debug, Default)]
pub(super) struct Plan {
    /// String values whose references change, with their new text.
    pub(super) strings: Vec<(Path, String)>,
    /// Mapping keys renamed in place, with their new key.
    pub(super) keys: Vec<(Path, String)>,
}

/// How references reach a renamed entry: the chain before its name, and the
/// subtree they may sit in (a `with:` binding is read by its own task only).
struct Chain {
    prefix: Vec<String>,
    scope: Path,
}

fn chain(path: &Path) -> Option<Chain> {
    let segments: Vec<&str> = path.segments().iter().map(String::as_str).collect();
    let whole = Path::root();
    let (prefix, scope): (Vec<&str>, Path) = match segments.as_slice() {
        ["tasks", _] => (vec!["tasks"], whole),
        [root @ ("inputs" | "const" | "secrets"), _] => (vec![*root], whole),
        ["tasks", id, "with", _] => (vec!["with"], Path::new(["tasks", *id])),
        ["tasks", id, "extract", _] => (vec!["tasks", *id], whole),
        _ => return None,
    };
    Some(Chain {
        prefix: prefix.into_iter().map(str::to_owned).collect(),
        scope,
    })
}

/// The rewrites renaming the entry at `path` to `to` makes in `literal`.
///
/// # Errors
/// The entry is absent, the new key is taken, or a reference cannot be
/// rewritten exactly (an island that does not parse, a computed index).
pub(super) fn plan(literal: &Value, path: &Path, to: &str) -> Result<Plan, Refusal> {
    let shape = |detail: String| Refusal::Shape {
        path: path.clone(),
        detail,
    };
    let (Some(parent), Some(old)) = (path.parent(), path.last()) else {
        return Err(shape("the document root has no name".to_owned()));
    };
    let entries = literal
        .pointer(&parent.to_pointer())
        .and_then(Value::as_object)
        .ok_or_else(|| shape("only a mapping entry carries a name".to_owned()))?;
    if !entries.contains_key(old) {
        return Err(Refusal::UnknownPath { path: path.clone() });
    }
    if old == to {
        return Ok(Plan::default());
    }
    if entries.contains_key(to) {
        return Err(shape(format!("the mapping already has the key `{to}`")));
    }
    let mut plan = Plan::default();
    if let Some(chain) = chain(path) {
        let mut leaves = Vec::new();
        strings(literal, &Path::root(), &mut leaves);
        for (at, text) in leaves
            .into_iter()
            .filter(|(at, _)| at.starts_with(&chain.scope))
        {
            let lift = matches!(at.segments(), [t, _, l, _, f] if t == "tasks" && l == "lift" && f == "from");
            let rewritten = if lift {
                dotted(&text, &chain.prefix, old, to)
            } else {
                islands(&text, &chain.prefix, old, to).map_err(|detail| Refusal::Layout {
                    path: at.clone(),
                    detail,
                })?
            };
            if let Some(text) = rewritten {
                plan.strings.push((at, text));
            }
        }
        if chain.prefix == ["tasks"] {
            for (id, task) in literal["tasks"].as_object().into_iter().flatten() {
                if task
                    .pointer("/after")
                    .and_then(Value::as_object)
                    .is_some_and(|a| a.contains_key(old))
                {
                    plan.keys.push((
                        Path::new(["tasks", id.as_str(), "after", old]),
                        to.to_owned(),
                    ));
                }
            }
        }
    }
    plan.keys.push((path.clone(), to.to_owned()));
    Ok(plan)
}

/// Every string leaf of `value` with its path.
fn strings(value: &Value, at: &Path, out: &mut Vec<(Path, String)>) {
    match value {
        Value::String(text) => out.push((at.clone(), text.clone())),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                strings(item, &at.child(i.to_string()), out);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                strings(item, &at.child(key.as_str()), out);
            }
        }
        _ => {}
    }
}

/// A `lift.from` dotted binding with the renamed segment replaced.
fn dotted(text: &str, prefix: &[String], old: &str, to: &str) -> Option<String> {
    let mut segments: Vec<&str> = text.split('.').collect();
    let named = segments.len() > prefix.len()
        && segments.iter().zip(prefix).all(|(s, p)| s == p)
        && segments[prefix.len()] == old;
    named.then(|| {
        segments[prefix.len()] = to;
        segments.join(".")
    })
}

/// `text` with the renamed chain rewritten inside each `${{ }}` island;
/// `None` when no island reads it.
fn islands(text: &str, prefix: &[String], old: &str, to: &str) -> Result<Option<String>, String> {
    let Ok(found) = nika_tmpl::scan_islands(text) else {
        // An unterminated island is the parser's to refuse; it reads nothing.
        return Ok(None);
    };
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for island in found {
        let Some(body) = rewrite_body(island.body, prefix, old, to)? else {
            continue;
        };
        let before = parse_expression(island.body.trim())
            .map_err(|e| format!("an island that reads the name does not parse: {e}"))?;
        let after = parse_expression(body.trim())
            .map_err(|e| format!("the rewritten island does not parse: {e}"))?;
        if after != rename_expr(&before, prefix, old, to) {
            return Err("an island would read differently beyond the rename".to_owned());
        }
        out.push_str(text.get(last..island.body_start).unwrap_or_default());
        out.push_str(&body);
        last = island.body_start + island.body.len();
    }
    if last == 0 {
        return Ok(None);
    }
    out.push_str(text.get(last..).unwrap_or_default());
    Ok(Some(out))
}

/// One lexical token of an island body: what it is and where.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tok {
    Ident,
    Str,
    Dot,
    Open,
    Close,
    Other,
}

/// The tokens of an island body, string literals skipped whole.
fn lex(body: &str) -> Result<Vec<(Tok, usize, usize)>, String> {
    let b = body.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        let start = i;
        let tok = match c {
            b' ' | b'\t' | b'\n' | b'\r' => {
                i += 1;
                continue;
            }
            b'\'' | b'"' => {
                i += 1;
                loop {
                    match b.get(i) {
                        Some(b'\\') => i += 2,
                        Some(&q) if q == c => break,
                        Some(_) => i += 1,
                        None => return Err("an unterminated string in an island".to_owned()),
                    }
                }
                i += 1;
                Tok::Str
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                while matches!(
                    b.get(i),
                    Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_')
                ) {
                    i += 1;
                }
                Tok::Ident
            }
            b'0'..=b'9' => {
                while matches!(b.get(i), Some(b'0'..=b'9' | b'.' | b'e' | b'E')) {
                    i += 1;
                }
                Tok::Other
            }
            b'.' => {
                i += 1;
                Tok::Dot
            }
            b'[' => {
                i += 1;
                Tok::Open
            }
            b']' => {
                i += 1;
                Tok::Close
            }
            _ => {
                i += body
                    .get(i..)
                    .and_then(|r| r.chars().next())
                    .map_or(1, char::len_utf8);
                Tok::Other
            }
        };
        out.push((tok, start, i));
    }
    Ok(out)
}

/// The island body with every `prefix.old` chain renamed (the member and
/// the string-index forms); `None` when no chain names it.
fn rewrite_body(
    body: &str,
    prefix: &[String],
    old: &str,
    to: &str,
) -> Result<Option<String>, String> {
    let toks = lex(body)?;
    let text = |k: usize| toks.get(k).and_then(|&(_, s, e)| body.get(s..e));
    let is = |k: usize, tok: Tok| toks.get(k).is_some_and(|t| t.0 == tok);
    // The segment at token `k` names `name`: an identifier after a dot, or a
    // string inside brackets; the next token index follows it.
    let segment = |k: usize, name: &str| -> Option<(usize, (usize, usize))> {
        if is(k, Tok::Dot) && is(k + 1, Tok::Ident) && text(k + 1) == Some(name) {
            let (_, s, e) = toks[k + 1];
            return Some((k + 2, (s, e)));
        }
        let quoted = text(k + 1).and_then(|t| t.get(1..t.len().saturating_sub(1)));
        if is(k, Tok::Open) && is(k + 1, Tok::Str) && quoted == Some(name) && is(k + 2, Tok::Close)
        {
            let (_, s, e) = toks[k + 1];
            return Some((k + 3, (s + 1, e - 1)));
        }
        None
    };
    let mut edits = Vec::new();
    for k in 0..toks.len() {
        let root = is(k, Tok::Ident)
            && text(k) == prefix.first().map(String::as_str)
            && (k == 0 || !is(k - 1, Tok::Dot));
        if !root {
            continue;
        }
        let mut next = k + 1;
        let mut matched = true;
        for part in prefix.iter().skip(1) {
            if let Some((after, _)) = segment(next, part) {
                next = after;
            } else {
                matched = false;
                break;
            }
        }
        if let Some((_, span)) = segment(next, old).filter(|_| matched) {
            edits.push(span);
        }
    }
    if edits.is_empty() {
        return Ok(None);
    }
    let mut out = body.to_owned();
    for (s, e) in edits.into_iter().rev() {
        out.replace_range(s..e, to);
    }
    Ok(Some(out))
}

/// The chain the expression `e` names, when it is a pure access chain.
fn pure_chain(e: &Expr) -> Option<Vec<String>> {
    match e {
        Expr::Ident(name) => Some(vec![name.clone()]),
        Expr::Member { base, field } => pure_chain(base).map(|mut c| {
            c.push(field.clone());
            c
        }),
        Expr::Index { base, index } => match index.as_ref() {
            Expr::Lit(Literal::Str(s)) => pure_chain(base).map(|mut c| {
                c.push(s.clone());
                c
            }),
            _ => None,
        },
        _ => None,
    }
}

/// `e` with every `prefix.old` chain renamed `prefix.to`, the rest kept.
fn rename_expr(e: &Expr, prefix: &[String], old: &str, to: &str) -> Expr {
    let go = |x: &Expr| Box::new(rename_expr(x, prefix, old, to));
    let named = |base: &Expr| pure_chain(base).as_deref() == Some(prefix);
    match e {
        Expr::Or(a, b) => Expr::Or(go(a), go(b)),
        Expr::And(a, b) => Expr::And(go(a), go(b)),
        Expr::Not(a) => Expr::Not(go(a)),
        Expr::Relation { op, lhs, rhs } => Expr::Relation {
            op: *op,
            lhs: go(lhs),
            rhs: go(rhs),
        },
        Expr::Ternary { cond, then, else_ } => Expr::Ternary {
            cond: go(cond),
            then: go(then),
            else_: go(else_),
        },
        Expr::SizeCall(a) => Expr::SizeCall(go(a)),
        Expr::HasCall(a) => Expr::HasCall(go(a)),
        Expr::StringMethod { base, method, arg } => Expr::StringMethod {
            base: go(base),
            method: *method,
            arg: go(arg),
        },
        Expr::SizeMethod(a) => Expr::SizeMethod(go(a)),
        Expr::Member { base, field } => Expr::Member {
            field: if named(base) && field == old {
                to.to_owned()
            } else {
                field.clone()
            },
            base: go(base),
        },
        Expr::Index { base, index } => Expr::Index {
            index: match index.as_ref() {
                Expr::Lit(Literal::Str(s)) if named(base) && s == old => {
                    Box::new(Expr::Lit(Literal::Str(to.to_owned())))
                }
                _ => go(index),
            },
            base: go(base),
        },
        Expr::List(items) => Expr::List(
            items
                .iter()
                .map(|x| rename_expr(x, prefix, old, to))
                .collect(),
        ),
        Expr::Ident(_) | Expr::Lit(_) => e.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{islands, rewrite_body};

    fn tasks() -> Vec<String> {
        vec!["tasks".to_owned()]
    }

    #[test]
    fn only_reference_tokens_change() {
        let text = "see tasks.fetch.output ${{ tasks.fetch.output }} and ${{ 'tasks.fetch' + with.tasks.fetch }} ${{ tasks.fetch_all.output }}";
        let out = islands(text, &tasks(), "fetch", "grab").expect("rewrites");
        assert_eq!(
            out.as_deref(),
            Some(
                "see tasks.fetch.output ${{ tasks.grab.output }} and ${{ 'tasks.fetch' + with.tasks.fetch }} ${{ tasks.fetch_all.output }}"
            )
        );
    }

    #[test]
    fn the_string_index_form_is_renamed_too() {
        assert_eq!(
            rewrite_body(
                " tasks['fetch'].status == 'success' ",
                &tasks(),
                "fetch",
                "grab"
            )
            .expect("lexes")
            .as_deref(),
            Some(" tasks['grab'].status == 'success' ")
        );
    }

    #[test]
    fn a_text_without_the_reference_is_left_alone() {
        assert_eq!(
            islands("${{ inputs.fetch }}", &tasks(), "fetch", "grab"),
            Ok(None)
        );
        assert_eq!(islands("no island", &tasks(), "fetch", "grab"), Ok(None));
    }
}
