// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `run.access:` / `run.reasoning:` shape law: exact values on success,
//! the exact refusal class on every malformed shape, in both modes.

use nika_types::access::{AccessFallback, AccessProtocol};

use crate::parser::{ParseMode, parse};
use crate::source::FileId;
use crate::types::{RunAccess, RunClock, RunDecl, RunEntropy, RunReasoning};

const BASE: &str = "nika: demo\n";

/// The shared rich fixture (the language owner's preservation input).
const RICH: &str = include_str!("../../../tests/fixtures/run-access-rich.nika");

fn decl(yaml: &str) -> RunDecl {
    parse(yaml, FileId::new(0), ParseMode::Strict)
        .expect("parses")
        .run
        .expect("run block")
        .value
}

fn refused(yaml: &str) -> (String, String) {
    let err = parse(yaml, FileId::new(0), ParseMode::Strict).expect_err("refused");
    (err.spec_code().to_string(), err.to_string())
}

fn run_block(body: &str) -> String {
    format!("{BASE}run:\n{body}")
}

#[test]
fn the_rich_fixture_parses_every_authored_field_exactly() {
    let decl = decl(RICH);
    assert_eq!(decl.entropy, Some(RunEntropy::Seeded(42)));
    assert_eq!(decl.clock, Some(RunClock::Virtual));
    assert_eq!(
        decl.access,
        Some(RunAccess::new(
            Some("codex".to_owned()),
            Some(AccessProtocol::Acp),
            Some(AccessFallback::None),
        ))
    );
    assert_eq!(
        decl.reasoning,
        Some(RunReasoning::new(Some("xhigh".to_owned())))
    );
    let req = decl.access_requirement().expect("declared");
    assert_eq!(
        req.to_json(),
        serde_json::json!({"via":"codex","protocol":"acp","fallback":"none","effort":"xhigh"})
    );
}

#[test]
fn absent_blocks_keep_the_status_quo_and_entropy_still_parses() {
    let decl = decl(&run_block("  entropy: none\n"));
    assert_eq!(decl.entropy, Some(RunEntropy::None));
    assert_eq!(decl.access, None);
    assert_eq!(decl.reasoning, None);
    assert_eq!(decl.access_requirement(), None);
    let wf = parse(BASE, FileId::new(0), ParseMode::Strict).expect("parses");
    assert!(wf.run.is_none());
}

#[test]
fn each_access_key_is_optional_alone() {
    let only_via = decl(&run_block("  access: { via: claude-code }\n"));
    assert_eq!(
        only_via.access,
        Some(RunAccess::new(Some("claude-code".to_owned()), None, None))
    );
    let only_protocol = decl(&run_block("  access: { protocol: api }\n"));
    assert_eq!(
        only_protocol.access,
        Some(RunAccess::new(None, Some(AccessProtocol::Api), None))
    );
    let only_fallback = decl(&run_block("  access: { fallback: none }\n"));
    assert_eq!(
        only_fallback.access,
        Some(RunAccess::new(None, None, Some(AccessFallback::None)))
    );
    let only_effort = decl(&run_block("  reasoning: { effort: \"3\" }\n"));
    assert_eq!(
        only_effort.reasoning,
        Some(RunReasoning::new(Some("3".to_owned()))),
        "a quoted native value is the author's string, verbatim"
    );
    assert_eq!(only_effort.access, None);
}

#[test]
fn unknown_keys_are_refused_in_both_modes_with_a_suggestion() {
    for mode in [ParseMode::Strict, ParseMode::Lenient] {
        for (body, typo, near) in [
            ("  acess: { via: codex }\n", "acess", "access"),
            (
                "  access: { via: codex, protocl: acp }\n",
                "protocl",
                "protocol",
            ),
            ("  reasoning: { efort: high }\n", "efort", "effort"),
        ] {
            let err = parse(&run_block(body), FileId::new(0), mode).expect_err(typo);
            assert_eq!(err.spec_code().to_string(), "NIKA-PARSE-005", "{typo}");
            let text = err.to_string();
            assert!(text.contains(typo) && text.contains(near), "{text}");
        }
    }
}

#[test]
fn malformed_shapes_are_refused_with_the_failed_field() {
    let cases = [
        ("  access: codex\n", "run.access"),
        ("  access: {}\n", "selects nothing"),
        ("  reasoning: {}\n", "needs `effort:`"),
        ("  reasoning: high\n", "run.reasoning"),
        ("  access: { via: [codex] }\n", "run.access.via"),
        ("  access: { via: Codex }\n", "kebab-case"),
        ("  access: { via: claude_code }\n", "kebab-case"),
        ("  access: { via: 9lives }\n", "kebab-case"),
        ("  access: { via: \"\" }\n", "non-empty"),
        ("  access: { via: null }\n", "got null"),
        ("  access: { via: ~ }\n", "got null"),
        ("  access: { via: \" codex\" }\n", "verbatim"),
        (
            "  access: { via: \"${{ inputs.via }}\" }\n",
            "literal value",
        ),
        ("  access: { protocol: native }\n", "`api` or `acp`"),
        ("  access: { protocol: cli }\n", "`api` or `acp`"),
        ("  access: { protocol: ACP }\n", "`api` or `acp`"),
        ("  access: { fallback: api }\n", "only `none`"),
        ("  reasoning: { effort: 3 }\n", "YAML number"),
        ("  reasoning: { effort: true }\n", "YAML boolean"),
        ("  reasoning: { effort: \"  \" }\n", "non-empty"),
        (
            "  reasoning: { effort: \"${{ inputs.effort }}\" }\n",
            "literal value",
        ),
        (
            "  reasoning: { effort: { level: high } }\n",
            "must be a string",
        ),
    ];
    for (body, needle) in cases {
        let (code, text) = refused(&run_block(body));
        assert_eq!(code, "NIKA-PARSE-019", "{body}: {text}");
        assert!(text.contains(needle), "{body}: {text}");
    }
}

#[test]
fn the_determinism_law_still_judges_beside_an_access_block() {
    let (code, text) = refused(&run_block(
        "  entropy: ambient\n  clock: virtual\n  access: { via: codex }\n",
    ));
    assert_eq!(code, "NIKA-PARSE-026", "{text}");
    let (code, _) = refused(&run_block(
        "  entropy: none\n  clock: system\n  reasoning: { effort: high }\n",
    ));
    assert_eq!(code, "NIKA-PARSE-027");
}

#[test]
fn a_shape_error_in_access_is_reported_before_the_contradiction_law() {
    // The shape pass runs first: the author fixes the typo, then meets
    // the semantic law (one refusal at a time, the earliest layer).
    let (code, _) = refused(&run_block(
        "  entropy: ambient\n  clock: virtual\n  access: { protocol: cli }\n",
    ));
    assert_eq!(code, "NIKA-PARSE-019");
}
