// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Template-island scanning rules — `NIKA-DAG-003` · `NIKA-VAR-001` ·
//! `NIKA-PARSE-WHEN-001` · output-binding rules.
//!
//! Spec `03-dag.md` §referencing · « If a task references `tasks.<id>`
//! **anywhere** — in `when:` · `with:` · any verb field (`prompt:` ·
//! `command:` · `args:` · …) — that task **MUST** declare `<id>` in its
//! `depends_on:` » · the engine does NOT infer the edge.
//!
//! Exemptions (per the spec's own examples) · `on_finally:` bodies
//! reference the OWNING task's result (example 16 · `tasks.test.status`
//! inside `test`'s own cleanup) and `on_error.recover:` references a
//! fallback task (example 22 · `tasks.cached.output` with no edge) —
//! both are scanned for RESOLUTION (`NIKA-VAR`) but not for the edge.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::SchemaError;
use crate::expression::{ExprError, NamespaceRef, expr_refs, is_boolean_shaped, scan_templates};
use crate::raw::{ForEachValue, RawAction, RawTask, RawWorkflow};
use crate::source::{Span, Spanned};

/// The reserved result-record fields (spec `04-variables.md` §result
/// record) — valid `tasks.<id>.<field>` accessors + forbidden
/// `output:` binding names.
pub(super) const RESERVED_RECORD_FIELDS: &[&str] = &[
    "output",
    "status",
    "error",
    "started_at",
    "ended_at",
    "duration_ms",
];

/// Name-resolution index over the whole workflow.
pub(super) struct WorkflowIndex<'a> {
    vars: BTreeSet<&'a str>,
    env: BTreeSet<&'a str>,
    secrets: BTreeSet<&'a str>,
    task_ids: BTreeSet<&'a str>,
    /// task id → declared `output:` binding names.
    bindings: BTreeMap<&'a str, BTreeSet<&'a str>>,
    /// task id → declared structured-output `schema:` (infer/agent ·
    /// spec 04 §Static binding validation).
    schemas: BTreeMap<&'a str, &'a serde_json::Value>,
}

impl<'a> WorkflowIndex<'a> {
    /// Build the index from the parsed workflow.
    pub(super) fn new(wf: &'a RawWorkflow) -> Self {
        let mut bindings: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for task in &wf.tasks {
            bindings.insert(
                task.value.id.value.as_str(),
                task.value
                    .output
                    .iter()
                    .map(|(name, _)| name.value.as_str())
                    .collect(),
            );
        }
        let mut schemas: BTreeMap<&str, &serde_json::Value> = BTreeMap::new();
        for task in &wf.tasks {
            let declared = match &task.value.action {
                RawAction::Infer(f) => f.schema.as_ref().map(|sp| &sp.value),
                RawAction::Agent(g) => g.schema.as_ref().map(|sp| &sp.value),
                RawAction::Exec(_) | RawAction::Invoke(_) => None,
            };
            if let Some(schema) = declared {
                schemas.insert(task.value.id.value.as_str(), schema);
            }
        }
        Self {
            vars: wf.vars.iter().map(|(k, _)| k.value.as_str()).collect(),
            env: wf.env.iter().map(|(k, _)| k.value.as_str()).collect(),
            secrets: wf.secrets.iter().map(|(k, _)| k.value.as_str()).collect(),
            task_ids: wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect(),
            bindings,
            schemas,
        }
    }

    /// The declared structured-output schema of a task · if any.
    pub(super) fn schema_of(&self, task_id: &str) -> Option<&serde_json::Value> {
        self.schemas.get(task_id).copied()
    }
}

/// Where a scanned string lives — drives the edge / loop-local /
/// `with.` resolution rules.
struct ScanCtx<'a> {
    /// Human location for error messages (a task label · `outputs`).
    location: String,
    /// `depends_on` of the owning task — `Some` ⟺ NIKA-DAG-003 applies.
    edge_set: Option<&'a BTreeSet<&'a str>>,
    /// The owning task's `with:` names (None at workflow level).
    with_names: Option<&'a BTreeSet<&'a str>>,
    /// `item` / `index` in scope (the owning task has `for_each:`).
    allow_loop_locals: bool,
}

