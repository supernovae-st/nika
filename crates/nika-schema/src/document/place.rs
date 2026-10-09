// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Where an edit's bytes go: the exact span it replaces and the candidate
//! texts it may write there, in order of preference. Nothing outside the
//! span is rewritten; the caller keeps the first candidate the strict parser
//! reads back as the predicted document.

use serde_json::Value;

use super::layout::{Entry, Kind, Layout, column, key_end, line_start, next_line, skip_blank};
use super::{Edit, Path, Refusal, Splice, Style, emit};

/// What [`candidates`] reads of the document it places edits in.
pub(super) struct Site<'a> {
    pub(super) source: &'a str,
    pub(super) layout: &'a Layout,
}

impl Site<'_> {
    fn src(&self) -> &[u8] {
        self.source.as_bytes()
    }

    /// The line break this document writes.
    fn nl(&self) -> &'static str {
        if self.source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        }
    }

    fn entry(&self, path: &Path) -> Result<&Entry, Refusal> {
        self.layout
            .get(path)
            .ok_or_else(|| Refusal::UnknownPath { path: path.clone() })
    }

    /// The child entries of a collection, in document order.
    fn children<'b>(&'b self, entry: &'b Entry) -> impl Iterator<Item = &'b Entry> {
        entry
            .children
            .iter()
            .filter_map(|&i| self.layout.entries.get(i))
    }

    /// How much deeper a nested block sits than its parent: the document's
    /// own step when this collection shows one, else two spaces.
    fn step(&self, collection: &Entry) -> usize {
        let src = self.src();
        let parent = collection.lead.map(|lead| column(src, lead));
        let child = self
            .children(collection)
            .next()
            .and_then(|c| c.lead)
            .map(|lead| column(src, lead));
        match (parent, child) {
            (Some(p), Some(c)) if c > p => c - p,
            _ => 2,
        }
    }
}

/// The end of a node's bytes, or why it cannot be read in place.
fn end_of(entry: &Entry) -> Result<usize, Refusal> {
    entry
        .end
        .ok_or_else(|| layout(&entry.path, "its extent cannot be read in place"))
}

fn layout(path: &Path, detail: &str) -> Refusal {
    Refusal::Layout {
        path: path.clone(),
        detail: detail.to_owned(),
    }
}

fn splices(start: usize, end: usize, texts: Vec<String>) -> Vec<Splice> {
    texts
        .into_iter()
        .map(|replacement| Splice::new(start, end, replacement))
        .collect()
}

/// The splices `edit` may make in `site`, in order of preference;
/// `inserted_text` is the exact text of an [`Edit::InsertText`].
///
/// # Errors
/// The node is absent, or its presentation is not one this reading edits in place.
pub(super) fn candidates(site: &Site<'_>, edit: &Edit) -> Result<Vec<Splice>, Refusal> {
    match edit {
        Edit::Set { path, value } => set(site, site.entry(path)?, value),
        Edit::Insert { path, key, value } => {
            insert(site, site.entry(path)?, key, &Payload::Value(value))
        }
        Edit::InsertText { path, key, text } => {
            insert(site, site.entry(path)?, key, &Payload::Text(text))
        }
        Edit::Push { path, value } => push(site, site.entry(path)?, value),
        Edit::Remove { path } => remove(site, path),
        Edit::Rename { path, .. } => Err(layout(path, "a rename is placed key by key")),
    }
}

/// The bytes of the key at `path`, replaced by `to`.
///
/// # Errors
/// The entry is absent, or its key cannot be read in place.
pub(super) fn rekey(site: &Site<'_>, path: &Path, to: &str) -> Result<Splice, Refusal> {
    let entry = site.entry(path)?;
    let lead = entry
        .lead
        .filter(|_| path.parent().is_some())
        .ok_or_else(|| layout(path, "an item with no key"))?;
    let end = key_end(site.src(), lead, entry.in_flow)
        .ok_or_else(|| layout(path, "its key cannot be read in place"))?;
    Ok(Splice::new(lead, end, emit::key(to, entry.in_flow)))
}

