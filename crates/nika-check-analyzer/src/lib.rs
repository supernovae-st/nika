// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow analysis — the Core conformance rules over a [`RawWorkflow`].
//!
//! The analyzer COLLECTS every error (not fail-fast) so an author sees
//! the full diagnosis in one pass ·
//!
//! - envelope presence · `nika:` (the mark AND the kebab-case name) +
//!   a non-empty `tasks:` map (spec `01-envelope.md` · « That's the
//!   **whole minimum** »)
//! - duplicate task ids
//! - `NIKA-DAG-002` · `with:`/`after:` edge targets resolve
//! - `NIKA-DAG-001` · cycle detection over `G_p` = `E_d` ∪ `E_c` (incl.
//!   self-dependency)
//! - `NIKA-VAR-021` · `tasks.*` confined to the boundary (spec
//!   `04-variables.md` §the reference boundary)
//! - `NIKA-VAR-001` class · namespace-ref resolution (5 namespaces +
//!   `item`/`index` loop-locals · spec `04-variables.md`)
//! - `when:` boolean shape (`NIKA-VAR-005` class)
//! - `output:` binding rules · reserved names + pure-jq
//! - R3b · io `type:` speaks the full `TypeExpr` (NIKA-TYPE-001/006)
//!   · declared defaults conform (`NIKA-DEFAULT-001`)
//! - topological waves over the derived edges (spec `03-dag.md`)

// The test-lint waiver travels WITH the code it waives. It lived in the
// parent's lib.rs header while these modules were `mod analyzer;` there;
// carved out, the files kept their `.expect()`-in-test idiom and lost the
// permission for it — 101 clippy errors, none of them a behaviour change.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::used_underscore_items,
        clippy::float_cmp,
        clippy::manual_string_new,
        clippy::panic,
        clippy::unreachable,
    )
)]

mod builtin_shape;
mod dag;
pub mod edges;
mod jq_lint;
pub mod native_first;
mod scan;
mod schema_lint;
mod schema_paths;
mod static_ref;
mod thinking;
pub mod types_contract;

use std::collections::BTreeMap;

use nika_schema::error::SchemaError;
use nika_schema::raw::{RawTask, RawWorkflow};
use nika_schema::source::Spanned;
use nika_schema::types::AfterPredicate;

pub use edges::{Edge, EdgeKind, RecoveryRead, SettledState, role_of_field};
pub use static_ref::{bare_static_ref, static_literal_of};
pub use thinking::{MIN_REASONING_MAX_TOKENS, ThinkingFinding, thinking_findings};
pub use types_contract::{lowered_returns, returns_type};

/// The analyzer's output — the Graph IR plus its waves · lowering is
/// the runtime's job.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AnalyzedWorkflow {
    /// The derived scheduling edges (`G_p` = `E_d` ∪ `E_c`) — THE one edge
    /// computation every surface projects (spec 03 §the four graphs).
    pub edges: Vec<Edge>,
    /// The recovery reads (`E_r` · `on_error.recover` · non-scheduling).
    pub recovery_reads: Vec<RecoveryRead>,
    /// Topological execution waves · `topo_waves[n]` holds indices into
    /// `wf.tasks` that may run in parallel once wave `n-1` completed.
    pub topo_waves: Vec<Vec<usize>>,
}

