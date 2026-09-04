// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Mutate the event spine; project its committed terminal without refolding it.

use super::{
    JobEvent, JobStatus, JobStoreError, StoredJob, ValidatedEventBatch, ensure_receipt_matches,
    hash_event,
};

impl StoredJob {
    pub(super) fn final_sequence_after(
        &self,
        batch: &ValidatedEventBatch<'_>,
    ) -> Result<u64, JobStoreError> {
        let appended = u64::try_from(batch.len())
            .map_err(|_| JobStoreError::SequenceExhausted(self.record.id.clone()))?;
        self.event_count
            .checked_add(appended)
            .ok_or_else(|| JobStoreError::SequenceExhausted(self.record.id.clone()))
    }

    pub(super) fn append_payloads(
        &mut self,
        batch: &ValidatedEventBatch<'_>,
    ) -> Result<Vec<JobEvent>, JobStoreError> {
        for (index, payload) in batch.payloads().iter().enumerate() {
            if is_pause_payload(payload) {
                if !batch.transition
                    || self.record.status != JobStatus::Paused
                    || index + 1 != batch.len()
                {
                    return Err(JobStoreError::InvalidObservationEvent);
                }
                validate_pause_payload(payload, &self.record.id)?;
            }
        }
        let mut next = self
            .events
            .last()
            .map_or(Ok(1), |event| event.sequence.checked_add(1).ok_or(()))
            .map_err(|()| JobStoreError::SequenceExhausted(self.record.id.clone()))?;
        let mut appended = Vec::with_capacity(batch.len());
        for payload in batch.payloads() {
            let previous_hash = self.event_head.clone();
            let hash = hash_event(
                &self.record,
                self.terminal_sequence,
                next,
                previous_hash.as_ref(),
                payload,
            )?;
            let event = JobEvent {
                sequence: next,
                payload: payload.clone(),
                previous_hash,
                hash,
            };
            next = next
                .checked_add(1)
                .ok_or_else(|| JobStoreError::SequenceExhausted(self.record.id.clone()))?;
            self.events.push(event.clone());
            self.event_count = self
                .event_count
                .checked_add(1)
                .ok_or_else(|| JobStoreError::SequenceExhausted(self.record.id.clone()))?;
            self.event_head = Some(event.hash.clone());
            appended.push(event);
        }
        self.project_settlement()?;
        Ok(appended)
    }

    /// The terminal sequence is hash-bound and may precede later events.
    /// This cache never serializes: old v3 stores acquire it by reading the
    /// same validated event, with no migration, new hash or invented result.
    pub(super) fn project_settlement(&mut self) -> Result<(), JobStoreError> {
        let event = if self.record.status == JobStatus::Paused {
            // A pause closes an execution leg, not the resumable job.
            // Its result stays in the event payload so starting another leg
            // cannot invalidate a terminal hash bound to the mutable record.
            self.events.iter().rev().find(|event| {
                event.payload["kind"] == "execution.settled" && event.payload["status"] == "paused"
            })
        } else {
            self.terminal_sequence
                .filter(|_| self.record.status.is_settled())
                .and_then(|sequence| sequence.checked_sub(1))
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| self.events.get(index))
        };
        self.record.settlement = event
            .and_then(|event| event.payload.get("settlement"))
            .map(|settlement| {
                if !settlement.is_object()
                    || settlement.get("status").is_some_and(|status| {
                        status.as_str() != Some(self.record.status.to_string().as_str())
                    })
                {
                    return Err(JobStoreError::Corrupt(
                        "settlement contradicts the observation status".to_owned(),
                    ));
                }
                Ok(settlement.clone())
            })
            .transpose()?;
        self.record.paused_outputs = None;
        self.record.paused_receipt = None;
        if self.record.status == JobStatus::Paused
            && let Some(event) = event
        {
            self.record.paused_outputs = event
                .payload
                .get("outputs")
                .map(|value| serde_json::from_value(value.clone()))
                .transpose()
                .map_err(|_| JobStoreError::Corrupt("paused outputs are invalid".to_owned()))?;
            self.record.paused_receipt = event
                .payload
                .get("receipt")
                .map(|value| serde_json::from_value(value.clone()))
                .transpose()
                .map_err(|_| JobStoreError::Corrupt("paused receipt is invalid".to_owned()))?;
            if let Some(receipt) = &self.record.paused_receipt {
                receipt.validate()?;
                ensure_receipt_matches(&self.record, receipt)?;
            }
        }
        Ok(())
    }
}

fn is_pause_payload(payload: &serde_json::Value) -> bool {
    payload["kind"] == "execution.settled" && payload["status"] == "paused"
}

/// Historical legs are checked against their own receipt, not a later leg's
/// mutable execution identity. Current-leg full binding is checked on append.
pub(super) fn validate_pause_payload(
    payload: &serde_json::Value,
    job_id: &super::JobId,
) -> Result<(), JobStoreError> {
    if !is_pause_payload(payload) {
        return Ok(());
    }
    if payload
        .get("outputs")
        .is_some_and(|outputs| !outputs.is_object())
    {
        return Err(JobStoreError::Corrupt(
            "paused outputs are invalid".to_owned(),
        ));
    }
    if let Some(value) = payload.get("receipt") {
        let receipt: super::JobReceipt =
            serde_json::from_value(value.clone()).map_err(|_| JobStoreError::InvalidReceipt)?;
        receipt.validate()?;
        if receipt.job_id() != job_id {
            return Err(JobStoreError::ReceiptIdentityMismatch);
        }
    }
    if let Some(settlement) = payload.get("settlement")
        && (!settlement.is_object()
            || settlement
                .get("status")
                .is_some_and(|status| status != "paused"))
    {
        return Err(JobStoreError::Corrupt(
            "paused settlement is invalid".to_owned(),
        ));
    }
    Ok(())
}
