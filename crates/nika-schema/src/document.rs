// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The complete `.nika` document: its exact bytes, the strict parser's AST
//! of those bytes, their literal projection and where every node sits, with
//! typed targeted edits that rewrite only the addressed bytes.
//!
//! This is the program representation itself, not a projection of it: any
//! document the strict parser accepts is imported whole, whatever constructs
//! it uses, and an edit keeps every byte outside its span. Each edit is
//! proven twice before it is kept, by two independent readings:
//!
//! - **meaning** — the edited bytes are parsed again by [`crate::parse`]
//!   (strict); their literal projection must equal the projection the edit
//!   predicts, and every top-level component the edit does not address must
//!   read the same in the AST;
//! - **bytes** — every byte outside the edit's [`Splice`] is the previous
//!   document's byte ([`Applied::bytes_preserved`] replays the splices).
//!
//! A presentation this reading cannot place is refused as
//! [`Refusal::Layout`] with the source unchanged; nothing is ever
//! reserialized to make an edit fit. Pure: no I/O, no hashing, no authority;
//! revision identity and lineage live with the compiler's fidelity layer.
//!
//! ```
//! use nika_schema::document::{Document, Edit, Path};
//! let source = "nika: window\nconst:\n  hours: 48 # the window\ntasks:\n  t:\n    exec:\n      command: [\"echo\", \"${{ const.hours }}\"]\n";
//! let document = Document::parse(source)?;
//! let hours = Path::pointer("/const/hours").expect("a pointer");
//! let applied = document.apply(&[Edit::set(hours, serde_json::json!(72))])?;
//! assert_eq!(applied.document().source(), source.replace("48", "72"));
//! assert!(applied.bytes_preserved(source));
//! # Ok::<(), nika_schema::document::Refusal>(())
//! ```

mod edit;
mod emit;
mod layout;
mod path;
mod place;
mod refusal;
mod rename;
mod semantic;

pub use edit::Edit;
pub use path::Path;
pub use refusal::Refusal;

use std::collections::BTreeSet;
use std::ops::Range;

use serde_json::{Value, json};

use crate::raw::RawWorkflow;
use crate::types::VarDecl;
use crate::{FileId, ParseMode};
use layout::Layout;

/// How a node is written in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Style {
    /// An indented block mapping or sequence.
    Block,
    /// A `{ … }` or `[ … ]` collection.
    Flow,
    /// A plain scalar.
    Plain,
    /// A `'…'` scalar.
    SingleQuoted,
    /// A `"…"` scalar.
    DoubleQuoted,
    /// A `|` block scalar.
    Literal,
    /// A `>` block scalar.
    Folded,
    /// An omitted value (`key:` with nothing after it).
    Empty,
}

impl Style {
    /// The stable machine word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Flow => "flow",
            Self::Plain => "plain",
            Self::SingleQuoted => "single",
            Self::DoubleQuoted => "double",
            Self::Literal => "literal",
            Self::Folded => "folded",
            Self::Empty => "empty",
        }
    }
}

/// What a node holds, as the literal projection reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NodeKind {
    /// A mapping.
    Mapping,
    /// A sequence.
    Sequence,
    /// A string.
    String,
    /// A number.
    Number,
    /// `true` or `false`.
    Boolean,
    /// `null`, `~` or an omitted value.
    Null,
}

impl NodeKind {
    fn of(value: &Value) -> Self {
        match value {
            Value::Object(_) => Self::Mapping,
            Value::Array(_) => Self::Sequence,
            Value::String(_) => Self::String,
            Value::Number(_) => Self::Number,
            Value::Bool(_) => Self::Boolean,
            Value::Null => Self::Null,
        }
    }

    /// The stable machine word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Mapping => "mapping",
            Self::Sequence => "sequence",
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Null => "null",
        }
    }
}

