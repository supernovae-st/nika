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
//!   undeclared keys: close it (`additionalProperties: false`) and the
//!   structured-output shape is deterministic across providers.
//! - **redundant success-gate** (`redundant-gate`) — `when: ${{
//!   tasks.D.status == 'success' }}` where `D` is a dep that can never
//!   be `skipped`: the spec names this the discouraged restatement of
//!   the default gate (spec 03 §the gate).
//! - **retry on uncontracted effects** (`retry-effects`) — see
//!   [`push_retry_effects_hint`].
//! - **concurrent same-path writers** (`parallel-writers`) — emitted by
//!   the DAG analysis pass (`check/analysis.rs`) and merged here.

use std::collections::BTreeSet;

use crate::expression::{Expr, Literal, RelOp, scan_templates, task_output_paths};
use crate::raw::{RawAction, RawWorkflow};
use crate::types::OnErrorAction;

/// One advisory improvement with its concrete unlock.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct Hint {
    /// The hint class — the closed set today: `cost` · `dead-spend` ·
    /// `typing` · `permits` · `strictness` · `redundant-gate` ·
    /// `retry-effects` · `parallel-writers` · `secrets-store` (additive ·
    /// agents route on it; the module doc describes each).
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
    let mut hints = Vec::new();

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
                if !consumed.contains(id) {
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
            }
            RawAction::Agent(a) => {
                if a.max_tokens_total.is_none() {
                    hints.push(hint("cost", id, format!(
                        "declare `max_tokens_total` on `{id}` — the agent loop gets a hard budget instead of UNBOUNDED"
                    )));
                }
                push_strictness_hint(&mut hints, id, a.schema.as_ref().map(|s| &s.value));
                any_effect = true; // an agent dispatches tools
            }
            RawAction::Exec(_) | RawAction::Invoke(_) => any_effect = true,
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
    hints
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

/// Whether any object node in the schema declares `properties` without
/// closing `additionalProperties`. Descends the same composite shapes
/// the lint walks (`properties` · `items` · `anyOf`/`oneOf`/`allOf`);
/// `$ref` is opaque (no claim).
fn has_open_object(node: &serde_json::Value) -> bool {
    let Some(obj) = node.as_object() else {
        return false;
    };
    if obj.contains_key("$ref") {
        return false;
    }
    if let Some(props) = obj.get("properties").and_then(serde_json::Value::as_object) {
        if obj.get("additionalProperties") != Some(&serde_json::Value::Bool(false)) {
            return true;
        }
        if props.values().any(has_open_object) {
            return true;
        }
    }
    if obj.get("items").is_some_and(has_open_object) {
        return true;
    }
    ["anyOf", "oneOf", "allOf"].iter().any(|key| {
        obj.get(*key)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|branches| branches.iter().any(has_open_object))
    })
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
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    exec: { command: \"true\" }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'success' }}\n    exec: { command: \"true\" }\n",
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
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\nvars: { go: \"y\" }\ntasks:\n  - id: root\n    exec: { command: \"true\" }\n  - id: a\n    depends_on: [root]\n    when: ${{ vars.go == 'y' }}\n    exec: { command: \"true\" }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'success' }}\n    exec: { command: \"true\" }\n",
        );
        assert!(
            !gated.iter().any(|x| x.kind == "redundant-gate"),
            "{gated:?}"
        );
        let skip_route = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    exec: { command: \"true\" }\n    on_error: { skip: true }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'success' }}\n    exec: { command: \"true\" }\n",
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
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\nvars: { env: \"p\" }\ntasks:\n  - id: a\n    exec: { command: \"true\" }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'success' && vars.env == 'p' }}\n    exec: { command: \"true\" }\n",
        );
        assert!(
            !compound.iter().any(|x| x.kind == "redundant-gate"),
            "{compound:?}"
        );
        let reversed = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    exec: { command: \"true\" }\n  - id: b\n    depends_on: [a]\n    when: ${{ 'success' == tasks.a.status }}\n    exec: { command: \"true\" }\n",
        );
        assert!(
            reversed.iter().any(|x| x.kind == "redundant-gate"),
            "{reversed:?}"
        );
        let failure = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    exec: { command: \"true\" }\n  - id: b\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'failure' }}\n    exec: { command: \"true\" }\n",
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
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\n  - id: b\n    exec: { command: \"echo done\" }\n",
        );
        assert!(h.iter().any(|x| x.kind == "dead-spend" && x.task == "a"));
        // consumed via outputs: → no dead-spend hint
        let h2 = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(!h2.iter().any(|x| x.kind == "dead-spend"), "{h2:?}");
    }

    #[test]
    fn deeply_referenced_unschema_d_output_gets_a_typing_hint() {
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"echo ${{ tasks.a.output.field }}\" }\n",
        );
        assert!(
            h.iter().any(|x| x.kind == "typing" && x.task == "a"),
            "{h:?}"
        );
        // shallow consumption only → no typing hint
        let h2 = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"echo ${{ tasks.a.output }}\" }\n",
        );
        assert!(!h2.iter().any(|x| x.kind == "typing"), "{h2:?}");
    }

    #[test]
    fn effectful_workflow_without_permits_gets_the_boundary_hint() {
        let h = hints_of(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { command: \"echo hi\" }\n",
        );
        assert!(h.iter().any(|x| x.kind == "permits"), "{h:?}");
        // boundary declared → no hint
        let h2 = hints_of(
            "nika: v1\nworkflow: w\npermits: { exec: true }\ntasks:\n  - id: t\n    exec: { command: \"echo hi\" }\n",
        );
        assert!(!h2.iter().any(|x| x.kind == "permits"), "{h2:?}");
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

    #[test]
    fn schema_d_task_gets_no_typing_hint() {
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        properties:\n          field: { type: string }\n  - id: b\n    depends_on: [a]\n    exec: { command: \"echo ${{ tasks.a.output.field }}\" }\n",
        );
        assert!(!h.iter().any(|x| x.kind == "typing"), "{h:?}");
    }

    #[test]
    fn retried_exec_warns_at_least_once_semantics() {
        let h = hints_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: deploy\n    retry: { max_attempts: 3 }\n    exec: { command: \"./deploy.sh\" }\n",
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
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: ask\n    retry: { max_attempts: 3 }\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n  - id: save\n    retry: { max_attempts: 3 }\n    depends_on: [ask]\n    invoke:\n      tool: nika:write\n      args: { path: out.md, content: \"${{ tasks.ask.output }}\" }\n  - id: once\n    retry: { max_attempts: 1 }\n    depends_on: [save]\n    exec: { command: \"true\" }\n",
        );
        assert!(!h.iter().any(|x| x.kind == "retry-effects"), "{h:?}");
    }
}
