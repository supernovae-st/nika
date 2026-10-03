// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! JSON, bounded. A lexer validates the bytes read and a printer lays them
//! out without building a tree: nesting costs one byte of stack per level
//! and never a recursion, so a document ten thousand levels deep is as
//! safe as a flat one. Keys wear the accent, punctuation is dim, and a
//! document too long for the body is summarised by the engine's own shape
//! (`nika_display::shape::summarize`). JSON Patch operations (RFC 6902)
//! and merge patches (RFC 7396) are read through a small bounded tree and
//! shown as the edits they are.

use std::fmt::Write as _;

use nika_display::theme::Role;
use ratatui::text::Span;

use super::cells::{self, Sheet, paint, plain};
use super::mask::Walker;
use super::{Format, Note, secret};

/// One token of JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok {
    /// `{` or `[`.
    Open(u8),
    /// `}` or `]`.
    Close(u8),
    /// `:`.
    Colon,
    /// `,`.
    Comma,
    /// A string, quotes included.
    Str,
    /// A number.
    Num,
    /// `true`, `false` or `null`.
    Lit,
}

/// Why the lexer stopped inside a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    /// The bytes read end inside a token.
    Cut,
    /// The byte at this offset breaks the grammar.
    Invalid(usize),
}

/// A token and its byte range, `None` at the end of the bytes read.
type Lexed = Result<Option<(Tok, usize, usize)>, Stop>;

/// One token at a time over the bytes read; `cut` says the bytes stop
/// before the object does, so a token touching their end is unfinished.
struct Lexer<'a> {
    b: &'a [u8],
    at: usize,
    cut: bool,
}

impl<'a> Lexer<'a> {
    fn new(b: &'a [u8], cut: bool) -> Self {
        Self { b, at: 0, cut }
    }

    fn next(&mut self) -> Lexed {
        while self
            .b
            .get(self.at)
            .is_some_and(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.at += 1;
        }
        let start = self.at;
        let Some(&c) = self.b.get(start) else {
            return Ok(None);
        };
        let tok = match c {
            b'{' | b'[' => Tok::Open(c),
            b'}' | b']' => Tok::Close(c),
            b':' => Tok::Colon,
            b',' => Tok::Comma,
            b'"' => {
                self.string()?;
                return Ok(Some((Tok::Str, start, self.at)));
            }
            b'-' | b'0'..=b'9' => {
                self.number()?;
                return Ok(Some((Tok::Num, start, self.at)));
            }
            b't' | b'f' | b'n' => {
                self.literal()?;
                return Ok(Some((Tok::Lit, start, self.at)));
            }
            _ => return Err(Stop::Invalid(start)),
        };
        self.at += 1;
        Ok(Some((tok, start, self.at)))
    }

    fn peek(&mut self) -> Lexed {
        let at = self.at;
        let next = self.next();
        self.at = at;
        next
    }

    /// The stop for a token that ends where the bytes end.
    fn ended(&self) -> Stop {
        if self.at >= self.b.len() {
            Stop::Cut
        } else {
            Stop::Invalid(self.at)
        }
    }

    fn string(&mut self) -> Result<(), Stop> {
        let start = self.at;
        self.at += 1;
        loop {
            let Some(&c) = self.b.get(self.at) else {
                return Err(Stop::Cut);
            };
            match c {
                b'"' => break,
                b'\\' => match self.b.get(self.at + 1) {
                    None => return Err(Stop::Cut),
                    Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => self.at += 2,
                    Some(b'u') => match self.b.get(self.at + 2..self.at + 6) {
                        None => return Err(Stop::Cut),
                        Some(hex) if hex.iter().all(u8::is_ascii_hexdigit) => self.at += 6,
                        Some(_) => return Err(Stop::Invalid(self.at)),
                    },
                    Some(_) => return Err(Stop::Invalid(self.at)),
                },
                0x00..=0x1F => return Err(Stop::Invalid(self.at)),
                _ => self.at += 1,
            }
        }
        self.at += 1;
        if std::str::from_utf8(&self.b[start..self.at]).is_err() {
            return Err(Stop::Invalid(start));
        }
        Ok(())
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while self.b.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        self.at - start
    }