/// One addressable node of a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Node {
    /// Its address.
    pub path: Path,
    /// What it holds.
    pub kind: NodeKind,
    /// How it is written.
    pub style: Style,
    /// The bytes of its value, when this reading places them.
    pub span: Option<Range<usize>>,
    /// The closed block of the language whose vocabulary the strict parser
    /// held this node's key to (`envelope` · `task` · `verb` · `infer` ·
    /// `retry` · `for_each` · `lift` · `permits` · …); `None` for a name the
    /// author chose or a key inside free data (arguments, bindings, schemas,
    /// untyped constant values).
    pub keyset: Option<&'static str>,
    /// Its literal value.
    pub value: Value,
}

impl Node {
    /// `{"path", "kind", "style", "span"?, "keyset"?, "value"}`, the pointer as `path`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut out = json!({
            "path": self.path.to_pointer(),
            "kind": self.kind.word(),
            "style": self.style.word(),
            "value": self.value,
        });
        if let Some(span) = &self.span {
            out["span"] = json!([span.start, span.end]);
        }
        if let Some(keyset) = self.keyset {
            out["keyset"] = json!(keyset);
        }
        out
    }
}

/// One replacement of a document's bytes: `start..end` of the text it was
/// applied to became `replacement`. Every other byte stayed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Splice {
    /// First replaced byte.
    pub start: usize,
    /// One past the last replaced byte (`start` for a pure insertion).
    pub end: usize,
    /// The bytes written in their place.
    pub replacement: String,
}

impl Splice {
    /// A replacement of `start..end` by `replacement`.
    #[must_use]
    pub fn new(start: usize, end: usize, replacement: impl Into<String>) -> Self {
        Self {
            start,
            end,
            replacement: replacement.into(),
        }
    }

    /// `text` with this splice applied; `None` when the range is not one of `text`.
    #[must_use]
    pub fn apply(&self, text: &str) -> Option<String> {
        let before = text.get(..self.start)?;
        let after = text.get(self.end..)?;
        Some(format!("{before}{}{after}", self.replacement))
    }
}

/// A complete document at its exact bytes. See the module documentation.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Document {
    source: String,
    workflow: RawWorkflow,
    literal: Value,
    layout: Layout,
}

/// The result of [`Document::apply`]: the revised document, the nodes the
/// edits addressed, and the splices that made it, one per edit, each in the
/// coordinates of the text it was applied to.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Applied {
    document: Document,
    changed: Vec<Path>,
    splices: Vec<Splice>,
}

impl Applied {
    /// The revised document.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The revised document, owned.
    #[must_use]
    pub fn into_document(self) -> Document {
        self.document
    }

    /// The node each edit wrote, removed or added, in edit order.
    #[must_use]
    pub fn changed(&self) -> &[Path] {
        &self.changed
    }

    /// The splices, in edit order.
    #[must_use]
    pub fn splices(&self) -> &[Splice] {
        &self.splices
    }

    /// Whether replaying the splices on `base` yields exactly the revised
    /// bytes: every byte outside them is `base`'s own.
    #[must_use]
    pub fn bytes_preserved(&self, base: &str) -> bool {
        let mut text = base.to_owned();
        for splice in &self.splices {
            match splice.apply(&text) {
                Some(next) => text = next,
                None => return false,
            }
        }
        text == self.document.source
    }
}

