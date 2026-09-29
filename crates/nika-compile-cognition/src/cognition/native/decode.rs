// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Syntax feedback consumes the existing native repair budget, never a transport retry.
//! Every refused answer is journaled with its class (`failure_class`).
use super::super::{
    Objects, answer_objects, answer_shaped, digests, first_json_object, record_objects,
    syntax_target,
};
use super::answer::{Defect, WireAnswer};
use super::{Answer, AuthoringPolicy, CompileOutcome, DiagnosticKind, Round, Talk, knowledge};
use nika_kernel::ai::provider::{ContentBlock, InferResponse, Message, Role, StopReason};
use serde_json::json;

/// Two different answers in one text.
const CONFLICTING: &str = "CONFLICTING_CANDIDATES";
/// An answer beside an object that never closes: undecided.
const AMBIGUOUS: &str = "AMBIGUOUS_ANSWER_TEXT";

/// A kind of answer a decoder reads, and the keys only that kind carries: an object that
/// carries one competes with the answer even when a defect keeps it from decoding.
pub(in crate::cognition) trait Shaped: serde::de::DeserializeOwned {
    const KEYS: &'static [&'static str];
}

impl Shaped for WireAnswer {
    const KEYS: &'static [&'static str] = &["candidate", "candidate_lines"];
}

enum Decoded<T> {
    Answer(T, String),
    Invalid(String, serde_json::Error),
    Stop,
}

