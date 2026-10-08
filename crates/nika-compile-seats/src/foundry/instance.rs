// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A component instantiated and expanded into a document (C13 · R5): the admitted bytes, bound
//! at their holes by bounded literal edits, then merged into a parent `.nika` document — its
//! `inputs`, `const`, `tasks` and `outputs` entries added beside the parent's, each name once.
//!
//! An expansion grants nothing and observes nothing. The component's `permits:` never reach the
//! document: what it declares is recorded as a need, and the parent's own boundary (the human's)
//! is what Check judges. Its `model:` and its `nika:` name are its probe's, not the request's:
//! dropped and recorded. A hole left open is refused, so a probe's path, field or default is never
//! taken for the request's. A name the parent already holds is refused, never renamed behind the
//! author's back. The merge is proven by the parser: the expanded document's literal projection
//! must equal the parent's with exactly the component's entries added. The result is checked as
//! a whole, and the receipt binds the component, the bindings and the digest of every node the
//! expansion produced to the exact candidate bytes ([`super::witness`] re-derives them).

use std::fmt;

use nika_compile::CompileStatus;
use nika_compile::surface::{finish, initial, literal_projection, sha256};
use nika_compile_fidelity::literal::literal_at;
use serde_json::{Map, Value, json};

use super::bind::{Binding, BindingError, edit_literal, judge, open_holes};
use super::component::Component;

/// The sections an expansion merges, entry by entry, in the envelope's order.
pub const MERGED: [&str; 4] = ["inputs", "const", "tasks", "outputs"];

/// The envelope's top-level keys, in their order: where a new section is placed.
const ENVELOPE: [&str; 9] = [
    "nika", "model", "inputs", "const", "secrets", "permits", "run", "tasks", "outputs",
];

/// The law an expansion receipt states.
pub const EXPANDED: &str = "expanded: an admitted component's exact bytes, bound at its holes by literal edits the parser proved, merged into the document; its permits, model and name are not inherited";

/// A component bound at its holes: the bound program and what was bound.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Instance {
    /// The component, as resolved.
    pub component: Component,
    /// The bindings, as applied.
    pub bindings: Vec<Binding>,
    /// The component's bytes with each bound literal replaced, every other byte kept.
    pub source: String,
    /// The holes no binding closes.
    pub open: Vec<String>,
}

impl Instance {
    /// The bindings as a receipt names them: path, hole, owner, the component's literal there
    /// and the bound one.
    #[must_use]
    pub fn bindings_record(&self) -> Value {
        let mut document = literal_projection(&self.component.source).unwrap_or(Value::Null);
        let rows: Vec<Value> = (self.bindings.iter())
            .map(|binding| {
                let hole = self.component.hole(&binding.path);
                let held =
                    literal_at(&mut document, &binding.path).map_or(Value::Null, |v| v.clone());
                json!({
                    "path": binding.path,
                    "hole": hole.map(|h| h.name.clone()),
                    "owner": hole.map(|h| h.owner.clone()),
                    "component_literal": held,
                    "bound": binding.value,
                })
            })
            .collect();
        Value::Array(rows)
    }
}

/// Bind `component` at `bindings`: each judged first ([`judge`]), then applied by a bounded
/// literal edit on the admitted bytes. Holes may stay open here; [`expand`] refuses them.
///
/// # Errors
/// The first binding refused ([`BindingError`]); nothing is returned half-bound.
pub fn instantiate(component: &Component, bindings: &[Binding]) -> Result<Instance, BindingError> {
    judge(component, bindings)?;
    let mut source = component.source.clone();
    for binding in bindings {
        source =
            edit_literal(&source, &binding.path, &binding.value).map_err(BindingError::Unproven)?;
    }
    Ok(Instance {
        component: component.clone(),
        bindings: bindings.to_vec(),
        source,
        open: open_holes(component, bindings),
    })
}

/// Why an instance is not expanded into a document. The parent is never changed by a refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExpandError {
    /// The instance is not bound: open holes or a refused binding.
    Binding(BindingError),
    /// The parent's literals cannot be read.
    Parent,
    /// The parent already holds an entry of that name.
    Collision {
        /// `inputs` · `const` · `tasks` · `outputs`.
        section: String,
        /// The name both hold.
        name: String,
    },
    /// The component carries a section an expansion cannot merge (`secrets`, or a `run:`
    /// declaration other than the parent's): it governs the whole program.
    Unmergeable(String),
    /// A section is written in a presentation the merge does not prove (a flow mapping with
    /// entries), or the merged document's projection is not the expected one.
    Unproven(String),
}