/// The literal projection of a whole document: every value as the strict
/// parser's own free-form literal decoder reads it (quoted scalars are
/// strings, plain scalars follow YAML 1.2 core), the document nested as one
/// untyped constant. The same reading as `nika_compile::surface::literal_projection`,
/// a leading byte-order mark excepted. `None` when the document is not one
/// YAML mapping the parser reads.
#[must_use]
pub fn literal_projection(source: &str) -> Option<Value> {
    let body = source.strip_prefix('\u{FEFF}').unwrap_or(source);
    let indented = body
        .lines()
        .map(|line| {
            if stream_marker(line) {
                String::new()
            } else {
                format!("    {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let projected = format!("nika: compile-literal-view\nconst:\n  document:\n{indented}\n");
    let workflow = crate::parse(&projected, FileId::new(0), ParseMode::Strict).ok()?;
    let (_, VarDecl::Untyped(value)) = workflow.consts.into_iter().next()? else {
        return None;
    };
    Some(value)
}

/// A line the YAML stream reads and the document's one mapping never holds:
/// a `%` directive, or a bare `---` start or `...` end marker at column 0
/// (block content is always indented, so such a line is never a value's).
/// A marker followed by content stays, and the projection then fails loud.
fn stream_marker(line: &str) -> bool {
    let marker = |m: &str| {
        line.strip_prefix(m).is_some_and(|rest| {
            let rest = rest.trim_start_matches([' ', '\t']);
            rest.is_empty() || (rest.starts_with('#') && rest.len() < line.len() - m.len())
        })
    };
    line.starts_with('%') || marker("---") || marker("...")
}

/// The value exact YAML `text` states, read as the projection reads values.
fn text_value(text: &str) -> Option<Value> {
    let body = text.trim_end_matches(['\n', '\r']);
    let indented = body
        .lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let projected = format!("nika: compile-literal-view\nconst:\n  v:\n{indented}\n");
    let workflow = crate::parse(&projected, FileId::new(0), ParseMode::Strict).ok()?;
    let (_, VarDecl::Untyped(value)) = workflow.consts.into_iter().next()? else {
        return None;
    };
    Some(value)
}

impl Document {
    /// Import exact bytes: the strict parser's AST of them, their literal
    /// projection and every node's position. The bytes are kept verbatim.
    ///
    /// # Errors
    /// [`Refusal::Language`] when the strict parser refuses the bytes;
    /// [`Refusal::Layout`] when their projection or positions cannot be read.
    pub fn parse(source: impl Into<String>) -> Result<Self, Refusal> {
        let source = source.into();
        let root = Path::root();
        let workflow =
            crate::parse(&source, FileId::new(0), ParseMode::Strict).map_err(|error| {
                Refusal::Language {
                    path: root.clone(),
                    error: Box::new(error),
                }
            })?;
        let unread = |detail: &str| Refusal::Layout {
            path: root.clone(),
            detail: detail.to_owned(),
        };
        let literal = literal_projection(&source)
            .ok_or_else(|| unread("its literal projection cannot be read"))?;
        let layout =
            Layout::build(&source).ok_or_else(|| unread("its positions cannot be read"))?;
        Ok(Self {
            source,
            workflow,
            literal,
            layout,
        })
    }

    /// The exact bytes.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The strict parser's AST of exactly these bytes.
    #[must_use]
    pub fn workflow(&self) -> &RawWorkflow {
        &self.workflow
    }

    /// The literal projection of these bytes ([`literal_projection`]).
    #[must_use]
    pub fn literal(&self) -> &Value {
        &self.literal
    }

    /// The literal value at `path`.
    #[must_use]
    pub fn value(&self, path: &Path) -> Option<&Value> {
        self.literal.pointer(&path.to_pointer())
    }

    /// The path a binding of constant `name` writes: its `value` when it is
    /// declared typed (`{type, value}`), the constant itself otherwise.
    #[must_use]
    pub fn constant_path(&self, name: &str) -> Option<Path> {
        let (_, decl) = self.workflow.consts.iter().find(|(n, _)| n.value == name)?;
        let path = Path::new(["const", name]);
        Some(if matches!(decl, VarDecl::Typed { .. }) {
            path.child("value")
        } else {
            path
        })
    }

    /// The node at `path`.
    #[must_use]
    pub fn node(&self, path: &Path) -> Option<Node> {
        let entry = self.layout.get(path)?;
        self.node_of(entry)
    }

    /// Every node, in document order, the root first.
    #[must_use]
    pub fn nodes(&self) -> Vec<Node> {
        self.layout
            .entries
            .iter()
            .filter_map(|entry| self.node_of(entry))
            .collect()
    }

    fn node_of(&self, entry: &layout::Entry) -> Option<Node> {
        let value = self.value(&entry.path)?.clone();
        Some(Node {
            path: entry.path.clone(),
            kind: NodeKind::of(&value),
            style: entry.style,
            span: entry.end.map(|end| entry.start..end),
            keyset: keyset(&self.workflow, &entry.path),
            value,
        })
    }

    /// Apply `edits` in order, each to the document the previous one left.
    /// An empty list returns the document unchanged.
    ///
    /// # Errors
    /// The first edit that is refused, with the source unchanged.
    pub fn apply(&self, edits: &[Edit]) -> Result<Applied, Refusal> {
        let mut document = self.clone();
        let mut changed = Vec::with_capacity(edits.len());
        let mut splices = Vec::with_capacity(edits.len());
        for edit in edits {
            let (next, made, target) = document.step(edit)?;
            document = next;
            changed.push(target);
            splices.extend(made);
        }
        Ok(Applied {
            document,
            changed,
            splices,
        })
    }

    /// One edit: predict the projection, place the candidate splices, keep the
    /// first one both readings accept.
    fn step(&self, edit: &Edit) -> Result<(Self, Vec<Splice>, Path), Refusal> {
        let path = edit.path();
        if let Edit::Rename { to, .. } = edit {
            return self.rename(path, to);
        }
        let text = match edit {
            Edit::InsertText { text, .. } => {
                Some(text_value(text).ok_or_else(|| Refusal::Shape {
                    path: path.clone(),
                    detail: "the inserted text is not one YAML value".to_owned(),
                })?)
            }
            _ => None,
        };
        let expected = edit::expected(&self.literal, edit, text.as_ref())?;
        let target = match edit {
            Edit::Insert { key, .. } | Edit::InsertText { key, .. } => path.child(key.as_str()),
            Edit::Push { .. } => {
                let len = self
                    .value(path)
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                path.child(len.to_string())
            }
            _ => path.clone(),
        };
        let site = place::Site {
            source: &self.source,
            layout: &self.layout,
        };
        let mut last = None;
        for splice in place::candidates(&site, edit)? {
            let Some(text) = splice.apply(&self.source) else {
                continue;
            };
            match self.judge(text, &expected, path, std::slice::from_ref(&target)) {
                Ok(next) => return Ok((next, vec![splice], target)),
                Err(refusal) => last = Some(refusal),
            }
        }
        Err(last.unwrap_or_else(|| Refusal::Layout {
            path: path.clone(),
            detail: "no text can be placed there".to_owned(),
        }))
    }

    /// A rename: each reference rewritten as its own judged edit, then the
    /// keys, the entity's own last; every splice is kept in order.
    fn rename(&self, path: &Path, to: &str) -> Result<(Self, Vec<Splice>, Path), Refusal> {
        let plan = rename::plan(&self.literal, path, to)?;
        let mut document = self.clone();
        let mut splices = Vec::new();
        for (at, text) in plan.strings {
            let (next, made, _) = document.step(&Edit::set(at, Value::String(text)))?;
            document = next;
            splices.extend(made);
        }
        for (at, key) in plan.keys {
            let (next, splice) = document.rekey(&at, &key)?;
            document = next;
            splices.push(splice);
        }
        let target = path.parent().map_or_else(Path::root, |p| p.child(to));
        Ok((document, splices, target))
    }

    /// One mapping key replaced in place, its value and everything else kept.
    fn rekey(&self, path: &Path, to: &str) -> Result<(Self, Splice), Refusal> {
        let edit = Edit::rename(path.clone(), to);
        let expected = edit::expected(&self.literal, &edit, None)?;
        let site = place::Site {
            source: &self.source,
            layout: &self.layout,
        };
        let splice = place::rekey(&site, path, to)?;
        let text = splice.apply(&self.source).ok_or_else(|| Refusal::Layout {
            path: path.clone(),
            detail: "the key cannot be placed".to_owned(),
        })?;
        let renamed = path.parent().map_or_else(Path::root, |p| p.child(to));
        let next = self.judge(text, &expected, path, &[path.clone(), renamed])?;
        Ok((next, splice))
    }

    /// Both readings of an edited text: the strict parser accepts it, its
    /// projection is the predicted one, and every component outside the
    /// targets reads the same in the AST.
    fn judge(
        &self,
        text: String,
        expected: &Value,
        path: &Path,
        targets: &[Path],
    ) -> Result<Self, Refusal> {
        let revised = match Self::parse(text) {
            Ok(document) => document,
            Err(Refusal::Language { error, .. }) => {
                return Err(Refusal::Language {
                    path: path.clone(),
                    error,
                });
            }
            // The edited bytes parse but cannot be placed: never a verdict.
            Err(other) => {
                return Err(Refusal::Layout {
                    path: path.clone(),
                    detail: other.to_string(),
                });
            }
        };
        if &revised.literal != expected {
            let at = first_difference(expected, &revised.literal, "");
            return Err(Refusal::Drift {
                path: path.clone(),
                detail: format!("the reparsed literal differs at `{at}`"),
            });
        }
        let touched: Option<Vec<String>> = targets.iter().map(semantic::component).collect();
        if let Some(touched) = touched {
            let before = semantic::components(&self.workflow);
            let after = semantic::components(&revised.workflow);
            let ids: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
            let untouched = |id: &&String| !touched.iter().any(|t| semantic::within(id, t));
            if let Some(id) = ids
                .into_iter()
                .filter(untouched)
                .find(|id| before.get(*id) != after.get(*id))
            {
                return Err(Refusal::Drift {
                    path: path.clone(),
                    detail: format!("the parser reads `{id}` differently"),
                });
            }
        }
        Ok(revised)
    }
}

/// The first pointer where `expected` and `found` differ.
fn first_difference(expected: &Value, found: &Value, at: &str) -> String {
    match (expected, found) {
        (Value::Object(wanted), Value::Object(read)) => {
            for key in wanted.keys().chain(read.keys()) {
                let next = format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
                match (wanted.get(key), read.get(key)) {
                    (Some(left), Some(right)) if left == right => {}
                    (Some(left), Some(right)) => return first_difference(left, right, &next),
                    _ => return next,
                }
            }
            at.to_owned()
        }
        (Value::Array(wanted), Value::Array(read)) if wanted.len() == read.len() => wanted
            .iter()
            .zip(read)
            .enumerate()
            .find(|(_, (left, right))| left != right)
            .map_or_else(
                || at.to_owned(),
                |(index, (left, right))| first_difference(left, right, &format!("{at}/{index}")),
            ),
        _ if at.is_empty() => "/".to_owned(),
        _ => at.to_owned(),
    }
}

/// The closed block of the language that holds the last key of `path`: the
/// strict parser refuses any other key there, so the key of a parsed
/// document belongs to that block's vocabulary by construction. `None` for a
/// name the author chose (a task id, an input, a binding) or a key inside
/// free data (arguments, bindings, schemas, untyped constant values).
fn keyset(workflow: &RawWorkflow, path: &Path) -> Option<&'static str> {
    let segments: Vec<&str> = path.segments().iter().map(String::as_str).collect();
    let verb = |id: &str| {
        workflow
            .tasks
            .iter()
            .find(|t| t.value.id.value == id)
            .map(|t| t.value.action.verb())
    };
    let typed_const = |name: &str| {
        workflow
            .consts
            .iter()
            .any(|(n, d)| n.value == name && matches!(d, VarDecl::Typed { .. }))
    };
    let block = match segments.as_slice() {
        [_] => "envelope",
        ["inputs", _, _] => "inputs",
        ["const", name, _] if typed_const(name) => "const",
        ["secrets", _, _] => "secrets",
        ["secrets", _, "egress", _, _] => "egress",
        ["outputs", _, _] => "outputs",
        ["run", _] => "run",
        ["run", "entropy", _] => "run.entropy",
        ["permits", _] => "permits",
        ["permits", "fs", _] => "permits.fs",
        ["permits", "net", _] => "permits.net",
        ["tasks", id, key] if verb(id) == Some(*key) => "verb",
        ["tasks", _, _] => "task",
        ["tasks", id, block, _] if verb(id) == Some(*block) => verb(id)?,
        ["tasks", _, "retry", _] => "retry",
        ["tasks", _, "on_error", _] => "on_error",
        ["tasks", _, "for_each", _] => "for_each",
        ["tasks", _, "lift", _, _] => "lift",
        ["tasks", id, "infer", "thinking", _] if verb(id) == Some("infer") => "thinking",
        ["tasks", id, "infer", "vision", _, _] if verb(id) == Some("infer") => "vision",
        _ => return None,
    };
    Some(block)
}

#[cfg(test)]
mod tests;