    /// The byte after a number or a literal must end it.
    fn delimited(&self) -> Result<(), Stop> {
        match self.b.get(self.at) {
            None if self.cut => Err(Stop::Cut),
            None | Some(b' ' | b'\t' | b'\n' | b'\r' | b',' | b']' | b'}' | b':') => Ok(()),
            Some(_) => Err(Stop::Invalid(self.at)),
        }
    }

    fn number(&mut self) -> Result<(), Stop> {
        if self.b.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        match self.b.get(self.at) {
            None => return Err(Stop::Cut),
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            Some(_) => return Err(Stop::Invalid(self.at)),
        }
        if self.b.get(self.at) == Some(&b'.') {
            self.at += 1;
            if self.digits() == 0 {
                return Err(self.ended());
            }
        }
        if matches!(self.b.get(self.at), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.b.get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                return Err(self.ended());
            }
        }
        self.delimited()
    }

    fn literal(&mut self) -> Result<(), Stop> {
        let rest = &self.b[self.at..];
        for word in [&b"true"[..], b"false", b"null"] {
            if rest.starts_with(word) {
                self.at += word.len();
                return self.delimited();
            }
            if word.starts_with(rest) {
                return Err(Stop::Cut);
            }
        }
        Err(Stop::Invalid(self.at))
    }
}

/// What the grammar expects next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    Value,
    ValueOrClose,
    Key,
    KeyOrClose,
    Colon,
    CommaOrClose,
    End,
}

/// The expectation after `tok`, `None` when `tok` breaks the grammar.
fn step(expect: Expect, tok: Tok, stack: &mut Vec<u8>) -> Option<Expect> {
    let after = |stack: &[u8]| {
        if stack.is_empty() {
            Expect::End
        } else {
            Expect::CommaOrClose
        }
    };
    match (expect, tok) {
        (Expect::Value | Expect::ValueOrClose, Tok::Open(c)) => {
            stack.push(c);
            Some(if c == b'{' {
                Expect::KeyOrClose
            } else {
                Expect::ValueOrClose
            })
        }
        (Expect::Value | Expect::ValueOrClose, Tok::Str | Tok::Num | Tok::Lit) => {
            Some(after(stack))
        }
        (Expect::Key | Expect::KeyOrClose, Tok::Str) => Some(Expect::Colon),
        (Expect::Colon, Tok::Colon) => Some(Expect::Value),
        (Expect::CommaOrClose, Tok::Comma) => Some(if stack.last() == Some(&b'{') {
            Expect::Key
        } else {
            Expect::Value
        }),
        (Expect::KeyOrClose | Expect::ValueOrClose | Expect::CommaOrClose, Tok::Close(c)) => {
            let open = if c == b'}' { b'{' } else { b'[' };
            (stack.pop() == Some(open)).then(|| after(stack))
        }
        _ => None,
    }
}

/// How far the bytes read hold one JSON document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// One whole document.
    Complete,
    /// A valid beginning: the bound cut the rest.
    Cut,
    /// Not JSON: the grammar breaks at this byte.
    Invalid(usize),
}

/// Validate the bytes read (`cut`: the bound stopped before the object).
pub(crate) fn validate(b: &[u8], cut: bool) -> Verdict {
    let mut lexer = Lexer::new(b, cut);
    let mut stack: Vec<u8> = Vec::new();
    let mut expect = Expect::Value;
    loop {
        match lexer.next() {
            Ok(Some((tok, start, _))) => match step(expect, tok, &mut stack) {
                Some(next) => expect = next,
                None => return Verdict::Invalid(start),
            },
            Ok(None) if expect == Expect::End => return Verdict::Complete,
            Ok(None) | Err(Stop::Cut) if cut => return Verdict::Cut,
            Ok(None) | Err(Stop::Cut) => return Verdict::Invalid(b.len()),
            Err(Stop::Invalid(at)) => return Verdict::Invalid(at),
        }
    }
}