impl fmt::Display for ExpandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Binding(error) => write!(f, "the component is not bound: {error}"),
            Self::Parent => f.write_str("the document's literals cannot be read"),
            Self::Collision { section, name } => write!(
                f,
                "the document already holds `{section}.{name}`: the component's entry is not renamed behind the author; invoke it as a child workflow or name the parent's entry otherwise"
            ),
            Self::Unmergeable(section) => write!(
                f,
                "the component declares `{section}`, which governs the whole program: not merged into the document"
            ),
            Self::Unproven(why) => write!(f, "the merge is not proven: {why}"),
        }
    }
}

impl std::error::Error for ExpandError {}

/// An instance merged into a document: the candidate, its Check, and the receipt.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Expansion {
    /// The expanded document.
    pub candidate: String,
    /// Whether the whole document passed the pure Check preview, with nothing left to ask.
    pub ready: bool,
    /// The receipt: component, bindings, dropped facts, authority needed, nodes and Check.
    pub receipt: Value,
}

/// Expand `instance` into `parent`: its `inputs`, `const`, `tasks` and `outputs` entries added
/// beside the parent's, the whole checked. `parent` may hold nothing but its envelope
/// (`nika: <id>` and the human's `permits:`).
///
/// # Errors
/// [`ExpandError`]: open holes, a name the parent holds, a section that cannot merge, or a merge
/// the parser does not prove.
pub fn expand(parent: &str, instance: &Instance) -> Result<Expansion, ExpandError> {
    if !instance.open.is_empty() {
        return Err(ExpandError::Binding(BindingError::Unbound(
            instance.open.clone(),
        )));
    }
    let base = literal_projection(parent).ok_or(ExpandError::Parent)?;
    let bound = literal_projection(&instance.source)
        .ok_or_else(|| ExpandError::Unproven("the bound component cannot be read".to_owned()))?;
    compatible(&base, &bound)?;
    let mut expected = base.clone();
    let mut candidate = parent.to_owned();
    for section in MERGED {
        let Some(Value::Object(entries)) = bound.get(section) else {
            continue;
        };
        if entries.is_empty() {
            continue;
        }
        let target = expected
            .as_object_mut()
            .ok_or(ExpandError::Parent)?
            .entry(section)
            .or_insert_with(|| Value::Object(Map::new()));
        let Some(target) = target.as_object_mut() else {
            return Err(ExpandError::Unproven(format!(
                "`{section}` is not a mapping"
            )));
        };
        for (name, value) in entries {
            if target.contains_key(name) {
                return Err(ExpandError::Collision {
                    section: section.to_owned(),
                    name: name.clone(),
                });
            }
            target.insert(name.clone(), value.clone());
        }
        let body = section_body(&instance.source, section)?;
        candidate = merge_section(&candidate, section, &body)?;
    }
    if literal_projection(&candidate).as_ref() != Some(&expected) {
        return Err(ExpandError::Unproven(
            "the expanded document does not read as the parent plus the component's entries"
                .to_owned(),
        ));
    }
    Ok(checked(candidate, instance, &bound))
}

/// Whether the component's whole-program sections fit the document: a component's `secrets`
/// never merge, and its `run` must be the document's own. Both the expansion and the adoption of
/// another editor's insert judge it, so neither carries a component the other refuses.
fn compatible(document: &Value, bound: &Value) -> Result<(), ExpandError> {
    if bound.get("secrets").is_some() {
        return Err(ExpandError::Unmergeable("secrets".to_owned()));
    }
    if let Some(run) = bound.get("run")
        && document.get("run") != Some(run)
    {
        return Err(ExpandError::Unmergeable("run".to_owned()));
    }
    Ok(())
}

/// A block scalar's exact text: its header line (`|`, `>-`, a comment after it), then its content
/// lines re-indented to zero, the trailing blank lines kept when its header keeps them (`+`). An
/// explicit indentation indicator counts from the key's own column, which an editor chooses:
/// unproven, never a text that would read otherwise.
fn block_scalar(header: &str, lines: &[&str]) -> Result<String, ExpandError> {
    let indicators = header.split('#').next().unwrap_or_default().trim();
    if indicators.chars().any(|c| c.is_ascii_digit()) {
        return Err(ExpandError::Unproven(format!(
            "the block scalar `{header}` states an indentation indicator"
        )));
    }
    let mut content = lines.to_vec();
    if !indicators.contains('+') {
        while content.last().is_some_and(|line| blank(line)) {
            content.pop();
        }
    }
    Ok(std::iter::once(header)
        .chain(content)
        .collect::<Vec<_>>()
        .join("\n"))
}

