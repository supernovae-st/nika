// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authoring round kept across a close (C7 · F-SESSION-1): evidence, never authority. The
//! home History is its one durable copy (`Saved.round`, written and read by
//! [`nika_onboard::compile::round`]; the project's structured record keeps none). A reopened
//! session names it and asks nothing — no model, no workflow executed — and `/meaning`,
//! `/status` and `/why` read it without changing it. `/restore` continues it only when asked:
//! the money gate reads the request again (no admitted span, account, review or consent is
//! restored), an EDIT's base must still be the exact bytes it revised, and the recorded plan is
//! replayed by the deterministic compiler against the observation it recorded — no model, no
//! provider call, no workflow executed — so the question that waited is asked again through the one
//! presenter, under this session's identity (an identity minted by the closed session is
//! refused). The next answer is an ordinary answer round: the compiler judges the answers
//! against the project as it is then, and any provider call passes the current admission.

use nika_onboard::compile::round::{Capture, KeptEdit, RoundReading, RoundRecord, change_money};
use serde_json::Value;

use super::draft::refused;
use super::history::Operation;
use super::{RefusalClass, SessionRuntime, TurnOutcome};
use crate::authoring::{AuthoringRound, Reading, compile_deterministic};
use crate::change::{ProjectChange, ProjectChangeSet, Witness};

/// How the continuation act appears in the conversation record.
const ACT: &str = "(continue the kept round)";

/// The restore notice's pointer to `/restore`, after the kept round's own line.
pub(super) const ROUND_HINT: &str = "\n  → type /restore to continue it under the project as it is now · no AI asked · its question is asked again, no workflow is saved until you say yes";

/// The help line of `/restore` while a kept round can be continued.
pub(super) const ROUND_HELP: &str = "/restore            continue the round kept from your last session (its request, its answers, the question asked again) · no AI asked · no workflow is saved until you say yes";

/// A round kept when an earlier session closed, with the question labels its record projected
/// beside it (`Saved.unresolved`).
#[derive(Clone, Debug)]
pub(super) struct KeptRound {
    reading: RoundReading,
    labels: Vec<String>,
}

impl KeptRound {
    /// The kept value read by the round schema, and the labels its record projected.
    pub(super) fn new(raw: Value, labels: Vec<String>) -> Self {
        Self {
            reading: RoundReading::from_raw(raw),
            labels,
        }
    }
}

impl SessionRuntime {
    /// The round `Saved.round` keeps: the live one, else the kept one unchanged.
    pub(super) fn round_to_keep(&self) -> Option<Value> {
        let Some(round) = &self.authoring else {
            return self
                .restored_round
                .as_ref()
                .map(|k| k.reading.raw().clone());
        };
        let redact = |text: &str| crate::broker::redact(text).0;
        let revises = self
            .revising
            .as_ref()
            .map(|(set, _)| self.proposal_id(set).to_string());
        let mut capture = Capture::new(&round.intent, &redact)
            .answers(&round.answers)
            .questions(&round.questions)
            .reasons(&round.reasons)
            .restatements(round.restatements)
            .continuation(round.continuation.as_ref())
            .knowledge(round.knowledge.as_ref())
            .authoring_receipt(round.authoring_receipt.as_ref())
            .revises(revises.as_deref());
        if let Some((base, change, original)) = &round.edit {
            // A saved workflow's revision names its file; a proposal's revision names none (its
            // base is the proposal kept beside it).
            let path = self
                .last_workflow
                .as_ref()
                .filter(|_| revises.is_none())
                .map(|p| {
                    p.strip_prefix(&self.snapshot.root)
                        .unwrap_or(p)
                        .display()
                        .to_string()
                });
            capture = capture.edit(path.as_deref(), base, change, original.as_deref());
        }
        capture.finish()
    }

    /// Whether a kept round can be continued now.
    pub(super) fn round_is_continuable(&self) -> bool {
        self.restored_round
            .as_ref()
            .is_some_and(|k| k.reading.continuable().is_some())
    }

    /// The question labels a readable kept round owns: named by its own line, never announced
    /// as expired.
    pub(super) fn kept_round_labels(&self) -> &[String] {
        match &self.restored_round {
            Some(kept) if kept.reading.record().is_ok() => &kept.labels,
            _ => &[],
        }
    }

    /// New work replaces a round kept from an earlier session: a live round, or a proposal
    /// other than the one the kept round revises (its EDIT's exact base).
    pub(super) fn drop_replaced_round(&mut self) {
        let replaced = self.authoring.is_some()
            || self
                .pending
                .as_ref()
                .is_some_and(|set| !self.kept_round_revises(set));
        if replaced {
            self.restored_round = None;
        }
    }

    /// Whether `set` is the proposal the kept round revises: its workflow's bytes are the
    /// EDIT's exact base.
    fn kept_round_revises(&self, set: &ProjectChangeSet) -> bool {
        let content = set.changes.first().map(ProjectChange::content);
        self.restored_round
            .as_ref()
            .and_then(|k| k.reading.record().ok())
            .zip(content)
            .is_some_and(|(record, content)| record.revises_base(content))
    }

