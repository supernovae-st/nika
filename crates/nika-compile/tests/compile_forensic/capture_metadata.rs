// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The returned-call metadata a scoped observer receives, held to an independent oracle.
//!
//! The reference framings below follow the written spec, not the observer: the legacy prompt
//! identity (`Debug(role) ':' text-bytes ':' text`, per message), the response framing
//! (`len ':' text`, per Text block) and the prompt shape v1 (domain, `u64` big-endian counts,
//! role bytes `S U A T`, `T`+length or `O` per block). Their literal vectors were computed with
//! Python's hashlib from the same spec. Product calls are then checked against the reference
//! over the exact messages the scripted provider received, and the legacy identities against
//! the digests the pre-change observer produced for the same compile.
//! Hermetic: scripted providers and in-memory sinks only; nothing is saved anywhere.

use super::*;
use nika_compile_cognition::observe::{
    Answered, AuthoringObservation, Failure, Sink, observe_authoring,
};
use nika_kernel::ai::provider::Message;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;
use std::sync::Arc;

// ── The reference framings (spec, not product) ──────────────────────────────────────────────

/// One block of an abstract prompt: Text of a byte length (no bytes needed), or another kind.
#[derive(Clone, Copy)]
enum Block<'a> {
    Text(&'a str),
    Other,
}

/// The legacy prompt identity: per message, its role's name, its Text byte total, its Text.
fn legacy(messages: &[(&str, Vec<Block<'_>>)]) -> String {
    let mut framed = String::new();
    for (role, blocks) in messages {
        let texts: Vec<&str> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Text(t) => Some(*t),
                Block::Other => None,
            })
            .collect();
        let bytes: usize = texts.iter().map(|t| t.len()).sum();
        let _ = write!(framed, "{role}:{bytes}:");
        framed.push_str(&texts.concat());
    }
    sha(&framed)
}

/// The shape v1 over lengths only, every count checked: `None` on any `u64` overflow, so huge
/// lengths are exercised without allocating them.
fn shape_of(messages: &[(u8, Vec<Option<u64>>)]) -> Option<String> {
    let mut hash = Sha256::new();
    hash.update(b"nika-authoring-prompt-shape-v1\0");
    hash.update(u64::try_from(messages.len()).ok()?.to_be_bytes());
    let mut text_bytes = 0u64;
    for (role, blocks) in messages {
        hash.update([*role]);
        hash.update(u64::try_from(blocks.len()).ok()?.to_be_bytes());
        for block in blocks {
            match block {
                Some(len) => {
                    text_bytes = text_bytes.checked_add(*len)?;
                    hash.update(b"T");
                    hash.update(len.to_be_bytes());
                }
                None => hash.update(b"O"),
            }
        }
    }
    Some(format!("{:x}", hash.finalize()))
}

/// The v1 role byte; a role this table does not name has no shape.
fn role_byte(role: &str) -> Option<u8> {
    match role {
        "System" => Some(b'S'),
        "User" => Some(b'U'),
        "Assistant" => Some(b'A'),
        "Tool" => Some(b'T'),
        _ => None,
    }
}

