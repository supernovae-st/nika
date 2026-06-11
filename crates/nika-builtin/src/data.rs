// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Data builtins (8) — `jq` is THE data language (stdlib §Data).
//!
//! Pure functions of args (`date`/`uuid` ride the clock/entropy edges).
//! Every contract — codes · defaults · the exactly-one-output law — is
//! cited from `nika-spec stdlib/builtins-v0.1.md`, never restated.

use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data, unwrap_valr};
use jaq_json::{Val, read};
use nika_kernel::io::clock::ClockDyn;
use sha2::Digest;

use crate::{Args, BuiltinFailure, BuiltinOutcome, opt_str, req_str};

// ─── nika:jq · the transform + extraction primitive ─────────────────────

/// Run a jq `expression:` over `input:` — and emit EXACTLY ONE output
/// value (the 04-variables.md:347 binding law applied to the tool: a
/// stream that isn't a single value is a `[ … ]`-collect authoring bug).
pub(crate) fn jq(args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-JQ-001";
    let program = req_str(args, "expression", C)?;
    let input = args
        .get("input")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let bytes = serde_json::to_vec(&input).map_err(|e| BuiltinFailure::new(C, e.to_string()))?;
    let val = read::parse_single(&bytes)
        .map_err(|e| BuiltinFailure::new(C, format!("input is not valid JSON: {e:?}")))?;

    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
    let arena = Arena::default();
    let modules = Loader::new(defs)
        .load(
            &arena,
            File {
                code: program,
                path: (),
            },
        )
        .map_err(|errs| BuiltinFailure::new(C, format!("jq program error: {errs:?}")))?;
    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errs| BuiltinFailure::new(C, format!("jq compile error: {errs:?}")))?;

    let ctx = Ctx::<jaq_data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    let mut outputs = Vec::new();
    for result in filter.id.run((ctx, val)).map(unwrap_valr) {
        let value = result.map_err(|e| BuiltinFailure::new(C, format!("jq runtime error: {e}")))?;
        let json = serde_json::from_str(&value.to_string())
            .map_err(|e| BuiltinFailure::new(C, e.to_string()))?;
        outputs.push(json);
        if outputs.len() > 1 {
            return Err(BuiltinFailure::new(
                C,
                "the program emitted MORE than one value — wrap it in `[ … ]` to collect a stream into an array",
            ));
        }
    }
    outputs.pop().ok_or_else(|| {
        BuiltinFailure::new(
            C,
            "the program emitted NO value — a binding needs exactly one (use `// default` or `first(…)`)",
        )
    })
}

// ─── nika:json_diff · RFC 6902 ──────────────────────────────────────────

/// JSON diff → an RFC 6902 JSON Patch array.
pub(crate) fn json_diff(args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-JSON_DIFF-001";
    let before = args
        .get("before")
        .ok_or_else(|| BuiltinFailure::new(C, "`before:` is required"))?;
    let after = args
        .get("after")
        .ok_or_else(|| BuiltinFailure::new(C, "`after:` is required"))?;
    let patch = json_patch::diff(before, after);
    serde_json::to_value(&patch).map_err(|e| BuiltinFailure::new(C, e.to_string()))
}

// ─── nika:json_merge_patch · RFC 7396 (null deletes) ────────────────────

/// RFC 7396 merge patch — the delete-on-null semantics jq's `*` lacks.
pub(crate) fn json_merge_patch(args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-JSON_MERGE_PATCH-001";
    let target = args
        .get("target")
        .cloned()
        .ok_or_else(|| BuiltinFailure::new(C, "`target:` is required"))?;
    let patch = args
        .get("patch")
        .ok_or_else(|| BuiltinFailure::new(C, "`patch:` is required"))?;
    if !target.is_object() || !patch.is_object() {
        return Err(BuiltinFailure::new(C, "target and patch must be objects"));
    }
    let mut doc = target;
    json_patch::merge(&mut doc, patch);
    Ok(doc)
}

