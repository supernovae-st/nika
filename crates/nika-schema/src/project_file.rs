// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Project files (spec `02-verbs.md` §Project files): `prompt:`, `system:` and `schema:` of
//! `infer:` and `agent:` may name a file beside the workflow as the closed mapping
//! `{ file: <path> }`.
//!
//! The parser keeps a [`ProjectFileRef`] and judges the context-independent lexical law
//! ([`path_law`]). [`compose_project_files`] judges the rest in the spec's order:
//! - containment by the project root, from the naming file's folder ([`contained`]);
//! - the read boundary BEFORE any read: an ungranted path is never opened;
//! - the read, through an injected reader (this crate stays zero-I/O; the caller holds the
//!   project root and refuses a symbolic link at every component);
//! - the cap and UTF-8, then the schema content law.
//!
//! Each admitted file replaces its field: a text by its text, a schema by the JSON Schema it
//! holds. Every law the same field written inline meets then judges the composed workflow.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::raw::{ProjectFileRef, RawAction, RawWorkflow};
use crate::source::Spanned;
use crate::types::Permits;

/// The most bytes a project file may hold (the read law).
pub const PROJECT_FILE_CAP: usize = 65_536;

/// Why the injected reader handed over no bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProjectFileRead {
    /// A component between the project root and the file is a symbolic link (`NIKA-FILE-001`).
    Linked,
    /// The file is missing or unreadable (`NIKA-FILE-002`).
    Missing,
    /// The path names something other than a regular file (`NIKA-FILE-002`).
    NotRegular,
}

/// One project file that could not compose: `NIKA-FILE-001..003` (path · read · bytes),
/// `NIKA-AUTH-006` or `NIKA-SEC-004` (the read boundary), `NIKA-TYPE-007` (no JSON Schema).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProjectFileFinding {
    /// The task that names the file.
    pub task: String,
    /// `prompt` · `system` · `schema`.
    pub field: &'static str,
    /// The path as written.
    pub path: String,
    /// The spec code.
    pub code: &'static str,
    /// What is wrong, and the repair.
    pub detail: String,
}

/// One project file a compose step read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProjectFile {
    /// The task that names the file.
    pub task: String,
    /// `prompt` · `system` · `schema`.
    pub field: &'static str,
    /// The path as written.
    pub path: String,
    /// The path the reader was given: project-relative, normalized.
    pub resolved: String,
    /// How many bytes it holds.
    pub bytes: usize,
}

/// What a compose step learned. `findings.is_empty()` ⇔ every project file the workflow names
/// composed.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ComposedProjectFiles {
    /// The workflow, each composed field holding its file's content.
    pub workflow: RawWorkflow,
    /// The files read, in reference order.
    pub files: Vec<ProjectFile>,
    /// The files that could not compose, in reference order.
    pub findings: Vec<ProjectFileFinding>,
}

/// Every project file a workflow names: `(task id, field, reference)` in declaration order.
#[must_use]
pub fn project_file_refs(wf: &RawWorkflow) -> Vec<(&str, &'static str, &ProjectFileRef)> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        let (prompt, system, schema) = match &task.value.action {
            RawAction::Infer(a) => (&a.prompt_file, &a.system_file, &a.schema_file),
            RawAction::Agent(a) => (&a.prompt_file, &a.system_file, &a.schema_file),
            _ => continue,
        };
        for (field, reference) in [("prompt", prompt), ("system", system), ("schema", schema)] {
            if let Some(reference) = reference {
                out.push((task.value.id.value.as_str(), field, reference));
            }
        }
    }
    out
}

/// The context-independent lexical law of a project file's path: `None` when it holds, else
/// what breaks it. Containment depends on the naming file's folder: see [`contained`].
#[must_use]
pub fn path_law(raw: &str) -> Option<&'static str> {
    if raw.is_empty() {
        return Some("is empty");
    }
    if raw.contains("${{") {
        return Some("carries a `${{ }}` template");
    }
    if raw.contains(['*', '?', '[']) {
        return Some("is a glob");
    }
    if raw.starts_with('/') {
        return Some("is absolute");
    }
    if raw.contains('\\') {
        return Some("holds a backslash");
    }
    if names_a_scheme(raw) {
        return Some("names a drive or a URI scheme");
    }
    for segment in raw.strip_prefix("./").unwrap_or(raw).split('/') {
        if segment.is_empty() {
            return Some("holds an empty segment");
        }
        if segment != ".." && segment.starts_with('.') {
            return Some("holds a hidden or `.` segment");
        }
    }
    None
}