fn shape(messages: &[(&str, Vec<Block<'_>>)]) -> Option<String> {
    let lengths = messages
        .iter()
        .map(|(role, blocks)| {
            let lens = blocks
                .iter()
                .map(|b| match b {
                    Block::Text(t) => Some(u64::try_from(t.len()).unwrap()),
                    Block::Other => None,
                })
                .collect();
            role_byte(role).map(|r| (r, lens))
        })
        .collect::<Option<Vec<_>>>()?;
    shape_of(&lengths)
}

/// The private response framing: per Text block, its byte length, `:`, its bytes.
fn framed(texts: &[&str]) -> String {
    let mut framed = String::new();
    for text in texts {
        let _ = write!(framed, "{}:{text}", text.len());
    }
    sha(&framed)
}

/// A provider message as the reference reads it (the role's name, every block in order).
fn abstract_of(message: &Message) -> (&'static str, Vec<Block<'_>>) {
    let role = match message.role {
        Role::System => "System",
        Role::User => "User",
        Role::Assistant => "Assistant",
        Role::Tool => "Tool",
        _ => "unknown",
    };
    let blocks = message
        .content
        .iter()
        .map(|b| match b {
            ContentBlock::Text { text } => Block::Text(text),
            _ => Block::Other,
        })
        .collect();
    (role, blocks)
}

/// An abstract prompt: each message's role name and its blocks.
type Prompt<'a> = Vec<(&'a str, Vec<Block<'a>>)>;

/// `(name, prompt, legacy sha256, shape v1 sha256)`, computed with Python's hashlib.
type Vector<'a> = (&'a str, Prompt<'a>, &'a str, &'a str);

fn prompt_vectors() -> Vec<Vector<'static>> {
    use Block::{Other as O, Text as T};
    let all_roles = vec![
        ("System", vec![T("Hello")]),
        ("User", vec![T("é✨"), T("")]),
        ("Assistant", vec![T("{}")]),
        ("Tool", vec![O, T("x")]),
    ];
    vec![
        (
            "all_roles",
            all_roles.clone(),
            "39fc91dd5c4bba28f3a862d504d379f435fe8b27253a16dbd301b12606fc9f22",
            "d00e32b71aaaf9b8b5a4b952205f57de43b9e34e7d52877d5cb1cde895365055",
        ),
        (
            "split_ab_c",
            vec![("User", vec![T("ab"), T("c")])],
            "fddad493820cfb0fdb5c7ebd244b14f48635818b0dfe64d72850dfa5461ecf6b",
            "708e92ae338176ed5cf863664d33f4bd7931b4ae11e036ea316091a751a5e59a",
        ),
        (
            "split_a_bc",
            vec![("User", vec![T("a"), T("bc")])],
            "fddad493820cfb0fdb5c7ebd244b14f48635818b0dfe64d72850dfa5461ecf6b",
            "088ae5b3d17a64b2cdf340dfc592d84ade37fce1ea56d88b6b7590e9d341ead3",
        ),
        (
            "joined_abc",
            vec![("User", vec![T("abc")])],
            "fddad493820cfb0fdb5c7ebd244b14f48635818b0dfe64d72850dfa5461ecf6b",
            "f97fb5773f5bd3b8db6f49deba4b5a589256a6f5c4ad93addf6bdd2af032cc0c",
        ),
        (
            "no_messages",
            vec![],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "21e3269ff166555b7856c91b5d3e91dde6ec3209ef2e0e8afba1283769d7d888",
        ),
        (
            "no_blocks",
            vec![("User", vec![])],
            "75bdefa877c56ded66fead01db5f8d27409a89707ca90579a6e77b5297dece3c",
            "06b6be68313d5ace9fc61caa935922cf5e3f642c15258294bdf64e92a5f42cef",
        ),
        (
            "only_other",
            vec![("User", vec![O, O])],
            "75bdefa877c56ded66fead01db5f8d27409a89707ca90579a6e77b5297dece3c",
            "880584568422239c7ecd0172138f6ff258db6dd7e8a009b8aaf735197da05439",
        ),
        (
            "empty_text",
            vec![("User", vec![T("")])],
            "75bdefa877c56ded66fead01db5f8d27409a89707ca90579a6e77b5297dece3c",
            "c2594c5415ff5f15f76132eee9f1b3c2223428ad7025d925de72ddab381c2482",
        ),
        (
            "roles_swapped",
            vec![
                ("User", vec![T("Hello")]),
                ("System", vec![T("é✨"), T("")]),
                ("Assistant", vec![T("{}")]),
                ("Tool", vec![O, T("x")]),
            ],
            "ad8d9cefaf371c185355ad0dd50cb307e3fac1ad5e0f6e308030ea9a359c8470",
            "3570db3361ea0b227d150c928dd24d1c36505b75d7e12e83ebc5c228231e70f9",
        ),
    ]
}

#[test]
fn the_reference_framings_match_the_independent_vectors() {
    for (name, messages, want_legacy, want_shape) in &prompt_vectors() {
        assert_eq!(legacy(messages), *want_legacy, "legacy {name}");
        assert_eq!(
            shape(messages).as_deref(),
            Some(*want_shape),
            "shape {name}"
        );
    }
    // A role outside the v1 table has no shape at all, never a guessed byte.
    assert_eq!(shape(&[("Narrator", vec![Block::Text("x")])]), None);
}

#[test]
fn the_reference_response_framing_matches_the_independent_vectors() {
    // A block split moves the shape, never the legacy identity; an empty Text, zero blocks
    // and non-Text blocks are three shapes over one legacy identity.
    let responses: [(&[&str], &str); 5] = [
        (
            &[],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            &[""],
            "ba768b331fd86cec803be04e56ab2b3d4c0e98ef4ee4fcd4e72ad7cce61a1d1f",
        ),
        (
            &["", ""],
            "390feabc786e369e55b904251d643b52b52b691c60eb74a498ef7c6df993bf12",
        ),
        (
            &["é✨", "{}"],
            "894615f6b9a9d501669ba450af05114a74358b5ac4e20a32140afa017bf2d627",
        ),
        (
            &["é✨{}"],
            "d62df112a864f6e7c23784646944b1f3d5be0d24097551e7e650bcea712c3856",
        ),
    ];
    for (texts, want) in responses {
        assert_eq!(framed(texts), want, "framed {texts:?}");
    }
}

#[test]
fn an_overflowing_shape_count_is_unknown_not_a_partial_digest() {
    // Lengths only: nothing of this size is ever allocated.
    let max = Some(u64::MAX);
    assert_eq!(shape_of(&[(b'U', vec![max, Some(1)])]), None);
    assert_eq!(shape_of(&[(b'U', vec![max]), (b'A', vec![max])]), None);
    assert!(shape_of(&[(b'U', vec![max])]).is_some());
    assert!(shape_of(&[(b'U', vec![Some(u64::MAX - 1), Some(1)])]).is_some());
}

// ── A scripted provider that keeps every request and answers with whole responses ───────────

enum Answer {
    Response(Box<InferResponse>),
    Fail,
    Refuse,
    Hang,
}

/// Answers its script in order (the last answer repeats); keeps every request's messages and
/// answer schema.
struct Canned {
    answers: Vec<Answer>,
    requests: std::sync::Mutex<Vec<(Vec<Message>, String)>>,
}

impl Canned {
    fn new(answers: Vec<Answer>) -> Self {
        Self {
            answers,
            requests: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn texts(texts: &[String]) -> Self {
        Self::new(
            texts
                .iter()
                .map(|t| Answer::Response(Box::new(text(t))))
                .collect(),
        )
    }

    fn requests(&self) -> Vec<(Vec<Message>, String)> {
        self.requests.lock().unwrap().clone()
    }
}

impl ProviderInferDyn for Canned {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let schema = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema.to_string(),
            _ => String::new(),
        };
        let index = {
            let mut requests = self.requests.lock().unwrap();
            requests.push((request.messages.clone(), schema));
            requests.len() - 1
        };
        match &self.answers[index.min(self.answers.len() - 1)] {
            Answer::Response(response) => Ok((**response).clone()),
            Answer::Fail => Err(ProviderError::Other {
                reason: "scripted failure".to_owned(),
            }),
            Answer::Refuse => Err(ProviderError::AdmissionDenied {
                reason: "scripted local admission refusal".to_owned(),
            }),
            Answer::Hang => std::future::pending().await,
        }
    }
}

fn response(content: Vec<ContentBlock>) -> InferResponse {
    InferResponse::new(content, TokenUsage::new(100, 50), StopReason::EndTurn)
}

fn text(text: &str) -> InferResponse {
    response(vec![ContentBlock::Text {
        text: text.to_owned(),
    }])
}

fn block(text: &str) -> ContentBlock {
    ContentBlock::Text {
        text: text.to_owned(),
    }
}

// ── What a test sink keeps of one observation (owned copies made inside the callback) ───────

#[derive(Clone, Debug, Default, PartialEq)]
struct Meta {
    ordinal: u32,
    role: String,
    prompt: String,
    schema: String,
    shape: Option<String>,
    counts: [Option<u64>; 4],
    instruction: String,
    usage_reported: Option<bool>,
    tokens: [Option<u64>; 3],
    model: Option<String>,
    stop: (Option<String>, Option<String>),
    texts: Option<Vec<String>>,
    len_empty: Option<(usize, bool)>,
    other_blocks: Option<usize>,
    framed: Option<String>,
    failure: Option<Failure>,
}

fn keep(o: &AuthoringObservation<'_>) -> Meta {
    let mut meta = Meta {
        ordinal: o.ordinal,
        role: o.role.to_owned(),
        prompt: o.prompt_sha256.to_owned(),
        schema: o.schema_sha256.to_owned(),
        shape: o.prompt_shape_sha256.map(str::to_owned),
        counts: [
            o.prompt_messages,
            o.prompt_text_blocks,
            o.prompt_other_blocks,
            o.prompt_text_bytes,
        ],
        instruction: o.instruction_sha256.to_owned(),
        usage_reported: o.usage_reported,
        tokens: [o.input_tokens, o.output_tokens, o.reasoning_tokens],
        model: o.response_model.map(str::to_owned),
        stop: (
            o.stop_reason_kind.map(str::to_owned),
            o.stop_reason_detail.map(str::to_owned),
        ),
        ..Meta::default()
    };
    match &o.answered {
        Answered::Text {
            blocks,
            other_blocks,
            framed_sha256,
        } => {
            meta.texts = Some(blocks.iter().map(str::to_owned).collect());
            meta.len_empty = Some((blocks.len(), blocks.is_empty()));
            meta.other_blocks = Some(*other_blocks);
            meta.framed = Some(framed_sha256.clone());
        }
        Answered::NoResponse(failure) => meta.failure = Some(*failure),
        _ => {}
    }
    meta
}

fn recorder() -> (Arc<std::sync::Mutex<Vec<Meta>>>, Sink) {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = Arc::clone(&seen);
    let sink: Sink = Arc::new(move |o: &AuthoringObservation<'_>| {
        kept.lock().unwrap().push(keep(o));
    });
    (seen, sink)
}

