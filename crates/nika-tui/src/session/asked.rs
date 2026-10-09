// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The question document and its identity come from one Session work snapshot.
//! Unsupported shapes keep the ordinary prompt; painting grants no answer or effect.

use nika_session::{QuestionId, work};
use std::sync::atomic::{AtomicU64, Ordering};

// One process-local scope per native runtime incarnation. This is a painted
// token namespace only; Session still owns and validates the strong QuestionId.
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

pub(super) fn epoch() -> u64 {
    NEXT_EPOCH.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn witness(id: &QuestionId, epoch: u64) -> String {
    format!("{epoch}:{}", id.as_str())
}

use crate::model::{Asked, Committed, Kind, Offer, Shape, Waiting};

pub(super) fn capture(snapshot: &work::Work, epoch: u64) -> Waiting {
    match &snapshot.waiting {
        work::Waiting::RunReview { .. } => plain("run_cost"),
        work::Waiting::CostChoice => plain("unknown_cost"),
        work::Waiting::IntelligenceChoice => Waiting::Choosing,
        work::Waiting::Consent { .. } => Waiting::Proposal,
        work::Waiting::Gate { .. } => Waiting::Gate,
        work::Waiting::Question { key, id } => {
            let asked = snapshot
                .question
                .as_ref()
                .filter(|q| q.key == *key)
                .and_then(|q| {
                    let shape = match q.answer_type {
                        "choice" if !q.options.is_empty() => Shape::Choice(
                            q.options
                                .iter()
                                .map(|offer| Offer {
                                    key: offer.key.clone(),
                                    label: offer.label.clone(),
                                })
                                .collect(),
                        ),
                        "text" => Shape::Text,
                        "literal" => Shape::Literal,
                        _ => return None,
                    };
                    Some(Asked {
                        label: q.label.clone(),
                        why: q.why.clone(),
                        mandatory: q.mandatory,
                        shape,
                        witness: witness(id, epoch),
                        epoch,
                    })
                });
            asked.map_or_else(|| plain(key), |asked| Waiting::asked(key.clone(), asked))
        }
        work::Waiting::Activation { key } => plain(key),
        work::Waiting::Input { .. } => plain(""),
        _ => Waiting::Free,
    }
}

fn plain(key: &str) -> Waiting {
    Waiting::Question {
        key: key.to_owned(),
    }
}

/// The words and their painted witness come from the same supported question snapshot.
/// Legacy prompts retain the complete words without lending a typed answer identity.
pub(super) fn question(snapshot: &work::Work, key: &str, text: String, epoch: u64) -> Committed {
    if !matches!(key, "run_cost" | "unknown_cost")
        && let Waiting::QuestionDocument {
            key: pending,
            asked,
        } = capture(snapshot, epoch)
        && pending == key
    {
        return Committed::question(asked.witness, text);
    }
    Committed::new(Kind::Question, text)
}
