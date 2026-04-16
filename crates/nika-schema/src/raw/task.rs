// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `RawTask` — a single task in the raw workflow AST.

use crate::guardrails::GuardrailConfig;
use crate::source::Spanned;
use crate::types::{BudgetConfig, CompletionConfig, DecomposeSpec, LimitsConfig, RecordSpec};

use super::action::RawAction;

/// A raw task — a single step in the workflow DAG.
///
/// Contains the task's action (verb), dependencies, and configuration.
/// Semantic validation (e.g., cycle detection) happens in the analyzer.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawTask {
    /// Task name (must be unique, `snake_case`).
    pub name: Spanned<String>,
    /// The task action (which verb to execute).
    pub action: RawAction,
    /// Dependencies: task names this task depends on.
    pub depends_on: Vec<Spanned<String>>,
    /// Condition expression (template, evaluated at runtime).
    pub condition: Option<Spanned<String>>,
    /// For-each iteration source (template expression).
    pub for_each: Option<Spanned<String>>,
    /// Record specification (how to store output).
    pub record: Option<Spanned<RecordSpec>>,
    /// Decomposition specification.
    pub decompose: Option<Spanned<DecomposeSpec>>,
    /// Budget configuration for this task.
    pub budget: Option<Spanned<BudgetConfig>>,
    /// Limits configuration (agent tasks).
    pub limits: Option<Spanned<LimitsConfig>>,
    /// Completion configuration (agent tasks).
    pub completion: Option<Spanned<CompletionConfig>>,
    /// Guardrails for output validation.
    pub guardrails: Vec<GuardrailConfig>,
    /// Maximum retry count on failure.
    pub max_retries: Option<Spanned<u32>>,
}

impl RawTask {
    /// Create a new raw task with the given name and action.
    #[must_use]
    pub fn new(name: Spanned<String>, action: RawAction) -> Self {
        Self {
            name,
            action,
            depends_on: Vec::new(),
            condition: None,
            for_each: None,
            record: None,
            decompose: None,
            budget: None,
            limits: None,
            completion: None,
            guardrails: Vec::new(),
            max_retries: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Span;

    #[test]
    fn new_has_empty_deps() {
        let name = Spanned {
            value: "my_task".into(),
            span: Span::default(),
        };
        let action = RawAction::Exec(super::super::action::RawExecAction::new(Spanned {
            value: "echo hello".into(),
            span: Span::default(),
        }));
        let task = RawTask::new(name, action);
        assert_eq!(task.name.value, "my_task");
        assert!(task.depends_on.is_empty());
        assert!(task.guardrails.is_empty());
        assert!(task.condition.is_none());
    }
}