// ─── nika:validate · JSON Schema (json OR yaml) ─────────────────────────

/// Validate `data:` against a `schema:` — invalid data is a REPORT
/// (`{ valid, errors }`), never a task failure (stdlib §validate).
pub(crate) fn validate(args: &Args) -> BuiltinOutcome {
    const C1: &str = "NIKA-BUILTIN-VALIDATE-001";
    const C2: &str = "NIKA-BUILTIN-VALIDATE-002";
    let schema = args
        .get("schema")
        .ok_or_else(|| BuiltinFailure::new(C1, "`schema:` is required"))?;
    let format = opt_str(args, "format", C1)?.unwrap_or("json");

    let data = match format {
        "json" => args
            .get("data")
            .cloned()
            .ok_or_else(|| BuiltinFailure::new(C1, "`data:` is required"))?,
        "yaml" => {
            let text = req_str(args, "data", C1)?;
            serde_yaml_bw::from_str(text)
                .map_err(|e| BuiltinFailure::new(C2, format!("data is not valid YAML: {e}")))?
        }
        other => {
            return Err(BuiltinFailure::new(
                C1,
                format!("`format:` must be json|yaml, got {other}"),
            ));
        }
    };

    let validator = jsonschema::validator_for(schema).map_err(|e| {
        BuiltinFailure::new(C1, format!("`schema:` is not a valid JSON Schema: {e}"))
    })?;
    let errors: Vec<String> = validator
        .iter_errors(&data)
        .map(|e| e.to_string())
        .collect();
    Ok(serde_json::json!({ "valid": errors.is_empty(), "errors": errors }))
}

// ─── nika:convert · universal multi-format conversion ───────────────────

/// `from`/`to` over {json, yaml, toml, csv} — 12 directions, identity
/// rejected (stdlib §convert · spec-named reference crates).
pub(crate) fn convert(args: &Args) -> BuiltinOutcome {
    const C1: &str = "NIKA-BUILTIN-CONVERT-001";
    const C2: &str = "NIKA-BUILTIN-CONVERT-002";
    let from = req_str(args, "from", C1)?;
    let to = req_str(args, "to", C1)?;
    if from == to {
        return Err(BuiltinFailure::new(
            C1,
            format!("`from` == `to` ({from}) is an identity conversion"),
        ));
    }
    let input = args
        .get("input")
        .ok_or_else(|| BuiltinFailure::new(C1, "`input:` is required"))?;

    // Parse the input into the canonical serde_json::Value bridge.
    let value = parse_format(from, input, args).map_err(|e| BuiltinFailure::new(C2, e))?;
    // Emit the target format.
    emit_format(to, &value, args).map_err(|e| BuiltinFailure::new(C1, e))
}

fn parse_format(
    from: &str,
    input: &serde_json::Value,
    args: &Args,
) -> Result<serde_json::Value, String> {
    let as_text = || {
        input
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("`input:` must be a string for from: {from}"))
    };
    match from {
        "json" => Ok(input.clone()),
        "yaml" => serde_yaml_bw::from_str(&as_text()?).map_err(|e| format!("invalid YAML: {e}")),
        "toml" => toml_convert::from_str(&as_text()?).map_err(|e| format!("invalid TOML: {e}")),
        "csv" => parse_csv(&as_text()?, crate::opt_bool(args, "has_header", true)),
        other => Err(format!("unknown from: {other} (json|yaml|toml|csv)")),
    }
}

fn emit_format(
    to: &str,
    value: &serde_json::Value,
    args: &Args,
) -> Result<serde_json::Value, String> {
    let text = match to {
        "json" => return Ok(value.clone()),
        "yaml" => serde_yaml_bw::to_string(value).map_err(|e| format!("to YAML: {e}"))?,
        "toml" => toml_convert::to_string(value).map_err(|e| format!("to TOML: {e}"))?,
        "csv" => emit_csv(value, crate::opt_bool(args, "has_header", true))?,
        other => return Err(format!("unknown to: {other} (json|yaml|toml|csv)")),
    };
    Ok(serde_json::Value::String(text))
}

