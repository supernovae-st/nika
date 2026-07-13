// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Improvement hints — the deterministic « ameliorateur ».
//!
//! Findings say « this is broken »; hints say « this could be BETTER »,
//! each with the concrete change that unlocks a stronger static
//! guarantee. They are advisory (never fail the check — `is_clean`
//! ignores them) and fully deterministic: the same workflow always
//! yields the same hints, because each one is derived from a structural
//! property the analyzer already computed.
//!
//! The hint classes, ranked by unlocked value ·
//!
//! - **unbounded cost** (`cost`) — an `infer:`/`agent:` task with no
//!   token bound: add one and the cost report becomes a hard ceiling.
//! - **unconsumed output** (`dead-spend`) — a pure `infer:` task whose
//!   output no one reads (no task references it · not in `outputs:`):
//!   every token it spends is dead spend.
//! - **opaque consumed output** (`typing`) — a task whose output IS
//!   deeply referenced (`tasks.X.output.field`) but declares no
//!   `schema:` / `output:` bindings: declare a shape and the dataflow
//!   typer starts proving those references.
//! - **no boundary** (`permits`) — effectful tasks and no `permits:`
//!   block: `--infer-permits` writes the tightest one.
//! - **open schema** (`strictness`) — an object schema admitting
//!   undeclared keys: close it and the output shape is deterministic.
//! - **grammar-blind constraint** (`schema-portability`) — keywords no
//!   provider grammar enforces — see [`push_portability_hint`].
//! - **redundant success-gate** (`redundant-gate`) — `when: ${{
//!   tasks.D.status == 'success' }}` where `D` is a dep that can never
//!   be `skipped`: the spec names this the discouraged restatement of
//!   the default gate (spec 03 §the gate).
//! - **retry on uncontracted effects** (`retry-effects`) — see
//!   [`push_retry_effects_hint`].
//! - **concurrent same-path writers** (`parallel-writers`) — emitted by
//!   the DAG analysis pass (`check/analysis.rs`) and merged here.
//! - **exec with a native path** (`native-first`) — emitted by the
//!   `check/native_first.rs` pass (the `native-first/001..005` ruleset:
//!   http/file/data/media/helper commands a builtin or MCP tool
//!   covers); `nika check --native-strict` promotes them to failures.
//! - **exec JSON stdout capture** (`exec-json-capture`) — an `exec:` task
//!   declares `capture: structured`, a binding parses `.stdout | fromjson`,
//!   and NO binding reads `exit_code`/`stderr`; use `capture: stdout` for
//!   JSON-producing helpers so non-zero exits fail as `NIKA-EXEC-001`
//!   instead of becoming data (a task branching on the record keeps
//!   `structured` — the hint stays silent there).
//! - **unwrapped reference** (`unwrapped-ref`) — a workflow `outputs:`
//!   value that spells a reference path (`tasks.X.output…` · `vars.X` · …)
//!   without the `${{ }}` wrapper rides as the LITERAL STRING (the run
//!   returns the path text, not the value); the hint names the wrap.
//! - **envelope bound into outputs** (`envelope-output`) — an
//!   `outputs:` binding referencing a BARE `tasks.X` captures the whole
//!   envelope (status · timestamps · output), so `nika test` goldens
//!   drift on the timestamps every run; bind `tasks.X.output` for the
//!   value. Suppresses `dead-spend` for the same task (the output IS
//!   consumed — in trap form).

use std::collections::BTreeSet;

use crate::expression::{Expr, Literal, RelOp, bare_task_refs, scan_templates, task_output_paths};
use crate::raw::{RawAction, RawTask, RawWorkflow};
use crate::types::{CaptureMode, OnErrorAction, VarDecl};

/// One advisory improvement with its concrete unlock.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct Hint {
    /// The hint class — the closed set today: `cost` · `dead-spend` ·
    /// `typing` · `permits` · `strictness` · `schema-portability` ·
    /// `redundant-gate` · `retry-effects` · `parallel-writers` ·
    /// `secrets-store` · `native-first` · `exec-json-capture` ·
    /// `unwrapped-ref` · `envelope-output` (additive · agents route on
    /// it; the module doc describes each).
    pub kind: &'static str,
    /// The task it concerns (`-` for workflow-level hints).
    pub task: String,
    /// What to change and what it unlocks.
    pub advice: String,
}

/// Compute the improvement hints for a workflow.
#[must_use]
pub(super) fn scan_hints(wf: &RawWorkflow) -> Vec<Hint> {
    let consumed = consumed_outputs(wf);
    let deep_referenced = deeply_referenced(wf);
    let envelope_bound = envelope_bound_outputs(wf);
    let envelope_ids: BTreeSet<&str> = envelope_bound.iter().map(|(_, id)| id.as_str()).collect();
    let mut hints = Vec::new();
    for (name, id) in &envelope_bound {
        hints.push(Hint {
            kind: "envelope-output",
            task: id.clone(),
            advice: format!(
                "outputs.{name} binds the whole ENVELOPE of `{id}` (status · timestamps · \
                 output) — `nika test` goldens drift on its timestamps every run; for the \
                 value alone bind ${{{{ tasks.{id}.output }}}}"
            ),
        });
    }

    let mut any_effect = false;
    for task in &wf.tasks {
        let t = &task.value;
        let id = t.id.value.as_str();
        // 6. redundant success-gate — meaningful only if the dep may be
        //    skipped (when:-gated or on_error: skip); otherwise the
        //    default gate already requires success.
        if let Some(when) = &t.when
            && let Some(src) = when.value.as_expr()
            && let Some(dep) = sole_success_gate(src)
            && t.depends_on.iter().any(|d| d.value == dep)
            && let Some(dep_task) = wf.tasks.iter().find(|x| x.value.id.value == dep)
            && dep_task.value.when.is_none()
            && !matches!(
                dep_task.value.on_error.as_ref().map(|oe| &oe.value.action),
                Some(OnErrorAction::Skip)
            )
        {
            hints.push(Hint {
                kind: "redundant-gate",
                task: id.to_owned(),
                advice: format!(
                    "`when:` restates the default gate \u{2014} `depends_on: [{dep}]` already requires `{dep}` to succeed (spec 03 \u{a7}the gate); drop the `when:` (it becomes meaningful only if `{dep}` may be skipped)"
                ),
            });
        }
        match &t.action {
            RawAction::Infer(a) => {
                if a.max_tokens.is_none() {
                    hints.push(hint("cost", id, format!(
                        "declare `max_tokens` on `{id}` — the cost report becomes a hard ceiling instead of UNBOUNDED"
                    )));
                }
                if !consumed.contains(id) && !envelope_ids.contains(id) {
                    hints.push(hint("dead-spend", id, format!(
                        "no task or output consumes `tasks.{id}.output` — every token this infer spends is unread; consume it or remove the task"
                    )));
                }
                if deep_referenced.contains(id) && a.schema.is_none() && t.output.is_empty() {
                    hints.push(hint("typing", id, format!(
                        "deep references into `tasks.{id}.output.<field>` exist but `{id}` declares no `schema:` — declare one and `nika check` starts proving those field names"
                    )));
                }
                push_strictness_hint(&mut hints, id, a.schema.as_ref().map(|s| &s.value));
                push_portability_hint(&mut hints, id, a.schema.as_ref().map(|s| &s.value));
            }
            RawAction::Agent(a) => {
                if a.max_tokens_total.is_none() {
                    hints.push(hint("cost", id, format!(
                        "declare `max_tokens_total` on `{id}` — the agent loop gets a hard budget instead of UNBOUNDED"
                    )));
                }
                push_strictness_hint(&mut hints, id, a.schema.as_ref().map(|s| &s.value));
                push_portability_hint(&mut hints, id, a.schema.as_ref().map(|s| &s.value));
                any_effect = true; // an agent dispatches tools
            }
            RawAction::Exec(exec) => {
                push_exec_json_capture_hint(&mut hints, t, exec);
                any_effect = true;
            }
            RawAction::Invoke(_) => any_effect = true,
        }
        push_retry_effects_hint(&mut hints, t);
    }

    if any_effect && wf.permits.is_none() {
        hints.push(hint(
            "permits",
            "-",
            "no `permits:` boundary declared — run `nika check --infer-permits` to generate the tightest one (default-deny once present)".to_owned(),
        ));
    }
    push_unresolvable_secret_hints(&mut hints, wf);
    push_unwrapped_output_ref_hints(&mut hints, wf);
    hints
}

