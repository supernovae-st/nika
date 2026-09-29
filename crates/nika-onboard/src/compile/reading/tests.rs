// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::compile::{CompileRequest, compile};

fn read(intent: &str) -> Reading {
    Reading::of(compile(&CompileRequest::create(intent)).expect("compiles"))
}

#[test]
fn the_reading_is_the_compilers_typed_fields() {
    assert!(matches!(
        read("hello there, how are you today?"),
        Reading::NotWork(_)
    ));
    assert!(matches!(
        read("Read ./notes/brief.md and write it to ./out/copy.md"),
        Reading::Ready(_)
    ));
    match read(
        "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md",
    ) {
        Reading::Questions(out) => {
            assert_eq!(out.questions[0].key, "model");
            assert!(
                out.provenance.plan.is_some(),
                "the HOT plan is recorded for replay"
            );
        }
        other => panic!("a draft needs its model, asked: {other:?}"),
    }
    assert!(matches!(
        read("Read ./a.md and do something clever with it, then write ./b.md"),
        Reading::Unsettled(_)
    ));
    match read("chain") {
        Reading::Questions(out) => assert_eq!(out.questions[0].key, "tasks.think.infer.prompt"),
        other => panic!("a skeleton with holes asks: {other:?}"),
    }
}

/// The compiler's own questions of each shape: a skeleton's prompt
/// hole is text, a skeleton's constant hole is a literal.
fn question_of(skeleton: &str, shape: QuestionType) -> CompileQuestion {
    let out = compile(&CompileRequest::create(skeleton)).expect("compiles");
    let question = out.questions.into_iter().next().expect("one hole");
    assert_eq!(question.answer_type, shape, "{skeleton}");
    question
}

#[test]
fn a_line_is_typed_to_the_questions_shape() {
    let text = question_of("chain", QuestionType::Text);
    let literal = question_of("bounded-batch", QuestionType::Literal);
    assert_eq!(literal_for(&text, "mock/echo"), "\"mock/echo\"");
    assert_eq!(literal_for(&text, " 5 "), "\"5\"");
    assert_eq!(literal_for(&literal, "5"), "5");
    assert_eq!(literal_for(&literal, "true"), "true");
    assert_eq!(literal_for(&literal, "./notes"), "\"./notes\"");
    assert_eq!(literal_for(&literal, "[\"a\"]"), "[\"a\"]");
}

#[test]
fn the_reasons_are_the_compilers_own_findings() {
    let unsettled = compile(&CompileRequest::create(
        "Read ./a.md and do something clever with it, then write ./b.md",
    ))
    .expect("compiles");
    let said = reasons(&unsettled);
    let findings: Vec<String> = unsettled
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.kind,
                DiagnosticKind::Unknown | DiagnosticKind::Missed | DiagnosticKind::Refused
            )
        })
        .map(|d| d.message.clone())
        .collect();
    assert_eq!(
        said, findings,
        "exactly the unknown · missed · refused ones"
    );
    let ready = compile(&CompileRequest::create(
        "Read ./notes/brief.md and write it to ./out/copy.md",
    ))
    .expect("compiles");
    assert!(
        reasons(&ready).is_empty(),
        "a Ready candidate states no reason"
    );
}

/// C10 · D-R · an answered `intent.clarification` replaces the request only with words: a
/// string that is not blank, never another literal.
#[test]
fn a_clarification_gives_words_only_when_it_is_a_string_that_is_not_blank() {
    let one = |literal: &str| {
        std::collections::BTreeMap::from([(CLARIFICATION_KEY.to_owned(), literal.to_owned())])
    };
    assert_eq!(
        clarified(&one("\"the whole request\"")).as_deref(),
        Some("the whole request")
    );
    for literal in ["\"  \"", "42", "not json", "null"] {
        assert_eq!(clarified(&one(literal)), None, "{literal}");
    }
    assert_eq!(clarified(&std::collections::BTreeMap::new()), None);
}

