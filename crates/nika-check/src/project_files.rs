// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Project files at check time (spec `02-verbs.md` §Project files): the report rows and the
//! reader-backed door.
//!
//! [`check_project`] composes the root workflow's project files through
//! [`nika_schema::compose_project_files`], judges the composed workflow with the lanes of
//! [`crate::check_composed`], and composes every child the composition walk loaded from the
//! child's own folder. The fully composed child meets the ordinary check and analyzer once, as
//! the same content written inline would: every finding it raises that the child without its
//! files does not is reported with the child, task, field and path of the file whose field holds
//! it, else of the files whose absence takes it away. A child's own read boundary stays its own
//! check's: an ungranted child file is never opened here.
//! A plain [`crate::check`] reads no files and judges none of their laws. A run does not carry
//! project files yet: it refuses a workflow that names one, before any task.

use std::collections::BTreeMap;

use nika_schema::error::spec_code::{SpecCategory, SpecCode};
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::source::{Span, Spanned};
use nika_schema::{
    ComposedProjectFiles, ProjectFile, ProjectFileRead, compose_project_files, project_file_refs,
};
use serde_json::Value;

use crate::{ByteSpan, CheckReport, UnifiedFinding, analyzer, check, composition, findings, tools};

/// The ordinary check's findings on one workflow, each beside its conformance spec code.
type Rows = Vec<(UnifiedFinding, Option<SpecCode>)>;

/// One project file that could not compose, or one law a child's composed file breaks, as the
/// report carries it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ProjectFileFinding {
    /// The task that names the file (`<child file> · <task>` for a child's file).
    pub task: String,
    /// `prompt` · `system` · `schema`.
    pub field: &'static str,
    /// The path as written.
    pub path: String,
    /// `NIKA-FILE-001..003` · `NIKA-AUTH-006` · `NIKA-SEC-004` · `NIKA-TYPE-007`, or the code of
    /// the law a child's composed file breaks (none for a class without a spec code).
    pub code: Option<String>,
    /// What is wrong, and the repair.
    pub detail: String,
    /// The code the conformance runner matches on, with its category.
    #[serde(skip)]
    pub(crate) spec: Option<SpecCode>,
}

impl ProjectFileFinding {
    /// The shared human row (one voice on every surface).
    #[must_use]
    pub fn row(&self) -> String {
        let (task, field, path, detail) = (&self.task, self.field, &self.path, &self.detail);
        match &self.code {
            Some(code) => format!(
                "[{code} · project files] task `{task}` {field} `{path}` · {detail} · fix: nika explain {code}"
            ),
            None => format!("[project files] task `{task}` {field} `{path}` · {detail}"),
        }
    }

    /// A compose step's finding about the file `path` that `task` names as `field`.
    fn composing(
        task: String,
        (field, path): (&'static str, String),
        code: &'static str,
        detail: String,
    ) -> Self {
        Self {
            task,
            field,
            path,
            code: Some(code.to_owned()),
            detail,
            spec: Some(compose_spec(code)),
        }
    }
}

impl From<nika_schema::ProjectFileFinding> for ProjectFileFinding {
    fn from(finding: nika_schema::ProjectFileFinding) -> Self {
        let file = (finding.field, finding.path);
        Self::composing(finding.task, file, finding.code, finding.detail)
    }
}

/// The spec code of a compose step's finding: the read boundary is the security class; the
/// path, read, bytes and content laws validation.
fn compose_spec(code: &str) -> SpecCode {
    match code {
        "NIKA-AUTH-006" => SpecCode::new("AUTH", 6, SpecCategory::SecurityError),
        "NIKA-SEC-004" => SpecCode::new("SEC", 4, SpecCategory::SecurityError),
        "NIKA-TYPE-007" => SpecCode::new("TYPE", 7, SpecCategory::ValidationError),
        "NIKA-FILE-002" => SpecCode::new("FILE", 2, SpecCategory::ValidationError),
        "NIKA-FILE-003" => SpecCode::new("FILE", 3, SpecCategory::ValidationError),
        _ => SpecCode::new("FILE", 1, SpecCategory::ValidationError),
    }
}

/// The project files one check reads: the root workflow's folder (as the composition lane names
/// files), the reader of project-relative bytes with each path read once for the whole check,
/// and what the children's files brought.
pub(crate) struct ChildFiles<'r, 'f> {
    folder: String,
    read: &'r mut (dyn FnMut(&str) -> Result<Vec<u8>, ProjectFileRead> + 'f),
    reads: BTreeMap<String, Result<Vec<u8>, ProjectFileRead>>,
    findings: Vec<ProjectFileFinding>,
}

impl ChildFiles<'_, '_> {
    /// Compose `wf`'s project files from its project-relative folder `naming`.
    fn compose(&mut self, wf: &RawWorkflow, naming: &str) -> ComposedProjectFiles {
        let (reader, cache) = (&mut *self.read, &mut self.reads);
        let mut cached = |path: &str| {
            (cache.entry(path.to_owned()))
                .or_insert_with(|| reader(path))
                .clone()
        };
        compose_project_files(wf, naming, &mut cached)
    }

