// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Paths introduced by the native door's applied answers. Neither a candidate's complete
//! boundary nor a saved path inventory grants a read: the same bake must produce these bytes.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::{CompileRequest, CompileStatus, Input};

#[cfg(test)]
mod tests;

/// Exact local paths introduced by the questions this native application answered, by role.
/// Constructed only by the core's bake and capability difference; not deserializable authority.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AnsweredPaths {
    reads: Vec<String>,
    writes: Vec<String>,
}

impl AnsweredPaths {
    pub(super) fn empty() -> Self {
        Self {
            reads: Vec::new(),
            writes: Vec::new(),
        }
    }

    /// The applied answers used as file reads; an answer used only as content is absent.
    #[must_use]
    pub fn reads(&self) -> &[String] {
        &self.reads
    }

    /// The applied answers used as file writes. These are never implicit read inputs.
    #[must_use]
    pub fn writes(&self) -> &[String] {
        &self.writes
    }

    pub(super) fn introduced(before: &str, after: &str) -> Self {
        let (Ok(before), Ok(after)) = (crate::parse(before), crate::parse(after)) else {
            return Self::empty();
        };
        let (before, after) = (
            nika_check::infer_permits(&before),
            nika_check::infer_permits(&after),
        );
        let delta = |write| {
            let introduced: Vec<String> = inferred(&after, write)
                .difference(&inferred(&before, write))
                .cloned()
                .collect();
            if introduced
                .iter()
                .any(|path| path.is_empty() || path.contains(['*', '?', '[']))
            {
                Vec::new()
            } else {
                introduced
            }
        };
        Self {
            reads: delta(false),
            writes: delta(true),
        }
    }
}

/// Reconstruct a native CREATE's answered paths from its current record and answers.
/// The record must name this effective intent, and the ordinary native application must
/// produce the exact clean-checked candidate. Extra saved metadata is never read as a grant.
/// This proves only which answers supplied paths; it grants neither execution nor consent.
/// Lists of selected sources and EDIT inheritance are not projected by this native seam.
#[must_use]
pub fn native_answered_paths(
    record: &Value,
    request: &CompileRequest,
    candidate: &str,
) -> Option<AnsweredPaths> {
    let Input::Create(original) = &request.input else {
        return None;
    };
    let effective = match request.answers.get("intent.clarification") {
        Some(raw) => serde_json::from_str::<String>(raw).ok()?,
        None => original.clone(),
    };
    // A semantic record is read through the replay's own validated rebuild, never its stored
    // source; a legacy native record as it was.
    let view = if record.get("semantic_record").is_some() {
        let folded = crate::lexicon::fold_apostrophes(&effective);
        Some(super::rebuilt(&folded, record, request).ok()?.0)
    } else {
        None
    };
    if view.is_none()
        && (effective.trim().is_empty()
            || record["strategy"].as_str() != Some("native")
            || record["intent_sha256"].as_str() != Some(super::intent_sha256(&effective).as_str())
            || !record["source"].is_string()
            || !record["questions"].is_array())
    {
        return None;
    }
    let mut out = crate::initial();
    let paths = super::apply_native(view.as_ref().unwrap_or(record), request, &mut out);
    (out.status == CompileStatus::Ready
        && out.candidate.as_deref() == Some(candidate)
        && out
            .check_preview
            .as_ref()
            .is_some_and(|p| p.report.is_clean()))
    .then_some(paths)
}

fn inferred(value: &nika_check::InferredPermits, write: bool) -> BTreeSet<String> {
    value
        .permits
        .fs
        .as_ref()
        .map(|fs| if write { &fs.write } else { &fs.read })
        .into_iter()
        .flatten()
        .cloned()
        .collect()
}