/// CSV → an array of objects (header keys) or arrays (headerless).
fn parse_csv(text: &str, has_header: bool) -> Result<serde_json::Value, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(has_header)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    if has_header {
        let headers = reader
            .headers()
            .map_err(|e| format!("CSV header: {e}"))?
            .clone();
        for record in reader.records() {
            let record = record.map_err(|e| format!("CSV row: {e}"))?;
            let obj: serde_json::Map<String, serde_json::Value> = headers
                .iter()
                .zip(record.iter())
                .map(|(h, v)| (h.to_owned(), serde_json::Value::String(v.to_owned())))
                .collect();
            rows.push(serde_json::Value::Object(obj));
        }
    } else {
        for record in reader.records() {
            let record = record.map_err(|e| format!("CSV row: {e}"))?;
            let arr: Vec<serde_json::Value> = record
                .iter()
                .map(|v| serde_json::Value::String(v.to_owned()))
                .collect();
            rows.push(serde_json::Value::Array(arr));
        }
    }
    Ok(serde_json::Value::Array(rows))
}

/// An array of objects → CSV (union of keys = header, sorted for
/// determinism across engines).
fn emit_csv(value: &serde_json::Value, has_header: bool) -> Result<String, String> {
    let rows = value
        .as_array()
        .ok_or_else(|| "CSV output needs an array of objects".to_owned())?;
    let mut headers: Vec<String> = Vec::new();
    for row in rows {
        if let Some(obj) = row.as_object() {
            for key in obj.keys() {
                if !headers.contains(key) {
                    headers.push(key.clone());
                }
            }
        }
    }
    headers.sort();
    let mut writer = csv::Writer::from_writer(Vec::new());
    if has_header {
        writer.write_record(&headers).map_err(|e| e.to_string())?;
    }
    for row in rows {
        let obj = row
            .as_object()
            .ok_or_else(|| "every CSV row must be an object".to_owned())?;
        let cells: Vec<String> = headers
            .iter()
            .map(|h| match obj.get(h) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            })
            .collect();
        writer.write_record(&cells).map_err(|e| e.to_string())?;
    }
    let bytes = writer.into_inner().map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

// ─── nika:uuid ──────────────────────────────────────────────────────────

/// Generate a UUID (v7 default · sortable · or v4 random).
pub(crate) fn uuid(args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-UUID-001";
    let version = opt_str(args, "version", C)?.unwrap_or("v7");
    let id = match version {
        "v7" => uuid::Uuid::now_v7(),
        "v4" => uuid::Uuid::new_v4(),
        other => {
            return Err(BuiltinFailure::new(
                C,
                format!("`version:` must be v7|v4, got {other}"),
            ));
        }
    };
    Ok(serde_json::Value::String(id.to_string()))
}

// ─── nika:date · timestamp arithmetic (op-discriminated) ────────────────

