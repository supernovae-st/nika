// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Strongly typed identifiers for the Nika diamond.
//!
//! UUID-based IDs use `UUIDv7` (RFC 9562, time-ordered) for natural sortability.
//! String-based IDs are domain identifiers that come from external systems.
//!
//! ## Why `UUIDv7` not `ULID`?
//! - RFC 9562 (2024 standard) vs `ULID` (community spec).
//! - Better tooling (uuid crate, `PostgreSQL`, etc.).
//! - Time-ordered: sortable like ULID, but standard.
//!
//! ## Newtype discipline
//! Every ID is a newtype. Never raw `String` or `Uuid` in APIs.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── UUID-based IDs (`UUIDv7`, time-ordered) ──────────────────────────

/// Run identifier (`UUIDv7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RunId {
    /// The underlying UUID.
    pub uuid: Uuid,
}

impl RunId {
    /// Create a new run ID from a UUID.
    #[must_use]
    pub fn new(uuid: Uuid) -> Self {
        Self { uuid }
    }

    /// Generate a new `UUIDv7` run ID.
    #[must_use]
    pub fn generate() -> Self {
        Self {
            uuid: Uuid::now_v7(),
        }
    }

    /// Create from raw bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self {
            uuid: Uuid::from_bytes(bytes),
        }
    }

    /// Nil (zero) run ID.
    #[must_use]
    pub fn nil() -> Self {
        Self { uuid: Uuid::nil() }
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "run-{}", self.uuid)
    }
}

/// Event identifier (`UUIDv7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EventId {
    /// The underlying UUID.
    pub uuid: Uuid,
}

impl EventId {
    /// Create a new event ID from a UUID.
    #[must_use]
    pub fn new(uuid: Uuid) -> Self {
        Self { uuid }
    }

    /// Generate a new `UUIDv7` event ID.
    #[must_use]
    pub fn generate() -> Self {
        Self {
            uuid: Uuid::now_v7(),
        }
    }

    /// Nil (zero) event ID.
    #[must_use]
    pub fn nil() -> Self {
        Self { uuid: Uuid::nil() }
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "evt-{}", self.uuid)
    }
}

/// Correlation identifier (`UUIDv7`). Machine-generated, survives retries.
///
/// Distinct from `TraceId` (W3C per-span tree) and `TaskId` (user-named).
/// `CorrelationId` represents a single user action that may span multiple
/// traces if the operation is retried. Support tickets ("what happened at
/// 3pm?") need this. Decision T2:A: `UUIDv7`-based, not String, because it
/// is machine-generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CorrelationId {
    /// The underlying UUID.
    pub uuid: Uuid,
}

impl CorrelationId {
    /// Create a new correlation ID from a UUID.
    #[must_use]
    pub fn new(uuid: Uuid) -> Self {
        Self { uuid }
    }

    /// Generate a new `UUIDv7` correlation ID.
    #[must_use]
    pub fn generate() -> Self {
        Self {
            uuid: Uuid::now_v7(),
        }
    }

    /// Nil (zero) correlation ID.
    #[must_use]
    pub fn nil() -> Self {
        Self { uuid: Uuid::nil() }
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cor-{}", self.uuid)
    }
}

// ─── Trace IDs (W3C Trace Context compatible) ───────────────────────

/// W3C Trace Context trace ID (16 bytes / 128 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct TraceId {
    /// Raw 16-byte trace identifier.
    pub bytes: [u8; 16],
}

impl TraceId {
    /// Create a trace ID from raw bytes.
    #[must_use]
    pub fn new(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }

    /// Nil (zero) trace ID.
    #[must_use]
    pub fn nil() -> Self {
        Self { bytes: [0; 16] }
    }

    /// Whether this is the nil trace ID.
    #[must_use]
    pub fn is_nil(&self) -> bool {
        self.bytes == [0; 16]
    }
}

impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.bytes {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl Serialize for TraceId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TraceId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        if s.len() != 32 {
            return Err(serde::de::Error::custom("trace ID must be 32 hex chars"));
        }
        let mut bytes = [0u8; 16];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hex = std::str::from_utf8(chunk)
                .map_err(|e| serde::de::Error::custom(format!("invalid utf8: {e}")))?;
            bytes[i] = u8::from_str_radix(hex, 16)
                .map_err(|e| serde::de::Error::custom(format!("invalid hex: {e}")))?;
        }
        Ok(Self { bytes })
    }
}

/// W3C Trace Context span ID (8 bytes / 64 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct SpanId {
    /// Raw 8-byte span identifier.
    pub bytes: [u8; 8],
}

impl SpanId {
    /// Create a span ID from raw bytes.
    #[must_use]
    pub fn new(bytes: [u8; 8]) -> Self {
        Self { bytes }
    }

    /// Nil (zero) span ID.
    #[must_use]
    pub fn nil() -> Self {
        Self { bytes: [0; 8] }
    }

    /// Whether this is the nil span ID.
    #[must_use]
    pub fn is_nil(&self) -> bool {
        self.bytes == [0; 8]
    }
}

impl fmt::Display for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.bytes {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl Serialize for SpanId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SpanId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        if s.len() != 16 {
            return Err(serde::de::Error::custom("span ID must be 16 hex chars"));
        }
        let mut bytes = [0u8; 8];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hex = std::str::from_utf8(chunk)
                .map_err(|e| serde::de::Error::custom(format!("invalid utf8: {e}")))?;
            bytes[i] = u8::from_str_radix(hex, 16)
                .map_err(|e| serde::de::Error::custom(format!("invalid hex: {e}")))?;
        }
        Ok(Self { bytes })
    }
}

