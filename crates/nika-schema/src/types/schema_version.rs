// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow schema version — the `schema:` field at workflow root.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Workflow schema version.
///
/// Currently only `v1` is supported. The enum is `#[non_exhaustive]`
/// to allow future versions without a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum SchemaVersion {
    /// Version 1 — the current and only supported version.
    #[default]
    V1,
}

impl SchemaVersion {
    /// Create a new schema version (defaults to V1).
    #[must_use]
    pub fn new() -> Self {
        Self::V1
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V1 => write!(f, "v1"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_v1() {
        assert_eq!(SchemaVersion::default(), SchemaVersion::V1);
    }

    #[test]
    fn new_is_v1() {
        assert_eq!(SchemaVersion::new(), SchemaVersion::V1);
    }

    #[test]
    fn display() {
        assert_eq!(SchemaVersion::V1.to_string(), "v1");
    }

    #[test]
    fn serde_roundtrip() {
        let v = SchemaVersion::V1;
        let json = serde_json::to_string(&v).expect("serialize");
        assert_eq!(json, "\"v1\"");
        let back: SchemaVersion = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, v);
    }
}
