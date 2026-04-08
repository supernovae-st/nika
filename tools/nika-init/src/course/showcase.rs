// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared types for showcase workflows.
//!
//! Both `showcase_builtin` and `showcase_llm` use this shared type.

/// A showcase workflow with metadata for generation and filtering.
#[derive(Debug, Clone)]
pub struct ShowcaseWorkflow {
    /// Workflow name (used as filename: `{name}.nika.yaml`)
    pub name: &'static str,
    /// One-line description of what the workflow does
    pub description: &'static str,
    /// Category for grouping (e.g., "core", "file", "media", "content")
    pub category: &'static str,
    /// Complete YAML content (with `{{PROVIDER}}`/`{{MODEL}}` placeholders)
    pub content: &'static str,
    /// Whether this workflow requires an LLM provider
    pub requires_llm: bool,
}
