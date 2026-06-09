// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `RawWorkflow` — the top-level AST node from YAML parsing.

use crate::source::Spanned;
use crate::types::SchemaVersion;

use super::task::RawTask;

/// A raw workflow — the direct output of YAML parsing.
///
/// All optional fields are `Option<Spanned<T>>` to preserve source spans.
/// Semantic validation happens in the analyzer, not here.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawWorkflow {
    /// Workflow schema version (e.g. `v1`).
    pub schema: Option<Spanned<SchemaVersion>>,
    /// Workflow name.
    pub name: Option<Spanned<String>>,
    /// Workflow description.
    pub description: Option<Spanned<String>>,
    /// Workflow goal (for agent-driven workflows).
    pub goal: Option<Spanned<String>>,
    /// Default provider for all tasks.
    pub provider: Option<Spanned<String>>,
    /// Default model for all tasks.
    pub model: Option<Spanned<String>>,
    /// The task list.
    pub tasks: Vec<Spanned<RawTask>>,
}

impl RawWorkflow {
    /// Create a new empty raw workflow.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema: None,
            name: None,
            description: None,
            goal: None,
            provider: None,
            model: None,
            tasks: Vec::new(),
        }
    }
}

impl Default for RawWorkflow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let w = RawWorkflow::new();
        assert!(w.schema.is_none());
        assert!(w.name.is_none());
        assert!(w.tasks.is_empty());
    }
}
