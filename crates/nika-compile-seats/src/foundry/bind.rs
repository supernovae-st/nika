// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Binding a component's holes: the bounded literal edit and the laws a binding is judged by
//! before it is applied.
//!
//! [`edit_literal`] replaces the literal a document holds at a dotted key path and keeps every
//! other byte. It locates nothing by YAML rules of its own: each span that writes the held
//! literal is a candidate, and a candidate is kept only when the parser's own literal projection
//! of the edited document ([`literal_projection`]) equals the document's projection with that
//! one value changed. A comment, another value that writes the same text, or a key is therefore
//! never edited; a presentation this cannot prove (a block scalar, a block mapping, an alias) is
//! refused, and so is a document where two spans would each prove the edit. This is the bounded
//! slice the document owner's typed edit will serve; nothing here adds a rule of the language.

use std::fmt;

use nika_compile::surface::literal_projection;
use nika_compile_fidelity::literal::{has_expression, literal_at};
use serde_json::Value;

use super::component::Component;

/// Why a bounded literal edit is not proven on a document.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditRefusal {
    /// The parser's literal projection does not read the document.
    NotADocument,
    /// The document holds no literal at the path: an edit never inserts a node.
    Absent(String),
    /// No span of the document is proven to write that literal: its presentation is outside
    /// the bounded slice (a block scalar, a block mapping, an alias).
    Unlocated(String),
    /// Several spans would each prove the edit: none is chosen.
    Ambiguous(String),
}

impl fmt::Display for EditRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotADocument => f.write_str("the document's literals cannot be read"),
            Self::Absent(path) => write!(f, "the document holds no literal at `{path}`"),
            Self::Unlocated(path) => write!(
                f,
                "the literal at `{path}` is presented in a form no bounded edit proves (a block scalar or mapping, an alias)"
            ),
            Self::Ambiguous(path) => write!(
                f,
                "several spans of the document would each prove the edit of `{path}`: none is chosen"
            ),
        }
    }
}

impl std::error::Error for EditRefusal {}

/// `source` with the literal at the dotted `path` replaced by `value`, every other byte kept. A
/// typed constant keeps its declaration (its `value:` is edited) and an input its type (its
/// `default:`), as the edit door's [`literal_at`] reads them. The value is written in its flow
/// form, a text always quoted.
///
/// # Errors
/// [`EditRefusal`]: no document, no literal there, or no single span proven to hold it.
pub fn edit_literal(source: &str, path: &str, value: &Value) -> Result<String, EditRefusal> {
    let before = literal_projection(source).ok_or(EditRefusal::NotADocument)?;
    let mut expected = before;
    let node = literal_at(&mut expected, path).ok_or_else(|| EditRefusal::Absent(path.into()))?;
    if node == value {
        return Ok(source.to_owned());
    }
    let replaced = std::mem::replace(node, value.clone());
    let written = flow(value);
    let mut proven: Vec<String> = Vec::new();
    for (start, end) in spans(source, &replaced) {
        let (Some(head), Some(tail)) = (source.get(..start), source.get(end..)) else {
            continue;
        };
        let edited = format!("{head}{written}{tail}");
        if !proven.contains(&edited) && literal_projection(&edited).as_ref() == Some(&expected) {
            proven.push(edited);
        }
    }
    match proven.len() {
        1 => Ok(proven.remove(0)),
        0 => Err(EditRefusal::Unlocated(path.to_owned())),
        _ => Err(EditRefusal::Ambiguous(path.to_owned())),
    }
}

/// A value's flow form: compact JSON, whose few YAML-hostile code points (DEL, the C1 controls,
/// the two BMP noncharacters) can only sit inside a string, where `\uXXXX` means the same to
/// both readers. The projection still proves the result.
fn flow(value: &Value) -> String {
    value
        .to_string()
        .chars()
        .map(|c| match c {
            '\u{7f}'..='\u{9f}' | '\u{fffe}' | '\u{ffff}' => format!("\\u{:04x}", u32::from(c)),
            _ => c.to_string(),
        })
        .collect()
}