/// The `unwrapped-ref` hint (output gauntlet 2026-07-11): a workflow
/// `outputs:` value that LOOKS like a reference (`tasks.<id>.output…` ·
/// `vars.<x>` · `env.<x>` · `with.<x>` · `secrets.<x>`) but carries no
/// `${{ }}` island rides as the LITERAL STRING — the run returns
/// `"tasks.data.output.count"`, not the extracted value. A silent footgun
/// (the workflow « works » and returns the wrong thing); the hint names
/// the wrap. Advisory: a literal string that happens to spell a namespace
/// path is legal (absurd, but the author's call), so this teaches, never
/// fails. The pattern is distinctive — a bare namespace-dotted path is
/// almost never a wanted constant.
fn push_unwrapped_output_ref_hints(hints: &mut Vec<Hint>, wf: &RawWorkflow) {
    const NAMESPACES: [&str; 5] = ["tasks.", "vars.", "env.", "with.", "secrets."];
    for (name, decl) in &wf.outputs {
        let value = &decl.value().value;
        // Already interpolated (any `${{ }}`) → the author knows the wrapper.
        if value.contains("${{") {
            continue;
        }
        let trimmed = value.trim();
        if NAMESPACES.iter().any(|ns| trimmed.starts_with(ns)) {
            hints.push(hint(
                "unwrapped-ref",
                &name.value,
                format!(
                    "output `{}` is the literal string `{trimmed}` — it looks like a reference; \
                     wrap it to interpolate: `${{{{ {trimmed} }}}}`",
                    name.value
                ),
            ));
        }
    }
}

/// `capture: structured` is for branching on `{stdout, stderr, exit_code}`
/// as data. When a binding parses `.stdout` as JSON and NO binding reads the
/// record's other fields (`exit_code` · `stderr`), the one-obvious-way is
/// `capture: stdout` + `fromjson`: a missing helper or non-zero subprocess
/// then fails as `NIKA-EXEC-001` with stderr preserved, rather than
/// surfacing later as an output-binding cardinality error. A task that DOES
/// branch on `exit_code`/`stderr` uses `structured` legitimately — the hint
/// stays silent there (its own advice would break that binding).
fn push_exec_json_capture_hint(
    hints: &mut Vec<Hint>,
    task: &RawTask,
    action: &crate::raw::RawExecAction,
) {
    if !matches!(
        action.capture.as_ref().map(|capture| capture.value),
        Some(CaptureMode::Structured)
    ) {
        return;
    }
    // The `.stdout | fromjson` chain, whitespace-insensitive — an unrelated
    // field that merely CONTAINS the substrings (`.stderr | fromjson |
    // .stdout_field`) is not the pattern.
    let parses_stdout_json = task.output.iter().any(|(_, binding)| {
        let compact: String = binding
            .value
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        compact.contains(".stdout|fromjson")
    });
    // Another binding consuming the structured record's OTHER fields means
    // `structured` is the point, not an accident.
    let reads_record_fields = task.output.iter().any(|(_, binding)| {
        binding.value.contains(".exit_code") || binding.value.contains(".stderr")
    });
    if parses_stdout_json && !reads_record_fields {
        let id = task.id.value.as_str();
        hints.push(hint(
            "exec-json-capture",
            id,
            format!(
                "`{id}` parses `.stdout | fromjson` while using `capture: structured` and no binding reads `exit_code`/`stderr` — for a JSON-producing helper, use `capture: stdout` and bindings like `fromjson`: a failing subprocess then errors as NIKA-EXEC-001 instead of becoming data"
            ),
        ));
    }
}

/// The `secrets-store` hint (MINOR-B): a referenced `secrets.X` whose
/// `source` the runtime cannot resolve yet (`vault`) — without this the
/// check is GREEN but the value fails at runtime with NIKA-1702 (an
/// unresolved reference). The hint names the gap so the author switches the
/// store (`env`/`file`) or waits for vault wiring, rather than hitting a
/// green-check → runtime-1702 surprise. Only fires for a REFERENCED secret
/// (a declared-but-unused vault secret is harmless). Advisory — never fails
/// the check.
fn push_unresolvable_secret_hints(hints: &mut Vec<Hint>, wf: &RawWorkflow) {
    use crate::types::SecretSource;
    if wf.secrets.is_empty() {
        return;
    }
    let referenced = referenced_secrets(wf);
    for (name, secret) in &wf.secrets {
        // `env`/`file` are wired; only the not-yet-resolvable sources warn.
        if matches!(secret.value.source, SecretSource::Env | SecretSource::File) {
            continue;
        }
        if referenced.contains(name.value.as_str()) {
            hints.push(hint(
                "secrets-store",
                "-",
                format!(
                    "`secrets.{name}` uses source `{source}`, not yet runtime-resolvable \u{2014} the check is green but `${{{{ secrets.{name} }}}}` will fail at run with NIKA-1702; use `source: env` or `source: file` until vault resolution ships",
                    name = name.value,
                    source = secret.value.source,
                ),
            ));
        }
    }
}

