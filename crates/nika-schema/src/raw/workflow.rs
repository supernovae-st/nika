// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `RawWorkflow` — the top-level AST node from YAML parsing.

use crate::source::Spanned;
use crate::types::{
    ContextConfig, IncludeSpec, LogConfig, OrchestrateConfig, RoutingConfig, ScheduleConfig,
    SchemaVersion,
};

use super::mcp::RawMcpConfig;
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
    /// MCP server configuration.
    pub mcp: Option<Spanned<RawMcpConfig>>,
    /// Context configuration.
    pub context: Option<Spanned<ContextConfig>>,
    /// Include directives.
    pub include: Vec<Spanned<IncludeSpec>>,
    /// Input variable definitions.
    pub inputs: Vec<(Spanned<String>, Spanned<serde_json::Value>)>,
    /// Logging configuration.
    pub logging: Option<Spanned<LogConfig>>,
    /// Orchestration configuration.
    pub orchestrate: Option<Spanned<OrchestrateConfig>>,
    /// Routing configuration.
    pub routing: Option<Spanned<RoutingConfig>>,
    /// Schedule configuration.
    pub schedule: Option<Spanned<ScheduleConfig>>,
    /// Maximum workflow duration in seconds.
    pub max_duration_secs: Option<Spanned<u64>>,
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
            mcp: None,
            context: None,
            include: Vec::new(),
            inputs: Vec::new(),
            logging: None,
            orchestrate: None,
            routing: None,
            schedule: None,
            max_duration_secs: None,
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
        assert!(w.include.is_empty());
        assert!(w.inputs.is_empty());
    }
}
