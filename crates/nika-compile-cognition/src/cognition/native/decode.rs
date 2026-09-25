// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Syntax feedback consumes the existing native repair budget, never a transport retry.
use super::{Answer, AuthoringPolicy, CompileOutcome, DiagnosticKind, Round, Talk, knowledge};
use nika_kernel::ai::provider::{ContentBlock, InferResponse, Message, Role, StopReason};
use serde_json::json;

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
    let (text, error) = match read(response, "native", round, talk, out) {
        Decoded::Answer(answer, text) => return Ok((answer, text)),
        Decoded::Invalid(text, error) => (text, error),
        Decoded::Stop => return Err(Round::Stop),
    };
    // EndTurn + one text was checked by read. Incomplete JSON, unreported usage
    // and typed-envelope violations never authorize another paid call here.
    if !response.usage_reported
        || !error.is_syntax()
        || super::super::first_json_object(&text).is_none()
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
                .push(json!({"round":round,"answer":"cut at the authoring cap"}));
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
    match serde_json::from_str(super::super::first_json_object(&text).unwrap_or(&text)) {
        Ok(answer) => {
            talk.last_decode = None;
            Decoded::Answer(answer, text)
        }
        Err(error) => {
            talk.rounds.push(json!({"round":round,
                "answer":format!("not a {what} answer: {error}"),
                "response_sha256":knowledge::sha256(&text),
                "decode_error":{"category":format!("{:?}",error.classify()),"line":error.line(),"column":error.column()},
                "usage_reported":response.usage_reported}));
            Decoded::Invalid(text, error)
        }
    }
}

fn invalid(out: &mut CompileOutcome, what: &str, error: &serde_json::Error) {
    crate::finding(
        out,
        DiagnosticKind::Unknown,
        "authoring_native",
        format!("The seat's answer is not a {what} answer ({error}); nothing was assembled."),
    );
}
