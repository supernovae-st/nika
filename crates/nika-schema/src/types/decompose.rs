// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Decomposition configuration — the `decompose:` section on tasks.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Strategy for decomposing a task into subtasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum DecomposeStrategy {
    /// Let the LLM decide how to decompose.
    #[default]
    Auto,
    /// Decompose into numbered steps.
    Sequential,
    /// Decompose into independent parallel subtasks.
    Parallel,
}

impl fmt::Display for DecomposeStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Sequential => write!(f, "sequential"),
            Self::Parallel => write!(f, "parallel"),
        }
    }
}

/// Decomposition specification for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DecomposeSpec {
    /// Decomposition strategy.
    #[serde(default)]
    pub strategy: DecomposeStrategy,
    /// Maximum number of subtasks to create.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_subtasks: Option<u32>,
}

impl DecomposeSpec {
    /// Create a new decompose spec with default strategy.
    #[must_use]
    pub fn new() -> Self {
        Self {
            strategy: DecomposeStrategy::default(),
            max_subtasks: None,
        }
    }
}

impl Default for DecomposeSpec {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_display() {
        assert_eq!(DecomposeStrategy::Auto.to_string(), "auto");
        assert_eq!(DecomposeStrategy::Sequential.to_string(), "sequential");
        assert_eq!(DecomposeStrategy::Parallel.to_string(), "parallel");
    }

    #[test]
    fn strategy_serde_roundtrip() {
        for strategy in [
            DecomposeStrategy::Auto,
            DecomposeStrategy::Sequential,
            DecomposeStrategy::Parallel,
        ] {
            let json = serde_json::to_string(&strategy).expect("serialize");
            let back: DecomposeStrategy = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, strategy);
        }
    }

    #[test]
    fn spec_serde_roundtrip() {
        let spec = DecomposeSpec {
            strategy: DecomposeStrategy::Parallel,
            max_subtasks: Some(5),
        };
        let json = serde_json::to_string(&spec).expect("serialize");
        let back: DecomposeSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.strategy, DecomposeStrategy::Parallel);
        assert_eq!(back.max_subtasks, Some(5));
    }
}
