// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Template-island scanning rules — `NIKA-VAR-021` · `NIKA-VAR-001` ·
//! `NIKA-PARSE-WHEN-001` · output-binding rules.
//!
//! Spec `04-variables.md` §the reference boundary (W2 « the flow ») ·
//! `tasks.*` crosses a task boundary through exactly two doors —
//! `with:` (data · the binding IS the edge) and `after:` (control) —
//! and body surfaces (`when:` · `for_each:` · any verb field) read
//! LOCAL names only: a `tasks.*` reference there is `NIKA-VAR-021`
//! with a machine-applicable fix (hoist into `with:`). The pre-W2
//! edge-restatement rule (`NIKA-DAG-003`) is retired — the boundary
//! replaces it in both directions (no invisible edges to infer · no
//! visible edges to restate).
//!
//! Resolution-only surfaces · `on_error.recover:` reads a fallback
//! task's settled record (spec 05 · a recovery edge, non-scheduling)
//! and an `on_finally:` body reads its PARENT task only (a sibling
//! read would race) — both are scanned for RESOLUTION (`NIKA-VAR`),
//! never for edges.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::SchemaError;
use crate::expression::{ExprError, NamespaceRef, expr_refs, is_boolean_shaped, scan_templates};
use crate::raw::{ForEachValue, RawAction, RawTask, RawWorkflow};
use crate::source::{Span, Spanned};
use crate::types::WhenGate;

/// The reserved result-record fields (spec `04-variables.md` §result
/// record + spec 13 `cause`) — valid `tasks.<id>.<field>` accessors +
/// forbidden `output:` binding names.
pub(super) const RESERVED_RECORD_FIELDS: &[&str] = &[
    "output",
    "status",
    "cause",
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
    /// task id → `lower(returns)` — the SAME walkable contract, derived
    /// from the typed door (spec 09 §returns · the walk runs on it with
    /// full precision · owned: the lowering is a projection, not a
    /// borrow of the AST).
    lowered: BTreeMap<String, serde_json::Value>,
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
            lowered: super::types_contract::lowered_returns(wf),
        }
    }

    /// The declared output contract of a task, as a walkable JSON
    /// Schema · a verb-level `schema:` OR the `lower(returns)`
    /// projection (spec 09 — `NIKA-TYPE-003` forbids both, so at most
    /// one exists; `returns:` reads through the same door).
    pub(super) fn schema_of(&self, task_id: &str) -> Option<&serde_json::Value> {
        self.schemas
            .get(task_id)
            .copied()
            .or_else(|| self.lowered.get(task_id))
    }
}

/// How a surface treats a `tasks.*` reference (spec `04-variables.md`
/// §the reference boundary · W2).
#[derive(Clone, Copy)]
enum TaskRefRule {
    /// A `with:` value — the binding IS the edge. Existence is the DAG
    /// layer's report (`NIKA-DAG-002`); the scan keeps the record-shape
    /// checks (bare envelope · unknown field) for KNOWN tasks.
    Boundary,
    /// A body surface (`when:` · `for_each:` · a verb field) — ANY
    /// `tasks.*` reference is outside the boundary (`NIKA-VAR-021` ·
    /// the hoist is machine-applicable).
    Body(&'static str),
    /// A read-only resolution surface (workflow `outputs:` · `model:` ·
    /// `on_error.recover`) — existence + record-shape checks apply ·
    /// no edge semantics.
    Resolution,
    /// An `on_finally:` surface — the PARENT is the only readable task
    /// (a sibling read would race · `NIKA-VAR-021` otherwise).
    FinallyParentOnly,
}

/// Where a scanned string lives — drives the boundary / loop-local /
/// `with.` resolution rules.
struct ScanCtx<'a> {
    /// Human location for error messages (a task label · `outputs`). Used
    /// for the `location:` field of `UnresolvedNamespaceRef`, which renders
    /// it PLAINLY (`in {location}`).
    location: String,
    /// The BARE owning task id (empty at workflow level). Used for the
    /// `task:` field of errors whose thiserror template already wraps the id
    /// in backticks — passing the wrapped [`Self::location`] there
    /// double-backticked the id in the rendered message.
    task_id: &'a str,
    /// How `tasks.*` references are judged on this surface.
    task_rule: TaskRefRule,
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

    // Workflow-level surfaces share one ctx shape (no task id · no
    // edge/loop-local rule): `outputs:` (fixture variables/001) and the
    // one templated envelope field, `model:` (deep/019 — an unresolved
    // ref there is NIKA-VAR-001 like any task surface).
    let envelope_ctx = |location: &str| ScanCtx {
        location: location.to_owned(),
        task_id: "",
        task_rule: TaskRefRule::Resolution,
        with_names: None,
        allow_loop_locals: false,
    };
    let outputs_ctx = envelope_ctx("outputs");
    for (_, decl) in &wf.outputs {
        let value = decl.value();
        scan_string(value, &outputs_ctx, &index, errors);
    }
    if let Some(model) = &wf.model {
        scan_string(model, &envelope_ctx("model"), &index, errors);
    }
}

