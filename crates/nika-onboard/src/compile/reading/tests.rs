// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::compile::{CompileRequest, compile};
use serde_json::json;

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
    let unnamed = json!({"call": "plan", "result": {"stop_reason": "EndTurn",
        "usage_reported": true, "input_tokens": 100, "output_tokens": 30}, "reasoning": {
        "configured": null, "transmitted": {"thinking": null,
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
        json!({"call": "plan", "result": {"stop_reason": "EndTurn", "usage_reported": true,
            "input_tokens": 100, "output_tokens": 30}, "reasoning": {"configured": "max", "transmitted":
            {"thinking": "enabled", "effort": "max"}, "served": "unknown",
            "reasoning_tokens": 812, "response_model": "deepseek-v4-pro"}}),
        json!({"call": "repair", "result": {"failure_kind": "admission_refused"}, "reasoning":
            {"configured": "max", "transmitted": "unobserved", "served": "unknown",
            "reasoning_tokens": null, "response_model": null}}),
    ];
    // B19 F3: the refused repair is attempted, never counted as sent.
    let head = "\n  authoring backend: deepseek/deepseek-v4-pro · 2 calls attempted · 1 answered · 0 without an answer, may have been sent · 1 refused before sending · 1500 ms · 300 in / 90 out tokens\n  sent to: deepseek · host api.deepseek.com";
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

/// B19 F3 · the headline counts only what the receipt shows: the calls attempted, those answered,
/// those without an answer (a provider error or a timeout: they may have been sent) and those
/// refused before sending. When every attempt was refused locally, nothing was sent, and the
/// destination line says so; when none was answered, it says the calls may have been sent.
#[test]
fn a_receipt_never_counts_a_call_refused_before_sending_as_sent() {
    use serde_json::json;
    let mut receipt = AuthoringReceipt::new("vllm/reasoning-wire");
    receipt.calls = 1;
    receipt.elapsed_ms = 3;
    receipt.backend = Some(json!({"kind": "direct_api", "provider": "vllm",
        "host": "127.0.0.1", "base_url_overridden": true}));
    let failed = |call: &str, kind: &str| json!({"call": call, "result": {"failure_kind": kind}});
    receipt.context = vec![failed("native", "admission_refused")];
    assert_eq!(
        receipt_words(&receipt, "run"),
        "\n  authoring backend: vllm/reasoning-wire · 1 call attempted · 0 answered · 0 without an answer, may have been sent · 1 refused before sending · 3 ms\n  nothing was sent to: vllm · host 127.0.0.1 (base URL overridden: a gateway or a local server, not the provider's own API)\n  cost: the compiler meters tokens, not money · run"
    );
    receipt.calls = 3;
    receipt.context = vec![
        failed("plan", "timeout"),
        failed("native", "provider_error"),
        failed("repair", "admission_refused"),
    ];
    let words = receipt_words(&receipt, "run");
    assert!(
        words.contains(" · 3 calls attempted · 0 answered · 2 without an answer, may have been sent · 1 refused before sending · 3 ms"),
        "{words}"
    );
    assert!(
        words.contains("\n  possibly sent to: vllm · host 127.0.0.1"),
        "{words}"
    );
}

/// B19 review · a call is answered only when its record holds the provider's response
/// (`stop_reason`), never by subtracting the failures from the count. A counted call with no
/// record, a record with no result, and a failure this reader does not know are unobserved, and
/// the destination then says the calls may have been sent. A response that reported no usage is
/// still an answer. A receipt listing another number of records than it counts says so.
#[test]
fn a_receipt_counts_an_answer_only_from_a_recorded_response() {
    use serde_json::json;
    let mut receipt = AuthoringReceipt::new("deepseek/deepseek-v4-pro");
    receipt.elapsed_ms = 3;
    receipt.backend = Some(json!({"kind": "direct_api", "provider": "deepseek",
        "host": "api.deepseek.com", "base_url_overridden": false}));
    let answered = json!({"call": "plan", "result": {"stop_reason": "EndTurn",
        "usage_reported": false}});
    // A response without usage is an answer: the words are as before.
    receipt.calls = 1;
    receipt.context = vec![answered.clone()];
    assert_eq!(
        receipt_words(&receipt, "run"),
        "\n  authoring backend: deepseek/deepseek-v4-pro · 1 call · 3 ms\n  sent to: deepseek · host api.deepseek.com\n  cost: the compiler meters tokens, not money · run"
    );
    let unobserved = " · 1 call attempted · 0 answered · 0 without an answer, may have been sent · 0 refused before sending · 1 unobserved";
    let possibly = " · 3 ms\n  possibly sent to: deepseek · host api.deepseek.com";
    for (records, note) in [
        (vec![], " · the receipt records 0 calls"),
        (vec![json!({"call": "plan"})], ""),
        (
            vec![json!({"call": "plan", "result": {"failure_kind": "quota_exceeded"}})],
            "",
        ),
        (
            vec![json!({"call": "plan", "result": {"usage_reported": true}})],
            "",
        ),
    ] {
        receipt.context = records;
        let got = receipt_words(&receipt, "run");
        assert!(
            got.contains(&format!("{unobserved}{note}{possibly}")),
            "{got}"
        );
    }
    // Two counted, one recorded and answered: the other is unobserved, never answered.
    receipt.calls = 2;
    receipt.context = vec![answered.clone()];
    let got = receipt_words(&receipt, "run");
    assert!(
        got.contains(" · 2 calls attempted · 1 answered · 0 without an answer, may have been sent · 0 refused before sending · 1 unobserved · the receipt records 1 call · 3 ms\n  sent to: deepseek"),
        "{got}"
    );
    // More records than counted calls: the receipt's own inconsistency is said, not resolved.
    receipt.calls = 1;
    receipt.context = vec![answered.clone(), answered];
    let got = receipt_words(&receipt, "run");
    assert!(
        got.contains(" · 1 call attempted · 2 answered · 0 without an answer, may have been sent · 0 refused before sending · the receipt records 2 calls · 3 ms"),
        "{got}"
    );
    // A call that asked a level and holds no result says so on its own line.
    receipt.context = vec![json!({"call": "plan", "reasoning": {"configured": "max",
        "transmitted": "unobserved", "served": "unknown", "reasoning_tokens": null,
        "response_model": null}})];
    let got = receipt_words(&receipt, "run");
    assert!(
        got.contains(
            " · reasoning tokens not reported · result unobserved · response model not reported"
        ),
        "{got}"
    );
}

/// B19 review · a receipt that counts no call and records none names where calls would have
/// gone, never that anything was sent there: « nothing was sent to », with the words before it
/// unchanged.
#[test]
fn a_receipt_with_no_call_says_nothing_was_sent() {
    use serde_json::json;
    let mut receipt = AuthoringReceipt::new("deepseek/deepseek-v4-pro");
    receipt.elapsed_ms = 3;
    receipt.backend = Some(json!({"kind": "direct_api", "provider": "deepseek",
        "host": "api.deepseek.com", "base_url_overridden": false}));
    assert_eq!((receipt.calls, receipt.context.len()), (0, 0));
    assert_eq!(
        receipt_words(&receipt, "run"),
        "\n  authoring backend: deepseek/deepseek-v4-pro · 0 calls · 3 ms\n  nothing was sent to: deepseek · host api.deepseek.com\n  cost: the compiler meters tokens, not money · run"
    );
}

const TIME_HEAD: &str = "The authoring model did not answer within the call's time limit";

fn outcome(target: &str, message: &str, context: Vec<Value>) -> CompileOutcome {
    let mut out = compile(&CompileRequest::create(
        "Read ./a.md and do something clever with it, then write ./b.md",
    ))
    .expect("outcome");
    out.status = CompileStatus::Incomplete;
    out.candidate = None;
    out.questions.clear();
    let mut diagnostic = out.diagnostics.first().expect("unsettled finding").clone();
    diagnostic.kind = DiagnosticKind::Unknown;
    diagnostic.target = target.to_owned();
    diagnostic.message = message.to_owned();
    out.diagnostics = vec![diagnostic];
    let mut receipt = AuthoringReceipt::new("ollama/qwen3.5:4b".to_owned());
    receipt.calls = u32::try_from(context.len()).expect("small fixture");
    receipt.context = context;
    out.provenance.authoring = Some(receipt);
    out
}

fn call(result: &Value) -> Value {
    json!({"call": "plan", "result": result})
}

/// The legacy reader recognizes provider prose, which can quote a past timeout during a
/// present monetary/admission refusal. That phrase alone must not assert a current deadline.
#[test]
fn timeout_words_without_a_last_timeout_record_keep_the_budget_headline() {
    let reason = "the previous call timed out; this request was refused before any byte left";
    for context in [
        vec![],
        vec![call(&json!({"failure_kind": "admission_refused"}))],
        vec![call(&json!({"failure_kind": "provider_error"}))],
        vec![call(&json!({"stop_reason": "MaxTokens"}))],
        vec![
            call(&json!({"failure_kind": "timeout"})),
            call(&json!({"failure_kind": "admission_refused"})),
        ],
        vec![
            call(&json!({"failure_kind": "timeout"})),
            json!({"call": "repair"}),
        ],
    ] {
        let reading = Reading::of(outcome("authoring_provider", reason, context));
        assert!(matches!(&reading, Reading::BudgetExhausted(_)));
        assert_eq!(
            authoring_budget_headline(reading.outcome().provenance.authoring.as_ref()),
            "I couldn't finish a workflow I trust within the authoring budget"
        );
        assert_eq!(reasons(reading.outcome()), [reason]);
    }
    let mut absent = outcome("authoring_provider", reason, vec![]);
    absent.provenance.authoring = None;
    assert!(!authoring_budget_headline(absent.provenance.authoring.as_ref()).contains(TIME_HEAD));
}

/// Ordinary output, request, money and repair limits do not enter the timeout branch.
/// These are their existing structured fates; no limit, retry or classifier is changed.
#[test]
fn non_time_authoring_limits_are_not_relabelled_as_deadlines() {
    for (target, reason, result) in [
        (
            "authoring_provider",
            "The seat stopped at its output cap before the plan was complete",
            json!({"stop_reason": "MaxTokens"}),
        ),
        (
            "authoring_provider",
            "the authoring authority is spent (1 of 1 sent): this request was refused before any byte left; authorize more with --authoring-max-calls",
            json!({"failure_kind": "admission_refused"}),
        ),
        (
            "authoring_provider",
            "the authorized monetary allowance cannot cover this request",
            json!({"failure_kind": "admission_refused"}),
        ),
        (
            "authoring_plan",
            "No candidate settled within the permitted repair rounds",
            json!({"stop_reason": "EndTurn"}),
        ),
    ] {
        let reading = Reading::of(outcome(target, reason, vec![call(&result)]));
        assert!(
            !matches!(&reading, Reading::BudgetExhausted(_)),
            "{reading:?}"
        );
        assert!(
            !authoring_budget_headline(reading.outcome().provenance.authoring.as_ref())
                .contains(TIME_HEAD)
        );
        assert_eq!(reasons(reading.outcome()), [reason]);
    }
}

#[test]
fn decision_words_preserve_routes_seat_ledger_and_absence_as_recorded() {
    for (decision, expected) in [
        (json!({}), "\n  decision: route none recorded"),
        (json!({"route":"HOT"}), "\n  decision: route HOT"),
        (
            json!({"route":["HOT", null, "COLD"],"seat":{"model":"fixture/model"},"ledger":[{},{}]}),
            "\n  decision: route HOT → COLD · seat fixture/model · ledger 2 clauses",
        ),
        (
            json!({"route":false,"ledger":[{}]}),
            "\n  decision: route none recorded · ledger 1 clause",
        ),
    ] {
        let mut text = String::new();
        decision_words(&decision, &mut text);
        assert_eq!(text, expected);
    }
}

/// The ways on every candidate its verifier did not accept offers.
const NEXT: &str = "\n  describe a correction, or `/intelligence` for another authoring model (it also judges unless a decision model is set) · `/meaning` shows what was understood";

/// A candidate kept as the preview, its decision as given, and a `verify_held` finding of `kind`
/// when one is named.
fn held(decision: &Value, finding: Option<DiagnosticKind>) -> CompileOutcome {
    let mut out = compile(&CompileRequest::create("bounded-batch")).expect("compiles");
    out.candidate = Some("nika: held\ntasks: {}\n".to_owned());
    out.diagnostics.clear();
    out.provenance.decision = Some(decision.clone());
    if let Some(kind) = finding {
        nika_compile::finding(&mut out, kind, "verify_held", "held after a doubt");
    }
    out
}

/// What a held candidate says, by what the verifier's last verification found, model or not:
/// parts it found missing that the repairs did not settle (the first named, an ellipsis when
/// there are more), a rejection with no defect located (the request contested, or a held
/// candidate whose attempt reads no cause), or an abstention. Each offers a correction and
/// another authoring model, which also judges unless a decision model is set; none asks for the
/// request again, and nothing is held without a candidate.
#[test]
fn a_candidate_its_verifier_did_not_accept_says_what_the_verifier_found() {
    const DOUBTED: &str = "The workflow is built but not proposed: the verifier did not accept it and located no defect a repair could start from; nothing was written.";
    const ABSTAINED: &str = "The workflow is built but not proposed: the verifier read it and abstained (it neither accepted nor rejected it); nothing was written.";
    const ONE: &str = "The workflow is built but not proposed: the verifier found a part missing that the repairs did not settle: « write ./b.md »; nothing was written.";
    const SEVERAL: &str = "The workflow is built but not proposed: the verifier found parts missing that the repairs did not settle: « read ./a.md »…; nothing was written.";
    let request = "Read ./a.md and write ./b.md";
    let open = json!({"pending": {"open": [request]}});
    let attempts = |attempts: Value| {
        let mut decision = open.clone();
        decision["semantic_verification"] = attempts;
        decision
    };
    let applied = Some(DiagnosticKind::Applied);
    let rejected = json!([{"attempt": 0, "defects": [], "contested": [request], "unknown": [],
        "declined": true, "rejected": true}]);
    let abstained = json!([{"attempt": 0, "defects": [], "contested": [], "unknown": [request],
        "declined": true, "rejected": false}]);
    let one = json!([{"attempt": 0, "defects": ["write ./b.md"], "contested": [],
        "declined": true, "rejected": true}]);
    let several = json!([{"attempt": 1, "defects": ["read ./a.md", "write ./b.md"],
        "contested": [request], "declined": true, "rejected": true}]);
    let earlier_doubt = json!([{"attempt": 0, "defects": [], "contested": [request],
        "declined": true, "rejected": true},
        {"attempt": 1, "defects": [], "contested": [], "unknown": [request],
        "declined": true, "rejected": false}]);
    for (case, decision, finding, said) in [
        ("rejected", attempts(rejected.clone()), applied, DOUBTED),
        ("contested", attempts(rejected), None, DOUBTED),
        ("no attempt", json!({}), applied, DOUBTED),
        (
            "no cause",
            attempts(json!([{"attempt": 0}])),
            applied,
            DOUBTED,
        ),
        ("abstained", attempts(abstained), applied, ABSTAINED),
        ("one part", attempts(one), applied, ONE),
        ("several", attempts(several), applied, SEVERAL),
        ("the last", attempts(earlier_doubt), applied, ABSTAINED),
    ] {
        let out = held(&decision, finding);
        for has_model in [true, false] {
            let words = held_words(&out, has_model);
            assert_eq!(words, Some(format!("{said}{NEXT}")), "{case}");
        }
        let mut gone = out;
        gone.candidate = None;
        assert_eq!(held_words(&gone, true), None, "{case}: no candidate");
    }
}

/// Any other held finish keeps the words of a judge still to come: a pending whole request with
/// no applied `verify_held` and no last verification contested with no defect (an earlier
/// attempt's dispute, a defect alone, a contested part beside a defect); nothing open, nothing
/// held.
#[test]
fn a_candidate_no_verifier_declined_keeps_the_words_of_a_judge_to_come() {
    const AWAITED: &str = "The workflow is built but not proposed: the seat wrote this program, and only a judge this round can permit settles it against your whole request — no judgment made in this round settled it; nothing was written.\n  state the request again for another attempt, or `/intelligence` for another model · `/meaning` shows what was understood";
    const UNSEATED: &str = "The workflow is built but not proposed: the seat wrote this program, and only a judge this round can permit settles it against your whole request — this session has no authoring model to judge it; nothing was written.\n  `/intelligence` chooses one, then state the request again · `/meaning` shows what was understood";
    let request = "Read ./a.md and write ./b.md";
    let open = json!({"pending": {"open": [request]}});
    let attempts = |attempts: Value| {
        let mut decision = open.clone();
        decision["semantic_verification"] = attempts;
        decision
    };
    for awaited in [
        held(&open, None),
        held(&open, Some(DiagnosticKind::Unknown)),
        held(
            &attempts(
                json!([{"attempt": 0, "contested": [request]}, {"attempt": 1, "contested": []}]),
            ),
            None,
        ),
        held(
            &attempts(json!([{"attempt": 0, "defects": ["write ./b.md"], "contested": []}])),
            None,
        ),
        held(
            &attempts(
                json!([{"attempt": 0, "defects": ["write ./b.md"], "contested": ["read ./a.md"]}]),
            ),
            None,
        ),
    ] {
        assert_eq!(held_words(&awaited, true).as_deref(), Some(AWAITED));
        assert_eq!(held_words(&awaited, false).as_deref(), Some(UNSEATED));
    }
    assert_eq!(
        held_words(&held(&json!({}), None), true),
        None,
        "nothing is open"
    );
}

/// A candidate the compiler held for its verifier is read as held before any provider finding
/// or question: a judge call that timed out, or was refused, while the verifier located what the
/// bytes lack stopped that localization and settles nothing, so the reading is `Unsettled` and
/// its words are the held ones, never the recovery card nor a question about rejected bytes.
/// The same outcome without the compiler's applied `verify_held` finding keeps its question,
/// budget and provider readings.
#[test]
fn a_held_candidate_is_read_as_held_before_any_provider_finding_or_question() {
    const DOUBTED: &str = "The workflow is built but not proposed: the verifier did not accept it and located no defect a repair could start from; nothing was written.";
    let request = "Read ./a.md and write ./b.md";
    let decision = json!({"pending": {"open": [request]}, "semantic_verification": [
        {"attempt": 0, "defects": [], "contested": [request], "unknown": [request],
         "declined": true, "rejected": true, "settled": false, "stopped": true}]});
    let timed_out = "the judge call timed out after 120 s";
    let refused = "the authoring authority is spent (7 of 7 sent): this request was refused before any byte left";
    let provider = |marked: bool, message: &str| {
        let mut out = held_or_not(&decision, marked);
        out.questions.clear();
        nika_compile::finding(
            &mut out,
            DiagnosticKind::Unknown,
            "authoring_provider",
            message,
        );
        out
    };
    // Held: the question a skeleton leaves, a timeout, a refusal all read as held.
    let asking = held_or_not(&decision, true);
    assert!(
        asking.questions.iter().any(|q| q.mandatory),
        "the fixture asks"
    );
    for (case, out) in [
        ("a question", asking),
        ("a timeout", provider(true, timed_out)),
        ("a refusal", provider(true, refused)),
    ] {
        let reading = Reading::of(out);
        assert!(
            matches!(&reading, Reading::Unsettled(_)),
            "{case}: {reading:?}"
        );
        let words = held_words(reading.outcome(), true);
        assert_eq!(words, Some(format!("{DOUBTED}{NEXT}")), "{case}");
    }
    // Not held: the same outcomes keep their own readings.
    assert!(matches!(
        Reading::of(held_or_not(&decision, false)),
        Reading::Questions(_)
    ));
    assert!(matches!(
        Reading::of(provider(false, timed_out)),
        Reading::BudgetExhausted(_)
    ));
    assert!(matches!(
        Reading::of(provider(false, refused)),
        Reading::ProviderFailed(_)
    ));
}

/// [`held`]'s candidate with the compiler's applied `verify_held` finding when `marked`, else
/// with none: the bounded-batch skeleton's mandatory questions left as they are.
fn held_or_not(decision: &Value, marked: bool) -> CompileOutcome {
    let out = held(decision, marked.then_some(DiagnosticKind::Applied));
    assert_eq!(out.status, CompileStatus::Incomplete);
    out
}
