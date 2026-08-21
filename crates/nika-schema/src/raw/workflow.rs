// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `RawWorkflow` — the top-level AST node from YAML parsing.
//!
//! Canonical nine-key envelope per spec `01-envelope.md` (identity on
//! `nika:` · three value authorities · `permits:` · `run:`):
//!
//! ```yaml
//! # optional prose — never a `description:` key
//! nika: my-id             # required · the mark AND the kebab-case name
//! model: provider/name    # optional · workflow-level default
//! inputs: { … }           # optional · typed caller-supplied inputs
//! const: { … }            # optional · named constants
//! secrets: { … }          # optional · vault-backed references
//! permits: { … }          # optional · declared capability boundary
//! run: { … }              # optional · entropy + clock declaration (F-P3)
//! tasks: { … }            # required · non-empty map keyed by task id
//! outputs: { … }          # optional · the workflow's return contract
//! ```
//!
//! The pre-C2 `vars:`/`env:` fields are DEAD (the E-split · they refuse
//! `NIKA-VALUES-001`/`NIKA-VALUES-002` at parse). `workflow:` /
//! `description:` / `config:` / `types:` / `policy:` died with the
//! nine-key envelope.

use crate::source::Spanned;
use crate::types::{OutputDecl, Permits, RunDecl, SecretRef, VarDecl};

use super::task::RawTask;

/// A raw workflow — the direct output of YAML parsing.
///
/// `nika:` stays `Option` here so the analyzer can report its absence
/// as a collected error (the parser is shape-only). Identity is only
/// that key — the value is stored on `workflow` (the file's NAME);
/// there is no `workflow:` envelope key. Mapping blocks are ordered
/// `Vec<(key, value)>` pairs — YAML duplicate keys are rejected at
/// load time, so order is the only thing to keep.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawWorkflow {
    /// The file's NAME, kebab-case — parsed from `nika:` (spec 01
    /// §`nika`). The KEY is the mark (« this is a Nika file »), the
    /// VALUE is the id. The key held the literal `v1` until the envelope
    /// nuke (2026-08-12): the version slot is gone forever and that is
    /// lossless — `v1` was the only legal value for the contract's whole
    /// lifetime and there is no `nika: v2`, ever.
    pub workflow: Option<Spanned<String>>,
    /// `model:` — workflow-level default `<provider>/<name>`.
    pub model: Option<Spanned<String>>,
    /// `inputs:` — typed workflow inputs (caller-supplied · spec 01 §inputs).
    pub inputs: Vec<(Spanned<String>, VarDecl)>,
    /// `const:` — named constants (spec 01 §const · bare literal →
    /// `VarDecl::Untyped` · `{type, value}` → `VarDecl::Typed` (value in `default`).
    pub consts: Vec<(Spanned<String>, VarDecl)>,
    /// `secrets:` — masked store references (`${{ secrets.X }}`).
    pub secrets: Vec<(Spanned<String>, Spanned<SecretRef>)>,
    /// `permits:` — the declared capability boundary (spec 01 §permits ·
    /// `None` = absent = ZERO authority (F-O8 · every effect refused) ·
    /// `Some` = default-deny).
    pub permits: Option<Spanned<Permits>>,
    /// `run:` — the run's entropy + clock declaration (F-P3 · `None` =
    /// absent = the status quo: `entropy: ambient` + `clock: system`).
    pub run: Option<Spanned<RunDecl>>,
    /// `tasks:` — the DAG.
    pub tasks: Vec<Spanned<RawTask>>,
    /// `outputs:` — the workflow's return contract.
    pub outputs: Vec<(Spanned<String>, OutputDecl)>,
}

impl RawWorkflow {
    /// Create a new empty raw workflow.
    #[must_use]
    pub fn new() -> Self {
        Self {
            workflow: None,
            model: None,
            inputs: Vec::new(),
            consts: Vec::new(),
            secrets: Vec::new(),
            permits: None,
            run: None,
            tasks: Vec::new(),
            outputs: Vec::new(),
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
        assert!(w.workflow.is_none());
        assert!(w.tasks.is_empty());
        assert!(w.inputs.is_empty());
        assert!(w.consts.is_empty());
        assert!(w.secrets.is_empty());
        assert!(w.run.is_none());
        assert!(w.outputs.is_empty());
    }
}
