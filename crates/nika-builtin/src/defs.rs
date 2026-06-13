// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The model-facing tool catalog — one [`ToolDef`] per builtin (name +
//! description + JSON-Schema params). This is the `ToolDefinitionProvider`
//! payload the agent hands the model; the descriptions are the model's
//! only guide to WHEN to reach for each tool, so they read like the spec's
//! one-line summaries (stdlib §each builtin · cited, condensed).

use nika_kernel::ai::provider::ToolDef;

/// A small JSON-Schema object builder — keeps the 22 defs declarative.
fn schema(properties: serde_json::Value, required: &[&str]) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "type".to_owned(),
        serde_json::Value::String("object".to_owned()),
    );
    obj.insert("properties".to_owned(), properties);
    obj.insert(
        "required".to_owned(),
        serde_json::Value::Array(
            required
                .iter()
                .map(|r| serde_json::Value::String((*r).to_owned()))
                .collect(),
        ),
    );
    serde_json::Value::Object(obj)
}

fn s(desc: &str) -> serde_json::Value {
    serde_json::json!({ "type": "string", "description": desc })
}

fn def(name: &str, desc: &str, properties: serde_json::Value, required: &[&str]) -> ToolDef {
    ToolDef::new(format!("nika:{name}"), desc, schema(properties, required))
}

/// Every builtin's model-facing definition — the canonical 23 (core 6 +
/// file 5 + data 8 + network 2 + introspection 2).
#[must_use]
pub fn tool_defs() -> Vec<ToolDef> {
    let mut defs = core_defs();
    defs.extend(file_defs());
    defs.extend(data_defs());
    defs.extend(net_defs());
    defs.extend(introspection_defs());
    defs
}

fn core_defs() -> Vec<ToolDef> {
    vec![
        // ── core 6 ──────────────────────────────────────────────────────
        def(
            "log",
            "Emit a human-readable diagnostic log line to the run stream (best-effort · never fails).",
            serde_json::json!({
                "level": s("debug | info | warn | error (default info)"),
                "message": s("the log message"),
                "data": { "description": "optional structured context (any JSON)" }
            }),
            &["message"],
        ),
        def(
            "emit",
            "Emit a custom machine event for subscribers/journal (distinct from log).",
            serde_json::json!({
                "event_type": s("^[a-z][a-z0-9_.-]*$"),
                "payload": { "description": "any JSON value" }
            }),
            &["event_type"],
        ),
        def(
            "assert",
            "Fail-fast guard: fail the task when condition is false (else no-op).",
            serde_json::json!({
                "condition": { "type": "boolean", "description": "the resolved boolean to check" },
                "message": s("failure message")
            }),
            &["condition"],
        ),
        def(
            "prompt",
            "Ask a human (confirm|input|choice). Blocks until answered; uses default when headless.",
            serde_json::json!({
                "mode": s("confirm (default) | input | choice"),
                "message": s("the question"),
                "choices": { "type": "array", "items": {"type": "string"}, "description": "choice mode only" },
                "default": { "description": "fallback when no human can answer" }
            }),
            &["message"],
        ),
        def(
            "done",
            "Mark the current agent loop complete (the loop-completion sentinel · agent loops only).",
            serde_json::json!({ "result": { "description": "optional final value (any JSON)" } }),
            &[],
        ),
        def(
            "wait",
            "Pause: relative duration (e.g. \"30s\") XOR absolute until (ISO 8601). Exactly one.",
            serde_json::json!({
                "duration": s("Go-duration string · relative"),
                "until": s("ISO 8601 timestamp · absolute"),
                "timeout": s("optional cap for absolute wait")
            }),
            &[],
        ),
    ]
}

fn file_defs() -> Vec<ToolDef> {
    vec![
        def(
            "read",
            "Read a file · returns its text (or opaque bytes with binary: true).",
            serde_json::json!({
                "path": s("file path"),
                "binary": { "type": "boolean", "description": "return opaque bytes (default false)" }
            }),
            &["path"],
        ),
        def(
            "write",
            "Write a file · returns the path. overwrite defaults true · create_dirs defaults false.",
            serde_json::json!({
                "path": s("destination path"),
                "content": s("the content to write"),
                "overwrite": { "type": "boolean" },
                "create_dirs": { "type": "boolean" }
            }),
            &["path", "content"],
        ),
        def(
            "edit",
            "In-place literal find/replace (all occurrences · count caps it). NOT regex.",
            serde_json::json!({
                "path": s("file to edit"),
                "find": s("the literal string to find"),
                "replace": s("the replacement"),
                "count": { "type": "integer", "description": "cap replacements" }
            }),
            &["path", "find", "replace"],
        ),
        def(
            "glob",
            "Glob match · returns paths sorted lexicographically.",
            serde_json::json!({
                "pattern": s("e.g. ./src/**/*.rs"),
                "exclude": { "type": "array", "items": {"type": "string"} }
            }),
            &["pattern"],
        ),
        def(
            "grep",
            "Recursive regex search · returns {path,line,match} sorted by (path,line).",
            serde_json::json!({
                "pattern": s("RE2-class regex"),
                "path": s("root to search (default .)"),
                "case_insensitive": { "type": "boolean" }
            }),
            &["pattern"],
        ),
    ]
}

