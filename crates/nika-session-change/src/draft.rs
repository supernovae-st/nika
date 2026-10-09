// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A proposal kept across a close: its record and the deterministic rebuild of its one
//! workflow file. The kept draft is evidence, never authority: restoring it restores no
//! consent, budget or run, and a rebuilt proposal needs a fresh review and a fresh consent.
//! No model is called. The Session's own re-proposal act is `nika-session`'s.
//!
//! Record (`Saved.pending`, JSON, schema 1): `{"schema": 1, "proposal", "goal", "files":
//! [{"path", "kind": "create_workflow" | "update_workflow" | "other", "before", "witness",
//! "text"}]}`. A value this engine cannot read (another schema, a malformed value, no schema)
//! is kept byte for byte, rides every later record unchanged and is never used.

use std::path::Path;

use nika_cli_host::context_redaction::redact;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::change::{ProjectChange, ProjectChangeSet, Witness};
use crate::outcome::ProposalId;

/// The draft schema this engine writes and reads.
pub const DRAFT_SCHEMA: u64 = 1;
/// A proposal kept across a close.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PendingDraft {
    pub schema: u64,
    /// The proposal identity the human was shown.
    pub proposal: String,
    /// The goal it answered (redacted).
    pub goal: String,
    pub files: Vec<DraftFile>,
}

/// One proposed file: its project-relative path, its change kind, the witness of the base it
/// was proposed over (an update), the witness of its exact proposed bytes, and its redacted
/// text. Old records that omitted text remain readable without inventing missing bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct DraftFile {
    pub path: String,
    pub kind: DraftKind,
    pub before: Option<String>,
    pub witness: String,
    pub text: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DraftKind {
    CreateWorkflow,
    UpdateWorkflow,
    Other,
}

/// A restored draft: one this engine reads, or one it keeps unread.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Restored {
    Usable { draft: PendingDraft, raw: Value },
    Unreadable { raw: Value, why: String },
}

impl Restored {
    pub fn from_raw(raw: Value) -> Self {
        let why = match raw.get("schema").and_then(Value::as_u64) {
            Some(DRAFT_SCHEMA) => match serde_json::from_value::<PendingDraft>(raw.clone()) {
                Ok(draft) => return Self::Usable { draft, raw },
                Err(error) => format!("a malformed draft record ({error})"),
            },
            Some(schema) => format!("draft schema {schema}; this engine reads {DRAFT_SCHEMA}"),
            None => "a draft record without a schema".to_owned(),
        };
        Self::Unreadable { raw, why }
    }

    /// The exact value the record carried: re-saved unchanged.
    #[must_use]
    pub fn raw(&self) -> &Value {
        match self {
            Self::Usable { raw, .. } | Self::Unreadable { raw, .. } => raw,
        }
    }

    /// The draft, or why this engine cannot read it.
    ///
    /// # Errors
    /// The reason this engine cannot read the kept value.
    pub fn read(&self) -> Result<&PendingDraft, &str> {
        match self {
            Self::Usable { draft, .. } => Ok(draft),
            Self::Unreadable { why, .. } => Err(why),
        }
    }
}

/// The pending proposal as a kept draft record.
#[must_use]
pub fn capture(id: &ProposalId, set: &ProjectChangeSet) -> Option<Value> {
    let files = set
        .changes
        .iter()
        .map(|change| {
            let content = change.content();
            let text = Some(redact(content).0);
            let kind = match change {
                ProjectChange::CreateWorkflow { .. } => DraftKind::CreateWorkflow,
                ProjectChange::UpdateWorkflow { .. } => DraftKind::UpdateWorkflow,
                _ => DraftKind::Other,
            };
            DraftFile {
                path: change.path().display().to_string(),
                kind,
                before: change.witness().map(|w| w.0.clone()),
                witness: Witness::of(content.as_bytes()).0,
                text,
            }
        })
        .collect();
    let draft = PendingDraft {
        schema: DRAFT_SCHEMA,
        proposal: id.to_string(),
        goal: redact(&set.goal).0,
        files,
    };
    serde_json::to_value(draft).ok()
}

