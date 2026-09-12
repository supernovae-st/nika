// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    reason = "fixture construction and required findings are test assertions"
)]

//! COMP-002 repair attribution through the public composed-check door.
//! All sources come from the injected reader: no filesystem or effects.

use nika_check::{CheckReport, check_composed};
use nika_schema::{ParseMode, parse, source::FileId};

const PARENT: &str = "workflows/main.nika.yaml";
const CHILD: &str = "workflows/children/worker.nika.yaml";
const TARGET: &str = "./children/worker.nika.yaml";
const ECHO: &str = "exec: { command: [echo, hello] }";
const ECHO_GRANT: &str = "{ exec: [echo] }";

fn source(name: &str, permits: Option<&str>, action: &str) -> String {
    let permits = permits.map_or_else(String::new, |p| format!("permits: {p}\n"));
    format!("nika: {name}\n{permits}tasks:\n  work:\n    {action}\n")
}

fn composed(parent: Option<&str>, child: Option<&str>, action: &str) -> CheckReport {
    let parent = source(
        "parent",
        parent,
        &format!("invoke: {{ workflow: '{TARGET}' }}"),
    );
    let child = source("child", child, action);
    let workflow = parse(&parent, FileId::new(0), ParseMode::Strict).expect("valid parent");
    check_composed(&workflow, PARENT, &mut |path| match path {
        CHILD => Ok(child.clone()),
        other => Err(format!("no fixture for {other}")),
    })
}

fn refusal(report: &CheckReport) -> &str {
    assert_eq!(report.composition.len(), 1, "{:?}", report.composition);
    let finding = &report.composition[0];
    assert_eq!(finding.code, "NIKA-COMP-002");
    assert_eq!(finding.target, TARGET, "preserve the authored target");
    &finding.detail
}

#[test]
fn an_empty_child_names_the_child_repair_and_grants_never_descend() {
    let report = composed(Some(ECHO_GRANT), Some("{}"), ECHO);
    let detail = refusal(&report);
    assert!(detail.contains(CHILD), "{detail}");
    assert!(detail.contains("permits: {}"), "{detail}");
    assert!(
        !detail.contains(PARENT),
        "the parent already admits echo: {detail}"
    );
    assert!(detail.contains("echo"), "{detail}");
    assert!(
        composed(Some(ECHO_GRANT), Some(ECHO_GRANT), ECHO)
            .composition
            .is_empty()
    );
}

#[test]
fn an_absent_child_is_distinguished_from_a_declared_empty_child() {
    let report = composed(Some(ECHO_GRANT), None, ECHO);
    let detail = refusal(&report);
    assert!(detail.contains(CHILD), "{detail}");
    assert!(detail.contains("absent"), "{detail}");
    assert!(!detail.contains("permits: {}"), "{detail}");
    assert!(!detail.contains(PARENT), "{detail}");
}

#[test]
fn a_missing_parent_and_two_denying_sides_name_the_actual_repairs() {
    let report = composed(None, Some(ECHO_GRANT), ECHO);
    let detail = refusal(&report);
    assert!(detail.contains(PARENT), "{detail}");
    assert!(detail.contains("absent"), "{detail}");
    assert!(
        !detail.contains(CHILD),
        "the child already admits echo: {detail}"
    );
    let both = composed(Some("{}"), Some("{}"), ECHO);
    let detail = refusal(&both);
    assert!(
        detail.contains(PARENT) && detail.contains(CHILD),
        "{detail}"
    );
    assert!(
        composed(Some(ECHO_GRANT), Some(ECHO_GRANT), ECHO)
            .composition
            .is_empty()
    );
}

#[test]
fn a_missing_category_does_not_claim_that_the_whole_block_is_empty() {
    let report = composed(
        Some(ECHO_GRANT),
        Some("{ net: { http: [api.example.com] } }"),
        ECHO,
    );
    let detail = refusal(&report);
    assert!(detail.contains(CHILD), "{detail}");
    assert!(!detail.contains("permits: {}"), "{detail}");
    assert!(!detail.contains("absent"), "{detail}");
}

