// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Include specification — the `include:` section at workflow root.

use serde::{Deserialize, Serialize};

/// An include directive that pulls in external workflow fragments.
///
/// Supports both local file paths and remote URLs (resolved at parse time
/// by the runtime, not by the schema crate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct IncludeSpec {
    /// Path or URL to the included fragment.
    pub path: String,
    /// Optional alias for namespacing included tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#as: Option<String>,
    /// Whether this include is optional (no error if missing).
    #[serde(default)]
    pub optional: bool,
}

impl IncludeSpec {
    /// Create a new include spec for a given path.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            r#as: None,
            optional: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_path() {
        let inc = IncludeSpec::new("./common.nika.yaml");
        assert_eq!(inc.path, "./common.nika.yaml");
        assert!(inc.r#as.is_none());
        assert!(!inc.optional);
    }

    #[test]
    fn serde_roundtrip() {
        let inc = IncludeSpec {
            path: "./lib/tools.nika.yaml".into(),
            r#as: Some("tools".into()),
            optional: true,
        };
        let json = serde_json::to_string(&inc).expect("serialize");
        let back: IncludeSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.path, "./lib/tools.nika.yaml");
        assert_eq!(back.r#as.as_deref(), Some("tools"));
        assert!(back.optional);
    }

    #[test]
    fn serde_minimal() {
        let json = r#"{"path": "common.yaml"}"#;
        let inc: IncludeSpec = serde_json::from_str(json).expect("deserialize");
        assert_eq!(inc.path, "common.yaml");
        assert!(!inc.optional);
    }
}
