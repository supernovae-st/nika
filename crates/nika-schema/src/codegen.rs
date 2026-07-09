// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! JSON-Schema codegen primitives for the Nika workflow schema.
//!
//! **Today** · pure-Rust helpers that emit JSON-Schema fragments derived
//! from `nika-catalog` (single source of truth · the canonical 27 builtins
//! per `nika/spec/stdlib/builtins-v0.1.md` post ADR-084 catalog
//! reconciliation, ADR-086 convert, ADR-087 wait, ADR-088 inspect,
//! ADR-096 compose, and `image_generate` — the first stdlib §Media
//! graduate).
//!
//! **Future (ADR-085 trigger-gated)** · once `WorkflowDocument` lands in
//! nika-schema with `#[derive(JsonSchema)]` (schemars 1.0 adoption · trigger
//! gate · cookbook `$schema` modeline + ≥1 LSP UX feedback signal), this
//! module exposes `schema_with` adapters that schemars consumes · the
//! existing hand-derived `nika/spec/schemas/workflow.schema.json` becomes
//! a CODEGEN OUTPUT emitted by a future `nika-schema-codegen` xtask
//! (replacing the hand-maintained oneOf).
//!
//! **Why it lives here today** · zero-cost · pure-Rust function · matches
//! L0 constraint (no I/O · no async) · ready to flip from "called by test"
//! to "called by schemars `schema_with`" the day schemars adopts.

use nika_catalog::all_builtins;
use serde_json::{Value, json};

/// Build the closed-enum JSON-Schema fragment for the `nika:<name>` branch
/// of the workflow's `invoke.tool` field.
///
/// Reads [`nika_catalog::all_builtins`] at call time · the schema is
/// automatically up-to-date with the engine catalog (no-drift property ·
/// **structural** · not discipline). The 24 entries are sorted alphabetically
/// (matches `ALL_BUILTINS` binary-search invariant).
///
/// # Returned shape
///
/// ```jsonc
/// {
///   "type": "string",
///   "description": "`nika:*` builtin · 26 canonical per stdlib v0.1",
///   "enum": [
///     "nika:assert", "nika:compose", "nika:convert", "nika:date",
///     "nika:done",   "nika:edit",    "nika:emit",    "nika:fetch",
///     "nika:glob",   "nika:grep",    "nika:hash",
///     "nika:image_generate",         "nika:inspect", "nika:jq",
///     "nika:json_diff",   "nika:json_merge_patch",   "nika:log",
///     "nika:notify", "nika:prompt",  "nika:read",    "nika:uuid",
///     "nika:validate", "nika:wait",  "nika:write"
///   ]
/// }
/// ```
///
/// # Cross-reference
///
/// - `nika/engine/docs/adr/adr-085-...` (this fn's parent ADR)
/// - `nika/spec/schemas/workflow.schema.json` (hand-derived oneOf · the
///   `nika` branch enum MUST match this output · enforced by the
///   sister tests below · spec-engine no-drift gate)
/// - `nika/spec/stdlib/builtins-v0.1.md` (the canonical 26 source-of-truth)
#[must_use]
pub fn nika_builtin_tool_enum_schema() -> Value {
    let names: Vec<String> = all_builtins()
        .iter()
        .map(|b| format!("nika:{}", b.name))
        .collect();
    let count = names.len();
    json!({
        "type": "string",
        "description": format!("`nika:*` builtin · {count} canonical per stdlib v0.1"),
        "enum": names
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_has_27_entries() {
        let schema = nika_builtin_tool_enum_schema();
        let arr = schema["enum"].as_array().expect("enum must be array");
        assert_eq!(arr.len(), 27, "expected 27 spec-canonical builtins");
    }

    #[test]
    fn enum_alphabetically_sorted() {
        let schema = nika_builtin_tool_enum_schema();
        let arr = schema["enum"].as_array().expect("enum must be array");
        for pair in arr.windows(2) {
            let a = pair[0].as_str().expect("string entry");
            let b = pair[1].as_str().expect("string entry");
            assert!(a < b, "enum not sorted · `{a}` >= `{b}`");
        }
    }

    #[test]
    fn enum_all_prefixed_with_nika() {
        let schema = nika_builtin_tool_enum_schema();
        let arr = schema["enum"].as_array().expect("enum must be array");
        for entry in arr {
            let s = entry.as_str().expect("string entry");
            assert!(s.starts_with("nika:"), "entry `{s}` missing `nika:` prefix");
        }
    }

    #[test]
    fn enum_matches_catalog_one_to_one() {
        // No-drift gate · the schema enum MUST mirror ALL_BUILTINS exactly.
        let schema = nika_builtin_tool_enum_schema();
        let actual: Vec<String> = schema["enum"]
            .as_array()
            .expect("enum array")
            .iter()
            .map(|v| v.as_str().expect("string").to_string())
            .collect();
        let expected: Vec<String> = all_builtins()
            .iter()
            .map(|b| format!("nika:{}", b.name))
            .collect();
        assert_eq!(
            actual, expected,
            "schema enum drifted from nika_catalog::ALL_BUILTINS \
             · spec/schemas/workflow.schema.json invoke.tool oneOf nika branch \
             MUST regenerate (see ADR-085)"
        );
    }

    #[test]
    fn schema_is_string_type() {
        let schema = nika_builtin_tool_enum_schema();
        assert_eq!(schema["type"], "string", "schema must declare type=string");
    }

    #[test]
    fn schema_has_description() {
        let schema = nika_builtin_tool_enum_schema();
        assert!(
            schema["description"].is_string(),
            "schema must carry a description"
        );
    }

    #[test]
    fn enum_includes_canonical_anchors() {
        // Sanity · 6 spec-canonical names from across the 6 categories.
        let schema = nika_builtin_tool_enum_schema();
        let arr = schema["enum"].as_array().expect("enum array");
        for canonical in [
            "nika:wait",
            "nika:read",
            "nika:jq",
            "nika:fetch",
            "nika:inspect",
            "nika:image_generate",
        ] {
            assert!(
                arr.iter().any(|v| v.as_str() == Some(canonical)),
                "missing canonical anchor `{canonical}`"
            );
        }
    }

    #[test]
    fn enum_excludes_legacy_post_d_n6() {
        // Regression guard · 12 legacy names (cut per D-2026-05-22-N6 +
        // 2026-05-27 json_merge cut + ADR-086 convert + ADR-087 wait +
        // ADR-088 inspect Rams sweep · jq + json_merge_patch subsume
        // the data-transform legacy · `nika:convert` supersedes
        // `nika:csv_to_json` · unified `nika:wait` supersedes
        // `nika:sleep` + `nika:wait_until` · view-discriminated
        // `nika:inspect` supersedes `cost`+`records`+`dag_info`+`threads`).
        let schema = nika_builtin_tool_enum_schema();
        let arr = schema["enum"].as_array().expect("enum array");
        for legacy in [
            "nika:map",
            "nika:filter",
            "nika:json_merge",
            "nika:run",
            "nika:complete",
            "nika:csv_to_json",
            "nika:sleep",
            "nika:wait_until",
            "nika:cost",
            "nika:records",
            "nika:dag_info",
            "nika:threads",
        ] {
            assert!(
                !arr.iter().any(|v| v.as_str() == Some(legacy)),
                "legacy `{legacy}` must not appear in spec-22 enum"
            );
        }
    }
}