#[test]
fn two_individually_admitting_patterns_still_fail_the_conservative_meet() {
    let cases = [
        (
            "{ tools: ['nika:*'], fs: { read: ['./data/item.txt'] } }",
            "{ tools: ['nika:read'], fs: { read: ['./data/item.txt'] } }",
            "invoke: { tool: 'nika:read', args: { path: './data/item.txt' } }",
        ),
        (
            "{ tools: ['nika:read'], fs: { read: ['./data/**'] } }",
            "{ tools: ['nika:read'], fs: { read: ['./data/item.txt'] } }",
            "invoke: { tool: 'nika:read', args: { path: './data/item.txt' } }",
        ),
        (
            "{ tools: ['nika:fetch'], net: { http: ['*.example.com'] } }",
            "{ tools: ['nika:fetch'], net: { http: ['api.example.com'] } }",
            "invoke: { tool: 'nika:fetch', args: { url: 'https://api.example.com/item' } }",
        ),
    ];
    for (parent, child, action) in cases {
        let report = composed(Some(parent), Some(child), action);
        let detail = refusal(&report);
        assert!(detail.contains("conservative"), "{detail}");
        assert!(
            detail.contains(PARENT) && detail.contains(CHILD),
            "{detail}"
        );
        assert!(
            composed(Some(child), Some(child), action)
                .composition
                .is_empty()
        );
    }
}

#[test]
fn a_shell_body_still_needs_any_exec_even_if_the_program_list_contains_echo() {
    let shell = "exec: { shell: 'echo hello' }";
    for (parent, child, denied_path) in [
        ("{ exec: true }", ECHO_GRANT, CHILD),
        (ECHO_GRANT, "{ exec: true }", PARENT),
    ] {
        let report = composed(Some(parent), Some(child), shell);
        let detail = refusal(&report);
        assert!(detail.contains(denied_path), "{detail}");
        assert!(detail.contains("exec: true"), "{detail}");
        assert!(!detail.contains("child declares `exec: true`"), "{detail}");
    }
    assert!(
        composed(Some("{ exec: true }"), Some("{ exec: true }"), shell)
            .composition
            .is_empty()
    );
}

#[test]
fn every_concrete_axis_points_to_the_denying_child() {
    for (grant, action, effect, count) in [
        (ECHO_GRANT, ECHO, "exec program", 1),
        (
            "{ tools: ['nika:read'], fs: { read: ['./data/item.txt'] } }",
            "invoke: { tool: 'nika:read', args: { path: './data/item.txt' } }",
            "fs read",
            2,
        ),
        (
            "{ tools: ['nika:write'], fs: { write: ['./data/item.txt'] } }",
            "invoke: { tool: 'nika:write', args: { path: './data/item.txt', content: hello } }",
            "fs write",
            2,
        ),
        (
            "{ tools: ['nika:fetch'], net: { http: ['api.example.com'] } }",
            "invoke: { tool: 'nika:fetch', args: { url: 'https://api.example.com/item' } }",
            "net host",
            2,
        ),
    ] {
        let report = composed(Some(grant), Some("{}"), action);
        assert_eq!(
            report.composition.len(),
            count,
            "{effect}: {:?}",
            report.composition
        );
        assert!(report.composition.iter().any(|f| f.detail.contains(effect)));
        for finding in &report.composition {
            assert_eq!(finding.code, "NIKA-COMP-002");
            assert!(finding.detail.contains(CHILD), "{finding:?}");
            assert!(!finding.detail.contains(PARENT), "{finding:?}");
        }
        assert!(
            composed(Some(grant), Some(grant), action)
                .composition
                .is_empty()
        );
    }
}

#[test]
fn the_public_finding_retains_the_same_repair_and_source_anchor() {
    let report = composed(Some(ECHO_GRANT), Some("{}"), ECHO);
    let detail = refusal(&report);
    let public = report
        .findings
        .iter()
        .find(|f| f.code.as_deref() == Some("NIKA-COMP-002"))
        .expect("composition projected through the shared public door");
    assert!(public.message.contains(detail), "{public:?}");
    assert_eq!(public.span, Some(report.composition[0].span));
    assert_eq!(public.task.as_deref(), Some("work"));
}
