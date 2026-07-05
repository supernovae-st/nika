// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika tools` — the embedded builtin-tool catalog on the wire.
//!
//! `--json` emits the versioned projection (`tools_version: 1`, built by
//! `nika-builtin::tools_json`): per builtin the model-facing JSON-Schema
//! (`parameters`) joined with the check-time contract (`category` ·
//! `args` · `required`). The bare form prints the human listing grouped
//! by category in spec order (core · file · data · network ·
//! introspection).

use crate::verbs::VerbOutput;

/// Spec order of the builtin categories (stdlib taxonomy · media last,
/// the first graduate of the deferred class).
const CATEGORY_ORDER: [&str; 6] = ["core", "file", "data", "network", "introspection", "media"];

/// `nika tools` — human listing, or the `--json` machine projection.
#[must_use]
pub fn run(json: bool) -> VerbOutput {
    let payload = nika_builtin::tools_json();
    if json {
        return match serde_json::to_string_pretty(&payload) {
            Ok(text) => VerbOutput::ok(text),
            Err(e) => VerbOutput::env(format!("tools projection failed: {e}")),
        };
    }
    VerbOutput::ok(human_listing(&payload))
}

/// The human listing — one section per category, spec order.
fn human_listing(payload: &serde_json::Value) -> String {
    use std::fmt::Write as _;
    let tools = payload["tools"].as_array().map_or(&[][..], Vec::as_slice);
    let mut out = format!(
        "nika tools — {} builtins · {} categories (embedded)\n",
        tools.len(),
        CATEGORY_ORDER.len(),
    );
    for category in CATEGORY_ORDER {
        let _ = write!(out, "\n{}\n", category.to_uppercase());
        for tool in tools.iter().filter(|t| t["category"] == category) {
            let name = tool["name"].as_str().unwrap_or("?");
            let desc = tool["description"].as_str().unwrap_or("");
            // The one-line teaching cut: descriptions use ` · ` separators.
            let first = desc.split(" · ").next().unwrap_or(desc);
            let _ = writeln!(out, "  {name:<24} {first}");
        }
    }
    out.push_str(
        "\nnika tools --json → the model-facing JSON-Schemas (parameters · args · required)",
    );
    out
}

#[cfg(test)]
#[allow(clippy::panic)] // formatted assertion messages (the nika-mcp tests precedent)
mod tests {
    use super::*;
    use crate::verbs::exit;

    #[test]
    fn json_surface_is_the_versioned_tools_payload() {
        let out = run(true);
        assert_eq!(out.code, exit::OK);
        let value: serde_json::Value =
            serde_json::from_str(&out.text).expect("--json emits parseable JSON");
        assert_eq!(value["tools_version"], 1, "the locked v1 wire marker");
        let tools = value["tools"].as_array().expect("tools array");
        assert_eq!(
            tools.len(),
            nika_builtin::tool_defs().len(),
            "every builtin is projected",
        );
    }

    #[test]
    fn human_surface_groups_by_category_in_spec_order() {
        let out = run(false);
        assert_eq!(out.code, exit::OK);
        let text = out.text;
        let mut last = 0usize;
        for category in CATEGORY_ORDER {
            let marker = category.to_uppercase();
            let pos = text
                .find(&marker)
                .unwrap_or_else(|| panic!("category section `{marker}` missing"));
            assert!(last <= pos, "`{marker}` out of spec order");
            last = pos;
        }
        assert!(
            text.contains("nika:jq"),
            "tool names carry the nika: namespace",
        );
        assert!(
            text.contains("--json"),
            "the human surface teaches the machine surface",
        );
    }
}