    /// The restore notice's line about a kept round, with the way on when it can be continued.
    pub(super) fn round_line(&self) -> Option<String> {
        Some(match self.restored_round.as_ref()?.reading.words() {
            Ok(words) => match words.blocked {
                None => format!("restored round: {}{ROUND_HINT}", words.summary),
                Some(why) => format!(
                    "restored round: {} · it cannot be continued ({why}); it stays kept as evidence · state the request again instead",
                    words.summary
                ),
            },
            Err(why) => format!(
                "a round kept by another engine version cannot be read here ({why}); it stays kept unchanged and grants nothing"
            ),
        })
    }

    /// `/meaning` beside a round kept but not continued yet (R4 73): what it holds, read-only.
    pub(super) fn kept_round_meaning(&self) -> Option<TurnOutcome> {
        let text = match self.restored_round.as_ref()?.reading.words() {
            Ok(words) => format!(
                "the round kept from your last session, not continued yet: {}\n  its clause-by-clause reading is shown once /restore continues it · nothing was asked or changed",
                words.summary
            ),
            Err(why) => format!(
                "a round is kept from your last session but this engine cannot read it ({why}) · it grants nothing"
            ),
        };
        Some(TurnOutcome::Facts(text))
    }

    /// `/why` while nothing waits but a round is kept: the question it waited on and why the
    /// compiler asked it, read-only.
    pub(super) fn kept_round_why(&self) -> Option<TurnOutcome> {
        let text = match self.restored_round.as_ref()?.reading.words() {
            Ok(words) => format!(
                "nothing waits now · the round kept from your last session has not continued: {}{}\n  {} · nothing was asked or changed",
                words.summary,
                words
                    .asked
                    .map_or_else(String::new, |why| format!("\n  why it was asked: {why}")),
                way(words.blocked, "/restore continues it (no AI asked)")
            ),
            Err(why) => format!(
                "nothing waits now · a round is kept from your last session but this engine cannot read it ({why}) · it grants nothing"
            ),
        };
        Some(TurnOutcome::Facts(text))
    }

    /// The `/status` line of a kept round: empty when none is kept.
    pub(super) fn kept_round_status(&self) -> String {
        match self.restored_round.as_ref().map(|k| k.reading.words()) {
            Some(Ok(words)) => format!(
                "\n  kept round (not continued): {} · {}",
                words.summary,
                way(words.blocked, "/restore continues it")
            ),
            Some(Err(_)) => "\n  kept round: unreadable by this engine · kept unchanged".to_owned(),
            None => String::new(),
        }
    }

    /// `/restore` for a kept round: recorded like a turn.
    pub fn restore_round(&mut self) -> TurnOutcome {
        self.recorded(Operation::Turn, ACT, Self::restore_round_unrecorded)
    }

    /// `/restore`: the kept round when one can be continued (with the proposal it revises, set
    /// aside again), else the kept draft's own re-proposal; a kept round that cannot be
    /// continued says why when no draft is kept either.
    pub(super) fn restore_kept(&mut self) -> TurnOutcome {
        let Some(record) = self
            .restored_round
            .as_ref()
            .and_then(|k| k.reading.continuable())
        else {
            return if self.restored_round.is_some() && self.restored_draft.is_none() {
                self.restore_round()
            } else {
                self.restore_draft()
            };
        };
        let Some(proposal) = record.revises.clone() else {
            return self.restore_round();
        };
        // The revision unit: the proposal it revises is proposed again by its own law, then set
        // aside, never consentable meanwhile; one that cannot be rebuilt refuses the round.
        if self.restored_draft_id() != Some(proposal.as_str()) {
            return refused(
                RefusalClass::NotAllowed,
                format!(
                    "the kept round revises proposal {proposal}, which can no longer be proposed · both stay kept as evidence; state the change again instead"
                ),
            );
        }
        let proposed = self.repropose_restored_draft();
        if !matches!(proposed, TurnOutcome::Proposal { .. }) || !self.round_is_continuable() {
            return proposed;
        }
        self.revising = self.pending.take().map(|set| (set, None));
        let outcome = self.restore_round();
        let outcome = self.keep_revising(outcome);
        match (outcome, proposed) {
            // The round could not go on: the proposal waits again, shown afresh with why.
            (TurnOutcome::Refusal(why), TurnOutcome::Proposal { id, preview })
                if self.pending.is_some() =>
            {
                TurnOutcome::Proposal {
                    id,
                    preview: format!(
                        "the kept round that revised it could not be continued: {}\n{preview}",
                        why.text
                    ),
                }
            }
            (outcome, _) => outcome,
        }
    }

