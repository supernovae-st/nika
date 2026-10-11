// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a model reads of earlier work, edited when a run starts (never mid-run, so the run's
//! cached prefix holds). The arguments and the reply of a superseded candidate call, and the reply
//! of a read made again later, become one-line stubs; the person's lines, the provenance replies
//! and the current candidate stay verbatim, and the stored tree is unchanged. Nothing before the
//! last message that replays a thinking block is edited: a provider replays such a block only
//! while everything before it is unchanged.

use std::collections::{BTreeMap, BTreeSet};

use nika_kernel::provider::{ContentBlock, Message, Role};
use serde_json::json;

/// The candidate's tools: a later call of any of them supersedes an earlier one.
const CANDIDATE: [&str; 3] = ["candidate_write", "candidate_edit", "compose"];
/// The tools that read: a later identical call supersedes an earlier one's reply.
const READS: [&str; 5] = ["observe", "language", "knowledge", "candidate_read", "read"];
/// What a superseded candidate call reads as, its arguments and its reply.
pub(crate) const SUPERSEDED: &str =
    "(Nika: a later revision replaced this candidate; the record keeps it whole.)";
/// What a read made again later reads as.
pub(crate) const READ_AGAIN: &str =
    "(Nika: read again later in this conversation; the later reply holds it.)";

/// The calls a run reads as stubs, fixed when the run starts.
#[derive(Debug, Default)]
pub(crate) struct Stubs {
    /// Superseded candidate calls: their arguments and their reply.
    superseded: BTreeSet<String>,
    /// Reads made again later: their reply.
    read_again: BTreeSet<String>,
}

impl Stubs {
    /// The stubs of the conversation `messages` holds when a run starts.
    pub(crate) fn of(messages: &[Message]) -> Self {
        let mut candidates = Vec::new();
        let mut reads: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let assistant = messages.iter().filter(|m| m.role == Role::Assistant);
        for block in assistant.flat_map(|m| &m.content) {
            let ContentBlock::ToolUse { id, name, input } = block else {
                continue;
            };
            if CANDIDATE.contains(&name.as_str()) {
                candidates.push(id.clone());
            } else if READS.contains(&name.as_str()) {
                let key = format!("{name} {input}");
                reads.entry(key).or_default().push(id.clone());
            }
        }
        // The current candidate, and the last of each read, stay whole.
        candidates.pop();
        let read_again = (reads.into_values())
            .flat_map(|mut ids| {
                ids.pop();
                ids
            })
            .collect();
        Self {
            superseded: candidates.into_iter().collect(),
            read_again,
        }
    }

    /// `messages` as the run reads them: each stub in place, after the last message that replays
    /// a thinking block.
    pub(crate) fn apply(&self, messages: &mut [Message]) {
        let thinking = |m: &Message| {
            (m.content.iter()).any(|block| matches!(block, ContentBlock::Thinking { .. }))
        };
        let from = messages.iter().rposition(thinking).map_or(0, |at| at + 1);
        for block in messages.iter_mut().skip(from).flat_map(|m| &mut m.content) {
            match block {
                ContentBlock::ToolUse { id, input, .. }
                    if self.superseded.contains(id.as_str()) =>
                {
                    *input = json!({"stub": SUPERSEDED});
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } => {
                    if self.superseded.contains(tool_use_id.as_str()) {
                        SUPERSEDED.clone_into(content);
                    } else if self.read_again.contains(tool_use_id.as_str()) {
                        READ_AGAIN.clone_into(content);
                    }
                }
                _ => {}
            }
        }
    }
}