/// Scan every template surface of the workflow · collect all errors.
pub(super) fn scan_workflow(wf: &RawWorkflow, errors: &mut Vec<SchemaError>) {
    let index = WorkflowIndex::new(wf);

    for task in &wf.tasks {
        scan_task(&task.value, &index, errors);
    }

    // Envelope `outputs:` — workflow level · no with/item/index · no
    // edge rule (NIKA-VAR existence only · fixture variables/001).
    let outputs_ctx = ScanCtx {
        location: "outputs".to_owned(),
        edge_set: None,
        with_names: None,
        allow_loop_locals: false,
    };
    for (_, decl) in &wf.outputs {
        let value = decl.value();
        scan_string(value, &outputs_ctx, &index, errors);
    }
}

/// Scan one task's surfaces.
fn scan_task(task: &RawTask, index: &WorkflowIndex<'_>, errors: &mut Vec<SchemaError>) {
    let id = task.id.value.as_str();
    let edge_set: BTreeSet<&str> = task.depends_on.iter().map(|d| d.value.as_str()).collect();
    let with_names: BTreeSet<&str> = task.with.iter().map(|(k, _)| k.value.as_str()).collect();
    let has_for_each = task.for_each.is_some();

    let body_ctx = ScanCtx {
        location: format!("task `{id}`"),
        edge_set: Some(&edge_set),
        with_names: Some(&with_names),
        allow_loop_locals: has_for_each,
    };

    // `when:` — single boolean-shaped island (NIKA-PARSE-WHEN-001).
    if let Some(when) = &task.when {
        check_single_island(when, "when", id, true, errors);
        scan_string(when, &body_ctx, index, errors);
    }
    // `for_each:` — expression form is a single island (no boolean
    // requirement) · literal-list form scans element strings.
    if let Some(for_each) = &task.for_each {
        match &for_each.value {
            ForEachValue::Expression(expr) => {
                let spanned = Spanned::new(expr.clone(), for_each.span);
                check_single_island(&spanned, "for_each", id, false, errors);
                scan_string(&spanned, &body_ctx, index, errors);
            }
            ForEachValue::List(list) => {
                let spanned = Spanned::new(list.clone(), for_each.span);
                scan_json(&spanned, &body_ctx, index, errors);
            }
        }
    }
    // `with:` values.
    for (_, value) in &task.with {
        scan_json(value, &body_ctx, index, errors);
    }
    // Verb fields.
    scan_action(&task.action, &body_ctx, index, errors);

    // `output:` bindings — reserved names + pure-jq (no `${{`).
    check_output_bindings(task, errors);

    // `on_error.recover:` — resolution only · NO edge rule (spec
    // example 22 · fallback ref without depends_on).
    let no_edge_ctx = ScanCtx {
        location: format!("task `{id}` on_error"),
        edge_set: None,
        with_names: Some(&with_names),
        allow_loop_locals: has_for_each,
    };
    if let Some(on_error) = &task.on_error
        && let crate::types::OnError::Recover(value) = &on_error.value
    {
        scan_json(value, &no_edge_ctx, index, errors);
    }

    // `on_finally:` — references the owning task's result (example 16)
    // · resolution only · NO edge rule.
    let finally_ctx = ScanCtx {
        location: format!("task `{id}` on_finally"),
        edge_set: None,
        with_names: Some(&with_names),
        allow_loop_locals: has_for_each,
    };
    for cleanup in &task.on_finally {
        if let Some(when) = &cleanup.value.when {
            check_single_island(when, "when", id, true, errors);
            scan_string(when, &finally_ctx, index, errors);
        }
        scan_action(&cleanup.value.action, &finally_ctx, index, errors);
    }
}