    /// Compose and judge the project files of the child the composition walk loaded as `id`,
    /// from the child's own folder. A child outside the project root keeps none of its files
    /// inside it; a file the child's own boundary does not grant is its own check's to refuse.
    pub(crate) fn compose_child(&mut self, id: &str, child: &RawWorkflow) {
        if project_file_refs(child).is_empty() {
            return;
        }
        let Some(naming) = self.naming_of(id) else {
            for (task, field, reference) in project_file_refs(child) {
                self.findings.push(ProjectFileFinding::composing(
                    format!("{id} · {task}"),
                    (field, reference.path.value.clone()),
                    "NIKA-FILE-001",
                    "the child workflow sits outside the project root, so its files cannot stay inside it".to_owned(),
                ));
            }
            return;
        };
        let composed = self.compose(child, &naming);
        let judged = judge_child_files(id, child, &naming, &composed, &self.reads);
        for finding in composed.findings {
            if matches!(finding.code, "NIKA-AUTH-006" | "NIKA-SEC-004") {
                continue; // never opened here: the child's own check refuses it
            }
            let mut finding = ProjectFileFinding::from(finding);
            finding.task = format!("{id} · {}", finding.task);
            self.findings.push(finding);
        }
        self.findings.extend(judged);
    }

    /// The child's folder relative to the project root, or `None` outside it.
    fn naming_of(&self, id: &str) -> Option<String> {
        let dir = id.rsplit_once('/').map_or("", |(dir, _)| dir);
        if dir == self.folder {
            Some(".".to_owned())
        } else if self.folder.is_empty() {
            Some(dir.to_owned())
        } else {
            dir.strip_prefix(self.folder.as_str())
                .and_then(|rest| rest.strip_prefix('/'))
                .map(str::to_owned)
        }
    }
}

/// The ordinary check over a composed child (spec 02 §Project files: a file's content meets every
/// law the same content written inline meets), run once on the fully composed child. A finding
/// it raises that the child without its files does not is the files': reported at the file whose
/// field holds it, else at the files whose absence takes it away.
fn judge_child_files(
    id: &str,
    child: &RawWorkflow,
    naming: &str,
    composed: &ComposedProjectFiles,
    cache: &BTreeMap<String, Result<Vec<u8>, ProjectFileRead>>,
) -> Vec<ProjectFileFinding> {
    let files = composed.files.as_slice();
    let mut out = Vec::new();
    let Some(first) = files.first() else {
        return out;
    };
    let fields = field_spans(&composed.workflow, files);
    let mut explained = judged(child);
    let mut unlocated = Vec::new();
    for (row, spec) in judged(&composed.workflow) {
        if let Some(at) = explained.iter().position(|(seen, _)| same_row(seen, &row)) {
            explained.swap_remove(at);
            continue;
        }
        let holder = row.span.and_then(|span| {
            (fields.iter())
                .find(|(field, _)| field.start <= span.start && span.end <= field.end)
                .map(|(_, file)| *file)
        });
        match holder {
            Some(file) => out.push(at_files(id, (file, &[]), row, spec)),
            None => unlocated.push((row, spec)),
        }
    }
    if unlocated.is_empty() {
        return out;
    }
    // The files whose absence takes a finding away brought it: the child composed without each.
    let without_each: Vec<Rows> = (files.iter())
        .map(|file| {
            let mut cached = |path: &str| match cache.get(path) {
                Some(read) if path != file.resolved => read.clone(),
                _ => Err(ProjectFileRead::Missing),
            };
            judged(&compose_project_files(child, naming, &mut cached).workflow)
        })
        .collect();
    for (row, spec) in unlocated {
        let causes: Vec<&ProjectFile> = (files.iter().zip(&without_each))
            .filter(|(_, rows)| !rows.iter().any(|(seen, _)| same_row(seen, &row)))
            .map(|(file, _)| file)
            .collect();
        let at = causes
            .split_first()
            .map_or((first, &[][..]), |(cause, rest)| (*cause, rest));
        out.push(at_files(id, at, row, spec));
    }
    out
}

/// A finding of a composed child at the project file that brought it, naming the other files it
/// needed.
fn at_files(
    id: &str,
    (file, others): (&ProjectFile, &[&ProjectFile]),
    row: UnifiedFinding,
    spec: Option<SpecCode>,
) -> ProjectFileFinding {
    let with: Vec<String> = (others.iter())
        .map(|other| format!("`{}` (task `{}` {})", other.path, other.task, other.field))
        .collect();
    let detail = if with.is_empty() {
        format!("the composed child breaks it: {}", row.message)
    } else {
        format!(
            "the composed child breaks it with {}: {}",
            with.join(", "),
            row.message
        )
    };
    ProjectFileFinding {
        task: format!("{id} · {}", file.task),
        field: file.field,
        path: file.path.clone(),
        code: row.code,
        detail,
        spec,
    }
}

