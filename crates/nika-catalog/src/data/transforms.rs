// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static transform catalog — 65 pipe transforms in a sorted array.
//!
//! Case-sensitive lookup via binary search. Transform names are
//! parser-controlled, always lowercase.

use crate::types::transform::{NullBehavior, TransformArity, TransformCategory, TransformDef};

use NullBehavior::{Fail, Propagate};
use TransformArity::{Nullary, Unary, Variadic};
use TransformCategory::{
    Aggregation, Array, Encoding, Escape, Introspection, Jq, Logic, Numeric, Parametric, Query,
    String, StringTest, System, Type, Url,
};

/// All 65 pipe transforms, **sorted alphabetically by name**.
///
/// Invariant: array MUST be sorted for `binary_search` to work.
/// This is validated by a unit test.
pub static ALL_TRANSFORMS: &[TransformDef] = &[
    TransformDef {
        name: "abs",
        arity: Nullary,
        null_behavior: Fail,
        category: Numeric,
    },
    TransformDef {
        name: "add",
        arity: Nullary,
        null_behavior: Fail,
        category: Aggregation,
    },
    TransformDef {
        name: "avg",
        arity: Nullary,
        null_behavior: Fail,
        category: Aggregation,
    },
    TransformDef {
        name: "base64_decode",
        arity: Nullary,
        null_behavior: Fail,
        category: Encoding,
    },
    TransformDef {
        name: "base64_encode",
        arity: Nullary,
        null_behavior: Fail,
        category: Encoding,
    },
    TransformDef {
        name: "ceil",
        arity: Nullary,
        null_behavior: Fail,
        category: Numeric,
    },
    TransformDef {
        name: "compact",
        arity: Nullary,
        null_behavior: Propagate,
        category: Array,
    },
    TransformDef {
        name: "contains",
        arity: Unary,
        null_behavior: Fail,
        category: StringTest,
    },
    TransformDef {
        name: "content_hash",
        arity: Nullary,
        null_behavior: Fail,
        category: Encoding,
    },
    TransformDef {
        name: "default",
        arity: Unary,
        null_behavior: Propagate,
        category: Parametric,
    },
    TransformDef {
        name: "ends_with",
        arity: Unary,
        null_behavior: Fail,
        category: StringTest,
    },
    TransformDef {
        name: "first",
        arity: Nullary,
        null_behavior: Propagate,
        category: Array,
    },
    TransformDef {
        name: "flatten",
        arity: Nullary,
        null_behavior: Propagate,
        category: Array,
    },
    TransformDef {
        name: "floor",
        arity: Nullary,
        null_behavior: Fail,
        category: Numeric,
    },
    TransformDef {
        name: "group_by",
        arity: Unary,
        null_behavior: Fail,
        category: Query,
    },
    TransformDef {
        name: "has",
        arity: Unary,
        null_behavior: Propagate,
        category: Introspection,
    },
    TransformDef {
        name: "html_escape",
        arity: Nullary,
        null_behavior: Fail,
        category: Escape,
    },
    TransformDef {
        name: "join",
        arity: Unary,
        null_behavior: Fail,
        category: Parametric,
    },
    TransformDef {
        name: "jq",
        arity: Unary,
        null_behavior: Fail,
        category: Jq,
    },
    TransformDef {
        name: "keys",
        arity: Nullary,
        null_behavior: Propagate,
        category: Array,
    },
    TransformDef {
        name: "last",
        arity: Nullary,
        null_behavior: Propagate,
        category: Array,
    },
    TransformDef {
        name: "length",
        arity: Nullary,
        null_behavior: Propagate,
        category: String,
    },
    TransformDef {
        name: "lower",
        arity: Nullary,
        null_behavior: Fail,
        category: String,
    },
    TransformDef {
        name: "max",
        arity: Nullary,
        null_behavior: Fail,
        category: Aggregation,
    },
    TransformDef {
        name: "max_by",
        arity: Unary,
        null_behavior: Fail,
        category: Aggregation,
    },
    TransformDef {
        name: "md_escape",
        arity: Nullary,
        null_behavior: Fail,
        category: Escape,
    },
    TransformDef {
        name: "merge",
        arity: Nullary,
        null_behavior: Fail,
        category: Array,
    },
    TransformDef {
        name: "min",
        arity: Nullary,
        null_behavior: Fail,
        category: Aggregation,
    },
    TransformDef {
        name: "min_by",
        arity: Unary,
        null_behavior: Fail,
        category: Aggregation,
    },
    TransformDef {
        name: "not",
        arity: Nullary,
        null_behavior: Propagate,
        category: Logic,
    },
    TransformDef {
        name: "omit",
        arity: Variadic,
        null_behavior: Fail,
        category: Query,
    },
    TransformDef {
        name: "parse_json",
        arity: Nullary,
        null_behavior: Fail,
        category: Type,
    },
    TransformDef {
        name: "parse_yaml",
        arity: Nullary,
        null_behavior: Fail,
        category: Type,
    },
    TransformDef {
        name: "pick",
        arity: Variadic,
        null_behavior: Fail,
        category: Query,
    },
    TransformDef {
        name: "pluck",
        arity: Unary,
        null_behavior: Fail,
        category: Query,
    },
    TransformDef {
        name: "regex",
        arity: Unary,
        null_behavior: Fail,
        category: Query,
    },
    TransformDef {
        name: "replace",
        arity: Variadic,
        null_behavior: Fail,
        category: String,
    },
    TransformDef {
        name: "reverse",
        arity: Nullary,
        null_behavior: Propagate,
        category: Array,
    },
    TransformDef {
        name: "round",
        arity: Nullary,
        null_behavior: Fail,
        category: Numeric,
    },
    TransformDef {
        name: "sanitize",
        arity: Nullary,
        null_behavior: Fail,
        category: Escape,
    },
    TransformDef {
        name: "shell",
        arity: Nullary,
        null_behavior: Fail,
        category: System,
    },
    TransformDef {
        name: "slice",
        arity: Variadic,
        null_behavior: Fail,
        category: Parametric,
    },
    TransformDef {
        name: "sort",
        arity: Nullary,
        null_behavior: Propagate,
        category: Array,
    },
    TransformDef {
        name: "sort_by",
        arity: Unary,
        null_behavior: Fail,
        category: Query,
    },
    TransformDef {
        name: "split",
        arity: Unary,
        null_behavior: Fail,
        category: Parametric,
    },
    TransformDef {
        name: "starts_with",
        arity: Unary,
        null_behavior: Fail,
        category: StringTest,
    },
    TransformDef {
        name: "sum",
        arity: Nullary,
        null_behavior: Fail,
        category: Aggregation,
    },
    TransformDef {
        name: "to_bool",
        arity: Nullary,
        null_behavior: Fail,
        category: Type,
    },
    TransformDef {
        name: "to_json",
        arity: Nullary,
        null_behavior: Propagate,
        category: Type,
    },
    TransformDef {
        name: "to_number",
        arity: Nullary,
        null_behavior: Fail,
        category: Type,
    },
    TransformDef {
        name: "to_string",
        arity: Nullary,
        null_behavior: Propagate,
        category: Type,
    },
    TransformDef {
        name: "trim",
        arity: Nullary,
        null_behavior: Fail,
        category: String,
    },
    TransformDef {
        name: "trim_end",
        arity: Nullary,
        null_behavior: Fail,
        category: String,
    },
    TransformDef {
        name: "trim_start",
        arity: Nullary,
        null_behavior: Fail,
        category: String,
    },
    TransformDef {
        name: "truncate",
        arity: Unary,
        null_behavior: Fail,
        category: String,
    },
    TransformDef {
        name: "type_of",
        arity: Nullary,
        null_behavior: Propagate,
        category: Introspection,
    },
    TransformDef {
        name: "unique",
        arity: Nullary,
        null_behavior: Propagate,
        category: Array,
    },
    TransformDef {
        name: "unique_urls",
        arity: Nullary,
        null_behavior: Fail,
        category: Encoding,
    },
    TransformDef {
        name: "upper",
        arity: Nullary,
        null_behavior: Fail,
        category: String,
    },
    TransformDef {
        name: "url_host",
        arity: Nullary,
        null_behavior: Fail,
        category: Url,
    },
    TransformDef {
        name: "url_normalize",
        arity: Nullary,
        null_behavior: Fail,
        category: Url,
    },
    TransformDef {
        name: "url_path",
        arity: Nullary,
        null_behavior: Fail,
        category: Url,
    },
    TransformDef {
        name: "url_without_query",
        arity: Nullary,
        null_behavior: Fail,
        category: Url,
    },
    TransformDef {
        name: "values",
        arity: Nullary,
        null_behavior: Propagate,
        category: Array,
    },
    TransformDef {
        name: "where",
        arity: Variadic,
        null_behavior: Fail,
        category: Query,
    },
];

