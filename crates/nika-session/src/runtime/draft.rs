// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A proposal kept across a close, and its deterministic re-proposal. The kept draft is
//! evidence, never authority: restoring it restores no consent, budget or run. Proposing it
//! again rebuilds a fresh proposal from its exact bytes through the same change primitive a
//! compiled candidate uses, checked against the project as it is now, and that proposal
//! needs a fresh review and a fresh consent. No model is called.
//!
//! Record (`Saved.pending`, JSON, schema 1): `{"schema": 1, "proposal", "goal", "files":
//! [{"path", "kind": "create_workflow" | "update_workflow" | "other", "before", "witness",
//! "text"}]}`. A value this engine cannot read (another schema, a malformed value, no schema)
//! is kept byte for byte, rides every later record unchanged and is never used.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

use super::history::Operation;
use super::{Refusal, RefusalClass, SessionRuntime, TurnOutcome};
use crate::change::{ProjectChange, ProjectChangeSet, Witness};
use crate::outcome::ProposalId;

/// The draft schema this engine writes and reads.
pub(super) const DRAFT_SCHEMA: u64 = 1;
/// Kept text stays far below the history's per-record bound; a larger proposal keeps its
/// identity and witnesses only and cannot be proposed again.
const TEXT_LIMIT: usize = 256 * 1024;
/// How the re-proposal act appears in the conversation record.
const ACT: &str = "(propose the kept draft again)";

/// A proposal kept across a close.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingDraft {
    pub schema: u64,
    /// The proposal identity the human was shown.
    pub proposal: String,
    /// The goal it answered (redacted).
    pub goal: String,
    pub files: Vec<DraftFile>,
}

/// One proposed file: its project-relative path, its change kind, the witness of the base it
/// was proposed over (an update), the witness of its exact proposed bytes, and its redacted
/// text when it fits the kept-text bound.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftFile {
    pub path: String,
    pub kind: DraftKind,
    pub before: Option<String>,
    pub witness: String,
    pub text: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DraftKind {
    CreateWorkflow,
    UpdateWorkflow,
    Other,
}

/// A restored draft: one this engine reads, or one it keeps unread.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Restored {
    Usable { draft: PendingDraft, raw: Value },
    Unreadable { raw: Value, why: String },
}

impl Restored {
    pub(super) fn from_raw(raw: Value) -> Self {
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
    pub(super) fn raw(&self) -> &Value {
        match self {
            Self::Usable { raw, .. } | Self::Unreadable { raw, .. } => raw,
        }
    }
}

/// The pending proposal as a kept draft record.
pub(super) fn capture(id: &ProposalId, set: &ProjectChangeSet) -> Option<Value> {
    let mut room = TEXT_LIMIT;
    let files = set
        .changes
        .iter()
        .map(|change| {
            let content = change.content();
            let text = (content.len() <= room).then(|| {
                room = room.saturating_sub(content.len());
                crate::broker::redact(content).0
            });
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
        goal: crate::broker::redact(&set.goal).0,
        files,
    };
    serde_json::to_value(draft).ok()
}

/// The restore notice's line about a kept draft.
pub(super) fn restored_line(restored: &Restored, inference_blocked: bool) -> String {
    let Restored::Usable { draft, .. } = restored else {
        let why = match restored {
            Restored::Unreadable { why, .. } => why.as_str(),
            Restored::Usable { .. } => "",
        };
        return format!(
            "a proposal kept by another engine version cannot be read here ({why}); it stays kept unchanged and grants nothing"
        );
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

impl SessionRuntime {
    /// The identity of the kept draft when it is available now: this engine reads it, its
    /// kept text is its proposed bytes, and the project still holds the base it was proposed
    /// over. Re-checked read-only on every call with the checks
    /// [`Self::repropose_restored_draft`] makes (nothing is written); `None` otherwise, while
    /// the draft itself stays kept and the restore notice says why. Available means the same
    /// bytes over the same base, not proof that they still mean what the request meant.
    #[must_use]
    pub fn restored_draft_id(&self) -> Option<&str> {
        match &self.restored_draft {
            Some(Restored::Usable { draft, .. }) if rebuild(&self.snapshot.root, draft).is_ok() => {
                Some(draft.proposal.as_str())
            }
            _ => None,
        }
    }

    /// Propose the kept draft again, deterministically and without a model call. Its exact
    /// bytes are rebuilt through the change primitive a compiled candidate uses (a contained,
    /// canonical workflow path; the `nika check` audit; the destination witnessed now), and
    /// refused when the kept text is not the proposed bytes (redacted, or over the kept-text
    /// bound) or the project changed under it. The result is a fresh proposal: it needs a
    /// fresh review and a fresh consent, and nothing of the earlier consent, budget or run
    /// is restored. Recorded like a turn.
    pub fn repropose_restored_draft(&mut self) -> TurnOutcome {
        self.recorded(Operation::Turn, ACT, Self::repropose_unrecorded)
    }

    fn repropose_unrecorded(&mut self) -> TurnOutcome {
        if self.pending.is_some()
            || self.pending_gate.is_some()
            || self.authoring.is_some()
            || self.waiting_cost_choice()
        {
            return refused(
                RefusalClass::WrongState,
                "something already waits for you; answer or discard it first · the kept draft stays kept"
                    .to_owned(),
            );
        }
        let draft = match &self.restored_draft {
            Some(Restored::Usable { draft, .. }) => draft.clone(),
            Some(Restored::Unreadable { why, .. }) => {
                return refused(
                    RefusalClass::NotAllowed,
                    format!(
                        "the kept draft cannot be used by this engine ({why}); it stays kept unchanged"
                    ),
                );
            }
            None => {
                return refused(
                    RefusalClass::WrongState,
                    "no kept draft to propose again".to_owned(),
                );
            }
        };
        match rebuild(&self.snapshot.root, &draft) {
            Ok(set) => {
                let preview = self.draft_preview(&set);
                let id = ProposalId::of(&preview);
                self.remember(
                    ACT,
                    &format!("(proposed {id} again from kept draft {})", draft.proposal),
                );
                self.bind_proposal_money(&id);
                self.pending = Some(set);
                TurnOutcome::Proposal { id, preview }
            }
            Err(why) => refused(
                RefusalClass::NotAllowed,
                format!(
                    "the kept draft {} cannot be proposed again: {why} · it stays kept; state the request again instead",
                    draft.proposal
                ),
            ),
        }
    }
}

/// The one file of a kept draft and its exact bytes, or why the draft can never be rebuilt,
/// whatever the project holds now: exactly one workflow file whose kept text is its proposed
/// bytes (redaction or the kept-text bound breaks that), with its base witness when it was an
/// update. The project itself is checked by [`rebuild`].
fn admissible(draft: &PendingDraft) -> Result<(&DraftFile, &str), String> {
    let [file] = draft.files.as_slice() else {
        return Err(format!(
            "it holds {} files; only a single-workflow draft can be proposed again",
            draft.files.len()
        ));
    };
    let Some(text) = file.text.as_deref() else {
        return Err("its text was not kept (over the kept-text bound)".to_owned());
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
fn rebuild(root: &Path, draft: &PendingDraft) -> Result<ProjectChangeSet, String> {
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

fn refused(class: RefusalClass, text: String) -> TurnOutcome {
    TurnOutcome::Refusal(Refusal::new(class, text))
}
