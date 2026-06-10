// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! `one-obvious-way` lint pass · spec `03-dag.md` §One obvious way.
//!
//! The spec table is **normative for linters** — 7 preference rules
//! (`one-obvious-way/001`…`/007` · table order) shipped as WARNINGS ·
//! never hard errors (the discouraged forms are legal · just not
//! canonical). Each rule gets a fires + does-not-fire pair so the
//! precision contract (low false-positive) is pinned by tests.

use nika_schema::lints::{Lint, one_obvious_way};
use nika_schema::{FileId, ParseMode, parse};

/// Parse a fixture (strict mode · must be valid) and run the lint pass.
fn lint(yaml: &str) -> Vec<Lint> {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("lint fixture must parse");
    one_obvious_way(&wf)
}

/// `(rule, task_id)` projection for compact assertions.
fn rules(yaml: &str) -> Vec<(String, String)> {
    lint(yaml)
        .into_iter()
        .map(|l| (l.rule.to_string(), l.task_id))
        .collect()
}

// ───────────────────────── 001 · redundant success `when:` ─────────────────────────

#[test]
fn rule_001_fires_on_redundant_success_when() {
    let yaml = r#"
nika: v1
workflow: f001
tasks:
  - id: a
    exec: { command: "./a.sh" }
  - id: b
    depends_on: [a]
    when: "${{ tasks.a.status == 'success' }}"
    exec: { command: "./b.sh" }
"#;
    assert_eq!(
        rules(yaml),
        vec![("one-obvious-way/001".to_string(), "b".to_string())]
    );
}

#[test]
fn rule_001_fires_on_reversed_operands() {
    let yaml = r#"
nika: v1
workflow: f001r
tasks:
  - id: a
    exec: { command: "./a.sh" }
  - id: b
    depends_on: [a]
    when: "${{ 'success' == tasks.a.status }}"
    exec: { command: "./b.sh" }
"#;
    assert_eq!(
        rules(yaml),
        vec![("one-obvious-way/001".to_string(), "b".to_string())]
    );
}

#[test]
fn rule_001_fires_on_singleton_in_list() {
    let yaml = r#"
nika: v1
workflow: f001in
tasks:
  - id: a
    exec: { command: "./a.sh" }
  - id: b
    depends_on: [a]
    when: "${{ tasks.a.status in ['success'] }}"
    exec: { command: "./b.sh" }
"#;
    assert_eq!(
        rules(yaml),
        vec![("one-obvious-way/001".to_string(), "b".to_string())]
    );
}

#[test]
fn rule_001_silent_when_dependency_can_be_skipped() {
    // `on_error: skip` on the dependency makes the status check REAL
    // work (skipped ≠ success) — not a restatement of the default gate.
    let yaml = r#"
nika: v1
workflow: f001skip
tasks:
  - id: a
    exec: { command: "./a.sh" }
    on_error: { skip: true }
  - id: b
    depends_on: [a]
    when: "${{ tasks.a.status == 'success' }}"
    exec: { command: "./b.sh" }
"#;
    assert!(rules(yaml).is_empty());
}

#[test]
fn rule_001_silent_on_compound_expression() {
    // A conjunct carries intent beyond the default gate — only the
    // WHOLE-expression restatement is noise.
    let yaml = r#"
nika: v1
workflow: f001and
vars:
  go: { type: boolean, default: true }
tasks:
  - id: a
    exec: { command: "./a.sh" }
  - id: b
    depends_on: [a]
    when: "${{ tasks.a.status == 'success' && vars.go }}"
    exec: { command: "./b.sh" }
"#;
    assert!(rules(yaml).is_empty());
}

