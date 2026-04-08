// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Raw workflow AST - directly parsed from YAML with spans.

use indexmap::IndexMap;

use super::mcp::RawMcpConfig;
use super::task::RawTask;
use crate::source::{Span, Spanned};

/// Raw workflow as parsed from YAML.
///
/// All fields preserve source positions via `Spanned<T>`.
/// This is Phase 1 of the Two-Phase IR - no validation, just structure.
#[derive(Debug, Clone, Default)]
pub struct RawWorkflow {
    /// Schema version: "nika/workflow@0.12"
    pub schema: Spanned<String>,

    /// Workflow identifier (optional in YAML, defaults to filename)
    pub workflow: Option<Spanned<String>>,

    /// Optional description
    pub description: Option<Spanned<String>>,

    /// Orchestration goal — when present, enables orchestrator mode.
    /// The orchestrator wraps all tasks into an agent loop that pursues this goal.
    pub goal: Option<Spanned<String>>,

    /// Default provider (e.g., "claude", "openai")
    pub provider: Option<Spanned<String>>,

    /// Default model (e.g., "claude-sonnet-4-6")
    pub model: Option<Spanned<String>>,

    /// MCP server configuration
    pub mcp: Option<Spanned<RawMcpConfig>>,

    /// Package includes: pkg: { include: [...] }
    pub pkg: Option<Spanned<RawPkgConfig>>,

    /// Context files configuration
    pub context: Option<Spanned<RawContextConfig>>,

    /// Include external workflows/modules
    ///
    /// ```yaml
    /// include:
    ///   - path: ./partials/setup.nika.yaml
    ///     prefix: setup_
    ///   - path: pkg:@nika/core@1.0/seo.nika.yaml
    /// ```
    pub include: Option<Spanned<Vec<Spanned<RawIncludeSpec>>>>,

    /// Input parameters with defaults
    pub inputs: Option<Spanned<IndexMap<Spanned<String>, Spanned<serde_json::Value>>>>,

    /// Artifact output configuration
    pub artifacts: Option<Spanned<serde_json::Value>>,

    /// Log configuration
    pub log: Option<Spanned<serde_json::Value>>,

    /// Reusable agent definitions
    pub agents: Option<Spanned<serde_json::Value>>,

    /// Workflow-level skills mapping (alias -> file path)
    ///
    /// Parsed from `skills:` block at workflow root. Maps skill aliases to
    /// file paths (local or `pkg:` URIs) for prompt augmentation.
    pub skills: Option<Spanned<indexmap::IndexMap<Spanned<String>, Spanned<String>>>>,

    /// Orchestrate configuration (goal-driven agent loop).
    pub orchestrate: Option<Spanned<serde_json::Value>>,

    /// Routing configuration (fallback chains, smart routing).
    pub routing: Option<Spanned<serde_json::Value>>,

    /// Schedule configuration for recurring execution.
    pub schedule: Option<Spanned<serde_json::Value>>,

    /// Global workflow timeout in seconds (optional, default: 3600s = 1h).
    pub max_duration_secs: Option<Spanned<crate::ast::templatable::Templatable<u64>>>,

    /// Task definitions (order matters for implicit flow)
    pub tasks: Spanned<Vec<Spanned<RawTask>>>,

    /// The span of the entire workflow document
    pub span: Span,
}

/// Raw package configuration.
#[derive(Debug, Clone, Default)]
pub struct RawPkgConfig {
    /// Included package specs: ["github:user/repo", "local:./path"]
    pub include: Vec<Spanned<String>>,
}

/// Raw context configuration for file loading.
#[derive(Debug, Clone, Default)]
pub struct RawContextConfig {
    /// File bindings: alias -> path
    pub files: Option<IndexMap<Spanned<String>, Spanned<String>>>,
}

/// Raw include specification.
///
/// DAG fusion: merge external workflow tasks into this workflow.
#[derive(Debug, Clone, Default)]
pub struct RawIncludeSpec {
    /// Path to the included workflow or skill file.
    /// Supports local paths and `pkg:` URIs.
    pub path: Spanned<String>,

    /// Optional task ID prefix for namespace isolation.
    /// When set, all included task IDs get this prefix.
    pub prefix: Option<Spanned<String>>,

    /// Span of the entire include spec
    pub span: Span,
}

impl RawWorkflow {
    /// Create a new empty raw workflow.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the workflow name, falling back to "unnamed".
    pub fn name(&self) -> &str {
        self.workflow
            .as_ref()
            .map(|s| s.value.as_str())
            .unwrap_or("unnamed")
    }

    /// Get the number of tasks.
    pub fn task_count(&self) -> usize {
        self.tasks.value.len()
    }

    /// Iterate over tasks.
    pub fn iter_tasks(&self) -> impl Iterator<Item = &Spanned<RawTask>> {
        self.tasks.value.iter()
    }

    /// Get a task by ID (linear search - Phase 2 uses interned IDs).
    pub fn get_task(&self, id: &str) -> Option<&Spanned<RawTask>> {
        self.tasks.value.iter().find(|t| t.value.id.value == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{FileId, Span};

    fn make_span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), start, end)
    }

    #[test]
    fn test_raw_workflow_default() {
        let workflow = RawWorkflow::default();
        assert_eq!(workflow.task_count(), 0);
        assert_eq!(workflow.name(), "unnamed");
    }

    #[test]
    fn test_raw_workflow_with_tasks() {
        use super::super::task::RawTask;

        let mut workflow = RawWorkflow::new();
        workflow.workflow = Some(Spanned::new("test-workflow".to_string(), make_span(0, 13)));
        workflow.tasks = Spanned::new(
            vec![
                Spanned::new(
                    RawTask {
                        id: Spanned::new("task1".to_string(), make_span(20, 25)),
                        ..Default::default()
                    },
                    make_span(18, 50),
                ),
                Spanned::new(
                    RawTask {
                        id: Spanned::new("task2".to_string(), make_span(60, 65)),
                        ..Default::default()
                    },
                    make_span(58, 90),
                ),
            ],
            make_span(15, 95),
        );

        assert_eq!(workflow.task_count(), 2);
        assert_eq!(workflow.name(), "test-workflow");
        assert!(workflow.get_task("task1").is_some());
        assert!(workflow.get_task("task2").is_some());
        assert!(workflow.get_task("task3").is_none());
    }

    #[test]
    fn test_raw_include_spec() {
        let spec = RawIncludeSpec {
            path: Spanned::new("./partials/setup.nika.yaml".to_string(), make_span(0, 25)),
            prefix: Some(Spanned::new("setup_".to_string(), make_span(30, 36))),
            span: make_span(0, 40),
        };

        assert_eq!(spec.path.value, "./partials/setup.nika.yaml");
        assert_eq!(spec.prefix.as_ref().unwrap().value, "setup_");
    }
}