/// The printer: tokens in, indented lines out, secrets masked.
struct Printer<'a> {
    src: &'a [u8],
    sheet: &'a mut Sheet,
    line: Vec<Span<'static>>,
    more: bool,
    stack: Vec<u8>,
    member: bool,
    cap: usize,
    deepest: usize,
    secrets: Walker,
    children: usize,
}

impl Printer<'_> {
    fn flush(&mut self) -> bool {
        if self.line.is_empty() {
            return true;
        }
        let spans = std::mem::take(&mut self.line);
        let more = std::mem::take(&mut self.more);
        self.sheet.body.push(spans, more)
    }

    /// Start a new line at the current depth.
    fn begin(&mut self) -> bool {
        if !self.flush() {
            return false;
        }
        let depth = self.stack.len().min(self.cap);
        self.deepest = self.deepest.max(self.stack.len());
        self.line.push(plain(" ".repeat(depth * 2)));
        true
    }

    fn dim(&mut self, text: &str) {
        let color = self.sheet.body.canvas().color;
        self.line.push(paint(text, Role::Dim, color));
    }

    /// The text of a token, cleaned, cut to the line bound.
    fn text(&mut self, start: usize, end: usize) -> String {
        let canvas = self.sheet.body.canvas();
        let limit = canvas.limits.line_bytes;
        let raw = std::str::from_utf8(&self.src[start..end]).unwrap_or_default();
        let raw = if raw.len() > limit {
            self.more = true;
            &raw[..cells::floor_boundary(raw, limit)]
        } else {
            raw
        };
        cells::clean(raw, canvas.ascii).0
    }

    /// What a token says: a string unescaped, a number or a literal as
    /// written.
    fn said(&self, tok: Tok, start: usize, end: usize) -> String {
        let raw = std::str::from_utf8(&self.src[start..end]).unwrap_or_default();
        if tok == Tok::Str {
            unescape(raw)
        } else {
            raw.to_owned()
        }
    }

    fn after_value(&mut self) {
        self.secrets.end();
        if self.stack.len() == 1 {
            self.children += 1;
        }
    }

    fn scalar(&mut self, tok: Tok, start: usize, end: usize) -> bool {
        let inline = std::mem::take(&mut self.member);
        if !inline && !self.begin() {
            return false;
        }
        let canvas = self.sheet.body.canvas();
        let text = self.text(start, end);
        let hidden = self.secrets.on() && {
            let said = self.said(tok, start, end);
            self.secrets.hide(self.stack.len(), &said, tok == Tok::Str)
        };
        let span = if hidden {
            let quote = if tok == Tok::Str { "\"" } else { "" };
            paint(
                format!("{quote}{}{quote}", secret::mask(canvas.ascii)),
                Role::Warn,
                canvas.color,
            )
        } else {
            match (tok, text.as_str()) {
                (Tok::Lit, "null") => paint(text, Role::Dim, canvas.color),
                (Tok::Num | Tok::Lit, _) => paint(text, Role::Strong, canvas.color),
                _ => plain(text),
            }
        };
        self.line.push(span);
        self.after_value();
        true
    }

    fn token(&mut self, tok: Tok, start: usize, end: usize, empty: bool) -> bool {
        let color = self.sheet.body.canvas().color;
        match tok {
            Tok::Open(c) => {
                let inline = std::mem::take(&mut self.member);
                if !inline && !self.begin() {
                    return false;
                }
                let open = char::from(c);
                if empty {
                    let close = if c == b'{' { "{}" } else { "[]" };
                    self.dim(close);
                    self.after_value();
                    return true;
                }
                self.dim(&open.to_string());
                self.secrets.open(self.stack.len());
                self.stack.push(c);
                self.flush()
            }
            Tok::Close(c) => {
                if !self.flush() {
                    return false;
                }
                self.stack.pop();
                self.secrets.close(self.stack.len());
                if !self.begin() {
                    return false;
                }
                self.dim(&char::from(c).to_string());
                self.after_value();
                true
            }
            Tok::Colon => {
                self.dim(": ");
                self.member = true;
                true
            }
            Tok::Comma => {
                self.dim(",");
                self.flush()
            }
            Tok::Str if self.stack.last() == Some(&b'{') && !self.member => {
                if !self.begin() {
                    return false;
                }
                let key = self.text(start, end);
                if self.secrets.on() {
                    let said = self.said(tok, start, end);
                    self.secrets.key(&said);
                }
                self.line.push(paint(key, Role::Accent, color));
                true
            }
            Tok::Str | Tok::Num | Tok::Lit => self.scalar(tok, start, end),
        }
    }
}

