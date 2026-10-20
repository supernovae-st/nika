// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Which question a kept output answered, read back after a reopen. What a Session keeps of a
//! conversation holds each answer's value and where it comes from, never the question it answered,
//! so a reopened conversation could no longer tell an open name's output from another output. The
//! tree, the conversation's own record, still holds that question as the call that showed or asked
//! it: an output is recalled with its question's offers only when every link of that record holds
//! exactly once (`asked_in`). The offers recalled only tell which outputs stand apart; they ask,
//! answer, show, consent to or authorize nothing, and a missing or conflicting link recalls nothing.

use std::collections::BTreeMap;

use nika_compile_fidelity::fidelity::resolution::{same_literal, typed};
use nika_kernel::provider::ContentBlock;
use nika_session_change::outcome::QuestionId;
use nika_session_change::work::{Binding, Offer, ProvenanceKind, ValueRole};
use serde_json::{Value, json};

use super::{Citations, Conversation, question_options};
use crate::tree::{Entry, EntryKind, Tree};

/// What Nika says to the author with a line the host records as a natural rerun: a new explicit
/// request to run a workflow, which ends a parked question without answering it.
pub const RERUN_NOTE: &str = "The person explicitly requested the named or current existing \
    workflow. Nika is validating that run; no new candidate was requested.";

impl Conversation {
    /// Recall from `tree`, this conversation's own record, the question each kept output of the
    /// current request answered, with the offers the call that showed or asked it recorded, so an
    /// open name's output and another output stand apart again after a reopen. `citations` places
    /// the person's lines in the request. Two answers to one question that disagree recall nothing
    /// for it, and an output with no exact record keeps the single-output rule. Nothing is asked,
    /// answered, shown, consented to or authorized, and nothing recalled is kept.
    pub fn recall(&mut self, tree: &Tree, citations: &Citations) {
        let branch = tree.branch();
        let mut recalled: BTreeMap<String, Option<Vec<Offer>>> = BTreeMap::new();
        let outputs = (self.bindings.iter().chain(&self.accepted))
            .filter(|binding| binding.role == ValueRole::OutputPath);
        for binding in outputs {
            let Some(key) = binding.key.as_deref() else {
                continue;
            };
            let line = citations.get(&binding.provenance.message);
            let options = (line.is_some_and(|line| line.at >= self.since))
                .then(|| asked_in(&branch, binding, key))
                .flatten();
            let known = (recalled.entry(key.to_owned())).or_insert_with(|| options.clone());
            if *known != options {
                *known = None;
            }
        }
        self.recalled = (recalled.into_iter())
            .filter_map(|(key, options)| Some((key, options?)))
            .collect();
        self.replied = replied(&branch, citations, self.since);
    }

    /// The person's line `cite` replied to a question Nika asked: the call it answers is a parked
    /// `ask`. Evidence of where the line stands in this request ([`Self::replies_in`]); it answers,
    /// binds and authorizes nothing, and nothing is kept.
    pub fn replied_by(&mut self, cite: &str) {
        if !self.replied.iter().any(|known| known == cite) {
            self.replied.push(cite.to_owned());
        }
    }

    /// Every line of this request that replied to a question Nika asked ([`Self::replied_by`]),
    /// located in the person's words by its citation (`human_lines`, its first line and the one
    /// after its last) with the line as typed (`human_reply`), keyed by that citation: whether or
    /// not a value was bound from it, and never a line that answered a proposal or asked to run.
    /// Evidence that keeps a reply from naming the workflow's file; it binds, settles and
    /// authorizes nothing.
    #[must_use]
    pub fn replies_in(&self, citations: &Citations) -> BTreeMap<String, String> {
        (self.replied.iter())
            .filter_map(|cite| {
                let (start, end) = citations.located(self.since, cite)?;
                let line = citations.get(cite)?;
                let context = json!({"human_citation": cite, "human_lines": [start, end],
                    "human_reply": line.text});
                Some((cite.clone(), json!({"choice_context": context}).to_string()))
            })
            .collect()
    }
}

/// The person's lines of the request since `since` that replied to a question Nika asked, by
/// citation: each answered one call that ended its turn as a successful `ask` naming the questions
/// it asked, held once each by that call's own input. A line the host recorded as a natural rerun
/// ([`RERUN_NOTE`]) is a new request, never a reply.
fn replied(branch: &[&Entry], citations: &Citations, since: u64) -> Vec<String> {
    (branch.iter())
        .filter_map(|entry| match &entry.kind {
            EntryKind::User {
                cite,
                answers: Some(call),
                nika,
                ..
            } if nika.as_deref() != Some(RERUN_NOTE)
                && citations.get(cite).is_some_and(|line| line.at >= since)
                && asked_by(branch, call) =>
            {
                Some(cite.clone())
            }
            _ => None,
        })
        .collect()
}

