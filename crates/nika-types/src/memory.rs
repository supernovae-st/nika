// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Memory types — pure L0 types shared by kernel memory traits and providers.
//!
//! Contains identifiers, level hierarchy, directives, and frame references.
//! Traits and storage types stay in `nika-kernel::memory`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── MemoryId ───────────────────────────────────────────────────────

/// Opaque memory identifier. `UUIDv7`-backed (time-sortable, RFC 9562).
///
/// Displayed as `"mem-{uuid}"` for wire stability. Wave 3 migrated the
/// underlying representation from `u128` to `uuid::Uuid` per ADR-033
/// follow-up #1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct MemoryId {
    /// Underlying UUID (`UUIDv7` in production, nil / arbitrary in tests).
    uuid: Uuid,
}

impl MemoryId {
    /// Create a new memory identifier wrapping an explicit UUID.
    ///
    /// Prefer [`Self::generate`] in production (`UUIDv7`, time-sortable);
    /// pass a specific UUID here when test determinism is needed.
    #[must_use]
    pub fn new(uuid: Uuid) -> Self {
        Self { uuid }
    }

    /// Generate a fresh `UUIDv7`-backed memory identifier.
    ///
    /// `UUIDv7` embeds a millisecond timestamp so IDs generated in order
    /// sort chronologically — useful for memory recall / pagination.
    #[must_use]
    pub fn generate() -> Self {
        Self {
            uuid: Uuid::now_v7(),
        }
    }

    /// Returns the underlying UUID.
    #[must_use]
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// Create a nil (all-zero) memory identifier.
    ///
    /// Used as the default in tests / mocks where the specific ID is
    /// not under test.
    #[must_use]
    pub fn nil() -> Self {
        Self { uuid: Uuid::nil() }
    }
}

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

impl core::fmt::Display for MemoryId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "mem-{}", self.uuid)
    }
}

impl Serialize for MemoryId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for MemoryId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let hex = s
            .strip_prefix("mem-")
            .ok_or_else(|| serde::de::Error::custom("expected 'mem-' prefix"))?;
        let uuid = Uuid::parse_str(hex)
            .map_err(|e| serde::de::Error::custom(format!("invalid uuid: {e}")))?;
        Ok(Self { uuid })
    }
}

// ─── MemoryLevel ────────────────────────────────────────────────────

/// Memory level in the cognitive hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryLevel {
    /// Short-term working memory (current task context).
    Working,
    /// Episode-specific memories (what happened).
    Episodic,
    /// General knowledge (facts, concepts).
    Semantic,
    /// How-to knowledge (procedures, recipes).
    Procedural,
    /// Meta-cognitive observations (patterns, self-reflection).
    Reflective,
    /// Abstract concept relationships.
    Conceptual,
}

// ─── MemoryDirective ────────────────────────────────────────────────

/// Directive for memory behavior during inference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryDirective {
    /// Let the system decide (default).
    #[default]
    Auto,
    /// Only recall, don't store.
    RecallOnly,
    /// Only store, don't recall.
    RememberOnly,
    /// Explicit recall and remember lists.
    Explicit {
        /// Topics to recall.
        recall: Vec<String>,
        /// Topics to remember.
        remember: Vec<String>,
    },
    /// Disable memory entirely for this request.
    Disabled,
}

// ─── MemoryFrameRef ─────────────────────────────────────────────────

/// Lightweight memory reference for `InferResponse`.
///
/// The `trust` field carries sticky taint from ingest — a frame
/// constructed from untrusted tool output never upgrades to
/// `TrustLevel::Trusted` on recall, even if the recall caller is
/// trusted. Consumers in the v0.95 Shield path read `trust` to
/// decide whether recalled frames are eligible to flow back into a
/// privileged prompt slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct MemoryFrameRef {
    /// Memory identifier.
    pub id: MemoryId,
    /// Cognitive level.
    pub level: MemoryLevel,
    /// Brief summary.
    pub summary: String,
    /// Sticky taint from ingest — never upgrades on recall.
    /// Reserved at v0.81 for v0.95 Shield integration (ADR-030).
    #[serde(default = "default_trust_untrusted")]
    pub trust: crate::trust::TrustLevel,
}

/// Default for `MemoryFrameRef::trust` when the field is absent on the
/// wire: conservatively treat legacy/unknown-origin frames as untrusted.
/// This keeps new-consumer-old-producer and old-consumer-new-producer
/// serde compat strictly fail-safe.
fn default_trust_untrusted() -> crate::trust::TrustLevel {
    crate::trust::TrustLevel::UNTRUSTED
}

impl MemoryFrameRef {
    /// Create a new memory frame reference (defaults to `TrustLevel::Untrusted`).
    #[must_use]
    pub fn new(id: MemoryId, level: MemoryLevel, summary: impl Into<String>) -> Self {
        Self {
            id,
            level,
            summary: summary.into(),
            trust: crate::trust::TrustLevel::UNTRUSTED,
        }
    }

