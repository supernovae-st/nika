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
    // Persistent Session inference constraint, independent of the next
    // proposal/Run default; a refusal/zero survives even without an account.
    pub inference_guard: Option<MonetaryDecision>,
    pub account: Option<nika_providers::InferenceAdmission>,
    pub admission_note: Option<String>,
    // A paused Run amendment holds cognition only until that gate is answered.
    // It never replaces the Session draft, guard or shared admission account.
    pub gate: Option<MonetaryDecision>,
    pub reconfirm: bool,
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

    pub(super) fn draft_review(
        &self,
        set: &ProjectChangeSet,
        out: &nika_onboard::compile::CompileOutcome,
        bytes: &str,
    ) -> String {
        let review = crate::review::render(set, out, bytes);
        match &self.money.draft {
            Some(decision) => format!("{review}{}\n", scoped_money_line(decision)),
            None => review,
        }
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
            scoped_money_line,
        ) + "\n"
            + &self.inference_line()
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
            admission: None,
            observed_cost_usd: None,
            proposal: None,
            refusal: None,
        }
    }

    fn rejected_money(&self, input: &str, reason: &str) -> MonetaryDecision {
        let mut decision = self.money_decision(input, &ParsedMoney::default());
        decision.effective_usd = None;
        decision.source = MonetarySource::Rejected;
        decision.refusal = Some(reason.to_owned());
        decision.inference = InferenceEnforcement::CallsBlocked;
        decision
    }

    pub(super) fn refuse_money(&mut self, input: &str, reason: &str) -> TurnOutcome {
        self.retain_money_guard();
        if let Some(a) = &self.money.account {
            let _ = a.close(reason);
        }
        let mut decision = self.rejected_money(input, reason);
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
        // Session rejection expires authority, not the cognition restriction.
        self.money.inference_guard = Some(decision.clone());
        self.money.draft = Some(decision.clone());
        self.money.current = Some(decision);
        self.expire_money_authority();
        TurnOutcome::Refusal(Refusal::new(RefusalClass::NotAllowed, reason))
    }

    fn expire_money_authority(&mut self) {
        self.money.pending = None;
        self.pending = None;
        self.authoring = None;
        self.run_inputs = None;
        self.interrupted = None;
        self.intent.unresolved.clear();
        self.last_outcome = None;
    }

    // Shared validation precedes either scope's continuation fast path.
    fn read_money(&self, input: &str) -> Result<ParsedMoney, String> {
        let parsed = money_parse::parse(input).map_err(str::to_owned)?;
        if let Some(error) = &self.snapshot.project_error {
            return Err(format!(
                "project money/default is unavailable: {error} — correct nika.yaml before preparing work"
            ));
        }
        if parsed
            .amount
            .or(self.snapshot.ceiling)
            .is_some_and(|v| !v.is_finite() || v < 0.0)
        {
            return Err(money_parse::INVALID.into());
        }
        if parsed.replaced_default.is_some() && parsed.replaced_default != self.snapshot.ceiling {
            return Err("the stated default to replace does not match the observed project default — confirm one finite, nonnegative amount".into());
        }
        Ok(parsed)
    }

    pub(super) fn admit_gate_money(&mut self, input: &str) -> Result<(), TurnOutcome> {
        let mut decision = match self.read_money(input) {
            Ok(parsed) if parsed.amount.is_none() => return Ok(()),
            Ok(parsed) => self.money_decision(input, &parsed),
            Err(reason) => {
                let decision = self.rejected_money(input, &reason);
                self.record_gate_money(decision);
                self.expire_money_authority();
                return Err(TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::NotAllowed,
                    reason,
                )));
            }
        };
        decision.inference = InferenceEnforcement::CallsBlocked;
        self.record_gate_money(decision);
        Ok(())
    }

    fn record_gate_money(&mut self, mut decision: MonetaryDecision) {
        if let Some(previous) = &self.money.gate {
            decision
                .original_intent
                .clone_from(&previous.original_intent);
        }
        self.money.gate = Some(decision.clone());
        self.money.current = Some(decision);
    }

    pub(super) fn restore_gate_money(&mut self) {
        if self.money.gate.is_none() {
            // Only a hold was proved. Neither an amount nor an account is restored.
            let decision = self.rejected_money(
                "restored paused gate",
                "restored gate monetary amendment held; amount and billed cost unknown",
            );
            self.record_gate_money(decision);
        }
    }

    pub(super) fn finish_gate_money(&mut self) {
        // Only the protocol answer ends this scope. No account is restored,
        // replaced, reopened or repriced: the prior Session state stayed live.
        if self.money.gate.take().is_some() {
            self.money.current.clone_from(&self.money.draft);
        }
    }

    /// Called before any compiler/classifier/reasoner. A continuation with
    /// no monetary clause keeps the admitted round's money, not a new default.
    pub(super) fn admit_money(
        &mut self,
        input: &str,
        continuation: bool,
    ) -> Result<(), TurnOutcome> {
        // A fresh turn is not an escape from a still-pending gate amendment.
        if self.money.gate.is_some() {
            return self.admit_gate_money(input);
        }
        let parsed = match self.read_money(input) {
            Ok(parsed) => parsed,
            Err(reason) => return Err(self.refuse_money(input, &reason)),
        };
        // The Session restriction cannot replace an independent gate observation.
        // Gate parsing above still holds cognition, but does not consume this flag.
        if self.money.reconfirm && parsed.amount.is_none() {
            return Err(self.refuse_money(input, super::inference::RESTORED_EXPOSURE));
        }
        if continuation && parsed.amount.is_none() {
            return Ok(());
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
        if parsed.amount.is_some() {
            self.retain_money_guard();
            self.configure_admission(&mut decision);
            self.money.inference_guard = Some(decision.clone());
        } else if self.money.inference_guard.is_none() && self.money.account.is_none() {
            self.configure_admission(&mut decision);
            if decision.inference == InferenceEnforcement::CallsBlocked {
                self.retain_money_guard();
                self.money.inference_guard = Some(decision.clone());
            }
        }
        // A fresh request owns its proposal/Run default, but only an explicit
        // Session amendment can change the persistent inference constraint.
        let decision = self.inference_observation(decision);
        self.money.current = Some(decision.clone());
        self.money.draft = Some(decision);
        Ok(())
    }

    /// Called at every cognition seam. Deterministic reading stays available;
    /// the selected intelligence is never substituted by a monetary decision.
    pub(super) fn money_blocks_cognition(&self) -> bool {
        if self.money.gate.is_some() || self.money.reconfirm {
            return true;
        }
        if let Some(a) = &self.money.account
            && a.snapshot().map_or(true, |r| {
                r.state != nika_providers::AdmissionState::Open || r.limit.nano_usd == 0
            })
        {
            return true;
        }
        self.money
            .inference_guard
            .as_ref()
            .is_some_and(|d| d.inference == InferenceEnforcement::CallsBlocked)
    }

    pub(super) fn cognition_money_refusal(&self) -> TurnOutcome {
        TurnOutcome::Refusal(Refusal::new(
            RefusalClass::NotAllowed,
            format!(
                "no further cognition admitted on {}: {} · deterministic work remains available; billed cost is unknown",
                self.reasoner.name(),
                self.inference_line()
            ),
        ))
    }

    pub(super) fn bind_proposal_money(&mut self, id: &ProposalId) {
        let receipt = self.money.account.as_ref().and_then(|a| a.snapshot().ok());
        self.money.pending = self.money.draft.clone().map(|mut d| {
            d.proposal = Some(id.clone());
            d.admission = receipt;
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
            self.money.current = Some(self.inference_observation(saved_money.decision.clone()));
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
        // No saved identity or journal claim binds this file. Its own Run
        // default does not amend the ongoing Session inference allowance.
        self.money.current =
            Some(self.inference_observation(self.money_decision(input, &ParsedMoney::default())));
        Ok(())
    }

    fn inference_observation(&self, mut decision: MonetaryDecision) -> MonetaryDecision {
        decision.inference = if self.money_blocks_cognition() {
            InferenceEnforcement::CallsBlocked
        } else if self.money.account.is_some() {
            InferenceEnforcement::CatalogAdmission
        } else {
            InferenceEnforcement::NotMetered
        };
        decision.admission = self.money.account.as_ref().and_then(|a| a.snapshot().ok());
        decision
    }
}

fn preview_with_money(set: &ProjectChangeSet, decision: Option<&MonetaryDecision>) -> String {
    match decision {
        Some(decision) => format!(
            "{}\n{}\nmonetary input: «{}»\n",
            set.preview(),
            scoped_money_line(decision),
            decision.input
        ),
        None => set.preview(),
    }
}

// The work's execution ceiling and Session inference allowance can differ.
// Render only stable admission limits here: live spending must not change a
// reviewed proposal's consent identity between preview and an answer.
fn scoped_money_line(decision: &MonetaryDecision) -> String {
    if decision.refusal.is_some() {
        return decision.line();
    }
    let inference = match decision.inference {
        InferenceEnforcement::CatalogAdmission => decision.admission.as_ref().map_or_else(
            || "catalog allowance tracked separately".to_owned(),
            |receipt| format!("catalog allowance {}", receipt.limit),
        ),
        InferenceEnforcement::CallsBlocked => {
            "blocked by the Session monetary constraint".to_owned()
        }
        InferenceEnforcement::NotMetered => {
            "unmetered; no aggregate inference allowance".to_owned()
        }
    };
    format!(
        "money: ${} USD · {:?} · proposal/Run ceiling · project default {:?} · policy cap unknown · machine cap unknown · execution requires a separate Run and downstream admission\nSession inference: {inference}; separate from the proposal/Run ceiling; catalog estimates are not invoices or a hard billing cap; billed cost unknown",
        decision.effective_usd.unwrap_or_default(),
        decision.source,
        decision.project_default_usd,
    )
}
