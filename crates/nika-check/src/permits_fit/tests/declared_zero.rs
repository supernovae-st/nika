// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The declared-block pure-internal exemption matrix (KR-01 ·
//! kimi-security-repair-v2): every block form × every call shape, judged
//! on the VISIBLE outcome (the escapes vector), never on the algorithm.
//! Extracted to its own file per the 1500-line cap (tests.rs keeps only
//! the one-line `mod declared_zero;` declaration). Zero `.unwrap()` /
//! `.expect()` here: the one panic path is the parse helper's.

use super::*;
use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;

fn escapes_of(yaml: &str) -> Vec<CapabilityEscape> {
    match parse(yaml, FileId::new(0), ParseMode::Strict) {
        Ok(wf) => scan_escapes(&wf),
        Err(e) => panic!("the fixture must parse: {e}"),
    }
}

/// The review's controlled fixture path (KR-01): an existing regular
/// file with no symlinks on its chain · never read by these tests
/// (static judgment only).
const P: &str = "/private/tmp/kimi-review/bundle.json";

fn assert_clean(case: &str, yaml: &str) {
    assert!(
        escapes_of(yaml).is_empty(),
        "{case}: expected clean, got {:?}",
        escapes_of(yaml)
    );
}

fn assert_refused(case: &str, yaml: &str) {
    assert!(
        !escapes_of(yaml).is_empty(),
        "{case}: expected a refusal, got clean"
    );
}

/// workflow without a const block (`permits` = "absent" or a block body).
fn wf(permits: &str, task: &str) -> String {
    match permits {
        "absent" => format!("nika: w\ntasks:\n  t:\n    {task}\n"),
        _ => format!("nika: w\npermits: {permits}\ntasks:\n  t:\n    {task}\n"),
    }
}

/// workflow whose const block binds `bundle` to P (the KR-01 repro form).
fn wf_const(permits: &str, task: &str) -> String {
    match permits {
        "absent" => format!("nika: w\nconst: {{ bundle: {P} }}\ntasks:\n  d:\n    {task}\n"),
        _ => format!(
            "nika: w\nconst: {{ bundle: {P} }}\npermits: {permits}\ntasks:\n  d:\n    {task}\n"
        ),
    }
}

fn decide_with(bundle: &str) -> String {
    format!("invoke: {{ tool: \"nika:decide\", args: {{ bundle: {bundle}, evidence: {{}} }} }}")
}

fn fs_only() -> String {
    format!("{{ fs: {{ read: [\"{P}\"] }} }}")
}

/// The class exemption under every block form: proven-pure calls need
/// no tools grant (spec 01 §permits · corrected 2026-08-11).
#[test]
fn proven_pure_calls_are_clean_under_any_block_form() {
    let jq = "invoke: { tool: \"nika:jq\", args: { expression: \".a+1\", input: { a: 1 } } }";
    assert_clean("absent · jq", &wf("absent", jq));
    assert_clean("`{}` · jq", &wf("{}", jq));
    assert_clean(
        "`{}` · hash (a second class member)",
        &wf(
            "{}",
            "invoke: { tool: \"nika:hash\", args: { content: \"x\", algo: sha256 } }",
        ),
    );
    assert_clean(
        "foreign tools block · jq (under ANY form)",
        &wf("{ tools: [\"nika:read\"] }", jq),
    );
}

/// `decide` really is pure when its bundle never names a path: absent
/// or inline-object bundles stay exempt under every block form.
#[test]
fn an_inline_object_bundle_stays_exempt_under_any_block_form() {
    let inline = decide_with("{ policy: {} }");
    assert_clean("absent · decide inline", &wf("absent", &inline));
    assert_clean("`{}` · decide inline", &wf("{}", &inline));
    assert_clean(
        "fs-only · decide inline (no fs question on an object)",
        &wf(&fs_only(), &inline),
    );
    assert_clean(
        "absent bundle · decide",
        &wf(
            "{}",
            "invoke: { tool: \"nika:decide\", args: { evidence: {} } }",
        ),
    );
}

/// KR-01 core: a path-carrying bundle — literal OR resolved/dynamic —
/// demands the SAME tools authority. The fs right alone never admits
/// the call.
#[test]
fn path_bundles_demand_the_tools_authority_literal_and_dynamic_alike() {
    let literal = decide_with(&format!("\"{P}\""));
    let dynamic = decide_with("\"${{ const.bundle }}\"");

    // literal path under the declared zero: the tools veto.
    let e = escapes_of(&wf("{}", &literal));
    assert_eq!(e.len(), 1, "literal path under `{{}}`: {e:?}");
    assert!(
        !e[0].undeclared,
        "a DECLARED block: the escapes_tool class, not AUTH-006"
    );

    // fs alone never suffices — literal or dynamic.
    assert_refused("fs-only · decide literal", &wf(&fs_only(), &literal));
    assert_refused(
        "fs-only · decide `${{ const.bundle }}` (the KR-01 repro)",
        &wf_const(&fs_only(), &dynamic),
    );

    // …and the dynamic bundle under the declared zero vetoes alike.
    assert_refused("`{}` · decide dynamic", &wf_const("{}", &dynamic));
}

/// The two rights stay independent: holding the tools grant does NOT
/// answer the fs question (the literal bundle then escapes on the fs
/// axis), and holding both admits the call.
#[test]
fn tools_and_fs_rights_stay_two_independent_questions() {
    let literal = decide_with(&format!("\"{P}\""));
    let e = escapes_of(&wf("{ tools: [\"nika:decide\"] }", &literal));
    assert!(
        e.iter().any(|x| x.category == "fs"),
        "tools granted · the PATH is still an fs question: {e:?}"
    );
    assert_clean(
        "tools + fs.read(P) · decide literal P",
        &wf(
            &format!("{{ tools: [\"nika:decide\"], fs: {{ read: [\"{P}\"] }} }}"),
            &literal,
        ),
    );
}

/// The absent block keeps its pre-existing chain: a literal effect
/// escapes, a dynamic one defers to the builtin's empty-boundary
/// backstop (no widening introduced here).
#[test]
fn the_absent_block_chain_is_unchanged() {
    assert_refused(
        "absent · decide literal path",
        &wf("absent", &decide_with(&format!("\"{P}\""))),
    );
    assert_clean(
        "absent · decide dynamic (pre-existing deferral)",
        &wf_const("absent", &decide_with("\"${{ const.bundle }}\"")),
    );
}

/// The agent twin: the whitelist-level exemption covers tools pure for
/// EVERY call — `decide` is call-dependent (its string bundle is the
/// model's choice at run), so the tools veto keeps it under any form;
/// the other class members stay exempt.
#[test]
fn the_agent_whitelist_exemption_covers_only_all_call_purity() {
    let agent = |tools: &str, permits: &str| {
        format!(
            "nika: w\nmodel: mock/echo\npermits: {permits}\ntasks:\n  a:\n    agent: {{ prompt: \"hi\", tools: [{tools}] }}\n"
        )
    };
    assert_clean("agent `{}` · [jq]", &agent("\"nika:jq\"", "{}"));
    assert_refused(
        "agent `{}` · [decide] (KR-01 agent arm)",
        &agent("\"nika:decide\"", "{}"),
    );
    assert_refused(
        "agent fs-only · [decide]",
        &agent("\"nika:decide\"", &fs_only()),
    );
    assert_clean(
        "agent tools-only · [decide] (granted)",
        &agent("\"nika:decide\"", "{ tools: [\"nika:decide\"] }"),
    );
    assert_refused(
        "agent `{}` · [write] (the effect class is intact)",
        &agent("\"nika:write\"", "{}"),
    );
}
