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

const PARENT: &str = "workflows/main.nika";
const CHILD: &str = "workflows/children/worker.nika";
const TARGET: &str = "./children/worker.nika";
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

/// Each child grant sits inside the parent's: `nika:read` under `nika:*`,
/// `./data/item.txt` under `./data/**`, `api.example.com` under
/// `*.example.com`. The meet keeps the narrower grant, so these compose
/// (they were refused while the meet compared spellings).
#[test]
fn two_individually_admitting_patterns_compose_through_the_meet() {
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
        assert!(report.composition.is_empty(), "{:?}", report.composition);
        assert!(
            composed(Some(child), Some(child), action)
                .composition
                .is_empty()
        );
    }
}

/// `./data/*.txt` and `./data/item*` both admit `./data/item.txt` and
/// neither contains the other: the meet stays empty and the refusal keeps
/// its conservative wording, naming both files.
#[test]
fn two_incomparable_patterns_still_fail_the_conservative_meet() {
    let report = composed(
        Some(&read_grant("./data/*.txt")),
        Some(&read_grant("./data/item*")),
        &read_of("./data/item.txt"),
    );
    let detail = refusal(&report);
    assert!(detail.contains("conservative"), "{detail}");
    assert!(
        detail.contains(PARENT) && detail.contains(CHILD),
        "{detail}"
    );
}

// ─── a child grant inside the parent's glob composes (spec 14 laws 3/4) ───

const SHELL: &str = "./references/preview-shell.html";

fn read_grant(glob: &str) -> String {
    format!("{{ tools: ['nika:read'], fs: {{ read: ['{glob}'] }} }}")
}

fn read_of(path: &str) -> String {
    format!("invoke: {{ tool: 'nika:read', args: {{ path: '{path}' }} }}")
}

/// Today's refusal when the parent does not admit a read the child grants.
fn parent_refusal(need: &str) -> String {
    format!(
        "child body needs fs read `{need}`; parent boundary in `{PARENT}` \
         (`permits:` declared) does not admit it; add the intended grant in \
         that file (spec 14 laws 3/4)"
    )
}

fn finding_codes(report: &CheckReport) -> Vec<Option<String>> {
    report.findings.iter().map(|f| f.code.clone()).collect()
}

/// The reported journey: the parent grants `./references/**` and the child
/// declares and reads exactly one file in it. That honest grant now checks
/// exactly as the workaround did (repeating the parent's glob in the child).
#[test]
fn a_literal_inside_the_parent_glob_composes_like_the_repeated_glob() {
    let parent = read_grant("./references/**");
    let honest = composed(Some(&parent), Some(&read_grant(SHELL)), &read_of(SHELL));
    let repeated = composed(Some(&parent), Some(&parent), &read_of(SHELL));
    assert!(honest.composition.is_empty(), "{:?}", honest.composition);
    assert!(
        repeated.composition.is_empty(),
        "{:?}",
        repeated.composition
    );
    assert_eq!(finding_codes(&honest), finding_codes(&repeated));
}

#[test]
fn a_literal_outside_the_parent_glob_keeps_todays_refusal() {
    let parent = read_grant("./references/**");
    let inside = composed(Some(&parent), Some(&read_grant(SHELL)), &read_of(SHELL));
    assert!(inside.composition.is_empty(), "{:?}", inside.composition);
    let outside = "./other/x.html";
    let report = composed(Some(&parent), Some(&read_grant(outside)), &read_of(outside));
    assert_eq!(refusal(&report), parent_refusal(outside));
}

/// A narrower child glob composes; a wider one (`./**`) is cut to the
/// parent's tree: a read inside it composes, a read outside it is refused.
#[test]
fn a_narrower_glob_composes_and_a_wider_child_glob_stays_capped() {
    let parent = read_grant("./references/**");
    let nested = composed(
        Some(&parent),
        Some(&read_grant("./references/html/**")),
        &read_of("./references/html/a.html"),
    );
    assert!(nested.composition.is_empty(), "{:?}", nested.composition);
    let wide = read_grant("./**");
    let within = composed(Some(&parent), Some(&wide), &read_of(SHELL));
    assert!(within.composition.is_empty(), "{:?}", within.composition);
    let outside = "./other/x.html";
    let report = composed(Some(&parent), Some(&wide), &read_of(outside));
    assert_eq!(refusal(&report), parent_refusal(outside));
}

/// Containment reads paths as the matcher does: the `./` spelling folds,
/// case does not, and a grant naming `..` keeps the spelling-only meet with
/// today's conservative refusal. Reads that leave the workspace (`..`
/// escapes · absolute paths) infer no need here at all; their refusal stays
/// the meet's and the run's (`nika-cap` · `NIKA-SEC-004`).
#[test]
fn dot_spelling_case_and_climbs_follow_the_matcher() {
    let spelled = composed(
        Some(&read_grant("references/**")),
        Some(&read_grant(SHELL)),
        &read_of(SHELL),
    );
    assert!(spelled.composition.is_empty(), "{:?}", spelled.composition);
    let parent = read_grant("./references/**");
    let upper = "./References/x.html";
    let report = composed(Some(&parent), Some(&read_grant(upper)), &read_of(upper));
    assert_eq!(refusal(&report), parent_refusal(upper));
    let climbing = composed(
        Some(&parent),
        Some(&read_grant("./references/../references/x.html")),
        &read_of("./references/x.html"),
    );
    let detail = refusal(&climbing);
    assert!(detail.contains("conservative"), "{detail}");
    assert!(
        detail.contains(PARENT) && detail.contains(CHILD),
        "{detail}"
    );
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