/// Replace a node's value where it stands.
fn set(site: &Site<'_>, entry: &Entry, value: &Value) -> Result<Vec<Splice>, Refusal> {
    let src = site.src();
    let end = end_of(entry)?;
    let start = entry.start;
    let nl = site.nl();
    let collection = matches!(value, Value::Object(_) | Value::Array(_));
    // An omitted value is written after its `:` or dash, one space apart.
    let lead = if entry.style == Style::Empty { " " } else { "" };
    let texts: Vec<String> = match (entry.kind, collection) {
        (Kind::Scalar, false) => {
            if entry.chomp == Some(b'+') {
                return Err(layout(&entry.path, "a keep-chomped block scalar"));
            }
            let indent = entry
                .content_indent
                .unwrap_or_else(|| entry.lead.map_or(2, |l| column(src, l) + 2));
            emit::scalars(value, entry.style, entry.in_flow, indent, nl)
                .into_iter()
                .map(|text| format!("{lead}{text}"))
                .collect()
        }
        (Kind::Scalar, true) => vec![format!("{lead}{}", emit::flow(value))],
        (_, _) if entry.style == Style::Flow || entry.in_flow => {
            if collection {
                vec![emit::flow(value)]
            } else {
                emit::scalars(value, Style::Plain, true, 2, nl)
            }
        }
        (_, true) => {
            let indent = column(src, start);
            let step = site.step(entry);
            vec![emit::block(value, indent, step, nl), emit::flow(value)]
        }
        (_, false) => {
            // A block collection becomes a scalar on its first line, deeper
            // than the key or dash that holds it.
            let deeper = entry
                .lead
                .is_some_and(|l| column(src, start) <= column(src, l));
            let pad = if deeper { "  " } else { "" };
            emit::scalars(value, Style::Plain, false, 2, nl)
                .into_iter()
                .map(|text| format!("{pad}{text}"))
                .collect()
        }
    };
    Ok(splices(start, end, texts))
}