fn data_defs() -> Vec<ToolDef> {
    vec![
        def(
            "jq",
            "Run a jq expression over input · THE data-transform language (map·filter·select·reshape). Emits exactly one value.",
            serde_json::json!({
                "expression": s("the jq program"),
                "input": { "description": "any JSON value (or an array for multi-input ops)" }
            }),
            &["expression"],
        ),
        def(
            "json_diff",
            "JSON diff · returns an RFC 6902 JSON Patch.",
            serde_json::json!({
                "before": { "description": "the original value" },
                "after": { "description": "the new value" }
            }),
            &["before", "after"],
        ),
        def(
            "validate",
            "Validate data against a JSON Schema · returns {valid, errors} (invalid data is a report, not a failure).",
            serde_json::json!({
                "data": { "description": "the value (or a YAML string with format: yaml)" },
                "schema": { "description": "a JSON Schema" },
                "format": s("json (default) | yaml")
            }),
            &["data", "schema"],
        ),
        def(
            "json_merge_patch",
            "RFC 7396 merge patch (null deletes a key) over a target object.",
            serde_json::json!({
                "target": { "type": "object" },
                "patch": { "type": "object" }
            }),
            &["target", "patch"],
        ),
        def(
            "convert",
            "Convert between json·yaml·toml·csv (from/to · identity rejected).",
            serde_json::json!({
                "input": { "description": "the data (string for text formats)" },
                "from": s("json|yaml|toml|csv"),
                "to": s("json|yaml|toml|csv"),
                "has_header": { "type": "boolean", "description": "CSV only (default true)" }
            }),
            &["input", "from", "to"],
        ),
        def(
            "uuid",
            "Generate a UUID · v7 (default · sortable) or v4 (random).",
            serde_json::json!({ "version": s("v7 (default) | v4") }),
            &[],
        ),
        def(
            "date",
            "Timestamp arithmetic · op-discriminated (now|add|subtract|format|parse|diff) · strftime grammar · ISO 8601 out.",
            serde_json::json!({
                "op": s("now | add | subtract | format | parse | diff"),
                "tz": s("IANA timezone (now · default UTC)"),
                "base": s("ISO 8601 base (add/subtract)"),
                "duration": s("ISO 8601 span like PT1h (add/subtract)"),
                "input": s("the timestamp to render (format) or the text to read (parse)"),
                "format": s("strftime grammar e.g. %Y-%m-%d (format/parse)"),
                "start": s("ISO 8601 (diff)"),
                "end": s("ISO 8601 (diff)"),
                "unit": s("seconds (default) | milliseconds | minutes | hours | days (diff)")
            }),
            &["op"],
        ),
        def(
            "hash",
            "Content hashing · blake3 (default) | sha256 | sha512 · hex (default) or base64.",
            serde_json::json!({
                "content": s("the content to hash"),
                "algo": s("blake3 | sha256 | sha512"),
                "encoding": s("hex | base64")
            }),
            &["content"],
        ),
    ]
}

fn net_defs() -> Vec<ToolDef> {
    vec![
        def(
            "fetch",
            "HTTP request + content extraction · returns the extracted body (non-2xx is an error). SSRF-defended.",
            serde_json::json!({
                "url": s("the URL"),
                "method": s("GET (default) | POST | PUT | DELETE | PATCH | HEAD"),
                "headers": { "type": "object" },
                "body": { "description": "request body (objects auto-JSON)" },
                "mode": s("markdown (default) | article | text | selector | jq | metadata | links | feed | sitemap | raw"),
                "selector": s("CSS selector (mode: selector only)"),
                "jq": s("a jq expression (mode: jq only · the one data language)")
            }),
            &["url"],
        ),
        def(
            "notify",
            "Send a notification · v0.1 engines support the webhook channel.",
            serde_json::json!({
                "channel": s("webhook (v0.1) | slack | email | discord | sms (gated)"),
                "target": s("the webhook URL"),
                "message": s("the alert text"),
                "severity": s("info (default) | warn | error"),
                "data": { "description": "optional structured context (any JSON · rides the payload beside the message)" }
            }),
            &["target", "message"],
        ),
    ]
}

fn introspection_defs() -> Vec<ToolDef> {
    vec![
        def(
            "compose",
            "Statically check a Nika workflow draft you wrote · returns the full              `nika check` verdict as JSON (conformance + secret-flow + permits +              the termination/cost certificate) · NEVER executes it. Iterate until              valid, then deliver the draft. Agent loops only.",
            serde_json::json!({ "workflow_yaml": s("the complete workflow YAML draft") }),
            &["workflow_yaml"],
        ),
        def(
            "inspect",
            "Introspect the running workflow · view: cost | records | dag_info | threads.",
            serde_json::json!({ "view": s("cost | records | dag_info | threads") }),
            &["view"],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_the_canonical_23_with_no_dupes() {
        let defs = tool_defs();
        assert_eq!(
            defs.len(),
            23,
            "stdlib v0.1 ships exactly 23 (+ nika:compose · ADR-093)"
        );
        let mut names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "no duplicate builtin names");
        assert!(defs.iter().all(|d| d.name.starts_with("nika:")));
    }

    #[test]
    fn every_def_carries_a_valid_schema_object() {
        for d in tool_defs() {
            assert_eq!(d.parameters["type"], "object", "{}", d.name);
            assert!(d.parameters["properties"].is_object(), "{}", d.name);
            assert!(d.parameters["required"].is_array(), "{}", d.name);
            assert!(!d.description.is_empty(), "{}", d.name);
            // every `required` key must be a declared property.
            let props = d.parameters["properties"].as_object().expect("props");
            for req in d.parameters["required"].as_array().expect("req") {
                let key = req.as_str().expect("string key");
                assert!(
                    props.contains_key(key),
                    "{} requires undeclared {key}",
                    d.name
                );
            }
        }
    }

    #[test]
    fn string_property_schema_carries_type_and_description() {
        // Pins the `s()` helper (a `Default` mutant returns Null, not a
        // typed string property).
        let prop = s("a helpful description");
        assert_eq!(prop["type"], "string");
        assert_eq!(prop["description"], "a helpful description");
    }
}