/// The byte span of each composed field, beside the file it holds.
fn field_spans<'f>(wf: &RawWorkflow, files: &'f [ProjectFile]) -> Vec<(ByteSpan, &'f ProjectFile)> {
    let mut out = Vec::new();
    for file in files {
        let action = (wf.tasks.iter())
            .find(|task| task.value.id.value == file.task)
            .map(|task| &task.value.action);
        let span = match action {
            Some(RawAction::Infer(a)) => {
                field_span(file.field, &a.prompt, a.system.as_ref(), a.schema.as_ref())
            }
            Some(RawAction::Agent(a)) => {
                field_span(file.field, &a.prompt, a.system.as_ref(), a.schema.as_ref())
            }
            _ => None,
        };
        if let Some(span) = span {
            out.push((ByteSpan::new(span.start.0, span.end.0), file));
        }
    }
    out
}

/// The span of `field` among a verb's prompt, system and schema.
fn field_span(
    field: &str,
    prompt: &Spanned<String>,
    system: Option<&Spanned<String>>,
    schema: Option<&Spanned<Value>>,
) -> Option<Span> {
    match field {
        "prompt" => Some(prompt.span),
        "system" => system.map(|system| system.span),
        "schema" => schema.map(|schema| schema.span),
        _ => None,
    }
}

/// The ordinary check's findings on one workflow, each beside the spec code the conformance
/// runner matches on when its class has one (the analyzer tier's and the check-only lanes').
fn judged(wf: &RawWorkflow) -> Rows {
    let report = check(wf);
    let analyzed = analyzer::analyze(wf).err().unwrap_or_default();
    let mut codes: Vec<SpecCode> = (analyzed.iter())
        .map(nika_schema::error::SchemaError::spec_code)
        .collect();
    codes.extend(report.extra_conformance_codes());
    (report.findings.into_iter())
        .map(|row| {
            let spec = row.code.as_deref().and_then(|code| {
                let at = codes.iter().position(|spec| spec.to_string() == code)?;
                Some(codes.swap_remove(at))
            });
            (row, spec)
        })
        .collect()
}

/// The same finding row, wherever it sits.
fn same_row(a: &UnifiedFinding, b: &UnifiedFinding) -> bool {
    a.kind == b.kind && a.code == b.code && a.task == b.task && a.message == b.message
}

/// The door every surface that reads files calls: the workflow with its project files composed,
/// and the [`crate::check_composed`] report over it, its children's project files included.
///
/// The project root is the root workflow's folder (`root` names the workflow, as for
/// [`crate::check_composed`]): `read_file` reads a path relative to that folder and refuses a
/// symbolic link at every component, each path once. The composed workflow is the one to
/// stamp, show and judge.
#[must_use]
pub fn check_project(
    wf: &RawWorkflow,
    root: &str,
    read: &mut dyn FnMut(&str) -> Result<String, String>,
    read_file: &mut (dyn FnMut(&str) -> Result<Vec<u8>, ProjectFileRead> + '_),
) -> (RawWorkflow, CheckReport) {
    let mut files = ChildFiles {
        folder: composition::folder_of(root),
        read: read_file,
        reads: BTreeMap::new(),
        findings: Vec::new(),
    };
    let composed = files.compose(wf, ".");
    let mut report = check_with(&composed.workflow, root, read, Some(&mut files));
    report.project_files = (composed.findings.into_iter())
        .map(ProjectFileFinding::from)
        .chain(files.findings)
        .collect();
    report.findings = findings::collect(&report);
    (composed.workflow, report)
}

/// [`crate::check_composed`]'s lanes, composing the children's project files when `files` reads
/// them.
pub(crate) fn check_with(
    wf: &RawWorkflow,
    root: &str,
    read: &mut dyn FnMut(&str) -> Result<String, String>,
    files: Option<&mut ChildFiles<'_, '_>>,
) -> CheckReport {
    // The walk first: every lane of the check then judges the calls it
    // resolved (the trifecta reads a child that brings untrusted content in
    // as the ingress it is · NIK-273).
    let resolved = composition::scan_resolved_with(wf, root, read, files);
    let mut report = crate::check_calls(wf, resolved.calls);
    report.composition = resolved.findings;
    // The composition COST half (spec 14 · the 2026-07-29 finding): every
    // resolvable child's ceiling folds into the parent's envelope — a
    // parent whose child alone explains `≤$X` stops printing `$0 model
    // spend`. The reader-less `check` stays child-blind BY DESIGN (it
    // never loads files; the pure half needs no filesystem).
    for call in composition::price_resolved(wf, root, read) {
        report.cost.fold_composed(
            call.task,
            call.target,
            &call.child,
            call.iterations,
            call.attempts,
            call.gated,
        );
    }
    // C04 — MCP server presence needs the injected reader (L0, zero I/O).
    report
        .unknown_tools
        .extend(tools::scan_mcp_against_registry(wf, read));
    // Beside it, advice that never blocks: a registered server whose pins the run would refuse.
    report.hints.extend(tools::scan_mcp_against_pins(wf, read));
    // Re-fold the class-erased list over the finished lane (one truth).
    report.findings = findings::collect(&report);
    report
}

#[cfg(test)]
mod tests;