/// Scan one task's surfaces.
fn scan_task(task: &RawTask, index: &WorkflowIndex<'_>, errors: &mut Vec<SchemaError>) {
    let id = task.id.value.as_str();
    let with_names: BTreeSet<&str> = task.with.iter().map(|(k, _)| k.value.as_str()).collect();
    let has_for_each = task.for_each.is_some();

    // `with:` — the data boundary (the binding IS the edge).
    let boundary_ctx = ScanCtx {
        location: format!("task `{id}`"),
        task_id: id,
        task_rule: TaskRefRule::Boundary,
        with_names: Some(&with_names),
        allow_loop_locals: has_for_each,
    };
    // Body surfaces read LOCAL names only (04 §the reference boundary).
    let body_ctx = |surface: &'static str| ScanCtx {
        location: format!("task `{id}`"),
        task_id: id,
        task_rule: TaskRefRule::Body(surface),
        with_names: Some(&with_names),
        allow_loop_locals: has_for_each,
    };

    // `when:` — the expression form is a single boolean-shaped island
    // over local namespaces (POST-gate · spec 03 §when) · the YAML
    // boolean literal has nothing to scan.
    if let Some(when) = &task.when
        && let WhenGate::Expr(expr) = &when.value
    {
        let spanned = Spanned::new(expr.clone(), when.span);
        check_single_island(&spanned, "when", id, true, errors);
        scan_string(&spanned, &body_ctx("when:"), index, errors);
    }
    // `for_each:` — a PRE-fan-out body surface (the collection crosses
    // the boundary through with: · spec 03 §for_each).
    if let Some(for_each) = &task.for_each {
        match &for_each.value {
            ForEachValue::Expression(expr) => {
                let spanned = Spanned::new(expr.clone(), for_each.span);
                check_single_island(&spanned, "for_each", id, false, errors);
                scan_string(&spanned, &body_ctx("for_each:"), index, errors);
            }
            ForEachValue::List(list) => {
                let spanned = Spanned::new(list.clone(), for_each.span);
                scan_json(&spanned, &body_ctx("for_each:"), index, errors);
            }
        }
    }
    // `with:` values — the boundary itself.
    for (_, value) in &task.with {
        scan_json(value, &boundary_ctx, index, errors);
    }
    // Verb fields — body surfaces.
    scan_action(&task.action, &body_ctx("a verb field"), index, errors);

    // `output:` bindings — reserved names + pure-jq (no `${{`).
    check_output_bindings(task, errors);

    // `on_error.recover:` — a resolution surface · NO edge semantics
    // (a fallback source is a settled record · spec 05 §recover).
    let no_edge_ctx = ScanCtx {
        location: format!("task `{id}` on_error"),
        task_id: id,
        task_rule: TaskRefRule::Resolution,
        with_names: Some(&with_names),
        allow_loop_locals: has_for_each,
    };
    if let Some(on_error) = &task.on_error
        && let crate::types::OnErrorAction::Recover(value) = &on_error.value.action
    {
        scan_json(value, &no_edge_ctx, index, errors);
    }

    // `on_finally:` — the PARENT is the only readable task (W2 · a
    // sibling may still be RUNNING when this cleanup fires — the read
    // would race · spec 03 §on_finally).
    let finally_ctx = ScanCtx {
        location: format!("task `{id}` on_finally"),
        task_id: id,
        task_rule: TaskRefRule::FinallyParentOnly,
        with_names: Some(&with_names),
        allow_loop_locals: has_for_each,
    };
    for cleanup in &task.on_finally {
        if let Some(when) = &cleanup.value.when
            && let WhenGate::Expr(expr) = &when.value
        {
            let spanned = Spanned::new(expr.clone(), when.span);
            check_single_island(&spanned, "when", id, true, errors);
            scan_string(&spanned, &finally_ctx, index, errors);
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
            match &exec.command {
                crate::raw::RawCommand::Shell(c) => strings.push(c),
                crate::raw::RawCommand::Argv(parts) => strings.extend(parts.iter()),
            }
            strings.extend(exec.cwd.as_ref());
            strings.extend(exec.stdin.as_ref());
            for (_, value) in &exec.env {
                strings.push(value);
            }
        }
        RawAction::Invoke(invoke) => {
            // The `workflow:` target is NOT an expression surface — a
            // templated target is refused whole (NIKA-COMP-001, spec 14),
            // one voice, so the island scanner never second-guesses it.
            if let Some(tool) = invoke.tool() {
                strings.push(tool);
            }
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
        // The teaching routes by DECLARED shape (agent battery A1 ·
        // 2026-07-11): the old examples (`> 0` · `!= ""`) applied to a
        // declared BOOLEAN would trade VAR-005 for a type error (rule 4 ·
        // no implicit coercion) — the bool route leads, since a bare flag
        // reference is the most natural thing this rule rejects.
        errors.push(SchemaError::WhenNotBoolean {
            field: field.to_owned(),
            task: task.to_owned(),
            reason: format!(
                "`{src}` is not boolean-shaped — the shape rule (cel-subset/0.1) wants \
                 an explicit relation or boolean operator: a boolean reads \
                 `{src} == true` (or `!{src}`) · a number `{src} > 0` · a string \
                 `{src} != \"\"`",
                src = islands[0].src
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

/// Map an [`ExprError`] into the schema error surface, splitting the two
/// spec codes the `${{ }}` surface owns: an UNTERMINATED island (a `${{`
/// with no closing `}}`) is `NIKA-VAR-008` ([`SchemaError::TemplateSyntax`] ·
/// « unclosed `${{` opener »); a CLOSED island whose CEL is outside the
/// `cel-subset/0.1` grammar (chained relation · unknown function ·
/// arithmetic · stray token) is `NIKA-VAR-005`
/// ([`SchemaError::ExpressionViolation`] · « static expression violation »).
/// Conflating the two made a chained-relation report `nika explain
/// NIKA-VAR-008` → "unclosed `${{` opener", a wrong diagnostic.
fn template_error(e: &ExprError, span: Span) -> SchemaError {
    match e {
        ExprError::UnterminatedTemplate { .. } => SchemaError::TemplateSyntax {
            reason: e.to_string(),
            span: Some(span),
        },
        _ => SchemaError::ExpressionViolation {
            reason: e.to_string(),
            span: Some(span),
        },
    }
}

/// Validate one classified root reference (spec `04-variables.md`
/// §Resolution order · `NIKA-VAR-001` class) + the `NIKA-VAR-021`
/// boundary rule for `tasks.<id>` refs.
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
                let hint = suggest_in("vars", name, index.vars.iter().copied());
                errors.push(unresolved(&format!("vars.{name}"), ctx, span, hint));
            }
        }
        NamespaceRef::Env(name) => {
            if !index.env.contains(name.as_str()) {
                let hint = suggest_in("env", name, index.env.iter().copied());
                errors.push(unresolved(&format!("env.{name}"), ctx, span, hint));
            }
        }
        NamespaceRef::Secrets(name) => {
            if !index.secrets.contains(name.as_str()) {
                let hint = suggest_in("secrets", name, index.secrets.iter().copied());
                errors.push(unresolved(&format!("secrets.{name}"), ctx, span, hint));
            }
        }
        NamespaceRef::With(name) => {
            let declared = ctx
                .with_names
                .is_some_and(|names| names.contains(name.as_str()));
            if !declared {
                let hint = ctx
                    .with_names
                    .and_then(|names| suggest_in("with", name, names.iter().copied()));
                errors.push(unresolved(&format!("with.{name}"), ctx, span, hint));
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
                    // the bare id · the #[error] template wraps it (`task
                    // `{task}``) — passing the wrapped `location` here would
                    // double-backtick it.
                    task: ctx.task_id.to_owned(),
                    span: Some(span),
                });
            }
        }
        NamespaceRef::Unknown(root) => {
            // a typo'd NAMESPACE root (`vrs.x`) — suggest among the roots
            const ROOTS: [&str; 7] = ["env", "index", "item", "secrets", "tasks", "vars", "with"];
            let hint = crate::suggest::did_you_mean(root, ROOTS).map(str::to_owned);
            errors.push(unresolved(root, ctx, span, hint));
        }
    }
}

