// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Earlier entries folded into a summary when the route's context window requires it. The
//! window is the route's real capacity, never a quota on the conversation: the recent entries
//! stay verbatim, the model summarizes the rest with the person's words, the decisions and the
//! values' provenance, and every entry stays in the tree.

use std::fmt::Write as _;

use nika_kernel::provider::{ContentBlock, Message, Role};

use crate::run::Request;
use crate::tree::{Entry, EntryId, EntryKind, Tree};

/// The instructions a summary is written under.
const SYSTEM: &str = "You write faithful summaries of a conversation between a person and the \
                      assistant that authors their Nika workflows with them.";

/// What a summary keeps.
const INSTRUCTION: &str = "Summarize the conversation above for a model that continues it \
without seeing it. Under these headings, keep what matters: Goal; Constraints and preferences; \
Decisions (each with the person's own words and its citation, such as u3); Values and their \
provenance (named, delegated, derived, offered, answered or retained, with the words that \
authorize each); Current candidate (its revision and what it does); Open questions; Next steps. \
Keep names, paths, addresses, model identifiers and numbers exactly. Write only the summary.";

/// The window a route offers a conversation, in tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Window {
    /// The route's context window.
    pub tokens: u64,
    /// The room kept for the reply and the next turn.
    pub reserve: u64,
    /// How much of the recent conversation stays verbatim after a compaction.
    pub keep: u64,
}

impl Window {
    /// A window of `tokens`, keeping `reserve` for the reply and `keep` verbatim.
    #[must_use]
    pub fn new(tokens: u64, reserve: u64, keep: u64) -> Self {
        Self {
            tokens,
            reserve,
            keep,
        }
    }

    /// A window of `tokens` whose reply reserve and verbatim tail are each a quarter of it, at
    /// most 16,384 and 20,000 tokens.
    #[must_use]
    pub fn of(tokens: u64) -> Self {
        Self::new(tokens, (tokens / 4).min(16_384), (tokens / 4).min(20_000))
    }

    /// Whether a conversation of `tokens` leaves the reply less than its reserve.
    #[must_use]
    pub fn exceeded_by(&self, tokens: u64) -> bool {
        tokens > self.tokens.saturating_sub(self.reserve)
    }
}

/// About how many tokens `entry` weighs when the model reads it: four characters a token.
fn weight(entry: &Entry) -> u64 {
    let chars = match &entry.kind {
        EntryKind::System { text } => text.len(),
        EntryKind::User { text, .. } => text.len() + 24,
        EntryKind::Assistant { content, .. } => content.iter().map(block_len).sum(),
        EntryKind::ToolResult { reply, .. } | EntryKind::Parked { reply, .. } => reply.text.len(),
        EntryKind::Compaction { summary, .. } => summary.len(),
        _ => 0,
    };
    u64::try_from(chars.div_ceil(4)).unwrap_or(u64::MAX)
}

fn block_len(block: &ContentBlock) -> usize {
    match block {
        ContentBlock::Text { text } | ContentBlock::Thinking { text } => text.len(),
        ContentBlock::ToolUse { name, input, .. } => name.len() + input.to_string().len(),
        ContentBlock::ToolResult { content, .. } => content.len(),
        _ => 0,
    }
}

/// The estimated size of the conversation the model reads now, in tokens: what the last
/// response since the last compaction reported for its request and answer, and about four
/// characters a token for what came after it (for all of it, when none reported).
#[must_use]
pub fn estimate(tree: &Tree) -> u64 {
    let branch = tree.branch();
    let since = branch
        .iter()
        .rposition(|e| matches!(e.kind, EntryKind::Compaction { .. }))
        .map_or(0, |k| k + 1);
    let reported = branch
        .iter()
        .enumerate()
        .skip(since)
        .rev()
        .find_map(|(k, e)| match &e.kind {
            EntryKind::Assistant { usage: Some(u), .. } => {
                Some((k, u.input_tokens.saturating_add(u.output_tokens)))
            }
            _ => None,
        });
    if let Some((k, tokens)) = reported {
        let after: u64 = branch.iter().skip(k + 1).map(|e| weight(e)).sum();
        tokens.saturating_add(after)
    } else {
        let (summary, kept) = Tree::kept(&branch);
        let summary = summary.map_or(0, |s| u64::try_from(s.len().div_ceil(4)).unwrap_or(0));
        let system = branch
            .iter()
            .rev()
            .find(|e| matches!(e.kind, EntryKind::System { .. }))
            .map_or(0, |e| weight(e));
        let kept: u64 = kept
            .iter()
            .filter(|e| !matches!(e.kind, EntryKind::System { .. }))
            .map(|e| weight(e))
            .sum();
        summary.saturating_add(system).saturating_add(kept)
    }
}

