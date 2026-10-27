// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Spec 02 §Harness images on the verb (B-D18) and a seat's `schema:` (D2): the images whose
//! bytes this session received become `.output.images`, written through `nika:write` under
//! `images.save`; a reported path is never one; a declared save with nothing received fails
//! `NIKA-AGENT-007`; the native loop refuses `images:`; a seat's final message is validated.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use base64::Engine as _;
use nika_error::traits::NikaErrorCode as _;
use nika_kernel::ai::harness::{
    DynAgentBackend, HarnessError, HarnessEvent, HarnessEventStream, HarnessImage, HarnessOutcome,
    HarnessRequest,
};
use nika_kernel::blob::{BlobError, BlobMetadata, BlobStoreDyn};
use nika_kernel::{ToolErrorMeta, ToolResult};
use nika_kernel_mock::{MockProvider, MockToolDefinitionProvider, MockToolExecutor};
use sha2::Digest as _;

use super::*;
use crate::harness_path::HarnessSeat;

/// A tape the seat plays back, and the requests it was asked.
struct Tape {
    tape: Mutex<Vec<HarnessEvent>>,
    asked: Mutex<Vec<HarnessRequest>>,
}

struct Playback(Vec<HarnessEvent>);

impl futures_core::Stream for Playback {
    type Item = Result<HarnessEvent, HarnessError>;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.0.is_empty() {
            Poll::Ready(None)
        } else {
            Poll::Ready(Some(Ok(self.0.remove(0))))
        }
    }
}

impl DynAgentBackend for Tape {
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>> {
        self.asked.lock().unwrap().push(request);
        let tape = std::mem::take(&mut *self.tape.lock().unwrap());
        Box::pin(async move { Ok(Box::pin(Playback(tape)) as HarnessEventStream) })
    }
}

/// An in-memory image store answering every put.
#[derive(Default)]
struct Kept(Mutex<Vec<bytes::Bytes>>);

impl BlobStoreDyn for Kept {
    fn put(
        &self,
        data: bytes::Bytes,
        mime_type: &str,
    ) -> impl Future<Output = Result<BlobMetadata, BlobError>> + Send {
        let meta = BlobMetadata::new("blake3:kept", mime_type, data.len() as u64);
        self.0.lock().unwrap().push(data);
        std::future::ready(Ok(meta))
    }
    fn get(&self, hash: &str) -> impl Future<Output = Result<bytes::Bytes, BlobError>> + Send {
        let hash = hash.to_owned();
        std::future::ready(Err(BlobError::NotFound { hash }))
    }
    fn exists(&self, _: &str) -> impl Future<Output = bool> + Send {
        std::future::ready(false)
    }
    fn stat(&self, hash: &str) -> impl Future<Output = Result<BlobMetadata, BlobError>> + Send {
        let hash = hash.to_owned();
        std::future::ready(Err(BlobError::NotFound { hash }))
    }
    fn delete(&self, hash: &str) -> impl Future<Output = Result<(), BlobError>> + Send {
        let hash = hash.to_owned();
        std::future::ready(Err(BlobError::NotFound { hash }))
    }
}

/// A recording observer.
#[derive(Default)]
struct Seen(Mutex<Vec<AgentEvent>>);

impl AgentObserver for Seen {
    fn on_event(&self, event: &AgentEvent) {
        let mut log = self.0.lock().unwrap();
        AgentEvent::record_into(&mut log, event);
    }
}

