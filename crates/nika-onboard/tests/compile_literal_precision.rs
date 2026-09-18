// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Integer-answer precision at the public Compile door. The canonical reader
//! holds an integer exactly only as `i64`; an integer token beyond it is refused
//! before any decoder can round it. No files or providers are used.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nika_onboard::compile::{
    CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, compile,
};
use nika_schema::{FileId, ParseMode};
use serde_json::{Value, json};

const SOURCE: &str = "# licence stays here\nnika: literal-precision\nconst:\n  payload: 0 # inline stays\n  untouched: 7\npermits: {tools: ['nika:jq']}\ntasks:\n  echo:\n    invoke:\n      tool: nika:jq\n      args: {input: '${{ const.payload }}', expression: '.'}\n# end\n";

/// `(answer, the integer token that cannot be held exactly)`.
const OVERSIZED: [(&str, &str); 13] = [
    ("18446744073709551616", "18446744073709551616"),
    ("12345678901234567890123", "12345678901234567890123"),
    ("18446744073709551615", "18446744073709551615"),
    ("9223372036854775808", "9223372036854775808"),
    ("-9223372036854775809", "-9223372036854775809"),
    ("-18446744073709551616", "-18446744073709551616"),
    ("-12345678901234567890123", "-12345678901234567890123"),
    (" 18446744073709551616\n", "18446744073709551616"),
    ("[1, 18446744073709551616]", "18446744073709551616"),
    (
        r#"{"a": {"b": [true, 12345678901234567890123]}}"#,
        "12345678901234567890123",
    ),
    (
        r#"{"18446744073709551616": "kept", "n": -9223372036854775809}"#,
        "-9223372036854775809",
    ),
    // The scanner must leave each string at its real closing quote.
    (
        r#"["\\", "\"", "1", 18446744073709551616]"#,
        "18446744073709551616",
    ),
    // The same magnitude is judged by its token: the exponent form is a float.
    (
        "[1e22, -2.5E-3, 10000000000000000000000]",
        "10000000000000000000000",
    ),
];

fn canonical_constant(source: &str, name: &str) -> Value {
    let wf = nika_schema::parse(source, FileId::new(0), ParseMode::Strict).unwrap();
    let (_, declaration) = wf.consts.iter().find(|(key, _)| key.value == name).unwrap();
    match declaration {
        nika_schema::VarDecl::Untyped(value) => value.clone(),
        nika_schema::VarDecl::Typed { default, .. } => default.clone().unwrap(),
    }
}

/// Every EDIT frontend: inline text, question then answer, structured operation.
fn edit_doors(source: &str, literal: &str) -> [(&'static str, CompileOutcome); 3] {
    [
        (
            "text",
            CompileRequest::edit(source, format!("Set const.payload to {literal}")),
        ),
        (
            "answer",
            CompileRequest::edit(source, "Set const.payload").answer("const.payload", literal),
        ),
        (
            "structured",
            CompileRequest::set_constant(source, "payload", literal),
        ),
    ]
    .map(|(door, request)| (door, compile(&request).unwrap()))
}

fn create_door(literal: &str) -> CompileOutcome {
    compile(&CompileRequest::create("classify-and-route").answer("const.request", literal)).unwrap()
}

fn assert_precision_refusal(out: &CompileOutcome, key: &str, token: &str, original: &str) {
    let seen = (out.status, &out.diagnostics);
    assert_eq!(out.status, CompileStatus::Refused, "{token}: {seen:?}");
    assert_eq!(out.candidate.as_deref(), Some(original), "{token}");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Applied),
        "{token}: {seen:?}"
    );
    // The answer itself is refused, naming the exact token: not a later
    // emission mismatch, and never a neighbouring quoted string.
    let refusals: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| d.kind == DiagnosticKind::Refused)
        .collect();
    assert_eq!(refusals.len(), 1, "{token}: {seen:?}");
    assert_eq!(refusals[0].target, key, "{token}");
    assert!(
        refusals[0].message.contains(&format!("`{token}`")),
        "{token}: {}",
        refusals[0].message
    );
    assert!(
        out.questions.iter().any(|q| q.key == key && q.mandatory),
        "{token}: {:?}",
        out.questions
    );
}

