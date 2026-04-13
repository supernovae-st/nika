// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static builtin tool catalog — 63 `nika:*` tools in a sorted array.
//!
//! Case-sensitive lookup via binary search. Tool names are engine-controlled,
//! always lowercase.

use crate::types::builtin::{Builtin, BuiltinCategory};

use BuiltinCategory::{
    Agent, Core, CostRecords, Data, DataSprint2, File, Introspection, MediaAlwaysOn, MediaCore,
    MediaOptIn,
};

/// All 63 builtin tools, **sorted alphabetically by name**.
///
/// Invariant: array MUST be sorted for `binary_search` to work.
/// This is validated by a unit test.
pub static ALL_BUILTINS: &[Builtin] = &[
    Builtin { name: "aggregate", category: DataSprint2 },
    Builtin { name: "assert", category: Core },
    Builtin { name: "chart", category: MediaOptIn },
    Builtin { name: "chunk", category: Data },
    Builtin { name: "compare", category: MediaOptIn },
    Builtin { name: "complete", category: Core },
    Builtin { name: "convert", category: MediaCore },
    Builtin { name: "cost", category: CostRecords },
    Builtin { name: "css_select", category: MediaOptIn },
    Builtin { name: "dag_info", category: Introspection },
    Builtin { name: "decode", category: MediaAlwaysOn },
    Builtin { name: "dimensions", category: MediaAlwaysOn },
    Builtin { name: "dominant_color", category: MediaAlwaysOn },
    Builtin { name: "edit", category: File },
    Builtin { name: "emit", category: Core },
    Builtin { name: "enrich", category: Data },
    Builtin { name: "extract_links", category: MediaOptIn },
    Builtin { name: "extract_metadata", category: MediaOptIn },
    Builtin { name: "fetch", category: Agent },
    Builtin { name: "filter", category: Data },
    Builtin { name: "glob", category: File },
    Builtin { name: "grep", category: File },
    Builtin { name: "group_by", category: Data },
    Builtin { name: "html_to_md", category: MediaOptIn },
    Builtin { name: "import", category: MediaAlwaysOn },
    Builtin { name: "inject", category: Data },
    Builtin { name: "jq", category: Data },
    Builtin { name: "json_diff", category: Data },
    Builtin { name: "json_flatten", category: DataSprint2 },
    Builtin { name: "json_merge", category: Data },
    Builtin { name: "json_unflatten", category: DataSprint2 },
    Builtin { name: "json_verify", category: DataSprint2 },
    Builtin { name: "locale_lookup", category: DataSprint2 },
    Builtin { name: "log", category: Core },
    Builtin { name: "map", category: Data },
    Builtin { name: "metadata", category: MediaOptIn },
    Builtin { name: "optimize", category: MediaOptIn },
    Builtin { name: "orchestrate", category: Introspection },
    Builtin { name: "pdf_extract", category: MediaOptIn },
    Builtin { name: "phash", category: MediaOptIn },
    Builtin { name: "pipeline", category: MediaOptIn },
    Builtin { name: "prompt", category: Core },
    Builtin { name: "provenance", category: MediaOptIn },
    Builtin { name: "qr_validate", category: MediaOptIn },
    Builtin { name: "quality", category: MediaOptIn },
    Builtin { name: "read", category: File },
    Builtin { name: "readability", category: MediaOptIn },
    Builtin { name: "records", category: CostRecords },
    Builtin { name: "run", category: Core },
    Builtin { name: "set_diff", category: Data },
    Builtin { name: "sleep", category: Core },
    Builtin { name: "strip", category: MediaCore },
    Builtin { name: "svg_render", category: MediaOptIn },
    Builtin { name: "task_status", category: Introspection },
    Builtin { name: "threads", category: Introspection },
    Builtin { name: "thumbhash", category: MediaAlwaysOn },
    Builtin { name: "thumbnail", category: MediaCore },
    Builtin { name: "token_count", category: Data },
    Builtin { name: "tree_data", category: Data },
    Builtin { name: "verify", category: MediaOptIn },
    Builtin { name: "write", category: File },
    Builtin { name: "yaml_validate", category: DataSprint2 },
    Builtin { name: "zip", category: Data },
];

/// Find a builtin tool by name (case-sensitive, O(log n) binary search).
#[must_use]
pub fn find_builtin(name: &str) -> Option<&'static Builtin> {
    ALL_BUILTINS
        .binary_search_by_key(&name, |b| b.name)
        .ok()
        .map(|i| &ALL_BUILTINS[i])
}

/// Check if a tool name (without `nika:` prefix) is a known builtin.
#[must_use]
pub fn is_known_builtin(name: &str) -> bool {
    ALL_BUILTINS.binary_search_by_key(&name, |b| b.name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_count() {
        assert_eq!(ALL_BUILTINS.len(), 63);
    }

    #[test]
    fn sorted_order() {
        for pair in ALL_BUILTINS.windows(2) {
            assert!(
                pair[0].name < pair[1].name,
                "builtins not sorted: `{}` >= `{}`",
                pair[0].name,
                pair[1].name
            );
        }
    }

    #[test]
    fn find_known_builtins() {
        let names = ["sleep", "jq", "pipeline", "read", "write", "complete"];
        for name in names {
            assert!(
                find_builtin(name).is_some(),
                "builtin `{name}` not found"
            );
        }
    }

    #[test]
    fn is_known_builtin_works() {
        assert!(is_known_builtin("sleep"));
        assert!(is_known_builtin("jq"));
        assert!(!is_known_builtin("typo_tool"));
        assert!(!is_known_builtin("json_query")); // deprecated, removed
    }

    #[test]
    fn unknown_returns_none() {
        assert!(find_builtin("nonexistent").is_none());
        assert!(find_builtin("").is_none());
    }

    #[test]
    fn case_sensitive() {
        // Builtins are case-sensitive — uppercase should NOT match
        assert!(find_builtin("Sleep").is_none());
        assert!(find_builtin("JQ").is_none());
    }

    #[test]
    fn all_builtins_have_non_empty_names() {
        for builtin in ALL_BUILTINS {
            assert!(!builtin.name.is_empty());
        }
    }

    #[test]
    fn category_counts() {
        let core = ALL_BUILTINS.iter().filter(|b| b.category == Core).count();
        let file = ALL_BUILTINS.iter().filter(|b| b.category == File).count();
        let data = ALL_BUILTINS.iter().filter(|b| b.category == Data).count();
        assert_eq!(core, 7, "expected 7 core builtins");
        assert_eq!(file, 5, "expected 5 file builtins");
        assert_eq!(data, 13, "expected 13 data builtins");
    }
}
