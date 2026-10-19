// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the person may say of a result, on the wire (NIK-174): the optional offer waiting at the
//! free prompt after the conversation's candidate ran, and what was recorded of it. An offer takes
//! no line: only an answer naming its question's identity reaches it, while nothing else waits,
//! and a line typed at the prompt closes it unanswered. A record is evidence of the person's view
//! of one scoped result, never an authority: Save, Run, sending and activation stay the person's
//! own acts.

use serde::{Deserialize, Serialize};

use super::AskedQuestion;

/// What an offer asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FeedbackKind {
    /// What to adjust in the result: dimensions of it, « Nothing to change » or « Later ».
    Adjust,
    /// The neutral opinion of a corrected result: suitable for this use, still needs changes,
    /// wrong result, not evaluated yet, or « Later ». None is recommended.
    Opinion,
}

impl FeedbackKind {
    /// The offer's question key: `adjust` or `opinion`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Adjust => "adjust",
            Self::Opinion => "opinion",
        }
    }
}

/// One artifact a run completed writing, as the project held it when the offer was made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FeedbackArtifact {
    /// Its path, relative to the project root, as the run named it.
    pub path: String,
    /// The witness of the bytes the project held there when the offer was made, read whole again
    /// under the project root through no link ([`crate::change::Witness`]); a reread then, not the
    /// bytes the run wrote.
    pub witness: String,
}

impl FeedbackArtifact {
    /// An artifact (INV-019).
    #[must_use]
    pub fn new(path: impl Into<String>, witness: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            witness: witness.into(),
        }
    }
}

/// The result an offer and its answer are about: the request and revision of the candidate that
/// ran, the hash of the bytes the run started, that run's one execution, and the artifacts it
/// completed writing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FeedbackScope {
    /// The candidate's request in the conversation: the conversation line that request starts on.
    pub request: u64,
    /// The candidate's revision in that request.
    pub revision: u64,
    /// The SHA-256 of the workflow bytes the run started, hex.
    pub source_sha256: String,
    /// The run's one execution.
    pub execution: String,
    /// The artifacts the run completed writing, each with its witness.
    pub artifacts: Vec<FeedbackArtifact>,
}

impl FeedbackScope {
    /// A scope (INV-019).
    #[must_use]
    pub fn new(
        (request, revision): (u64, u64),
        source_sha256: impl Into<String>,
        execution: impl Into<String>,
        artifacts: Vec<FeedbackArtifact>,
    ) -> Self {
        Self {
            request,
            revision,
            source_sha256: source_sha256.into(),
            execution: execution.into(),
            artifacts,
        }
    }
}

/// The offer waiting at the free prompt: its question in `ask`'s shape (an adjustment's options
/// are checked together; « Nothing to change » and « Later » stand alone), the result it is
/// about, and, for an opinion, the adjustment it follows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct FeedbackOffer {
    /// What it asks.
    pub kind: FeedbackKind,
    /// The question, with the identity an answer names.
    pub question: AskedQuestion,
    /// The result it is about.
    pub scope: FeedbackScope,
    /// For an opinion: the identity of the adjustment answer this result amends, a later
    /// revision of that answer's own request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follows: Option<String>,
}

impl FeedbackOffer {
    /// An offer (INV-019).
    #[must_use]
    pub fn new(
        kind: FeedbackKind,
        question: AskedQuestion,
        scope: FeedbackScope,
        follows: Option<String>,
    ) -> Self {
        Self {
            kind,
            question,
            scope,
            follows,
        }
    }
}

/// One answer the person gave to an offer: its question's identity, the keys picked in the
/// offer's order, the result it is about and, for an opinion, the adjustment it follows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FeedbackRecord {
    /// What the offer asked.
    pub kind: FeedbackKind,
    /// The identity of the offer's question, its witness.
    pub question: String,
    /// The keys picked, in the offer's order.
    pub picked: Vec<String>,
    /// The result it is about.
    pub scope: FeedbackScope,
    /// For an opinion: the identity of the adjustment answer that result amends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follows: Option<String>,
}

impl FeedbackRecord {
    /// The record of `picked`, the answer to `offer` (INV-019).
    #[must_use]
    pub fn new(offer: &FeedbackOffer, picked: Vec<String>) -> Self {
        Self {
            kind: offer.kind,
            question: offer.question.id.as_str().to_owned(),
            picked,
            scope: offer.scope.clone(),
            follows: offer.follows.clone(),
        }
    }
}

/// The offer waiting at the free prompt and the answers recorded, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Feedback {
    /// The offer waiting, if one is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offer: Option<FeedbackOffer>,
    /// The answers recorded in this conversation.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recorded: Vec<FeedbackRecord>,
}

impl Feedback {
    /// The offer waiting and the answers recorded (INV-019).
    #[must_use]
    pub fn new(offer: Option<FeedbackOffer>, recorded: Vec<FeedbackRecord>) -> Self {
        Self { offer, recorded }
    }

    /// Whether there is nothing to carry: no offer and no record.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.offer.is_none() && self.recorded.is_empty()
    }
}
