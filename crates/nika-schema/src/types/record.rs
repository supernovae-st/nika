// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Record specification — the `record:` section on tasks.

use serde::{Deserialize, Serialize};

/// Record specification controlling how task output is captured.
///
/// By default, task output is stored as the task's result variable.
/// `RecordSpec` allows customizing what is recorded and how.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RecordSpec {
    /// Variable name to store the output in (defaults to task name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#as: Option<String>,
    /// Whether to append to existing variable instead of overwriting.
    #[serde(default)]
    pub append: bool,
    /// `JMESPath` expression to extract from the output before recording.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<String>,
}

impl RecordSpec {
    /// Create a new empty record spec.
    #[must_use]
    pub fn new() -> Self {
        Self {
            r#as: None,
            append: false,
            extract: None,
        }
    }
}

impl Default for RecordSpec {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let r = RecordSpec::new();
        assert!(r.r#as.is_none());
        assert!(!r.append);
        assert!(r.extract.is_none());
    }

    #[test]
    fn serde_roundtrip() {
        let r = RecordSpec {
            r#as: Some("result_data".into()),
            append: true,
            extract: Some("data.items[0]".into()),
        };
        let json = serde_json::to_string(&r).expect("serialize");
        let back: RecordSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.r#as.as_deref(), Some("result_data"));
        assert!(back.append);
        assert_eq!(back.extract.as_deref(), Some("data.items[0]"));
    }
}
