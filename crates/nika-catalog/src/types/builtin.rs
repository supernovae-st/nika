// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Builtin tool definitions (nika:* tools).
//!
//! 22 tools across 5 categories. Stored in a sorted array for
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
    /// Declared argument keys (the `args:` object the builtin reads). The
    /// vocabulary `nika check` validates against — an `args:` key outside
    /// this set is the `nika:jq` `data:`-vs-`input:` footgun class (an
    /// ignored arg the runtime silently drops). Kept in lockstep with the
    /// model-facing `ToolDef` schemas in `nika-builtin` (a drift guard
    /// there asserts the two match). Empty `&[]` = no declared args yet.
    pub args: &'static [&'static str],
    /// The UNCONDITIONALLY required argument keys (the JSON-Schema
    /// `required` set the model-facing `ToolDef` declares). `nika check`
    /// flags a call missing any of these at check time instead of at run
    /// time (the « 5/23 builtins had a static required-arg check » gap).
    /// A subset of [`Self::args`]. Builtins whose requirement is
    /// CONDITIONAL (`nika:wait` `duration` XOR `until` · `nika:fetch`
    /// mode-dependent `jq`/`selector`) keep `&[]` here — the
    /// `analyzer::builtin_shape` ladder owns those non-flat contracts.
    pub required: &'static [&'static str],
}

impl Builtin {
    /// Explicit constructor — required because [`Builtin`] is
    /// `#[non_exhaustive]` (invariant #19). Declares no args; prefer
    /// [`Self::with_args`] for a builtin that takes an `args:` object.
    #[must_use]
    pub const fn new(name: &'static str, category: BuiltinCategory) -> Self {
        Self {
            name,
            category,
            args: &[],
            required: &[],
        }
    }

    /// Constructor with the declared argument-key vocabulary (no
    /// unconditionally-required keys · for builtins whose requirements are
    /// all optional OR conditional · `nika:wait`, `nika:fetch`).
    #[must_use]
    pub const fn with_args(
        name: &'static str,
        category: BuiltinCategory,
        args: &'static [&'static str],
    ) -> Self {
        Self {
            name,
            category,
            args,
            required: &[],
        }
    }

    /// Constructor with the declared arg vocabulary AND the
    /// unconditionally-required subset (the JSON-Schema `required` set).
    /// `required` MUST be a subset of `args` (a drift guard in
    /// `nika-builtin` pins it to the `ToolDef` schemas).
    #[must_use]
    pub const fn with_required(
        name: &'static str,
        category: BuiltinCategory,
        args: &'static [&'static str],
        required: &'static [&'static str],
    ) -> Self {
        Self {
            name,
            category,
            args,
            required,
        }
    }
}

/// Category of builtin tool.
///
/// 5 categories per `nika/spec/stdlib/builtins-v0.1.md` ·
/// - [`Self::Core`] (6) · `log` · `emit` · `assert` · `prompt` · `done` · `wait`
/// - [`Self::File`] (5) · `read` · `write` · `edit` · `glob` · `grep`
/// - [`Self::Data`] (8) · `jq` · `json_diff` · `validate` · `json_merge_patch` · `convert` · `uuid` · `date` · `hash`
/// - [`Self::Network`] (2) · `fetch` · `notify`
/// - [`Self::Introspection`] (1) · `inspect` (view-discriminated · 4 views · `cost` / `records` / `dag_info` / `threads`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum BuiltinCategory {
    /// Core control + assertion + observation primitives.
    /// `log` · `emit` · `assert` · `prompt` · `done` · `wait`.
    Core,
    /// File system primitives (read-side trust-propagating · write-side external).
    /// `read` · `write` · `edit` · `glob` · `grep`.
    File,
    /// Data transform + validation + identity primitives. `jq` is THE
    /// data language (subsumes legacy `map` · `filter` · `group_by` ·
    /// `json_merge` · etc per D-2026-05-22-N6 cut).
    /// `jq` · `json_diff` · `validate` · `json_merge_patch` · `convert` · `uuid` · `date` · `hash`.
    Data,
    /// Network I/O primitives. Output trust = `Untrusted` (always).
    /// `fetch` · `notify`.
    Network,
    /// Workflow introspection primitive · 1 builtin with view-discriminator
    /// covering DAG state · cost · records · threads (post ADR-088 collapse).
    /// `inspect` (view: `cost` | `records` | `dag_info` | `threads`).
    Introspection,
}

impl BuiltinCategory {
    /// Kebab-case wire identifier — the SAME rendering as the serde
    /// `rename_all = "kebab-case"` derive (a unit test pins the two), so
    /// serde-less consumers (`nika tools --json` join) speak one vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::File => "file",
            Self::Data => "data",
            Self::Network => "network",
            Self::Introspection => "introspection",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CATEGORIES: [BuiltinCategory; 5] = [
        BuiltinCategory::Core,
        BuiltinCategory::File,
        BuiltinCategory::Data,
        BuiltinCategory::Network,
        BuiltinCategory::Introspection,
    ];

    #[test]
    fn category_as_str_is_the_kebab_wire_identifier() {
        let strs: Vec<&str> = ALL_CATEGORIES.iter().map(|c| c.as_str()).collect();
        assert_eq!(strs, ["core", "file", "data", "network", "introspection"]);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn category_as_str_agrees_with_the_serde_rendering() {
        for cat in ALL_CATEGORIES {
            let via_serde = serde_json::to_value(cat).expect("category serializes");
            assert_eq!(
                via_serde,
                serde_json::Value::String(cat.as_str().to_owned()),
                "as_str and the serde kebab-case derive must be ONE rendering",
            );
        }
    }
}
