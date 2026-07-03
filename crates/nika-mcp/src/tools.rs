// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The MCP tool catalog — Nika's STATIC, read-only surface exposed as Model
//! Context Protocol tools. Every tool here is PURE over its arguments — static
//! analysis (parse · check · code lookup) or embedded pack data (schema ·
//! examples · templates · canon) — zero effects, zero network, no workflow
//! ever RUNS through MCP (running needs the effect-permits boundary · out of
//! scope for the read-only server surface). That purity is what makes a tool
//! safe to expose to any connecting client (Cursor · Claude Desktop · …) and
//! lets the whole server be unit-tested as a function.
//!
//! Two tool families:
//! - **validate** (`nika_check` · `nika_explain`) — the repair oracle.
//! - **learn** (`nika_schema` · `nika_examples` · `nika_template` ·
//!   `nika_canon`) — the authoring surface, so a wired agent follows the
//!   deterministic template→fill→check→repair protocol instead of guessing
//!   structure (the spec's §Writing-a-workflow path, reachable over MCP).

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
        },
        {
            "name": "nika_schema",
            "description": "The embedded JSON Schema for *.nika.yaml — the structural \
                            contract (verbs · fields · shapes) an agent authors against.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "nika_examples",
            "description": "Browse the embedded runnable examples. Without `slug`: the \
                            list. With `slug`: that example's full workflow source — read \
                            the canonical example instead of guessing a construct.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": {
                        "type": "string",
                        "description": "An example slug from the list (e.g. `pr-risk-review`)."
                    }
                }
            }
        },
        {
            "name": "nika_template",
            "description": "The 6 canonical workflow skeletons (chain · gate-and-act · \
                            fanout · …). Without `name`: the list. With `name`: that \
                            skeleton's source — copy it, fill the SLOT lines, never \
                            invent structure.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "A template name from the list (e.g. `chain`)."
                    }
                }
            }
        },
        {
            "name": "nika_canon",
            "description": "The spec canon SSOT (canon.yaml) — the locked counts and \
                            names: verbs, builtins, providers, extract modes. Cite it, \
                            never a remembered number.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

/// Execute a tool by name. `Ok(text)` is the success content; `Err(text)` is a
/// tool-level error (surfaced as `isError: true`, NOT a protocol error). PURE.
pub(crate) fn execute(name: &str, args: &Value) -> Result<String, String> {
    match name {
        "nika_check" => check(args),
        "nika_explain" => explain(args),
        "nika_schema" => Ok(nika_pack::schema_json().to_owned()),
        "nika_examples" => examples(args),
        "nika_template" => template(args),
        "nika_canon" => Ok(nika_pack::canon().to_owned()),
        other => Err(format!(
            "unknown tool `{other}` — nika exposes nika_check · nika_explain · \
             nika_schema · nika_examples · nika_template · nika_canon"
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
    // `is_clean()` checks TEN finding surfaces (conformance · secret leaks +
    // egresses · capability escapes · schema findings + lints · unknown/missing
    // /unknown args · gate findings) — render the FULL structured report so the
    // MCP client sees EVERY finding (the prior code listed only `conformance`,
    // dropping 9 classes · a model can parse the JSON + repair from it).
    let detail = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("check report serialization failed: {e}"))?;
    Ok(format!(
        "✖ findings — the workflow is not clean · the full check report:\n{detail}"
    ))
}

/// `nika_examples` — list the embedded example slugs, or return one example's
/// full workflow source (the LEARNING surface: read the canonical example for
/// a construct instead of guessing).
fn examples(args: &Value) -> Result<String, String> {
    match args.get("slug").and_then(Value::as_str) {
        None => Ok(nika_pack::example_slugs().join("\n")),
        Some(slug) => nika_pack::example(slug).map(str::to_owned).ok_or_else(|| {
            format!(
                "unknown example `{slug}` — call nika_examples without arguments \
                 for the list"
            )
        }),
    }
}

/// `nika_template` — list the canonical skeleton names, or return one
/// skeleton's source (copy · fill the `# SLOT:` lines · never invent shape).
fn template(args: &Value) -> Result<String, String> {
    match args.get("name").and_then(Value::as_str) {
        None => Ok(nika_pack::template_names().join("\n")),
        Some(name) => nika_pack::template(name).map(str::to_owned).ok_or_else(|| {
            format!(
                "unknown template `{name}` — call nika_template without arguments \
                 for the list"
            )
        }),
    }
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
    fn catalog_lists_the_validate_and_learn_tools() {
        let c = catalog();
        let names: Vec<&str> = c
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(
            names,
            [
                "nika_check",
                "nika_explain",
                "nika_schema",
                "nika_examples",
                "nika_template",
                "nika_canon"
            ]
        );
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
    fn check_surfaces_non_conformance_findings_too() {
        // A schema whose `required` key is absent from `properties` COMPILES
        // (legal JSON Schema · no PARSE-019) but is a satisfiability smell →
        // it lands in `schema_lints`, NOT `conformance`. The old code rendered
        // only `conformance` → an empty "✖ findings" body for this whole class
        // (the P1). The full-report render must surface it.
        let wf = "nika: v1\nworkflow: t\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    infer:\n      prompt: x\n      max_tokens: 10\n      schema: { type: object, properties: { a: { type: string } }, required: [b] }\n";
        let out = execute("nika_check", &json!({ "workflow": wf })).expect("ran");
        assert!(out.contains("findings"), "flags not-clean: {out}");
        assert!(
            out.contains("schema_lints") && out.contains('b'),
            "the non-conformance finding (required `b` not in properties) is rendered, \
             not dropped: {out}"
        );
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

    // ── the LEARNING surface (agents can learn, not just validate) ──────

    #[test]
    fn catalog_lists_the_learning_tools_too() {
        let c = catalog();
        let names: Vec<&str> = c
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .collect();
        for expected in [
            "nika_check",
            "nika_explain",
            "nika_schema",
            "nika_examples",
            "nika_template",
            "nika_canon",
        ] {
            assert!(names.contains(&expected), "{expected} missing: {names:?}");
        }
    }

    #[test]
    fn schema_returns_the_embedded_json_schema() {
        let out = execute("nika_schema", &json!({})).expect("ran");
        // The real schema: parses as JSON and declares the canonical $id.
        let v: Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(
            v["$id"], "https://nika.sh/spec/v1/workflow.schema.json",
            "the canonical $id travels with the schema"
        );
    }

    #[test]
    fn examples_without_slug_lists_slugs() {
        let out = execute("nika_examples", &json!({})).expect("ran");
        // The list is exactly the pack's slugs, one per line.
        let listed: Vec<&str> = out.lines().collect();
        assert_eq!(listed, nika_pack::example_slugs());
        assert!(!listed.is_empty(), "the pack ships examples");
    }

    #[test]
    fn examples_with_slug_returns_the_source() {
        let slug = nika_pack::example_slugs()
            .first()
            .cloned()
            .expect("pack has examples");
        let out = execute("nika_examples", &json!({ "slug": slug })).expect("ran");
        assert!(out.contains("nika: v1"), "a real workflow source: {out}");
    }

    #[test]
    fn examples_with_unknown_slug_is_a_tool_error_naming_the_list() {
        let err = execute("nika_examples", &json!({ "slug": "no-such-example" }))
            .expect_err("unknown slug");
        assert!(err.contains("unknown example"), "{err}");
    }

    #[test]
    fn template_without_name_lists_the_skeletons() {
        let out = execute("nika_template", &json!({})).expect("ran");
        assert!(out.contains("chain"), "the chain skeleton is listed: {out}");
    }

    #[test]
    fn template_with_name_returns_the_skeleton() {
        let out = execute("nika_template", &json!({ "name": "chain" })).expect("ran");
        assert!(
            out.contains("nika: v1") && out.contains("SLOT"),
            "a fillable skeleton with SLOT markers: {out}"
        );
    }

    #[test]
    fn canon_returns_the_ssot() {
        let out = execute("nika_canon", &json!({})).expect("ran");
        assert!(
            out.contains("verbs") && out.contains("builtins"),
            "the canon SSOT covers verbs + builtins: {out}"
        );
    }
}