fn sketch_policy(timeout: Duration) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, timeout)
        .with_native(NativeMode::Sketch)
        .with_repairs(1)
}

fn sketch_request(timeout: Duration) -> CompileRequest {
    CompileRequest::create(SKETCH_INTENT).with_authoring_policy(sketch_policy(timeout))
}

/// A refused sketch (an unknown task key), the accepted sketch, its fills: three calls whose
/// prompts carry System, User and Assistant messages.
fn three_calls() -> Vec<String> {
    let mut refused = recap_sketch();
    refused["tasks"][0]["zz_unknown"] = json!("x");
    vec![
        refused.to_string(),
        recap_sketch().to_string(),
        json!({"fills": valid_fills(), "notes": "fills"}).to_string(),
    ]
}

async fn observed(answers: Vec<Answer>) -> (CompileOutcome, Vec<Meta>, Canned) {
    let provider = Canned::new(answers);
    let (seen, sink) = recorder();
    let request = sketch_request(Duration::from_secs(2));
    let out = observe_authoring(sink, Box::pin(compile_with_provider(&request, &provider)))
        .await
        .unwrap();
    let seen = seen.lock().unwrap().clone();
    (out, seen, provider)
}

// ── Product parity with the reference ───────────────────────────────────────────────────────

/// The legacy prompt and response identities the pre-change observer produced for [`three_calls`]:
/// streaming them must leave every byte of the identity unchanged. The third prompt is the fill
/// call's: its baseline was rederived on 2026-10-04 from the legacy framing over the request
/// messages once the fill instruction gained the clause on a program bound to several edges
/// (`sketch.rs::holes_message`); without that clause the same framing gives the former
/// `13cce2041aeb39f23ea3b32e0a8f099e0f40556aa561f78c28fc2a2bb2540f60`.
const PRE_CHANGE_PROMPTS: [&str; 3] = [
    "0e4a52651458457939dabd8f6cd7d180694e878379a3c93c224200362e32e9d2",
    "a88d8dbd44640c28802bed0c058d02671f5d54ebc8376193e0d12baf290d0976",
    "6cf4dfc3521a4dbaeb8a8500470f2cd8789b3709d1f3c0af6924dc06325a0d97",
];
const PRE_CHANGE_FRAMED: [&str; 3] = [
    "6efb41e17fac4b9d131098b896e9ce39723cc35e3d5038daa6653e604b5c176a",
    "505c1a6fea337d067bd2d709a57504b68e7ac52c30007f0b8b8e3c31ad02c1e6",
    "6c601d5edc119a56186f3f871f61a6e7ffc340b4c42970d737dc8c06e4203bb8",
];

