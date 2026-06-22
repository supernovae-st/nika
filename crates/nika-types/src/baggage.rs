// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! W3C Baggage-compatible context propagation.
//!
//! Baggage carries cross-cutting metadata through the execution pipeline.
//! Bounded: max 64 entries, 8KB total.

use alloc::string::String;
use alloc::vec::Vec;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Maximum number of baggage entries.
pub const MAX_ENTRIES: usize = 64;
/// Maximum total size in bytes.
pub const MAX_SIZE_BYTES: usize = 8192;

/// A collection of baggage entries for context propagation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct Baggage {
    /// Baggage entries.
    pub entries: Vec<BaggageEntry>,
}

impl Baggage {
    /// Create empty baggage.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add an entry. Returns `false` if capacity limits exceeded.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) -> bool {
        if self.entries.len() >= MAX_ENTRIES {
            return false;
        }
        let entry = BaggageEntry {
            key: key.into(),
            value: value.into(),
            metadata: None,
        };
        let total_size: usize =
            self.entries.iter().map(BaggageEntry::size).sum::<usize>() + entry.size();
        if total_size > MAX_SIZE_BYTES {
            return false;
        }
        self.entries.push(entry);
        true
    }

    /// Get an entry by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&BaggageEntry> {
        self.entries.iter().find(|e| e.key == key)
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the baggage is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A single baggage entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct BaggageEntry {
    /// Entry key.
    pub key: String,
    /// Entry value.
    pub value: String,
    /// Optional metadata (e.g., properties from W3C Baggage spec).
    pub metadata: Option<String>,
}

impl BaggageEntry {
    /// Create a new baggage entry.
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            metadata: None,
        }
    }

    /// Approximate byte size.
    #[must_use]
    pub fn size(&self) -> usize {
        self.key.len() + self.value.len() + self.metadata.as_ref().map_or(0, String::len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_baggage() {
        let b = Baggage::new();
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn insert_and_get() {
        let mut b = Baggage::new();
        assert!(b.insert("tenant", "acme-corp"));
        assert_eq!(b.len(), 1);
        assert!(
            !b.is_empty(),
            "non-empty baggage must report is_empty() == false"
        );
        let entry = b.get("tenant").unwrap();
        assert_eq!(entry.value, "acme-corp");
    }

    #[test]
    fn insert_respects_max_entries() {
        let mut b = Baggage::new();
        for i in 0..MAX_ENTRIES {
            assert!(b.insert(format!("k{i}"), "v"));
        }
        assert!(!b.insert("overflow", "v"));
        assert_eq!(b.len(), MAX_ENTRIES);
    }

    #[test]
    fn insert_respects_max_size() {
        let mut b = Baggage::new();
        let big_value = "x".repeat(MAX_SIZE_BYTES);
        assert!(!b.insert("k", big_value));
        assert!(b.is_empty());
    }

    #[test]
    fn insert_accepts_exactly_max_size_then_rejects_over() {
        // Boundary: total_size == MAX_SIZE_BYTES is ACCEPTED (`>`, not `>=`).
        // key "k" (1) + value (MAX_SIZE_BYTES - 1) = exactly MAX_SIZE_BYTES.
        let mut b = Baggage::new();
        let exact = "x".repeat(MAX_SIZE_BYTES - 1);
        assert!(
            b.insert("k", exact),
            "total_size == MAX_SIZE_BYTES must be accepted (boundary)"
        );
        assert_eq!(b.len(), 1);
        // One byte over the cap is rejected.
        let mut b2 = Baggage::new();
        let over = "x".repeat(MAX_SIZE_BYTES); // key "k" pushes it 1 over
        assert!(
            !b2.insert("k", over),
            "one over MAX_SIZE_BYTES must be rejected"
        );
    }

    #[test]
    fn get_missing_returns_none() {
        let b = Baggage::new();
        assert!(b.get("missing").is_none());
    }

    #[test]
    fn baggage_entry_size() {
        let e = BaggageEntry::new("key", "value");
        assert_eq!(e.size(), 8); // "key" + "value"
    }

    #[test]
    fn baggage_entry_size_includes_metadata() {
        // Distinct lengths (2 + 3 + 4) so every `+` in `size()` must genuinely
        // ADD — a `+ → -` mutation on the metadata term (or the key/value term)
        // yields a wrong value or a usize underflow panic, killing the mutant.
        let mut e = BaggageEntry::new("ab", "cde"); // 2 + 3
        e.metadata = Some(String::from("fghi")); // + 4
        assert_eq!(e.size(), 9);
    }

    #[test]
    fn serde_roundtrip() {
        let mut b = Baggage::new();
        b.insert("tenant", "acme");
        let json = serde_json::to_string(&b).expect("serialize");
        let back: Baggage = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(b, back);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn baggage_types_are_send_sync() {
        _assert_send_sync::<Baggage>();
        _assert_send_sync::<BaggageEntry>();
    }
}