impl AnalyzedWorkflow {
    /// Create an analyzed workflow.
    #[must_use]
    pub fn new(
        edges: Vec<Edge>,
        recovery_reads: Vec<RecoveryRead>,
        topo_waves: Vec<Vec<usize>>,
    ) -> Self {
        Self {
            edges,
            recovery_reads,
            topo_waves,
        }
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
    let derived = edges::derive_edges(&wf.tasks, &ids);
    dag::check_edge_targets_resolve(&wf.tasks, &ids, &mut errors);
    dag::check_cycles(&wf.tasks, &ids, &derived, &mut errors);
    dag::check_recover_acyclic(&wf.tasks, &ids, &derived, &mut errors);
    check_unwind_never_folds(&wf.tasks, &mut errors);
    builtin_shape::check_builtin_shapes(&wf.tasks, &mut errors);
    scan::scan_workflow(wf, &mut errors);
    jq_lint::scan_jq(wf, &mut errors);
    schema_lint::scan_schemas(wf, &mut errors);
    types_contract::check_types_contract(wf, &mut errors);
    types_contract::check_io_declarations(wf, &mut errors);

    if errors.is_empty() {
        let waves = dag::topo_waves(wf.tasks.len(), &derived);
        let reads = edges::derive_recovery_reads(&wf.tasks, &ids);
        Ok(AnalyzedWorkflow::new(derived, reads, waves))
    } else {
        Err(errors)
    }
}

/// Envelope presence (spec `01-envelope.md` · « The `nika:` line and a
/// non-empty `tasks:` map. That's the **whole minimum** to be a valid
/// Nika workflow. »).
///
/// ONE required line since the envelope nuke (2026-08-12), not two:
/// `wf.workflow` holds the value of `nika:` — the mark AND the name in
/// a single key — so its absence is the absence of `nika:`.
fn check_envelope(wf: &RawWorkflow, errors: &mut Vec<SchemaError>) {
    if wf.workflow.is_none() {
        errors.push(SchemaError::MissingEnvelopeField {
            field: "nika".to_owned(),
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

/// `NIKA-DAG-009` — an unwind task may not join a group.
///
/// Cleanup is an `E_f` attachment that never enters `G_p`; a fan-in edge
/// from it would have no wave to schedule against (spec 03 §group).
fn check_unwind_never_folds(tasks: &[Spanned<RawTask>], errors: &mut Vec<SchemaError>) {
    for task in tasks {
        let Some(group) = &task.value.group else {
            continue;
        };
        let is_unwind = task
            .value
            .after
            .iter()
            .any(|(_, p)| matches!(p.value, AfterPredicate::Unwind));
        if is_unwind {
            errors.push(SchemaError::UnwindInGroup {
                task: task.value.id.value.clone(),
                group: group.value.clone(),
                span: Some(group.span),
            });
        }
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
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn analyze_yaml(yaml: &str) -> Result<AnalyzedWorkflow, Vec<SchemaError>> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        analyze(&wf)
    }

    fn assert_has<F: Fn(&SchemaError) -> bool>(errors: &[SchemaError], pred: F, what: &str) {
        assert!(errors.iter().any(pred), "expected {what} in {errors:?}");
    }

    const MINIMAL_OK: &str = "\
nika: hello
tasks:
  greet:
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
    fn missing_nika_and_tasks_all_collected() {
        // COLLECT all errors · not fail-fast. TWO since the envelope
        // nuke, not three — `nika:` carries the identity alone.
        let errors = analyze_yaml("model: mock/echo\n").expect_err("2 missing");
        assert_eq!(errors.len(), 2, "{errors:?}");
        for field in ["nika", "tasks"] {
            assert_has(
                &errors,
                |e| matches!(e, SchemaError::MissingEnvelopeField { field: f, .. } if f == field),
                field,
            );
        }
    }

    #[test]
    fn missing_nika_only() {
        // Conformance fixture envelope/parse-missing-nika-key.
        let yaml = "\
tasks:
  greet:
    infer: { prompt: hi }
";
        let errors = analyze_yaml(yaml).expect_err("missing nika");
        assert_eq!(errors.len(), 1, "tasks are present: {errors:?}");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::MissingEnvelopeField { field, .. } if field == "nika"),
            "missing nika",
        );
    }

    #[test]
    fn a_bare_tasks_envelope_teaches_var_020_alone() {
        // `${{ tasks }}` names the ENVELOPE, not a dependency. It yields
        // an empty id, and DAG-002 used to fire on it — accusing the
        // author of depending on a task named `` and burying the real
        // teaching (NIKA-VAR-020) under a phantom typo.
        let yaml = "\
nika: bare
tasks:
  a:
    exec: { command: [\"a\"] }
  summary:
    with:
      all: \"${{ tasks }}\"
    exec: { command: [\"report\"] }
";
        let errors = analyze_yaml(yaml).expect_err("the bare envelope is not a value");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::BareTaskEnvelope { .. }),
            "NIKA-VAR-020 · the envelope is not a value",
        );
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e, SchemaError::UnknownDependency { .. })),
            "no phantom dependency on a task named ``: {errors:?}"
        );
    }

    // ── group · the fan-in fold (spec 03 §group) ───────────────────

    const GROUP_OK: &str = "\
nika: fold
tasks:
  leg_a:
    group: probes
    exec: { command: [\"a\"] }
  leg_b:
    group: probes
    exec: { command: [\"b\"] }
  summary:
    with:
      legs: \"${{ group.probes }}\"
    exec: { command: [\"report\"] }
";

    #[test]
    fn a_fold_derives_one_edge_per_declared_member() {
        // The PLURAL of the data edge: two members, two edges, both
        // fan-in, arriving in DECLARATION order so the fold's shape is
        // stable across re-runs where completion order would not be.
        let a = analyze_yaml(GROUP_OK).expect("valid");
        let fan: Vec<_> = a
            .edges
            .iter()
            .filter(|e| matches!(e.kind, EdgeKind::FanIn))
            .collect();
        assert_eq!(fan.len(), 2, "one edge per member: {:?}", a.edges);
        assert_eq!(fan[0].from, 0, "declaration order · leg_a first");
        assert_eq!(fan[1].from, 1, "declaration order · leg_b second");
        assert!(fan.iter().all(|e| e.to == 2), "both land on the consumer");
    }

    #[test]
    fn a_fan_in_edge_admits_every_settled_state() {
        // DELIBERATELY not the intersection of the members' field roles ·
        // an intersection would leave {skipped} and every fold would be
        // NIKA-DAG-006-dead on arrival. The fold runs whatever happened.
        use edges::SettledState::{Cancelled, Failure, Skipped, Success};
        for st in [Success, Failure, Skipped, Cancelled] {
            assert!(EdgeKind::FanIn.admits(st), "fan-in admits {st:?}");
        }
    }

    #[test]
    fn a_group_nobody_declares_is_refused() {
        // The load-bearing choice: a rename must be an ERROR, not a
        // smaller fold that stays green. An empty group is the same fact
        // as an absent one, so one code covers both.
        let yaml = GROUP_OK.replace("group: probes", "group: legs");
        let errors = analyze_yaml(&yaml).expect_err("no task declares `probes`");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnknownGroup { name, .. } if name == "probes"),
            "NIKA-DAG-008 on the renamed group",
        );
    }

    #[test]
    fn a_bare_group_names_no_group() {
        let yaml = GROUP_OK.replace("${{ group.probes }}", "${{ group }}");
        let errors = analyze_yaml(&yaml).expect_err("bare group");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnknownGroup { name, .. } if name.is_empty()),
            "the bare group names nothing",
        );
    }

    #[test]
    fn a_fold_outside_the_with_boundary_is_refused() {
        // ONE door, where `tasks.*` has five — nothing in VAR-021 needed
        // widening to admit the fold.
        let yaml = GROUP_OK.replace(
            "    with:\n      legs: \"${{ group.probes }}\"\n",
            "    when: \"${{ group.probes }}\"\n",
        );
        let errors = analyze_yaml(&yaml).expect_err("group outside with:");
        assert_has(
            &errors,
            |e| {
                matches!(
                    e,
                    SchemaError::RefOutsideBoundary { reference, .. } if reference == "group.probes"
                )
            },
            "NIKA-VAR-021 on a fold outside `with:`",
        );
    }

    #[test]
    fn empty_tasks_array_errors() {
        // Conformance fixture envelope/006-empty-tasks-array (W1 form: an
        // empty MAP — the sequence form dies earlier as NIKA-PARSE-022).
        let yaml = "nika: hello\ntasks: {}\n";
        let errors = analyze_yaml(yaml).expect_err("empty tasks");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::MissingEnvelopeField { field, .. } if field == "tasks"),
            "empty tasks",
        );
    }

    #[test]
    fn duplicate_task_id_errors() {
        // Conformance fixtures envelope/011 + 021 · W1: duplicate identity is
        // a duplicate MAP KEY — the YAML loader itself refuses it (PARSE-007
        // mechanics), before the analyzer ever runs.
        let yaml = "\
nika: dup
tasks:
  same:
    exec: { command: [echo] }
  same:
    exec: { command: [echo] }
";
        let err = parse(yaml, FileId::new(0), ParseMode::Strict).expect_err("dup key");
        assert!(
            matches!(&err, SchemaError::DuplicateKey { message, .. } if message.contains("same")),
            "{err:?}"
        );
    }

    #[test]
    fn duplicate_task_id_analyzer_belt_holds() {
        // Defense in depth: a RawWorkflow built by another frontend (not the
        // YAML loader) with two same-id tasks still trips the analyzer.
        let yaml = "\
nika: dup
tasks:
  same:
    exec: { command: [echo] }
  other:
    exec: { command: [echo] }
";
        let mut wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let mut clone = wf.tasks[1].clone();
        clone.value.id.value = "same".to_owned();
        wf.tasks[1] = clone;
        let errors = analyze(&wf).expect_err("dup id");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::DuplicateTaskId { id, .. } if id == "same"),
            "duplicate id",
        );
    }