/// Sketch callers retain their existing terminal decode behavior.
pub(in crate::cognition) fn decode<T: Shaped>(
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

fn read<T: Shaped>(
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
    let json = match answer_objects(&text, |object| answer_shaped::<T>(object, T::KEYS)) {
        Objects::One { answer, unread } => {
            record_objects(out, "unread_objects", &unread);
            answer
        }
        // No complete object: the syntax path judges the broken answer, never a template.
        Objects::None => syntax_target(&text)
            .or_else(|| first_json_object(&text))
            .unwrap_or(&text),
        Objects::Two(objects) => {
            return beside(
                response,
                what,
                round,
                &text,
                (CONFLICTING, &objects),
                talk,
                out,
            );
        }
        Objects::Undecided(objects) => {
            return beside(
                response,
                what,
                round,
                &text,
                (AMBIGUOUS, &objects),
                talk,
                out,
            );
        }
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
/// with its class and every competing object by digest, nothing read or assembled, and no call
/// bought to choose.
fn beside<T>(
    response: &InferResponse,
    what: &str,
    round: u32,
    text: &str,
    (class, objects): (&str, &[&str]),
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
        "objects":digests(objects),
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
    use crate::{
        AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, NativeMode,
        QuestionType,
    };
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

    /// A seat that answers its texts in order (the last one repeats), counts its calls and
    /// keeps the last message each call carried.
    struct Seat {
        texts: Vec<String>,
        usage_reported: bool,
        calls: AtomicU32,
        seen: std::sync::Mutex<Vec<String>>,
    }

    /// The approval a verifier question offers (`faithful` for the whole request, `carried` for a
    /// clause), else `None`: the seat approves the judge's closed choices without counting them,
    /// as the suites' explicit judge double does (R4 A11); this module reads the native door.
    fn approval(request: &InferRequest) -> Option<&'static str> {
        let nika_kernel::ai::provider::ResponseFormat::JsonSchema(schema) =
            &request.response_format
        else {
            return None;
        };
        let keys = schema["properties"]["choice"]["enum"].as_array()?;
        ["faithful", "carried"]
            .into_iter()
            .find(|key| keys.iter().any(|k| k == key))
    }

    impl ProviderInferDyn for Seat {
        async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
            if let Some(key) = approval(&request) {
                return Ok(InferResponse::new(
                    vec![ContentBlock::Text {
                        text: json!({"choice": key}).to_string(),
                    }],
                    TokenUsage::new(1, 1),
                    StopReason::EndTurn,
                )
                .with_usage_reported(self.usage_reported));
            }
            if let Some(ContentBlock::Text { text }) = request
                .messages
                .last()
                .and_then(|message| message.content.first())
            {
                self.seen.lock().unwrap().push(text.clone());
            }
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

    fn policy(repairs: u32) -> AuthoringPolicy {
        AuthoringPolicy::new("mock/authoring", 4096, std::time::Duration::from_secs(2))
            .with_native(NativeMode::Only)
            .with_repairs(repairs)
    }

    async fn author(texts: &[String], repairs: u32, usage_reported: bool) -> (CompileOutcome, u32) {
        let request = CompileRequest::create(INTENT).with_authoring_policy(policy(repairs));
        let (out, calls, _) = author_with(texts, &request, usage_reported).await;
        (out, calls)
    }

    /// The door over one request; the calls, and the last message of each call.
    async fn author_with(
        texts: &[String],
        request: &CompileRequest,
        usage_reported: bool,
    ) -> (CompileOutcome, u32, Vec<String>) {
        let seat = Seat {
            texts: texts.to_vec(),
            usage_reported,
            calls: AtomicU32::new(0),
            seen: std::sync::Mutex::new(Vec::new()),
        };
        let out = Box::pin(crate::compile_with_provider(request, &seat))
            .await
            .unwrap();
        let seen = seat.seen.lock().unwrap().clone();
        (out, seat.calls.load(Ordering::SeqCst), seen)
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
            // Held for the round's judge (R4 A11, step 2): this keyless replay permits none.
            assert_eq!(replay.status, CompileStatus::Incomplete, "{replay:#?}");
            let whole = json!([{"clause": INTENT, "witness": null, "spans": [[0, INTENT.len()]]}]);
            let pending = &replay.provenance.decision.as_ref().unwrap()["pending"]["open"];
            assert_eq!(pending, &whole, "{replay:#?}");
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
    async fn a_defective_competitor_conflicts_and_the_syntax_path_names_the_broken_answer() {
        let good = envelope(SOURCE, &[]);
        let other = SOURCE.replace("./output.txt", "./elsewhere.txt");
        // A final answer with an unknown key or a null field still competes with the draft,
        // and the journal keeps every competing object by digest.
        for rival in [
            json!({"candidate": other, "candidate_lines": [], "questions": [], "gaps": [],
                "notes": "", "confidence": 0.9})
            .to_string(),
            json!({"candidate": other, "candidate_lines": null}).to_string(),
        ] {
            let text = format!("Draft:\n{good}\nFinal:\n{rival}");
            let (out, calls) = author(&[text.clone(), good.clone()], 3, true).await;
            assert_eq!(calls, 1, "{text}");
            nothing_assembled(&out, &text);
            let round = &native(&out)["rounds"][0];
            assert_eq!(round["failure_class"], CONFLICTING, "{round}");
            let competing = crate::cognition::digests(&[good.as_str(), rival.as_str()]);
            assert_eq!(round["objects"], competing, "{round}");
        }
        // An undecided text keeps its complete objects by digest too.
        let cut = format!("Draft:\n{good}\nFinal:\n{{\"candidate\": \"nika: b");
        let (out, _) = author(&[cut], 3, true).await;
        let round = &native(&out)["rounds"][0];
        assert_eq!(round["failure_class"], AMBIGUOUS, "{round}");
        assert_eq!(
            round["objects"],
            crate::cognition::digests(&[good.as_str()])
        );
        // Template braces before a broken answer: the paid repair names the broken answer.
        let text = "It uses ${{ with.content }}, then {\"candidate\": !}".to_owned();
        let request = CompileRequest::create(INTENT).with_authoring_policy(policy(1));
        let (out, calls, seen) = author_with(&[text, good], &request, true).await;
        assert_eq!(calls, 2);
        assert_eq!(out.candidate.as_deref(), Some(SOURCE), "{out:#?}");
        let round = &native(&out)["rounds"][0];
        assert_eq!(round["failure_class"], "ANSWER_JSON_SYNTAX", "{round}");
        for said in [round["answer"].as_str().unwrap(), seen[1].as_str()] {
            assert!(said.contains("expected value"), "{said}");
            assert!(!said.contains("key must be a string"), "{said}");
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

    /// An answer that writes nothing and asks: its questions and gaps as the seat sends them.
    fn ask_envelope(questions: &Value, gaps: &Value) -> String {
        json!({"candidate": "", "candidate_lines": [], "questions": questions, "gaps": gaps, "notes": "ask"})
            .to_string()
    }

    fn folder_question() -> Value {
        json!({"key": "const.archive_folder", "label": "Which folder keeps the copies?",
            "answer_type": "text", "why": "The request names no folder."})
    }

    fn keys(out: &CompileOutcome) -> Vec<&str> {
        out.questions.iter().map(|q| q.key.as_str()).collect()
    }

    /// The door entered as an escalation enters it: the cold round left one business question.
    async fn author_after_cold(texts: &[String], repairs: u32) -> (CompileOutcome, u32) {
        let seat = Seat {
            texts: texts.to_vec(),
            usage_reported: true,
            calls: AtomicU32::new(0),
            seen: std::sync::Mutex::new(Vec::new()),
        };
        let request = CompileRequest::create(INTENT).with_authoring_policy(policy(repairs));
        let mut cold = crate::initial();
        crate::question(
            &mut cold,
            COLD,
            "Who receives the copy?",
            QuestionType::Text,
        );
        let out = Box::pin(super::super::author(
            INTENT,
            &crate::lexicon::read(INTENT),
            request.authoring.as_ref().unwrap(),
            &seat,
            &request,
            Vec::new(),
            cold,
        ))
        .await
        .unwrap();
        (out, seat.calls.load(Ordering::SeqCst))
    }

    /// The cold round's question: a door that judged nothing restores it.
    const COLD: &str = "const.recipient";

    #[tokio::test]
    async fn an_ask_without_a_candidate_is_asked_in_one_call_and_never_ready() {
        let retention = json!({"key": "const.retention_days", "label": "How many days?",
            "answer_type": "literal", "why": ""});
        let ask = ask_envelope(
            &json!([folder_question(), retention]),
            &json!(["keep only the recent copies", " "]),
        );
        // A repair budget and a good next answer are available: the ask buys neither.
        let (out, calls) = author(&[ask, envelope(SOURCE, &[])], 3, true).await;
        assert_eq!(calls, 1);
        assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
        assert!(out.candidate.is_none(), "{out:#?}");
        assert!(
            out.provenance.plan.is_none(),
            "nothing to replay: the answers re-author"
        );
        let recent = super::super::ask::gap_key("keep only the recent copies");
        assert_eq!(
            keys(&out),
            [
                "const.archive_folder",
                "const.retention_days",
                recent.as_str()
            ]
        );
        let (folder, days, gap) = (&out.questions[0], &out.questions[1], &out.questions[2]);
        assert_eq!(
            (
                folder.label.as_str(),
                folder.why.as_str(),
                folder.answer_type
            ),
            (
                "Which folder keeps the copies?",
                "The request names no folder.",
                QuestionType::Text
            )
        );
        assert_eq!(days.answer_type, QuestionType::Literal);
        assert!(folder.mandatory && days.mandatory && gap.mandatory);
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::Missed
                    && d.message.contains("« keep only the recent copies »"))
        );
        let native = native(&out);
        assert_eq!(native["accepted"], false);
        assert_eq!(
            native["asked"],
            json!(["const.archive_folder", "const.retention_days"])
        );
        let round = &native["rounds"][0];
        assert!(
            round.get("candidate_sha256").is_none(),
            "nothing parsed: {round}"
        );
        assert!(out.diagnostics.iter().any(|d| {
            d.message
                .contains("round 0: asked 2 question(s), no candidate")
        }));
        // Gaps alone are an ask too: a finding and a question each, still no candidate.
        let gaps_only = ask_envelope(&json!([]), &json!(["keep only the recent copies"]));
        let (out, calls) = author(&[gaps_only, envelope(SOURCE, &[])], 3, true).await;
        assert_eq!((calls, keys(&out)), (1, vec![recent.as_str()]), "{out:#?}");
        assert!(out.candidate.is_none() && out.status == CompileStatus::Incomplete);
        // Lines that join to whitespace alone write nothing: the ask is still an ask.
        let blank = json!({"candidate": "", "candidate_lines": ["", ""],
            "questions": [folder_question()], "gaps": [], "notes": ""})
        .to_string();
        let (out, calls) = author(&[blank, envelope(SOURCE, &[])], 3, true).await;
        assert_eq!(
            (calls, keys(&out)),
            (1, vec!["const.archive_folder"]),
            "{out:#?}"
        );
    }

    #[tokio::test]
    async fn gaps_beyond_what_an_ask_carries_are_counted_never_lost() {
        let gaps: Vec<String> = (0..10).map(|n| format!("settle clause {n}")).collect();
        let (out, calls) = author(&[ask_envelope(&json!([]), &json!(gaps))], 0, true).await;
        assert_eq!(calls, 1);
        assert_eq!(out.questions.len(), 8, "{out:#?}");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.message.contains("2 more gap(s) than one ask carries (8)")),
            "{out:#?}"
        );
        assert_eq!(native(&out)["rounds"][0]["gaps_dropped"], 2);
    }

    /// A gap's key is its clause's (E10 FE10-5): renumbered gaps keep their keys, an answered
    /// clause is never asked again, and a reworded clause is another question, so an answer
    /// never settles a clause the human did not read when giving it.
    #[tokio::test]
    async fn an_asked_gap_is_keyed_by_its_clause_so_no_answer_settles_another() {
        let (a, b) = ("keep only the recent copies", "notify the owner by mail");
        // The key each clause was asked under, in either order the seat reports them.
        let asked = |out: &CompileOutcome, clause: &str| {
            out.questions
                .iter()
                .find(|q| q.label.contains(clause))
                .map(|q| q.key.clone())
                .expect("asked")
        };
        let (first, _) = author(&[ask_envelope(&json!([]), &json!([a, b]))], 0, true).await;
        let (other, _) = author(&[ask_envelope(&json!([]), &json!([b, a]))], 0, true).await;
        let key_a = asked(&first, a);
        assert_eq!(
            key_a,
            asked(&other, a),
            "renumbered, a clause keeps its key"
        );
        assert_eq!(asked(&first, b), asked(&other, b));
        assert_ne!(key_a, asked(&first, b));
        // The human answers the question asked for a; the seat reports b then a. The answered
        // clause is the candidate's to carry, and the refusal names it, never b.
        let request = CompileRequest::create(INTENT)
            .answer(&key_a, "\"drop\"")
            .with_authoring_policy(policy(0));
        let renumbered = ask_envelope(&json!([]), &json!([b, a]));
        let (out, calls, _) = author_with(&[renumbered], &request, true).await;
        assert_eq!(calls, 1);
        assert!(out.questions.is_empty(), "{out:#?}");
        let refused = native(&out)["rounds"][0]["diagnostics"][0]["message"].clone();
        let refused = refused.as_str().expect("a refusal");
        assert!(refused.contains(a) && !refused.contains(b), "{refused}");
        // Reworded, the clause is another question: the answer given for a never settles it.
        let reworded = "keep only the most recent copies";
        let ask = ask_envelope(&json!([]), &json!([reworded]));
        let (out, calls, _) = author_with(&[ask], &request, true).await;
        assert_eq!(calls, 1);
        assert_ne!(asked(&out, reworded), key_a, "{out:#?}");
        assert_eq!(out.questions.len(), 1, "{out:#?}");
    }

    #[tokio::test]
    async fn an_open_column_asked_without_a_candidate_is_a_choice_among_the_observed_ones() {
        let intent = "Additionne une colonne de ventes.csv dans total.txt.";
        let world = json!({"observed": [{"path": "ventes.csv", "kind": "csv", "delimiter": ",",
            "columns": ["montant", "autre"]}]});
        let request = CompileRequest::create(intent)
            .with_knowledge(world)
            .with_authoring_policy(policy(0));
        let ask = ask_envelope(
            &json!([{"key": "const.sum_column", "label": "Quelle colonne ?", "answer_type": "text", "why": ""}]),
            &json!([]),
        );
        let (out, calls, _) = author_with(&[ask], &request, true).await;
        assert_eq!(calls, 1);
        assert_eq!(keys(&out), ["const.sum_column"], "{out:#?}");
        assert_eq!(out.questions[0].answer_type, QuestionType::Choice);
        let offered: Vec<&str> = out.questions[0]
            .options
            .iter()
            .map(|o| o.key.as_str())
            .collect();
        assert_eq!(offered, ["montant", "autre"]);
    }

    #[tokio::test]
    async fn the_answers_to_an_ask_author_the_request_again() {
        // The host's next round carries the answers and no plan: the seat reads them and writes.
        let request = CompileRequest::create(INTENT)
            .answer("const.archive_folder", "\"./archive\"")
            .with_authoring_policy(policy(0));
        let (out, calls, seen) = author_with(&[envelope(SOURCE, &[])], &request, true).await;
        assert_eq!(calls, 1);
        assert!(out.provenance.authoring.is_some(), "authored, not replayed");
        assert_eq!(native(&out)["accepted"], true, "{out:#?}");
        let opening = &seen[0];
        assert!(
            opening.contains("answers_already_given") && opening.contains("./archive"),
            "{opening}"
        );
    }

    #[tokio::test]
    async fn an_ask_that_is_not_genuine_is_never_asked() {
        let good = envelope(SOURCE, &[]);
        let question = |key: &str, label: &str, answer_type: &str| json!({"key": key, "label": label, "answer_type": answer_type, "why": ""});
        let nine: Vec<Value> = (0..9)
            .map(|n| question(&format!("const.value_{n}"), "Which value?", "text"))
            .collect();
        let refused_asks = [
            json!([question("archive_folder", "Which folder?", "text")]),
            json!([question("const.source_glob", "Which files?", "text")]),
            json!([question("const.archive_folder", "Which folder?", "number")]),
            json!([question("const.archive_folder", " ", "text")]),
            json!([folder_question(), folder_question()]),
            json!(nine),
        ];
        for questions in refused_asks {
            let ask = ask_envelope(&questions, &json!([]));
            // Refused like a candidate's questions: diagnostics, never asked, no call at 0; the
            // door judged nothing, so the cold round's question stands and none of the seat's.
            let (out, calls) = author_after_cold(&[ask.clone(), good.clone()], 0).await;
            assert_eq!(calls, 1, "{ask}");
            let round = &native(&out)["rounds"][0];
            assert!(
                !round["diagnostics"].as_array().unwrap().is_empty(),
                "{round}"
            );
            assert!(native(&out).get("asked").is_none(), "{ask}");
            assert_eq!(keys(&out), [COLD], "{ask}");
            assert!(out.candidate.is_none());
            // The refused ask spent the budget: the exhaustion is stated, as a candidate's is.
            let route = &out.provenance.decision.as_ref().unwrap()["route"];
            let routed = route
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r == "native: exhausted");
            assert!(routed, "{ask}: {route}");
            assert!(
                out.diagnostics
                    .iter()
                    .any(|d| d.kind == DiagnosticKind::Unknown
                        && d.message.starts_with("No candidate passed the checks")),
                "{ask}"
            );
        }
        // Within the repair budget the diagnostics buy one repair, as a candidate's would.
        let ask = ask_envelope(
            &json!([question("archive_folder", "Which folder?", "text")]),
            &json!([]),
        );
        let (out, calls) = author(&[ask, good.clone()], 1, true).await;
        assert_eq!(calls, 2);
        assert_eq!(out.candidate.as_deref(), Some(SOURCE), "{out:#?}");
        // An already answered value is the candidate's to carry, never asked again.
        let request = CompileRequest::create(INTENT)
            .answer("const.archive_folder", "\"./archive\"")
            .with_authoring_policy(policy(0));
        let ask = ask_envelope(&json!([folder_question()]), &json!([]));
        let (out, calls, _) = author_with(&[ask], &request, true).await;
        assert_eq!(calls, 1);
        assert!(!keys(&out).contains(&"const.archive_folder"), "{out:#?}");
        // Questions of the wrong shape are no answer at all: one call, the cold question stands.
        for questions in [
            json!("Which folder?"),
            json!([{"key": "const.archive_folder"}]),
        ] {
            let ask = ask_envelope(&questions, &json!([]));
            let (out, calls) = author_after_cold(&[ask.clone(), good.clone()], 3).await;
            assert_eq!(calls, 1, "{ask}");
            assert_eq!(native(&out)["rounds"][0]["failure_class"], "ANSWER_SCHEMA");
            assert_eq!(keys(&out), [COLD], "{ask}");
        }
        // A genuine ask supersedes the cold plan: its questions are the seat's alone.
        let ask = ask_envelope(&json!([folder_question()]), &json!([]));
        let (out, calls) = author_after_cold(&[ask, good], 3).await;
        assert_eq!((calls, keys(&out)), (1, vec!["const.archive_folder"]));
    }

    #[tokio::test]
    async fn an_empty_answer_or_a_conflicting_one_takes_no_ask() {
        let good = envelope(SOURCE, &[]);
        // Nothing written and nothing asked: judged like any text that is not a workflow.
        let (out, calls) = author(&[envelope("", &[]), good.clone()], 1, true).await;
        assert_eq!(calls, 2);
        assert_eq!(out.candidate.as_deref(), Some(SOURCE));
        let round = &native(&out)["rounds"][0];
        assert!(round.get("asked").is_none() && round.get("candidate_sha256").is_some());
        // A conflicting dual with a genuine question: the conflict stops it, nothing is asked.
        let other = SOURCE.replace("./output.txt", "./elsewhere.txt");
        let conflict = json!({"candidate": SOURCE, "candidate_lines": lines(&other),
            "questions": [folder_question()], "gaps": [], "notes": ""})
        .to_string();
        let (out, calls) = author_after_cold(&[conflict, good], 3).await;
        assert_eq!(calls, 1);
        assert_eq!(
            native(&out)["rounds"][0]["failure_class"],
            "CONFLICTING_CANDIDATES"
        );
        assert_eq!(keys(&out), [COLD], "{out:#?}");
    }
}
