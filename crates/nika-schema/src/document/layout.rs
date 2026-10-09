// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Where each node of a document sits in its bytes. The strict parser's own
//! YAML frontend (marked-yaml, with the parser's loader options) marks where
//! every node starts; the end of a scalar is found lexically from its style,
//! and a block collection ends where its last child ends. This is a reading
//! of positions, never a second YAML decoder: every edit placed by these
//! spans is parsed again by the strict parser and compared with the meaning
//! it predicts, so a misplaced span can only refuse an edit.

use marked_yaml::types::{MarkedMappingNode, MarkedScalarNode, MarkedSequenceNode};
use marked_yaml::{LoaderOptions, Node as Yaml, parse_yaml_with_options};

use super::{Path, Style};

/// The shape of a node in the YAML tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Mapping,
    Sequence,
    Scalar,
}

/// One node: its path, shape, presentation and byte span.
#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub(super) path: Path,
    pub(super) kind: Kind,
    pub(super) style: Style,
    /// The first byte of the value (a block scalar starts at its `|`/`>`).
    pub(super) start: usize,
    /// One past the value's last byte; `None` when this reading cannot place it.
    pub(super) end: Option<usize>,
    /// The first byte of the entry holding the value: its key, or a block item's dash.
    pub(super) lead: Option<usize>,
    /// Whether the value sits inside a flow collection.
    pub(super) in_flow: bool,
    pub(super) children: Vec<usize>,
    /// A block scalar's content indentation, when it has content.
    pub(super) content_indent: Option<usize>,
    /// A block scalar's chomping indicator (`-` or `+`), when it states one.
    pub(super) chomp: Option<u8>,
}

/// Every node of one document, in document order (index 0 is the root).
#[derive(Clone, Debug)]
pub(super) struct Layout {
    pub(super) entries: Vec<Entry>,
}

impl Layout {
    /// The positions of every node of `source`, read with the strict
    /// parser's loader options; `None` when the YAML does not load.
    pub(super) fn build(source: &str) -> Option<Self> {
        let bom = if source.starts_with('\u{FEFF}') {
            '\u{FEFF}'.len_utf8()
        } else {
            0
        };
        let text = source.get(bom..)?;
        let options = LoaderOptions::default()
            .error_on_duplicate_keys(true)
            .prevent_coercion(true);
        let root = parse_yaml_with_options(0, text, options).ok()?;
        // Line starts: the loader's line and column are exact, while its
        // character index counts the BYTES of a block scalar's content lines
        // (yaml-rust2 0.10 adds `line_buffer.len()`), so every index after a
        // non-ASCII block scalar runs ahead of the text.
        let mut lines = vec![bom];
        lines.extend(
            source
                .bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\n')
                .map(|(i, _)| i + 1),
        );
        let mut builder = Builder {
            src: source.as_bytes(),
            text: source,
            lines,
            entries: Vec::new(),
        };
        builder.node(&root, Path::root(), Context::default());
        Some(Self {
            entries: builder.entries,
        })
    }

    /// The node at `path`.
    pub(super) fn get(&self, path: &Path) -> Option<&Entry> {
        self.entries.iter().find(|e| &e.path == path)
    }
}

/// Where a value sits: inside a flow collection, under which indentation,
/// after which key or dash.
#[derive(Clone, Copy, Debug, Default)]
struct Context {
    in_flow: bool,
    /// The column of the key or dash that holds the value.
    indent: Option<usize>,
    /// The key or dash itself.
    lead: Option<usize>,
    /// The byte after which an omitted or block value starts (`:` or `-` plus one).
    after: Option<usize>,
}

struct Builder<'a> {
    src: &'a [u8],
    text: &'a str,
    /// The byte where each line starts (line 1 after a byte-order mark).
    lines: Vec<usize>,
    entries: Vec<Entry>,
}