#[tokio::test]
async fn every_identity_matches_the_reference_over_the_exact_request() {
    let provider = Canned::texts(&three_calls());
    let (seen, sink) = recorder();
    let request = sketch_request(Duration::from_secs(2));
    observe_authoring(sink, Box::pin(compile_with_provider(&request, &provider)))
        .await
        .unwrap();
    let seen = seen.lock().unwrap().clone();
    let requests = provider.requests();
    assert_eq!(seen.len(), 3, "{seen:?}");
    let mut roles = std::collections::BTreeSet::new();
    for (meta, (messages, schema)) in seen.iter().zip(&requests) {
        let abstracted: Vec<_> = messages.iter().map(abstract_of).collect();
        roles.extend(abstracted.iter().map(|(role, _)| *role));
        assert_eq!(meta.prompt, legacy(&abstracted), "{}", meta.role);
        assert_eq!(meta.shape, shape(&abstracted), "{}", meta.role);
        let blocks = abstracted.iter().flat_map(|(_, b)| b.iter());
        let texts: Vec<&str> = blocks
            .clone()
            .filter_map(|b| match b {
                Block::Text(t) => Some(*t),
                Block::Other => None,
            })
            .collect();
        let count = |n: usize| Some(u64::try_from(n).unwrap());
        assert_eq!(
            meta.counts,
            [
                count(messages.len()),
                count(texts.len()),
                count(blocks.count() - texts.len()),
                count(texts.iter().map(|t| t.len()).sum()),
            ]
        );
        assert_eq!(meta.schema, sha(schema));
        let instruction = messages
            .iter()
            .find(|m| matches!(m.role, Role::System))
            .map(|m| abstract_of(m).1)
            .unwrap_or_default()
            .iter()
            .filter_map(|b| match b {
                Block::Text(t) => Some(*t),
                Block::Other => None,
            })
            .collect::<String>();
        assert_eq!(meta.instruction, sha(&instruction));
    }
    assert_eq!(
        roles.into_iter().collect::<Vec<_>>(),
        ["Assistant", "System", "User"],
        "the repair prompt carries the refused answer back"
    );
    let replies = three_calls();
    for (meta, reply) in seen.iter().zip(&replies) {
        assert_eq!(meta.framed.as_deref(), Some(framed(&[reply]).as_str()));
        assert_eq!(meta.texts.as_deref(), Some(&[reply.clone()][..]));
    }
    assert_eq!(
        seen.iter().map(|m| m.prompt.as_str()).collect::<Vec<_>>(),
        PRE_CHANGE_PROMPTS
    );
    assert_eq!(
        seen.iter()
            .map(|m| m.framed.as_deref().unwrap())
            .collect::<Vec<_>>(),
        PRE_CHANGE_FRAMED
    );
}