/// `tasks.<id>[.<field>]` — judged by the surface's boundary rule
/// (spec `04-variables.md` §the reference boundary) · then existence ·
/// then the record-shape checks (bare envelope · unknown field).
fn check_task_ref(
    id: &str,
    field: Option<&str>,
    span: Span,
    ctx: &ScanCtx<'_>,
    index: &WorkflowIndex<'_>,
    errors: &mut Vec<SchemaError>,
) {
    match ctx.task_rule {
        TaskRefRule::Body(surface) => {
            // NIKA-VAR-021 dominates: the fix (hoist into with:) removes
            // the reference from this surface entirely — existence and
            // shape are judged at the boundary after the hoist.
            errors.push(SchemaError::RefOutsideBoundary {
                task: ctx.task_id.to_owned(),
                surface: surface.to_owned(),
                reference: id.to_owned(),
                span: Some(span),
            });
            return;
        }
        TaskRefRule::FinallyParentOnly => {
            if id != ctx.task_id {
                errors.push(SchemaError::RefOutsideBoundary {
                    task: ctx.task_id.to_owned(),
                    surface: "on_finally".to_owned(),
                    reference: id.to_owned(),
                    span: Some(span),
                });
                return;
            }
        }
        TaskRefRule::Boundary | TaskRefRule::Resolution => {}
    }
    if !index.task_ids.contains(id) {
        if matches!(ctx.task_rule, TaskRefRule::Boundary) {
            return; // an edge to nowhere · the DAG layer reports NIKA-DAG-002
        }
        let hint = suggest_in("tasks", id, index.task_ids.iter().copied());
        errors.push(unresolved(&format!("tasks.{id}"), ctx, span, hint));
        return;
    }
    if field.is_none() {
        // 0.103 · #75 D2 — the envelope is not a value: the projection
        // set is CLOSED and required (kills the #524 golden-drift class
        // at the root · the pre-0.103 bare form denoted the whole record).
        errors.push(SchemaError::BareTaskEnvelope {
            task: id.to_owned(),
            location: ctx.location.clone(),
            span: Some(span),
        });
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
}

/// Build a `NIKA-VAR-001`-class unresolved-reference error.
fn unresolved(
    reference: &str,
    ctx: &ScanCtx<'_>,
    span: Span,
    suggestion: Option<String>,
) -> SchemaError {
    SchemaError::UnresolvedNamespaceRef {
        reference: reference.to_owned(),
        location: ctx.location.clone(),
        suggestion,
        span: Some(span),
    }
}

/// The fully-qualified did-you-mean within ONE namespace's declared
/// names (`vars.topic` for a typo'd `vars.topci`) — suggestions never
/// cross namespaces (a `vars.` typo is not repaired with a secret).
fn suggest_in<'a>(
    namespace: &str,
    name: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    crate::suggest::did_you_mean(name, candidates).map(|s| format!("{namespace}.{s}"))
}