/// Whether `call` is an `ask` that ended its turn naming the questions it asked, each held once by
/// its own input on `branch`.
fn asked_by(branch: &[&Entry], call: &str) -> bool {
    let parked = one(branch.iter().filter_map(|entry| match &entry.kind {
        EntryKind::Parked {
            call: parked,
            name,
            reply,
        } if parked == call && name == "ask" && !reply.is_error => Some(reply),
        _ => None,
    }));
    let Some(said) = parked.and_then(|reply| serde_json::from_str::<Value>(&reply.text).ok())
    else {
        return false;
    };
    let blocks = branch.iter().flat_map(|entry| match &entry.kind {
        EntryKind::Assistant { content, .. } => content.as_slice(),
        _ => &[][..],
    });
    let input = one(blocks.filter_map(|block| match block {
        ContentBlock::ToolUse { id, name, input } if id == call && name == "ask" => Some(input),
        _ => None,
    }));
    let (Some(asked), Some(questions)) = (
        said["asked"].as_array().filter(|asked| !asked.is_empty()),
        input.and_then(|input| input["questions"].as_array()),
    ) else {
        return false;
    };
    (asked.iter()).all(|named| {
        (questions.iter())
            .filter(|q| q["key"] == named["key"])
            .count()
            == 1
    })
}

/// The call that asked the question `id` on `tree`'s branch: the one successful `ask` whose reply
/// names `id` among the questions it asked, each held once by its own input. A reply to another
/// question of that call's group, given after the call stopped waiting, still replies to it.
/// `None` when no call or more than one asked it. It asks, answers, binds or authorizes nothing.
#[must_use]
pub fn asking_call(tree: &Tree, id: &QuestionId) -> Option<String> {
    let branch = tree.branch();
    let call = one(branch.iter().filter_map(|entry| match &entry.kind {
        EntryKind::Parked { call, name, reply } if name == "ask" && !reply.is_error => {
            let said = serde_json::from_str::<Value>(&reply.text).ok()?;
            (said["asked"].as_array()?.iter())
                .any(|asked| asked["id"].as_str() == Some(id.as_str()))
                .then_some(call.as_str())
        }
        _ => None,
    }))?;
    asked_by(&branch, call).then(|| call.to_owned())
}

/// The offers of the question `key` as `branch` records it for the output `binding`: the person's
/// line the output rests on answered one call; that call ended its turn with a reply naming the
/// question as shown (`propose`'s `open`) or asked (`ask`'s `asked`); the call's own input holds
/// the question once; and the offer picked there holds the value, or, with no offer picked, the
/// line holds the value as typed. None when any link is missing or not unique.
fn asked_in(branch: &[&Entry], binding: &Binding, key: &str) -> Option<Vec<Offer>> {
    let provenance = &binding.provenance;
    if (provenance.question.as_deref()).is_some_and(|question| question != key) {
        return None;
    }
    let (text, call) = one(branch.iter().filter_map(|entry| match &entry.kind {
        EntryKind::User {
            cite,
            text,
            answers,
            ..
        } if *cite == provenance.message => Some((text.as_str(), answers.as_deref())),
        _ => None,
    }))?;
    let call = call?;
    let (name, reply) = one(branch.iter().filter_map(|entry| match &entry.kind {
        EntryKind::Parked {
            call: parked,
            name,
            reply,
        } if parked == call && !reply.is_error => Some((name.as_str(), reply)),
        _ => None,
    }))?;
    let (shown, written) = match name {
        "propose" => ("open", "open"),
        "ask" => ("asked", "questions"),
        _ => return None,
    };
    let said: Value = serde_json::from_str(&reply.text).ok()?;
    one((said[shown].as_array()?.iter()).filter(|question| question["key"] == key))?;
    let blocks = branch.iter().flat_map(|entry| match &entry.kind {
        EntryKind::Assistant { content, .. } => content.as_slice(),
        _ => &[][..],
    });
    let input = one(blocks.filter_map(|block| match block {
        ContentBlock::ToolUse {
            id,
            name: tool,
            input,
        } if id == call && tool == name => Some(input),
        _ => None,
    }))?;
    let questions = input[written].as_array()?.iter();
    let question = one(questions.filter(|question| question["key"] == key))?;
    let options = question_options(question, &mut |_: &str| None);
    let holds = match provenance.option.as_deref() {
        Some(option) => (options.iter()).any(|offer| {
            offer.key == option
                && (offer.values.iter()).any(|value| {
                    value.role == binding.role && same_literal(&value.value, &binding.value)
                })
        }),
        None => provenance.kind == ProvenanceKind::Answered && typed(text, &binding.value),
    };
    holds.then_some(options)
}

/// The one item `items` yields, when it yields exactly one.
fn one<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