impl Builder<'_> {
    /// The byte a loader marker names, from its 1-based line and column.
    fn byte(&self, marker: Option<&marked_yaml::Marker>) -> Option<usize> {
        let marker = marker?;
        let start = *self.lines.get(marker.line().checked_sub(1)?)?;
        let end = line_end(self.src, start);
        let line = self.text.get(start..end)?;
        line.char_indices()
            .map(|(b, _)| start + b)
            .chain([end])
            .nth(marker.column().checked_sub(1)?)
    }

    fn push(&mut self, path: Path, kind: Kind, style: Style, ctx: Context) -> usize {
        self.entries.push(Entry {
            path,
            kind,
            style,
            start: 0,
            end: None,
            lead: ctx.lead,
            in_flow: ctx.in_flow,
            children: Vec::new(),
            content_indent: None,
            chomp: None,
        });
        self.entries.len() - 1
    }

    fn node(&mut self, yaml: &Yaml, path: Path, ctx: Context) -> usize {
        match yaml {
            Yaml::Scalar(scalar) => self.scalar(scalar, path, ctx),
            Yaml::Mapping(map) => self.mapping(map, &path, ctx),
            Yaml::Sequence(seq) => self.sequence(seq, &path, ctx),
        }
    }

    fn scalar(&mut self, scalar: &MarkedScalarNode, path: Path, ctx: Context) -> usize {
        let mark = self.byte(scalar.span().start());
        let src = self.src;
        // A block scalar is marked at its first content byte, which may be a
        // quote: its `|`/`>` header after the `:` or dash names it.
        let header = ctx
            .after
            .filter(|_| !ctx.in_flow && !scalar.may_coerce())
            .map(|from| skip_blank(src, from))
            .is_some_and(|h| matches!(src.get(h), Some(b'|' | b'>')));
        let (style, start, end) = if scalar.may_coerce() && scalar.as_str().is_empty() {
            // An omitted value: the loader marks the NEXT token, so the value
            // sits right after its `:` or dash, zero bytes wide.
            let at = ctx.after.or(mark).unwrap_or(0);
            (Style::Empty, at, ctx.after)
        } else {
            match (mark, mark.and_then(|m| src.get(m))) {
                (Some(m), _) if header => return self.block_scalar(path, ctx, m),
                (Some(m), Some(b'"')) => (Style::DoubleQuoted, m, scan_double(src, m)),
                (Some(m), Some(b'\'')) => (Style::SingleQuoted, m, scan_single(src, m)),
                (Some(m), _) if scalar.may_coerce() => {
                    (Style::Plain, m, scan_plain(src, m, ctx.in_flow, ctx.indent))
                }
                (Some(m), _) => return self.block_scalar(path, ctx, m),
                (None, _) => (Style::Plain, 0, None),
            }
        };
        let index = self.push(path, Kind::Scalar, style, ctx);
        let entry = &mut self.entries[index];
        entry.start = start;
        entry.end = end;
        index
    }

    /// A literal or folded scalar: its header follows the key's `:` or the
    /// item's dash; the loader marks its first content byte.
    fn block_scalar(&mut self, path: Path, ctx: Context, content: usize) -> usize {
        let src = self.src;
        let header = ctx
            .after
            .map(|from| skip_blank(src, from))
            .filter(|&h| matches!(src.get(h), Some(b'|' | b'>')));
        let style = match header.and_then(|h| src.get(h)) {
            Some(b'>') => Style::Folded,
            _ => Style::Literal,
        };
        let index = self.push(path, Kind::Scalar, style, ctx);
        let Some(header) = header else {
            self.entries[index].start = content;
            return index;
        };
        let read = scan_block(src, header, ctx.indent.unwrap_or(0));
        let entry = &mut self.entries[index];
        entry.start = header;
        if let Some(block) = read {
            // The loader's mark must fall inside the content this reading placed.
            let consistent = block
                .content
                .is_none_or(|(first, last)| (first..=last).contains(&content));
            entry.end = consistent.then_some(block.end);
            entry.content_indent = block.indent;
            entry.chomp = block.chomp;
        }
        index
    }

    fn mapping(&mut self, map: &MarkedMappingNode, path: &Path, ctx: Context) -> usize {
        let src = self.src;
        let open = self.byte(map.span().start());
        let close = self.byte(map.span().end());
        let flow = open.and_then(|o| src.get(o)) == Some(&b'{')
            && close.and_then(|c| src.get(c)) == Some(&b'}');
        let style = if flow { Style::Flow } else { Style::Block };
        let index = self.push(path.clone(), Kind::Mapping, style, ctx);
        let mut children = Vec::new();
        let mut placed = true;
        for (key, value) in map.iter() {
            let Some(key_start) = self.byte(key.span().start()) else {
                placed = false;
                continue;
            };
            let colon = key_end(src, key_start, ctx.in_flow || flow)
                .map(|end| skip_blank(src, end))
                .filter(|&c| src.get(c) == Some(&b':'));
            placed &= colon.is_some();
            let child = Context {
                in_flow: ctx.in_flow || flow,
                indent: Some(column(src, key_start)),
                lead: Some(key_start),
                after: colon.map(|c| c + 1),
            };
            children.push(self.node(value, path.child(key.as_str()), child));
        }
        let (start, end) = if flow {
            (open.unwrap_or(0), close.map(|c| c + 1))
        } else {
            let first = children.first().and_then(|&c| self.entries[c].lead);
            let last = children.last().and_then(|&c| self.entries[c].end);
            (first.unwrap_or(0), last.filter(|_| first.is_some()))
        };
        let entry = &mut self.entries[index];
        entry.start = start;
        entry.end = end.filter(|_| placed);
        entry.children = children;
        index
    }

    fn sequence(&mut self, seq: &MarkedSequenceNode, path: &Path, ctx: Context) -> usize {
        let src = self.src;
        let open = self.byte(seq.span().start());
        let close = self.byte(seq.span().end());
        let flow = open.and_then(|o| src.get(o)) == Some(&b'[')
            && close.and_then(|c| src.get(c)) == Some(&b']');
        let style = if flow { Style::Flow } else { Style::Block };
        let index = self.push(path.clone(), Kind::Sequence, style, ctx);
        let mut children = Vec::new();
        let mut placed = true;
        for (i, item) in seq.iter().enumerate() {
            let child = if flow {
                Context {
                    in_flow: true,
                    indent: ctx.indent,
                    lead: None,
                    after: None,
                }
            } else {
                let dash = self.dash(item);
                placed &= dash.is_some();
                Context {
                    in_flow: ctx.in_flow,
                    indent: dash.map(|d| column(src, d)),
                    lead: dash,
                    after: dash.map(|d| d + 1),
                }
            };
            children.push(self.node(item, path.child(i.to_string()), child));
        }
        let (start, end) = if flow {
            (open.unwrap_or(0), close.map(|c| c + 1))
        } else {
            let first = children.first().and_then(|&c| self.entries[c].lead);
            let last = children.last().and_then(|&c| self.entries[c].end);
            (first.unwrap_or(0), last.filter(|_| first.is_some()))
        };
        let entry = &mut self.entries[index];
        entry.start = start;
        entry.end = end.filter(|_| placed);
        entry.children = children;
        index
    }

    /// The dash of a block sequence item: found back from the item's first
    /// byte on the same line, or on the header line of a block scalar item.
    fn dash(&self, item: &Yaml) -> Option<usize> {
        let src = self.src;
        let anchor = match item {
            Yaml::Scalar(s) if s.may_coerce() && s.as_str().is_empty() => return None,
            Yaml::Scalar(s) => {
                let mark = self.byte(s.span().start())?;
                // A quoted or plain item sits on its dash's line; a block
                // scalar item's content starts on the line below its header.
                let same_line = dash_before(src, mark);
                if same_line.is_none() && !s.may_coerce() {
                    return block_item_dash(src, mark);
                }
                return same_line;
            }
            Yaml::Mapping(map) => {
                let open = self.byte(map.span().start())?;
                if src.get(open) == Some(&b'{') {
                    open
                } else {
                    let (key, _) = map.iter().next()?;
                    self.byte(key.span().start())?
                }
            }
            Yaml::Sequence(inner) => {
                let open = self.byte(inner.span().start())?;
                if src.get(open) == Some(&b'[') {
                    open
                } else {
                    self.dash(inner.iter().next()?)?
                }
            }
        };
        dash_before(src, anchor)
    }
}