/// The byte spans that may write `held`: each written form of a scalar, at token boundaries, or
/// each balanced flow collection for a list or an object. Candidates only: the projection
/// decides.
fn spans(source: &str, held: &Value) -> Vec<(usize, usize)> {
    match held {
        Value::Array(_) | Value::Object(_) => collections(source, held.is_array()),
        Value::String(text) => {
            let mut spans = plain(source, text);
            for quoted in [
                Value::String(text.clone()).to_string(),
                format!("'{}'", text.replace('\'', "''")),
            ] {
                spans.extend(occurrences(source, &quoted));
            }
            spans
        }
        Value::Null => ["null", "~"]
            .iter()
            .flat_map(|word| plain(source, word))
            .collect(),
        other => plain(source, &other.to_string()),
    }
}

/// Every occurrence of `text` in `source`, overlapping ones included.
fn occurrences(source: &str, text: &str) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(at) = source.get(from..).and_then(|rest| rest.find(text)) {
        let start = from + at;
        spans.push((start, start + text.len()));
        from = start + source[start..].chars().next().map_or(1, char::len_utf8);
    }
    spans
}

/// The occurrences of `text` that stand as a whole plain token: after a separator, before one.
fn plain(source: &str, text: &str) -> Vec<(usize, usize)> {
    let before = |c: char| c.is_whitespace() || ":,[{-".contains(c);
    let after = |c: char| c.is_whitespace() || ",]}#".contains(c);
    occurrences(source, text)
        .into_iter()
        .filter(|(start, end)| {
            source[..*start].chars().next_back().is_none_or(before)
                && source[*end..].chars().next().is_none_or(after)
        })
        .collect()
}

/// The spans of balanced flow collections, `[…]` (`list`) or `{…}`: a quote opens a scalar only
/// where a token starts, and a `#` after a blank opens a comment to the line's end.
fn collections(source: &str, list: bool) -> Vec<(usize, usize)> {
    let (open, close) = if list { ('[', ']') } else { ('{', '}') };
    let mut spans = Vec::new();
    let mut stack: Vec<(char, usize)> = Vec::new();
    let mut quote: Option<char> = None;
    let mut comment = false;
    let mut previous = '\n';
    let mut chars = source.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if comment {
            comment = c != '\n';
        } else if let Some(q) = quote {
            if q == '"' && c == '\\' {
                chars.next();
            } else if c == q {
                // `''` inside a single-quoted scalar is one quote, not its end.
                if q == '\'' && chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                } else {
                    quote = None;
                }
            }
        } else if (c == '"' || c == '\'')
            && (previous.is_whitespace() || ":,[{-".contains(previous))
        {
            quote = Some(c);
        } else if c == '#' && previous.is_whitespace() {
            comment = true;
        } else if c == '[' || c == '{' {
            stack.push((c, at));
        } else if (c == ']' || c == '}')
            && let Some((opened, start)) = stack.pop()
            && opened == open
            && c == close
        {
            spans.push((start, at + 1));
        }
        previous = c;
    }
    spans
}

/// One hole bound to a literal.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Binding {
    /// The key path bound: a hole, or a path under one (`inputs.company.default`).
    pub path: String,
    /// The literal it is bound to.
    pub value: Value,
}

impl Binding {
    /// `path` bound to `value`.
    #[must_use]
    pub fn new(path: impl Into<String>, value: Value) -> Self {
        Self {
            path: path.into(),
            value,
        }
    }
}

/// Why a binding is refused: before any edit, or by the bounded edit itself. Nothing of the
/// component is changed by a refused binding.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BindingError {
    /// No hole of the component covers the path: a component is bound only where its producer
    /// marked it open.
    UnknownHole(String),
    /// The component holds no literal at the path.
    Absent(String),
    /// The literal's kind is not the one the component holds there.
    Incompatible {
        /// The path bound.
        path: String,
        /// The kind the component holds.
        held: &'static str,
        /// The kind given.
        given: &'static str,
    },
    /// The literal carries an expression island: a binding authors no reference.
    Expression(String),
    /// Two bindings reach the same literal.
    Duplicate(String),
    /// Holes the expansion needs bound are open: a component's own literals there (its probe's
    /// paths, fields, defaults) are never taken for the request's.
    Unbound(Vec<String>),
    /// The bounded edit refused the literal.
    Unproven(EditRefusal),
}

