//! Static catalog of known `nika:*` builtin tool names.
//!
//! This list is used by the analyzer to validate `invoke: nika:xxx` references
//! at check time. It must be kept in sync with the runtime router in nika-engine.

/// All known nika:* builtin tool names (without the `nika:` prefix).
///
/// Categories:
/// - Core (7): sleep, log, emit, assert, prompt, run, complete
/// - File (5): read, write, edit, glob, grep
/// - Data (12): jq, tree_data, inject, map, filter, group_by, enrich,
///   json_merge, set_diff, zip, chunk, token_count
/// - Sprint 2 (6): json_verify, yaml_validate, locale_lookup, aggregate,
///   json_flatten, json_unflatten
/// - Introspection (4): dag_info, task_status, threads, orchestrate
/// - Cost/Records (2): cost, records
/// - Media always-on (5): import, decode, dimensions, thumbhash, dominant_color
/// - Media core (3): thumbnail, convert, strip
/// - Media opt-in (17): metadata, optimize, svg_render, chart, phash, compare,
///   pdf_extract, provenance, verify, qr_validate, quality, html_to_md,
///   css_select, extract_metadata, extract_links, readability, pipeline
pub static KNOWN_BUILTIN_TOOLS: &[&str] = &[
    // Core (7)
    "sleep",
    "log",
    "emit",
    "assert",
    "prompt",
    "run",
    "complete",
    // File (5)
    "read",
    "write",
    "edit",
    "glob",
    "grep",
    // Data (12)
    "jq",
    "tree_data",
    "inject",
    "map",
    "filter",
    "group_by",
    "enrich",
    "json_merge",
    "set_diff",
    "zip",
    "chunk",
    "token_count",
    // Sprint 2 (6)
    "json_verify",
    "yaml_validate",
    "locale_lookup",
    "aggregate",
    "json_flatten",
    "json_unflatten",
    // Introspection (4)
    "dag_info",
    "task_status",
    "threads",
    "orchestrate",
    // Cost / Records (2)
    "cost",
    "records",
    // Media always-on (5)
    "import",
    "decode",
    "dimensions",
    "thumbhash",
    "dominant_color",
    // Media core (3)
    "thumbnail",
    "convert",
    "strip",
    // Media opt-in (17)
    "metadata",
    "optimize",
    "svg_render",
    "chart",
    "phash",
    "compare",
    "pdf_extract",
    "provenance",
    "verify",
    "qr_validate",
    "quality",
    "html_to_md",
    "css_select",
    "extract_metadata",
    "extract_links",
    "readability",
    "pipeline",
];

/// Check if a tool name (without nika: prefix) is a known builtin.
pub fn is_known_builtin(name: &str) -> bool {
    KNOWN_BUILTIN_TOOLS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_builtins_count() {
        // 7 + 5 + 12 + 6 + 4 + 2 + 5 + 3 + 17 = 61
        assert_eq!(KNOWN_BUILTIN_TOOLS.len(), 61);
    }

    #[test]
    fn test_is_known_builtin() {
        assert!(is_known_builtin("sleep"));
        assert!(is_known_builtin("jq"));
        assert!(is_known_builtin("pipeline"));
        assert!(!is_known_builtin("typo_tool"));
        assert!(!is_known_builtin("json_query")); // deprecated, removed
    }
}