/// The dash right before `anchor` on its line, blanks between them.
fn dash_before(src: &[u8], anchor: usize) -> Option<usize> {
    let before = src.get(..anchor)?;
    let at = before.iter().rposition(|b| !matches!(b, b' ' | b'\t'))?;
    (src.get(at) == Some(&b'-')).then_some(at)
}

/// The dash of a block scalar item (`- |`), read on the line above its content.
fn block_item_dash(src: &[u8], content: usize) -> Option<usize> {
    let line = line_start(src, content);
    let header_line = line_start(src, line.checked_sub(1)?);
    let dash = skip_blank(src, header_line);
    let indicator = skip_blank(src, dash + 1);
    (src.get(dash) == Some(&b'-') && matches!(src.get(indicator), Some(b'|' | b'>')))
        .then_some(dash)
}

/// The first byte at or after `from` that is not a space or a tab.
pub(super) fn skip_blank(src: &[u8], from: usize) -> usize {
    let mut i = from;
    while matches!(src.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    i
}

/// The first byte of the line holding `at`.
pub(super) fn line_start(src: &[u8], at: usize) -> usize {
    src.get(..at)
        .and_then(|before| before.iter().rposition(|&b| b == b'\n'))
        .map_or(0, |n| n + 1)
}

/// The byte where the line holding `at` breaks (`\r` of a CRLF, `\n`, or the end).
pub(super) fn line_end(src: &[u8], at: usize) -> usize {
    let mut i = at;
    while let Some(&b) = src.get(i) {
        if b == b'\n' || (b == b'\r' && src.get(i + 1) == Some(&b'\n')) {
            return i;
        }
        i += 1;
    }
    src.len()
}

/// The first byte of the line after the one holding `at`; `None` on the last line.
pub(super) fn next_line(src: &[u8], at: usize) -> Option<usize> {
    let end = line_end(src, at);
    match src.get(end) {
        Some(b'\r') => Some(end + 2),
        Some(b'\n') => Some(end + 1),
        _ => None,
    }
}

/// The column of `at` in its line (bytes; block indentation is spaces only).
pub(super) fn column(src: &[u8], at: usize) -> usize {
    at - line_start(src, at)
}

/// Whether the line starting at `start` holds only spaces and tabs.
fn blank_line(src: &[u8], start: usize) -> bool {
    let end = line_end(src, start);
    src.get(start..end)
        .is_some_and(|line| line.iter().all(|b| matches!(b, b' ' | b'\t')))
}

/// The number of leading spaces of the line starting at `start`.
fn spaces(src: &[u8], start: usize) -> usize {
    src.get(start..)
        .map_or(0, |rest| rest.iter().take_while(|&&b| b == b' ').count())
}

/// One past the closing quote of a double-quoted scalar opening at `open`.
fn scan_double(src: &[u8], open: usize) -> Option<usize> {
    let mut i = open + 1;
    while let Some(&b) = src.get(i) {
        match b {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// One past the closing quote of a single-quoted scalar opening at `open`.
fn scan_single(src: &[u8], open: usize) -> Option<usize> {
    let mut i = open + 1;
    while let Some(&b) = src.get(i) {
        if b == b'\'' {
            if src.get(i + 1) == Some(&b'\'') {
                i += 2;
            } else {
                return Some(i + 1);
            }
        } else {
            i += 1;
        }
    }
    None
}

/// One past the last byte of the line's content from `from`: before a ` #`
/// comment or the line break, trailing blanks excluded.
fn content_end(src: &[u8], from: usize) -> usize {
    let mut last = from;
    let mut i = from;
    while let Some(&b) = src.get(i) {
        if b == b'\n' || b == b'\r' {
            break;
        }
        if b == b'#' && i > from && matches!(src.get(i - 1), Some(b' ' | b'\t')) {
            break;
        }
        if !matches!(b, b' ' | b'\t') {
            last = i + 1;
        }
        i += 1;
    }
    last
}

/// One past the last byte of a plain scalar starting at `start`. In a flow
/// collection it ends at an indicator or a comment; in a block it continues
/// on each following line indented deeper than its key or dash.
fn scan_plain(src: &[u8], start: usize, in_flow: bool, indent: Option<usize>) -> Option<usize> {
    if in_flow {
        let mut last = start;
        let mut i = start;
        while let Some(&b) = src.get(i) {
            let next = src.get(i + 1).copied();
            let separator = b == b':'
                && next
                    .is_none_or(|n| matches!(n, b' ' | b'\t' | b'\n' | b'\r' | b',' | b']' | b'}'));
            let comment = b == b'#'
                && i > start
                && matches!(src.get(i - 1), Some(b' ' | b'\t' | b'\n' | b'\r'));
            if matches!(b, b',' | b']' | b'}') || separator || comment {
                break;
            }
            if !matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                last = i + 1;
            }
            i += 1;
        }
        return Some(last);
    }
    let floor = indent?;
    let mut end = content_end(src, start);
    let mut line = next_line(src, start);
    while let Some(at) = line {
        if blank_line(src, at) {
            line = next_line(src, at);
            continue;
        }
        let depth = spaces(src, at);
        if depth <= floor || src.get(at + depth) == Some(&b'#') {
            break;
        }
        end = content_end(src, at + depth);
        line = next_line(src, at);
    }
    Some(end)
}

/// What [`scan_block`] read of a block scalar.
struct Block {
    /// One past the last content byte (the header's line end without content).
    end: usize,
    indent: Option<usize>,
    chomp: Option<u8>,
    /// The first and last content bytes, when there is content.
    content: Option<(usize, usize)>,
}

/// A block scalar whose `|`/`>` header is at `header`, under a key or dash
/// at column `floor`: its indicators, content indentation and last content line.
fn scan_block(src: &[u8], header: usize, floor: usize) -> Option<Block> {
    let mut i = header + 1;
    let mut chomp = None;
    let mut explicit = None;
    for _ in 0..2 {
        match src.get(i) {
            Some(&c @ (b'+' | b'-')) if chomp.is_none() => chomp = Some(c),
            Some(&d @ b'1'..=b'9') if explicit.is_none() => explicit = Some(usize::from(d - b'0')),
            _ => break,
        }
        i += 1;
    }
    let rest = skip_blank(src, i);
    if !matches!(src.get(rest), None | Some(b'\n' | b'\r' | b'#')) {
        return None;
    }
    let header_end = line_end(src, header);
    let mut indent = explicit.map(|d| floor + d);
    let mut content: Option<(usize, usize)> = None;
    let mut line = next_line(src, header);
    while let Some(at) = line {
        if blank_line(src, at) {
            line = next_line(src, at);
            continue;
        }
        let depth = spaces(src, at);
        let wanted = *indent.get_or_insert(depth);
        if depth < wanted || depth <= floor {
            break;
        }
        let first = content.map_or(at + depth, |(first, _)| first);
        content = Some((first, line_end(src, at)));
        line = next_line(src, at);
    }
    Some(Block {
        end: content.map_or(header_end, |(_, last)| last),
        indent: content.and(indent),
        chomp,
        content,
    })
}

/// One past the last byte of a mapping key starting at `start`.
pub(super) fn key_end(src: &[u8], start: usize, in_flow: bool) -> Option<usize> {
    match src.get(start)? {
        b'"' => scan_double(src, start),
        b'\'' => scan_single(src, start),
        _ => {
            let mut last = start;
            let mut i = start;
            while let Some(&b) = src.get(i) {
                let next = src.get(i + 1).copied();
                let separator = b == b':'
                    && next.is_none_or(|n| {
                        matches!(n, b' ' | b'\t' | b'\n' | b'\r')
                            || (in_flow && matches!(n, b',' | b']' | b'}'))
                    });
                if separator || b == b'\n' || b == b'\r' || (in_flow && matches!(b, b',' | b'}')) {
                    break;
                }
                if !matches!(b, b' ' | b'\t') {
                    last = i + 1;
                }
                i += 1;
            }
            Some(last)
        }
    }
}

#[cfg(test)]
mod tests;