/// The first entry kept verbatim when the branch is compacted. A cut falls at a person's line
/// (never an answer to a waiting call) with conversation before it to fold: walking back from
/// the leaf, the first such line whose tail weighs at least `keep`, or else the oldest one;
/// none when no line has anything before it to fold.
#[must_use]
pub fn cut(tree: &Tree, keep: u64) -> Option<EntryId> {
    let branch = tree.branch();
    let (_, kept) = Tree::kept(&branch);
    let first = kept
        .iter()
        .position(|e| !matches!(e.kind, EntryKind::System { .. } | EntryKind::Fact { .. }))?;
    let mut tail = 0u64;
    let mut chosen = None;
    for (k, entry) in kept.iter().enumerate().rev() {
        tail = tail.saturating_add(weight(entry));
        if k > first && matches!(&entry.kind, EntryKind::User { answers: None, .. }) {
            chosen = Some(k);
            if tail >= keep {
                break;
            }
        }
    }
    chosen.map(|k| kept[k].id.clone())
}

/// The request a summary is written from: the earlier summary and the entries before
/// `first_kept` as a transcript, then what the summary must keep (and `focus`, when the person
/// named one). It offers no tool.
#[must_use]
pub fn request(tree: &Tree, first_kept: &EntryId, focus: Option<&str>) -> Request {
    let branch = tree.branch();
    let (summary, kept) = Tree::kept(&branch);
    let until = kept
        .iter()
        .position(|e| &e.id == first_kept)
        .unwrap_or(kept.len());
    let mut text = String::new();
    if let Some(summary) = summary {
        let _ = writeln!(text, "Earlier summary:\n{summary}\n");
    }
    for entry in &kept[..until] {
        transcribe(&mut text, entry);
    }
    let _ = write!(text, "\n---\n{INSTRUCTION}");
    if let Some(focus) = focus.map(str::trim).filter(|f| !f.is_empty()) {
        let _ = write!(text, "\nGive particular attention to: {focus}");
    }
    let messages = vec![Message::text(Role::User, text)];
    Request::new(Some(SYSTEM.to_owned()), messages, Vec::new())
}

/// One entry as a transcript line: who said or did what, citations and call identities kept.
fn transcribe(out: &mut String, entry: &Entry) {
    match &entry.kind {
        EntryKind::User {
            cite,
            text,
            answers,
            ..
        } => match answers {
            Some(call) => {
                let _ = writeln!(out, "[{cite}] The person, answering call {call}: {text}");
            }
            None => {
                let _ = writeln!(out, "[{cite}] The person: {text}");
            }
        },
        EntryKind::Assistant { content, .. } => {
            for block in content {
                match block {
                    ContentBlock::Text { text } if !text.trim().is_empty() => {
                        let _ = writeln!(out, "Assistant: {text}");
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        let _ = writeln!(out, "Assistant called {name} ({id}) with {input}");
                    }
                    _ => {}
                }
            }
        }
        EntryKind::ToolResult { call, name, reply } => {
            let failed = if reply.is_error { " a failure" } else { "" };
            let _ = writeln!(out, "{name} ({call}) replied{failed}: {}", reply.text);
        }
        EntryKind::Parked { call, name, reply } => {
            let _ = writeln!(out, "{name} ({call}) waits for the person: {}", reply.text);
        }
        EntryKind::Stopped { .. } => {
            let _ = writeln!(out, "The person stopped the run.");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
