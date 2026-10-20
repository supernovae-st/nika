// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The original verifier records beside a ready scope. They grant no authority and are never
//! rebuilt from call counts. The host's activity envelope supplies Session and command identity.

use nika_onboard::compile::{CompileOutcome, CompileStatus};
use serde_json::{Value, json};

/// A successful desk judgment with the compiler evidence it actually received.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Judged {
    pub(crate) scope: String,
    candidate_sha256: String,
    records: Option<Vec<Value>>,
    reused: bool,
}

impl Judged {
    /// A scope without retained compiler evidence remains explicitly without that evidence.
    pub(crate) fn bare(scope: String, source: &str) -> Self {
        Self {
            scope,
            candidate_sha256: nika_compile::surface::sha256(source),
            records: None,
            reused: false,
        }
    }

    /// Keep the compiler's whole attempt array, unchanged, only from a ready outcome.
    pub(crate) fn ready(scope: String, source: &str, out: &CompileOutcome) -> Self {
        let mut judged = Self::bare(scope, source);
        if out.status == CompileStatus::Ready {
            judged.records = out
                .provenance
                .decision
                .as_ref()
                .and_then(|decision| decision.get("semantic_verification"))
                .and_then(Value::as_array)
                .filter(|records| !records.is_empty() && records.iter().all(Value::is_object))
                .cloned();
        }
        judged
    }

    /// Reuse the same records without changing their calls, answers or consumed counts.
    pub(crate) fn cached(mut self) -> Self {
        self.reused = true;
        self
    }

    /// Version the original records without interpreting them. The `ToolReply` builder checks
    /// secret shapes before this projection enters the public tool reply or its durable tree.
    pub(crate) fn projection(&self, revision: u64) -> Option<Value> {
        let records = self.records.as_ref()?;
        let value = json!({
            "schema": "nika/semantic-verification@1",
            "candidate_sha256": self.candidate_sha256,
            "candidate_revision": revision,
            "reused": self.reused,
            "records": records,
        });
        Some(value)
    }
}

/// The evidence retained with the exact proposal shown, separately from admission authority.
#[derive(Clone)]
pub(crate) struct Shown {
    revision: u64,
    judged: Judged,
}

impl Shown {
    pub(crate) fn new(revision: u64, judged: Judged) -> Self {
        Self { revision, judged }
    }

    /// The existing shown-verdict route may copy only this revision's own record.
    pub(crate) fn repeated(&self, revision: u64, source: &str, scope: &str) -> Option<Judged> {
        (self.revision == revision
            && self.judged.candidate_sha256 == nika_compile::surface::sha256(source)
            && self.judged.scope == scope)
            .then(|| self.judged.clone().cached())
    }
}

#[cfg(test)]
mod tests;