/// C10 · D-H · the compiler's fidelity grammar in a human's words, beside the reading it
/// refines: machine sentences dropped, duplicates folded, each closed form said plainly, a cut
/// answer named as an internal limit, anything else as the compiler said it.
#[test]
fn the_compilers_reasons_read_in_a_humans_words() {
    let said = human_reasons(
        [
            "the semantic plan has no step",
            "unmapped part: .",
            "Candidate 2 is not feasible: dropped the recognized operation `sum` (the total of amount)",
            "the path `./out.csv` is no longer carried by the candidate",
            "the literal `42` is not in the request",
            "the answer was cut: raise --authoring-max-tokens above 16384",
            "a reason as the compiler said it.",
            "a reason as the compiler said it",
        ]
        .map(str::to_owned)
        .to_vec(),
    );
    assert_eq!(
        said,
        [
            "the draft lost « the total of amount » (the sum step)",
            "the draft dropped « ./out.csv »: nothing reads or writes it any more",
            "the draft invented a value (« 42 ») your request never gave",
            "the model's answer was cut at its 16384-token output limit before it was complete — an internal limit of this attempt, not a problem with your request",
            "a reason as the compiler said it",
        ]
    );
}

/// C10 · D-H · a question's own grammar: the clause it quotes, whether it asks for code, and how
/// many clauses the outcome's ledger holds.
#[test]
fn a_question_quotes_its_clause_and_says_when_it_asks_for_code() {
    assert_eq!(
        clause_of("How should `the total of amount` be computed?").as_deref(),
        Some("the total of amount")
    );
    assert_eq!(clause_of("no quoted clause"), None);
    assert_eq!(clause_of("an empty `  ` clause"), None);
    let mut out = compile(&CompileRequest::create("bounded-batch")).expect("compiles");
    let mut question = out.questions[0].clone();
    assert!(!asks_for_syntax(&question), "{}", question.label);
    question.key = "const.rule_expression".to_owned();
    assert!(asks_for_syntax(&question), "a const expression is code");
    question.key = "value".to_owned();
    question.label = "Which jq expression keeps the rows?".to_owned();
    assert!(asks_for_syntax(&question), "a jq expression is code");
    out.provenance.decision = Some(serde_json::json!({"ledger": [{}, {}]}));
    assert_eq!(clauses_understood(&out), Some(2));
    out.provenance.decision = Some(serde_json::json!({"ledger": []}));
    assert_eq!(
        clauses_understood(&out),
        None,
        "an empty ledger says nothing"
    );
}

/// C11 · an outcome's words a host reads before its own protocol words: the question as a human
/// reads it, the syntax question in words, the honest incomplete. No host command rides them.
#[test]
fn an_outcomes_words_leave_the_hosts_protocol_to_the_host() {
    let skeleton = compile(&CompileRequest::create("bounded-batch")).expect("compiles");
    let mut question = skeleton.questions[0].clone();
    question.label = "Which column holds the amount?".to_owned();
    question.why = "the request names no column".to_owned();
    assert_eq!(
        question_words(&question, &["no column is named".to_owned()]),
        "Which column holds the amount?\n  (the request names no column)\n  what I could not settle:\n    · no column is named"
    );
    question.why.clear();
    assert_eq!(
        question_words(&question, &[]),
        "Which column holds the amount?"
    );
    let asked = syntax_question("sum of amount");
    assert!(
        asked.contains("how to do « sum of amount »") && asked.ends_with("No code is needed."),
        "{asked}"
    );
    let out = compile(&CompileRequest::create("do the thing with the stuff")).expect("compiles");
    let incomplete = incomplete_words(&out, None);
    assert!(
        incomplete.starts_with("I read this as work but cannot build it yet:")
            && incomplete.ends_with("write it to ./digest.md »"),
        "{incomplete}"
    );
    assert!(incomplete_words(&out, Some("the way on")).ends_with("\n  the way on"));
    for words in [&asked, &incomplete] {
        assert!(
            !words.contains("cancel") && !words.contains("why?"),
            "{words}"
        );
    }
}