/// Ready at every owned door, holding exactly the value JSON gives the answer.
fn assert_accepted(literal: &str) {
    let expected: Value = serde_json::from_str(literal).unwrap();
    for (door, out) in edit_doors(SOURCE, literal) {
        let seen = (out.status, &out.diagnostics);
        assert_eq!(
            out.status,
            CompileStatus::Ready,
            "{door} {literal}: {seen:?}"
        );
        let candidate = out.candidate.as_deref().unwrap();
        assert_eq!(canonical_constant(candidate, "payload"), expected);
        assert_eq!(canonical_constant(candidate, "untouched"), json!(7));
        assert!(
            candidate.starts_with("# licence stays here\n"),
            "{candidate}"
        );
        assert!(candidate.contains(" # inline stays\n"), "{candidate}");
        assert!(candidate.ends_with("# end\n"), "{candidate}");
    }
    let created = create_door(literal);
    let seen = (created.status, &created.diagnostics);
    assert_eq!(created.status, CompileStatus::Ready, "{literal}: {seen:?}");
    assert_eq!(
        canonical_constant(created.candidate.as_deref().unwrap(), "request"),
        expected
    );
}

#[test]
fn oversized_integer_answers_are_refused_before_rounding_at_every_edit_door() {
    for (literal, token) in OVERSIZED {
        let outcomes = edit_doors(SOURCE, literal);
        for (_, out) in &outcomes {
            assert_precision_refusal(out, "const.payload", token, SOURCE);
        }
        // One guarded answer path: the frontends cannot disagree about a refusal.
        let [(_, text), (_, answer), (_, structured)] = &outcomes;
        assert_eq!(text.diagnostics, structured.diagnostics, "{literal}");
        assert_eq!(text.diagnostics, answer.diagnostics, "{literal}");
    }
}

#[test]
fn oversized_integer_answers_are_refused_at_the_create_literal_door() {
    let template = nika_pack::template("classify-and-route").unwrap();
    for (literal, token) in OVERSIZED {
        assert_precision_refusal(&create_door(literal), "const.request", token, template);
    }
}

#[test]
fn typed_constants_and_text_holes_never_accept_an_oversized_integer() {
    // A whole f64 satisfies `type: integer`, so Check cannot catch the rounding.
    let typed = SOURCE.replace(
        "payload: 0 # inline stays",
        "payload: {type: integer, value: 0} # inline stays",
    );
    for (_, out) in edit_doors(&typed, "18446744073709551616") {
        assert_precision_refusal(&out, "const.payload", "18446744073709551616", &typed);
    }
    let pending = compile(&CompileRequest::create("chain")).unwrap();
    let key = &pending
        .questions
        .iter()
        .find(|q| q.key.starts_with("tasks."))
        .expect("prompt hole")
        .key;
    let out =
        compile(&CompileRequest::create("chain").answer(key, "12345678901234567890123")).unwrap();
    assert_precision_refusal(
        &out,
        key,
        "12345678901234567890123",
        pending.candidate.as_deref().unwrap(),
    );
}

#[test]
fn exact_domain_boundaries_and_ordinary_literals_remain_accepted() {
    for literal in [
        "9223372036854775807",
        "-9223372036854775808",
        "-42",
        "250",
        "true",
        "null",
        r#"[9223372036854775807, -9223372036854775808, {"n": [1.5, 7]}]"#,
    ] {
        assert_accepted(literal);
    }
}

#[test]
fn fraction_and_exponent_tokens_keep_the_float_contract() {
    // A float answer is an f64 by contract; only integer tokens are judged here.
    for literal in [
        "2.25",
        "-0.5",
        "1e3",
        "1e22",
        "1E+22",
        "18446744073709551616.0",
        "1.8446744073709552e19",
        "-9223372036854775808.0",
        r#"[1e3, {"n": -2.5E-3}]"#,
    ] {
        assert_accepted(literal);
    }
}

#[test]
fn digit_text_inside_strings_and_keys_is_never_judged_as_a_number() {
    for literal in [
        r#""18446744073709551616""#,
        r#""-12345678901234567890123""#,
        r#""say \"18446744073709551616\" now""#,
        r#""tail \\18446744073709551616""#,
        r#"["12345678901234567890123", {"18446744073709551616": "-9223372036854775809"}, 7]"#,
    ] {
        assert_accepted(literal);
    }
}