// ─── String-based domain IDs ────────────────────────────────────────

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[non_exhaustive]
        pub struct $name {
            /// The identifier value.
            pub value: String,
        }

        impl $name {
            /// Create a new identifier.
            #[must_use]
            pub fn new(value: impl Into<String>) -> Self {
                Self {
                    value: value.into(),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.value)
            }
        }
    };
}

string_id!(
    /// Workflow identifier (from YAML `workflow:` field).
    WorkflowId
);

string_id!(
    /// Tenant identifier for multi-tenant deployments.
    TenantId
);

string_id!(
    /// Provider identifier (e.g., `"anthropic"`, `"openai"`).
    ProviderId
);

string_id!(
    /// Model identifier (e.g., `"claude-sonnet-4-20250514"`).
    ModelId
);

#[cfg(test)]
mod tests {
    use super::*;

    // ─── RunId ──────────────────────────────────────────────────────

    #[test]
    fn run_id_generate_is_unique() {
        let a = RunId::generate();
        let b = RunId::generate();
        assert_ne!(a, b);
    }

    #[test]
    fn run_id_nil() {
        let id = RunId::nil();
        assert_eq!(id.uuid, Uuid::nil());
    }

    #[test]
    fn run_id_display_prefix() {
        let id = RunId::nil();
        assert!(id.to_string().starts_with("run-"));
    }

    #[test]
    fn run_id_from_bytes() {
        let bytes = [1u8; 16];
        let id = RunId::from_bytes(bytes);
        assert_eq!(id.uuid.as_bytes(), &bytes);
    }

    #[test]
    fn run_id_serde_roundtrip() {
        let id = RunId::generate();
        let json = serde_json::to_string(&id).expect("serialize");
        let back: RunId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    // ─── EventId ────────────────────────────────────────────────────

    #[test]
    fn event_id_generate_is_unique() {
        let a = EventId::generate();
        let b = EventId::generate();
        assert_ne!(a, b);
    }

    #[test]
    fn event_id_nil() {
        let id = EventId::nil();
        assert_eq!(id.uuid, Uuid::nil());
    }

    #[test]
    fn event_id_display_prefix() {
        let id = EventId::nil();
        assert!(id.to_string().starts_with("evt-"));
    }

    #[test]
    fn event_id_serde_roundtrip() {
        let id = EventId::generate();
        let json = serde_json::to_string(&id).expect("serialize");
        let back: EventId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    // ─── CorrelationId (NEW — T2:A) ────────────────────────────────

    #[test]
    fn correlation_id_generate_is_unique() {
        let a = CorrelationId::generate();
        let b = CorrelationId::generate();
        assert_ne!(a, b);
    }

    #[test]
    fn correlation_id_nil() {
        let id = CorrelationId::nil();
        assert_eq!(id.uuid, Uuid::nil());
    }

    #[test]
    fn correlation_id_display_prefix() {
        let id = CorrelationId::nil();
        assert!(id.to_string().starts_with("cor-"));
    }

    #[test]
    fn correlation_id_serde_roundtrip() {
        let id = CorrelationId::generate();
        let json = serde_json::to_string(&id).expect("serialize");
        let back: CorrelationId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    // ─── TraceId ────────────────────────────────────────────────────

    #[test]
    fn trace_id_nil() {
        let id = TraceId::nil();
        assert!(id.is_nil());
        assert_eq!(id.to_string(), "00000000000000000000000000000000");
    }

    #[test]
    fn trace_id_display_hex() {
        let id = TraceId::new([0xff; 16]);
        assert_eq!(id.to_string(), "ffffffffffffffffffffffffffffffff");
        assert_eq!(id.to_string().len(), 32);
    }

    #[test]
    fn trace_id_serde_roundtrip() {
        let id = TraceId::new([
            0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45,
            0x67, 0x89,
        ]);
        let json = serde_json::to_string(&id).expect("serialize");
        let back: TraceId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    // ─── SpanId ─────────────────────────────────────────────────────

    #[test]
    fn span_id_nil() {
        let id = SpanId::nil();
        assert!(id.is_nil());
        assert_eq!(id.to_string(), "0000000000000000");
    }

    #[test]
    fn span_id_display_hex() {
        let id = SpanId::new([0xff; 8]);
        assert_eq!(id.to_string(), "ffffffffffffffff");
        assert_eq!(id.to_string().len(), 16);
    }

    #[test]
    fn span_id_serde_roundtrip() {
        let id = SpanId::new([0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]);
        let json = serde_json::to_string(&id).expect("serialize");
        let back: SpanId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    // ─── String IDs ─────────────────────────────────────────────────

    #[test]
    fn workflow_id_serde_roundtrip() {
        let id = WorkflowId::new("research-pipeline");
        let json = serde_json::to_string(&id).expect("serialize");
        let back: WorkflowId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn tenant_id_new() {
        let id = TenantId::new("org-supernovae");
        assert_eq!(id.value, "org-supernovae");
    }

    #[test]
    fn provider_id_new() {
        let id = ProviderId::new("anthropic");
        assert_eq!(id.to_string(), "anthropic");
    }

    #[test]
    fn model_id_new() {
        let id = ModelId::new("claude-sonnet-4-20250514");
        assert_eq!(id.to_string(), "claude-sonnet-4-20250514");
    }

    // ─── Send + Sync ────────────────────────────────────────────────

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn id_types_are_send_sync() {
        _assert_send_sync::<RunId>();
        _assert_send_sync::<EventId>();
        _assert_send_sync::<CorrelationId>();
        _assert_send_sync::<TraceId>();
        _assert_send_sync::<SpanId>();
        _assert_send_sync::<WorkflowId>();
        _assert_send_sync::<TenantId>();
        _assert_send_sync::<ProviderId>();
        _assert_send_sync::<ModelId>();
    }
}