/// Every `secrets.<name>` referenced anywhere in the workflow's `${{ }}`
/// islands (task fields · `with:` · `outputs:`) — drives the `secrets-store`
/// hint so it fires only for a USED secret.
fn referenced_secrets(wf: &RawWorkflow) -> BTreeSet<String> {
    use crate::expression::{NamespaceRef, expr_refs};
    let mut out = BTreeSet::new();
    let mut collect = |text: &str| {
        if let Ok(islands) = scan_templates(text) {
            for island in &islands {
                for r in expr_refs(&island.expr) {
                    if let NamespaceRef::Secrets(name) = r {
                        out.insert(name);
                    }
                }
            }
        }
    };
    for task in &wf.tasks {
        for text in task_text_fields(&task.value) {
            collect(text);
        }
    }
    for (_, decl) in &wf.outputs {
        collect(decl.value().value.as_str());
    }
    out
}

/// Every authored text fragment of a task that may carry a `${{ secrets.X }}`
/// island (effect fields · `with:` values · `when:` body) — the surface
/// [`referenced_secrets`] scans.
fn task_text_fields(t: &crate::raw::RawTask) -> Vec<&str> {
    let mut fields = Vec::new();
    match &t.action {
        RawAction::Exec(a) => {
            fields.extend(a.command.text_fragments());
            if let Some(stdin) = &a.stdin {
                fields.push(stdin.value.as_str());
            }
            for (_, v) in &a.env {
                fields.push(v.value.as_str());
            }
        }
        RawAction::Invoke(a) => {
            if let Some(args) = a.args.as_ref() {
                collect_json_strings_into(&args.value, &mut fields);
            }
        }
        RawAction::Infer(a) => {
            fields.push(a.prompt.value.as_str());
            if let Some(s) = &a.system {
                fields.push(s.value.as_str());
            }
        }
        RawAction::Agent(a) => {
            fields.push(a.prompt.value.as_str());
            if let Some(s) = &a.system {
                fields.push(s.value.as_str());
            }
        }
    }
    for (_, v) in &t.with {
        collect_json_strings_into(&v.value, &mut fields);
    }
    fields
}

/// Every string leaf of a JSON value (with-values · invoke args).
fn collect_json_strings_into<'a>(value: &'a serde_json::Value, out: &mut Vec<&'a str>) {
    match value {
        serde_json::Value::String(s) => out.push(s.as_str()),
        serde_json::Value::Array(items) => {
            for it in items {
                collect_json_strings_into(it, out);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_json_strings_into(v, out);
            }
        }
        _ => {}
    }
}

/// The retry-safety hint (class `retry-effects`): `retry:` replays the
/// WHOLE attempt on transient failure — at-least-once semantics. For
/// effect classes with no idempotency contract that means duplicated
/// side effects (a subprocess killed mid-write already mutated the
/// world; the replay mutates it again). Conservative scope — only the
/// classes whose contract is genuinely unknown:
///
/// - `exec:` — arbitrary subprocess, side effects unknowable;
/// - `invoke: mcp:*` — external tool, no idempotency contract.
///
/// `nika:` builtins carry documented semantics (atomic-overwrite write
/// · GET fetch) and an `infer:` retry re-spends tokens but mutates
/// nothing external — no claim on those. An `agent:` retry DOES replay
/// its whole tool loop, but which effects that re-dispatches depends on
/// the runtime whitelist state — tool-mediated and out of this static
/// rung's scope (a dedicated agent-retry read would own it). The formal
/// idempotency treatments verify ENGINES, not workflow files (Rehearsal
/// · Shambaugh et al. · PLDI 2016 · Puppet manifests via SMT; Durable
/// Functions · Burckhardt et al. · OOPSLA 2021 · deterministic-replay
/// semantics) — this hint is the static, file-level read of the hazard.
fn push_retry_effects_hint(hints: &mut Vec<Hint>, t: &crate::raw::RawTask) {
    let retries = t.retry.as_ref().is_some_and(|r| r.value.max_attempts > 1);
    if !retries {
        return;
    }
    let id = t.id.value.as_str();
    match &t.action {
        RawAction::Exec(_) => {
            hints.push(hint("retry-effects", id, format!(
                "`{id}` retries a subprocess — a transient failure mid-effect replays side effects already applied (at-least-once); make the command idempotent or guard it with a pre-check"
            )));
        }
        RawAction::Invoke(a) if a.tool.value.starts_with("mcp:") => {
            hints.push(hint("retry-effects", id, format!(
                "`{id}` retries `{}` — external MCP tools carry no idempotency contract; a transient failure replays the call's side effects (at-least-once)",
                a.tool.value
            )));
        }
        _ => {}
    }
}

/// The structured-output determinism hint (class `strictness`): an
/// object node declaring `properties` but NOT `additionalProperties:
/// false` admits undeclared keys — the model can emit extra fields and
/// the validated shape varies across providers/runs. Closing it pins
/// the shape (the recipe provider-native strict modes require). One
/// hint per task, however many open nodes.
fn push_strictness_hint(hints: &mut Vec<Hint>, id: &str, schema: Option<&serde_json::Value>) {
    if schema.is_some_and(has_open_object) {
        hints.push(hint("strictness", id, format!(
            "`{id}`'s schema admits undeclared keys — add `additionalProperties: false` to its object nodes for a deterministic output shape across providers"
        )));
    }
}

/// Visit every child subschema of one node — the ONE composite descent the
/// schema walkers share (`properties` values · `items` · branch keywords).
fn for_each_subschema(
    obj: &serde_json::Map<String, serde_json::Value>,
    f: &mut impl FnMut(&serde_json::Value),
) {
    let props = obj.get("properties").and_then(serde_json::Value::as_object);
    props
        .into_iter()
        .flat_map(serde_json::Map::values)
        .for_each(&mut *f);
    for key in [
        "items", "not", "if", "then", "else", "anyOf", "oneOf", "allOf",
    ] {
        match obj.get(key) {
            Some(serde_json::Value::Array(kids)) => kids.iter().for_each(&mut *f),
            Some(kid) => f(kid),
            None => {}
        }
    }
}

/// Whether any object node in the schema declares `properties` without
/// closing `additionalProperties`; `$ref` is opaque (no claim).
fn has_open_object(node: &serde_json::Value) -> bool {
    node.as_object()
        .filter(|o| !o.contains_key("$ref"))
        .is_some_and(|obj| {
            let closed = obj.get("additionalProperties") == Some(&serde_json::Value::Bool(false));
            let has_props = obj
                .get("properties")
                .and_then(serde_json::Value::as_object)
                .is_some();
            let mut open = !closed && has_props;
            for_each_subschema(obj, &mut |child| open = open || has_open_object(child));
            open
        })
}