/// The deepest indent a printer draws on a canvas `width` cells wide.
fn indent_cap(width: usize) -> usize {
    (width.saturating_sub(24) / 2).clamp(1, 16)
}

/// Print the JSON in `b` (validated: complete or cut) into the sheet;
/// returns the top-level shape as a fact.
fn print(sheet: &mut Sheet, b: &[u8], cut: bool, protected: bool) -> String {
    let width = sheet.body.width();
    let dot = cells::sep(sheet.body.canvas().ascii);
    let mut printer = Printer {
        src: b,
        sheet,
        line: Vec::new(),
        more: false,
        stack: Vec::new(),
        member: false,
        cap: indent_cap(width),
        deepest: 0,
        secrets: Walker::new(protected),
        children: 0,
    };
    let mut lexer = Lexer::new(b, cut);
    let mut top = None;
    while let Ok(Some((tok, start, end))) = lexer.next() {
        top.get_or_insert(tok);
        let empty =
            matches!(tok, Tok::Open(_)) && matches!(lexer.peek(), Ok(Some((Tok::Close(_), ..))));
        if empty {
            let _ = lexer.next();
        }
        if !printer.token(tok, start, end, empty) {
            break;
        }
    }
    printer.flush();
    let (deepest, cap, masked, children) = (
        printer.deepest,
        printer.cap,
        printer.secrets.masked(),
        printer.children,
    );
    if deepest > cap {
        sheet.notes.push(Note::DepthCapped { depth: cap });
    }
    if masked > 0 {
        sheet.notes.push(Note::Masked { count: masked });
    }
    let least = if cut { "at least " } else { "" };
    match top {
        Some(Tok::Open(b'{')) => format!("object{dot}{least}{}", cells::count(children, "key")),
        Some(Tok::Open(_)) => format!("array{dot}{least}{}", cells::count(children, "item")),
        Some(Tok::Str) => "a string".to_owned(),
        Some(Tok::Num) => "a number".to_owned(),
        _ => "a literal".to_owned(),
    }
}

/// A small tree for the patch views, bounded in nodes and depth.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Null,
    Scalar(String),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

/// The nodes and the depth a patch tree may hold.
const TREE_NODES: usize = 20_000;
const TREE_DEPTH: usize = 64;

/// One JSON string token, unescaped.
fn unescape(raw: &str) -> String {
    let inner = raw
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or(raw);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    let mut pending: Option<u32> = None;
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let e = chars.next().unwrap_or('\\');
        let simple = match e {
            'b' => Some('\u{8}'),
            'f' => Some('\u{c}'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            'u' => None,
            other => Some(other),
        };
        if let Some(c) = simple {
            out.push(c);
            continue;
        }
        let hex: String = chars.by_ref().take(4).collect();
        let unit = u32::from_str_radix(&hex, 16).unwrap_or(0xFFFD);
        match (pending.take(), unit) {
            (None, 0xD800..=0xDBFF) => pending = Some(unit),
            (Some(high), 0xDC00..=0xDFFF) => {
                let code = 0x10000 + ((high - 0xD800) << 10) + (unit - 0xDC00);
                out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
            }
            (_, unit) => out.push(char::from_u32(unit).unwrap_or('\u{FFFD}')),
        }
    }
    out
}

