// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sprint 2 Item 6 — Nika Shield L-SEC-001..008 lint rule tests.

#![cfg(test)]

use std::sync::Arc;

use nika_core::ast::analyzed::{
    AnalyzedAgentAction, AnalyzedExecAction, AnalyzedFetchAction, AnalyzedInferAction,
    AnalyzedTask, AnalyzedTaskAction, AnalyzedWorkflow, TaskId,
};
use nika_core::ast::templatable::Templatable;
use nika_core::binding::types::{BindingPath, BindingSource};
use nika_core::binding::{WithEntry, WithSpec};
use nika_core::source::Span;

use super::{lint_workflow, LintFinding, Severity};

fn make_task(wf: &mut AnalyzedWorkflow, name: &str, action: AnalyzedTaskAction) -> TaskId {
    let id = wf.task_table.insert(name);
    wf.tasks.push(AnalyzedTask {
        id,
        name: name.to_string(),
        description: None,
        action,
        provider: None,
        model: None,
        with_spec: WithSpec::default(),
        depends_on: Vec::new(),
        implicit_deps: Vec::new(),
        output: None,
        for_each: None,
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        trust_elevated: false,
        span: Span::dummy(),
    });
    id
}

fn make_task_with_dep(
    wf: &mut AnalyzedWorkflow,
    name: &str,
    action: AnalyzedTaskAction,
    upstream: TaskId,
    upstream_name: &str,
) -> TaskId {
    let mut spec = WithSpec::default();
    spec.insert(
        "data".to_string(),
        WithEntry::simple(BindingPath {
            source: BindingSource::Task(Arc::from(upstream_name)),
            segments: vec![],
        }),
    );
    let id = wf.task_table.insert(name);
    wf.tasks.push(AnalyzedTask {
        id,
        name: name.to_string(),
        description: None,
        action,
        provider: None,
        model: None,
        with_spec: spec,
        depends_on: vec![upstream],
        implicit_deps: Vec::new(),
        output: None,
        for_each: None,
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        trust_elevated: false,
        span: Span::dummy(),
    });
    id
}

fn has_rule(findings: &[LintFinding], rule: &str) -> bool {
    findings.iter().any(|f| f.rule == rule)
}

fn has_rule_for(findings: &[LintFinding], rule: &str, task: &str) -> bool {
    findings
        .iter()
        .any(|f| f.rule == rule && f.task_id.as_deref() == Some(task))
}

#[test]
fn l_sec_001_fires_on_untrusted_to_shell() {
    let mut wf = AnalyzedWorkflow::default();
    let fetch_id = make_task(
        &mut wf,
        "fetch_data",
        AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
    );
    make_task_with_dep(
        &mut wf,
        "run_shell",
        AnalyzedTaskAction::Exec(AnalyzedExecAction::default()),
        fetch_id,
        "fetch_data",
    );
    let findings = lint_workflow(&wf);
    assert!(has_rule_for(&findings, "L-SEC-001", "run_shell"));
}

#[test]
fn l_sec_002_fires_on_untrusted_to_agent_with_dangerous_tools() {
    let mut wf = AnalyzedWorkflow::default();
    let fetch_id = make_task(
        &mut wf,
        "fetch_data",
        AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
    );
    let agent = AnalyzedAgentAction {
        tools: vec!["nika:write".to_string(), "novanet::search".to_string()],
        ..Default::default()
    };
    make_task_with_dep(
        &mut wf,
        "agent_task",
        AnalyzedTaskAction::Agent(Box::new(agent)),
        fetch_id,
        "fetch_data",
    );
    let findings = lint_workflow(&wf);
    assert!(has_rule_for(&findings, "L-SEC-002", "agent_task"));
}

#[test]
fn l_sec_003_fires_on_untrusted_to_infer_no_schema() {
    let mut wf = AnalyzedWorkflow::default();
    let fetch_id = make_task(
        &mut wf,
        "fetch_data",
        AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
    );
    make_task_with_dep(
        &mut wf,
        "summarize",
        AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
        fetch_id,
        "fetch_data",
    );
    let findings = lint_workflow(&wf);
    assert!(has_rule_for(&findings, "L-SEC-003", "summarize"));
}