#[cfg(test)]
mod tests {
    use crate::analyzer::analyze;
    use crate::error::SchemaError;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn analyze_yaml(yaml: &str) -> Result<crate::analyzer::AnalyzedWorkflow, Vec<SchemaError>> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        analyze(&wf)
    }

    const HEADER: &str = "nika: v1\nworkflow:\n  id: t\n";

    /// The unresolved-ref reference string carried by the first finding.
    fn sole_unresolved(yaml: &str) -> String {
        let errors = analyze_yaml(yaml).expect_err("unresolved ref");
        errors
            .iter()
            .find_map(|e| match e {
                SchemaError::UnresolvedNamespaceRef { reference, .. } => Some(reference.clone()),
                _ => None,
            })
            .expect("an UnresolvedNamespaceRef finding")
    }

    #[test]
    fn scan_json_descends_into_an_array_value() {
        // Kills `scan_json::walk` 377 — delete the `Value::Array(items)` arm.
        // The `with:` value is a JSON ARRAY whose element is an
        // `${{ vars.ghost }}` island referencing an UNDECLARED var. The walker
        // MUST recurse into the array element to scan it → an
        // `UnresolvedNamespaceRef` for `vars.ghost`. Deleting the Array arm
        // skips the nested string, so the unresolved ref goes silently
        // unreported and the workflow wrongly analyzes clean.
        let yaml = format!(
            "{HEADER}tasks:
  t:
    with: {{ payload: [\"${{{{ vars.ghost }}}}\"] }}
    exec: {{ command: [echo] }}
"
        );
        assert_eq!(
            sole_unresolved(&yaml),
            "vars.ghost",
            "a ref nested inside a `with:` ARRAY must still be resolved"
        );
    }

    #[test]
    fn scan_json_descends_into_an_object_value() {
        // Kills `scan_json::walk` 382 — delete the `Value::Object(map)` arm.
        // The `with:` value is a nested JSON OBJECT whose leaf is an
        // `${{ vars.ghost }}` island. The walker MUST recurse through the
        // object's values to scan it → an `UnresolvedNamespaceRef`. Deleting
        // the Object arm skips the nested string, hiding the unresolved ref.
        let yaml = format!(
            "{HEADER}tasks:
  t:
    with: {{ payload: {{ inner: \"${{{{ vars.ghost }}}}\" }} }}
    exec: {{ command: [echo] }}
"
        );
        assert_eq!(
            sole_unresolved(&yaml),
            "vars.ghost",
            "a ref nested inside a `with:` OBJECT must still be resolved"
        );
    }
}