/// One open container of the tree being built.
enum Frame {
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>, Option<String>),
}

/// Attach a finished value to the open container, or make it the root.
fn attach(stack: &mut [Frame], root: &mut Option<Value>, value: Value) {
    match stack.last_mut() {
        None => *root = Some(value),
        Some(Frame::Arr(items)) => items.push(value),
        Some(Frame::Obj(members, key)) => members.push((key.take().unwrap_or_default(), value)),
    }
}

/// The tree of a complete, valid document; `None` past the bounds.
fn tree(b: &[u8]) -> Option<Value> {
    let mut lexer = Lexer::new(b, false);
    let mut stack: Vec<Frame> = Vec::new();
    let mut root = None;
    let mut nodes = 0usize;
    while let Some((tok, start, end)) = lexer.next().ok()? {
        let raw = std::str::from_utf8(&b[start..end]).ok()?;
        nodes += 1;
        if nodes > TREE_NODES || stack.len() > TREE_DEPTH {
            return None;
        }
        match tok {
            Tok::Open(b'{') => stack.push(Frame::Obj(Vec::new(), None)),
            Tok::Open(_) => stack.push(Frame::Arr(Vec::new())),
            Tok::Close(_) => {
                let value = match stack.pop()? {
                    Frame::Arr(items) => Value::Arr(items),
                    Frame::Obj(members, _) => Value::Obj(members),
                };
                attach(&mut stack, &mut root, value);
            }
            Tok::Str => {
                if let Some(Frame::Obj(_, key @ None)) = stack.last_mut() {
                    *key = Some(unescape(raw));
                } else {
                    attach(&mut stack, &mut root, Value::Str(unescape(raw)));
                }
            }
            Tok::Num | Tok::Lit if raw == "null" => attach(&mut stack, &mut root, Value::Null),
            Tok::Num | Tok::Lit => attach(&mut stack, &mut root, Value::Scalar(raw.to_owned())),
            Tok::Colon | Tok::Comma => nodes -= 1,
        }
    }
    root
}

/// A value on one line, at most `budget` bytes (then cut with `...`), its
/// scalars walked through `secrets` from `depth` exactly as the document
/// view walks them: a masked one shows as the mask.
fn compact(
    value: &Value,
    out: &mut String,
    budget: usize,
    secrets: &mut Walker,
    depth: usize,
    ascii: bool,
) {
    if out.len() > budget {
        return;
    }
    match value {
        Value::Arr(items) => {
            out.push('[');
            secrets.open(depth);
            for (i, item) in items.iter().enumerate() {
                if out.len() > budget {
                    out.push_str("...");
                    break;
                }
                if i > 0 {
                    out.push_str(", ");
                }
                compact(item, out, budget, secrets, depth + 1, ascii);
            }
            secrets.close(depth);
            out.push(']');
        }
        Value::Obj(members) => {
            out.push('{');
            secrets.open(depth);
            for (i, (key, item)) in members.iter().enumerate() {
                if out.len() > budget {
                    out.push_str("...");
                    break;
                }
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "\"{key}\": ");
                secrets.key(key);
                compact(item, out, budget, secrets, depth + 1, ascii);
            }
            secrets.close(depth);
            out.push('}');
        }
        Value::Null | Value::Scalar(_) | Value::Str(_) => {
            let (said, quote) = match value {
                Value::Str(s) => (s.as_str(), "\""),
                Value::Scalar(s) => (s.as_str(), ""),
                _ => ("null", ""),
            };
            out.push_str(quote);
            if secrets.hide(depth, said, !quote.is_empty()) {
                out.push_str(secret::mask(ascii));
            } else if quote.is_empty() {
                out.push_str(said);
            } else {
                out.extend(said.chars().take(budget));
            }
            out.push_str(quote);
        }
    }
    secrets.end();
}