/// The restore notice's line about a kept draft.
#[must_use]
pub fn restored_line(restored: &Restored, inference_blocked: bool) -> String {
    let draft = match restored.read() {
        Ok(draft) => draft,
        Err(why) => {
            return format!(
                "a proposal kept by another engine version cannot be read here ({why}); it stays kept unchanged and grants nothing"
            );
        }
    };
    let files: Vec<String> = draft
        .files
        .iter()
        .map(|f| {
            format!(
                "{} (witness {})",
                f.path,
                f.witness.get(..12).unwrap_or(&f.witness)
            )
        })
        .collect();
    // A draft that can never be rebuilt (redacted, truncated, several files, not a workflow)
    // is named as such: the notice never promises a re-proposal that must refuse.
    let (again, blocked) = match admissible(draft) {
        Ok(_) => (
            "it can be proposed again without a model call, after each file is checked against the project now, for your fresh review and consent".to_owned(),
            " · model authoring stays blocked while an earlier charge is unknown; proposing the kept draft again calls no model",
        ),
        Err(why) => (
            format!("it cannot be proposed again ({why}); state the request again instead"),
            " · model authoring stays blocked while an earlier charge is unknown",
        ),
    };
    let blocked = if inference_blocked { blocked } else { "" };
    format!(
        "restored proposal {} was pending at close and was not applied: {} · kept as evidence; no consent, budget or run carries over · {again}{blocked}",
        draft.proposal,
        files.join(", ")
    )
}

/// The one file of a kept draft and its exact bytes, or why the draft can never be rebuilt,
/// whatever the project holds now: exactly one workflow file whose kept text is its proposed
/// bytes (redaction, alteration or an old missing-text record breaks that), with its base witness when it was an
/// update. The project itself is checked by [`rebuild`].
fn admissible(draft: &PendingDraft) -> Result<(&DraftFile, &str), String> {
    let [file] = draft.files.as_slice() else {
        return Err(format!(
            "it holds {} files; only a single-workflow draft can be proposed again",
            draft.files.len()
        ));
    };
    let Some(text) = file.text.as_deref() else {
        return Err("its text was not kept by the engine that wrote this record".to_owned());
    };
    if Witness::of(text.as_bytes()).0 != file.witness {
        return Err(
            "its kept text differs from the proposed bytes (redacted or altered); rebuilding it would change your code"
                .to_owned(),
        );
    }
    match file.kind {
        DraftKind::Other => Err("it is not a workflow file".to_owned()),
        DraftKind::UpdateWorkflow if file.before.is_none() => {
            Err("its base witness is missing".to_owned())
        }
        DraftKind::CreateWorkflow | DraftKind::UpdateWorkflow => Ok((file, text)),
    }
}

/// The kept draft as a fresh change set, or why it cannot be one.
///
/// # Errors
/// Why the draft cannot be rebuilt now: not admissible, or the project changed under it.
pub fn rebuild(root: &Path, draft: &PendingDraft) -> Result<ProjectChangeSet, String> {
    let (file, text) = admissible(draft)?;
    let expected = match file.kind {
        DraftKind::UpdateWorkflow => file.before.as_deref(),
        DraftKind::CreateWorkflow | DraftKind::Other => None,
    };
    let set = ProjectChangeSet::workflow_at(root, &draft.goal, &file.path, text.to_owned())
        .map_err(|error| error.to_string())?;
    let now = set
        .changes
        .first()
        .and_then(ProjectChange::witness)
        .map(|w| w.0.as_str());
    if now != expected {
        return Err(match (expected, now) {
            (None, Some(_)) => format!("`{}` exists now; the draft created it", file.path),
            (Some(_), None) => format!("`{}` no longer exists; the draft updated it", file.path),
            _ => format!("`{}` changed since the draft was proposed", file.path),
        });
    }
    Ok(set)
}