    fn restore_round_unrecorded(&mut self) -> TurnOutcome {
        let kept = self.restored_round.as_ref().map(|k| k.reading.record());
        let record = match self.kept_to_use(kept, "round", "continue") {
            Ok(record) => record.clone(),
            Err(refusal) => return refusal,
        };
        if let Err(why) = record.continuable() {
            return refused(
                RefusalClass::NotAllowed,
                format!(
                    "the kept round cannot be continued: {why} · it stays kept as evidence; state the request again instead"
                ),
            );
        }
        // Zero calls: a base that moved is said, never re-grounded under the zero-call claim.
        if let Some(edit) = &record.edit
            && let Err(why) = self.base_holds(edit, &record)
        {
            return refused(RefusalClass::NotAllowed, why);
        }
        // The money gate reads the request again: nothing admitted earlier rides along. A
        // revision's words are its change, read as the live revision reads them (B15), never the
        // composed goal it was kept with: the original's words are another request's.
        let words = (record.edit.as_ref()).map_or(record.request.text.as_str(), |edit| {
            edit.change.text.as_str()
        });
        if let Err(refusal) = self.admit_money(words, false) {
            return refusal;
        }
        let round = rebuilt(&record, &self.money.admitted);
        // The recorded plan against its recorded observation: no seat, no fresh observation.
        match compile_deterministic(&round.request()) {
            Ok(out) => {
                self.restored_round = None;
                self.intent.goal = Some(round.effective_intent());
                let settled = self.settle(round, Reading::of(out));
                preface(
                    settled,
                    &format!(
                        "your request from the last session: « {} » · continued from the kept round: its recorded plan replayed by the deterministic compiler — no AI asked, no workflow executed",
                        record.request.text
                    ),
                )
            }
            Err(error) => self.machinery(&error),
        }
    }

    /// Whether a revision's base still holds the exact bytes it revised (a saved workflow's
    /// revision) — read through the change primitive's contained witness, never followed out.
    fn base_holds(&self, edit: &KeptEdit, record: &RoundRecord) -> Result<(), String> {
        let Some(path) = edit.path.as_ref().map(|p| p.text.as_str()) else {
            // A proposal's revision: that proposal is its base (the revision unit).
            return if record.revises.is_some() && self.revising.is_some() {
                Ok(())
            } else {
                Err("the proposal the kept round revised is not proposed again · the change is kept as evidence; state it again instead".to_owned())
            };
        };
        let change = &edit.change.text;
        let set = ProjectChangeSet::workflow_at(
            &self.snapshot.root,
            change,
            path,
            edit.base.text.clone(),
        )
        .map_err(|error| {
            format!(
                "the base `{path}` cannot be read now ({error}) · the change « {change} » is kept as evidence; nothing was compiled"
            )
        })?;
        match set.changes.first().and_then(ProjectChange::witness) {
            None => Err(format!(
                "the base `{path}` no longer exists · the change « {change} » is kept as evidence; nothing was compiled or proposed"
            )),
            Some(now) if *now != Witness::of(edit.base.text.as_bytes()) => Err(format!(
                "the base `{path}` changed since this revision was asked · the change « {change} » and its answers are kept as evidence; say the change again to revise the file as it is now · nothing was compiled"
            )),
            Some(_) => Ok(()),
        }
    }
}

/// The round a kept record continues: its request, answers, plan and evidence, with the money
/// this session's gate admitted now: a request's spans of its own bytes, or a revision's change
/// under the live revision's law (B15), its own directives when the gate admitted money in it.
fn rebuilt(record: &RoundRecord, admitted: &[std::ops::Range<usize>]) -> AuthoringRound {
    let mut round = AuthoringRound::new(record.request.text.clone());
    round.answers = record.answer_map();
    round.continuation = record.continuation.as_ref().and_then(|p| p.value.clone());
    round.reasons.clone_from(&record.reasons);
    round.restatements = record.restatements;
    round.knowledge.clone_from(&record.knowledge);
    round.authoring_receipt = record.authoring_receipt();
    round.money = match &record.edit {
        Some(edit) => change_money(&edit.change.text, !admitted.is_empty()),
        None => admitted.to_vec(),
    };
    round.edit = record.edit.as_ref().map(KeptEdit::texts);
    round
}

/// The outcome with a first line naming where it came from.
fn preface(outcome: TurnOutcome, line: &str) -> TurnOutcome {
    match outcome {
        TurnOutcome::Question { key, question } => TurnOutcome::Question {
            key,
            question: format!("{line}\n{question}"),
        },
        TurnOutcome::Proposal { id, preview } => TurnOutcome::Proposal {
            id,
            preview: format!("{line}\n{preview}"),
        },
        TurnOutcome::Facts(text) => TurnOutcome::Facts(format!("{line}\n{text}")),
        other => other,
    }
}

/// The way on from a kept round: `restore` when it can be continued, else why it cannot.
fn way(blocked: Option<String>, restore: &str) -> String {
    blocked.map_or_else(
        || restore.to_owned(),
        |why| format!("it cannot be continued ({why})"),
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod round_tests;
