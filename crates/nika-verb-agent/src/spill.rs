// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tool-result spill — oversized outputs leave the CONVERSATION, never
//! the record.
//!
//! The `ReAct` loop feeds every tool result back to the model verbatim
//! (ADR-096). That is the right default — the model repairs from what it
//! sees — until one result is a 2 MiB HTML page: the window fills with
//! bytes the model needed one line of, and every later turn re-pays them.
//!
//! The spill inverts the placement: past [`SPILL_THRESHOLD_BYTES`], the
//! full text goes to the blob store (content-addressed — the hash IS the
//! locator) and the conversation keeps a bounded preview plus the pointer.
//! Nothing is discarded: the bytes stay addressable for the run's record
//! and for the human — the loop just stops re-reading what it cannot use.
//!
//! ⚠️ A spill failure NEVER loses content: if the store refuses, the
//! original text rides the conversation exactly as it did before this
//! seam existed. The spill is an optimization of the model's reading,
//! not a gate on the data.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use nika_kernel::ai::provider::ContentBlock;
use nika_kernel::blob::{BlobError, BlobMetadata, BlobStoreDyn};

/// The kernel's `BlobStoreDyn` keeps `impl Future` returns — statically
/// dispatched, NOT object-safe. The loop needs the store OPTIONAL, and an
/// optional generic would infect every embedder signature (the rationale
/// the observer seam already lives by) — so the spill goes through this
/// object-safe bridge over the same kernel trait.
pub(crate) trait SpillStoreDyn: Send + Sync {
    /// Store bytes, returning the content-addressed metadata (the hash
    /// is the locator), boxed so the trait stays dyn-compatible.
    fn put(
        &self,
        data: bytes::Bytes,
        mime_type: &'static str,
    ) -> Pin<Box<dyn Future<Output = Result<BlobMetadata, BlobError>> + Send + '_>>;
}

impl<T: BlobStoreDyn + Sync> SpillStoreDyn for T {
    fn put(
        &self,
        data: bytes::Bytes,
        mime_type: &'static str,
    ) -> Pin<Box<dyn Future<Output = Result<BlobMetadata, BlobError>> + Send + '_>> {
        Box::pin(async move { self.put(data, mime_type).await })
    }
}

/// Above this many bytes a tool result spills. Sixteen KiB: ordinary
/// outputs (a file read, a fetch, a page of search hits) stay inline and
/// the loop is byte-unchanged for them; past it the per-turn re-read cost
/// dominates whatever the model could still learn from the middle of the
/// blob. Not configurable on purpose: it is a property of the loop, not
/// of a workflow (a workflow that needs its outputs whole narrows the
/// query, it does not tune the reader).
pub(crate) const SPILL_THRESHOLD_BYTES: usize = 16 * 1024;

/// How much of a spilled result stays in the conversation. Two KiB holds
/// the head of almost any payload — enough for the model to recognize
/// what it got and to decide whether a narrower re-query is worth it.
pub(crate) const SPILL_PREVIEW_BYTES: usize = 2 * 1024;

/// Rewrite oversized tool results in place: full text to the store,
/// preview + locator into the conversation. Results under the threshold,
/// and every result when the store fails, pass through byte-identical.
pub(crate) async fn spill_tool_results(
    content: &mut [ContentBlock],
    store: &Arc<dyn SpillStoreDyn>,
) {
    for block in content.iter_mut() {
        let ContentBlock::ToolResult { content, .. } = block else {
            continue;
        };
        if content.len() <= SPILL_THRESHOLD_BYTES {
            continue;
        }
        let Ok(meta) = store
            .put(
                bytes::Bytes::copy_from_slice(content.as_bytes()),
                "text/plain",
            )
            .await
        else {
            continue; // never a loss: the full text stays (see module doc)
        };
        let total = content.len();
        let mut end = SPILL_PREVIEW_BYTES.min(total);
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        *content = format!(
            "{}\n[… spilled · {total} bytes · the full output is kept as blob `{}` · the preview above is the first {end} bytes · narrow the query if you need a later part]",
            &content[..end],
            meta.hash,
        );
    }
}
