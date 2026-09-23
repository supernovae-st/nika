// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Monetary admission and revision custody. The saved binding is to the
//! proposal AND the exact workflow bytes; it never grants execution.

use std::path::PathBuf;

use super::money_parse::{self, ParsedMoney};
use super::{DEFAULT_CEILING_USD, SessionRuntime, TurnOutcome};
use crate::change::{ProjectChangeSet, Witness};
use crate::consent::ConsentRecord;
use crate::money::{CapKnowledge, InferenceEnforcement, MonetaryDecision, MonetarySource};
use crate::outcome::{ProposalId, Refusal, RefusalClass};

#[derive(Default)]
pub(super) struct MoneyState {
    pub current: Option<MonetaryDecision>,
    pub draft: Option<MonetaryDecision>,
    pub pending: Option<MonetaryDecision>,
    saved: Vec<SavedMoney>,
}

struct SavedMoney {
    path: PathBuf,
    resolved: Option<PathBuf>,
    witness: Witness,
    decision: MonetaryDecision,
}

impl SessionRuntime {
    pub(super) fn proposal_preview(&self, set: &ProjectChangeSet) -> String {
        preview_with_money(set, self.money.pending.as_ref())
    }

    pub(super) fn proposal_id(&self, set: &ProjectChangeSet) -> ProposalId {
        ProposalId::of(&self.proposal_preview(set))
    }

    pub(super) fn draft_preview(&self, set: &ProjectChangeSet) -> String {
        preview_with_money(set, self.money.draft.as_ref())
    }

    pub(super) fn consent_money_route(
        &mut self,
        set: ProjectChangeSet,
        id: &ProposalId,
        answer: &str,
    ) -> TurnOutcome {
        if let Err(refusal) = self.admit_money(answer, true) {
            return refusal;
        }
        // A closed monetary-only amendment changes Session's own ceiling,
        // never workflow bytes. It has its own preview and fresh consent id.
        if money_parse::parse(answer).is_ok_and(|p| p.money_only) {
            let preview = self.draft_preview(&set);
            let id = ProposalId::of(&preview);
            self.bind_proposal_money(&id);
            self.pending = Some(set);
            return TurnOutcome::Proposal { id, preview };
        }
        let changed = self
            .money
            .draft
            .as_ref()
            .map(|d| (&d.input, d.effective_usd, d.inference))
            != self
                .money
                .pending
                .as_ref()
                .map(|d| (&d.input, d.effective_usd, d.inference));
        let outcome = self.route_at_consent(set, id.clone(), answer);
        if changed && self.pending_proposal().as_ref() == Some(id) {
            return self.refuse_money(answer, "the monetary amendment did not produce a new reviewed proposal — the old consent expired; prepare the revised request");
        }
        outcome
    }

    /// Actual monetary admission for the latest work/Run request or amendment.
    /// A refused request replaces the observation; it cannot expose stale
    /// accepted money. Unknown caps and billing remain explicitly unknown.
    #[must_use]
    pub fn monetary_decision(&self) -> Option<&MonetaryDecision> {
        self.money.current.as_ref()
    }

    pub(super) fn money_line(&self) -> String {
        self.money.current.as_ref().map_or_else(
            || "money: no request admitted · independent caps and billed cost unknown".to_owned(),
            MonetaryDecision::line,
        )
    }

    fn money_decision(&self, input: &str, parsed: &ParsedMoney) -> MonetaryDecision {
        let project = self.snapshot.ceiling;
        let explicit = parsed.amount;
        let source = match (explicit, project) {
            (Some(_), Some(_)) => MonetarySource::Override,
            (Some(_), None) => MonetarySource::Explicit,
            (None, Some(_)) => MonetarySource::ProjectDefault,
            (None, None) => MonetarySource::SessionDefault,
        };
        let amount = explicit.or(project).unwrap_or(DEFAULT_CEILING_USD);
        MonetaryDecision {
            original_intent: input.to_owned(),
            input: input.to_owned(),
            effective_usd: Some(amount),
            source,
            explicit_amount: parsed.literal.clone(),
            project_default_usd: project,
            project_file: self.snapshot.project_file.clone(),
            policy_cap: CapKnowledge::Unknown {
                reason: "Session has no independent policy-cap observation; project ceiling is a default".to_owned(),
            },
            machine_cap: CapKnowledge::Unknown {
                reason: "Session has no independent machine-cap observation".to_owned(),
            },
            inference: if explicit.is_some() || amount == 0.0 {
                InferenceEnforcement::CallsBlocked
            } else {
                InferenceEnforcement::NotMetered
            },
            observed_cost_usd: None,
            proposal: None,
            refusal: None,
        }
    }

