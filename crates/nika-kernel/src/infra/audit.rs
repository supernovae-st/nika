// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `AuditSink` — append-only, never-sampled, persist-or-fail.
//!
//! 5th observability channel (after Event/Metrics/Trace/Billing). Used by
//! Shield (capability override, taint, canary leak), budget enforcement,
//! and compliance signals (GDPR, EU AI Act). Records MUST persist before
//! the trait method returns `Ok` — implementations cannot buffer in memory.
//!
//! See: ADR-028 (forward-compat reservation policy), Q12 of
//! `docs/architecture/l0-l05-architecture-decisions.md`, ADR-014 (sealed).

use nika_error::id::TenantId;
use serde::{Deserialize, Serialize};

use crate::sealed;

/// Severity level for audit records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Severity {
    /// Informational — admin override, key rotation.
    Info,
    /// Warning — soft budget breach, retry exhaustion.
    Warn,
    /// Critical — taint violation, canary leak, hard budget breach.
    Critical,
}

/// Append-only audit record. Every public variant carries a `tenant_id`
/// for multi-tenant routing. New variants are added behind `Extension`
/// before promotion to first-class variants (ADR-007 + ADR-028).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AuditRecord {
    /// A capability check was overridden by an admin.
    CapabilityOverride {
        /// Target tenant.
        tenant_id: TenantId,
        /// Capability that was bypassed.
        capability: String,
        /// Operator-provided reason.
        reason: String,
    },
    /// Taint propagation detected an untrusted-data → privileged-action path.
    TaintViolation {
        /// Target tenant.
        tenant_id: TenantId,
        /// Source -> sink path description.
        path: String,
        /// Severity of the violation.
        severity: Severity,
    },
    /// A canary token leaked through model output.
    CanaryLeaked {
        /// Target tenant.
        tenant_id: TenantId,
        /// Identifier of the canary that leaked.
        canary_id: String,
    },
    /// Budget cap (tokens, cost, runs) exhausted for the tenant.
    BudgetExhausted {
        /// Target tenant.
        tenant_id: TenantId,
        /// Budget dimension (e.g. `"tokens"`, `"cost_usd"`, `"runs"`).
        dimension: String,
        /// Hard cap.
        cap: u64,
        /// Observed value at the time of the breach.
        observed: u64,
    },
    /// Policy denied a workflow step (Shield).
    PolicyDenied {
        /// Target tenant.
        tenant_id: TenantId,
        /// Policy rule identifier (e.g. `"L-SEC-003"`).
        rule_id: String,
        /// What was denied.
        target: String,
    },
    /// A secret/key was used (audit trail for `KeyProvider` access).
    KeyUsed {
        /// Target tenant.
        tenant_id: TenantId,
        /// Key identifier (never the value).
        key_id: String,
    },
    /// A secret was redacted in an outbound payload.
    SecretRedacted {
        /// Target tenant.
        tenant_id: TenantId,
        /// Where the secret was found.
        location: String,
    },
    /// Future-feature escape hatch. Must be promoted to a first-class
    /// variant once the namespace stabilises (ADR-028).
    Extension {
        /// Dotted namespace (e.g. `"compliance.gdpr"`).
        ns: String,
        /// Variant name within the namespace.
        name: String,
        /// Free-form payload — implementations MUST treat as untrusted.
        payload: serde_json::Value,
    },
}

impl AuditRecord {
    /// Convenience: build a `CapabilityOverride` record.
    #[must_use]
    pub fn capability_override(
        tenant_id: impl Into<String>,
        capability: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::CapabilityOverride {
            tenant_id: TenantId::new(tenant_id),
            capability: capability.into(),
            reason: reason.into(),
        }
    }

    /// Convenience: build a `TaintViolation` record.
    #[must_use]
    pub fn taint_violation(
        tenant_id: impl Into<String>,
        path: impl Into<String>,
        severity: Severity,
    ) -> Self {
        Self::TaintViolation {
            tenant_id: TenantId::new(tenant_id),
            path: path.into(),
            severity,
        }
    }

