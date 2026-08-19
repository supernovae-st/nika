// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Spill seam tests (`spill.rs`) — split from `tests.rs` under the
//! 1500-LOC file cap · same `super::*` semantics.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use nika_kernel::ai::provider::{InferResponse, StopReason, TokenUsage, ToolDef};
use nika_kernel::blob::{BlobError, BlobMetadata, BlobStoreDyn};
use nika_kernel::runtime::tool_executor::ToolResult;
use nika_kernel_mock::{MockProvider, MockToolDefinitionProvider, MockToolExecutor};

use super::*;

/// Past the 16 KiB spill threshold.
const BIG: usize = 20 * 1024;

/// An in-memory spill store for these tests — `BlobStoreDyn`-native.
/// (`MockBlob` predates the Dyn variant and implements only the base
/// trait; a thin in-crate store keeps the test honest without touching
/// another crate's public surface.)
#[derive(Clone, Default)]
struct SpillMem {
    blobs: Arc<RwLock<BTreeMap<String, bytes::Bytes>>>,
    next: Arc<AtomicU64>,
}

impl SpillMem {
    // no .expect() here: to the hygiene ratchet these helpers are
    // production code, and a poisoned lock answers « empty » — the
    // assertion downstream says why.
    fn len(&self) -> usize {
        match self.blobs.read() {
            Ok(g) => g.len(),
            Err(_) => 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(&self, hash: &str) -> Option<bytes::Bytes> {
        match self.blobs.read() {
            Ok(g) => g.get(hash).cloned(),
            Err(_) => None,
        }
    }
}

impl BlobStoreDyn for SpillMem {
    fn put(
        &self,
        data: bytes::Bytes,
        mime_type: &str,
    ) -> impl std::future::Future<Output = Result<BlobMetadata, BlobError>> + Send {
        let n = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        let hash = format!("mock-{n:08x}");
        let meta = BlobMetadata::new(&hash, mime_type, data.len() as u64);
        match self.blobs.write() {
            Ok(mut g) => {
                g.insert(hash, data);
                std::future::ready(Ok(meta))
            }
            Err(_) => std::future::ready(Err(BlobError::Io {
                reason: "lock poisoned".to_owned(),
            })),
        }
    }

    fn get(
        &self,
        hash: &str,
    ) -> impl std::future::Future<Output = Result<bytes::Bytes, BlobError>> + Send {
        let found = match self.blobs.read() {
            Ok(g) => g.get(hash).cloned(),
            Err(_) => None,
        };
        let hash = hash.to_owned();
        std::future::ready(found.ok_or(BlobError::NotFound { hash }))
    }

    fn exists(&self, hash: &str) -> impl std::future::Future<Output = bool> + Send {
        let there = match self.blobs.read() {
            Ok(g) => g.contains_key(hash),
            Err(_) => false,
        };
        std::future::ready(there)
    }

    fn stat(
        &self,
        hash: &str,
    ) -> impl std::future::Future<Output = Result<BlobMetadata, BlobError>> + Send {
        let found = match self.blobs.read() {
            Ok(g) => g
                .get(hash)
                .map(|b| BlobMetadata::new(hash, "text/plain", b.len() as u64)),
            Err(_) => None,
        };
        let hash = hash.to_owned();
        std::future::ready(found.ok_or(BlobError::NotFound { hash }))
    }

    fn delete(
        &self,
        hash: &str,
    ) -> impl std::future::Future<Output = Result<(), BlobError>> + Send {
        let gone = match self.blobs.write() {
            Ok(mut g) => g.remove(hash),
            Err(_) => None,
        };
        let hash = hash.to_owned();
        std::future::ready(gone.map(|_| ()).ok_or(BlobError::NotFound { hash }))
    }
}

/// A store that always refuses — the «never a loss» path's witness.
struct FailingBlob;

impl BlobStoreDyn for FailingBlob {
    fn put(
        &self,
        _data: bytes::Bytes,
        _mime_type: &str,
    ) -> impl std::future::Future<Output = Result<BlobMetadata, BlobError>> + Send {
        std::future::ready(Err(BlobError::Io {
            reason: "test disk full".to_owned(),
        }))
    }

    fn get(
        &self,
        hash: &str,
    ) -> impl std::future::Future<Output = Result<bytes::Bytes, BlobError>> + Send {
        std::future::ready(Err(BlobError::NotFound {
            hash: hash.to_owned(),
        }))
    }

    fn exists(&self, _hash: &str) -> impl std::future::Future<Output = bool> + Send {
        std::future::ready(false)
    }

    fn stat(
        &self,
        hash: &str,
    ) -> impl std::future::Future<Output = Result<BlobMetadata, BlobError>> + Send {
        std::future::ready(Err(BlobError::NotFound {
            hash: hash.to_owned(),
        }))
    }

    fn delete(
        &self,
        hash: &str,
    ) -> impl std::future::Future<Output = Result<(), BlobError>> + Send {
        std::future::ready(Err(BlobError::NotFound {
            hash: hash.to_owned(),
        }))
    }
}

fn text_response(text: &str) -> InferResponse {
    InferResponse::new(
        vec![ContentBlock::Text {
            text: text.to_owned(),
        }],
        TokenUsage::default(),
        StopReason::EndTurn,
    )
}

fn tool_use_response(id: &str, name: &str) -> InferResponse {
    InferResponse::new(
        vec![
            ContentBlock::Text {
                text: format!("calling {name}"),
            },
            ContentBlock::ToolUse {
                id: id.to_owned(),
                name: name.to_owned(),
                input: serde_json::json!({}),
            },
        ],
        TokenUsage::default(),
        StopReason::ToolUse,
    )
}

fn def(name: &str) -> ToolDef {
    ToolDef::new(name, format!("{name} description"), serde_json::json!({}))
}

/// The rig with the spill store seated — `rig()`'s shape plus the seam.
fn spill_rig(
    provider: MockProvider,
    tools: MockToolExecutor,
) -> (
    Arc<MockProvider>,
    Arc<SpillMem>,
    AgentVerb<MockProvider, MockToolExecutor, MockToolDefinitionProvider>,
) {
    let provider = Arc::new(provider);
    let blob = Arc::new(SpillMem::default());
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        Arc::new(InvokeVerb::new(Arc::new(tools))),
        Arc::new(MockToolDefinitionProvider::with_defs(vec![def(
            "nika:read",
        )])),
        "mock/agent",
    )
    .with_spill(Arc::clone(&blob));
    (provider, blob, verb)
}

/// What the model was fed back through tool results at one request.
fn fed_results(provider: &MockProvider, request: usize) -> String {
    provider.captured_requests()[request]
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolResult { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn oversized_result_spills_with_locator_and_the_bytes_survive() {
    let big = "x".repeat(BIG);
    let (provider, blob, verb) = spill_rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response("call-1", "nika:read"))
            .enqueue_response(text_response("got the head")),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("call-1", big.clone())),
    );
    let mut input = AgentInput::new("read the big file");
    input.tools = vec!["nika:read".to_owned()];
    let out = verb.run(input).await.expect("completes");
    assert_eq!(out.turns, 2);