/// The `schema-portability` hint: keywords NO provider grammar enforces
/// (proven live 2026-07-07) — only LOCAL validation holds them, per-retry.
fn push_portability_hint(hints: &mut Vec<Hint>, id: &str, schema: Option<&serde_json::Value>) {
    let mut found = BTreeSet::new();
    schema.inspect(|s| collect_grammar_blind(s, &mut found));
    if !found.is_empty() {
        let list = found.into_iter().collect::<Vec<_>>().join("` · `");
        hints.push(hint("schema-portability", id, format!(
            "`{id}`'s schema relies on `{list}` — provider grammars accept but do NOT enforce these keywords (constrained decoding emits violating values unchecked); only Nika's local validation holds them, spending schema retries when the model strays. Express the constraint structurally (`enum` · item bounds · closed objects) where possible"
        )));
    }
}

/// Binding occurrences only (`uniqueItems: false` / a bare `if` constrain
/// nothing — no claim); property NAMES are never keywords; `$ref` opaque.
fn collect_grammar_blind(node: &serde_json::Value, out: &mut BTreeSet<&'static str>) {
    if let Some(obj) = node.as_object().filter(|o| !o.contains_key("$ref")) {
        let cond = obj.contains_key("if") && (obj.contains_key("then") || obj.contains_key("else"));
        let unique = obj.get("uniqueItems").and_then(serde_json::Value::as_bool) == Some(true);
        out.extend(unique.then_some("uniqueItems"));
        out.extend(obj.contains_key("not").then_some("not"));
        out.extend(cond.then_some("if/then/else"));
        for_each_subschema(obj, &mut |kid| collect_grammar_blind(kid, out));
    }
}

/// Task ids whose output is referenced ANYWHERE (any `tasks.X.output…`
/// chain in any island, or an envelope `outputs:` entry).
fn consumed_outputs(wf: &RawWorkflow) -> BTreeSet<String> {
    let mut consumed = BTreeSet::new();
    for_each_island_text(wf, &mut |text| {
        if let Ok(islands) = scan_templates(text) {
            for island in islands {
                for (target, _) in task_output_paths(&island.expr) {
                    consumed.insert(target);
                }
            }
        }
    });
    consumed
}

/// `(output name, task id)` for every `outputs:` binding that
/// references a BARE task envelope (`tasks.X` — no field hop). Scoped
/// to `outputs:` deliberately: a bare envelope in a gate or a prompt is
/// legitimate plumbing; bound into the workflow's public contract it is
/// the golden-drift trap.
fn envelope_bound_outputs(wf: &RawWorkflow) -> Vec<(String, String)> {
    let mut bound = Vec::new();
    for (name, decl) in &wf.outputs {
        if let Ok(islands) = scan_templates(&decl.value().value) {
            for island in islands {
                for id in bare_task_refs(&island.expr) {
                    bound.push((name.value.clone(), id));
                }
            }
        }
    }
    bound
}

/// Task ids referenced with a DEEP path (`tasks.X.output.field…`).
fn deeply_referenced(wf: &RawWorkflow) -> BTreeSet<String> {
    let mut deep = BTreeSet::new();
    for_each_island_text(wf, &mut |text| {
        if let Ok(islands) = scan_templates(text) {
            for island in islands {
                for (target, path) in task_output_paths(&island.expr) {
                    if !path.is_empty() {
                        deep.insert(target);
                    }
                }
            }
        }
    });
    deep
}

/// Visit every expression-bearing text in the workflow (the same surface
/// the dataflow typer walks: verbs · `when:` · `with:` · `for_each` ·
/// `on_finally` · envelope `outputs:`).
fn for_each_island_text(wf: &RawWorkflow, visit: &mut dyn FnMut(&str)) {
    for task in &wf.tasks {
        let t = &task.value;
        visit_action(&t.action, visit);
        if let Some(when) = &t.when
            && let Some(expr) = when.value.as_expr()
        {
            visit(expr);
        }
        if let Some(f) = &t.for_each
            && let crate::raw::ForEachValue::Expression(src) = &f.value
        {
            visit(src);
        }
        for (_, v) in &t.with {
            visit_json(&v.value, visit);
        }
        for cleanup in &t.on_finally {
            if let Some(when) = &cleanup.value.when
                && let Some(expr) = when.value.as_expr()
            {
                visit(expr);
            }
            visit_action(&cleanup.value.action, visit);
        }
    }
    for (_, decl) in &wf.outputs {
        visit(&decl.value().value);
    }
}

fn visit_action(action: &RawAction, visit: &mut dyn FnMut(&str)) {
    match action {
        RawAction::Exec(a) => {
            for fragment in a.command.text_fragments() {
                visit(fragment);
            }
            if let Some(stdin) = &a.stdin {
                visit(&stdin.value);
            }
            for (_, v) in &a.env {
                visit(&v.value);
            }
        }
        RawAction::Invoke(a) => {
            if let Some(args) = &a.args {
                visit_json(&args.value, visit);
            }
        }
        RawAction::Infer(a) => {
            visit(&a.prompt.value);
            if let Some(system) = &a.system {
                visit(&system.value);
            }
        }
        RawAction::Agent(a) => {
            visit(&a.prompt.value);
            if let Some(system) = &a.system {
                visit(&system.value);
            }
        }
    }
}