/// Find a transform by name (case-sensitive, O(log n) binary search).
#[must_use]
pub fn find_transform(name: &str) -> Option<&'static TransformDef> {
    ALL_TRANSFORMS
        .binary_search_by_key(&name, |t| t.name)
        .ok()
        .map(|i| &ALL_TRANSFORMS[i])
}

/// Check if a transform name is known.
#[must_use]
pub fn is_known_transform(name: &str) -> bool {
    ALL_TRANSFORMS
        .binary_search_by_key(&name, |t| t.name)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_count() {
        assert_eq!(ALL_TRANSFORMS.len(), 65);
    }

    #[test]
    fn sorted_order() {
        for pair in ALL_TRANSFORMS.windows(2) {
            assert!(
                pair[0].name < pair[1].name,
                "transforms not sorted: `{}` >= `{}`",
                pair[0].name,
                pair[1].name
            );
        }
    }

    #[test]
    fn find_known_transforms() {
        let names = ["upper", "lower", "trim", "jq", "shell", "join", "default"];
        for name in names {
            assert!(
                find_transform(name).is_some(),
                "transform `{name}` not found"
            );
        }
    }

    #[test]
    fn is_known_transform_works() {
        assert!(is_known_transform("upper"));
        assert!(is_known_transform("base64_encode"));
        assert!(!is_known_transform("pad"));
        assert!(!is_known_transform("enumerate"));
    }

    #[test]
    fn unknown_returns_none() {
        assert!(find_transform("nonexistent").is_none());
        assert!(find_transform("").is_none());
    }

    #[test]
    fn case_sensitive() {
        assert!(find_transform("Upper").is_none());
        assert!(find_transform("TRIM").is_none());
    }

    #[test]
    fn all_transforms_have_non_empty_names() {
        for t in ALL_TRANSFORMS {
            assert!(!t.name.is_empty());
        }
    }

    #[test]
    fn arity_validation() {
        // Nullary: no args
        let upper = find_transform("upper").unwrap();
        assert_eq!(upper.arity, Nullary);

        // Unary: one arg
        let join = find_transform("join").unwrap();
        assert_eq!(join.arity, Unary);

        // Variadic: multiple args
        let pick = find_transform("pick").unwrap();
        assert_eq!(pick.arity, Variadic);
    }

    #[test]
    fn default_propagates_null() {
        let default = find_transform("default").unwrap();
        assert_eq!(default.null_behavior, Propagate);
    }
}