    // The record kept the bytes, content-addressed.
    assert_eq!(blob.len(), 1, "one spill, one blob");
    let hash = "mock-00000001";
    let back = blob.get(hash).expect("the locator resolves");
    assert_eq!(back.as_ref(), big.as_bytes(), "the store holds every byte");

    // What the model reads instead: a bounded preview + the pointer.
    let fed = fed_results(&provider, 1);
    assert!(
        fed.contains("[… spilled ·"),
        "the marker says what happened · {fed}"
    );
    assert!(fed.contains(hash), "the locator rides · {fed}");
    assert!(fed.contains(&BIG.to_string()), "the total size rides");
    assert!(
        !fed.contains(&"x".repeat(4 * 1024)),
        "the body left the conversation"
    );
}

#[tokio::test]
async fn ordinary_result_never_spills() {
    let (provider, blob, verb) = spill_rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response("call-1", "nika:read"))
            .enqueue_response(text_response("done")),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("call-1", "hello from notes")),
    );
    let mut input = AgentInput::new("read my notes");
    input.tools = vec!["nika:read".to_owned()];
    verb.run(input).await.expect("completes");

    assert!(blob.is_empty(), "nothing reaches the store");
    assert_eq!(fed_results(&provider, 1), "hello from notes");
}

#[tokio::test]
async fn without_the_seam_an_oversized_result_rides_byte_identical() {
    // The default (no `with_spill`) is the pre-spill behavior: the loop
    // feeds everything back, however big — the seam is opt-in.
    let big = "x".repeat(BIG);
    let provider = Arc::new(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response("call-1", "nika:read"))
            .enqueue_response(text_response("done")),
    );
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        Arc::new(InvokeVerb::new(Arc::new(
            MockToolExecutor::new().enqueue_ok(ToolResult::success("call-1", big.clone())),
        ))),
        Arc::new(MockToolDefinitionProvider::with_defs(vec![def(
            "nika:read",
        )])),
        "mock/agent",
    );
    let mut input = AgentInput::new("read the big file");
    input.tools = vec!["nika:read".to_owned()];
    verb.run(input).await.expect("completes");

    let fed = fed_results(&provider, 1);
    assert!(fed.contains(&big), "no store, no spill, no loss");
}

#[tokio::test]
async fn a_store_refusal_never_loses_the_result() {
    let big = "x".repeat(BIG);
    let provider = Arc::new(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response("call-1", "nika:read"))
            .enqueue_response(text_response("done")),
    );
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        Arc::new(InvokeVerb::new(Arc::new(
            MockToolExecutor::new().enqueue_ok(ToolResult::success("call-1", big.clone())),
        ))),
        Arc::new(MockToolDefinitionProvider::with_defs(vec![def(
            "nika:read",
        )])),
        "mock/agent",
    )
    .with_spill(Arc::new(FailingBlob));
    let mut input = AgentInput::new("read the big file");
    input.tools = vec!["nika:read".to_owned()];
    verb.run(input).await.expect("completes");

    let fed = fed_results(&provider, 1);
    assert!(
        fed.contains(&big),
        "a store refusal is not a reason to lose the output"
    );
}

#[tokio::test]
async fn the_preview_never_splits_a_char_boundary() {
    // « é » is two bytes: a naive byte cut of the preview lands mid-char.
    let big = "é".repeat(BIG);
    let (provider, blob, verb) = spill_rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response("call-1", "nika:read"))
            .enqueue_response(text_response("done")),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("call-1", big)),
    );
    let mut input = AgentInput::new("lire le gros fichier");
    input.tools = vec!["nika:read".to_owned()];
    verb.run(input).await.expect("completes");

    let fed = fed_results(&provider, 1);
    assert!(fed.contains("[… spilled ·"));
    assert!(blob.len() == 1);
}
