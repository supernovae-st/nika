// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `run.access:` / `run.reasoning:` shape law: exact values on success,
//! the exact refusal class on every malformed shape, in both modes.

use nika_types::access::{AccessFallback, AccessProtocol, ServiceTier};

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

/// The rich fixture declares no tier: it is the omission witness, and its
/// exact values above must not move when the key exists.
#[test]
fn the_rich_fixture_declares_no_service_tier() {
    let access = decl(RICH).access.expect("an access block");
    assert_eq!(access.service_tier, None);
}

#[test]
fn a_declared_service_tier_parses_to_its_exact_word() {
    for (word, tier) in [
        ("fast", ServiceTier::Fast),
        ("standard", ServiceTier::Standard),
    ] {
        let parsed = decl(&run_block(&format!(
            "  access: {{ via: codex, protocol: acp, service_tier: {word} }}\n"
        )));
        assert_eq!(
            parsed.access,
            Some(
                RunAccess::new(Some("codex".to_owned()), Some(AccessProtocol::Acp), None)
                    .with_service_tier(Some(tier))
            ),
            "{word}"
        );
        let req = parsed.access_requirement().expect("declared");
        assert_eq!(
            req.to_json(),
            serde_json::json!({"via":"codex","protocol":"acp","service_tier": word})
        );
    }
    // Each access key is optional on its own, the tier included.
    let alone = decl(&run_block("  access: { service_tier: fast }\n"));
    assert_eq!(
        alone.access,
        Some(RunAccess::default().with_service_tier(Some(ServiceTier::Fast)))
    );
    assert_eq!(
        alone.access_requirement().expect("declared").identity(),
        "service_tier=fast"
    );
    // A quoted word is the same literal.
    let quoted = decl(&run_block("  access: { service_tier: \"standard\" }\n"));
    assert_eq!(
        quoted.access.and_then(|access| access.service_tier),
        Some(ServiceTier::Standard)
    );
}

/// The tier is two exact words. A route's own word for its speed control,
/// another case, a boolean, a template or a non-scalar is refused with the
/// failed field named, never mapped.
#[test]
fn a_malformed_service_tier_is_refused_with_the_failed_field() {
    let cases = [
        (
            "  access: { service_tier: priority }\n",
            "`standard` or `fast`",
        ),
        ("  access: { service_tier: Fast }\n", "`standard` or `fast`"),
        ("  access: { service_tier: on }\n", "`standard` or `fast`"),
        ("  access: { service_tier: off }\n", "`standard` or `fast`"),
        (
            "  access: { service_tier: \"true\" }\n",
            "`standard` or `fast`",
        ),
        ("  access: { service_tier: true }\n", "YAML boolean"),
        ("  access: { service_tier: 1 }\n", "YAML number"),
        ("  access: { service_tier: \"\" }\n", "non-empty"),
        ("  access: { service_tier: null }\n", "got null"),
        ("  access: { service_tier: \" fast\" }\n", "verbatim"),
        (
            "  access: { service_tier: \"${{ inputs.tier }}\" }\n",
            "literal value",
        ),
        (
            "  access: { service_tier: [fast] }\n",
            "run.access.service_tier",
        ),
        (
            "  access: { service_tier: { tier: fast } }\n",
            "must be a string",
        ),
    ];
    for (body, needle) in cases {
        let (code, text) = refused(&run_block(body));
        assert_eq!(code, "NIKA-PARSE-019", "{body}: {text}");
        assert!(text.contains(needle), "{body}: {text}");
        assert!(text.contains("run.access.service_tier"), "{body}: {text}");
    }
}

/// The key belongs to `run.access` and nowhere else: under `run:`, under
/// `run.reasoning:` or misspelled inside `access:`, it is an unknown key in
/// both parse modes, with the near key named.
#[test]
fn a_misplaced_or_misspelled_service_tier_key_is_refused_in_both_modes() {
    for mode in [ParseMode::Strict, ParseMode::Lenient] {
        for (body, key) in [
            ("  service_tier: fast\n", "service_tier"),
            (
                "  reasoning: { effort: low, service_tier: fast }\n",
                "service_tier",
            ),
            (
                "  access: { via: codex, service-tier: fast }\n",
                "service-tier",
            ),
            (
                "  access: { via: codex, serviceTier: fast }\n",
                "serviceTier",
            ),
        ] {
            let err = parse(&run_block(body), FileId::new(0), mode).expect_err(key);
            assert_eq!(err.spec_code().to_string(), "NIKA-PARSE-005", "{body}");
            assert!(err.to_string().contains(key), "{body}: {err}");
        }
    }
    // Inside `access:` the near key is offered, so `check --fix` can splice it.
    let err = parse(
        &run_block("  access: { via: codex, service-tier: fast }\n"),
        FileId::new(0),
        ParseMode::Strict,
    )
    .expect_err("an unknown key");
    assert!(err.to_string().contains("service_tier"), "{err}");
}

#[test]
fn the_empty_access_block_teaches_every_key_it_takes() {
    let (code, text) = refused(&run_block("  access: {}\n"));
    assert_eq!(code, "NIKA-PARSE-019");
    for key in ["`via:`", "`protocol:`", "`fallback:`", "`service_tier:`"] {
        assert!(text.contains(key), "{key}: {text}");
    }
}

#[test]
fn the_determinism_law_still_judges_beside_a_service_tier() {
    let (code, text) = refused(&run_block(
        "  entropy: ambient\n  clock: virtual\n  access: { service_tier: fast }\n",
    ));
    assert_eq!(code, "NIKA-PARSE-026", "{text}");
    let seeded = decl(&run_block(
        "  entropy: { seeded: 7 }\n  clock: virtual\n  access: { service_tier: standard }\n",
    ));
    assert_eq!(seeded.entropy, Some(RunEntropy::Seeded(7)));
    assert_eq!(
        seeded.access.and_then(|access| access.service_tier),
        Some(ServiceTier::Standard)
    );
}