    pub(super) fn refuse_money(&mut self, input: &str, reason: &str) -> TurnOutcome {
        let mut decision = self.money_decision(input, &ParsedMoney::default());
        if let Some(previous) = self
            .money
            .pending
            .as_ref()
            .or_else(|| self.authoring.as_ref().and(self.money.draft.as_ref()))
        {
            decision
                .original_intent
                .clone_from(&previous.original_intent);
        }
        decision.effective_usd = None;
        decision.source = MonetarySource::Rejected;
        decision.refusal = Some(reason.to_owned());
        decision.inference = InferenceEnforcement::CallsBlocked;
        // Rejection expires authority, not the cognition restriction. A
        // pending gate's next non-monetary question is still a continuation;
        // only fresh admission may replace this rejected decision.
        self.money.draft = Some(decision.clone());
        self.money.current = Some(decision);
        self.money.pending = None;
        self.pending = None;
        self.authoring = None;
        self.run_inputs = None;
        self.interrupted = None;
        self.intent.unresolved.clear();
        self.last_outcome = None;
        TurnOutcome::Refusal(Refusal::new(RefusalClass::NotAllowed, reason))
    }

    /// Called before any compiler/classifier/reasoner. A continuation with
    /// no monetary clause keeps the admitted round's money, not a new default.
    pub(super) fn admit_money(
        &mut self,
        input: &str,
        continuation: bool,
    ) -> Result<(), TurnOutcome> {
        let parsed = match money_parse::parse(input) {
            Ok(parsed) => parsed,
            Err(reason) => return Err(self.refuse_money(input, reason)),
        };
        if continuation && parsed.amount.is_none() {
            return Ok(());
        }
        if let Some(error) = self.snapshot.project_error.clone() {
            return Err(self.refuse_money(input, &format!("project money/default is unavailable: {error} — correct nika.yaml before preparing work")));
        }
        if parsed.replaced_default.is_some() && parsed.replaced_default != self.snapshot.ceiling {
            return Err(self.refuse_money(input, "the stated default to replace does not match the observed project default — confirm one finite, nonnegative amount"));
        }
        let mut decision = self.money_decision(input, &parsed);
        if decision
            .effective_usd
            .is_none_or(|v| !v.is_finite() || v < 0.0)
        {
            return Err(self.refuse_money(input, money_parse::INVALID));
        }
        if continuation && let Some(previous) = &self.money.draft {
            decision
                .original_intent
                .clone_from(&previous.original_intent);
        }
        self.money.current = Some(decision.clone());
        self.money.draft = Some(decision);
        Ok(())
    }

    /// Called at every cognition seam. Deterministic reading stays available;
    /// the selected intelligence is never substituted by a monetary decision.
    pub(super) fn money_blocks_cognition(&self) -> bool {
        self.money
            .draft
            .as_ref()
            .is_some_and(|d| d.inference == InferenceEnforcement::CallsBlocked)
    }

    pub(super) fn cognition_money_refusal(&self) -> TurnOutcome {
        TurnOutcome::Refusal(Refusal::new(
            RefusalClass::NotAllowed,
            format!(
                "the monetary ceiling cannot be enforced by {}: Session has no aggregate USD admission/receipt seam — no cognition call was made; deterministic work remains available and the selected intelligence was kept",
                self.reasoner.name()
            ),
        ))
    }