#[tokio::test]
async fn no_response_zero_text_and_empty_text_are_three_different_answers() {
    let (_, none, _) = observed(vec![Answer::Fail]).await;
    let (_, refused, _) = observed(vec![Answer::Refuse]).await;
    let zero = response(vec![
        ContentBlock::ToolUse {
            id: "t".to_owned(),
            name: "x".to_owned(),
            input: json!({}),
        },
        ContentBlock::Thinking {
            text: "hidden ✨".to_owned(),
        },
    ]);
    let (_, zero, _) = observed(vec![Answer::Response(Box::new(zero))]).await;
    let (_, empty, _) = observed(vec![Answer::Response(Box::new(text("")))]).await;
    let mixed = response(vec![
        ContentBlock::Thinking {
            text: "t".to_owned(),
        },
        block("é✨"),
        ContentBlock::Image {
            source: "cas:x".to_owned(),
            detail: None,
        },
        block("{}"),
        ContentBlock::ToolResult {
            tool_use_id: "t".to_owned(),
            content: "r".to_owned(),
            is_error: false,
        },
    ]);
    let (_, mixed, _) = observed(vec![Answer::Response(Box::new(mixed))]).await;

    assert_eq!(none[0].failure, Some(Failure::ProviderError));
    assert_eq!(refused[0].failure, Some(Failure::AdmissionRefused));
    for meta in [&none[0], &refused[0]] {
        assert_eq!((meta.texts.as_ref(), meta.framed.as_ref()), (None, None));
        assert_eq!(
            meta.usage_reported, None,
            "no response is not a reported usage"
        );
        assert_eq!(meta.tokens, [None; 3]);
        assert_eq!((meta.model.as_ref(), &meta.stop), (None, &(None, None)));
    }
    let shapes = |m: &Meta| {
        (
            m.texts.clone(),
            m.len_empty,
            m.other_blocks,
            m.framed.clone(),
        )
    };
    assert_eq!(
        shapes(&zero[0]),
        (Some(vec![]), Some((0, true)), Some(2), Some(framed(&[])))
    );
    assert_eq!(
        shapes(&empty[0]),
        (
            Some(vec![String::new()]),
            Some((1, false)),
            Some(0),
            Some(framed(&[""]))
        )
    );
    assert_eq!(
        shapes(&mixed[0]),
        (
            Some(vec!["é✨".to_owned(), "{}".to_owned()]),
            Some((2, false)),
            Some(3),
            Some(framed(&["é✨", "{}"]))
        )
    );
}