    // ── NIKA-VAR-021 · the reference boundary ───────────────────────

    #[test]
    fn when_task_ref_is_outside_the_boundary() {
        // Conformance fixture dag-topology/003-when-task-ref-illegal ·
        // `when:` reads LOCAL names only — even WITH a control edge in
        // place, a `tasks.*` read there is refused (hoist into `with:`).
        let yaml = "\
nika: t
tasks:
  test:
    exec: { command: [\"./test.sh\"] }
  deploy:
    after: { test: success }
    when: ${{ tasks.test.status == 'success' }}
    exec: { command: [\"./deploy.sh\"] }
";
        let errors = analyze_yaml(yaml).expect_err("ref outside the boundary");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::RefOutsideBoundary { reference, surface, .. } if reference == "test" && surface == "when:"),
            "VAR-021 in when:",
        );
    }

    #[test]
    fn with_binding_is_the_edge() {
        // Conformance fixture dag-topology/005-with-binding-is-the-edge ·
        // pre-W2 this exact shape was the NIKA-DAG-003 class (a `with:`
        // reference without its `depends_on:` restatement); W2 derives the
        // edge FROM the binding, so the shape is simply valid.
        let yaml = "\
nika: t
tasks:
  research:
    infer: { prompt: \"research\" }
  summarize:
    with:
      content: ${{ tasks.research.output }}
    infer: { prompt: \"summarize ${{ with.content }}\" }
";
        let analyzed = analyze_yaml(yaml).expect("the binding IS the edge");
        assert_eq!(analyzed.edges.len(), 1);
        assert_eq!(analyzed.topo_waves, vec![vec![0], vec![1]]);
    }

    #[test]
    fn verb_body_ref_is_outside_the_boundary() {
        // Conformance fixture dag-topology/006-verb-body-reference-illegal.
        let yaml = "\
nika: t
tasks:
  research:
    infer: { prompt: \"research\" }
  brief:
    infer:
      prompt: \"Brief from ${{ tasks.research.output }}\"
";
        let errors = analyze_yaml(yaml).expect_err("ref outside the boundary");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::RefOutsideBoundary { reference, surface, .. } if reference == "research" && surface == "a verb field"),
            "VAR-021 via prompt",
        );
    }

    #[test]
    fn for_each_ref_is_outside_the_boundary() {
        // Conformance fixture dag-topology/007-for-each-reference-illegal ·
        // the collection crosses through `with:`, never directly.
        let yaml = "\
nika: t
tasks:
  discover:
    invoke: { tool: \"nika:read\" }
  process:
    for_each: { items: \"${{ tasks.discover.output }}\" }
    exec: { command: [echo] }
";
        let errors = analyze_yaml(yaml).expect_err("ref outside the boundary");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::RefOutsideBoundary { reference, surface, .. } if reference == "discover" && surface == "for_each:"),
            "VAR-021 via for_each",
        );
    }

    #[test]
    fn var_021_message_is_not_double_backticked() {
        // The VAR-021 #[error] template wraps the task id (`task `{task}``);
        // passing an already-wrapped `task `x`` location into the id field
        // would render `task `task `x```. The id field carries the BARE id —
        // the message reads `task `b` a verb field references …` cleanly.
        let yaml = "\
nika: t
tasks:
  a:
    exec: { command: [\"echo\", \"hi\"] }
  b:
    exec: { command: [\"echo\", \"${{ tasks.a.output }}\"] }
";
        let errors = analyze_yaml(yaml).expect_err("ref outside the boundary");
        let rendered = errors
            .iter()
            .find(|e| matches!(e, SchemaError::RefOutsideBoundary { .. }))
            .map(std::string::ToString::to_string)
            .expect("a VAR-021 finding");
        assert!(
            rendered.contains("task `b` a verb field references"),
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
        // the task id renders ONCE, wrapped once — never `task `task `a```.
        // The wording moved when `when:`/`for_each:` stopped admitting the
        // locals; what this test protects is the rendering, not the prose.
        let yaml = "\
nika: t
tasks:
  a:
    exec: { command: [\"echo\", \"${{ item }}\"] }
";
        let errors = analyze_yaml(yaml).expect_err("loop-local out of scope");
        let rendered = errors
            .iter()
            .find(|e| matches!(e, SchemaError::LoopLocalOutsideForEach { .. }))
            .map(std::string::ToString::to_string)
            .expect("a loop-local finding");
        assert!(
            rendered.contains("in task `a` here"),
            "the id renders once, cleanly: {rendered}"
        );
        assert!(
            !rendered.contains("task `task"),
            "no double-backtick wrap: {rendered}"
        );
    }

    #[test]
    fn tightened_value_edge_is_valid() {
        // Conformance fixture dag-topology/009-valid-tightened-value-edge ·
        // `after: {fetch: success}` BESIDE the value edge is a meaningful
        // tightening (edges compose by intersection · spec 03 §after).
        let yaml = "\
nika: t
tasks:
  fetch:
    invoke: { tool: \"nika:fetch\", args: { url: \"https://x.io\" } }
  use:
    after: { fetch: success }
    with:
      data: ${{ tasks.fetch.output }}
    infer: { prompt: \"use ${{ with.data }}\" }
";
        analyze_yaml(yaml).expect("valid");
    }

    #[test]
    fn on_error_recover_ref_needs_no_edge() {
        // Spec example 22 · recover references the fallback task with
        // NO depends_on edge.
        let yaml = "\
nika: t
tasks:
  cached:
    invoke: { tool: \"nika:read\" }
  fetch_article:
    invoke: { tool: \"nika:fetch\", args: { url: \"https://x.test\" } }
    on_error:
      recover: ${{ tasks.cached.output }}
";
        analyze_yaml(yaml).expect("valid");
    }

    // ── NIKA-VAR-001 class ──────────────────────────────────────────

    #[test]
    fn vars_undeclared_errors() {
        // Conformance fixture variables/003-vars-undeclared (post-C2: the
        // inputs authority).
        let yaml = "\
nika: t
inputs:
  topic: { type: string, required: true }
tasks:
  go:
    infer: { prompt: \"about ${{ inputs.topik }}\" }
";
        let errors = analyze_yaml(yaml).expect_err("typo'd input");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, .. } if reference == "inputs.topik"),
            "inputs.topik unresolved",
        );
    }

    #[test]
    fn dead_vars_ref_refuses_with_values_001() {
        // C2 · a `${{ vars.X }}` read is the dead namespace's refusal,
        // never the generic unresolved class (LAW-GRAMMAR-0201).
        let yaml = "\
nika: t
tasks:
  go:
    infer: { prompt: \"about ${{ vars.topic }}\" }
";
        let errors = analyze_yaml(yaml).expect_err("dead vars read");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::DeadValueForm { form, .. } if matches!(form, nika_schema::error::DeadForm::Vars)),
            "NIKA-VALUES-001 for the dead vars read",
        );
        let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            rendered
                .iter()
                .any(|m| m.contains("dead `vars` namespace")
                    && m.contains("inputs · const · secrets")),
            "the classification teaching: {rendered:?}"
        );
    }

    #[test]
    fn dead_env_ref_refuses_with_values_002() {
        // C2 · a `${{ env.X }}` read (LAW-GRAMMAR-0202).
        let yaml = "\
nika: t
tasks:
  go:
    exec: { command: [\"echo\", \"${{ env.HOME }}\"] }
";
        let errors = analyze_yaml(yaml).expect_err("dead env read");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::DeadValueForm { form, .. } if matches!(form, nika_schema::error::DeadForm::Env)),
            "NIKA-VALUES-002 for the dead env read",
        );
    }

    #[test]
    fn foreign_value_namespace_refuses_with_values_003() {
        // C2 · conformance fixture values/invalid/foreign-value-namespace ·
        // `${{ params.region }}` — outside the three-authority family AND
        // the runtime namespaces (LAW-SURFACE-0201). The refusal is
        // LAYERED: VAR-001 carries the did-you-mean, VALUES-003 teaches
        // the closed family (the oracle emits both · match-any protocol).
        let yaml = "\
nika: t
inputs:
  region: { type: string, required: true }
tasks:
  go:
    invoke:
      tool: \"nika:log\"
      args: { message: \"deploy to ${{ params.region }}\" }
";
        let errors = analyze_yaml(yaml).expect_err("foreign namespace");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, .. } if reference == "params"),
            "VAR-001 rides alongside (the unresolved class)",
        );
        assert_has(
            &errors,
            |e| {
                matches!(e, SchemaError::ForeignValueNamespace { root, .. } if root == "params")
                    && e.spec_code().to_string() == "NIKA-VALUES-003"
            },
            "NIKA-VALUES-003 for the foreign namespace",
        );
        let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            rendered
                .iter()
                .any(|m| m.contains("outside the three-authority family")
                    && m.contains("inputs · const · secrets")),
            "the closed-family teaching: {rendered:?}"
        );
    }

    #[test]
    fn envelope_model_unresolved_var_is_flagged() {
        // deep/019 — the oracle false-green class: an envelope
        // `model: "${{ vars.nope }}"` was ACCEPTED and died at dispatch.
        let yaml =
            "nika: w\nmodel: \"${{ inputs.nope }}\"\ntasks:\n  a:\n    infer: { prompt: hi }\n";
        let errors = analyze_yaml(yaml).expect_err("envelope model ref must flag");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, .. } if reference == "inputs.nope"),
            "inputs.nope unresolved at the envelope",
        );
    }

    #[test]
    fn foreign_value_namespace_refuses_with_values_003_layered_on_var_001() {
        // The conformance `values/invalid/foreign-value-namespace` case:
        // `params` is not one of the three authorities — the layered oracle
        // emits BOTH the unresolved refusal (VAR-001, did-you-mean) AND the
        // family refusal (VALUES-003 · LAW-SURFACE-0201).
        let yaml = "\
nika: t
inputs:
  region: { type: string, required: true }
tasks:
  go:
    invoke:
      tool: \"nika:log\"
      args: { message: \"deploy to ${{ params.region }}\" }
";
        let errors = analyze_yaml(yaml).expect_err("foreign namespace");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, .. } if reference == "params"),
            "the unresolved half (VAR-001)",
        );
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::ForeignValueNamespace { root, .. } if root == "params"),
            "the family half (VALUES-003)",
        );
        let foreign = errors
            .iter()
            .find(|e| matches!(e, SchemaError::ForeignValueNamespace { .. }))
            .expect("the foreign-namespace finding");
        assert_eq!(foreign.spec_code().to_string(), "NIKA-VALUES-003");
        let rendered = foreign.to_string();
        assert!(
            rendered.contains("params") && rendered.contains("inputs · const · secrets"),
            "the byte-mirrored teaching: {rendered}"
        );
    }