/// A 2×3 PNG header and a tail: enough for the media bound and the dimensions.
fn png(tail: &[u8]) -> bytes::Bytes {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    bytes.extend_from_slice(&2_u32.to_be_bytes());
    bytes.extend_from_slice(&3_u32.to_be_bytes());
    bytes.extend_from_slice(tail);
    bytes.into()
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

/// An image received on the session: bytes, MIME and their digest, as the client reports them.
fn received(id: &str, bytes: &bytes::Bytes) -> HarnessEvent {
    let mut image = HarnessImage::new(id);
    image.data = Some(bytes.clone());
    image.received_bytes = Some(bytes.len() as u64);
    image.mime_type = Some("image/png".to_owned());
    image.sha256 = Some(sha(bytes));
    image.reported_saved_path = Some("/Users/someone/.codex/generated_images/other/x.png".into());
    HarnessEvent::ImageObserved {
        image: Box::new(image),
    }
}

/// An image the harness only REPORTED (a path, no bytes).
fn reported(id: &str) -> HarnessEvent {
    let mut image = HarnessImage::new(id);
    image.reported_saved_path = Some("/Users/someone/.codex/generated_images/x.png".into());
    HarnessEvent::ImageObserved {
        image: Box::new(image),
    }
}

fn completed(text: &str) -> HarnessEvent {
    HarnessEvent::Completed {
        outcome: Box::new(HarnessOutcome::new(text)),
    }
}

type Verb = AgentVerb<MockProvider, MockToolExecutor, MockToolDefinitionProvider>;

/// A seated verb over `tape`, its images kept, its tools the recording mock.
fn seated(
    tape: Vec<HarnessEvent>,
    tools: MockToolExecutor,
) -> (Verb, Arc<Tape>, Arc<MockToolExecutor>) {
    let backend = Arc::new(Tape {
        tape: Mutex::new(tape),
        asked: Mutex::new(Vec::new()),
    });
    let tools = Arc::new(tools);
    let verb = AgentVerb::new(
        Arc::new(MockProvider::new("mock")),
        Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::clone(&tools))),
        Arc::new(MockToolDefinitionProvider::new()),
        "openai/gpt-6-luna",
    )
    .with_harness_seat(HarnessSeat::new(backend.clone(), "/tmp"))
    .with_harness_image_store(Arc::new(Kept::default()));
    (verb, backend, tools)
}

fn saving(path: &str) -> AgentInput {
    let mut input = AgentInput::new("draw the banner");
    input.images = Some(AgentImages::new(Some(path.to_owned())));
    input
}

/// B-D18 · the received bytes are the value and the file: `nika:write` gets exactly those
/// bytes at the declared path (then `-2` for the second), the value carries their digest, size
/// and header dimensions, and each observation settles with its `saved_path`.
#[tokio::test]
async fn received_images_become_the_value_and_are_written_where_declared() {
    let (first, second) = (png(b"first"), png(b"second"));
    let (verb, _, tools) = seated(
        vec![
            received("ig-1", &first),
            received("ig-2", &second),
            completed("here is your banner"),
        ],
        MockToolExecutor::new()
            .enqueue_ok(ToolResult::success("w1", "./out/banner.png"))
            .enqueue_ok(ToolResult::success("w2", "./out/banner-2.png")),
    );
    let seen = Seen::default();
    let out = verb
        .run_observed(saving("./out/banner.png"), &seen)
        .await
        .expect("delivered");
    let AgentValue::Structured(value) = &out.output else {
        panic!("{:?}", out.output);
    };
    assert_eq!(value["message"], "here is your banner");
    let images = value["images"].as_array().expect("images");
    assert_eq!(images.len(), 2);
    for (index, (bytes, path)) in [
        (&first, "./out/banner.png"),
        (&second, "./out/banner-2.png"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            images[index],
            serde_json::json!({"index": index, "path": path, "sha256": sha(bytes),
                "size_bytes": bytes.len(), "mime_type": "image/png", "width": 2, "height": 3})
        );
    }
    let calls = tools.captured_calls();
    assert_eq!(calls.len(), 2, "one write per received image, nothing else");
    for (call, (bytes, path)) in calls.iter().zip([
        (&first, "./out/banner.png"),
        (&second, "./out/banner-2.png"),
    ]) {
        assert_eq!(call.name, "nika:write");
        assert_eq!(call.input["path"], path);
        assert_eq!(call.input["overwrite"], true);
        let written = base64::engine::general_purpose::STANDARD
            .decode(
                call.input["content"]["bytes_base64"]
                    .as_str()
                    .expect("bytes"),
            )
            .expect("base64");
        assert_eq!(
            written,
            bytes.as_ref(),
            "the very bytes received, never a reported path"
        );
    }
    let log = seen.0.lock().unwrap();
    let saved: Vec<(Option<String>, Option<String>)> = log
        .iter()
        .filter_map(AgentEvent::harness_image)
        .map(|image| {
            let observation = image.observation();
            (
                observation["saved_path"].as_str().map(str::to_owned),
                observation["received_sha256"].as_str().map(str::to_owned),
            )
        })
        .collect();
    assert_eq!(
        saved,
        [
            (Some("./out/banner.png".to_owned()), Some(sha(&first))),
            (Some("./out/banner-2.png".to_owned()), Some(sha(&second))),
        ],
        "each observation settles with where its bytes were written"
    );
}

