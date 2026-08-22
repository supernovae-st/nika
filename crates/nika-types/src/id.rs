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

#[cfg(feature = "serde")]
use alloc::format;
use alloc::string::String;
#[cfg(feature = "serde")]
use alloc::string::ToString;
use core::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── UUID-based IDs (`UUIDv7`, time-ordered) ──────────────────────────

/// Run identifier (`UUIDv7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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

/// Identifier for one admitted execution (`UUIDv7`).
///
/// An execution is distinct from an interface request, durable job, workflow
/// run, and trace. Its UUID bytes deterministically seed the execution's root
/// [`TraceId`], keeping that relationship direct and typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct ExecutionId {
    /// The underlying UUID.
    pub uuid: Uuid,
}

impl ExecutionId {
    /// Create an execution ID from a UUID.
    #[must_use]
    pub fn new(uuid: Uuid) -> Self {
        Self { uuid }
    }

    /// Generate a new time-ordered execution ID.
    #[must_use]
    pub fn generate() -> Self {
        Self {
            uuid: Uuid::now_v7(),
        }
    }

    /// Create an execution ID from raw UUID bytes.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self {
            uuid: Uuid::from_bytes(bytes),
        }
    }

    /// Nil (zero) execution ID.
    #[must_use]
    pub fn nil() -> Self {
        Self { uuid: Uuid::nil() }
    }
}

impl fmt::Display for ExecutionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exe-{}", self.uuid)
    }
}

/// Event identifier (`UUIDv7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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

impl From<ExecutionId> for TraceId {
    fn from(execution: ExecutionId) -> Self {
        Self::new(*execution.uuid.as_bytes())
    }
}

#[cfg(feature = "serde")]
impl Serialize for TraceId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for TraceId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        if s.len() != 32 {
            return Err(serde::de::Error::custom("trace ID must be 32 hex chars"));
        }
        let mut bytes = [0u8; 16];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hex = core::str::from_utf8(chunk)
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