    #[test]
    fn with_undeclared_errors() {
        // Conformance fixture variables/004-with-undeclared.
        let yaml = "\
nika: t
tasks:
  go:
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
        // Conformance fixtures variables/005 + 006 (an undeclared read of
        // a LIVE authority — `inputs:` since `config:` died).
        let yaml = "\
nika: t
tasks:
  go:
    exec:
      command: [\"echo\", \"${{ inputs.MISSING }}\", \"${{ secrets.api_key }}\"]
";
        let errors = analyze_yaml(yaml).expect_err("undeclared");
        assert_has(
            &errors,
            |e| matches!(e, SchemaError::UnresolvedNamespaceRef { reference, .. } if reference == "inputs.MISSING"),
            "inputs.MISSING",
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
nika: t
tasks:
  go:
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
nika: t
tasks:
  real:
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
nika: t
tasks:
  process:
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
nika: t
const:
  locales: [\"fr\", \"es\"]
tasks:
  translate:
    for_each: { items: \"${{ const.locales }}\" }
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
nika: t
tasks:
  doc:
    infer:
      prompt: \"The syntax \\\\${{ vars.x }} references variables\"
";
        analyze_yaml(yaml).expect("escaped island is literal text");
    }

    #[test]
    fn unclosed_expression_errors() {
        // Conformance fixture variables/011-unclosed-expression.
        let yaml = "\
nika: t
tasks:
  go:
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
                "nika: t\ntasks:\n  go:\n    when: {expr}\n    exec: {{ command: [\"echo\", \"hi\"] }}\n"
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
        // A declared `output:` binding and a reserved record field both
        // resolve — judged at the boundary (`with:`), where refs live now.
        let yaml = "\
nika: t
tasks:
  api:
    invoke: { tool: \"nika:fetch\", args: { url: \"https://x.test\" } }
    extract:
      user_count: \".data.users | length\"
  report:
    with:
      count: ${{ tasks.api.user_count }}
      outcome: ${{ tasks.api.status }}
    infer:
      prompt: \"count ${{ with.count }} status ${{ with.outcome }}\"
";
        analyze_yaml(yaml).expect("record field + declared binding valid");
    }

    #[test]
    fn unknown_task_field_errors() {
        // The record-shape check fires on the boundary surface.
        let yaml = "\
nika: t
tasks:
  api:
    invoke: { tool: \"nika:fetch\" }
  report:
    with:
      bad: ${{ tasks.api.nonexistent_field }}
    infer:
      prompt: \"${{ with.bad }}\"
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
nika: t
tasks:
  api:
    invoke: { tool: \"nika:fetch\" }
    extract:
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
nika: t
tasks:
  api:
    invoke: { tool: \"nika:fetch\" }
    extract:
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
nika: t
tasks:
  go:
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
        // Spec 03 §when invalid · a bare flag reference that is not
        // boolean-shaped.
        let yaml = "\
nika: t
inputs:
  threshold: { type: integer, required: true }
tasks:
  go:
    when: ${{ inputs.threshold }}
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
        // Spec 03 §when · « when: ${{ inputs.threshold > 0 }} ».
        let yaml = "\
nika: t
inputs:
  threshold: { type: integer, required: true }
tasks:
  go:
    when: ${{ inputs.threshold > 0 }}
    exec: { command: [echo] }
";
        analyze_yaml(yaml).expect("comparison is boolean-shaped");
    }

    #[test]
    fn unresolved_refs_carry_did_you_mean() {
        // The most frequent agent error class: a typo'd name. Every
        // namespace suggests within ITS OWN declared set (rustc model).
        let yaml = "\
nika: t
inputs: { topic: { type: string, required: true } }
tasks:
  extract:
    exec: { command: [echo] }
  report:
    after: { extarct: success }
    with:
      data: ${{ tasks.extract.output }}
    exec: { command: [\"echo\", \"${{ inputs.topci }}\", \"${{ with.data }}\"] }
";
        let errors = analyze_yaml(yaml).expect_err("typos");
        let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            rendered
                .iter()
                .any(|m| m.contains("`extarct`") && m.contains("did you mean `extract`?")),
            "after: target typo repaired: {rendered:?}"
        );
        assert!(
            rendered
                .iter()
                .any(|m| m.contains("inputs.topci") && m.contains("did you mean `inputs.topic`?")),
            "inputs typo repaired in-namespace: {rendered:?}"
        );
    }

    #[test]
    fn typo_d_namespace_root_suggests_the_root() {
        let yaml = "\
nika: t
inputs: { topic: { type: string, required: true } }
tasks:
  a:
    exec: { command: [\"echo\", \"${{ inpts.topic }}\"] }
";
        let errors = analyze_yaml(yaml).expect_err("root typo");
        let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            rendered
                .iter()
                .any(|m| m.contains("did you mean `inputs`?")),
            "{rendered:?}"
        );
    }

    #[test]
    fn far_typo_gets_no_suggestion_clause() {
        let yaml = "\
nika: t
inputs: { topic: { type: string, required: true } }
tasks:
  a:
    exec: { command: [\"echo\", \"${{ inputs.zzzzzzzzz }}\"] }
";
        let errors = analyze_yaml(yaml).expect_err("far typo");
        let rendered: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            rendered.iter().any(|m| m.contains("inputs.zzzzzzzzz")),
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
nika: t
tasks:
  a:
    after: { ghost: success }
    when: ${{ vars.nope }}
    exec: { command: [echo] }
";
        let errors = analyze_yaml(yaml).expect_err("multi");
        assert!(errors.len() >= 3, "expected ≥3 errors, got {errors:?}");
    }
}