/// The segments of a JSON Pointer, unescaped (RFC 6901): the keys a patch
/// value sits under.
fn pointer(path: &str) -> Vec<String> {
    path.split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// The operations of a JSON Patch, drawn one per row; `false` when the
/// tree is not a list of RFC 6902 operations.
fn patch(sheet: &mut Sheet, root: &Value, protected: bool) -> bool {
    let Value::Arr(ops) = root else { return false };
    let field = |members: &[(String, Value)], name: &str| {
        members
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    let mut rows = Vec::new();
    for op in ops {
        let Value::Obj(members) = op else {
            return false;
        };
        let (Some(Value::Str(verb)), Some(Value::Str(path))) =
            (field(members, "op"), field(members, "path"))
        else {
            return false;
        };
        let known = ["add", "remove", "replace", "move", "copy", "test"];
        if !known.contains(&verb.as_str()) {
            return false;
        }
        let from = match field(members, "from") {
            Some(Value::Str(from)) => Some(from),
            _ => None,
        };
        rows.push((verb, path, from, field(members, "value")));
    }
    let canvas = sheet.body.canvas();
    let mut masked = 0;
    for (verb, path, from, value) in &rows {
        let (mark, tone) = match verb.as_str() {
            "add" => ("+", Role::Good),
            "remove" => ("-", Role::Bad),
            "replace" => ("~", Role::Warn),
            "move" => (">", Role::Accent),
            "copy" => ("=", Role::Accent),
            _ => ("?", Role::Dim),
        };
        let arrow = if canvas.ascii { " -> " } else { " → " };
        let target = match from {
            Some(from) => format!(
                "{}{arrow}{}",
                cells::clean(from, canvas.ascii).0,
                cells::clean(path, canvas.ascii).0
            ),
            None => cells::clean(path, canvas.ascii).0,
        };
        let mut spans = vec![
            paint(format!("{mark} {verb:<8}"), tone, canvas.color),
            plain(target),
        ];
        if let Some(value) = value {
            let mut secrets = Walker::new(protected);
            let segments = pointer(path);
            for (at, segment) in segments.iter().enumerate() {
                if at > 0 {
                    secrets.open(at);
                }
                secrets.key(segment);
            }
            let mut text = String::new();
            let depth = segments.len();
            compact(value, &mut text, 240, &mut secrets, depth, canvas.ascii);
            masked += secrets.masked();
            spans.push(paint("  ", Role::Dim, canvas.color));
            spans.push(plain(cells::clean(&text, canvas.ascii).0));
        }
        if !sheet.body.push(spans, false) {
            break;
        }
    }
    if masked > 0 {
        sheet.notes.push(Note::Masked { count: masked });
    }
    sheet.facts.push(cells::count(rows.len(), "operation"));
    true
}

/// One step of the merge walk: a member to show, or a container left.
enum Step<'v> {
    /// The member's path, its key, its value and its depth.
    Member(String, Option<&'v str>, &'v Value, usize),
    /// The container opened at this depth ends.
    Leave(usize),
}

/// The edits of a merge patch, one row per leaf: `null` removes a key, any
/// other value sets it, an object merges deeper. The walk feeds the masking
/// walker in document order, so a value under a key naming a credential
/// is masked however deep it sits.
fn merge(sheet: &mut Sheet, root: &Value, protected: bool) {
    let canvas = sheet.body.canvas();
    let mut secrets = Walker::new(protected);
    let mut edits = 0;
    let mut todo = vec![Step::Member(String::new(), None, root, 0)];
    while let Some(step) = todo.pop() {
        let (path, key, value, depth) = match step {
            Step::Leave(depth) => {
                secrets.close(depth);
                secrets.end();
                continue;
            }
            Step::Member(path, key, value, depth) => (path, key, value, depth),
        };
        if let Some(key) = key {
            secrets.key(key);
        }
        if let Value::Obj(members) = value {
            secrets.open(depth);
            todo.push(Step::Leave(depth));
            for (key, item) in members.iter().rev() {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                let path = format!("{path}/{escaped}");
                todo.push(Step::Member(path, Some(key.as_str()), item, depth + 1));
            }
            continue;
        }
        edits += 1;
        let shown_path = if path.is_empty() {
            "/ (the whole document)".to_owned()
        } else {
            path
        };
        let spans = if matches!(value, Value::Null) {
            secrets.end();
            vec![
                paint("- ", Role::Bad, canvas.color),
                plain(cells::clean(&shown_path, canvas.ascii).0),
                paint("  null: removes the key", Role::Dim, canvas.color),
            ]
        } else {
            let mut text = String::new();
            compact(value, &mut text, 240, &mut secrets, depth, canvas.ascii);
            vec![
                paint("= ", Role::Good, canvas.color),
                plain(cells::clean(&shown_path, canvas.ascii).0),
                plain("  "),
                plain(cells::clean(&text, canvas.ascii).0),
            ]
        };
        if !sheet.body.push(spans, false) {
            break;
        }
    }
    if secrets.masked() > 0 {
        sheet.notes.push(Note::Masked {
            count: secrets.masked(),
        });
    }
    sheet.facts.push(cells::count(edits, "edit"));
}

/// Show JSON bytes (`cut`: the bound stopped before the object) as the
/// sheet's format asks: a document, a JSON Patch or a merge patch. `Err`
/// with the offending byte when the bytes are not JSON: the caller shows
/// them as text.
pub(crate) fn show(sheet: &mut Sheet, b: &[u8], cut: bool, protected: bool) -> Result<(), usize> {
    let verdict = validate(b, cut);
    if let Verdict::Invalid(at) = verdict {
        return Err(at);
    }
    if matches!(sheet.format, Format::JsonPatch | Format::MergePatch) {
        let root = (verdict == Verdict::Complete).then(|| tree(b)).flatten();
        let drawn = match (&root, sheet.format) {
            (Some(root), Format::JsonPatch) => patch(sheet, root, protected),
            (Some(root), _) => {
                merge(sheet, root, protected);
                true
            }
            (None, _) => false,
        };
        if drawn {
            return Ok(());
        }
        let why = if root.is_none() {
            "cut or too large to read as a patch"
        } else {
            "not a list of RFC 6902 operations"
        };
        sheet.notes.push(Note::Fallback {
            to: Format::Json,
            why,
        });
        sheet.format = Format::Json;
    }
    let shape = print(sheet, b, cut, protected);
    sheet.facts.push(shape);
    if verdict == Verdict::Complete && sheet.body.is_full() {
        let ascii = sheet.body.canvas().ascii;
        if protected {
            sheet
                .facts
                .push("shape withheld: the object is protected".to_owned());
        } else if let Ok(text) = std::str::from_utf8(b)
            && let Some(summary) = nika_display::shape::summarize(text, 60)
        {
            let summary = cells::clean(&summary, ascii).0;
            sheet
                .facts
                .push(format!("shape {}", cells::dots(&summary, ascii)));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{Canvas, Limits};

    fn sheet(width: u16, format: Format) -> Sheet {
        Sheet::new(Canvas::new(width, true, false), format)
    }

    fn rows(sheet: Sheet) -> Vec<String> {
        let mut notes = Vec::new();
        sheet
            .body
            .finish(&mut notes)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn validation_separates_complete_cut_and_invalid() {
        assert_eq!(
            validate(br#"{"a": [1, 2.5e3, true, null]}"#, false),
            Verdict::Complete
        );
        assert_eq!(validate(br#"{"a": [1, 2"#, true), Verdict::Cut);
        assert_eq!(validate(br#"{"a": [1, 2"#, false), Verdict::Invalid(11));
        assert_eq!(validate(br#"{"a" 1}"#, false), Verdict::Invalid(5));
        assert_eq!(validate(b"[1,]", false), Verdict::Invalid(3));
        assert_eq!(validate(b"01", false), Verdict::Invalid(1));
        assert_eq!(validate(b"{} {}", false), Verdict::Invalid(3));
        assert_eq!(validate(b"\"a\x01\"", false), Verdict::Invalid(2));
        assert_eq!(validate(b"\"\xff\"", false), Verdict::Invalid(0));
        assert_eq!(validate(&b"[true]"[..4], true), Verdict::Cut);
        assert_eq!(
            validate(b"[12", true),
            Verdict::Cut,
            "a number at the cut may go on"
        );
    }

    #[test]
    fn a_document_is_laid_out_with_its_keys_and_empty_containers_whole() {
        let mut s = sheet(80, Format::Json);
        show(
            &mut s,
            br#"{"name":"release","tags":["a","b"],"meta":{},"n":null}"#,
            false,
            false,
        )
        .expect("json");
        assert_eq!(s.facts, ["object - 4 keys"]);
        assert_eq!(
            rows(s),
            [
                "{",
                "  \"name\": \"release\",",
                "  \"tags\": [",
                "    \"a\",",
                "    \"b\"",
                "  ],",
                "  \"meta\": {},",
                "  \"n\": null",
                "}"
            ]
        );
    }

    #[test]
    fn deep_nesting_is_iterative_and_its_indent_capped() {
        let depth = 100_000;
        let mut deep = "[".repeat(depth);
        deep.push_str(&"]".repeat(depth));
        let mut s = Sheet::new(
            Canvas::new(40, true, false).with_limits(Limits::new(1 << 20, 50, 4096)),
            Format::Json,
        );
        show(&mut s, deep.as_bytes(), false, false).expect("deep json is json");
        assert!(
            s.notes.contains(&Note::DepthCapped { depth: 8 }),
            "{:?}",
            s.notes
        );
        assert!(s.body.is_full());
    }

    #[test]
    fn a_protected_document_masks_its_secrets() {
        let mut s = sheet(80, Format::Json);
        let doc = br#"{"user":"ann","api_key":"abc","auth":{"token":"t0k","ttl":3},"note":"sk-proj-0123456789"}"#;
        show(&mut s, doc, false, true).expect("json");
        assert_eq!(s.notes, [Note::Masked { count: 4 }]);
        let text = rows(s).join("\n");
        for secret in ["abc", "t0k", "sk-proj"] {
            assert!(
                !text.contains(secret),
                "protected fixture value was not masked"
            );
        }
        assert!(text.contains("\"ann\""));
    }

    #[test]
    fn a_json_patch_reads_as_its_operations() {
        let mut s = sheet(80, Format::JsonPatch);
        let doc = br#"[{"op":"add","path":"/tags/-","value":"v8"},{"op":"remove","path":"/draft"},{"op":"move","from":"/a","path":"/b"}]"#;
        show(&mut s, doc, false, false).expect("patch");
        assert_eq!(s.facts, ["3 operations"]);
        assert_eq!(
            rows(s),
            [
                "+ add     /tags/-  \"v8\"",
                "- remove  /draft",
                "> move    /a -> /b"
            ]
        );
        let mut not_patch = sheet(80, Format::JsonPatch);
        show(&mut not_patch, br#"{"op":"add"}"#, false, false).expect("json");
        assert_eq!(not_patch.format, Format::Json);
        assert!(matches!(
            not_patch.notes[0],
            Note::Fallback {
                to: Format::Json,
                ..
            }
        ));
    }

    #[test]
    fn a_merge_patch_names_what_null_removes() {
        let mut s = sheet(80, Format::MergePatch);
        show(
            &mut s,
            br#"{"title":"v8","draft":null,"meta":{"a/b":1}}"#,
            false,
            false,
        )
        .expect("merge");
        assert_eq!(
            rows(s),
            [
                "= /title  \"v8\"",
                "- /draft  null: removes the key",
                "= /meta/a~1b  1"
            ]
        );
    }

    #[test]
    fn escapes_unescape_including_surrogate_pairs() {
        assert_eq!(unescape(r#""a\"b\né😀""#), "a\"b\né😀");
    }
}