/// `^[A-Za-z][A-Za-z0-9+.-]*:` — a drive (`c:`) or a URI scheme (`https:`).
fn names_a_scheme(raw: &str) -> bool {
    let mut chars = raw.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    for c in chars {
        if c == ':' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')) {
            return false;
        }
    }
    false
}

/// `raw` joined to the naming file's folder (`naming`: project-relative, `.` at the root) and
/// normalized lexically: the project-relative path, or `None` when a `..` climbs past the root.
#[must_use]
pub fn contained(naming: &str, raw: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for segment in naming.split('/').chain(raw.split('/')) {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// Compose every project file `wf` names, in the spec's order of judgment.
///
/// `naming` is the workflow's folder relative to the project root (`.` when the workflow sits
/// at it). `read` hands over the bytes of a project-relative path (at most
/// [`PROJECT_FILE_CAP`] + 1 of them is enough) and refuses a symbolic link at every component.
/// Each path is read at most once, and a reference an earlier compose step read is not read
/// again: its field keeps the content that step put there.
#[must_use]
pub fn compose_project_files(
    wf: &RawWorkflow,
    naming: &str,
    read: &mut (dyn FnMut(&str) -> Result<Vec<u8>, ProjectFileRead> + '_),
) -> ComposedProjectFiles {
    let mut workflow = wf.clone();
    let mut composer = Composer {
        naming,
        permits: wf.permits.as_ref().map(|p| &p.value),
        read,
        reads: BTreeMap::new(),
        files: Vec::new(),
        findings: Vec::new(),
    };
    for task in &mut workflow.tasks {
        let id = task.value.id.value.clone();
        let mut fields = match &mut task.value.action {
            RawAction::Infer(a) => {
                let a = &mut **a;
                Fields {
                    prompt: &mut a.prompt,
                    prompt_file: &mut a.prompt_file,
                    system: &mut a.system,
                    system_file: &mut a.system_file,
                    schema: &mut a.schema,
                    schema_file: &mut a.schema_file,
                }
            }
            RawAction::Agent(a) => {
                let a = &mut **a;
                Fields {
                    prompt: &mut a.prompt,
                    prompt_file: &mut a.prompt_file,
                    system: &mut a.system,
                    system_file: &mut a.system_file,
                    schema: &mut a.schema,
                    schema_file: &mut a.schema_file,
                }
            }
            _ => continue,
        };
        composer.compose(&id, &mut fields);
    }
    ComposedProjectFiles {
        workflow,
        files: composer.files,
        findings: composer.findings,
    }
}

/// The three fields a project file can hold, borrowed from one `infer:` or `agent:`.
struct Fields<'a> {
    prompt: &'a mut Spanned<String>,
    prompt_file: &'a mut Option<ProjectFileRef>,
    system: &'a mut Option<Spanned<String>>,
    system_file: &'a mut Option<ProjectFileRef>,
    schema: &'a mut Option<Spanned<Value>>,
    schema_file: &'a mut Option<ProjectFileRef>,
}

/// One compose step's state: the naming folder, the boundary, the reader and what it learned.
struct Composer<'r, 'f> {
    naming: &'r str,
    permits: Option<&'r Permits>,
    read: &'r mut (dyn FnMut(&str) -> Result<Vec<u8>, ProjectFileRead> + 'f),
    reads: BTreeMap<String, Result<Vec<u8>, ProjectFileRead>>,
    files: Vec<ProjectFile>,
    findings: Vec<ProjectFileFinding>,
}

impl Composer<'_, '_> {
    /// Compose the fields of `task` no earlier step read.
    fn compose(&mut self, task: &str, fields: &mut Fields<'_>) {
        if let Some(reference) = fields.prompt_file
            && !reference.composed
            && let Some(text) = self.text(task, "prompt", reference)
        {
            fields.prompt.value = text;
            reference.composed = true;
        }
        if let Some(reference) = fields.system_file
            && !reference.composed
            && let Some(text) = self.text(task, "system", reference)
        {
            *fields.system = Some(Spanned::new(text, reference.path.span));
            reference.composed = true;
        }
        if let Some(reference) = fields.schema_file
            && !reference.composed
            && let Some(text) = self.text(task, "schema", reference)
        {
            match schema_content(&text) {
                Ok(schema) => {
                    *fields.schema = Some(Spanned::new(schema, reference.path.span));
                    reference.composed = true;
                }
                Err(why) => self.refuse(
                    task,
                    "schema",
                    &reference.path.value,
                    "NIKA-TYPE-007",
                    format!("holds no JSON Schema: {why}"),
                ),
            }
        }
    }

    /// The file's text, once the path, the boundary, the read and the bytes laws hold; `None`
    /// with its finding recorded otherwise.
    fn text(
        &mut self,
        task: &str,
        field: &'static str,
        reference: &ProjectFileRef,
    ) -> Option<String> {
        let raw = reference.path.value.as_str();
        if let Some(why) = path_law(raw) {
            let detail = format!("the path {why}: a project file's path is static and relative");
            self.refuse(task, field, raw, "NIKA-FILE-001", detail);
            return None;
        }
        let Some(resolved) = contained(self.naming, raw) else {
            let detail = "a `..` climbs past the project root: keep the file inside the project";
            self.refuse(task, field, raw, "NIKA-FILE-001", detail.to_owned());
            return None;
        };
        match self.permits {
            None => {
                let detail = format!(
                    "the file is read under an absent `permits:` block, which is zero authority: declare `permits: {{ fs: {{ read: [\"{raw}\"] }} }}` (its content reaches the model)"
                );
                self.refuse(task, field, raw, "NIKA-AUTH-006", detail);
                return None;
            }
            Some(permits) if !permits.allows_path(raw, false) => {
                let detail = "the path is outside the `permits.fs.read` boundary: grant it, or move the file inside a granted path";
                self.refuse(task, field, raw, "NIKA-SEC-004", detail.to_owned());
                return None;
            }
            Some(_) => {}
        }
        let read = &mut *self.read;
        let bytes = self
            .reads
            .entry(resolved.clone())
            .or_insert_with(|| read(&resolved))
            .clone();
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(ProjectFileRead::Linked) => {
                let detail = "a symbolic link sits between the project root and the file";
                self.refuse(task, field, raw, "NIKA-FILE-001", detail.to_owned());
                return None;
            }
            Err(_) => {
                let detail = "the file is missing, unreadable or not a regular file";
                self.refuse(task, field, raw, "NIKA-FILE-002", detail.to_owned());
                return None;
            }
        };
        if bytes.len() > PROJECT_FILE_CAP {
            let detail = format!("the file holds more than {PROJECT_FILE_CAP} bytes");
            self.refuse(task, field, raw, "NIKA-FILE-003", detail);
            return None;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            self.refuse(
                task,
                field,
                raw,
                "NIKA-FILE-003",
                "the file is not valid UTF-8".to_owned(),
            );
            return None;
        };
        self.files.push(ProjectFile {
            task: task.to_owned(),
            field,
            path: raw.to_owned(),
            resolved,
            bytes: text.len(),
        });
        Some(text)
    }

    fn refuse(
        &mut self,
        task: &str,
        field: &'static str,
        path: &str,
        code: &'static str,
        detail: String,
    ) {
        self.findings.push(ProjectFileFinding {
            task: task.to_owned(),
            field,
            path: path.to_owned(),
            code,
            detail,
        });
    }
}

/// The schema content law: one JSON Schema, JSON text or YAML under the workflow's own loader
/// profile, whose value is a mapping and whose every `$ref` is a fragment of the file itself.
/// Whether the mapping is a valid JSON Schema is then the inline schema's own law.
fn schema_content(text: &str) -> Result<Value, String> {
    let value = match serde_json::from_str::<Value>(text) {
        Ok(value) => value,
        Err(_) => crate::parser::yaml_document_value(text)
            .map_err(|err| format!("neither JSON nor YAML the workflow profile admits ({err})"))?,
    };
    if !value.is_object() {
        return Err("its value is not a mapping".to_owned());
    }
    if let Some(reference) = leaving_ref(&value) {
        return Err(format!("its `$ref` `{reference}` leaves the file"));
    }
    Ok(value)
}

/// The first `$ref` that is not a fragment of the file itself (`#…`).
fn leaving_ref(value: &Value) -> Option<&str> {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get("$ref")
                && !reference.starts_with('#')
            {
                return Some(reference.as_str());
            }
            map.values().find_map(leaving_ref)
        }
        Value::Array(items) => items.iter().find_map(leaving_ref),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