#[tokio::test]
async fn a_timeout_is_observed_as_no_response() {
    let provider = Canned::new(vec![Answer::Hang]);
    let (seen, sink) = recorder();
    let request = sketch_request(Duration::from_millis(30));
    observe_authoring(sink, Box::pin(compile_with_provider(&request, &provider)))
        .await
        .unwrap();
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0].failure, Some(Failure::Timeout));
    assert_eq!(seen[0].usage_reported, None);
}

#[tokio::test]
async fn usage_absent_unreported_and_reported_zero_stay_distinct() {
    let mut zero = text("{}");
    zero.usage = TokenUsage::new(0, 0);
    let mut unreported = text("{}");
    unreported.usage_reported = false;
    let mut reasoned = text("{}");
    reasoned.usage.reasoning_tokens = Some(7);
    reasoned.gen_ai.response_model = Some("mock/served-é ✨".to_owned());
    let mut cases = Vec::new();
    for response in [zero, unreported, reasoned] {
        let (_, seen, _) = observed(vec![Answer::Response(Box::new(response))]).await;
        cases.push((
            seen[0].usage_reported,
            seen[0].tokens,
            seen[0].model.clone(),
        ));
    }
    assert_eq!(
        cases,
        [
            (Some(true), [Some(0), Some(0), None], None),
            (Some(false), [None, None, None], None),
            (
                Some(true),
                [Some(100), Some(50), Some(7)],
                Some("mock/served-é ✨".to_owned())
            ),
        ]
    );
}

#[tokio::test]
async fn every_known_stop_has_its_word_and_an_unknown_one_keeps_only_its_detail() {
    let stops = [
        (StopReason::EndTurn, "end_turn", None),
        (StopReason::MaxTokens, "max_tokens", None),
        (StopReason::StopSequence, "stop_sequence", None),
        (StopReason::ToolUse, "tool_use", None),
        (StopReason::ContentFilter, "content_filter", None),
        (
            StopReason::Unknown("vendor_x « é »".to_owned()),
            "unknown",
            Some("vendor_x « é »"),
        ),
    ];
    for (stop, kind, detail) in stops {
        let mut answer = text("{}");
        answer.stop_reason = stop;
        let (_, seen, _) = observed(vec![Answer::Response(Box::new(answer))]).await;
        assert_eq!(
            seen[0].stop,
            (Some(kind.to_owned()), detail.map(str::to_owned)),
            "{kind}"
        );
    }
}

// ── Hermetic callback controls ───────────────────────────────────────────────────────────────

/// The public record without its wall-clock fields: everything a compile decides and accounts.
fn decided(out: &CompileOutcome) -> Value {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(map) => {
                map.retain(|k, _| k != "elapsed_ms");
                map.values_mut().for_each(strip);
            }
            Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut document = outcome_document(out);
    strip(&mut document);
    document
}