/// Scan the string + JSON surfaces of a verb body.
fn scan_action(
    action: &RawAction,
    ctx: &ScanCtx<'_>,
    index: &WorkflowIndex<'_>,
    errors: &mut Vec<SchemaError>,
) {
    let mut strings: Vec<&Spanned<String>> = Vec::new();
    let mut jsons: Vec<&Spanned<serde_json::Value>> = Vec::new();
    match action {
        RawAction::Infer(infer) => {
            strings.push(&infer.prompt);
            strings.extend(infer.system.as_ref());
            strings.extend(infer.model.as_ref());
            for vision in &infer.vision {
                match &vision.value {
                    crate::raw::VisionInput::File { path } => strings.push(path),
                    crate::raw::VisionInput::Url { url } => strings.push(url),
                }
            }
        }
        RawAction::Exec(exec) => {
            strings.push(&exec.command);
            strings.extend(exec.cwd.as_ref());
            strings.extend(exec.stdin.as_ref());
            for (_, value) in &exec.env {
                strings.push(value);
            }
        }
        RawAction::Invoke(invoke) => {
            strings.push(&invoke.tool);
            jsons.extend(invoke.args.as_ref());
        }
        RawAction::Agent(agent) => {
            strings.push(&agent.prompt);
            strings.extend(agent.system.as_ref());
            strings.extend(agent.model.as_ref());
        }
    }
    for s in strings {
        scan_string(s, ctx, index, errors);
    }
    for j in jsons {
        scan_json(j, ctx, index, errors);
    }
}

/// `when:` / `for_each:` must be EXACTLY one `${{ … }}` island spanning
/// the whole (trimmed) value (spec `03-dag.md` §when · « `when:
/// "literal string"` ❌ not a `${{ }}` expression ») · `when:` roots
/// must additionally be boolean-shaped (`NIKA-PARSE-WHEN-001`).
fn check_single_island(
    value: &Spanned<String>,
    field: &str,
    task: &str,
    require_boolean: bool,
    errors: &mut Vec<SchemaError>,
) {
    let trimmed = value.value.trim();
    // On a template scan error · return (scan_string reports it).
    let Ok(islands) = scan_templates(trimmed) else {
        return;
    };
    let whole = islands.len() == 1 && islands[0].start == 0 && islands[0].end == trimmed.len();
    if !whole {
        errors.push(SchemaError::WhenNotBoolean {
            field: field.to_owned(),
            task: task.to_owned(),
            reason: "must be a single `${{ … }}` expression spanning the whole value".to_owned(),
            span: Some(value.span),
        });
        return;
    }
    if require_boolean && !is_boolean_shaped(&islands[0].expr) {
        errors.push(SchemaError::WhenNotBoolean {
            field: field.to_owned(),
            task: task.to_owned(),
            reason: format!(
                "`{}` is not boolean-shaped — use an explicit comparison (e.g. `… > 0` · `… != \"\"`)",
                islands[0].src
            ),
            span: Some(value.span),
        });
    }
}

/// `output:` bindings · names ∉ reserved record fields (spec 04 §rules)
/// · values are PURE jq — `${{` inside is an error (04 §binding rules ·
/// « the two expression layers never nest »).
fn check_output_bindings(task: &RawTask, errors: &mut Vec<SchemaError>) {
    let id = task.id.value.as_str();
    for (name, value) in &task.output {
        if RESERVED_RECORD_FIELDS.contains(&name.value.as_str()) {
            errors.push(SchemaError::ReservedBindingName {
                name: name.value.clone(),
                task: id.to_owned(),
                span: Some(name.span),
            });
        }
        if value.value.contains("${{") {
            errors.push(SchemaError::JqBindingContainsTemplate {
                name: name.value.clone(),
                task: id.to_owned(),
                span: Some(value.span),
            });
        }
    }
}

/// Scan one string for islands · validate each ref.
fn scan_string(
    value: &Spanned<String>,
    ctx: &ScanCtx<'_>,
    index: &WorkflowIndex<'_>,
    errors: &mut Vec<SchemaError>,
) {
    match scan_templates(&value.value) {
        Ok(islands) => {
            for island in islands {
                for r in expr_refs(&island.expr) {
                    check_ref(&r, value.span, ctx, index, errors);
                }
                // Static binding validation vs declared schema:
                // (spec 04 §Static binding validation · NIKA-VAR-003).
                super::schema_paths::check_expr(&island.expr, value.span, index, errors);
            }
        }
        Err(e) => errors.push(template_error(&e, value.span)),
    }
}

