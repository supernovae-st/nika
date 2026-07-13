// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow analysis — the Core conformance rules over a [`RawWorkflow`].
//!
//! The analyzer COLLECTS every error (not fail-fast) so an author sees
//! the full diagnosis in one pass ·
//!
//! - envelope presence · `nika:` + `workflow:` + non-empty `tasks:`
//!   (spec `01-envelope.md` · « That's the **whole minimum** »)
//! - duplicate task ids
//! - `NIKA-DAG-002` · `depends_on` entries resolve
//! - `NIKA-DAG-001` · cycle detection (incl. self-dependency)
//! - `NIKA-DAG-003` · `tasks.<id>` refs require the explicit edge
//! - `NIKA-VAR-001` class · namespace-ref resolution (5 namespaces +
//!   `item`/`index` loop-locals · spec `04-variables.md`)
//! - `NIKA-PARSE-WHEN-001` · `when:` boolean shape
//! - `output:` binding rules · reserved names + pure-jq
//! - topological waves (spec `03-dag.md` execution model)

mod builtin_shape;
mod dag;
mod jq_lint;
mod scan;
mod schema_lint;
mod schema_paths;

use std::collections::BTreeMap;

use crate::error::SchemaError;
use crate::raw::RawWorkflow;

/// The analyzer's output — minimal · lowering is the runtime's job.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AnalyzedWorkflow {
    /// Topological execution waves · `topo_waves[n]` holds indices into
    /// `wf.tasks` that may run in parallel once wave `n-1` completed.
    pub topo_waves: Vec<Vec<usize>>,
}

impl AnalyzedWorkflow {
    /// Create an analyzed workflow.
    #[must_use]
    pub fn new(topo_waves: Vec<Vec<usize>>) -> Self {
        Self { topo_waves }
    }
}

/// Analyze a parsed workflow against the Core conformance rules.
///
/// # Errors
///
/// Returns ALL rule violations found (never fail-fast) — an empty
/// error list is impossible (`Err` ⟺ at least one violation).
pub fn analyze(wf: &RawWorkflow) -> Result<AnalyzedWorkflow, Vec<SchemaError>> {
    let mut errors = Vec::new();

    check_envelope(wf, &mut errors);
    let ids = task_id_index(wf, &mut errors);
    dag::check_depends_on_resolve(&wf.tasks, &ids, &mut errors);
    dag::check_cycles(&wf.tasks, &ids, &mut errors);
    dag::check_recover_acyclic(&wf.tasks, &ids, &mut errors);
    builtin_shape::check_builtin_shapes(&wf.tasks, &mut errors);
    scan::scan_workflow(wf, &mut errors);
    jq_lint::scan_jq(wf, &mut errors);
    schema_lint::scan_schemas(wf, &mut errors);

    if errors.is_empty() {
        Ok(AnalyzedWorkflow::new(dag::topo_waves(&wf.tasks, &ids)))
    } else {
        Err(errors)
    }
}

/// Envelope presence (spec `01-envelope.md` · « Two required lines
/// (`nika:` + `workflow:`) and a non-empty `tasks:` »).
fn check_envelope(wf: &RawWorkflow, errors: &mut Vec<SchemaError>) {
    if wf.nika.is_none() {
        errors.push(SchemaError::MissingEnvelopeField {
            field: "nika".to_owned(),
            span: None,
        });
    }
    if wf.workflow.is_none() {
        errors.push(SchemaError::MissingEnvelopeField {
            field: "workflow".to_owned(),
            span: None,
        });
    }
    if wf.tasks.is_empty() {
        errors.push(SchemaError::MissingEnvelopeField {
            field: "tasks".to_owned(),
            span: None,
        });
    }
}