/// One entry an expansion adds: its section, its name, and its value as the component writes it
/// (its lines without their indentation), for a document editor that inserts exact text.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Entry {
    /// `inputs` · `const` · `tasks` · `outputs`.
    pub section: String,
    /// The entry's key.
    pub name: String,
    /// Its value's exact text: an inline value (for a block scalar, its header line and then its
    /// content lines re-indented to zero, the blank lines a keep indicator keeps included), or
    /// the block under the key, re-indented to zero. Every line counts, the last blank ones too.
    pub text: String,
}

impl Instance {
    /// The entries an expansion of this instance adds, in the envelope's section order and the
    /// component's own order, each value as the bound bytes write it. A comment line between two
    /// entries is not carried here ([`expand`] keeps it).
    ///
    /// # Errors
    /// [`ExpandError::Unproven`] when a section is not written in block form, or a block scalar
    /// states an indentation indicator.
    pub fn entries(&self) -> Result<Vec<Entry>, ExpandError> {
        let mut entries = Vec::new();
        for key in MERGED {
            if section(&self.source, key).is_none() {
                continue;
            }
            let body = section_body(&self.source, key)?;
            let mut lines = body.iter().peekable();
            while let Some(line) = lines.next() {
                let Some((name, inline)) = line.split_once(':') else {
                    continue;
                };
                if line.starts_with([' ', '#']) || line.is_empty() {
                    continue;
                }
                let mut value: Vec<&str> = Vec::new();
                while let Some(next) =
                    lines.next_if(|next| next.is_empty() || next.starts_with(' '))
                {
                    value.push(next);
                }
                let indent = (value.iter())
                    .filter(|l| !blank(l))
                    .map(|l| spaces(l))
                    .min()
                    .unwrap_or(0);
                let block: Vec<&str> = value
                    .iter()
                    .map(|l| l.get(indent..).unwrap_or(""))
                    .collect();
                // The separation after the colon is spaces and tabs; a no-break space is content.
                let header = inline.trim_matches([' ', '\t']);
                let text = if header.is_empty() {
                    let joined = block.join("\n");
                    joined.trim_end_matches([' ', '\n']).to_owned()
                } else if header.starts_with(['|', '>']) {
                    block_scalar(header, &block)?
                } else {
                    header.to_owned()
                };
                entries.push(Entry {
                    section: key.to_owned(),
                    name: name.to_owned(),
                    text,
                });
            }
        }
        Ok(entries)
    }
}

/// The receipt of an expansion another editor applied (the document owner's exact-text insert of
/// [`Instance::entries`]): `candidate` must hold every entry of the bound instance exactly as
/// bound, and no hole may stay open; it is then checked as a whole and receipted as [`expand`]
/// receipts its own.
///
/// # Errors
/// [`ExpandError`]: open holes, a section the component cannot merge (its `secrets`, a `run`
/// the candidate does not share), or an entry the candidate does not hold as bound.
pub fn adopt(candidate: &str, instance: &Instance) -> Result<Expansion, ExpandError> {
    if !instance.open.is_empty() {
        return Err(ExpandError::Binding(BindingError::Unbound(
            instance.open.clone(),
        )));
    }
    let document = literal_projection(candidate).ok_or(ExpandError::Parent)?;
    let bound = literal_projection(&instance.source)
        .ok_or_else(|| ExpandError::Unproven("the bound component cannot be read".to_owned()))?;
    compatible(&document, &bound)?;
    for section in MERGED {
        for (name, value) in bound
            .get(section)
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if document.get(section).and_then(|s| s.get(name)) != Some(value) {
                return Err(ExpandError::Unproven(format!(
                    "the candidate does not hold `{section}.{name}` as the bound component writes it"
                )));
            }
        }
    }
    Ok(checked(candidate.to_owned(), instance, &bound))
}