    /// Create a new frame reference with explicit trust taint.
    #[must_use]
    pub fn with_trust(
        id: MemoryId,
        level: MemoryLevel,
        summary: impl Into<String>,
        trust: crate::trust::TrustLevel,
    ) -> Self {
        Self {
            id,
            level,
            summary: summary.into(),
            trust,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── MemoryId tests ─────────────────────────────────────────────

    #[test]
    fn memory_id_display_uses_uuid_hyphenated() {
        let id = MemoryId::new(Uuid::from_u128(0xFF));
        assert_eq!(id.to_string(), "mem-00000000-0000-0000-0000-0000000000ff");
    }

    #[test]
    fn memory_id_nil() {
        let id = MemoryId::nil();
        assert_eq!(id.uuid, Uuid::nil());
        assert_eq!(id.to_string(), "mem-00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn memory_id_generate_is_v7_and_non_nil() {
        let id = MemoryId::generate();
        assert_ne!(id.uuid, Uuid::nil());
        assert_eq!(id.uuid.get_version_num(), 7);
    }

    #[test]
    fn memory_id_generate_is_unique() {
        let a = MemoryId::generate();
        let b = MemoryId::generate();
        assert_ne!(a, b);
    }

    #[test]
    fn memory_id_serde_roundtrip() {
        let id = MemoryId::new(Uuid::from_u128(42));
        let json = serde_json::to_string(&id).expect("serialize");
        let back: MemoryId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn memory_id_serde_roundtrip_v7() {
        let id = MemoryId::generate();
        let json = serde_json::to_string(&id).expect("serialize");
        let back: MemoryId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn memory_id_deserialize_missing_prefix() {
        let json = "\"00000000-0000-0000-0000-000000000042\"";
        let err = serde_json::from_str::<MemoryId>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("mem-"),
            "error should mention expected 'mem-' prefix, got: {msg}",
        );
    }

    #[test]
    fn memory_id_deserialize_invalid_uuid_after_prefix() {
        let json = "\"mem-not-a-valid-uuid\"";
        let err = serde_json::from_str::<MemoryId>(json).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("invalid uuid") || msg.contains("invalid character"),
            "error should mention invalid uuid, got: {msg}",
        );
    }

    // ─── MemoryLevel tests ──────────────────────────────────────────

    #[test]
    fn memory_level_serde_roundtrip() {
        let level = MemoryLevel::Episodic;
        let json = serde_json::to_string(&level).expect("serialize");
        assert_eq!(json, "\"episodic\"");
        let back: MemoryLevel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, level);
    }

    // ─── MemoryDirective tests ──────────────────────────────────────

    #[test]
    fn memory_directive_default_is_auto() {
        let directive = MemoryDirective::default();
        assert!(matches!(directive, MemoryDirective::Auto));
    }

    // ─── MemoryFrameRef tests ───────────────────────────────────────

    #[test]
    fn memory_frame_ref_new() {
        let r = MemoryFrameRef::new(
            MemoryId::new(Uuid::from_u128(1)),
            MemoryLevel::Working,
            "summary",
        );
        assert_eq!(r.summary, "summary");
        assert_eq!(r.trust, crate::trust::TrustLevel::UNTRUSTED);
    }

    #[test]
    fn memory_frame_ref_with_trust() {
        let r = MemoryFrameRef::with_trust(
            MemoryId::new(Uuid::from_u128(2)),
            MemoryLevel::Semantic,
            "trusted frame",
            crate::trust::TrustLevel::TRUSTED,
        );
        assert_eq!(r.trust, crate::trust::TrustLevel::TRUSTED);
    }

    #[test]
    fn memory_frame_ref_trust_defaults_untrusted_on_missing_wire_field() {
        // Wire payload emitted by an older producer that doesn't know trust.
        let json = r#"{"id":"mem-00000000-0000-0000-0000-00000000002a","level":"working","summary":"legacy"}"#;
        let r: MemoryFrameRef = serde_json::from_str(json).expect("deserialize legacy");
        // Missing `trust` MUST default to Untrusted — fail-safe taint policy.
        assert_eq!(r.trust, crate::trust::TrustLevel::UNTRUSTED);
    }

    #[test]
    fn memory_frame_ref_trust_roundtrip_preserves_value() {
        let original = MemoryFrameRef::with_trust(
            MemoryId::new(Uuid::from_u128(3)),
            MemoryLevel::Reflective,
            "sem",
            crate::trust::TrustLevel::TRUSTED,
        );
        let json = serde_json::to_string(&original).expect("ser");
        let back: MemoryFrameRef = serde_json::from_str(&json).expect("de");
        assert_eq!(back.trust, crate::trust::TrustLevel::TRUSTED);
    }

    // ─── Send + Sync ────────────────────────────────────────────────

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn memory_types_send_sync() {
        _assert_send_sync::<MemoryId>();
        _assert_send_sync::<MemoryFrameRef>();
    }
}