#[test]
fn l_sec_005_fires_on_untrusted_to_fetch_url() {
    let mut wf = AnalyzedWorkflow::default();
    let fetch_id = make_task(
        &mut wf,
        "fetch_first",
        AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
    );
    make_task_with_dep(
        &mut wf,
        "fetch_second",
        AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
        fetch_id,
        "fetch_first",
    );
    let findings = lint_workflow(&wf);
    assert!(has_rule_for(&findings, "L-SEC-005", "fetch_second"));
}

#[test]
fn l_sec_006_fires_on_untrusted_when_condition() {
    let mut wf = AnalyzedWorkflow::default();
    let fetch_id = make_task(
        &mut wf,
        "fetch_data",
        AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
    );
    let cond_id = make_task_with_dep(
        &mut wf,
        "cond_task",
        AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
        fetch_id,
        "fetch_data",
    );
    wf.tasks.iter_mut().find(|t| t.id == cond_id).unwrap().when = Some("{{with.data}}".into());
    let findings = lint_workflow(&wf);
    assert!(has_rule_for(&findings, "L-SEC-006", "cond_task"));
}

#[test]
fn l_sec_007_fires_on_skill_without_integrity_entry() {
    let mut wf = AnalyzedWorkflow::default();
    wf.skills_map
        .insert("brand".to_string(), "./skills/brand.md".to_string());
    make_task(&mut wf, "t1", AnalyzedTaskAction::default());
    let findings = lint_workflow(&wf);
    assert!(has_rule(&findings, "L-SEC-007"));
}

#[test]
fn l_sec_008_fires_on_high_max_turns_with_untrusted_inputs() {
    let mut wf = AnalyzedWorkflow::default();
    let fetch_id = make_task(
        &mut wf,
        "fetch_data",
        AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
    );
    let agent = AnalyzedAgentAction {
        max_turns: Some(Templatable::Value(50)),
        tools: vec!["novanet::search".to_string()],
        ..Default::default()
    };
    make_task_with_dep(
        &mut wf,
        "long_agent",
        AnalyzedTaskAction::Agent(Box::new(agent)),
        fetch_id,
        "fetch_data",
    );
    let findings = lint_workflow(&wf);
    assert!(has_rule_for(&findings, "L-SEC-008", "long_agent"));
}

#[test]
fn l_sec_008_silent_when_max_turns_under_20() {
    let mut wf = AnalyzedWorkflow::default();
    let fetch_id = make_task(
        &mut wf,
        "fetch_data",
        AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
    );
    let agent = AnalyzedAgentAction {
        max_turns: Some(Templatable::Value(10)),
        ..Default::default()
    };
    make_task_with_dep(
        &mut wf,
        "short_agent",
        AnalyzedTaskAction::Agent(Box::new(agent)),
        fetch_id,
        "fetch_data",
    );
    let findings = lint_workflow(&wf);
    assert!(!has_rule_for(&findings, "L-SEC-008", "short_agent"));
}

#[test]
fn l_sec_clean_workflow_has_no_security_findings() {
    let mut wf = AnalyzedWorkflow::default();
    make_task(
        &mut wf,
        "generate",
        AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
    );
    let findings = lint_workflow(&wf);
    let sec_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.rule.starts_with("L-SEC-"))
        .collect();
    assert!(
        sec_findings.is_empty(),
        "clean trusted workflow should have no L-SEC findings: {sec_findings:?}"
    );
}

#[test]
fn l_sec_lint_dispatcher_runs_security_module() {
    let mut wf = AnalyzedWorkflow::default();
    let fetch_id = make_task(
        &mut wf,
        "fetch_data",
        AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
    );
    make_task_with_dep(
        &mut wf,
        "run_shell",
        AnalyzedTaskAction::Exec(AnalyzedExecAction::default()),
        fetch_id,
        "fetch_data",
    );
    let findings = lint_workflow(&wf);
    let l_sec_count = findings
        .iter()
        .filter(|f| f.rule.starts_with("L-SEC-"))
        .count();
    assert!(
        l_sec_count > 0,
        "lint_workflow must include L-SEC findings via lint_security"
    );
    assert!(findings.iter().any(|f| f.severity == Severity::Warning));
}