    /// Convenience: build a `CanaryLeaked` record.
    #[must_use]
    pub fn canary_leaked(tenant_id: impl Into<String>, canary_id: impl Into<String>) -> Self {
        Self::CanaryLeaked {
            tenant_id: TenantId::new(tenant_id),
            canary_id: canary_id.into(),
        }
    }

    /// Convenience: build a `BudgetExhausted` record.
    #[must_use]
    pub fn budget_exhausted(
        tenant_id: impl Into<String>,
        dimension: impl Into<String>,
        cap: u64,
        observed: u64,
    ) -> Self {
        Self::BudgetExhausted {
            tenant_id: TenantId::new(tenant_id),
            dimension: dimension.into(),
            cap,
            observed,
        }
    }

    /// Convenience: build an `Extension` record.
    #[must_use]
    pub fn extension(
        ns: impl Into<String>,
        name: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self::Extension {
            ns: ns.into(),
            name: name.into(),
            payload,
        }
    }
}

/// Errors a sink can raise. Persist failures are non-recoverable; the
/// caller should kill the run (compliance contract).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum AuditSinkError {
    /// Persisted backend (file/db) write failed.
    #[error("audit persist failed: {reason}")]
    PersistFailed {
        /// What went wrong.
        reason: String,
    },
    /// Sink not configured (boot-time error).
    #[error("audit sink not configured: {reason}")]
    NotConfigured {
        /// Why no sink.
        reason: String,
    },
}

/// `AuditSink` — append-only, never-sampled. Implementations MUST persist
/// the record durably (file, DB, log-shipping appliance) before returning
/// `Ok`. Buffering in memory is forbidden by contract.
#[trait_variant::make(AuditSinkDyn: Send)]
pub trait AuditSink: Send + Sync + sealed::Sealed {
    /// Append a record to the audit log.
    ///
    /// CANCEL SAFETY: cancel-safe by contract. Impls MUST complete the
    /// durable write before the first `.await` return point (fsync on
    /// a local log, synchronous HTTP POST to an append-only sink). If
    /// the caller drops the future, the record either landed fully or
    /// never started — never partial. Compliance audits require this.
    async fn audit(&self, record: AuditRecord) -> Result<(), AuditSinkError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn audit_types_are_send_sync() {
        _assert_send_sync::<AuditRecord>();
        _assert_send_sync::<AuditSinkError>();
        _assert_send_sync::<Severity>();
    }

    #[test]
    fn audit_record_serde_roundtrip_capability() {
        let r = AuditRecord::capability_override("t-1", "verb.invoke", "admin override");
        let json = serde_json::to_string(&r).expect("serialize");
        let back: AuditRecord = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(back, AuditRecord::CapabilityOverride { .. }));
    }

    #[test]
    fn audit_record_extension_payload_preserved() {
        let r = AuditRecord::extension(
            "compliance.eu_ai_act",
            "incident_report",
            serde_json::json!({"severity": "high"}),
        );
        let json = serde_json::to_string(&r).expect("serialize");
        assert!(json.contains("incident_report"));
        assert!(json.contains("severity"));
    }

    #[test]
    fn audit_record_taint_roundtrip_with_severity() {
        let r = AuditRecord::taint_violation("t", "src->sink", Severity::Critical);
        let json = serde_json::to_string(&r).expect("serialize");
        let back: AuditRecord = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(
            back,
            AuditRecord::TaintViolation {
                severity: Severity::Critical,
                ..
            }
        ));
    }

    #[test]
    fn audit_sink_error_displays() {
        let e = AuditSinkError::PersistFailed {
            reason: "disk full".into(),
        };
        assert!(e.to_string().contains("persist failed"));
        let e = AuditSinkError::NotConfigured {
            reason: "no sink".into(),
        };
        assert!(e.to_string().contains("not configured"));
    }
}