/// The new entry's value: a literal, or exact YAML text.
enum Payload<'a> {
    Value(&'a Value),
    Text(&'a str),
}

/// Add an entry after a mapping's last one.
fn insert(
    site: &Site<'_>,
    mapping: &Entry,
    key: &str,
    payload: &Payload<'_>,
) -> Result<Vec<Splice>, Refusal> {
    if mapping.kind != Kind::Mapping {
        return Err(Refusal::Shape {
            path: mapping.path.clone(),
            detail: "it is not a mapping".to_owned(),
        });
    }
    let nl = site.nl();
    if mapping.style == Style::Flow {
        let value = match payload {
            Payload::Value(value) => emit::flow_item(value),
            Payload::Text(text) if !text.trim_end().contains('\n') => text.trim().to_owned(),
            Payload::Text(_) => {
                return Err(layout(
                    &mapping.path,
                    "exact multi-line text inside a flow mapping",
                ));
            }
        };
        let entry = format!("{}: {value}", emit::key(key, true));
        let end = end_of(mapping)?;
        return Ok(match site.children(mapping).last() {
            Some(last) => {
                let at = end_of(last)?;
                vec![Splice::new(at, at, format!(", {entry}"))]
            }
            None => vec![Splice::new(end - 1, end - 1, entry)],
        });
    }
    let last = site
        .children(mapping)
        .last()
        .ok_or_else(|| layout(&mapping.path, "an empty block mapping"))?;
    let src = site.src();
    let indent = last.lead.map_or(0, |l| column(src, l));
    let step = site.step(mapping);
    let (at, before) = after_line(site, end_of(last)?);
    let name = emit::key(key, false);
    let pad = " ".repeat(indent);
    let tails = match payload {
        Payload::Value(value) => vec![emit::tail(value, indent, step, nl)],
        Payload::Text(text) => text_tails(text, indent + step, nl),
    };
    Ok(tails
        .into_iter()
        .map(|tail| Splice::new(at, at, format!("{before}{pad}{name}:{tail}{nl}")))
        .collect())
}

/// The candidate tails of exact text under a new key: on the key's line when
/// it is one line, then always on the following lines, re-indented to `indent`.
fn text_tails(text: &str, indent: usize, nl: &str) -> Vec<String> {
    let body = text.trim_end_matches(['\n', '\r']);
    let pad = " ".repeat(indent);
    let lines: Vec<String> = body
        .lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect();
    let mut out = Vec::new();
    if !body.contains('\n') {
        out.push(format!(" {}", body.trim_start()));
    }
    out.push(format!("{nl}{}", lines.join(nl)));
    out
}

/// Where a new line goes after the line holding `end`: the next line's start,
/// or the document's end behind a fresh line break.
fn after_line(site: &Site<'_>, end: usize) -> (usize, &'static str) {
    match next_line(site.src(), end) {
        Some(at) => (at, ""),
        None => (site.source.len(), site.nl()),
    }
}

/// Append an item after a sequence's last one.
fn push(site: &Site<'_>, seq: &Entry, value: &Value) -> Result<Vec<Splice>, Refusal> {
    if seq.kind != Kind::Sequence {
        return Err(Refusal::Shape {
            path: seq.path.clone(),
            detail: "it is not a sequence".to_owned(),
        });
    }
    let nl = site.nl();
    if seq.style == Style::Flow {
        let item = emit::flow_item(value);
        let end = end_of(seq)?;
        return Ok(match site.children(seq).last() {
            Some(last) => {
                let at = end_of(last)?;
                vec![Splice::new(at, at, format!(", {item}"))]
            }
            None => vec![Splice::new(end - 1, end - 1, item)],
        });
    }
    let last = site
        .children(seq)
        .last()
        .ok_or_else(|| layout(&seq.path, "an empty block sequence"))?;
    let src = site.src();
    let dash = last
        .lead
        .ok_or_else(|| layout(&seq.path, "an item without its dash"))?;
    let indent = column(src, dash);
    let step = site.step(seq);
    let (at, before) = after_line(site, end_of(last)?);
    let item = emit::item_text(value, indent + 2, step, nl);
    let pad = " ".repeat(indent);
    Ok(vec![Splice::new(
        at,
        at,
        format!("{before}{pad}- {item}{nl}"),
    )])
}

/// Remove an entry or item with the bytes that belong to it alone.
fn remove(site: &Site<'_>, path: &Path) -> Result<Vec<Splice>, Refusal> {
    let target = site.entry(path)?;
    let parent_path = path.parent().ok_or_else(|| Refusal::Shape {
        path: path.clone(),
        detail: "the document root cannot be removed".to_owned(),
    })?;
    let parent = site.entry(&parent_path)?;
    let siblings: Vec<&Entry> = site.children(parent).collect();
    let k = siblings
        .iter()
        .position(|e| e.path == target.path)
        .ok_or_else(|| Refusal::UnknownPath { path: path.clone() })?;
    let src = site.src();
    let lead = target.lead.unwrap_or(target.start);
    let end = end_of(target)?;
    if parent.style == Style::Flow {
        let range = if siblings.len() == 1 {
            (lead, end)
        } else if let Some(next) = siblings.get(k + 1) {
            (lead, next.lead.unwrap_or(next.start))
        } else {
            let previous = siblings
                .get(k.wrapping_sub(1))
                .ok_or_else(|| layout(path, "an item between unreadable neighbours"))?;
            (end_of(previous)?, end)
        };
        return Ok(vec![Splice::new(range.0, range.1, String::new())]);
    }
    if siblings.len() == 1 {
        let empty = if parent.kind == Kind::Sequence {
            "[]"
        } else {
            "{}"
        };
        let parent_end = end_of(parent)?;
        return Ok(vec![Splice::new(
            parent.start,
            parent_end,
            empty.to_owned(),
        )]);
    }
    let from = line_start(src, lead);
    if skip_blank(src, from) != lead {
        return Err(layout(path, "an entry that shares its line with another"));
    }
    let to = next_line(src, end).unwrap_or(src.len());
    Ok(vec![Splice::new(from, to, String::new())])
}