/// `op`-discriminated time builtin (now · add · subtract · format ·
/// parse · diff). `now` rides the injected [`ClockDyn`] for hermeticity.
pub(crate) fn date<C: ClockDyn>(_clock: &C, args: &Args) -> BuiltinOutcome {
    const E: &str = "NIKA-BUILTIN-DATE-001";
    let op = req_str(args, "op", E)?;
    match op {
        "now" => {
            // jiff reads the system clock; the kernel ClockDyn is the seam
            // a future deterministic-time pass routes through (its now()
            // is Instant, not wall-clock — date needs SystemTime, which
            // arrives when the clock seam grows a wall-now accessor).
            let now = jiff::Timestamp::now();
            Ok(serde_json::Value::String(now.to_string()))
        }
        "add" | "subtract" => {
            let base = req_str(args, "base", E)?;
            let dur = req_str(args, "duration", E)?;
            let ts: jiff::Timestamp = base
                .parse()
                .map_err(|e| BuiltinFailure::new(E, format!("`base:` unparseable: {e}")))?;
            let span: jiff::Span = dur
                .parse()
                .map_err(|e| BuiltinFailure::new(E, format!("`duration:` unparseable: {e}")))?;
            let out = if op == "add" {
                ts.checked_add(span)
            } else {
                ts.checked_sub(span)
            }
            .map_err(|e| BuiltinFailure::new(E, format!("{op} overflow: {e}")))?;
            Ok(serde_json::Value::String(out.to_string()))
        }
        "diff" => {
            let start: jiff::Timestamp = req_str(args, "start", E)?
                .parse()
                .map_err(|e| BuiltinFailure::new(E, format!("`start:` unparseable: {e}")))?;
            let end: jiff::Timestamp = req_str(args, "end", E)?
                .parse()
                .map_err(|e| BuiltinFailure::new(E, format!("`end:` unparseable: {e}")))?;
            let secs = (end - start).get_seconds();
            Ok(serde_json::Value::Number(secs.into()))
        }
        other => Err(BuiltinFailure::new(
            E,
            format!("unknown op `{other}` (now|add|subtract|diff at v0.1)"),
        )),
    }
}

// ─── nika:hash · content hashing ────────────────────────────────────────