#[test]
fn rule_001_silent_when_status_is_not_a_dependency() {
    // Guarding on a task OUTSIDE depends_on is not the default-gate
    // restatement (it is a different — possibly invalid — construct).
    let yaml = r#"
nika: v1
workflow: f001nodep
tasks:
  - id: a
    exec: { command: "./a.sh" }
  - id: c
    exec: { command: "./c.sh" }
  - id: b
    depends_on: [a, c]
    when: "${{ tasks.a.status == 'failure' }}"
    exec: { command: "./b.sh" }
"#;
    // `== 'failure'` is not the success restatement → 001 silent.
    let fired = rules(yaml);
    assert!(!fired.iter().any(|(r, _)| r == "one-obvious-way/001"));
}

// ───────────────────────── 002 · skip-for-dependents ─────────────────────────

#[test]
fn rule_002_fires_on_skip_with_unguarded_dependent() {
    let yaml = r#"
nika: v1
workflow: f002
tasks:
  - id: a
    exec: { command: "./a.sh" }
    on_error: { skip: true }
  - id: b
    depends_on: [a]
    exec: { command: "./b.sh" }
"#;
    assert_eq!(
        rules(yaml),
        vec![("one-obvious-way/002".to_string(), "a".to_string())]
    );
}

#[test]
fn rule_002_silent_when_dependents_guard_on_status() {
    // The canonical skip pattern (05-errors example) · dependents read
    // the status explicitly.
    let yaml = r#"
nika: v1
workflow: f002ok
tasks:
  - id: a
    exec: { command: "./a.sh" }
    on_error: { skip: true }
  - id: b
    depends_on: [a]
    when: "${{ tasks.a.status == 'success' }}"
    exec: { command: "./b.sh" }
"#;
    assert!(rules(yaml).is_empty());
}

#[test]
fn rule_002_silent_without_dependents() {
    let yaml = r#"
nika: v1
workflow: f002leaf
tasks:
  - id: a
    exec: { command: "./a.sh" }
    on_error: { skip: true }
"#;
    assert!(rules(yaml).is_empty());
}

// ───────────────────────── 003 · retry via duplicate task ─────────────────────────

#[test]
fn rule_003_fires_on_failure_guarded_duplicate() {
    let yaml = r#"
nika: v1
workflow: f003
tasks:
  - id: a
    invoke:
      tool: "nika:fetch"
      args: { url: "https://example.com/data" }
  - id: a_retry
    depends_on: [a]
    when: "${{ tasks.a.status == 'failure' }}"
    invoke:
      tool: "nika:fetch"
      args: { url: "https://example.com/data" }
"#;
    assert_eq!(
        rules(yaml),
        vec![("one-obvious-way/003".to_string(), "a_retry".to_string())]
    );
}

#[test]
fn rule_003_silent_when_bodies_differ() {
    // A failure-guarded task doing DIFFERENT work is legitimate
    // (« use a task only when real work runs on failure »).
    let yaml = r#"
nika: v1
workflow: f003diff
tasks:
  - id: a
    invoke:
      tool: "nika:fetch"
      args: { url: "https://example.com/data" }
  - id: report
    depends_on: [a]
    when: "${{ tasks.a.status == 'failure' }}"
    invoke:
      tool: "nika:notify"
      args: { channel: webhook, target: "https://hooks.example.com", message: "a failed" }
"#;
    assert!(rules(yaml).is_empty());
}

// ───────────────────────── 004 · fallback value via task ─────────────────────────

#[test]
fn rule_004_fires_on_literal_jq_fallback_task() {
    let yaml = r#"
nika: v1
workflow: f004
tasks:
  - id: a
    invoke:
      tool: "nika:fetch"
      args: { url: "https://example.com/data" }
  - id: fallback
    depends_on: [a]
    when: "${{ tasks.a.status == 'failure' }}"
    invoke:
      tool: "nika:jq"
      args: { input: { count: 0 }, expression: "." }
"#;
    assert_eq!(
        rules(yaml),
        vec![("one-obvious-way/004".to_string(), "fallback".to_string())]
    );
}