/// A whole document through the compiler's finish (strict parse, pure Check preview, open
/// questions): whether it is ready, the record of what was found, and the boundary its body
/// needs as Check derives it.
pub(super) fn check(candidate: &str) -> (bool, Value, Value) {
    let mut out = initial();
    finish(candidate.to_owned(), &mut out);
    let ready = out.status == CompileStatus::Ready;
    let report = (out.check_preview.as_ref())
        .and_then(|preview| serde_json::to_value(&preview.report).ok())
        .unwrap_or(Value::Null);
    let findings: Vec<Value> = (report["findings"].as_array().into_iter().flatten())
        .map(|f| json!({"code": f["code"], "message": f["message"]}))
        .collect();
    let diagnostics: Vec<Value> = (out.diagnostics.iter())
        .map(|d| json!({"target": d.target, "message": d.message}))
        .collect();
    let record = json!({"ready": ready, "findings": findings, "diagnostics": diagnostics});
    (ready, record, report["permits"]["needed"].clone())
}

/// The expansion checked as a whole and receipted.
fn checked(candidate: String, instance: &Instance, bound: &Value) -> Expansion {
    let (ready, check, needed) = check(&candidate);
    let document = literal_projection(&candidate).unwrap_or(Value::Null);
    let component = &instance.component;
    let receipt = json!({
        "law": EXPANDED,
        "component": component.record(),
        "bindings": instance.bindings_record(),
        "open": instance.open,
        "not_inherited": {
            "nika": bound.get("nika"),
            "model": bound.get("model"),
            "permits": bound.get("permits"),
        },
        "authority": {
            "inherited": false,
            "component_declares": {
                "effects": component.effects,
                "authority": component.authority,
                "callables": component.callables,
            },
            "document_needs": needed,
        },
        "nodes": nodes(&document, bound),
        "candidate_sha256": sha256(&candidate),
        "check": check,
    });
    Expansion {
        candidate,
        ready,
        receipt,
    }
}

/// The digest of every node the expansion produced, as the expanded document holds it: by
/// section, the sha256 of the node's compact JSON.
#[must_use]
pub fn nodes(document: &Value, bound: &Value) -> Value {
    let mut nodes = Map::new();
    for section in MERGED {
        let Some(Value::Object(entries)) = bound.get(section) else {
            continue;
        };
        let digests: Map<String, Value> = (entries.keys())
            .map(|name| {
                let node = document.get(section).and_then(|s| s.get(name));
                let digest = node.map_or(Value::Null, |n| json!(sha256(&n.to_string())));
                (name.clone(), digest)
            })
            .collect();
        nodes.insert(section.to_owned(), Value::Object(digests));
    }
    Value::Object(nodes)
}

/// A top-level key line: a key at the line's start, then `:` and a blank or the line's end.
fn top_level_key(line: &str) -> Option<&str> {
    let (key, rest) = line.trim_end_matches(['\n', '\r']).split_once(':')?;
    let token = !key.is_empty()
        && key
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    (token && (rest.is_empty() || rest.starts_with([' ', '\t']))).then_some(key)
}

/// The lines of `source`, each with its byte offset.
fn lines(source: &str) -> Vec<(usize, &str)> {
    let mut at = 0;
    source
        .split_inclusive('\n')
        .map(|line| {
            let start = at;
            at += line.len();
            (start, line)
        })
        .collect()
}

/// One top-level section as written: where its key line starts and ends, what follows its
/// colon on that line, and where its block body ends (before the next top-level line).
pub(super) struct Section<'a> {
    pub(super) start: usize,
    pub(super) header_end: usize,
    pub(super) inline: &'a str,
    pub(super) body_end: usize,
}

/// The top-level section `key` of `source`, when it holds one.
pub(super) fn section<'a>(source: &'a str, key: &str) -> Option<Section<'a>> {
    let all = lines(source);
    let at = all
        .iter()
        .position(|(_, line)| top_level_key(line) == Some(key))?;
    let (start, line) = all[at];
    let header_end = start + line.len();
    let inline = line
        .trim_end()
        .split_once(':')
        .map_or("", |(_, rest)| rest.trim());
    // The body: the indented, blank and indented-comment lines that follow; a line at column
    // zero (a key, a comment, a document marker) ends it, and so does the end.
    let body_end = all[at + 1..]
        .iter()
        .find(|(_, line)| {
            let text = line.trim_end_matches(['\n', '\r']);
            !text.is_empty() && !text.starts_with([' ', '\t'])
        })
        .map_or(source.len(), |(offset, _)| *offset);
    Some(Section {
        start,
        header_end,
        inline,
        body_end,
    })
}

