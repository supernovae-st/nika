// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Syntax feedback consumes the existing native repair budget, never a transport retry.
//! Every refused answer is journaled with its class (`failure_class`).
use super::super::{Objects, answer_objects, first_json_object, record_unread};
use super::answer::{Defect, WireAnswer};
use super::{Answer, AuthoringPolicy, CompileOutcome, DiagnosticKind, Round, Talk, knowledge};
use nika_kernel::ai::provider::{ContentBlock, InferResponse, Message, Role, StopReason};
use serde_json::json;

/// Two different answers in one text.
const CONFLICTING: &str = "CONFLICTING_CANDIDATES";
/// An answer beside an object that never closes: undecided.
const AMBIGUOUS: &str = "AMBIGUOUS_ANSWER_TEXT";

enum Decoded<T> {
    Answer(T, String),
    Invalid(String, serde_json::Error),
    Stop,
}

/// Sketch callers retain their existing terminal decode behavior.
pub(in crate::cognition) fn decode<T: serde::de::DeserializeOwned>(
    response: &InferResponse,
    what: &str,
    round: u32,
    talk: &mut Talk,
    out: &mut CompileOutcome,
) -> Option<(T, String)> {
    match read(response, what, round, talk, out) {
        Decoded::Answer(answer, text) => Some((answer, text)),
        Decoded::Invalid(_, error) => {
            invalid(out, what, &error);
            None
        }
        Decoded::Stop => None,
    }
}

pub(super) fn native(
    response: &InferResponse,
    round: u32,
    policy: &AuthoringPolicy,
    talk: &mut Talk,
    out: &mut CompileOutcome,
) -> Result<(Answer, String), Round> {
    let (text, error) = match read::<WireAnswer>(response, "native", round, talk, out) {
        Decoded::Answer(wire, text) => {
            return match Answer::try_from(wire) {
                Ok(answer) => Ok((answer, text)),
                Err(defect) => {
                    refuse(response, round, &text, &defect, talk, out);
                    Err(Round::Stop)
                }
            };
        }
        Decoded::Invalid(text, error) => (text, error),
        Decoded::Stop => return Err(Round::Stop),
    };
    // EndTurn + one text was checked by read. Incomplete JSON, unreported usage
    // and typed-envelope violations never authorize another paid call here.
    if !response.usage_reported
        || !error.is_syntax()
        || first_json_object(&text).is_none()
        || round >= policy.repairs.min(5)
    {
        invalid(out, "native", &error);
        return Err(Round::Stop);
    }
    let digest = knowledge::sha256(&text);
    if talk.last_decode.as_ref() == Some(&digest) {
        invalid(out, "native", &error);
        return Err(Round::Stalled);
    }
    talk.last_decode = Some(digest);
    if let Some(entry) = talk.rounds.last_mut() {
        entry["repair"] = json!("requested within native repair budget");
    }
    talk.messages.push(Message::text(Role::Assistant, text));
    talk.messages.push(Message::text(Role::User, json!({
        "kind":"answer_json_syntax", "diagnostic":error.to_string(),
        "instruction":"The completed answer was rejected before candidate judgment. Return a new complete JSON answer using the same answer schema and original request. Correct JSON syntax only; do not invent facts, effects or permissions. The compiler will still parse, check and judge the candidate."
    }).to_string()));
    Err(Round::Repair)
}

