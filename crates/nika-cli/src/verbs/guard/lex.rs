// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The guard's shell lexer — quotes honoured, expansions MARKED never
//! performed. Split out of `guard.rs` (2026-10-15): the judge reads the
//! tokens, this module only produces them.

/// The shell operators the lexer splits on — `Redirect` stays inside a
/// segment (its target is never the workflow), the rest separate the
/// simple commands of a `;`/`&&`/`||`/`|` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Op {
    And,
    Or,
    Seq,
    Pipe,
    Redirect,
}

/// One lexed token. `dynamic` marks an UNEXPANDED dynamic form (a
/// variable · a command substitution · a glob outside quotes) — the
/// guard judges what a run names, it never guesses an expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Tok {
    pub(super) text: String,
    pub(super) dynamic: bool,
    pub(super) op: Option<Op>,
}

/// One simple command between operators, with the operators that open
/// and close it (a `cd` inside a pipeline never reaches the parent
/// shell's cwd).
#[derive(Debug)]
pub(super) struct Seg {
    pub(super) toks: Vec<Tok>,
    pub(super) before: Option<Op>,
    pub(super) after: Option<Op>,
}

/// The shell tokenizer — quotes honoured, expansions MARKED never
/// performed (the exact class the regex hook's whitespace split got
/// wrong: a quoted path with spaces is ONE token; `$WF` is unknown,
/// not empty).
pub(super) fn lex(line: &str) -> Vec<Tok> {
    let chars: Vec<char> = line.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // `#` at token start opens a comment to end-of-line — a
        // commented `nika run` is text, not an execution.
        if c == '#' {
            break;
        }
        if let Some((op, len)) = operator_at(&chars, i) {
            toks.push(Tok {
                text: chars[i..i + len].iter().collect(),
                dynamic: false,
                op: Some(op),
            });
            i += len;
            continue;
        }
        let (tok, next) = lex_word(&chars, i);
        toks.push(tok);
        i = next;
    }
    toks
}

/// An operator starting at `i`, with its length — `>&2`/`2>>`-style
/// redirect clusters included.
fn operator_at(chars: &[char], i: usize) -> Option<(Op, usize)> {
    let c = chars[i];
    let two = chars.get(i + 1);
    match c {
        '&' if two == Some(&'&') => Some((Op::And, 2)),
        // A background `&` sequences just like `;` for the guard.
        '&' | ';' => Some((Op::Seq, 1)),
        '|' if two == Some(&'|') => Some((Op::Or, 2)),
        '|' => Some((Op::Pipe, 1)),
        '>' | '<' => {
            let mut j = i;
            while j < chars.len() && matches!(chars[j], '>' | '<' | '&' | '0'..='9') {
                j += 1;
            }
            Some((Op::Redirect, j - i))
        }
        _ => None,
    }
}

/// A single-quoted span from the quote at `i` — literal, never dynamic.
fn lex_single_quote(chars: &[char], mut i: usize, text: &mut String) -> usize {
    i += 1;
    while i < chars.len() && chars[i] != '\'' {
        text.push(chars[i]);
        i += 1;
    }
    (i + 1).min(chars.len())
}

/// A double-quoted span — `\` honoured, `$`/backtick still expand in a
/// real shell so the word is marked dynamic.
fn lex_double_quote(chars: &[char], mut i: usize, text: &mut String) -> (usize, bool) {
    let mut dynamic = false;
    i += 1;
    while i < chars.len() && chars[i] != '"' {
        if chars[i] == '\\' && i + 1 < chars.len() {
            text.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if matches!(chars[i], '$' | '`') {
            dynamic = true;
        }
        text.push(chars[i]);
        i += 1;
    }
    ((i + 1).min(chars.len()), dynamic)
}

/// A `$( … )` command substitution — one dynamic span, spaces included.
fn lex_dollar(chars: &[char], mut i: usize, text: &mut String) -> usize {
    if chars.get(i + 1) != Some(&'(') {
        text.push('$');
        return i + 1;
    }
    let mut depth = 0i32;
    while i < chars.len() {
        match chars[i] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            ch => text.push(ch),
        }
        i += 1;
    }
    i
}

/// A backtick command substitution — one dynamic span.
fn lex_backtick(chars: &[char], mut i: usize, text: &mut String) -> usize {
    i += 1;
    while i < chars.len() && chars[i] != '`' {
        text.push(chars[i]);
        i += 1;
    }
    (i + 1).min(chars.len())
}

/// One word from `start` — quotes delegated to the span helpers, unquoted
/// `$`/`*?[`/backticks marked dynamic. A trailing digit-run before
/// `>`/`<` folds into the redirect token (`2>/dev/null`).
fn lex_word(chars: &[char], start: usize) -> (Tok, usize) {
    let mut text = String::new();
    let mut dynamic = false;
    let mut i = start;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || matches!(c, '&' | '|' | ';' | '#') {
            break;
        }
        if c == '>' || c == '<' {
            if !text.is_empty() && text.chars().all(|d| d.is_ascii_digit()) {
                // `2>` — the fd prefix belongs to the redirect token.
                let (op, len) = operator_at(chars, i).unwrap_or((Op::Redirect, 1));
                text.push_str(&chars[i..i + len].iter().collect::<String>());
                return (
                    Tok {
                        text,
                        dynamic,
                        op: Some(op),
                    },
                    i + len,
                );
            }
            break;
        }
        match c {
            '\'' => i = lex_single_quote(chars, i, &mut text),
            '"' => {
                let (next, span_dynamic) = lex_double_quote(chars, i, &mut text);
                dynamic |= span_dynamic;
                i = next;
            }
            '\\' if i + 1 < chars.len() => {
                text.push(chars[i + 1]);
                i += 2;
            }
            '$' => {
                dynamic = true;
                i = lex_dollar(chars, i, &mut text);
            }
            '`' => {
                dynamic = true;
                i = lex_backtick(chars, i, &mut text);
            }
            '*' | '?' | '[' => {
                dynamic = true;
                text.push(c);
                i += 1;
            }
            _ => {
                text.push(c);
                i += 1;
            }
        }
    }
    (
        Tok {
            text,
            dynamic,
            op: None,
        },
        i,
    )
}

/// Split the token stream into simple commands, remembering the joining
/// operators (a `cd` only reaches segments the pipeline law allows).
pub(super) fn segments(toks: &[Tok]) -> Vec<Seg> {
    let mut out = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut before: Option<Op> = None;
    for t in toks {
        match t.op {
            Some(op) if op != Op::Redirect => {
                out.push(Seg {
                    toks: std::mem::take(&mut cur),
                    before,
                    after: Some(op),
                });
                before = Some(op);
            }
            _ => cur.push(t.clone()),
        }
    }
    out.push(Seg {
        toks: cur,
        before,
        after: None,
    });
    out
}

/// Redirects (`> log` · `2>/dev/null`) carry no command word — drop the
/// operator and its target.
pub(super) fn strip_redirects(toks: &[Tok]) -> Vec<&Tok> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < toks.len() {
        if toks[k].op == Some(Op::Redirect) {
            k += 1;
            if k < toks.len() && toks[k].op.is_none() {
                k += 1; // the target word
            }
            continue;
        }
        out.push(&toks[k]);
        k += 1;
    }
    out
}