#[cfg(feature = "serde")]
impl Serialize for SpanId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for SpanId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        if s.len() != 16 {
            return Err(serde::de::Error::custom("span ID must be 16 hex chars"));
        }
        let mut bytes = [0u8; 8];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hex = core::str::from_utf8(chunk)
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
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
        #[non_exhaustive]
        pub struct $name {
            /// The identifier value (private — use `new()` to construct,
            /// `as_str()` to read). Privacy enables future validation in
            /// `new()` without breaking callers (e.g. lowercase kebab-case
            /// for `ProviderId`). Per Audit-1 P2-10 (2026-04-16).
            value: String,
        }

        impl $name {
            /// Create a new identifier.
            #[must_use]
            pub fn new(value: impl Into<String>) -> Self {
                Self {
                    value: value.into(),
                }
            }

            /// Borrow the inner string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.value
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

impl TenantId {
    /// The conventional default tenant identifier — `"default"`.
    ///
    /// Single-tenant deployments and pre-v0.95 code paths use this
    /// value as the canonical keyspace. Multi-tenant hosts MUST override
    /// this explicitly per request. Reserved for ADR-031.
    pub const DEFAULT_VALUE: &'static str = "default";

    /// Construct the conventional default tenant.
    #[must_use]
    pub fn default_tenant() -> Self {
        Self::new(Self::DEFAULT_VALUE)
    }
}

impl Default for TenantId {
    fn default() -> Self {
        Self::default_tenant()
    }
}

string_id!(
    /// Provider identifier (e.g., `"anthropic"`, `"openai"`).
    ProviderId
);

string_id!(
    /// Model identifier (e.g., `"claude-sonnet-4-20250514"`).
    ModelId
);

// ─── TaskId (user-named, not machine-generated) ────────────────────

string_id!(
    /// Opaque task identifier (user-named in YAML workflows).
    ///
    /// Unlike UUID-based IDs, `TaskId` comes from the workflow definition
    /// (e.g., `research_step`, `summarize`). It is user-chosen, not generated.
    ///
    /// Inner field is private (Audit-1 P0-2, 2026-04-16) so future
    /// validation can be added in `new()` without breaking callers.
    TaskId
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

    // ─── ExecutionId ──────────────────────────────────

    #[test]
    fn execution_id_generate_is_unique() {
        let a = ExecutionId::generate();
        let b = ExecutionId::generate();
        assert_ne!(a, b);
    }

    #[test]
    fn execution_id_is_the_trace_identity() {
        let bytes = [7u8; 16];
        let execution = ExecutionId::from_bytes(bytes);
        let trace = TraceId::from(execution);
        assert_eq!(trace.bytes, bytes);
    }

    #[test]
    fn execution_id_display_and_serde_are_stable() {
        let id = ExecutionId::nil();
        assert!(id.to_string().starts_with("exe-"));
        let json = serde_json::to_string(&id).expect("serialize");
        let back: ExecutionId = serde_json::from_str(&json).expect("deserialize");
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
    fn tenant_default_is_the_named_default_not_struct_default() {
        // `default_tenant()` must return the conventional "default" tenant, not
        // whatever `Default::default()` yields (kills the `-> Default::default()`
        // mutant, which would silently swap the value).
        assert_eq!(TenantId::default_tenant(), TenantId::new("default"));
        assert_eq!(TenantId::default_tenant().value, TenantId::DEFAULT_VALUE);
    }

    #[test]
    fn trace_and_span_is_nil_false_for_non_nil() {
        // is_nil() must be FALSE for a non-zero id (kills `is_nil -> true`).
        let t = TraceId::new([1u8; 16]);
        assert!(!t.is_nil());
        assert!(TraceId::new([0u8; 16]).is_nil());
        let s = SpanId::new([1u8; 8]);
        assert!(!s.is_nil());
        assert!(SpanId::new([0u8; 8]).is_nil());
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

    // ─── TaskId ──────────────────────────────────────────────────────

    #[test]
    fn task_id_display() {
        let id = TaskId::new("research_step");
        assert_eq!(id.to_string(), "research_step");
    }

    #[test]
    fn task_id_equality() {
        let a = TaskId::new("t1");
        let b = TaskId::new("t1");
        assert_eq!(a, b);
    }

    #[test]
    fn task_id_serde_roundtrip() {
        let id = TaskId::new("abc-123");
        let json = serde_json::to_string(&id).expect("serialize");
        let back: TaskId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    // ─── Send + Sync ────────────────────────────────────────────────

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn id_types_are_send_sync() {
        _assert_send_sync::<RunId>();
        _assert_send_sync::<ExecutionId>();
        _assert_send_sync::<EventId>();
        _assert_send_sync::<CorrelationId>();
        _assert_send_sync::<TraceId>();
        _assert_send_sync::<SpanId>();
        _assert_send_sync::<WorkflowId>();
        _assert_send_sync::<TenantId>();
        _assert_send_sync::<ProviderId>();
        _assert_send_sync::<ModelId>();
        _assert_send_sync::<TaskId>();
    }

    // ─── Proptest: serde JSON roundtrip invariants ──────────────────

    proptest::proptest! {
        /// Every valid UTF-8 non-empty string roundtrips through TenantId
        /// via serde JSON.
        #[test]
        fn tenant_id_serde_json_roundtrip(s in "[a-zA-Z0-9_\\-]{1,64}") {
            let id = TenantId::new(s.clone());
            let json = serde_json::to_string(&id).expect("ser");
            let back: TenantId = serde_json::from_str(&json).expect("de");
            proptest::prop_assert_eq!(back.as_str(), s.as_str());
        }

        /// Same roundtrip invariant for ProviderId.
        #[test]
        fn provider_id_serde_json_roundtrip(s in "[a-z0-9\\-]{1,32}") {
            let id = ProviderId::new(s.clone());
            let json = serde_json::to_string(&id).expect("ser");
            let back: ProviderId = serde_json::from_str(&json).expect("de");
            proptest::prop_assert_eq!(back.as_str(), s.as_str());
        }

        /// Same roundtrip invariant for ModelId.
        #[test]
        fn model_id_serde_json_roundtrip(s in "[a-zA-Z0-9._\\-]{1,64}") {
            let id = ModelId::new(s.clone());
            let json = serde_json::to_string(&id).expect("ser");
            let back: ModelId = serde_json::from_str(&json).expect("de");
            proptest::prop_assert_eq!(back.as_str(), s.as_str());
        }

        /// TaskId roundtrip for workflow-named task ids.
        #[test]
        fn task_id_serde_json_roundtrip(s in "[a-zA-Z_][a-zA-Z0-9_\\-]{0,63}") {
            let id = TaskId::new(s.clone());
            let json = serde_json::to_string(&id).expect("ser");
            let back: TaskId = serde_json::from_str(&json).expect("de");
            proptest::prop_assert_eq!(back.as_str(), s.as_str());
        }
    }

    // Binary-ID proptest: TraceId (W3C 16-byte) + SpanId (W3C 8-byte)
    // must roundtrip through serde JSON for every random byte array.
    proptest::proptest! {
        #[test]
        fn trace_id_random_bytes_roundtrip(
            bytes in proptest::array::uniform16(proptest::num::u8::ANY),
        ) {
            let id = TraceId::new(bytes);
            let json = serde_json::to_string(&id).expect("ser");
            let back: TraceId = serde_json::from_str(&json).expect("de");
            proptest::prop_assert_eq!(back, id);
        }

        #[test]
        fn span_id_random_bytes_roundtrip(
            bytes in proptest::array::uniform8(proptest::num::u8::ANY),
        ) {
            let id = SpanId::new(bytes);
            let json = serde_json::to_string(&id).expect("ser");
            let back: SpanId = serde_json::from_str(&json).expect("de");
            proptest::prop_assert_eq!(back, id);
        }
    }
}
