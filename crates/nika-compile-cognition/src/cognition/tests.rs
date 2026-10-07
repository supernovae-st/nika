// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::{Objects, UNCLOSED_RETRIES, answer_objects, first_json_object};
use nika_kernel::ai::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};

const A: &str = r#"{"steps": [], "note": "a {brace} and a \" quote in a string"}"#;
const B: &str = r#"{"steps": [{"op": "read"}]}"#;
const EXAMPLE: &str = r#"{"status": "paid"}"#;

/// The answer's own type, for these tests: an object that carries `steps`.
fn is_plan(object: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(object).is_ok_and(|v| v.get("steps").is_some())
}

fn read(text: &str) -> Option<&str> {
    match answer_objects(text, is_plan) {
        Objects::One { answer, .. } => Some(answer),
        _ => None,
    }
}

#[test]
fn one_answer_is_read_through_prose_repetitions_templates_and_examples() {
    for text in [
        A.to_owned(),
        format!("Sure!\n```json\n{A}\n```"),
        // The same answer twice, bare or in prose, is one answer.
        format!("{A}\n{A}"),
        format!("Draft {A} final {A}"),
        // Template braces, an empty object and a closing prose brace are not answers.
        format!("{A}\nIt reads ${{{{ with.content }}}}, keeps permits: {{}}, ends {{name}}"),
        format!("It uses ${{{{ with.content }}}} before the answer:\n{A}"),
        // An example that cannot be an answer never kills the one answer.
        format!("For example {EXAMPLE}, then {A}"),
        format!("{A} then {{ an unclosed prose brace"),
    ] {
        assert_eq!(read(&text), Some(A), "{text}");
    }
    let beside_example = format!("E.g. {EXAMPLE}: {A}");
    let Objects::One { unread, .. } = answer_objects(&beside_example, is_plan) else {
        panic!("one answer beside an example");
    };
    assert_eq!(unread, [EXAMPLE], "kept by digest, never read");
    assert_eq!(
        first_json_object(&format!("Sure!\n```json\n{A}\n```")),
        Some(A)
    );
}

#[test]
fn two_answers_or_one_beside_a_cut_object_are_never_resolved_by_reading_the_first() {
    for text in [
        format!("Draft {A} final {B}"),
        format!("{A}\n{B}"),
        format!("```json\n{B}\n```\n```json\n{A}\n```"),
        format!("{A} then {{ an unclosed brace, then {B}"),
    ] {
        assert!(
            matches!(answer_objects(&text, is_plan), Objects::Two(ref objects) if objects.len() == 2),
            "{text}"
        );
    }
    for text in [
        // A competitor that opens like an object and never closes, after or before.
        format!("Draft:\n{A}\nFinal:\n{{\"steps\": [{{\"op\": \"write\""),
        format!("Draft:\n{{ \"steps\": [\nFinal:\n{A}"),
        // Past the retry bound, the rest of the text is not judged: never one answer.
        format!("{A}{}", " {".repeat(UNCLOSED_RETRIES + 1)),
    ] {
        assert!(
            matches!(answer_objects(&text, is_plan), Objects::Undecided(ref objects) if objects == &[A]),
            "{text}"
        );
    }
    assert_eq!(
        read(&format!("{A}{}", " {".repeat(UNCLOSED_RETRIES))),
        Some(A)
    );
    // Without a complete object, the text keeps its syntax path.
    assert!(matches!(
        answer_objects("{\"steps\": !}", is_plan),
        Objects::None
    ));
    assert!(matches!(
        answer_objects("no object", is_plan),
        Objects::None
    ));
}

#[test]
fn a_cold_plan_is_read_beside_an_example_and_never_beside_another_plan() {
    let plan = r#"{"steps":[],"effects":[],"obligations":[],"constraints":[],"unknowns":[]}"#;
    let other = r#"{"steps":[{"op":"draft","detail":"x","evidence":"x"}],"effects":[],"obligations":[],"constraints":[],"unknowns":[]}"#;
    let response = |text: String| {
        InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        )
    };
    let mut out = crate::initial();
    let two = response(format!("Plan A:\n{plan}\nPlan B:\n{other}"));
    assert!(super::proposal::decode(&two, &mut out).is_none());
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("two plans")),
        "{out:#?}"
    );
    let mut out = crate::initial();
    let one = response(format!("Plan:\n{plan}\nFor example {EXAMPLE}."));
    assert!(super::proposal::decode(&one, &mut out).is_some());
}

/// An outcome whose receipt holds one call, as `call_with_schema` leaves it.
fn called() -> crate::CompileOutcome {
    let mut out = crate::initial();
    let mut receipt = crate::AuthoringReceipt::new("mock/authoring".to_owned());
    receipt.context.push(serde_json::json!({"call": "plan"}));
    out.provenance.authoring = Some(receipt);
    out
}

#[test]
fn a_competitor_with_a_defect_is_still_a_competitor_and_its_digest_is_kept() {
    use super::proposal::Proposal;
    let plan = r#"{"steps":[],"effects":[],"obligations":[],"constraints":[],"unknowns":[]}"#;
    // An unknown key or a null field keeps it from decoding, never from competing.
    for rival in [
        r#"{"steps":[{"op":"draft","detail":"x","evidence":"x"}],"confidence":0.9}"#,
        r#"{"steps":null,"effects":[]}"#,
    ] {
        assert!(
            super::answer_shaped::<Proposal>(rival, &["steps"]),
            "{rival}"
        );
        let text = format!("Draft:\n{plan}\nFinal:\n{rival}");
        assert!(
            matches!(answer_objects(&text, |o| super::answer_shaped::<Proposal>(o, &["steps"])), Objects::Two(ref o) if o == &[plan, rival]),
            "{text}"
        );
        let mut out = called();
        let response = InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        );
        assert!(super::proposal::decode(&response, &mut out).is_none());
        let call = &out.provenance.authoring.as_ref().unwrap().context[0];
        assert_eq!(call["competing_objects"], super::digests(&[plan, rival]));
    }
    // An object that carries none of a plan's keys and does not decode is an example.
    assert!(!super::answer_shaped::<Proposal>(EXAMPLE, &["steps"]));
}

#[test]
fn the_syntax_path_judges_the_broken_answer_never_a_template() {
    let broken = r#"{"steps": !}"#;
    for text in [
        format!("It uses ${{{{ with.content }}}}, then {broken}"),
        format!("{{name}} {broken} {{{{ x }}}}"),
        broken.to_owned(),
    ] {
        assert_eq!(super::syntax_target(&text), Some(broken), "{text}");
    }
    for text in ["{{ a }} {name}", "{\"steps\": [", "no object"] {
        assert_eq!(super::syntax_target(text), None, "{text}");
    }
}
