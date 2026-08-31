// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use sha2::{Digest as _, Sha256};

use crate::{JobOrigin, JobReceipt, JobRecord, JobStoreError, RequestDigest};

use super::{EventHash, IDENTITY_HASH_DOMAIN, StoredJob};

pub(super) fn validate_snapshot_digest(value: &str) -> Result<(), JobStoreError> {
    RequestDigest::new(value.to_owned()).map(|_| ())
}

pub(super) fn ensure_receipt_matches(
    record: &JobRecord,
    receipt: &JobReceipt,
) -> Result<(), JobStoreError> {
    if receipt.job_id() != &record.id
        || receipt.execution_id() != record.execution_id
        || receipt.trace_id() != record.trace_id
        || receipt.snapshot_digest() != record.snapshot_digest
        || receipt
            .origin()
            .is_some_and(|origin| origin != &record.origin)
        || (receipt.origin().is_none() && record.origin != JobOrigin::Manual)
    {
        return Err(JobStoreError::ReceiptIdentityMismatch);
    }
    Ok(())
}

pub(super) fn attach_interrupted_receipt(record: &mut JobRecord) -> Result<(), JobStoreError> {
    if !has_complete_execution_identity(record) {
        return Err(JobStoreError::InvalidReceipt);
    }
    record.receipt = Some(JobReceipt::with_origin(
        record.id.clone(),
        record.execution_id.clone(),
        record.trace_id.clone(),
        record.snapshot_digest.clone(),
        None,
        record.origin.clone(),
    )?);
    Ok(())
}

pub(super) fn has_complete_execution_identity(record: &JobRecord) -> bool {
    !record.execution_id.is_empty()
        && !record.trace_id.is_empty()
        && !record.snapshot_digest.is_empty()
}

pub(super) fn migrate_legacy_nonterminal_record(mut record: JobRecord) -> JobRecord {
    record.execution_id.clear();
    record.trace_id.clear();
    record.snapshot_digest.clear();
    record
}

pub(super) fn validate_identity_binding(job: &StoredJob) -> Result<(), JobStoreError> {
    if job.record.execution_id.is_empty() {
        if job.identity_digest.is_some() {
            return Err(JobStoreError::Corrupt(
                "identity digest is present without an execution identity".to_owned(),
            ));
        }
        return Ok(());
    }
    let persisted = job.identity_digest.as_ref().ok_or_else(|| {
        JobStoreError::Corrupt("execution identity is missing its binding digest".to_owned())
    })?;
    persisted.validate()?;
    if persisted != &hash_execution_identity(&job.record)? {
        return Err(JobStoreError::Corrupt(
            "execution identity does not match its canonical binding digest".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn hash_execution_identity(record: &JobRecord) -> Result<EventHash, JobStoreError> {
    // Preserve the exact v3 manual preimage so existing durable stores remain
    // readable. Schedule provenance is additive and participates in the
    // identity binding for every newly-introduced scheduled run.
    let preimage = if record.origin == JobOrigin::Manual {
        serde_json::json!({
            "execution_id": &record.execution_id,
            "job_id": record.id.as_str(),
            "request_digest": record.request_digest.as_str(),
            "snapshot_digest": &record.snapshot_digest,
            "trace_id": &record.trace_id,
        })
    } else {
        serde_json::json!({
            "execution_id": &record.execution_id,
            "job_id": record.id.as_str(),
            "origin": &record.origin,
            "request_digest": record.request_digest.as_str(),
            "snapshot_digest": &record.snapshot_digest,
            "trace_id": &record.trace_id,
        })
    };
    let canonical = serde_json::to_vec(&preimage)
        .map_err(|_| JobStoreError::Corrupt("identity preimage cannot be encoded".to_owned()))?;
    let mut hasher = Sha256::new();
    hasher.update(IDENTITY_HASH_DOMAIN);
    hasher.update(canonical);
    Ok(EventHash::from_bytes(hasher.finalize().into()))
}

pub(super) fn validate_terminal_record(record: &JobRecord) -> Result<(), JobStoreError> {
    record
        .origin
        .validate()
        .map_err(|_| JobStoreError::Corrupt("job origin is invalid".to_owned()))?;
    if record.execution_id.is_empty() != record.trace_id.is_empty() {
        return Err(JobStoreError::Corrupt(
            "execution and trace identities must be present together".to_owned(),
        ));
    }
    if !record.snapshot_digest.is_empty() {
        if record.execution_id.is_empty() {
            return Err(JobStoreError::Corrupt(
                "snapshot digest is missing its execution identity".to_owned(),
            ));
        }
        validate_snapshot_digest(&record.snapshot_digest)
            .map_err(|_| JobStoreError::Corrupt("snapshot digest is invalid".to_owned()))?;
    }
    if !record.status.is_settled() && (record.outputs.is_some() || record.receipt.is_some()) {
        return Err(JobStoreError::Corrupt(
            "unsettled job carries terminal result data".to_owned(),
        ));
    }
    if let Some(receipt) = &record.receipt {
        receipt
            .validate()
            .map_err(|_| JobStoreError::Corrupt("terminal receipt is invalid".to_owned()))?;
        ensure_receipt_matches(record, receipt).map_err(|_| {
            JobStoreError::Corrupt("terminal receipt identity mismatches".to_owned())
        })?;
    }
    Ok(())
}