/// Build the id → index map · report duplicates (first declaration
/// wins for downstream rules).
fn task_id_index(wf: &RawWorkflow, errors: &mut Vec<SchemaError>) -> BTreeMap<String, usize> {
    let mut ids = BTreeMap::new();
    for (i, task) in wf.tasks.iter().enumerate() {
        let id = &task.value.id;
        if ids.contains_key(&id.value) {
            errors.push(SchemaError::DuplicateTaskId {
                id: id.value.clone(),
                span: Some(id.span),
            });
        } else {
            ids.insert(id.value.clone(), i);
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn analyze_yaml(yaml: &str) -> Result<AnalyzedWorkflow, Vec<SchemaError>> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        analyze(&wf)
    }

    fn assert_has<F: Fn(&SchemaError) -> bool>(errors: &[SchemaError], pred: F, what: &str) {
        assert!(errors.iter().any(pred), "expected {what} in {errors:?}");
    }

    const MINIMAL_OK: &str = "\
nika: v1
workflow: hello
tasks:
  - id: greet
    infer:
      prompt: \"Say hi\"
";

    #[test]
    fn minimal_valid_analyzes() {
        // Conformance fixture envelope/001-valid-minimal.
        let analyzed = analyze_yaml(MINIMAL_OK).expect("valid");
        assert_eq!(analyzed.topo_waves, vec![vec![0]]);
    }

    // ── Envelope rules ──────────────────────────────────────────────

    #[test]
    fn missing_nika_workflow_and_tasks_all_collected() {
        // COLLECT all errors · not fail-fast.
        let errors = analyze_yaml("description: empty\n").expect_err("3 missing");
        assert_eq!(errors.len(), 3, "{errors:?}");
        for field in ["nika", "workflow", "tasks"] {
            assert_has(
                &errors,
                |e| matches!(e, SchemaError::MissingEnvelopeField { field: f, .. } if f == field),
                field,
            );
        }
    }

    #[test]
    fn missing_workflow_only() {
        // Conformance fixture envelope/007-missing-workflow.
        let yaml = "\
nika: v1
tasks:
  - id: greet
    infer: { prompt: hi }
";
        let errors = analyze_yaml(yaml).expect_err("missing workflow");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::MissingEnvelopeField { field, .. } if field == "workflow"),
            "missing workflow",
        );
    }

    #[test]
    fn empty_tasks_array_errors() {
        // Conformance fixture envelope/006-empty-tasks-array.
        let yaml = "nika: v1\nworkflow: hello\ntasks: []\n";
        let errors = analyze_yaml(yaml).expect_err("empty tasks");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::MissingEnvelopeField { field, .. } if field == "tasks"),
            "empty tasks",
        );
    }

    #[test]
    fn duplicate_task_id_errors() {
        // Conformance fixture envelope/011-duplicate-task-id.
        let yaml = "\
nika: v1
workflow: dup
tasks:
  - id: same
    exec: { command: [echo] }
  - id: same
    exec: { command: [echo] }
";
        let errors = analyze_yaml(yaml).expect_err("dup id");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::DuplicateTaskId { id, .. } if id == "same"),
            "duplicate id",
        );
    }

    // ── NIKA-DAG-003 · refs require the edge ────────────────────────

    #[test]
    fn when_ref_without_edge_errors() {
        // Conformance fixture dag-topology/003.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: test
    exec: { command: [\"./test.sh\"] }
  - id: deploy
    when: ${{ tasks.test.status == 'success' }}
    exec: { command: [\"./deploy.sh\"] }
";
        let errors = analyze_yaml(yaml).expect_err("no edge");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::MissingDependsOnEdge { referenced, .. } if referenced == "test"),
            "DAG-003",
        );
    }

    #[test]
    fn with_ref_without_edge_errors() {
        // Conformance fixture dag-topology/005.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: research
    infer: { prompt: \"research\" }
  - id: summarize
    with:
      content: ${{ tasks.research.output }}
    infer: { prompt: \"summarize ${{ with.content }}\" }
";
        let errors = analyze_yaml(yaml).expect_err("no edge");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::MissingDependsOnEdge { referenced, .. } if referenced == "research"),
            "DAG-003 via with",
        );
    }

    #[test]
    fn verb_body_ref_without_edge_errors() {
        // Conformance fixture dag-topology/006.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: research
    infer: { prompt: \"research\" }
  - id: brief
    infer:
      prompt: \"Brief from ${{ tasks.research.output }}\"
";
        let errors = analyze_yaml(yaml).expect_err("no edge");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::MissingDependsOnEdge { referenced, .. } if referenced == "research"),
            "DAG-003 via prompt",
        );
    }

    #[test]
    fn for_each_ref_without_edge_errors() {
        // Conformance fixture dag-topology/007.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: discover
    invoke: { tool: \"nika:read\" }
  - id: process
    for_each: ${{ tasks.discover.output }}
    exec: { command: [echo] }
";
        let errors = analyze_yaml(yaml).expect_err("no edge");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::MissingDependsOnEdge { referenced, .. } if referenced == "discover"),
            "DAG-003 via for_each",
        );
    }

    #[test]
    fn dag_003_message_is_not_double_backticked() {
        // The DAG-003 #[error] template wraps the task id (`task `{task}``);
        // the scan used to pass the already-wrapped `task `x`` LOCATION into
        // the id field, rendering `task `task `x```. The id field now carries
        // the BARE id — the message reads `task `b` references …` cleanly.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: a
    exec: { command: [\"echo\", \"hi\"] }
  - id: b
    exec: { command: [\"echo\", \"${{ tasks.a.output }}\"] }
";
        let errors = analyze_yaml(yaml).expect_err("no edge");
        let rendered = errors
            .iter()
            .find(|e| matches!(e, SchemaError::MissingDependsOnEdge { .. }))
            .map(std::string::ToString::to_string)
            .expect("a DAG-003 finding");
        assert!(
            rendered.contains("task `b` references"),
            "the id renders once, cleanly: {rendered}"
        );
        assert!(
            !rendered.contains("task `task"),
            "no double-backtick wrap: {rendered}"
        );
    }

    #[test]
    fn loop_local_outside_for_each_message_is_not_double_backticked() {
        // Same double-backtick class for NIKA-VAR-001's loop-local error:
        // `loop-local `item` used in task `a` which has no `for_each:``,
        // never `task `task `a```.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: a
    exec: { command: [\"echo\", \"${{ item }}\"] }
";
        let errors = analyze_yaml(yaml).expect_err("loop-local out of scope");
        let rendered = errors
            .iter()
            .find(|e| matches!(e, SchemaError::LoopLocalOutsideForEach { .. }))
            .map(std::string::ToString::to_string)
            .expect("a loop-local finding");
        assert!(
            rendered.contains("in task `a` which has no"),
            "the id renders once, cleanly: {rendered}"
        );
        assert!(
            !rendered.contains("task `task"),
            "no double-backtick wrap: {rendered}"
        );
    }

    #[test]
    fn paired_ref_with_edge_is_valid() {
        // Conformance fixture dag-topology/009.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: fetch
    invoke: { tool: \"nika:fetch\", args: { url: \"https://x.io\" } }
  - id: use
    depends_on: [fetch]
    with:
      data: ${{ tasks.fetch.output }}
    infer: { prompt: \"use ${{ with.data }}\" }
";
        analyze_yaml(yaml).expect("valid");
    }

    #[test]
    fn on_finally_owner_ref_needs_no_edge() {
        // Spec example 16 · on_finally references the OWNING task's
        // status — no self-edge required.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: test
    exec: { command: [\"cargo\", \"test\"] }
    on_finally:
      - invoke:
          tool: nika:emit
          args:
            status: \"${{ tasks.test.status }}\"
";
        analyze_yaml(yaml).expect("valid");
    }

    #[test]
    fn on_error_recover_ref_needs_no_edge() {
        // Spec example 22 · recover references the fallback task with
        // NO depends_on edge.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: cached
    invoke: { tool: \"nika:read\" }
  - id: fetch_article
    invoke: { tool: \"nika:fetch\", args: { url: \"https://x.test\" } }
    on_error:
      recover: ${{ tasks.cached.output }}
";
        analyze_yaml(yaml).expect("valid");
    }

    // ── NIKA-VAR-001 class ──────────────────────────────────────────

    #[test]
    fn vars_undeclared_errors() {
        // Conformance fixture variables/003-vars-undeclared.
        let yaml = "\
nika: v1
workflow: t
vars:
  topic: \"ai\"
tasks:
  - id: go
    infer: { prompt: \"about ${{ vars.topik }}\" }
";
        let errors = analyze_yaml(yaml).expect_err("typo'd var");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, .. } if reference == "vars.topik"),
            "vars.topik unresolved",
        );
    }

    #[test]
    fn envelope_model_unresolved_var_is_flagged() {
        // deep/019 — the oracle false-green class: an envelope
        // `model: "${{ vars.nope }}"` was ACCEPTED and died at dispatch.
        let yaml = "nika: v1\nworkflow: w\nmodel: \"${{ vars.nope }}\"\ntasks:\n  - id: a\n    infer: { prompt: hi }\n";
        let errors = analyze_yaml(yaml).expect_err("envelope model ref must flag");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, .. } if reference == "vars.nope"),
            "vars.nope unresolved at the envelope",
        );
    }

    #[test]
    fn with_undeclared_errors() {
        // Conformance fixture variables/004-with-undeclared.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: go
    with:
      present: \"x\"
    infer: { prompt: \"${{ with.missing }}\" }
";
        let errors = analyze_yaml(yaml).expect_err("missing with");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, .. } if reference == "with.missing"),
            "with.missing unresolved",
        );
    }

    #[test]
    fn env_and_secrets_undeclared_error() {
        // Conformance fixtures variables/005 + 006.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: go
    exec:
      command: [\"echo\", \"${{ env.MISSING }}\", \"${{ secrets.api_key }}\"]
";
        let errors = analyze_yaml(yaml).expect_err("undeclared");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, .. } if reference == "env.MISSING"),
            "env.MISSING",
        );
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, .. } if reference == "secrets.api_key"),
            "secrets.api_key",
        );
    }

    #[test]
    fn unknown_namespace_root_errors() {
        // Conformance fixture variables/002-undefined-namespace ·
        // « Five namespaces. That's it. »
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: go
    infer: { prompt: \"${{ foo.bar }}\" }
";
        let errors = analyze_yaml(yaml).expect_err("foo namespace");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, .. } if reference == "foo"),
            "unknown root",
        );
    }

    #[test]
    fn outputs_reference_missing_task_errors() {
        // Conformance fixture variables/001-outputs-reference-missing-task.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: real
    exec: { command: [echo] }
outputs:
  result: ${{ tasks.ghost.output }}
";
        let errors = analyze_yaml(yaml).expect_err("ghost task");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, location, .. } if reference == "tasks.ghost" && location == "outputs"),
            "outputs ghost ref",
        );
    }

    #[test]
    fn item_outside_for_each_errors() {
        // Conformance fixture variables/007-item-outside-for-each.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: process
    infer: { prompt: \"handle ${{ item }}\" }
";
        let errors = analyze_yaml(yaml).expect_err("item outside loop");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::LoopLocalOutsideForEach { local, .. } if local == "item"),
            "loop-local item",
        );
    }

    #[test]
    fn item_index_inside_for_each_valid() {
        // Conformance fixture variables/008 + spec example 26.
        let yaml = "\
nika: v1
workflow: t
vars:
  locales: [\"fr\", \"es\"]
tasks:
  - id: translate
    for_each: ${{ vars.locales }}
    with:
      locale: ${{ item }}
      n: ${{ index }}
    infer: { prompt: \"to ${{ with.locale }} (#${{ with.n }})\" }
";
        analyze_yaml(yaml).expect("valid");
    }

    #[test]
    fn escaped_literal_needs_no_declaration() {
        // Conformance fixture variables/010-valid-escaped-literal.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: doc
    infer:
      prompt: \"The syntax \\\\${{ vars.x }} references variables\"
";
        analyze_yaml(yaml).expect("escaped island is literal text");
    }

    #[test]
    fn unclosed_expression_errors() {
        // Conformance fixture variables/011-unclosed-expression.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: go
    infer:
      prompt: \"broken ${{ vars.x \"
";
        let errors = analyze_yaml(yaml).expect_err("unclosed");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::TemplateSyntax { .. }),
            "template syntax",
        );
    }

    #[test]
    fn closed_island_grammar_error_is_expression_violation_not_template_syntax() {
        // A CLOSED `${{ }}` whose CEL is outside cel-subset/0.1 is the
        // NIKA-VAR-005 « static expression violation » class — NOT the
        // NIKA-VAR-008 « unclosed `${{` opener » that an UNTERMINATED island
        // gets. Conflating them made `nika explain` mis-teach the error.
        for expr in [
            "${{ vars.a < vars.b < vars.c }}", // chained relation
            "${{ vars.s.matches('h.*o') }}",   // unknown function
            "${{ vars.a + vars.b }}",          // arithmetic (outside the subset)
        ] {
            let yaml = format!(
                "nika: v1\nworkflow: t\ntasks:\n  - id: go\n    when: {expr}\n    exec: {{ command: [\"echo\", \"hi\"] }}\n"
            );
            let errors = analyze_yaml(&yaml).expect_err("grammar error");
            assert!(
                errors
                    .iter()
                    .any(|e| matches!(e, SchemaError::ExpressionViolation { .. })),
                "`{expr}` must be ExpressionViolation (VAR-005), got {errors:?}"
            );
            assert!(
                !errors
                    .iter()
                    .any(|e| matches!(e, SchemaError::TemplateSyntax { .. })),
                "`{expr}` must NOT be TemplateSyntax (VAR-008 is unclosed-only): {errors:?}"
            );
        }
    }

    #[test]
    fn task_record_field_and_binding_resolution() {
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: api
    invoke: { tool: \"nika:fetch\", args: { url: \"https://x.test\" } }
    output:
      user_count: \".data.users | length\"
  - id: report
    depends_on: [api]
    infer:
      prompt: \"count ${{ tasks.api.user_count }} status ${{ tasks.api.status }}\"
";
        analyze_yaml(yaml).expect("record field + declared binding valid");
    }

    #[test]
    fn unknown_task_field_errors() {
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: api
    invoke: { tool: \"nika:fetch\" }
  - id: report
    depends_on: [api]
    infer:
      prompt: \"${{ tasks.api.nonexistent_field }}\"
";
        let errors = analyze_yaml(yaml).expect_err("bad field");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnknownTaskField { field, .. } if field == "nonexistent_field"),
            "unknown task field",
        );
    }

    // ── output: binding rules ───────────────────────────────────────

    #[test]
    fn reserved_binding_name_errors() {
        // Conformance fixture variables/009-output-binding-reserved-name.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: api
    invoke: { tool: \"nika:fetch\" }
    output:
      status: \".data.status\"
";
        let errors = analyze_yaml(yaml).expect_err("reserved");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::ReservedBindingName { name, .. } if name == "status"),
            "reserved binding",
        );
    }

    #[test]
    fn jq_binding_with_template_errors() {
        // Spec 04 §binding rules · « the two expression layers never
        // nest ».
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: api
    invoke: { tool: \"nika:fetch\" }
    output:
      field: \".data | ${{ vars.x }}\"
";
        let errors = analyze_yaml(yaml).expect_err("template in jq");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::JqBindingContainsTemplate { name, .. } if name == "field"),
            "jq binding template",
        );
    }

    // ── when: shape rules ───────────────────────────────────────────

    #[test]
    fn when_literal_string_errors() {
        // Spec 03 §when invalid · « when: "literal string" ❌ not a
        // ${{ }} expression ».
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: go
    when: \"literal string\"
    exec: { command: [echo] }
";
        let errors = analyze_yaml(yaml).expect_err("literal when");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::WhenNotBoolean { field, .. } if field == "when"),
            "when literal",
        );
    }

    #[test]
    fn when_non_boolean_root_errors() {
        // Spec 03 §when invalid · « ${{ vars.threshold }} ❌ returns
        // integer · not bool ».
        let yaml = "\
nika: v1
workflow: t
vars:
  threshold: 5
tasks:
  - id: go
    when: ${{ vars.threshold }}
    exec: { command: [echo] }
";
        let errors = analyze_yaml(yaml).expect_err("non-bool when");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::WhenNotBoolean { reason, .. } if reason.contains("boolean")),
            "when not boolean-shaped",
        );
        // A1 (agent battery 2026-07-11): the teaching names all three
        // shape routes — the BOOL route leads (a bare flag reference is
        // the most natural thing this rule rejects, and the old
        // examples applied to a bool would trade VAR-005 for the rule-4
        // no-coercion type error).
        assert_has(
            &errors,
            |e| {
                matches!(e, SchemaError::WhenNotBoolean { reason, .. }
                if reason.contains("== true") && reason.contains("> 0") && reason.contains("!= \"\""))
            },
            "the teaching names the bool · number · string routes",
        );
    }

    #[test]
    fn when_explicit_comparison_valid() {
        // Spec 03 §when · « when: ${{ vars.threshold > 0 }} ».
        let yaml = "\
nika: v1
workflow: t
vars:
  threshold: 5
tasks:
  - id: go
    when: ${{ vars.threshold > 0 }}
    exec: { command: [echo] }
";
        analyze_yaml(yaml).expect("comparison is boolean-shaped");
    }

    #[test]
    fn unresolved_refs_carry_did_you_mean() {
        // The most frequent agent error class: a typo'd name. Every
        // namespace suggests within ITS OWN declared set (rustc model).
        let yaml = "\
nika: v1
workflow: t
vars: { topic: \"x\" }
tasks:
  - id: extract
    exec: { command: [echo] }
  - id: report
    depends_on: [extarct]
    exec: { command: [\"echo\", \"${{ vars.topci }}\", \"${{ tasks.extract.output }}\"] }
";
        let errors = analyze_yaml(yaml).expect_err("typos");
        let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            rendered
                .iter()
                .any(|m| m.contains("`extarct`") && m.contains("did you mean `extract`?")),
            "depends_on typo repaired: {rendered:?}"
        );
        assert!(
            rendered
                .iter()
                .any(|m| m.contains("vars.topci") && m.contains("did you mean `vars.topic`?")),
            "vars typo repaired in-namespace: {rendered:?}"
        );
    }

    #[test]
    fn typo_d_namespace_root_suggests_the_root() {
        let yaml = "\
nika: v1
workflow: t
vars: { topic: \"x\" }
tasks:
  - id: a
    exec: { command: [\"echo\", \"${{ vrs.topic }}\"] }
";
        let errors = analyze_yaml(yaml).expect_err("root typo");
        let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            rendered.iter().any(|m| m.contains("did you mean `vars`?")),
            "{rendered:?}"
        );
    }

    #[test]
    fn far_typo_gets_no_suggestion_clause() {
        let yaml = "\
nika: v1
workflow: t
vars: { topic: \"x\" }
tasks:
  - id: a
    exec: { command: [\"echo\", \"${{ vars.zzzzzzzzz }}\"] }
";
        let errors = analyze_yaml(yaml).expect_err("far typo");
        let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            rendered.iter().any(|m| m.contains("vars.zzzzzzzzz")),
            "{rendered:?}"
        );
        assert!(
            !rendered.iter().any(|m| m.contains("did you mean")),
            "silence beats a wrong guess: {rendered:?}"
        );
    }

    #[test]
    fn all_errors_collected_not_fail_fast() {
        // One workflow · several independent violations · ALL reported.
        let yaml = "\
nika: v1
workflow: t
tasks:
  - id: a
    depends_on: [ghost]
    when: ${{ vars.nope }}
    exec: { command: [echo] }
";
        let errors = analyze_yaml(yaml).expect_err("multi");
        assert!(errors.len() >= 3, "expected ≥3 errors, got {errors:?}");
    }
}