fn read<T: serde::de::DeserializeOwned>(
    response: &InferResponse,
    what: &str,
    round: u32,
    talk: &mut Talk,
    out: &mut CompileOutcome,
) -> Decoded<T> {
    let text = match response.content.as_slice() {
        [ContentBlock::Text { text }] if response.stop_reason == StopReason::EndTurn => {
            text.clone()
        }
        _ if response.stop_reason == StopReason::MaxTokens => {
            talk.rounds
                .push(json!({"round":round,"answer":"cut at the authoring cap",
                "failure_class":"OUTPUT_TRUNCATED"}));
            crate::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_native",
                format!(
                    "The seat's answer was cut at the authoring cap ({} output tokens): raise --authoring-max-tokens, or seat a model that does not spend the budget on its reasoning.",
                    response.usage.output_tokens
                ),
            );
            return Decoded::Stop;
        }
        _ => {
            talk.rounds
                .push(json!({"round":round,"answer":"not one complete JSON text"}));
            crate::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_native",
                "The seat did not return one complete JSON text; nothing was assembled.",
            );
            return Decoded::Stop;
        }
    };
    let json = match answer_objects(&text, |object| serde_json::from_str::<T>(object).is_ok()) {
        Objects::One { answer, unread } => {
            record_unread(out, &unread);
            answer
        }
        Objects::None => first_json_object(&text).unwrap_or(&text),
        Objects::Two => return beside(response, what, round, &text, CONFLICTING, talk, out),
        Objects::Undecided => return beside(response, what, round, &text, AMBIGUOUS, talk, out),
    };
    match serde_json::from_str(json) {
        Ok(answer) => {
            talk.last_decode = None;
            Decoded::Answer(answer, text)
        }
        Err(error) => {
            talk.rounds.push(json!({"round":round,
                "answer":format!("not a {what} answer: {error}"),
                "failure_class":failure_class(&error),
                "response_sha256":knowledge::sha256(&text),
                "decode_error":{"category":format!("{:?}",error.classify()),"line":error.line(),"column":error.column()},
                "usage_reported":response.usage_reported}));
            Decoded::Invalid(text, error)
        }
    }
}

/// A text carrying two different answers, or one beside an object that never closes: journaled
/// with its class, nothing read or assembled, and no call bought to choose.
fn beside<T>(
    response: &InferResponse,
    what: &str,
    round: u32,
    text: &str,
    class: &str,
    talk: &mut Talk,
    out: &mut CompileOutcome,
) -> Decoded<T> {
    let why = if class == CONFLICTING {
        "it carries two different answer objects; neither was read"
    } else {
        "an object that never closes sits beside the answer; neither was read"
    };
    talk.rounds.push(json!({"round":round,
        "answer":format!("not a {what} answer: {class}: {why}"),
        "failure_class":class,
        "response_sha256":knowledge::sha256(text),
        "usage_reported":response.usage_reported}));
    invalid(out, what, &format_args!("{class}: {why}"));
    Decoded::Stop
}

/// The class of an answer the decoder could not read: its JSON, or its answer schema.
fn failure_class(error: &serde_json::Error) -> &'static str {
    use serde_json::error::Category;
    match error.classify() {
        Category::Data => "ANSWER_SCHEMA",
        // `from_str` reads no I/O; a text that ends early is incomplete JSON.
        Category::Syntax | Category::Eof | Category::Io => "ANSWER_JSON_SYNTAX",
    }
}

/// A well-formed answer without one candidate: its class is journaled, nothing is chosen or
/// assembled, and no call is bought. A transport re-ask would need an explicit multiplicity
/// authority, which the native repair budget is not.
fn refuse(
    response: &InferResponse,
    round: u32,
    text: &str,
    defect: &Defect,
    talk: &mut Talk,
    out: &mut CompileOutcome,
) {
    let mut entry = json!({"round":round,
        "answer":format!("not a native answer: {defect}"),
        "failure_class":defect.class(),
        "response_sha256":knowledge::sha256(text),
        "usage_reported":response.usage_reported});
    match defect {
        Defect::ConflictingCandidates(dual) => entry["transport"] = dual.record(),
        Defect::CandidateLinePhysicalBreak {
            element,
            code_point,
        } => {
            entry["physical_break"] = json!({"element": element,
                "code_point": format!("U+{:04X}", u32::from(*code_point))});
        }
    }
    talk.rounds.push(entry);
    invalid(out, "native", defect);
}

fn invalid(out: &mut CompileOutcome, what: &str, error: &dyn std::fmt::Display) {
    crate::finding(
        out,
        DiagnosticKind::Unknown,
        "authoring_native",
        format!("The seat's answer is not a {what} answer ({error}); nothing was assembled."),
    );
}

#[cfg(test)]
mod tests {
    //! The native door end to end over synthetic answers: what the seat sent, what the judge
    //! saw, how many calls it cost. No live seat is qualified here.
    use super::{AMBIGUOUS, CONFLICTING};
    use crate::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode};
    use nika_kernel::ai::provider::{
        ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
        TokenUsage,
    };
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicU32, Ordering};

    const INTENT: &str = "Read ./input.txt and copy its exact contents to ./output.txt using only deterministic builtin tools.";

    /// A deterministic candidate the judge accepts for `INTENT` in one round (Ready, no question).
    const SOURCE: &str = r#"nika: deterministic-copy