/// The entries of a block-style section, each line without the section's indentation.
fn section_body(source: &str, key: &str) -> Result<Vec<String>, ExpandError> {
    let unproven = |why: &str| ExpandError::Unproven(format!("`{key}` {why}"));
    let found = section(source, key).ok_or_else(|| unproven("is not a top-level section"))?;
    let comment = found.inline.starts_with('#');
    if !found.inline.is_empty() && !comment {
        return Err(unproven("is written in flow form"));
    }
    let body = &source[found.header_end..found.body_end];
    let indent = body.lines().find(|line| !blank(line)).map_or(0, spaces);
    let mut kept: Vec<String> = Vec::new();
    for line in body.lines() {
        if blank(line) {
            // A space past the indentation may be a block scalar's own: kept.
            kept.push(line.get(indent..).unwrap_or_default().to_owned());
        } else if spaces(line) < indent {
            return Err(unproven("is less indented than its first entry"));
        } else {
            kept.push(line[indent..].to_owned());
        }
    }
    // The blank lines that end the section are layout, unless a value keeps them (a block
    // scalar's keep indicator): the parser decides, from the values both read.
    let mut trimmed = kept.clone();
    while trimmed.last().is_some_and(|line| blank(line)) {
        trimmed.pop();
    }
    let read = |lines: &[String]| {
        let indented: Vec<String> = lines.iter().map(|line| format!("  {line}")).collect();
        literal_projection(&format!("{key}:\n{}\n", indented.join("\n")))
    };
    if trimmed.len() < kept.len() && read(&trimmed) != read(&kept) {
        return Ok(kept);
    }
    Ok(trimmed)
}

/// The indentation of a line: YAML indents with ASCII spaces only, never another white space
/// (a no-break space is content).
fn spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// A line of ASCII spaces only, or none.
fn blank(line: &str) -> bool {
    line.bytes().all(|byte| byte == b' ')
}

/// `source` with the whole section `text` (its key line included) placed where the envelope's
/// order puts `key`: before the first later key (and the comment lines right above it, which stay
/// with their key), or at the end.
pub(super) fn place_section(source: &str, key: &str, text: &str) -> String {
    let later = &ENVELOPE[ENVELOPE
        .iter()
        .position(|k| *k == key)
        .map_or(0, |at| at + 1)..];
    let all = lines(source);
    let at = all
        .iter()
        .position(|(_, line)| top_level_key(line).is_some_and(|k| later.contains(&k)))
        .map_or(source.len(), |mut at| {
            while at > 0 && all[at - 1].1.starts_with('#') {
                at -= 1;
            }
            all[at].0
        });
    let head = &source[..at];
    let separator = if head.is_empty() || head.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    format!("{head}{separator}{text}{}", &source[at..])
}

/// `source` with `body` (entries without indentation) added to its top-level section `key`: after
/// its last entry, into its `{}`, or as a new section placed in the envelope's order.
pub(super) fn merge_section(
    source: &str,
    key: &str,
    body: &[String],
) -> Result<String, ExpandError> {
    let indented = |indent: usize| -> String {
        (body.iter())
            .map(|line| {
                if line.is_empty() {
                    "\n".to_owned()
                } else {
                    format!("{}{line}\n", " ".repeat(indent))
                }
            })
            .collect()
    };
    let Some(found) = section(source, key) else {
        return Ok(place_section(
            source,
            key,
            &format!("{key}:\n{}", indented(2)),
        ));
    };
    if found.inline == "{}" {
        let header = format!("{key}:\n{}", indented(2));
        return Ok(format!(
            "{}{header}{}",
            &source[..found.start],
            &source[found.header_end..]
        ));
    }
    if !found.inline.is_empty() && !found.inline.starts_with('#') {
        return Err(ExpandError::Unproven(format!(
            "the document's `{key}` is written in flow form"
        )));
    }
    let body_text = &source[found.header_end..found.body_end];
    let indent = body_text
        .lines()
        .find(|line| !blank(line) && !line.trim_start_matches(' ').starts_with('#'))
        .map_or(2, spaces);
    // After the last line of the body that is not blank: trailing blank lines stay after.
    let last = lines(body_text)
        .into_iter()
        .filter(|(_, line)| !blank(line))
        .map(|(offset, line)| offset + line.len())
        .next_back()
        .map_or(found.header_end, |end| found.header_end + end);
    let head = &source[..last];
    let separator = if head.ends_with('\n') { "" } else { "\n" };
    Ok(format!(
        "{head}{separator}{}{}",
        indented(indent),
        &source[last..]
    ))
}
