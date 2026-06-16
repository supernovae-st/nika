// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The MCP tool catalog — Nika's STATIC, read-only surface exposed as Model
//! Context Protocol tools. Every tool here is PURE static analysis over its
//! arguments (parse · check · code lookup) — zero effects, zero network, no
//! workflow ever RUNS through MCP (running needs the effect-permits boundary ·
//! out of scope for the read-only server surface). That purity is what makes a
//! tool safe to expose to any connecting client (Cursor · Claude Desktop · …)
//! and lets the whole server be unit-tested as a function.

use std::fmt::Write as _;

use serde_json::{Value, json};

/// The tool catalog (`tools/list`): name · description · `inputSchema`
/// (JSON Schema the client validates arguments against).
#[must_use]
pub(crate) fn catalog() -> Value {
    json!([
        {
            "name": "nika_check",
            "description": "Statically audit a Nika workflow (schema · DAG · CEL · \
                            effects · permits · cost) BEFORE running it. Returns the \
                            findings, or a clean verdict — auditable before a token is spent.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workflow": {
                        "type": "string",
                        "description": "The *.nika.yaml workflow source."
                    }
                },
                "required": ["workflow"]
            }
        },
        {
            "name": "nika_explain",
            "description": "Teach one Nika error code — cause, category, and the fix form.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "A code like `NIKA-VAR-001` or the bare `440`."
                    }
                },
                "required": ["code"]
            }
        }
    ])
}

/// Execute a tool by name. `Ok(text)` is the success content; `Err(text)` is a
/// tool-level error (surfaced as `isError: true`, NOT a protocol error). PURE.
pub(crate) fn execute(name: &str, args: &Value) -> Result<String, String> {
    match name {
        "nika_check" => check(args),
        "nika_explain" => explain(args),
        other => Err(format!(
            "unknown tool `{other}` — nika exposes `nika_check` and `nika_explain`"
        )),
    }
}

/// `nika_check` — parse + the static check ladder over the supplied YAML.
fn check(args: &Value) -> Result<String, String> {
    let yaml = args
        .get("workflow")
        .and_then(Value::as_str)
        .ok_or("missing `workflow` (the *.nika.yaml source)")?;
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .map_err(|e| format!("PARSE ✗ {e}"))?;
    let report = nika_schema::check(&wf);
    if report.is_clean() {
        return Ok("✔ clean — audited before a single token was spent".to_owned());
    }
    let mut s = String::from("✖ findings:\n");
    for c in &report.conformance {
        let _ = writeln!(s, "  [{}] {}", c.code, c.message);
    }
    Ok(s)
}

/// `nika_explain` — teach one error code (numeric registry or spec code).
fn explain(args: &Value) -> Result<String, String> {
    let code = args
        .get("code")
        .and_then(Value::as_str)
        .ok_or("missing `code` (e.g. `NIKA-VAR-001`)")?;
    let normalized = if code.starts_with("NIKA-") {
        code.to_owned()
    } else {
        format!("NIKA-{code}")
    };
    // 1 · the numeric crate registry (`NIKA-440` · engine codes · code_help).
    if let Some(c) = nika_error::codes::lookup(&normalized) {
        return Ok(nika_error::codes::code_help(c).to_owned());
    }
    // 2 · the spec conformance codes (`NIKA-VAR-*` · `NIKA-DAG-*` · what `nika
    //     check` emits) from the embedded canon — the SSOT row, no network.
    if let Some(row) = nika_pack::error_codes()
        .into_iter()
        .find(|r| r.code == normalized)
    {
        return Ok(format!(
            "{normalized} · {} · transient: {}\n\n  {}",
            row.category, row.transient, row.failure
        ));
    }
    Err(format!(
        "unknown code `{code}` — the registry knows NIKA-001..9999 + the spec \
         codes (NIKA-VAR-* · NIKA-DAG-* · …); see docs.nika.sh/errors"
    ))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn catalog_lists_the_two_static_tools() {
        let c = catalog();
        let names: Vec<&str> = c
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(names, ["nika_check", "nika_explain"]);
        // Each tool carries a JSON-Schema inputSchema (the client validates args).
        for t in c.as_array().expect("array") {
            assert_eq!(t["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn check_a_clean_workflow_is_ok() {
        let wf = "nika: v1\nworkflow: t\ntasks:\n  - id: a\n    exec: { command: \"echo hi\" }\n";
        let out = execute("nika_check", &json!({ "workflow": wf })).expect("ran");
        assert!(out.contains("clean"), "{out}");
    }

    #[test]
    fn check_a_broken_workflow_lists_findings() {
        // A dangling depends_on edge — a DAG finding the ladder catches.
        let wf = "nika: v1\nworkflow: t\ntasks:\n  - id: a\n    depends_on: [ghost]\n    exec: { command: \"x\" }\n";
        let out = execute("nika_check", &json!({ "workflow": wf })).expect("ran");
        assert!(out.contains("findings") && out.contains("NIKA-"), "{out}");
    }

    #[test]
    fn check_missing_arg_is_a_tool_error() {
        assert!(execute("nika_check", &json!({})).is_err());
    }

    #[test]
    fn explain_a_known_code_teaches_it() {
        let out = execute("nika_explain", &json!({ "code": "NIKA-VAR-001" })).expect("ran");
        assert!(!out.is_empty());
        let bare = execute("nika_explain", &json!({ "code": "VAR-001" })).expect("ran");
        assert_eq!(out, bare, "the bare form normalizes to the same code");
    }

    #[test]
    fn explain_an_unknown_code_is_a_tool_error() {
        assert!(execute("nika_explain", &json!({ "code": "NIKA-GHOST-999" })).is_err());
    }

    #[test]
    fn unknown_tool_is_an_error() {
        assert!(execute("nika_nonexistent", &json!({})).is_err());
    }
}
