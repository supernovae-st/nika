// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static builtin tool catalog — 22 `nika:*` tools in a sorted array.
//!
//! Case-sensitive lookup via binary search. Tool names are engine-controlled,
//! always lowercase.
//!
//! Source of truth · `nika/spec/stdlib/builtins-v0.1.md` (canonical 22 per
//! D-2026-05-22-N6 stdlib-collapse 42→26 · `jq` subsumes ~13 data builtins ·
//! `JSONPath` dropped · media DEFERRED to stdlib v0.x) + 2026-05-27 follow-on
//! `nika:json_merge` cut + ADR-086 `csv_to_json` → `convert` + ADR-087
//! `sleep` + `wait_until` → `wait` + ADR-088 `cost`+`records`+`dag_info`+
//! `threads` → `inspect` view-discriminated (`jaq` source-verified ·
//! `nika:json_merge_patch` stays for RFC-7396 null-delete which `jq *` cannot
//! express).
//!
//! 5 categories · Core 6 · File 5 · Data 8 · Network 2 · Introspection 1 = 22.

use crate::types::builtin::{Builtin, BuiltinCategory};

use BuiltinCategory::{Core, Data, File, Introspection, Network};

/// All 22 builtin tools, **sorted alphabetically by name**.
///
/// Invariant: array MUST be sorted for `binary_search` to work.
/// This is validated by a unit test.
pub static ALL_BUILTINS: &[Builtin] = &[
    Builtin {
        name: "assert",
        category: Core,
    },
    Builtin {
        name: "convert",
        category: Data,
    },
    Builtin {
        name: "date",
        category: Data,
    },
    Builtin {
        name: "done",
        category: Core,
    },
    Builtin {
        name: "edit",
        category: File,
    },
    Builtin {
        name: "emit",
        category: Core,
    },
    Builtin {
        name: "fetch",
        category: Network,
    },
    Builtin {
        name: "glob",
        category: File,
    },
    Builtin {
        name: "grep",
        category: File,
    },
    Builtin {
        name: "hash",
        category: Data,
    },
    Builtin {
        name: "inspect",
        category: Introspection,
    },
    Builtin {
        name: "jq",
        category: Data,
    },
    Builtin {
        name: "json_diff",
        category: Data,
    },
    Builtin {
        name: "json_merge_patch",
        category: Data,
    },
    Builtin {
        name: "log",
        category: Core,
    },
    Builtin {
        name: "notify",
        category: Network,
    },
    Builtin {
        name: "prompt",
        category: Core,
    },
    Builtin {
        name: "read",
        category: File,
    },
    Builtin {
        name: "uuid",
        category: Data,
    },
    Builtin {
        name: "validate",
        category: Data,
    },
    Builtin {
        name: "wait",
        category: Core,
    },
    Builtin {
        name: "write",
        category: File,
    },
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
        assert_eq!(ALL_BUILTINS.len(), 22);
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
        // Sample across all 5 categories.
        let names = ["wait", "read", "jq", "fetch", "inspect"];
        for name in names {
            assert!(find_builtin(name).is_some(), "builtin `{name}` not found");
        }
    }

    #[test]
    fn is_known_builtin_works() {
        // Spec-canonical builtins are known.
        assert!(is_known_builtin("wait"));
        assert!(is_known_builtin("jq"));
        assert!(is_known_builtin("validate"));
        assert!(is_known_builtin("notify"));
        assert!(is_known_builtin("inspect"));
        // Unknown + legacy (cut per D-N6 + ADR-086/087/088) return false.
        assert!(!is_known_builtin("typo_tool"));
        assert!(!is_known_builtin("json_query")); // deprecated, removed
        assert!(!is_known_builtin("map")); // legacy, subsumed by jq
        assert!(!is_known_builtin("json_merge")); // legacy, subsumed by jq
        assert!(!is_known_builtin("pipeline")); // legacy, media DEFERRED
        assert!(!is_known_builtin("sleep")); // ADR-087, merged into wait
        assert!(!is_known_builtin("wait_until")); // ADR-087, merged into wait
        assert!(!is_known_builtin("csv_to_json")); // ADR-086, replaced by convert
        assert!(!is_known_builtin("cost")); // ADR-088, merged into inspect
        assert!(!is_known_builtin("records")); // ADR-088, merged into inspect
        assert!(!is_known_builtin("dag_info")); // ADR-088, merged into inspect
        assert!(!is_known_builtin("threads")); // ADR-088, merged into inspect
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
        let network = ALL_BUILTINS
            .iter()
            .filter(|b| b.category == Network)
            .count();
        let intro = ALL_BUILTINS
            .iter()
            .filter(|b| b.category == Introspection)
            .count();
        assert_eq!(
            core, 6,
            "expected 6 core builtins (post-ADR-087 wait merge)"
        );
        assert_eq!(file, 5, "expected 5 file builtins");
        assert_eq!(data, 8, "expected 8 data builtins");
        assert_eq!(network, 2, "expected 2 network builtins");
        assert_eq!(
            intro, 1,
            "expected 1 introspection builtin (post-ADR-088 inspect merge)"
        );
        assert_eq!(
            core + file + data + network + intro,
            22,
            "total must equal 22"
        );
    }
}