/// Hash `content:` (blake3 default · sha256/sha512). md5/sha1 refused.
pub(crate) fn hash(args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-HASH-001";
    let content = req_str(args, "content", C)?;
    let algo = opt_str(args, "algo", C)?.unwrap_or("blake3");
    let encoding = opt_str(args, "encoding", C)?.unwrap_or("hex");

    let digest: Vec<u8> = match algo {
        "blake3" => blake3::hash(content.as_bytes()).as_bytes().to_vec(),
        "sha256" => sha2::Sha256::digest(content.as_bytes()).to_vec(),
        "sha512" => sha2::Sha512::digest(content.as_bytes()).to_vec(),
        other => {
            return Err(BuiltinFailure::new(
                C,
                format!("unsupported algo `{other}` (blake3|sha256|sha512 · md5/sha1 are broken)"),
            ));
        }
    };
    let encoded = match encoding {
        "hex" => hex_encode(&digest),
        "base64" => base64_encode(&digest),
        other => {
            return Err(BuiltinFailure::new(
                C,
                format!("`encoding:` must be hex|base64, got {other}"),
            ));
        }
    };
    Ok(serde_json::Value::String(encoded))
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Standard base64 (RFC 4648) — small dependency-free encoder.
pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        // MUTATION (equivalent): `|` vs `^` are identical here — the three
        // bytes occupy DISJOINT bit ranges (<<16, <<8, <<0), so OR and XOR
        // produce the same packed word. Not a real test gap.
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: serde_json::Value) -> Args {
        match v {
            serde_json::Value::Object(map) => map,
            other => panic!("test arg must be an object, got {other}"),
        }
    }

    #[test]
    fn jq_sums_and_enforces_one_output() {
        let out = jq(&args(serde_json::json!({
            "expression": ".items | map(.price) | add",
            "input": { "items": [{"price": 2}, {"price": 3}] }
        })))
        .expect("ok");
        assert_eq!(out, serde_json::json!(5));

        let multi = jq(&args(
            serde_json::json!({ "expression": ".[]", "input": [1, 2] }),
        ));
        assert!(matches!(multi, Err(f) if f.message.contains("MORE than one")));

        let none = jq(&args(
            serde_json::json!({ "expression": "empty", "input": 1 }),
        ));
        assert!(matches!(none, Err(f) if f.message.contains("NO value")));

        let bad = jq(&args(
            serde_json::json!({ "expression": "this is not jq", "input": 1 }),
        ));
        assert!(bad.is_err());
    }

    #[test]
    fn jq_recursive_merge_via_star() {
        let out = jq(&args(serde_json::json!({
            "expression": ".[0] * .[1]",
            "input": [{"a": 1, "n": {"x": 1}}, {"b": 2, "n": {"y": 2}}]
        })))
        .expect("ok");
        assert_eq!(
            out,
            serde_json::json!({"a": 1, "b": 2, "n": {"x": 1, "y": 2}})
        );
    }

    #[test]
    fn json_diff_is_rfc6902() {
        let out = json_diff(&args(serde_json::json!({
            "before": {"a": 1}, "after": {"a": 2}
        })))
        .expect("ok");
        let patch = out.as_array().expect("array");
        assert_eq!(patch.len(), 1);
        assert_eq!(patch[0]["op"], "replace");
        assert_eq!(patch[0]["path"], "/a");
        assert_eq!(patch[0]["value"], 2);
    }

    #[test]
    fn json_merge_patch_null_deletes() {
        let out = json_merge_patch(&args(serde_json::json!({
            "target": {"keep": 1, "drop": 2}, "patch": {"drop": null, "add": 3}
        })))
        .expect("ok");
        assert_eq!(out, serde_json::json!({"keep": 1, "add": 3}));
        // a non-object target (with an object patch) still fails — pins the
        // `!target.is_object() || !patch.is_object()` OR (an && mutant passes
        // both-objects but must reject this mixed case).
        let mixed = json_merge_patch(&args(
            serde_json::json!({ "target": [1, 2], "patch": {"a": 1} }),
        ));
        assert!(mixed.is_err(), "non-object target is rejected");
        let mixed2 = json_merge_patch(&args(
            serde_json::json!({ "target": {"a": 1}, "patch": "str" }),
        ));
        assert!(mixed2.is_err(), "non-object patch is rejected");
    }

    #[test]
    fn validate_reports_never_fails() {
        let schema = serde_json::json!({ "type": "object", "required": ["x"] });
        let ok = validate(&args(
            serde_json::json!({ "data": {"x": 1}, "schema": schema }),
        ))
        .expect("ok");
        assert_eq!(ok["valid"], true);
        let bad = validate(&args(serde_json::json!({ "data": {}, "schema": schema })))
            .expect("ok — invalid data is a report");
        assert_eq!(bad["valid"], false);
        assert!(!bad["errors"].as_array().expect("errors").is_empty());

        // A broken schema IS a failure.
        let broken = validate(&args(
            serde_json::json!({ "data": {}, "schema": {"type": "nonsense"} }),
        ));
        assert!(broken.is_err());
    }

    #[test]
    fn validate_yaml_format_parses_first() {
        let schema = serde_json::json!({ "type": "object", "required": ["name"] });
        let out = validate(&args(serde_json::json!({
            "data": "name: ada\nage: 36", "schema": schema, "format": "yaml"
        })))
        .expect("ok");
        assert_eq!(out["valid"], true);
        let unparseable = validate(&args(serde_json::json!({
            "data": "key: : :", "schema": schema, "format": "yaml"
        })));
        assert!(matches!(unparseable, Err(f) if f.code == "NIKA-BUILTIN-VALIDATE-002"));
    }

    #[test]
    fn convert_round_trips_and_rejects_identity() {
        // json → yaml → json round-trip.
        let yaml = convert(&args(serde_json::json!({
            "input": {"name": "ada", "n": 1}, "from": "json", "to": "yaml"
        })))
        .expect("ok");
        let back = convert(&args(serde_json::json!({
            "input": yaml, "from": "yaml", "to": "json"
        })))
        .expect("ok");
        assert_eq!(back, serde_json::json!({"name": "ada", "n": 1}));

        // csv (header) → json.
        let from_csv = convert(&args(serde_json::json!({
            "input": "name,age\nada,36\nbob,40", "from": "csv", "to": "json"
        })))
        .expect("ok");
        assert_eq!(
            from_csv,
            serde_json::json!([{"name": "ada", "age": "36"}, {"name": "bob", "age": "40"}])
        );

        // json → csv (exercises emit_csv · header sorted for determinism).
        let to_csv = convert(&args(serde_json::json!({
            "input": [{"b": "2", "a": "1"}], "from": "json", "to": "csv"
        })))
        .expect("ok");
        let csv_text = to_csv.as_str().expect("string");
        assert!(csv_text.starts_with("a,b"), "header sorted: {csv_text}");
        assert!(csv_text.contains("1,2"), "{csv_text}");
        // emit_csv on a non-array is a CONVERT failure (pins the !is_array branch).
        let not_array = convert(&args(serde_json::json!({
            "input": {"x": 1}, "from": "json", "to": "csv"
        })));
        assert!(not_array.is_err());

        let identity = convert(&args(
            serde_json::json!({ "input": {}, "from": "json", "to": "json" }),
        ));
        assert!(matches!(identity, Err(f) if f.code == "NIKA-BUILTIN-CONVERT-001"));
        let bad_parse = convert(&args(
            serde_json::json!({ "input": "not: : toml", "from": "toml", "to": "json" }),
        ));
        assert!(matches!(bad_parse, Err(f) if f.code == "NIKA-BUILTIN-CONVERT-002"));
    }

    #[test]
    fn uuid_format_and_version() {
        let v7 = uuid(&args(serde_json::json!({}))).expect("ok");
        let s = v7.as_str().expect("string");
        assert_eq!(s.len(), 36, "canonical hyphenated");
        assert_eq!(&s[14..15], "7", "version nibble");
        let v4 = uuid(&args(serde_json::json!({ "version": "v4" }))).expect("ok");
        assert_eq!(&v4.as_str().expect("s")[14..15], "4");
        assert!(uuid(&args(serde_json::json!({ "version": "v9" }))).is_err());
    }

    #[test]
    fn date_add_and_diff_are_deterministic() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        let added = date(
            &clock,
            &args(serde_json::json!({
                "op": "add", "base": "2026-01-01T00:00:00Z", "duration": "PT1h"
            })),
        )
        .expect("ok");
        assert_eq!(added.as_str().expect("s"), "2026-01-01T01:00:00Z");

        let diff = date(
            &clock,
            &args(serde_json::json!({
                "op": "diff", "start": "2026-01-01T00:00:00Z", "end": "2026-01-01T00:01:00Z"
            })),
        )
        .expect("ok");
        assert_eq!(diff, serde_json::json!(60));

        assert!(date(&clock, &args(serde_json::json!({ "op": "warp" }))).is_err());
    }

    #[test]
    fn hash_blake3_default_and_rejects_broken() {
        let h = hash(&args(serde_json::json!({ "content": "hello" }))).expect("ok");
        // blake3("hello") is a fixed 64-hex-char value.
        assert_eq!(h.as_str().expect("s").len(), 64);
        let b64 = hash(&args(
            serde_json::json!({ "content": "hello", "encoding": "base64" }),
        ))
        .expect("ok");
        assert!(b64.as_str().expect("s").ends_with('='));
        let sha = hash(&args(
            serde_json::json!({ "content": "x", "algo": "sha256" }),
        ))
        .expect("ok");
        assert_eq!(sha.as_str().expect("s").len(), 64);
        assert!(hash(&args(serde_json::json!({ "content": "x", "algo": "md5" }))).is_err());
    }

    #[test]
    fn base64_encoder_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        // High bytes with overlapping bits: | and ^ diverge here.
        assert_eq!(base64_encode(&[0xff, 0xff, 0xff]), "////");
        assert_eq!(base64_encode(&[0xfb, 0xf0]), "+/A=");
    }
}