model: mock/echo
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["./input.txt"]
    write: ["./output.txt"]
tasks:
  read:
    invoke:
      tool: "nika:read"
      args: { path: "./input.txt" }
  write:
    with: { content: "${{ tasks.read.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./output.txt", content: "${{ with.content }}", overwrite: true }
"#;

    /// A seat that answers its texts in order (the last one repeats) and counts its calls.
    struct Seat {
        texts: Vec<String>,
        usage_reported: bool,
        calls: AtomicU32,
    }

    impl ProviderInferDyn for Seat {
        async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) as usize;
            let text = self.texts[n.min(self.texts.len() - 1)].clone();
            Ok(InferResponse::new(
                vec![ContentBlock::Text { text }],
                TokenUsage::new(100, 50),
                StopReason::EndTurn,
            )
            .with_usage_reported(self.usage_reported))
        }
    }

    fn envelope(candidate: &str, lines: &[&str]) -> String {
        json!({"candidate": candidate, "candidate_lines": lines, "questions": [], "gaps": [], "notes": ""})
            .to_string()
    }

    fn lines(source: &str) -> Vec<&str> {
        source.split('\n').collect()
    }

    async fn author(texts: &[String], repairs: u32, usage_reported: bool) -> (CompileOutcome, u32) {
        let seat = Seat {
            texts: texts.to_vec(),
            usage_reported,
            calls: AtomicU32::new(0),
        };
        let policy =
            AuthoringPolicy::new("mock/authoring", 4096, std::time::Duration::from_secs(2))
                .with_native(NativeMode::Only)
                .with_repairs(repairs);
        let request = CompileRequest::create(INTENT).with_authoring_policy(policy);
        let out = Box::pin(crate::compile_with_provider(&request, &seat))
            .await
            .unwrap();
        (out, seat.calls.load(Ordering::SeqCst))
    }

    fn native(out: &CompileOutcome) -> Value {
        out.provenance.decision.as_ref().unwrap()["native"].clone()
    }

    /// Nothing was accepted, emitted or recorded for replay.
    fn nothing_assembled(out: &CompileOutcome, what: &str) {
        assert_ne!(out.status, CompileStatus::Ready, "{what}: {out:#?}");
        assert!(out.candidate.is_none(), "{what}: {out:#?}");
        assert_ne!(native(out)["accepted"], true, "{what}: {out:#?}");
        assert_ne!(
            out.provenance.plan.as_ref().map(|p| &p["strategy"]),
            Some(&json!("native")),
            "{what}: no native replay record"
        );
    }

    #[tokio::test]
    async fn an_identical_dual_answer_is_judged_exactly_as_its_single_representation() {
        let single = envelope(SOURCE, &[]);
        let (reference, calls) = author(&[single], 3, true).await;
        assert_eq!(calls, 1);
        assert_eq!(reference.status, CompileStatus::Ready, "{reference:#?}");
        assert_eq!(reference.candidate.as_deref(), Some(SOURCE));
        let digest = crate::cognition::knowledge::sha256(SOURCE);
        for (text, dual) in [
            (envelope("", &lines(SOURCE)), false),
            (envelope(SOURCE, &lines(SOURCE)), true),
        ] {
            let (out, calls) = author(std::slice::from_ref(&text), 3, true).await;
            assert_eq!(calls, 1, "T0 normalization is free: {text}");
            assert_eq!(out.status, CompileStatus::Ready, "{text}: {out:#?}");
            assert_eq!(out.candidate.as_deref(), Some(SOURCE), "{text}");
            assert_eq!(out.provenance.plan, reference.provenance.plan, "{text}");
            let mut rounds = native(&out)["rounds"].clone();
            // Both texts stay in evidence, apart from the judged source's own digest.
            let transport = rounds[0]
                .as_object_mut()
                .and_then(|round| round.remove("transport"));
            assert_eq!(transport.is_some(), dual, "{text}");
            if let Some(transport) = transport {
                assert_eq!(transport["verdict"], "EQUIVALENT");
                assert_eq!(transport["candidate"]["sha256"], digest);
                assert_eq!(transport["candidate_lines"]["joined_sha256"], digest);
                assert_eq!(transport["kept_sha256"], rounds[0]["candidate_sha256"]);
            }
            assert_eq!(rounds, native(&reference)["rounds"], "{text}");
            assert!(
                out.check_preview.as_ref().unwrap().report.is_clean(),
                "{text}: the existing Check judged the same bytes"
            );
            let replay = crate::compile(
                &CompileRequest::create(INTENT).with_plan(out.provenance.plan.clone().unwrap()),
            )
            .unwrap();
            assert_eq!(replay.status, CompileStatus::Ready, "{replay:#?}");
            assert_eq!(replay.candidate.as_deref(), Some(SOURCE));
            assert!(
                replay.provenance.authoring.is_none(),
                "replay makes no call"
            );
        }
    }

    #[tokio::test]
    async fn conflicting_representations_are_never_chosen_and_buy_no_call() {
        let other = SOURCE.replace("./output.txt", "./elsewhere.txt");
        let crlf = SOURCE.replace('\n', "\r\n");
        let unterminated = SOURCE.trim_end_matches('\n');
        let good = envelope(SOURCE, &[]);
        for (conflict, newlines_only) in [
            (envelope(SOURCE, &lines(&other)), false),
            (envelope(&other, &lines(SOURCE)), false),
            (envelope(SOURCE, &lines(unterminated)), true),
            (envelope(&crlf, &lines(SOURCE)), true),
            (envelope(" ", &lines(SOURCE)), false),
        ] {
            // A repair budget and a good next answer are available: neither is used.
            let (out, calls) = author(&[conflict.clone(), good.clone()], 3, true).await;
            assert_eq!(calls, 1, "{conflict}");
            nothing_assembled(&out, &conflict);
            let round = &native(&out)["rounds"][0];
            assert_eq!(round["failure_class"], "CONFLICTING_CANDIDATES", "{round}");
            assert_eq!(round["transport"]["verdict"], "CONFLICTING", "{round}");
            assert_eq!(
                round["transport"]["newlines_only"], newlines_only,
                "{round}"
            );
            assert!(round["transport"]["first_difference"].is_u64(), "{round}");
            assert_eq!(round["transport"]["kept_sha256"], Value::Null, "{round}");
            assert!(
                round.get("candidate_sha256").is_none(),
                "never judged: {round}"
            );
            assert!(round.get("repair").is_none(), "{round}");
            assert!(
                out.diagnostics
                    .iter()
                    .any(|d| d.message.contains("not a native answer")
                        && d.message.contains("CONFLICTING_CANDIDATES")),
                "{out:#?}"
            );
        }
    }

    #[tokio::test]
    async fn two_answers_in_one_text_are_never_resolved_by_reading_the_first() {
        let good = envelope(SOURCE, &[]);
        let other = envelope(&SOURCE.replace("./output.txt", "./elsewhere.txt"), &[]);
        let cut = "{\"candidate\": \"nika: b\\ntasks: {}";
        for (text, class) in [
            (format!("Draft:\n{good}\nFinal:\n{other}"), CONFLICTING),
            (
                format!("```json\n{other}\n```\n```json\n{good}\n```"),
                CONFLICTING,
            ),
            (format!("{good}\n{other}"), CONFLICTING),
            // An answer-shaped example is a plausible second answer: never dropped for yield.
            (
                format!("Example: {{\"notes\": \"x\"}}\nAnswer: {good}"),
                CONFLICTING,
            ),
            // A competitor cut short beside a complete answer leaves the text undecided.
            (format!("Draft:\n{good}\nFinal:\n{cut}"), AMBIGUOUS),
        ] {
            let (out, calls) = author(&[text.clone(), good.clone()], 3, true).await;
            assert_eq!(calls, 1, "{text}");
            nothing_assembled(&out, &text);
            let round = &native(&out)["rounds"][0];
            assert_eq!(round["failure_class"], class, "{round}");
            assert!(
                round.get("candidate_sha256").is_none(),
                "never judged: {round}"
            );
        }
        // One answer is read as the only answer, without a repair call: through prose and
        // template braces before or after it, repeated bare or in prose, beside an example that
        // cannot be an answer (kept on its call by digest, never read).
        let example = "{\"status\": \"paid\"}";
        for text in [
            format!("Here is the answer:\n```json\n{good}\n```\nIt uses ${{{{ with.content }}}}."),
            format!("It uses ${{{{ with.content }}}}, then:\n{good}"),
            format!("{good}\n{good}"),
            format!("Draft:\n{good}\nFinal:\n{good}"),
            format!("{good}\nFor example {example}."),
        ] {
            let (out, calls) = author(std::slice::from_ref(&text), 0, true).await;
            assert_eq!(calls, 1, "{text}");
            assert_eq!(out.candidate.as_deref(), Some(SOURCE), "{text}: {out:#?}");
            let call = &out.provenance.authoring.as_ref().unwrap().context[0];
            let unread = text.contains(example).then(|| {
                json!([{"sha256": crate::cognition::knowledge::sha256(example), "bytes": example.len()}])
            });
            assert_eq!(call.get("unread_objects"), unread.as_ref(), "{text}");
        }
    }

    #[tokio::test]
    async fn typed_envelope_violations_stop_before_any_repair_call() {
        let good = envelope(SOURCE, &[]);
        let schema = ("ANSWER_SCHEMA", Value::Null);
        let physical = |element: usize, code_point: &str| {
            (
                "CANDIDATE_LINE_PHYSICAL_BREAK",
                json!({"element": element, "code_point": code_point}),
            )
        };
        for (bad, (class, physical_break)) in [
            (
                json!({"candidate": SOURCE, "candidate_lines": null}),
                schema.clone(),
            ),
            (
                json!({"candidate": null, "candidate_lines": lines(SOURCE)}),
                schema.clone(),
            ),
            (json!({"candidate_lines": [7]}), schema.clone()),
            (json!({"candidate": SOURCE, "extra": true}), schema.clone()),
            (
                json!({"candidate": SOURCE, "candidate_lines": [SOURCE]}),
                physical(0, "U+000A"),
            ),
            (
                json!({"candidate_lines": ["nika: x", "tasks: {}\r"]}),
                physical(1, "U+000D"),
            ),
        ] {
            let bad = bad.to_string();
            let (out, calls) = author(&[bad.clone(), good.clone()], 3, true).await;
            assert_eq!(calls, 1, "{bad}");
            nothing_assembled(&out, &bad);
            let round = &native(&out)["rounds"][0];
            assert_eq!(round["failure_class"], class, "{bad}: {round}");
            assert_eq!(round["physical_break"], physical_break, "{bad}: {round}");
            assert!(
                round["answer"]
                    .as_str()
                    .unwrap()
                    .contains("not a native answer")
            );
        }
    }

    #[tokio::test]
    async fn json_syntax_repair_keeps_its_budget_and_its_authority() {
        let malformed = r#"{"candidate": !}"#.to_owned();
        let good = envelope(SOURCE, &lines(SOURCE));
        let (out, calls) = author(&[malformed.clone(), good.clone()], 1, true).await;
        assert_eq!(calls, 2, "one repair inside the authorized budget");
        assert_eq!(out.candidate.as_deref(), Some(SOURCE));
        let rounds = native(&out)["rounds"].clone();
        assert_eq!(rounds[0]["failure_class"], "ANSWER_JSON_SYNTAX", "{rounds}");
        assert_eq!(rounds[0]["repair"], "requested within native repair budget");
        // Zero repairs and unreported usage never buy the call.
        for (repairs, usage_reported) in [(0, true), (3, false)] {
            let (out, calls) =
                author(&[malformed.clone(), good.clone()], repairs, usage_reported).await;
            assert_eq!(calls, 1, "repairs {repairs}, usage {usage_reported}");
            nothing_assembled(&out, &malformed);
            assert_eq!(
                native(&out)["rounds"][0]["failure_class"],
                "ANSWER_JSON_SYNTAX"
            );
        }
    }

    #[tokio::test]
    async fn an_empty_representation_is_absent_and_empty_answers_are_never_a_workflow() {
        // An empty text stands for no candidate: the other representation is the one candidate.
        let (out, calls) = author(&[envelope(SOURCE, &[""])], 0, true).await;
        assert_eq!(
            (calls, out.candidate.as_deref()),
            (1, Some(SOURCE)),
            "{out:#?}"
        );
        // Both empty: decoded (no transport death), judged, refused, never a workflow.
        for empty in [envelope("", &[]), envelope("", &[""])] {
            let (out, calls) = author(std::slice::from_ref(&empty), 0, true).await;
            assert_eq!(calls, 1, "{empty}");
            nothing_assembled(&out, &empty);
            let round = &native(&out)["rounds"][0];
            assert!(round.get("failure_class").is_none(), "{round}");
            assert_eq!(
                round["candidate"], "",
                "the judge saw the empty candidate: {round}"
            );
        }
    }
}