#[test]
fn rule_004_fires_on_echo_fallback_task() {
    let yaml = r#"
nika: v1
workflow: f004echo
tasks:
  - id: a
    exec: { command: "./a.sh" }
  - id: fallback
    depends_on: [a]
    when: "${{ tasks.a.status == 'failure' }}"
    exec: { command: "echo default-value" }
"#;
    assert_eq!(
        rules(yaml),
        vec![("one-obvious-way/004".to_string(), "fallback".to_string())]
    );
}

#[test]
fn rule_004_silent_on_real_failure_work() {
    // nika:fetch on failure = real work · NOT a mere value.
    let yaml = r#"
nika: v1
workflow: f004real
tasks:
  - id: a
    invoke:
      tool: "nika:fetch"
      args: { url: "https://primary.example.com" }
  - id: mirror
    depends_on: [a]
    when: "${{ tasks.a.status == 'failure' }}"
    invoke:
      tool: "nika:fetch"
      args: { url: "https://mirror.example.com" }
"#;
    assert!(rules(yaml).is_empty());
}

// ───────────────────────── 005 · cleanup via terminal task ─────────────────────────

#[test]
fn rule_005_fires_on_depends_on_everything_with_permissive_when() {
    let yaml = r#"
nika: v1
workflow: f005
tasks:
  - id: a
    exec: { command: "./a.sh" }
  - id: b
    exec: { command: "./b.sh" }
  - id: cleanup
    depends_on: [a, b]
    when: "${{ tasks.a.status in ['success', 'failure'] }}"
    exec: { command: "./cleanup.sh" }
"#;
    assert_eq!(
        rules(yaml),
        vec![("one-obvious-way/005".to_string(), "cleanup".to_string())]
    );
}

#[test]
fn rule_005_silent_on_plain_join_task() {
    // A sink that depends on everything WITHOUT a permissive when is a
    // legitimate join.
    let yaml = r#"
nika: v1
workflow: f005join
tasks:
  - id: a
    exec: { command: "./a.sh" }
  - id: b
    exec: { command: "./b.sh" }
  - id: summarize
    depends_on: [a, b]
    exec: { command: "./summarize.sh" }
"#;
    assert!(rules(yaml).is_empty());
}

#[test]
fn rule_005_silent_when_not_depending_on_everything() {
    let yaml = r#"
nika: v1
workflow: f005partial
tasks:
  - id: a
    exec: { command: "./a.sh" }
  - id: b
    exec: { command: "./b.sh" }
  - id: c
    depends_on: [a]
    when: "${{ tasks.a.status in ['success', 'failure'] }}"
    exec: { command: "./c.sh" }
"#;
    assert!(rules(yaml).is_empty());
}

// ───────────────────────── 006 · per-element timing tricks ─────────────────────────

#[test]
fn rule_006_fires_on_timeout_command_in_for_each() {
    let yaml = r#"
nika: v1
workflow: f006
tasks:
  - id: shards
    for_each: [1, 2, 3]
    exec: { command: "timeout 30 ./process.sh" }
"#;
    assert_eq!(
        rules(yaml),
        vec![("one-obvious-way/006".to_string(), "shards".to_string())]
    );
}

#[test]
fn rule_006_silent_without_for_each() {
    let yaml = r#"
nika: v1
workflow: f006plain
tasks:
  - id: one
    exec: { command: "timeout 30 ./process.sh" }
"#;
    assert!(rules(yaml).is_empty());
}

#[test]
fn rule_006_silent_on_plain_for_each_command() {
    let yaml = r#"
nika: v1
workflow: f006ok
tasks:
  - id: shards
    for_each: [1, 2, 3]
    timeout: 30s
    exec: { command: "./process.sh" }
"#;
    assert!(rules(yaml).is_empty());
}

// ───────────────────────── 007 · manual sharding ─────────────────────────