#[tokio::test]
async fn an_observer_on_off_or_declining_changes_nothing_decided_or_requested() {
    let run = |sink: Option<Sink>| async move {
        let provider = Canned::texts(&three_calls());
        let request = sketch_request(Duration::from_secs(2));
        let out = match sink {
            Some(sink) => {
                observe_authoring(sink, Box::pin(compile_with_provider(&request, &provider))).await
            }
            None => compile_with_provider(&request, &provider).await,
        }
        .unwrap();
        (
            decided(&out),
            out.status,
            out.candidate.clone(),
            provider.requests(),
        )
    };
    let (recorded, _) = {
        let (seen, sink) = recorder();
        (run(Some(sink)).await, seen)
    };
    let noop: Sink = Arc::new(|_: &AuthoringObservation<'_>| {});
    let declining: Sink = Arc::new(|o: &AuthoringObservation<'_>| {
        // Reads a fact and keeps nothing: the trusted sink declines without panicking.
        let _ = o.response_model.is_some();
    });
    let off = run(None).await;
    for (name, other) in [
        ("noop", run(Some(noop)).await),
        ("declining", run(Some(declining)).await),
        ("recorded", recorded),
    ] {
        assert_eq!(other.0, off.0, "{name}: the public record");
        assert_eq!(other.1, off.1, "{name}: status");
        assert_eq!(other.2, off.2, "{name}: candidate");
        let summary = |r: &[(Vec<Message>, String)]| -> Vec<(String, String)> {
            r.iter()
                .map(|(m, s)| {
                    (
                        legacy(&m.iter().map(abstract_of).collect::<Vec<_>>()),
                        s.clone(),
                    )
                })
                .collect()
        };
        assert_eq!(summary(&other.3), summary(&off.3), "{name}: requests");
    }
}

#[tokio::test]
async fn concurrent_scopes_keep_their_own_returned_facts() {
    let model = |name: &str| {
        let mut answer = text("{}");
        answer.gen_ai.response_model = Some(name.to_owned());
        Answer::Response(Box::new(answer))
    };
    let (left, left_sink) = recorder();
    let (right, right_sink) = recorder();
    let left_provider = Canned::new(vec![model("left")]);
    let right_provider = Canned::new(vec![model("right")]);
    let (lr, rr) = (
        sketch_request(Duration::from_secs(2)),
        sketch_request(Duration::from_secs(2)),
    );
    let (a, b) = tokio::join!(
        observe_authoring(
            left_sink,
            Box::pin(compile_with_provider(&lr, &left_provider))
        ),
        observe_authoring(
            right_sink,
            Box::pin(compile_with_provider(&rr, &right_provider))
        ),
    );
    a.unwrap();
    b.unwrap();
    for (seen, want) in [(left, "left"), (right, "right")] {
        let seen = seen.lock().unwrap().clone();
        assert!(!seen.is_empty());
        assert!(
            seen.iter().all(|m| m.model.as_deref() == Some(want)),
            "{seen:?}"
        );
        assert_eq!(seen[0].ordinal, 1, "each scope counts its own calls");
    }
}

#[tokio::test]
async fn a_returned_response_is_observed_whole_even_if_the_compile_is_then_dropped() {
    let mut first = text(&three_calls()[0]);
    first.gen_ai.response_model = Some("mock/first".to_owned());
    first.usage.reasoning_tokens = Some(3);
    let provider = Canned::new(vec![Answer::Response(Box::new(first)), Answer::Hang]);
    let (seen, sink) = recorder();
    let request = sketch_request(Duration::from_secs(30));
    let compile = observe_authoring(sink, Box::pin(compile_with_provider(&request, &provider)));
    // The second call never answers: the outer deadline drops the compile mid-flight.
    let dropped = tokio::time::timeout(Duration::from_millis(300), compile).await;
    assert!(dropped.is_err(), "the compile was cancelled, not finished");
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0].model.as_deref(), Some("mock/first"));
    assert_eq!(seen[0].stop.0.as_deref(), Some("end_turn"));
    assert_eq!(seen[0].tokens, [Some(100), Some(50), Some(3)]);
    assert_eq!(
        seen[0].texts.as_deref(),
        Some(&[three_calls()[0].clone()][..])
    );
    assert_eq!(
        provider.requests().len(),
        2,
        "the second call was asked, never answered"
    );
}