fn visit_json(value: &serde_json::Value, visit: &mut dyn FnMut(&str)) {
    match value {
        serde_json::Value::String(s) => visit(s),
        serde_json::Value::Array(items) => {
            for item in items {
                visit_json(item, visit);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                visit_json(item, visit);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

/// Resolve an invoke arg to a STATIC string when it is a plain literal
/// or a single `${{ vars.X }}` whose declaration carries a literal
/// default — the two shapes a scaffold ships. Anything dynamic (task
/// refs · env · concatenations) resolves to `None`: analysis never
/// guesses.
fn static_string_arg(wf: &RawWorkflow, value: &serde_json::Value) -> Option<String> {
    let s = value.as_str()?;
    let trimmed = s.trim();
    if !trimmed.contains("${{") {
        return Some(trimmed.to_owned());
    }
    let inner = trimmed.strip_prefix("${{")?.strip_suffix("}}")?.trim();
    let var = inner.strip_prefix("vars.")?;
    if var.contains(['.', '[']) {
        return None;
    }
    wf.vars.iter().find_map(|(name, decl)| {
        if name.value != var {
            return None;
        }
        let default = match decl {
            VarDecl::Untyped(v) => Some(v),
            VarDecl::Typed { default, .. } => default.as_ref(),
        };
        default.and_then(|d| d.as_str()).map(str::to_owned)
    })
}

/// Every `nika:read` whose `path` arg resolves STATICALLY — the pure
/// half of the missing-input lint (V-arc F1 2026-07-09): the analyzer
/// names the (task · path) pairs, the CALLER decides what existence
/// means on its side of the I/O boundary (the CLI checks the local
/// filesystem; a server might check an artifact store).
#[must_use]
pub fn static_read_paths(wf: &RawWorkflow) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        let RawAction::Invoke(invoke) = &task.value.action else {
            continue;
        };
        if invoke.tool.value != "nika:read" {
            continue;
        }
        let Some(args) = &invoke.args else { continue };
        let Some(path_val) = args.value.get("path") else {
            continue;
        };
        if let Some(path) = static_string_arg(wf, path_val) {
            out.push((task.value.id.value.clone(), path));
        }
    }
    out
}

fn hint(kind: &'static str, task: &str, advice: String) -> Hint {
    Hint {
        kind,
        task: task.to_owned(),
        advice,
    }
}

/// The WHOLE gate is exactly `tasks.<dep>.status == 'success'` (either
/// operand order) — a conjunct inside a larger expression is a real
/// condition beyond the default gate and never flagged.
fn sole_success_gate(src: &str) -> Option<String> {
    let islands = scan_templates(src).ok()?;
    let island = islands.into_iter().next()?;
    let Expr::Relation {
        op: RelOp::Eq,
        lhs,
        rhs,
    } = &island.expr
    else {
        return None;
    };
    let (dep, lit) = super::reach::status_atom(lhs, rhs)?;
    matches!(lit, Expr::Lit(Literal::Str(s)) if s == "success").then(|| dep.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn hints_of(yaml: &str) -> Vec<Hint> {
        scan_hints(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn plain_success_gate_on_unskippable_dep_is_redundant() {
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    exec: { shell: \"true\" }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'success' }}\n    exec: { shell: \"true\" }\n",
        );
        assert!(
            h.iter()
                .any(|x| x.kind == "redundant-gate" && x.task == "b"),
            "{h:?}"
        );
    }

    #[test]
    fn success_gate_on_skippable_dep_is_meaningful_not_redundant() {
        // a may be skipped two ways — when:-gated · on_error: skip —
        // the spec's own « meaningful only when X may be skipped »
        let gated = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\nvars: { go: \"y\" }\ntasks:\n  - id: root\n    exec: { shell: \"true\" }\n  - id: a\n    depends_on: [root]\n    when: ${{ vars.go == 'y' }}\n    exec: { shell: \"true\" }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'success' }}\n    exec: { shell: \"true\" }\n",
        );
        assert!(
            !gated.iter().any(|x| x.kind == "redundant-gate"),
            "{gated:?}"
        );
        let skip_route = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    exec: { shell: \"true\" }\n    on_error: { skip: true }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'success' }}\n    exec: { shell: \"true\" }\n",
        );
        assert!(
            !skip_route.iter().any(|x| x.kind == "redundant-gate"),
            "{skip_route:?}"
        );
    }

    #[test]
    fn compound_or_reversed_or_other_status_is_not_flagged() {
        // conjunct = a condition beyond the default gate; reversed
        // operand IS the same plain gate; 'failure' is not the pattern
        let compound = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\nvars: { env: \"p\" }\ntasks:\n  - id: a\n    exec: { shell: \"true\" }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'success' && vars.env == 'p' }}\n    exec: { shell: \"true\" }\n",
        );
        assert!(
            !compound.iter().any(|x| x.kind == "redundant-gate"),
            "{compound:?}"
        );
        let reversed = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    exec: { shell: \"true\" }\n  - id: b\n    depends_on: [a]\n    when: ${{ 'success' == tasks.a.status }}\n    exec: { shell: \"true\" }\n",
        );
        assert!(
            reversed.iter().any(|x| x.kind == "redundant-gate"),
            "{reversed:?}"
        );
        let failure = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    exec: { shell: \"true\" }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'failure' }}\n    exec: { shell: \"true\" }\n",
        );
        assert!(
            !failure.iter().any(|x| x.kind == "redundant-gate"),
            "{failure:?}"
        );
    }

    #[test]
    fn unbounded_infer_gets_a_cost_hint() {
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\" }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(h.iter().any(|x| x.kind == "cost" && x.task == "a"), "{h:?}");
    }

    #[test]
    fn unconsumed_infer_is_dead_spend() {
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\n  - id: b\n    exec: { shell: \"echo done\" }\n",
        );
        assert!(h.iter().any(|x| x.kind == "dead-spend" && x.task == "a"));
        // consumed via outputs: → no dead-spend hint
        let h2 = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(!h2.iter().any(|x| x.kind == "dead-spend"), "{h2:?}");
    }

    /// The first-day trap, taught where it is born: `outputs.r: ${{
    /// tasks.a }}` binds the ENVELOPE — the hint names the output, the
    /// task, the drift, and the fix; the contradictory dead-spend voice
    /// (« nothing consumes it ») is suppressed for that task.
    #[test]
    fn envelope_bound_output_teaches_and_silences_dead_spend() {
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.a }}\n",
        );
        let env: Vec<_> = h.iter().filter(|x| x.kind == "envelope-output").collect();
        assert_eq!(env.len(), 1, "{h:?}");
        assert_eq!(env[0].task, "a");
        assert!(env[0].advice.contains("outputs.r"), "{}", env[0].advice);
        assert!(
            env[0].advice.contains("${{ tasks.a.output }}"),
            "the fix is spelled: {}",
            env[0].advice
        );
        assert!(
            !h.iter().any(|x| x.kind == "dead-spend"),
            "one voice — the envelope binding IS consumption: {h:?}"
        );
        // A bare envelope in a GATE is plumbing, not a trap — silent.
        let gate = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\n  - id: b\n    depends_on: [a]\n    when: ${{ size(tasks.a.output) > 0 }}\n    exec: { shell: \"echo go\" }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(
            !gate.iter().any(|x| x.kind == "envelope-output"),
            "{gate:?}"
        );
    }

    #[test]
    fn deeply_referenced_unschema_d_output_gets_a_typing_hint() {
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\n  - id: b\n    depends_on: [a]\n    exec: { shell: \"echo ${{ tasks.a.output.field }}\" }\n",
        );
        assert!(
            h.iter().any(|x| x.kind == "typing" && x.task == "a"),
            "{h:?}"
        );
        // shallow consumption only → no typing hint
        let h2 = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\n  - id: b\n    depends_on: [a]\n    exec: { shell: \"echo ${{ tasks.a.output }}\" }\n",
        );
        assert!(!h2.iter().any(|x| x.kind == "typing"), "{h2:?}");
    }

    #[test]
    fn effectful_workflow_without_permits_gets_the_boundary_hint() {
        let h = hints_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { shell: \"echo hi\" }\n",
        );
        assert!(h.iter().any(|x| x.kind == "permits"), "{h:?}");
        // boundary declared → no hint
        let h2 = hints_of(
            "nika: v1\nworkflow: w\npermits: { exec: true }\ntasks:\n  - id: t\n    exec: { shell: \"echo hi\" }\n",
        );
        assert!(!h2.iter().any(|x| x.kind == "permits"), "{h2:?}");
    }

    #[test]
    fn structured_exec_parsing_stdout_json_gets_capture_hint() {
        let h = hints_of(
            "nika: v1\nworkflow: w\npermits: { exec: true }\ntasks:\n  - id: crawl\n    exec:\n      command: [\"node\", \"helper.mjs\"]\n      capture: structured\n    output:\n      crawl: \".stdout | fromjson\"\n      url: \".stdout | fromjson | .url\"\n",
        );
        let hit = h
            .iter()
            .find(|x| x.kind == "exec-json-capture" && x.task == "crawl")
            .expect("capture hint");
        assert!(hit.advice.contains("capture: stdout"), "{hit:?}");
        assert!(hit.advice.contains("exit_code"), "{hit:?}");

        let intentional = hints_of(
            "nika: v1\nworkflow: w\npermits: { exec: true }\ntasks:\n  - id: probe\n    exec:\n      command: [\"false\"]\n      capture: structured\n    output:\n      exit_code: \".exit_code\"\n",
        );
        assert!(
            !intentional.iter().any(|x| x.kind == "exec-json-capture"),
            "{intentional:?}"
        );

        // The MIXED task — one binding parses stdout JSON, ANOTHER branches on
        // exit_code. `structured` is the point (switching would break `ok`);
        // the hint must stay silent (Gate-11 review: the any-vs-all misfire).
        let mixed = hints_of(
            "nika: v1\nworkflow: w\npermits: { exec: true }\ntasks:\n  - id: health\n    exec:\n      command: [\"curl\", \"-s\", \"https://api.example/health\"]\n      capture: structured\n    output:\n      body: \".stdout | fromjson\"\n      ok: \".exit_code == 0\"\n",
        );
        assert!(
            !mixed.iter().any(|x| x.kind == "exec-json-capture"),
            "{mixed:?}"
        );

        // Substring lookalike — the binding CONTAINS both `.stdout` and
        // `fromjson` (the old independent-substring predicate fired) but they
        // never form the `.stdout | fromjson` chain; no hint.
        let lookalike = hints_of(
            "nika: v1\nworkflow: w\npermits: { exec: true }\ntasks:\n  - id: diag\n    exec:\n      command: [\"node\", \"diag.mjs\"]\n      capture: structured\n    output:\n      log: \".raw | fromjson | .stdout_field\"\n",
        );
        assert!(
            !lookalike.iter().any(|x| x.kind == "exec-json-capture"),
            "{lookalike:?}"
        );
    }

    #[test]
    fn consumption_inside_invoke_args_json_counts() {
        // the output is consumed INSIDE an invoke args JSON value — the
        // visit_json walker path; with it blinded, a phantom dead-spend
        // hint would fire here.
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\n  - id: b\n    depends_on: [a]\n    invoke: { tool: \"nika:write\", args: { path: \"./o\", content: \"${{ tasks.a.output }}\" } }\n",
        );
        assert!(
            !h.iter().any(|x| x.kind == "dead-spend"),
            "consumed via args JSON: {h:?}"
        );
    }

    #[test]
    fn pure_compute_workflow_needs_no_boundary() {
        // infer-only → no permits hint (nothing to bound).
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(!h.iter().any(|x| x.kind == "permits"), "{h:?}");
    }

    #[test]
    fn open_object_schema_gets_the_strictness_hint() {
        // properties declared but additionalProperties unclosed → the
        // model can emit undeclared keys → shape varies across providers.
        let open = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        properties:\n          s: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(
            open.iter().any(|h| h.kind == "strictness" && h.task == "a"),
            "{open:?}"
        );
        // closed at every object node → no hint
        let closed = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          s: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(!closed.iter().any(|h| h.kind == "strictness"), "{closed:?}");
    }

    #[test]
    fn nested_open_object_is_found_one_hint_per_task() {
        // the root is closed but a nested items-object is open — still
        // hinted, and only ONCE for the task.
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          tags:\n            type: array\n            items:\n              type: object\n              properties:\n                name: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert_eq!(
            h.iter().filter(|x| x.kind == "strictness").count(),
            1,
            "{h:?}"
        );
    }

    // ─── schema-portability hint · grammar-blind keywords ─────────────

    #[test]
    fn grammar_blind_keywords_get_the_portability_hint() {
        // uniqueItems:true + not — every provider wire ACCEPTS this
        // schema and no grammar enforces either keyword (llama.cpp +
        // ollama proven live 2026-07-07); the hint names the local-
        // validation-only reality, once per task, listing both.
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          tags:\n            type: array\n            uniqueItems: true\n            items:\n              type: string\n              not: { enum: [forbidden] }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        let hit = h
            .iter()
            .find(|x| x.kind == "schema-portability")
            .expect("hint");
        assert_eq!(hit.task, "a");
        assert!(
            hit.advice.contains("`uniqueItems`") && hit.advice.contains("`not`"),
            "{hit:?}"
        );
        assert_eq!(
            h.iter().filter(|x| x.kind == "schema-portability").count(),
            1,
            "one hint per task: {h:?}"
        );
    }

    #[test]
    fn conditional_family_flags_only_when_it_binds() {
        // `if` + `then` binds → hinted; a bare `if` without then/else
        // constrains nothing anywhere — not even locally — so no claim.
        let bound = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          x: { type: string }\n        if:\n          properties:\n            x: { const: a }\n        then:\n          required: [x]\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(
            bound
                .iter()
                .any(|x| x.kind == "schema-portability" && x.advice.contains("`if/then/else`")),
            "{bound:?}"
        );
        let bare = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          x: { type: string }\n        if:\n          required: [x]\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(
            !bare.iter().any(|x| x.kind == "schema-portability"),
            "{bare:?}"
        );
    }

    #[test]
    fn portability_hint_reads_keywords_not_property_names() {
        // a property NAMED `not` + `uniqueItems: false` (the default,
        // binds nothing) → silence; the walker reads keys only at
        // schema-node positions.
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          not: { type: string }\n          tags:\n            type: array\n            uniqueItems: false\n            items: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(!h.iter().any(|x| x.kind == "schema-portability"), "{h:?}");
    }

    #[test]
    fn schema_d_task_gets_no_typing_hint() {
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        properties:\n          field: { type: string }\n  - id: b\n    depends_on: [a]\n    exec: { shell: \"echo ${{ tasks.a.output.field }}\" }\n",
        );
        assert!(!h.iter().any(|x| x.kind == "typing"), "{h:?}");
    }

    #[test]
    fn retried_exec_warns_at_least_once_semantics() {
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: deploy\n    retry: { max_attempts: 3 }\n    exec: { shell: \"./deploy.sh\" }\n",
        );
        let hit = h.iter().find(|x| x.kind == "retry-effects").expect("hint");
        assert_eq!(hit.task, "deploy");
        assert!(hit.advice.contains("at-least-once"), "{hit:?}");
    }

    #[test]
    fn retried_mcp_tool_warns_no_idempotency_contract() {
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: post\n    retry: { max_attempts: 2 }\n    invoke:\n      tool: mcp:slack/send\n      args: { text: \"hi\" }\n",
        );
        let hit = h.iter().find(|x| x.kind == "retry-effects").expect("hint");
        assert!(hit.advice.contains("mcp:slack/send"), "{hit:?}");
    }

    #[test]
    fn retry_on_contracted_effects_makes_no_claim() {
        // infer retries re-spend tokens (covered by cost) · nika:
        // builtins carry documented idempotent semantics · max_attempts
        // 1 is no retry at all — none of these hint.
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: ask\n    retry: { max_attempts: 3 }\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n  - id: save\n    retry: { max_attempts: 3 }\n    depends_on: [ask]\n    invoke:\n      tool: nika:write\n      args: { path: out.md, content: \"${{ tasks.ask.output }}\" }\n  - id: once\n    retry: { max_attempts: 1 }\n    depends_on: [save]\n    exec: { shell: \"true\" }\n",
        );
        assert!(!h.iter().any(|x| x.kind == "retry-effects"), "{h:?}");
    }

    // ─── secrets-store hint pipeline ───────────────────────────────────
    // push_unresolvable_secret_hints → referenced_secrets → task_text_fields
    //   → collect_json_strings_into. These functions are exercised both
    //   behaviorally (through scan_hints) and as units below.

    fn wf_of(yaml: &str) -> RawWorkflow {
        parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse")
    }

    #[test]
    fn referenced_vault_secret_gets_the_secrets_store_hint() {
        // a vault-source secret that IS referenced via `${{ secrets.FOO }}`
        // in a task field → push_unresolvable_secret_hints must emit a
        // `secrets-store` hint naming FOO. Kills:
        //   - push_unresolvable_secret_hints → () (no hint at all)
        //   - referenced_secrets → {} / {""} / {"xyzzy"} (FOO not in set →
        //     the `referenced.contains(name)` guard fails → no hint)
        let h = hints_of(
            "nika: v1\nworkflow: w\nsecrets:\n  FOO:\n    source: vault\n    key: prod/foo\ntasks:\n  - id: t\n    exec: { shell: \"echo ${{ secrets.FOO }}\" }\n",
        );
        let hit = h
            .iter()
            .find(|x| x.kind == "secrets-store")
            .expect("secrets-store hint");
        assert_eq!(hit.task, "-");
        assert!(hit.advice.contains("secrets.FOO"), "{hit:?}");
    }

    #[test]
    fn unreferenced_vault_secret_gets_no_hint() {
        // declared-but-unused vault secret is harmless — the hint fires
        // ONLY for a referenced secret. If referenced_secrets returned a
        // spurious {"FOO"} (the from_iter(["xyzzy"]) family with a
        // matching name would not, but a hardcoded set could) this would
        // also catch over-collection.
        let h = hints_of(
            "nika: v1\nworkflow: w\nsecrets:\n  FOO:\n    source: vault\n    key: prod/foo\ntasks:\n  - id: t\n    exec: { shell: \"echo hi\" }\n",
        );
        assert!(!h.iter().any(|x| x.kind == "secrets-store"), "{h:?}");
    }

    #[test]
    fn referenced_secrets_collects_exactly_the_referenced_names() {
        // Direct unit on referenced_secrets — FOO referenced in a prompt,
        // BAR referenced in an output, BAZ declared but never referenced.
        // The returned set must be exactly {FOO, BAR}. Kills the
        // referenced_secrets → BTreeSet::new() / from_iter([""]) /
        // from_iter(["xyzzy"]) mutations precisely (wrong cardinality OR
        // wrong contents).
        let wf = wf_of(
            "nika: v1\nworkflow: w\nsecrets:\n  FOO:\n    source: vault\n    key: a\n  BAR:\n    source: vault\n    key: b\n  BAZ:\n    source: vault\n    key: c\ntasks:\n  - id: t\n    infer: { prompt: \"use ${{ secrets.FOO }}\", max_tokens: 10 }\noutputs:\n  r: ${{ secrets.BAR }}\n",
        );
        let refs = referenced_secrets(&wf);
        let got: Vec<&str> = refs.iter().map(String::as_str).collect();
        assert_eq!(got, vec!["BAR", "FOO"], "BTreeSet is sorted");
    }

    #[test]
    fn referenced_secrets_empty_when_none_referenced() {
        // No `${{ secrets.X }}` island anywhere → empty set. This is the
        // baseline the from_iter([""]) / from_iter(["xyzzy"]) mutations
        // violate (they would return a non-empty set here).
        let wf = wf_of(
            "nika: v1\nworkflow: w\nsecrets:\n  FOO:\n    source: vault\n    key: a\ntasks:\n  - id: t\n    exec: { shell: \"echo plain\" }\n",
        );
        assert!(referenced_secrets(&wf).is_empty());
    }

    #[test]
    fn secret_referenced_only_in_task_field_is_found() {
        // Isolates task_text_fields: the secret appears ONLY inside a task
        // field (the infer prompt), NEVER in outputs. If task_text_fields
        // returns vec![] / vec![""] / vec!["xyzzy"], the prompt island is
        // never scanned → FOO is absent → the secrets-store hint vanishes.
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\nsecrets:\n  FOO:\n    source: vault\n    key: a\ntasks:\n  - id: t\n    infer: { prompt: \"call with ${{ secrets.FOO }}\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.t.output }}\n",
        );
        assert!(
            h.iter()
                .any(|x| x.kind == "secrets-store" && x.advice.contains("secrets.FOO")),
            "{h:?}"
        );
    }

    #[test]
    fn task_text_fields_collects_every_action_text_surface() {
        // Direct unit on task_text_fields across the action variants +
        // `with:`. Kills task_text_fields → vec![] / vec![""] /
        // vec!["xyzzy"] (the real surfaces are none of those) and confirms
        // the exec/invoke/infer/agent + with arms each contribute.

        // exec: command + stdin + env values
        let exec = wf_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec:\n      shell: \"run CMD\"\n      stdin: \"STDIN\"\n      env: { K: \"ENVVAL\" }\n",
        );
        let f = task_text_fields(&exec.tasks[0].value);
        assert!(f.contains(&"run CMD"), "{f:?}");
        assert!(f.contains(&"STDIN"), "{f:?}");
        assert!(f.contains(&"ENVVAL"), "{f:?}");

        // infer: prompt + system
        let infer = wf_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: t\n    infer: { prompt: \"PROMPT\", system: \"SYSTEM\", max_tokens: 10 }\n",
        );
        let f = task_text_fields(&infer.tasks[0].value);
        assert!(f.contains(&"PROMPT") && f.contains(&"SYSTEM"), "{f:?}");

        // agent: prompt + system
        let agent = wf_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: t\n    agent: { prompt: \"APROMPT\", system: \"ASYSTEM\", max_tokens_total: 10 }\n",
        );
        let f = task_text_fields(&agent.tasks[0].value);
        assert!(f.contains(&"APROMPT") && f.contains(&"ASYSTEM"), "{f:?}");

        // invoke args JSON strings + with JSON strings
        let invoke = wf_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    with: { wkey: \"WITHVAL\" }\n    invoke: { tool: \"nika:write\", args: { path: \"ARGVAL\" } }\n",
        );
        let f = task_text_fields(&invoke.tasks[0].value);
        assert!(f.contains(&"ARGVAL"), "invoke args string: {f:?}");
        assert!(f.contains(&"WITHVAL"), "with value string: {f:?}");
    }

    #[test]
    fn collect_json_strings_into_gathers_all_nested_string_leaves() {
        // Direct unit on collect_json_strings_into. Feed
        // {"a":"x","b":["y",{"c":"z"}]} → ALL of x,y,z must be collected.
        //   - String arm deleted → top-level "x" (and nested) dropped
        //   - Array arm deleted → "y" and the object under it dropped
        //   - Object arm deleted → "z" (object inside array) + "x"/"y"
        //     (top object) dropped
        //   - whole fn → () → nothing collected
        let value = serde_json::json!({ "a": "x", "b": ["y", { "c": "z" }] });
        let mut out = Vec::new();
        collect_json_strings_into(&value, &mut out);
        out.sort_unstable();
        assert_eq!(out, vec!["x", "y", "z"], "all nested leaves: {out:?}");
    }

    #[test]
    fn collect_json_strings_into_array_arm_descends() {
        // Targeted at the Array match arm: a top-level array of strings.
        // Deleting the Array arm drops both leaves; the String/Object arms
        // alone cannot reach them.
        let value = serde_json::json!(["one", "two"]);
        let mut out = Vec::new();
        collect_json_strings_into(&value, &mut out);
        out.sort_unstable();
        assert_eq!(out, vec!["one", "two"], "{out:?}");
    }

    #[test]
    fn collect_json_strings_into_object_arm_descends() {
        // Targeted at the Object match arm: a flat object. Deleting the
        // Object arm drops the leaf entirely.
        let value = serde_json::json!({ "k": "deep" });
        let mut out = Vec::new();
        collect_json_strings_into(&value, &mut out);
        assert_eq!(out, vec!["deep"], "{out:?}");
    }

    #[test]
    fn collect_json_strings_into_string_arm_pushes_the_leaf() {
        // Targeted at the String match arm: a bare string value. Deleting
        // the String arm drops it; the `_ => {}` catch-all would swallow it.
        let value = serde_json::json!("bare");
        let mut out = Vec::new();
        collect_json_strings_into(&value, &mut out);
        assert_eq!(out, vec!["bare"], "{out:?}");
    }

    #[test]
    fn collect_json_strings_into_ignores_non_string_scalars() {
        // numbers/bools/null contribute nothing (the `_ => {}` arm). This
        // pins the boundary the deleted-arm mutants must not cross.
        let value = serde_json::json!({ "n": 1, "b": true, "z": null, "s": "keep" });
        let mut out = Vec::new();
        collect_json_strings_into(&value, &mut out);
        assert_eq!(out, vec!["keep"], "{out:?}");
    }

    #[test]
    fn secret_referenced_inside_invoke_args_json_is_found() {
        // End-to-end: a secret reachable ONLY through the invoke-args JSON
        // walk (collect_json_strings_into via task_text_fields). With any
        // of the collect arms blinded, FOO is never seen → no hint.
        let h = hints_of(
            "nika: v1\nworkflow: w\nsecrets:\n  FOO:\n    source: vault\n    key: a\ntasks:\n  - id: t\n    invoke: { tool: \"nika:write\", args: { path: \"./o\", content: \"${{ secrets.FOO }}\" } }\n",
        );
        assert!(
            h.iter()
                .any(|x| x.kind == "secrets-store" && x.advice.contains("secrets.FOO")),
            "{h:?}"
        );
    }

    #[test]
    fn unwrapped_output_ref_is_hinted_wrapped_is_silent() {
        // Output gauntlet (2026-07-11): a bare `tasks.X.output…` output
        // value is the LITERAL STRING (the run returns the path text, not
        // the value) — hint the wrap. The pattern is distinctive across
        // the five reference namespaces.
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: mock/echo\ntasks:\n  - id: data\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: { count: 42 } } }\noutputs:\n  just_count: tasks.data.output.count\n",
        );
        let hit = h
            .iter()
            .find(|x| x.kind == "unwrapped-ref")
            .unwrap_or_else(|| panic!("expected unwrapped-ref: {h:?}"));
        assert_eq!(hit.task, "just_count");
        assert!(
            hit.advice.contains("literal string")
                && hit.advice.contains("${{ tasks.data.output.count }}"),
            "{}",
            hit.advice
        );

        // A properly wrapped output is SILENT (the common correct case).
        let wrapped = hints_of(
            "nika: v1\nworkflow: w\nmodel: mock/echo\ntasks:\n  - id: data\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: { count: 42 } } }\noutputs:\n  just_count: ${{ tasks.data.output.count }}\n",
        );
        assert!(
            !wrapped.iter().any(|x| x.kind == "unwrapped-ref"),
            "{wrapped:?}"
        );

        // A genuine string constant that is NOT a namespace path is silent.
        let plain = hints_of(
            "nika: v1\nworkflow: w\nmodel: mock/echo\ntasks:\n  - id: data\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: {} } }\noutputs:\n  label: production\n",
        );
        assert!(
            !plain.iter().any(|x| x.kind == "unwrapped-ref"),
            "{plain:?}"
        );
    }
}