    pub(super) fn bind_proposal_money(&mut self, id: &ProposalId) {
        self.money.pending = self.money.draft.clone().map(|mut d| {
            d.proposal = Some(id.clone());
            d
        });
        if self.money.pending.is_some() {
            self.money.current.clone_from(&self.money.pending);
        }
    }

    pub(super) fn save_proposal_money(&mut self, set: &ProjectChangeSet, id: &ProposalId) {
        let Some(decision) = self
            .money
            .pending
            .take()
            .filter(|d| d.proposal.as_ref() == Some(id))
        else {
            return;
        };
        for change in set.changes.iter().filter(|c| c.is_workflow()) {
            let path = change.path();
            self.money.saved.retain(|saved| saved.path != path);
            self.money.saved.push(SavedMoney {
                resolved: self.snapshot.root.join(&path).canonicalize().ok(),
                path,
                witness: Witness::of(change.content().as_bytes()),
                decision: decision.clone(),
            });
        }
        self.money.current = Some(decision);
    }

    pub(super) fn run_money(
        &mut self,
        input: &str,
        workflow: &std::path::Path,
        explicit: Option<f64>,
    ) -> Result<f64, TurnOutcome> {
        if explicit.is_none() {
            self.restore_run_money(input, workflow)?;
        }
        self.money
            .current
            .as_ref()
            .and_then(|d| d.effective_usd)
            .ok_or_else(|| self.refuse_money(input, money_parse::INVALID))
    }

    fn restore_run_money(
        &mut self,
        input: &str,
        workflow: &std::path::Path,
    ) -> Result<(), TurnOutcome> {
        let target = self.snapshot.root.join(workflow);
        let Ok(resolved) = target.canonicalize() else {
            return Err(self.refuse_money(
                input,
                "the workflow identity could not be resolved — review it before running",
            ));
        };
        let matches = |path: &std::path::Path| {
            path == workflow
                || self
                    .snapshot
                    .root
                    .join(path)
                    .canonicalize()
                    .is_ok_and(|p| p == resolved)
        };
        let mut saved = self
            .money
            .saved
            .iter()
            .filter(|saved| matches(&saved.path) || saved.resolved.as_ref() == Some(&resolved));
        if let Some(saved_money) = saved.next() {
            if saved.next().is_some() {
                return Err(self.refuse_money(input, "multiple saved monetary decisions resolve to this workflow — prepare and review one unambiguous revision"));
            }
            let unchanged = saved_money.resolved.as_ref() == Some(&resolved)
                && std::fs::read(&target)
                    .is_ok_and(|bytes| Witness::of(&bytes) == saved_money.witness);
            if !unchanged {
                return Err(self.refuse_money(input, "the prepared workflow changed since its monetary decision — prepare and review the revision before running"));
            }
            self.money.current = Some(saved_money.decision.clone());
            self.money.draft = Some(saved_money.decision.clone());
            return Ok(());
        }
        // The existing journal proves that Save occurred, but its v1 schema
        // does not record spending constraints. After reopening, absence of
        // an in-memory binding therefore cannot justify a fresh default. Read
        // at Run even when the host did not call restore_state; do not restore
        // execution authority or equate distinct files by their bytes.
        let records = match ConsentRecord::read_all(&self.snapshot.root) {
            Ok(records) if records.iter().all(|r| r.version == ConsentRecord::VERSION) => records,
            _ => return Err(self.refuse_money(input, "saved monetary evidence is unreadable — provide an explicit Run ceiling or prepare and review the workflow again")),
        };
        if records
            .iter()
            .any(|record| record.written.iter().any(|path| matches(path)))
        {
            return Err(self.refuse_money(input, "the saved spending constraint cannot be proved in this session — provide an explicit Run ceiling or prepare and review the workflow again; no default was substituted"));
        }
        Ok(())
    }
}

fn preview_with_money(set: &ProjectChangeSet, decision: Option<&MonetaryDecision>) -> String {
    match decision {
        Some(decision) => format!(
            "{}\n{}\nmonetary input: «{}»\n",
            set.preview(),
            decision.line(),
            decision.input
        ),
        None => set.preview(),
    }
}