/// The value without `save:`: listed, digested, never written.
#[tokio::test]
async fn without_save_the_images_are_a_value_only() {
    let bytes = png(b"only");
    let (verb, _, tools) = seated(
        vec![received("ig-1", &bytes), completed("done")],
        MockToolExecutor::new(),
    );
    let mut input = AgentInput::new("draw");
    input.images = Some(AgentImages::new(None));
    let out = verb.run(input).await.expect("delivered");
    let AgentValue::Structured(value) = &out.output else {
        panic!("{:?}", out.output);
    };
    assert_eq!(value["images"][0]["path"], serde_json::Value::Null);
    assert_eq!(value["images"][0]["sha256"], sha(&bytes));
    assert!(tools.captured_calls().is_empty(), "nothing is written");
}

/// B-D18 · a path the harness reported is never opened, listed or saved: with nothing
/// received, a declared save fails NIKA-AGENT-007 — and no write is ever attempted.
#[tokio::test]
async fn a_reported_path_alone_is_never_saved_and_the_task_fails_agent_007() {
    let (verb, _, tools) = seated(
        vec![
            reported("ig-1"),
            completed("saved to ~/.codex/generated_images/x.png"),
        ],
        MockToolExecutor::new(),
    );
    let err = verb
        .run(saving("./out/banner.png"))
        .await
        .expect_err("nothing received");
    assert_eq!(err.spec_code(), "NIKA-AGENT-007");
    assert!(
        matches!(&err, VerbAgentError::NoImage { reported: 1, partial_output, .. }
            if partial_output.contains("generated_images")),
        "{err:?}"
    );
    assert!(
        tools.captured_calls().is_empty(),
        "the reported path is never a source"
    );
}

/// The boundary speaks first: a save outside `permits.fs.write` is the write builtin's own
/// NIKA-SEC-004, never fed anywhere and never a success.
#[tokio::test]
async fn a_save_the_boundary_refuses_fails_with_its_code() {
    let bytes = png(b"x");
    let refusal = ToolResult::error("w1", "path outside permits.fs.write")
        .with_error_meta(ToolErrorMeta::new(Some("NIKA-SEC-004".to_owned()), false));
    let (verb, _, _) = seated(
        vec![received("ig-1", &bytes), completed("done")],
        MockToolExecutor::new().enqueue_ok(refusal),
    );
    let err = verb
        .run(saving("../outside.png"))
        .await
        .expect_err("refused");
    assert_eq!(err.spec_code(), "NIKA-SEC-004");
    assert!(
        matches!(err, VerbAgentError::SecurityBoundary { .. }),
        "{err:?}"
    );
}

/// Spec 02 §Harness images · every path is judged before any byte: a grant that holds the
/// first image and not its `-2` sibling refuses the delivery with NIKA-SEC-004, and the first
/// image is never written either.
#[tokio::test]
async fn a_second_image_outside_the_grant_refuses_before_the_first_is_written() {
    let (verb, _, tools) = seated(
        vec![
            received("ig-1", &png(b"first")),
            received("ig-2", &png(b"second")),
            completed("two banners"),
        ],
        MockToolExecutor::new(),
    );
    let mut input = saving("./out/banner.png");
    let mut permits = nika_schema::types::Permits::new();
    permits.fs = Some(nika_schema::types::FsPermits::new(
        Vec::new(),
        vec!["./out/banner.png".to_owned()],
    ));
    input.permits = Some(permits);
    let err = verb.run(input).await.expect_err("no room for banner-2.png");
    assert_eq!(err.spec_code(), "NIKA-SEC-004");
    assert!(
        matches!(&err, VerbAgentError::SecurityBoundary { tool, .. } if tool == "nika:write"),
        "{err:?}"
    );
    assert!(
        tools.captured_calls().is_empty(),
        "nothing is written when any path is outside"
    );
}