/// C11 · the authoring receipt's words: a call that asked no level adds nothing (the metered and
/// subscription bytes as `/details` said them before); a call that asked one says each fact apart
/// (configured, the keys read back from the sent body, served unknown, the reported usage and
/// model), and a body never read back is `unobserved`, never the level.
#[test]
fn a_receipt_says_each_explicit_effort_fact_apart_and_nothing_for_none() {
    use serde_json::json;
    let mut receipt = AuthoringReceipt::new("deepseek/deepseek-v4-pro");
    receipt.calls = 2;
    receipt.input_tokens = Some(300);
    receipt.output_tokens = Some(90);
    receipt.elapsed_ms = 1500;
    receipt.backend = Some(json!({"kind": "direct_api", "provider": "deepseek",
        "host": "api.deepseek.com", "base_url_overridden": false}));
    let unnamed = json!({"call": "plan", "result": {"usage_reported": true, "input_tokens": 100,
        "output_tokens": 30}, "reasoning": {"configured": null, "transmitted": {"thinking": null,
        "effort": "low"}, "served": "unknown", "reasoning_tokens": null,
        "response_model": "deepseek-v4-pro"}});
    receipt.context = vec![unnamed.clone(), unnamed];
    let head = "\n  authoring backend: deepseek/deepseek-v4-pro · 2 calls · 1500 ms · 300 in / 90 out tokens\n  sent to: deepseek · host api.deepseek.com";
    let tail = "\n  cost: the compiler meters tokens, not money · the host's own words";
    assert_eq!(
        receipt_words(&receipt, "the host's own words"),
        format!("{head}{tail}")
    );
    receipt.context = vec![
        json!({"call": "plan", "result": {"usage_reported": true, "input_tokens": 100,
            "output_tokens": 30}, "reasoning": {"configured": "max", "transmitted":
            {"thinking": "enabled", "effort": "max"}, "served": "unknown",
            "reasoning_tokens": 812, "response_model": "deepseek-v4-pro"}}),
        json!({"call": "repair", "result": {"failure_kind": "admission_refused"}, "reasoning":
            {"configured": "max", "transmitted": "unobserved", "served": "unknown",
            "reasoning_tokens": null, "response_model": null}}),
    ];
    assert_eq!(
        receipt_words(&receipt, "the host's own words"),
        format!(
            "{head}\n    plan call · reasoning effort max configured · keys read back from the sent body: thinking enabled · effort max · effort served unknown · reasoning tokens 812 · usage 100 in / 30 out tokens · response model deepseek-v4-pro\n    repair call · reasoning effort max configured · keys read back from the sent body: unobserved · effort served unknown · reasoning tokens not reported · no answer (admission_refused) · response model not reported{tail}"
        )
    );
    receipt.backend = Some(json!({"kind": "direct_api", "provider": "openai",
        "host": "gateway.invalid", "base_url_overridden": true}));
    assert!(
        receipt_words(&receipt, "")
            .contains("sent to: openai · host gateway.invalid (base URL overridden: a gateway or a local server, not the provider's own API)")
    );
    let mut subscription = AuthoringReceipt::new("claude-code");
    subscription.calls = 1;
    subscription.elapsed_ms = 20;
    subscription.backend = Some(json!({"kind": "harness_infer", "adapter": "claude-code",
        "requested_model": null, "carried_from_authoring_round": true, "observed": [
        {"status": "returned", "observed_model": "opus", "usage_observed": true},
        {"status": "failed"}]}));
    assert_eq!(
        receipt_words(&subscription, "the host's own words"),
        "\n  authoring backend: subscription claude-code · requested harness default · 1 compiler calls · 20 ms\n    receipt carried from the authoring round; this clarification replay made zero calls\n    responding model: opus · usage marker true\n  cost: subscription invoice unknown · no numeric token meter reported · no paid provider fallback"
    );
}
