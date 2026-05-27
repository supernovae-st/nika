// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Builtin tool definitions (nika:* tools).
//!
//! 26 tools across 5 categories. Stored in a sorted array for
//! case-sensitive binary search.
//!
//! Reconciled to spec v0.1 stdlib per D-2026-05-22-N6 (42→26 collapse ·
//! `jq` subsumes ~13 data builtins · `JSONPath` dropped · media DEFERRED
//! to stdlib v0.x) + 2026-05-27 follow-on `nika:json_merge` cut (`jaq`
//! source-verified · `jq *` recursive-merge subsumes it).

/// A known `nika:*` builtin tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Builtin {
    /// Tool name without the `nika:` prefix (e.g. `"sleep"`).
    pub name: &'static str,
    /// Functional category for grouping.
    pub category: BuiltinCategory,
}

impl Builtin {
    /// Explicit constructor — required because [`Builtin`] is
    /// `#[non_exhaustive]` (invariant #19).
    #[must_use]
    pub const fn new(name: &'static str, category: BuiltinCategory) -> Self {
        Self { name, category }
    }
}

/// Category of builtin tool.
///
/// 5 categories per `nika/spec/stdlib/builtins-v0.1.md` ·
/// - [`Self::Core`] (7) · `sleep` · `log` · `emit` · `assert` · `prompt` · `done` · `wait_until`
/// - [`Self::File`] (5) · `read` · `write` · `edit` · `glob` · `grep`
/// - [`Self::Data`] (8) · `jq` · `json_diff` · `validate` · `json_merge_patch` · `csv_to_json` · `uuid` · `date` · `hash`
/// - [`Self::Network`] (2) · `fetch` · `notify`
/// - [`Self::Introspection`] (4) · `cost` · `records` · `dag_info` · `threads`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum BuiltinCategory {
    /// Core control + assertion + observation primitives.
    /// `sleep` · `log` · `emit` · `assert` · `prompt` · `done` · `wait_until`.
    Core,
    /// File system primitives (read-side trust-propagating · write-side external).
    /// `read` · `write` · `edit` · `glob` · `grep`.
    File,
    /// Data transform + validation + identity primitives. `jq` is THE
    /// data language (subsumes legacy `map` · `filter` · `group_by` ·
    /// `json_merge` · etc per D-2026-05-22-N6 cut).
    /// `jq` · `json_diff` · `validate` · `json_merge_patch` · `csv_to_json` · `uuid` · `date` · `hash`.
    Data,
    /// Network I/O primitives. Output trust = `Untrusted` (always).
    /// `fetch` · `notify`.
    Network,
    /// Workflow introspection primitives (DAG state · cost · records · threads).
    /// `cost` · `records` · `dag_info` · `threads`.
    Introspection,
}