/// Spec 02 §Harness images · a save names at most four files (the path, then `-2` to `-4`, the
/// paths every check judged): a turn that delivers a fifth image fails before any write, even
/// from a lent backend no ACP client bounded. The writer keeps its own cap.
#[tokio::test]
async fn a_fifth_received_image_refuses_the_save_before_any_write() {
    let mut tape: Vec<HarnessEvent> = (1..=5)
        .map(|n| received(&format!("ig-{n}"), &png(format!("image {n}").as_bytes())))
        .collect();
    tape.push(completed("five banners"));
    let (verb, _, tools) = seated(tape, MockToolExecutor::new());
    let err = verb
        .run(saving("./out/banner.png"))
        .await
        .expect_err("a save names at most four files");
    assert!(
        matches!(&err, VerbAgentError::Harness { source: HarnessError::Refused { reason }, .. }
            if reason.contains("four images") && reason.contains("nothing was written")),
        "{err:?}"
    );
    assert!(
        tools.captured_calls().is_empty(),
        "no image is written, the first four included: {:?}",
        tools.captured_calls()
    );
}

/// `images:` is a seat's field: the engine's own loop refuses it before any model call.
#[tokio::test]
async fn the_native_loop_refuses_images_before_any_call() {
    let provider = Arc::new(MockProvider::new("mock"));
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        Arc::new(nika_verb_invoke::InvokeVerb::new(Arc::new(
            MockToolExecutor::new(),
        ))),
        Arc::new(MockToolDefinitionProvider::new()),
        "mock/agent",
    );
    let err = verb
        .run(saving("./out/a.png"))
        .await
        .expect_err("native refuses images");
    assert!(
        matches!(&err, VerbAgentError::InvalidParam { param: "images", detail }
            if detail.contains("nika:image_generate")),
        "{err:?}"
    );
    assert!(provider.captured_requests().is_empty(), "no call was made");
}

/// D2 · a seat's `schema:`: the prompt carries it, the final message is validated, the value
/// is the object (and the `images` message under `images:`).
#[tokio::test]
async fn a_seat_schema_is_validated_on_the_final_message() {
    let bytes = png(b"s");
    let (verb, backend, _) = seated(
        vec![
            received("ig-1", &bytes),
            completed("Done!\n```json\n{\"caption\": \"a calm sea\"}\n```"),
        ],
        MockToolExecutor::new(),
    );
    let mut input = AgentInput::new("draw and caption");
    input.schema = Some(
        serde_json::json!({"type": "object", "required": ["caption"],
        "properties": {"caption": {"type": "string"}}}),
    );
    input.images = Some(AgentImages::new(None));
    let out = verb.run(input).await.expect("validated");
    let AgentValue::Structured(value) = &out.output else {
        panic!("{:?}", out.output);
    };
    assert_eq!(
        value["message"],
        serde_json::json!({"caption": "a calm sea"})
    );
    {
        let asked = backend.asked.lock().unwrap();
        assert!(
            asked[0].prompt.starts_with("draw and caption")
                && asked[0]
                    .prompt
                    .contains("ONLY a JSON value that satisfies this JSON Schema"),
            "{}",
            asked[0].prompt
        );
    }

    let (verb, _, _) = seated(vec![completed("no json at all")], MockToolExecutor::new());
    let mut input = AgentInput::new("caption");
    input.schema = Some(serde_json::json!({"type": "object"}));
    let err = verb.run(input).await.expect_err("never valid");
    assert_eq!(err.spec_code(), "NIKA-INFER-002");
    assert!(err.to_string().contains("without a repair turn"), "{err}");
}
