// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Syntax feedback consumes the existing native repair budget, never a transport retry.
//! Every refused answer is journaled with its class (`failure_class`).
use super::super::{
    Objects, answer_objects, answer_shaped, digests, first_json_object, record_objects,
    syntax_target,
};
use super::{CompileOutcome, DiagnosticKind, Talk, knowledge};
use nika_kernel::ai::provider::{InferResponse, StopReason};
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

enum Decoded<T> {
    Answer(T, String),
    Invalid(serde_json::Error),
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
        Decoded::Invalid(error) => {
            invalid(out, what, &shown(&error));
            None
        }
        Decoded::Stop => None,
    }
}

fn read<T: Shaped>(
    response: &InferResponse,
    what: &str,
    round: u32,
    talk: &mut Talk,
    out: &mut CompileOutcome,
) -> Decoded<T> {
    let text = match crate::decide::answer_text(response) {
        Some(text) => text.to_owned(),
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
                "answer":format!("not a {what} answer: {}", shown(&error)),
                "failure_class":failure_class(&error),
                "response_sha256":knowledge::sha256(&text),
                "decode_error":{"category":format!("{:?}",error.classify()),"line":error.line(),"column":error.column()},
                "usage_reported":response.usage_reported}));
            Decoded::Invalid(error)
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

/// What a public record says of a decode error: a JSON syntax error as serde words it (it quotes
/// no answer text), an answer outside its schema by class and position only — the key or value
/// the seat wrote is never repeated. The seat's own repair still reads the whole error.
fn shown(error: &serde_json::Error) -> String {
    match error.classify() {
        serde_json::error::Category::Data => format!(
            "the answer schema, line {}, column {}",
            error.line(),
            error.column()
        ),
        _ => error.to_string(),
    }
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

fn invalid(out: &mut CompileOutcome, what: &str, error: &dyn std::fmt::Display) {
    crate::finding(
        out,
        DiagnosticKind::Unknown,
        "authoring_native",
        format!("The seat's answer is not a {what} answer ({error}); nothing was assembled."),
    );
}