#[test]
fn rule_007_fires_on_sequential_exec_shards() {
    let yaml = r#"
nika: v1
workflow: f007
tasks:
  - id: shard1
    exec: { command: "./process.sh part1" }
  - id: shard2
    depends_on: [shard1]
    exec: { command: "./process.sh part2" }
  - id: shard3
    depends_on: [shard2]
    exec: { command: "./process.sh part3" }
"#;
    assert_eq!(
        rules(yaml),
        vec![("one-obvious-way/007".to_string(), "shard1".to_string())]
    );
}

#[test]
fn rule_007_fires_on_invoke_arg_shards() {
    let yaml = r#"
nika: v1
workflow: f007invoke
tasks:
  - id: page1
    invoke:
      tool: "nika:fetch"
      args: { url: "https://example.com/page/1" }
  - id: page2
    depends_on: [page1]
    invoke:
      tool: "nika:fetch"
      args: { url: "https://example.com/page/2" }
  - id: page3
    depends_on: [page2]
    invoke:
      tool: "nika:fetch"
      args: { url: "https://example.com/page/3" }
"#;
    assert_eq!(
        rules(yaml),
        vec![("one-obvious-way/007".to_string(), "page1".to_string())]
    );
}

#[test]
fn rule_007_silent_on_genuine_pipeline() {
    // Distinct steps (different token shapes) = a pipeline · not shards.
    let yaml = r#"
nika: v1
workflow: f007pipe
tasks:
  - id: fetch
    exec: { command: "./fetch.sh" }
  - id: parse
    depends_on: [fetch]
    exec: { command: "./parse.sh --strict input.json" }
  - id: report
    depends_on: [parse]
    exec: { command: "./report.sh" }
"#;
    assert!(rules(yaml).is_empty());
}

#[test]
fn rule_007_silent_on_two_task_chain() {
    let yaml = r#"
nika: v1
workflow: f007two
tasks:
  - id: shard1
    exec: { command: "./process.sh part1" }
  - id: shard2
    depends_on: [shard1]
    exec: { command: "./process.sh part2" }
"#;
    assert!(rules(yaml).is_empty());
}

// ───────────────────────── pass shape ─────────────────────────

#[test]
fn lints_are_deterministic_and_task_ordered() {
    // Two independent findings → emitted in task order.
    let yaml = r#"
nika: v1
workflow: forder
tasks:
  - id: a
    exec: { command: "./a.sh" }
    on_error: { skip: true }
  - id: b
    depends_on: [a]
    exec: { command: "./b.sh" }
  - id: shards
    for_each: [1, 2]
    exec: { command: "timeout 5 ./p.sh" }
"#;
    let fired = rules(yaml);
    assert_eq!(
        fired,
        vec![
            ("one-obvious-way/002".to_string(), "a".to_string()),
            ("one-obvious-way/006".to_string(), "shards".to_string()),
        ]
    );
}

#[test]
fn lint_carries_message_and_suggestion() {
    let yaml = r#"
nika: v1
workflow: fshape
tasks:
  - id: a
    exec: { command: "./a.sh" }
  - id: b
    depends_on: [a]
    when: "${{ tasks.a.status == 'success' }}"
    exec: { command: "./b.sh" }
"#;
    let lints = lint(yaml);
    assert_eq!(lints.len(), 1);
    let l = &lints[0];
    assert_eq!(l.rule, "one-obvious-way/001");
    assert!(!l.message.is_empty());
    assert!(!l.suggestion.is_empty());
    // The suggestion names the canonical form.
    assert!(l.suggestion.contains("depends_on"));
}

#[test]
fn clean_workflow_yields_no_lints() {
    let yaml = r#"
nika: v1
workflow: fclean
tasks:
  - id: fetch
    invoke:
      tool: "nika:fetch"
      args: { url: "https://example.com/data" }
    retry: { max_attempts: 3 }
    on_error:
      recover: { items: [] }
  - id: process
    depends_on: [fetch]
    for_each: "${{ tasks.fetch.output.items }}"
    max_parallel: 4
    timeout: 60s
    exec: { command: "./process.sh" }
"#;
    assert!(rules(yaml).is_empty());
}