/// Recursively scan every string inside a JSON value (the value's
/// outer span labels all findings).
fn scan_json(
    value: &Spanned<serde_json::Value>,
    ctx: &ScanCtx<'_>,
    index: &WorkflowIndex<'_>,
    errors: &mut Vec<SchemaError>,
) {
    fn walk(
        v: &serde_json::Value,
        span: Span,
        ctx: &ScanCtx<'_>,
        index: &WorkflowIndex<'_>,
        errors: &mut Vec<SchemaError>,
    ) {
        match v {
            serde_json::Value::String(s) => {
                let spanned = Spanned::new(s.clone(), span);
                scan_string(&spanned, ctx, index, errors);
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, span, ctx, index, errors);
                }
            }
            serde_json::Value::Object(map) => {
                for item in map.values() {
                    walk(item, span, ctx, index, errors);
                }
            }
            _ => {}
        }
    }
    walk(&value.value, value.span, ctx, index, errors);
}

/// Map an [`ExprError`] into the schema error surface.
fn template_error(e: &ExprError, span: Span) -> SchemaError {
    SchemaError::TemplateSyntax {
        reason: e.to_string(),
        span: Some(span),
    }
}

/// Validate one classified root reference (spec `04-variables.md`
/// §Resolution order · `NIKA-VAR-001` class) + the `NIKA-DAG-003` edge
/// rule for `tasks.<id>` refs.
fn check_ref(
    r: &NamespaceRef,
    span: Span,
    ctx: &ScanCtx<'_>,
    index: &WorkflowIndex<'_>,
    errors: &mut Vec<SchemaError>,
) {
    match r {
        NamespaceRef::Vars(name) => {
            if !index.vars.contains(name.as_str()) {
                errors.push(unresolved(&format!("vars.{name}"), ctx, span));
            }
        }
        NamespaceRef::Env(name) => {
            if !index.env.contains(name.as_str()) {
                errors.push(unresolved(&format!("env.{name}"), ctx, span));
            }
        }
        NamespaceRef::Secrets(name) => {
            if !index.secrets.contains(name.as_str()) {
                errors.push(unresolved(&format!("secrets.{name}"), ctx, span));
            }
        }
        NamespaceRef::With(name) => {
            let declared = ctx
                .with_names
                .is_some_and(|names| names.contains(name.as_str()));
            if !declared {
                errors.push(unresolved(&format!("with.{name}"), ctx, span));
            }
        }
        NamespaceRef::Tasks { id, field } => {
            check_task_ref(id, field.as_deref(), span, ctx, index, errors);
        }
        NamespaceRef::Item | NamespaceRef::Index => {
            if !ctx.allow_loop_locals {
                let local = if matches!(r, NamespaceRef::Item) {
                    "item"
                } else {
                    "index"
                };
                errors.push(SchemaError::LoopLocalOutsideForEach {
                    local: local.to_owned(),
                    task: ctx.location.clone(),
                    span: Some(span),
                });
            }
        }
        NamespaceRef::Unknown(root) => {
            errors.push(unresolved(root, ctx, span));
        }
    }
}

/// `tasks.<id>[.<field>]` — existence (`NIKA-VAR`) · result-record
/// field validity (04 §result record) · the `depends_on` edge
/// (`NIKA-DAG-003`) when the context requires it.
fn check_task_ref(
    id: &str,
    field: Option<&str>,
    span: Span,
    ctx: &ScanCtx<'_>,
    index: &WorkflowIndex<'_>,
    errors: &mut Vec<SchemaError>,
) {
    if !index.task_ids.contains(id) {
        errors.push(unresolved(&format!("tasks.{id}"), ctx, span));
        return;
    }
    if let Some(field) = field {
        let declared = index
            .bindings
            .get(id)
            .is_some_and(|names| names.contains(field));
        if !RESERVED_RECORD_FIELDS.contains(&field) && !declared {
            errors.push(SchemaError::UnknownTaskField {
                task: id.to_owned(),
                field: field.to_owned(),
                span: Some(span),
            });
        }
    }
    if let Some(edges) = ctx.edge_set
        && !edges.contains(id)
    {
        errors.push(SchemaError::MissingDependsOnEdge {
            task: ctx.location.clone(),
            referenced: id.to_owned(),
            span: Some(span),
        });
    }
}

/// Build a `NIKA-VAR-001`-class unresolved-reference error.
fn unresolved(reference: &str, ctx: &ScanCtx<'_>, span: Span) -> SchemaError {
    SchemaError::UnresolvedNamespaceRef {
        reference: reference.to_owned(),
        location: ctx.location.clone(),
        span: Some(span),
    }
}
