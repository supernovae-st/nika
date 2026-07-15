// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `RawWorkflow` — the top-level AST node from YAML parsing.
//!
//! Canonical v1 envelope per spec `01-envelope.md` ·
//!
//! ```yaml
//! nika: v1                # required · language + contract version
//! workflow: my-id         # required · kebab-case
//! description: "…"        # optional
//! model: provider/name    # optional · workflow-level default
//! vars: { … }             # optional · untyped OR typed inputs
//! env: { … }              # optional · non-sensitive runtime config
//! secrets: { … }          # optional · vault-backed references
//! tasks: [ … ]            # required · non-empty (analyzer-enforced)
//! outputs: { … }          # optional · the workflow's return contract
//! ```

use crate::source::Spanned;
use crate::types::{
    AssertProperty, OutputDecl, Permits, Policy, SchemaVersion, SecretRef, VarDecl,
};

use super::task::RawTask;

/// A raw workflow — the direct output of YAML parsing.
///
/// `nika:` / `workflow:` stay `Option` here so the analyzer can report
/// their absence as a collected error (the parser is shape-only).
/// Mapping blocks are ordered `Vec<(key, value)>` pairs — YAML duplicate
/// keys are rejected at load time, so order is the only thing to keep.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawWorkflow {
    /// `nika:` — the language contract version (exactly `v1`).
    pub nika: Option<Spanned<SchemaVersion>>,
    /// `workflow:` — kebab-case workflow id.
    pub workflow: Option<Spanned<String>>,
    /// `description:` — free-form human text.
    pub description: Option<Spanned<String>>,
    /// `model:` — workflow-level default `<provider>/<name>`.
    pub model: Option<Spanned<String>>,
    /// `vars:` — workflow inputs (untyped OR typed).
    pub vars: Vec<(Spanned<String>, VarDecl)>,
    /// `env:` — non-sensitive runtime config (`${{ env.X }}`).
    pub env: Vec<(Spanned<String>, Spanned<String>)>,
    /// `secrets:` — masked store references (`${{ secrets.X }}`).
    pub secrets: Vec<(Spanned<String>, Spanned<SecretRef>)>,
    /// `permits:` — the declared capability boundary (spec 01 §permits ·
    /// `None` = absent = today's behavior · `Some` = default-deny).
    pub permits: Option<Spanned<Permits>>,
    /// `policy:` — named workflow law (spec 10 · hard families judged
    /// at check · soft recorded, never judged).
    pub policy: Option<Spanned<Policy>>,
    /// `types:` — named type declarations (spec 09 · `PascalCase` name →
    /// RAW type expression · parsed/validated by the type core at
    /// check time, never here — the parser is shape-only).
    pub types: Vec<(Spanned<String>, Spanned<serde_json::Value>)>,
    /// `tasks:` — the DAG.
    pub tasks: Vec<Spanned<RawTask>>,
    /// `outputs:` — the workflow's return contract.
    pub outputs: Vec<(Spanned<String>, OutputDecl)>,
    /// `assert:` — the author's obligations (spec 15 §assert · the closed
    /// vocabulary the engine judges at an honest level · malformed refused
    /// `NIKA-ASSERT-001`).
    pub assert: Vec<Spanned<AssertProperty>>,
}

impl RawWorkflow {
    /// Create a new empty raw workflow.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nika: None,
            workflow: None,
            description: None,
            model: None,
            vars: Vec::new(),
            env: Vec::new(),
            secrets: Vec::new(),
            permits: None,
            policy: None,
            types: Vec::new(),
            tasks: Vec::new(),
            outputs: Vec::new(),
            assert: Vec::new(),
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
        assert!(w.nika.is_none());
        assert!(w.workflow.is_none());
        assert!(w.policy.is_none());
        assert!(w.tasks.is_empty());
        assert!(w.vars.is_empty());
        assert!(w.env.is_empty());
        assert!(w.secrets.is_empty());
        assert!(w.types.is_empty());
        assert!(w.outputs.is_empty());
    }
}