impl fmt::Display for BindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownHole(path) => write!(f, "no hole of the component covers `{path}`"),
            Self::Absent(path) => write!(f, "the component holds no literal at `{path}`"),
            Self::Incompatible { path, held, given } => write!(
                f,
                "`{path}` holds a {held}; a {given} cannot be bound there"
            ),
            Self::Expression(path) => write!(
                f,
                "the literal bound at `{path}` carries an expression island: a binding authors no reference"
            ),
            Self::Duplicate(path) => write!(f, "`{path}` is bound twice"),
            Self::Unbound(holes) => write!(
                f,
                "open holes: {}; the component's own literals there are not the request's",
                holes.join(", ")
            ),
            Self::Unproven(refusal) => refusal.fmt(f),
        }
    }
}

impl std::error::Error for BindingError {}

/// A literal's kind, as the binding laws compare them.
#[must_use]
pub fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "text",
        Value::Array(_) => "list",
        Value::Object(_) => "object",
    }
}

/// Judge `bindings` against `component` before any edit: each path under a hole, held as a
/// literal, given in the held kind (an integer where a fraction is held is a number too), with
/// no expression island, and reached once.
///
/// # Errors
/// The first binding refused.
pub fn judge(component: &Component, bindings: &[Binding]) -> Result<(), BindingError> {
    let Some(mut document) = literal_projection(&component.source) else {
        return Err(BindingError::Unproven(EditRefusal::NotADocument));
    };
    let mut seen: Vec<&str> = Vec::new();
    for binding in bindings {
        let path = binding.path.as_str();
        if component.hole(path).is_none() {
            return Err(BindingError::UnknownHole(path.to_owned()));
        }
        if seen.iter().any(|other| overlaps(other, path)) {
            return Err(BindingError::Duplicate(path.to_owned()));
        }
        seen.push(path);
        let held =
            literal_at(&mut document, path).ok_or_else(|| BindingError::Absent(path.to_owned()))?;
        let (held, given) = (kind(held), kind(&binding.value));
        if held != given && !(held == "number" && given == "integer") {
            return Err(BindingError::Incompatible {
                path: path.to_owned(),
                held,
                given,
            });
        }
        if has_expression(&binding.value) {
            return Err(BindingError::Expression(path.to_owned()));
        }
    }
    Ok(())
}

/// Whether two dotted paths reach the same literal: one is the other or lies under it.
fn overlaps(a: &str, b: &str) -> bool {
    let under = |long: &str, short: &str| {
        long.strip_prefix(short)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    };
    under(a, b) || under(b, a)
}

/// The holes of `component` the bindings leave open, by name. A hole is closed when a binding
/// names it, or — a mapping hole such as `inputs` — when every entry of it that holds a literal
/// (an input's default) is bound: one bound input never closes the others' probe defaults.
#[must_use]
pub fn open_holes(component: &Component, bindings: &[Binding]) -> Vec<String> {
    let document = literal_projection(&component.source).unwrap_or(Value::Null);
    let bound = |path: &str| bindings.iter().any(|b| overlaps(&b.path, path));
    let closed = |name: &str| {
        if bindings.iter().any(|b| b.path == name) {
            return true;
        }
        let node = name
            .split('.')
            .try_fold(&document, |node, key| node.get(key));
        let Some(Value::Object(entries)) = node else {
            return false;
        };
        entries.keys().all(|key| {
            let path = format!("{name}.{key}");
            literal_at(&mut document.clone(), &path).is_none() || bound(&path)
        })
    };
    component
        .holes
        .iter()
        .filter(|hole| !closed(&hole.name))
        .map(|hole| hole.name.clone())
        .collect()
}
