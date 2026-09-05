// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The model-facing tool catalog — one [`ToolDef`] per builtin (name +
//! description + JSON-Schema params). This is the `ToolDefinitionProvider`
//! payload the agent hands the model; the descriptions are the model's
//! only guide to WHEN to reach for each tool, so they read like the spec's
//! one-line summaries (stdlib §each builtin · cited, condensed).

use nika_cap::{HashAlgorithm, HashEncoding};
use nika_kernel::ai::provider::ToolDef;

/// A small JSON-Schema object builder — keeps the defs declarative.
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

/// Every builtin's model-facing definition — the canonical set (the
/// live count is the catalog's · asserted by the parity tests below).
#[must_use]
pub fn tool_defs() -> Vec<ToolDef> {
    let mut defs = core_defs();
    defs.extend(file_defs());
    defs.extend(data_defs());
    defs.extend(net_defs());
    defs.extend(introspection_defs());
    defs.extend(media_defs());
    defs
}

/// Version marker of the `nika tools --json` envelope. Additive-only.
pub const TOOLS_EXPORT_VERSION: u32 = 1;

/// The builtin-tool wire projection — what `nika tools --json` and the MCP
/// `nika_tools` tool emit.
///
/// One entry per builtin: the model-facing definition (`name` ·
/// `description` · `parameters` JSON-Schema, whose own `required` may carry
/// CONDITIONAL model hints for `wait`/`fetch`) JOINED with the check-time
/// contract from the catalog (`category` · declared `args` vocabulary ·
/// the UNCONDITIONALLY-`required` subset `nika check` scans for). The two
/// sides are one contract — the drift guards in this file's tests pin them.
///
/// Versioned envelope `tools_version: 1`, evolution ADDITIVE-ONLY;
/// consumers must ignore unknown fields.
#[must_use]
pub fn tools_json() -> serde_json::Value {
    let tools: Vec<serde_json::Value> = tool_defs()
        .into_iter()
        .map(|d| {
            let bare = d.name.strip_prefix("nika:").unwrap_or(&d.name);
            let entry = nika_catalog::find_builtin(bare);
            serde_json::json!({
                "name": d.name,
                "category": entry.map(|b| b.category.as_str()),
                "description": d.description,
                "parameters": d.parameters,
                "args": entry.map_or(&[][..], |b| b.args),
                "required": entry.map_or(&[][..], |b| b.required),
            })
        })
        .collect();
    serde_json::json!({
        "tools_version": TOOLS_EXPORT_VERSION,
        "tools": tools,
    })
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
                "exclude": {
                    "description": "a glob pattern or a list of them to drop from the result",
                    "oneOf": [
                        { "type": "string" },
                        { "type": "array", "items": {"type": "string"} }
                    ]
                }
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

fn hash_def() -> ToolDef {
    def(
        "hash",
        "Content hashing · blake3 (default) | sha256 | sha512 · hex (default) or base64.",
        serde_json::json!({
            // Not `type: string`. Runtime hashes a string as-is and any
            // other JSON value as compact JSON — a string-only schema
            // taught authors to `| tojson` a roster (2026-08-19).
            "content": {
                "not": { "type": "null" },
                "description": "bytes to hash — a string is hashed as-is; any other JSON value (object · array · number · bool) is compact JSON. Do not pre-pipe | tojson."
            },
            "algo": {
                "type": "string",
                "enum": HashAlgorithm::ALL.map(HashAlgorithm::as_str),
                "default": HashAlgorithm::default().as_str()
            },
            "encoding": {
                "type": "string",
                "enum": HashEncoding::ALL.map(HashEncoding::as_str),
                "default": HashEncoding::default().as_str()
            }
        }),
        &["content"],
    )
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
                "has_header": { "type": "boolean", "description": "CSV only (default true)" },
                "formula_guard": { "type": "boolean", "description": "CSV only · prefix formula-triggering cells (=+-@) with ' against spreadsheet injection · default false (alters data)" }
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
                "unit": s("seconds (default) | milliseconds | minutes | hours | days | weeks (diff)")
            }),
            &["op"],
        ),
        hash_def(),
        def(
            "decide",
            "Deterministic decision kernel (spec 11) · evaluates a portable Decision Bundle against an EvidenceSnapshot · returns the full receipt (outcome · term-by-term contributions · intervals · conflicts+witnesses · determination provenance). The LLM never decides — collect facts first, then apply the rubric here.",
            serde_json::json!({
                "bundle": { "description": "the Decision Bundle · a ./path.bundle.json string (a permits.fs read) or the inline bundle object" },
                "evidence": { "type": "object", "description": "the EvidenceSnapshot { t, evidence: [{key, value, source, integrity, digest, ...}] }" }
            }),
            &["bundle", "evidence"],
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
                "headers": { "type": "object", "description": "string map · auth rides here: { x-api-key: \"${{ secrets.KEY }}\" }" },
                "body": { "description": "request body (objects auto-JSON) · at most one of body/form/multipart" },
                "form": { "type": "object", "description": "application/x-www-form-urlencoded scalar fields · POST/PUT/PATCH only" },
                "multipart": { "type": "array", "description": "multipart/form-data parts · {name, value} text or {name, path, filename?, content_type?} file (path is permits.fs.read-gated · ≤32 MiB total) · POST/PUT/PATCH only" },
                "traverse": { "type": "object", "description": "bounded same-origin crawl · { max_pages: 1..=25 (required), respect_robots?: bool (default true) } · GET only · excludes mode/selector/jq/body/form/multipart · emits { url, page_count, pages[], assets } page digests" },
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
            "Statically check a Nika workflow draft you wrote · returns the full `nika check` verdict as JSON (conformance + secret-flow + permits + the termination/cost certificate) · NEVER executes it. Iterate until valid, then deliver the draft. Agent loops only.",
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

fn media_defs() -> Vec<ToolDef> {
    vec![
        def(
            "chart",
            "Render a DETERMINISTIC chart artifact from rows + a semantic spec (bar | line | area_band | scatter | heatmap) — byte-identical SVG saved at `out`, sha256 in outputs (the trace-chain receipt) · optional Vega-Lite sibling via compile_to. Pure compute + one permit-gated write: no network, no clock, re-runs are idempotent.",
            serde_json::json!({
                "data": s("rows · array of flat objects (strings + numbers) · or { path: <json file> }"),
                "semantics": { "type": "object", "description": "field → usd | duration_ms | tokens | count | delta | percent | timestamp | category (drives formatting + palettes · delta ⇒ diverging anchored 0)" },
                "chart": { "type": "object", "description": "{ type, x, y, y_lo?, y_hi? (area_band bounds), y2? (actual overlay), color? (series split · heatmap value), title?, width?, height? } — x/y/… are field names" },
                "out": s("artifact path ending .svg (fs-permit gated · parents created · idempotent)"),
                "compile_to": s("vega_lite — also writes the .vl.json sibling next to the svg"),
            }),
            &["data", "chart", "out"],
        ),
        def(
            "tts_generate",
            "Synthesize speech audio (local compat servers · openai gpt-4o-mini-tts · elevenlabs · mock for offline runs) — saves ONE audio file under output_dir and returns path + format + sha256 + duration (+ a provenance manifest); audio bytes never ride outputs.",
            serde_json::json!({
                "provider": s("local | openai | elevenlabs | mock (inferred from model: when omitted · local is never inferred)"),
                "model": s("provider model id · defaults: tts-1 (local · LocalAI convention) · gpt-4o-mini-tts · eleven_multilingual_v2 · mock-tts-1"),
                "text": s("the text to speak · ≤4096 chars (fan long scripts out with for_each)"),
                "voice": s("provider voice · defaults: alloy (openai/local) · Rachel's id (elevenlabs) · sine (mock)"),
                "format": s("mp3 | wav | auto (default · provider-native · the saved extension follows the bytes)"),
                "speed": { "type": "number", "description": "0.25..=4.0 (openai/local · warned-dropped elsewhere)" },
                "output_dir": s("directory the audio lands in (created if missing · fs-permit gated)"),
                "filename_prefix": s("filename stem (default: metadata.page_slug, else `speech`)"),
                "metadata": { "type": "object", "description": "free-form provenance echoed into output + manifest" },
                "manifest": { "type": "boolean", "description": "write the sidecar provenance manifest (default true)" },
                "timeout_ms": { "type": "integer", "description": "per-request cap · default 120000 (local 300000) · max 600000" },
            }),
            &["text", "output_dir"],
        ),
        def(
            "image_generate",
            "Generate image assets (local compat servers · openai gpt-image-2 · gemini gemini-3.1-flash-image · xai grok-imagine-image · mock for offline runs) — saves files under output_dir and returns paths + dimensions + sha256 (+ a provenance manifest); image bytes never ride outputs.",
            serde_json::json!({
                "provider": s("local | openai | gemini | xai | mock (inferred from model: when omitted · local is never inferred)"),
                "model": s("provider model id · defaults: stablediffusion (local · LocalAI convention) · gpt-image-2 · gemini-3.1-flash-image · grok-imagine-image · mock-image-1"),
                "prompt": s("the creative brief the provider renders"),
                "mode": s("generate (default · text→image) | edit (source image(s) + instruction)"),
                "image": s("mode:edit · one source image PATH (read · permit-gated fs.read)"),
                "images": s("mode:edit · source image PATHS (multi-ref · capped per provider)"),
                "mask": s("mode:edit · optional pixel-mask PATH (openai/local only · refused elsewhere)"),
                "n": { "type": "integer", "description": "variant count 1..=10 (gemini runs n sequential calls)" },
                "aspect_ratio": s("1:1 | 16:9 | 9:16 | 4:3 | 3:4 | 3:2 | 2:3 | 21:9"),
                "size": s("exact WIDTHxHEIGHT (gpt-image-2 arbitrary /16 sizes · gemini folds to a size class) or auto — an exact size wins over aspect_ratio"),
                "quality": s("auto (default) | low | medium | high | ultra (folds to high on openai · model-tier on gemini)"),
                "format": s("png (default) | jpeg | webp — magic bytes are the authority on what lands"),
                "compression": { "type": "integer", "description": "0..=100 · jpeg/webp only (openai output_compression)" },
                "background": s("auto (default) | transparent (needs png/webp and a supporting model) | opaque"),
                "seed": { "type": "integer", "description": "gemini best-effort · openai has no seed (warned + dropped)" },
                "reference_images": { "type": "array", "items": {"type": "string"}, "description": "media roadmap — rejected loudly in V1" },
                "provider_options": { "type": "object", "description": "vetted pass-through · openai {moderation, user} · gemini {thinking_level, image_size} · xai {user, resolution}" },
                "output_dir": s("directory the assets land in (rides the declared permits.fs boundary)"),
                "filename_prefix": s("filename stem (else metadata.page_slug, else `image`) — sanitized, traversal-free"),
                "save": { "type": "boolean", "description": "V1 contract: stays true — assets land on disk, never inline" },
                "manifest": { "type": "boolean", "description": "write the provenance manifest JSON beside the assets (default true)" },
                "metadata": { "type": "object", "description": "free provenance fields (campaign · page_slug · locale …) echoed to output + manifest" },
                "timeout_ms": { "type": "integer", "description": "per-request deadline · default 180000 · 1000..=600000" },
                "debug": { "type": "boolean", "description": "echo the sanitized raw provider response (base64 stripped)" }
            }),
            &["prompt", "output_dir"],
        ),
        def(
            "image_fx",
            "Apply deterministic artistic effects to a PNG (dither · palette · duotone · pixelate · halftone · grain · vignette · chromatic_aberration · scanlines · glitch · ascii) — byte-identical forever for identical input+args; the recipe rides the artifact (tEXt) and outputs carry path + sha256, bytes never inline.",
            serde_json::json!({
                "input": s("source PNG path (read · permit-gated fs.read · PNG v1: produce upstream via image_generate format png)"),
                "out": s("output artifact path (.png · .txt/.ans for ascii text/ansi emits) — rides the declared permits.fs boundary"),
                "ops": { "type": "array", "description": "ordered single-key op maps · resize{width,height,filter} · crop{x,y,width,height} · levels{brightness,contrast} · grayscale · palette_map{palette} · dither{mode,palette} · duotone{dark,light} · pixelate{block} · halftone{cell,angle} · grain{intensity} · vignette{strength} · chromatic_aberration{shift} · scanlines{strength,period} · glitch{line_shift,channel_shift,blocks} · ascii{cols,emit} (ascii must be last)" },
                "seed": { "type": "integer", "description": "stochastic-op seed (grain · glitch) · default 0 · the seed IS the style" }
            }),
            &["input", "out", "ops"],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_json_is_the_versioned_total_join() {
        let payload = tools_json();
        assert_eq!(payload["tools_version"], 1, "the locked v1 wire marker");
        let tools = payload["tools"].as_array().expect("tools array");
        assert_eq!(
            tools.len(),
            tool_defs().len(),
            "every model-facing def is projected",
        );
        assert_eq!(
            tools.len(),
            nika_catalog::all_builtins().len(),
            "the join is total — defs and catalog are the same set",
        );
    }

    #[test]
    fn every_projected_tool_carries_the_joined_contract() {
        const CATEGORIES: [&str; 6] = ["core", "file", "data", "network", "introspection", "media"];
        let payload = tools_json();
        for tool in payload["tools"].as_array().expect("tools array") {
            let name = tool["name"].as_str().expect("name string");
            assert!(name.starts_with("nika:"), "`{name}` misses the namespace");
            let category = tool["category"]
                .as_str()
                .unwrap_or_else(|| panic!("`{name}`: category missing from the join"));
            assert!(
                CATEGORIES.contains(&category),
                "`{name}`: category `{category}` outside the closed set",
            );
            assert!(
                !tool["description"]
                    .as_str()
                    .expect("description")
                    .is_empty(),
                "`{name}`: empty description",
            );
            assert_eq!(
                tool["parameters"]["type"], "object",
                "`{name}`: parameters must be the JSON-Schema object",
            );
            let args: Vec<&str> = tool["args"]
                .as_array()
                .expect("args array")
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            for req in tool["required"].as_array().expect("required array") {
                let key = req.as_str().expect("required key string");
                assert!(
                    args.contains(&key),
                    "`{name}`: required `{key}` outside the declared args",
                );
            }
        }
    }

    #[test]
    fn the_catalog_is_the_canonical_26_with_no_dupes() {
        let defs = tool_defs();
        assert_eq!(
            defs.len(),
            28,
            "stdlib ships exactly 28 (22 Rams-swept + nika:compose ADR-096 + \
             nika:image_generate stdlib §Media + nika:tts_generate stdlib §Audio + \
             nika:image_fx stdlib §Media + nika:decide spec 11 W-DEC)"
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

    /// The vNext fetch families are DECLARED — a third-source pin. The
    /// defs↔catalog equality guard cannot see a key missing from BOTH
    /// sides (empirical 2026-07-07: `traverse` shipped in the runtime +
    /// shape checker while both declaration lists lacked it, and every
    /// drift test stayed green — the merged binary then refused every
    /// traverse workflow at ARGS).
    #[test]
    fn fetch_declares_the_vnext_payload_families() {
        let fetch = nika_catalog::find_builtin("fetch").expect("fetch in catalog");
        for key in ["form", "multipart", "traverse"] {
            assert!(fetch.args.contains(&key), "catalog lost fetch `{key}`");
        }
    }

    #[test]
    fn arg_keys_match_the_catalog_vocabulary_no_drift() {
        // THE DRIFT GUARD (Finding #6): the model-facing `ToolDef`
        // properties here and the `Builtin::args` vocabulary `nika check`
        // validates against (nika-catalog) are two views of ONE contract.
        // If they diverge, the checker would flag a valid arg OR miss a
        // typo — so a mismatch fails HERE, in the crate that owns the
        // schemas, before any release.
        use std::collections::BTreeSet;
        for d in tool_defs() {
            let name = d.name.strip_prefix("nika:").expect("nika: prefix");
            let catalog = nika_catalog::find_builtin(name)
                .unwrap_or_else(|| panic!("`{name}` missing from the catalog"));
            let def_keys: BTreeSet<&str> = d.parameters["properties"]
                .as_object()
                .expect("properties object")
                .keys()
                .map(String::as_str)
                .collect();
            let catalog_keys: BTreeSet<&str> = catalog.args.iter().copied().collect();
            assert_eq!(
                def_keys, catalog_keys,
                "`nika:{name}` arg keys drifted: ToolDef vs catalog Builtin::args"
            );
        }
    }

    #[test]
    fn required_keys_match_the_catalog_required_no_drift() {
        // The required-arg drift guard (F5): the JSON-Schema `required`
        // array each `ToolDef` declares and the catalog `Builtin::required`
        // set `nika check`'s missing-args scan reads are ONE contract.
        // EXCEPTION: the two CONDITIONAL-contract builtins (`wait` ·
        // `fetch`) keep `Builtin::required` empty by design — their `url`/
        // XOR/mode rules live in `analyzer::builtin_shape`, not the flat
        // set — so their ToolDef `required` (a model hint) is allowed to
        // differ. Every OTHER builtin must match exactly.
        use std::collections::BTreeSet;
        const CONDITIONAL: [&str; 2] = ["wait", "fetch"];
        for d in tool_defs() {
            let name = d.name.strip_prefix("nika:").expect("nika: prefix");
            let catalog = nika_catalog::find_builtin(name)
                .unwrap_or_else(|| panic!("`{name}` missing from the catalog"));
            let def_required: BTreeSet<&str> = d.parameters["required"]
                .as_array()
                .expect("required array")
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            if CONDITIONAL.contains(&name) {
                assert!(
                    catalog.required.is_empty(),
                    "`nika:{name}` is conditional — Builtin::required must stay empty"
                );
                continue;
            }
            let catalog_required: BTreeSet<&str> = catalog.required.iter().copied().collect();
            assert_eq!(
                def_required, catalog_required,
                "`nika:{name}` required keys drifted: ToolDef vs catalog Builtin::required"
            );
        }
    }

    #[test]
    fn hash_content_is_not_typed_as_string_only() {
        // Check-time / model-facing schema must not contradict the
        // runtime: an object-shaped task output is valid `content:`.
        let hash = tool_defs()
            .into_iter()
            .find(|d| d.name == "nika:hash")
            .expect("hash is catalogued");
        let content = &hash.parameters["properties"]["content"];
        assert_ne!(
            content.get("type"),
            Some(&serde_json::json!("string")),
            "type:string taught authors to | tojson a roster"
        );
        let desc = content["description"].as_str().expect("description");
        assert!(
            desc.contains("compact JSON"),
            "schema must say objects hash: {desc}"
        );
        assert!(
            desc.contains("tojson"),
            "schema must name the pre-pass not to take: {desc}"
        );
    }
}
