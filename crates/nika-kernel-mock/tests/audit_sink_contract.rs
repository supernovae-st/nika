// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Contract test for `AuditSink` — every variant must round-trip + persist.
//!
//! Lives in `nika-kernel-mock` rather than `nika-kernel` to avoid a
//! dev-dependency cycle (mock already depends on kernel).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use nika_kernel::audit::{AuditRecord, AuditSink, AuditSinkError, Severity};
use nika_kernel_mock::audit::{FailingAuditSink, NullAuditSink};

#[tokio::test]
async fn audit_sink_accepts_capability_override() {
    let sink = NullAuditSink::new();
    let rec =
        AuditRecord::capability_override("tenant-123", "verb.invoke", "shield bypass requested");
    let result = sink.audit(rec).await;
    assert!(result.is_ok(), "AuditSink must persist or fail loudly");
}

#[tokio::test]
async fn audit_sink_accepts_taint_violation() {
    let sink = NullAuditSink::new();
    let rec = AuditRecord::taint_violation(
        "tenant-123",
        "untrusted_input -> exec_shell",
        Severity::Critical,
    );
    assert!(sink.audit(rec).await.is_ok());
}

#[tokio::test]
async fn audit_sink_accepts_budget_exhausted() {
    let sink = NullAuditSink::new();
    let rec = AuditRecord::budget_exhausted("tenant-123", "tokens", 100_000, 99_999);
    assert!(sink.audit(rec).await.is_ok());
}

#[tokio::test]
async fn audit_sink_propagates_persist_failure() {
    let sink = FailingAuditSink::new();
    let rec = AuditRecord::canary_leaked("tenant-123", "secret-id-42");
    let err = sink.audit(rec).await.expect_err("must fail");
    assert!(matches!(err, AuditSinkError::PersistFailed { .. }));
}

#[test]
fn audit_record_extension_escape_hatch() {
    // Future variants (GDPR, EU AI Act) ship via Extension before getting
    // promoted to first-class variants.
    let rec = AuditRecord::extension(
        "compliance.gdpr",
        "right_to_erasure",
        serde_json::json!({"subject_id": "user-42", "tenant": "t-1"}),
    );
    let json = serde_json::to_string(&rec).expect("serializable");
    assert!(json.contains("right_to_erasure"));
}
