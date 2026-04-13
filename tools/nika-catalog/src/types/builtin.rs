// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Builtin tool definitions (nika:* tools).
//!
//! 63 tools across 10 categories. Stored in a sorted array for
//! case-sensitive binary search.

/// A known `nika:*` builtin tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Builtin {
    /// Tool name without the `nika:` prefix (e.g. `"sleep"`).
    pub name: &'static str,
    /// Functional category for grouping.
    pub category: BuiltinCategory,
}

/// Category of builtin tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum BuiltinCategory {
    /// Core tools: sleep, log, emit, assert, prompt, run, complete.
    Core,
    /// File tools: read, write, edit, glob, grep.
    File,
    /// Data tools: `jq`, `json_merge`, `json_diff`, etc.
    Data,
    /// Sprint 2 data tools: `json_verify`, `yaml_validate`, etc.
    DataSprint2,
    /// Introspection tools: `dag_info`, `task_status`, `threads`, `orchestrate`.
    Introspection,
    /// Cost and records tools.
    CostRecords,
    /// Agent tools: fetch.
    Agent,
    /// Always-on media tools: `import`, `decode`, `dimensions`, `thumbhash`, `dominant_color`.
    MediaAlwaysOn,
    /// Core media tools: thumbnail, convert, strip.
    MediaCore,
    /// Opt-in media tools: `metadata`, `optimize`, `svg_render`, etc.
    MediaOptIn,
}
