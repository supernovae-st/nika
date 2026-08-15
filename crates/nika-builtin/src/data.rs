// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Data builtins (6) — `jq` is THE data language (stdlib §Data).
//!
//! Pure functions of args (the clock/entropy-edged `date`/`uuid` live in
//! [`crate::date`]). Every contract — codes · defaults · the
//! exactly-one-output law — is cited from `nika-spec
//! stdlib/builtins-v0.1.md`, never restated.

use jaq_core::load::{Arena, Error as JqLoadError, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data, unwrap_valr};
use jaq_json::{Val, read};
use sha2::Digest;

use crate::{Args, BuiltinFailure, BuiltinOutcome, opt_str, req_str, strict_bool};

// ─── nika:jq · the transform + extraction primitive ─────────────────────

/// The rendered-output ceiling for one jq value (16 MiB) — bounds the
/// string + re-parse allocations on a model-controlled `expression:`. jaq's
/// INTERNAL cost is NOT bounded here (jaq-core 3.1.0 has no eval-budget hook +
/// `spawn_blocking` can't be cancelled): the streaming case is caught by the
/// exactly-one-value law below, but `[range(1e12)]` materializes in-jaq first.
/// Real fix = jaq step-budget or subprocess rlimit (deferred · spec §4 gaps).
const MAX_JQ_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

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
        .map_err(|e| BuiltinFailure::new(C, format!("input is not valid JSON: {e}")))?;

    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs())
        .chain(jq_std_corrections()?);
    // D-2026-08-11-N26 · the withheld natives never enter the function set, so
    // a program that reaches for the environment or the clock does not compile.
    // The SAME filter runs at the `extract:` binding seam (`nika-runtime::jq`)
    // and at the static compile-check (`nika-check::analyzer::jq_lint`) — one
    // list in `nika_cap`, three seams, no room for a silent divergence.
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs())
        .filter(|f| !nika_cap::is_withheld_jq_native(f.0));
    let arena = Arena::default();
    let modules = Loader::new(defs)
        .load(
            &arena,
            File {
                code: program,
                path: (),
            },
        )
        .map_err(|errs| {
            BuiltinFailure::new(C, format!("jq syntax error — {}", render_jq_load(&errs)))
        })?;
    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errs| {
            BuiltinFailure::new(
                C,
                format!("jq compile error — {}", render_jq_compile(&errs)),
            )
        })?;

    let ctx = Ctx::<jaq_data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    let mut single: Option<serde_json::Value> = None;
    for result in filter.id.run((ctx, val)).map(unwrap_valr) {
        let value = result.map_err(|e| BuiltinFailure::new(C, format!("jq runtime error: {e}")))?;
        // The exactly-one law fires BEFORE serializing a second value —
        // a long stream never pays per-element render cost past the law.
        if single.is_some() {
            return Err(BuiltinFailure::new(
                C,
                "the program emitted MORE than one value — wrap it in `[ … ]` to collect a stream into an array",
            ));
        }
        let text = value.to_string();
        if text.len() > MAX_JQ_OUTPUT_BYTES {
            return Err(BuiltinFailure::new(
                C,
                format!(
                    "the output is {} bytes — the per-value ceiling is {MAX_JQ_OUTPUT_BYTES} (narrow the expression)",
                    text.len()
                ),
            ));
        }
        single = Some(serde_json::from_str(&text).map_err(|e| jq_render_failure(&text, &e))?);
    }
    single.ok_or_else(|| {
        BuiltinFailure::new(
            C,
            "the program emitted NO value — a binding needs exactly one (use `// default` or `first(…)`)",
        )
    })
}

/// jq-std defs we SHADOW with the jq-correct semantics (loaded last, so the
/// compiler's name resolution picks them over the upstream defs).
///
/// - `scan` — jaq-std 3.0.1 defines `scan(re; flags): matches(re; flags)[]`
///   WITHOUT the global flag, so `scan(re)` yields the FIRST match only and
///   `[.s | scan("\\S+")]` on "one two three" silently returns `["one"]`
///   (green check · green run · every number wrong — the 2026-07-29
///   finding). jq defines scan as global by construction (`match(re;
///   "g"+flags)`). Unfixed upstream on `main` at pin time; the shadow retires
///   the day a jaq release carries the correction.
const JQ_STD_CORRECTIONS: &str = r#"
def scan(re; flags): matches(re; "g" + flags)[] | .[0].string;
def scan(re): scan(re; "");
"#;

/// Parse the shadow defs (static string — a parse failure can only come from
/// an edit of [`JQ_STD_CORRECTIONS`], so it is a typed failure here, never a
/// panic) into the `Def` items the loader chains after jaq-std's.
fn jq_std_corrections() -> Result<Vec<jaq_core::load::parse::Def<&'static str>>, BuiltinFailure> {
    jaq_core::load::parse(JQ_STD_CORRECTIONS, |p| p.defs()).ok_or_else(|| {
        BuiltinFailure::new(
            "NIKA-BUILTIN-JQ-001",
            "internal: the jq std correction defs failed to parse (static string)",
        )
    })
}

/// Render a jaq LOAD error set (lex/parse/io) as one clean author-facing
/// line — NOT the raw jaq `Debug` repr (the jq-3 finding · `nika check`'s
/// `analyzer::jq_lint` renders the identical shape statically).
fn render_jq_load(errs: &[(File<&str, ()>, JqLoadError<&str>)]) -> String {
    let Some((_, first)) = errs.first() else {
        return "does not parse".to_owned();
    };
    match first {
        JqLoadError::Io(v) => v
            .first()
            .map_or_else(|| "io error".to_owned(), |(_, m)| format!("io: {m}")),
        JqLoadError::Lex(v) => v.first().map_or_else(
            || "lexing error".to_owned(),
            |(exp, at)| jq_syntax_msg(exp.as_str(), at),
        ),
        JqLoadError::Parse(v) => v.first().map_or_else(
            || "parse error".to_owned(),
            |(exp, at)| jq_syntax_msg(exp.as_str(), at),
        ),
    }
}

/// Render a jaq COMPILE error set (undefined filters/variables) as one line —
/// and, when the undefined name is one this engine WITHHELDS, say so and name
/// the class it would have read (D-2026-08-11-N26) rather than telling the
/// author that a filter jq really does define is « undefined ».
#[allow(clippy::type_complexity)] // the shape is jaq's `compile::Errors`, not ours
fn render_jq_compile<U>(errs: &[(File<&str, ()>, Vec<(&str, U)>)]) -> String {
    errs.first().and_then(|(_, v)| v.first()).map_or_else(
        || "compile error".to_owned(),
        |(name, _)| {
            nika_cap::withheld_jq_reason(name)
                .unwrap_or_else(|| format!("undefined filter or variable `{name}`"))
        },
    )
}

/// One clean « expected X near Y » line (jaq `Expect::as_str` + the slice).
fn jq_syntax_msg(expected: &str, at: &str) -> String {
    let at = at.trim();
    if at.is_empty() {
        format!("expected {expected} (unexpected end of input)")
    } else {
        let snippet: String = at.chars().take(24).collect();
        format!("expected {expected} near `{snippet}`")
    }
}

/// A jq output that won't re-parse as JSON — jaq renders non-finite
/// numbers as the bare tokens `NaN`/`Infinity` (valid jq, invalid JSON),
/// so surface THAT cause instead of a raw parser offset.
fn jq_render_failure(text: &str, e: &serde_json::Error) -> BuiltinFailure {
    const C: &str = "NIKA-BUILTIN-JQ-001";
    let hint = if text.contains("Infinity") || text.contains("NaN") {
        " (the program produced a non-finite number — NaN/Infinity is not valid JSON)"
    } else {
        ""
    };
    BuiltinFailure::new(C, format!("output is not valid JSON: {e}{hint}"))
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
        return Err(BuiltinFailure::new(
            C,
            "this builtin implements the object-patch subset of RFC 7396 — both `target:` and \
             `patch:` must be JSON objects; use `nika:jq` for non-object patch semantics",
        ));
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
    // Structured error objects, not bare strings — `path` (JSON Pointer
    // to the failing value) + `schema_path` (the violated keyword) are
    // what a repair step branches on; the human `message` rides along.
    // The spec pins `errors: [...]` without an element shape (stdlib
    // §validate) — this is the machine-readable refinement.
    let errors: Vec<serde_json::Value> = validator
        .iter_errors(&data)
        .map(|e| {
            serde_json::json!({
                "path": e.instance_path().to_string(),
                "schema_path": e.schema_path().to_string(),
                "message": e.to_string(),
            })
        })
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
    // CSV `has_header:` is STRICT (default true) — resolved ONCE here so a
    // non-bool (`"false"`, `0`) is a LOUD CONVERT-001 arg error · NOT the
    // silent coercion a lax reader would do (reading every non-bool as
    // the default → header-AWARE output for `has_header: "false"`, the
    // opposite of intent · the F1 silent-data-corruption footgun). It only
    // matters for the csv direction, but validating unconditionally is the
    // loud floor — a non-bool flag is an authoring bug regardless.
    let has_header = strict_bool(args, "has_header", true, C1)?;
    // CSV formula-injection guard (opt-in · default false). When on, a cell
    // whose first byte a spreadsheet reads as a formula (`= + - @` or the
    // `\t`/`\r` control chars) is prefixed with `'` so Excel/Sheets/LibreOffice
    // render it as literal text (CWE-1236). Opt-in because it ALTERS data — a
    // negative number `-5` becomes the text `'-5` — so a workflow enables it
    // only when the CSV is untrusted AND destined for a spreadsheet. Resolved
    // here (loud on a non-bool) even for non-csv `to`, matching has_header:
    // an errant flag is an authoring bug in any direction.
    let formula_guard = strict_bool(args, "formula_guard", false, C1)?;

    let value = parse_format(from, input, has_header).map_err(|e| BuiltinFailure::new(C2, e))?;
    // Emit the target format.
    emit_format(to, &value, has_header, formula_guard).map_err(|e| BuiltinFailure::new(C1, e))
}

/// The CSV formula-injection guard (CWE-1236). A spreadsheet interprets a
/// cell that begins with `= + - @` as a formula — so `=HYPERLINK(...)` or
/// `=cmd|...` in untrusted data executes when the file is opened. The OWASP
/// mitigation: prefix such a cell with a single quote, which those apps strip
/// on display and treat as text.
///
/// Two subtleties beyond a naive first-byte check:
/// - **Leading whitespace**: Google Sheets (and some Excel import paths) trim
///   leading spaces/tabs BEFORE formula detection, so ` =cmd` executes. We
///   check the first NON-whitespace byte, not byte 0.
/// - **Control-char triggers**: a cell whose very first byte is `\t` or `\r`
///   is itself a trigger (those bytes ARE whitespace, so the non-whitespace
///   scan would skip past them — they're caught explicitly).
///
/// Takes ownership so the common path (guard off, or a safe cell) is
/// zero-allocation — it returns the input unchanged. Byte-level is sound: an
/// ASCII trigger can never be a UTF-8 continuation/lead byte (those are ≥0x80).
fn guard_formula(cell: String, on: bool) -> String {
    if !on {
        return cell;
    }
    let bytes = cell.as_bytes();
    let leading_control = matches!(bytes.first(), Some(b'\t' | b'\r'));
    let first_significant = bytes.iter().find(|b| !b.is_ascii_whitespace());
    let is_formula_start = matches!(first_significant, Some(b'=' | b'+' | b'-' | b'@'));
    if leading_control || is_formula_start {
        let mut guarded = String::with_capacity(cell.len() + 1);
        guarded.push('\'');
        guarded.push_str(&cell);
        return guarded;
    }
    cell
}

fn parse_format(
    from: &str,
    input: &serde_json::Value,
    has_header: bool,
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
        "toml" => {
            let parsed: toml_convert::Value =
                toml_convert::from_str(&as_text()?).map_err(|e| format!("invalid TOML: {e}"))?;
            toml_to_json(parsed)
        }
        "csv" => parse_csv(&as_text()?, has_header),
        other => Err(format!("unknown from: {other} (json|yaml|toml|csv)")),
    }
}

/// The typed TOML→JSON bridge. A serde round-trip leaks toml's internal
/// `$__toml_private_datetime` sentinel for date values — walking the
/// typed `toml::Value` instead renders datetimes as their ISO 8601
/// strings (the only JSON-representable form).
fn toml_to_json(value: toml_convert::Value) -> Result<serde_json::Value, String> {
    Ok(match value {
        toml_convert::Value::String(s) => serde_json::Value::String(s),
        toml_convert::Value::Integer(i) => serde_json::Value::Number(i.into()),
        toml_convert::Value::Float(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .ok_or("TOML non-finite float (nan/inf) is not representable in JSON")?,
        toml_convert::Value::Boolean(b) => serde_json::Value::Bool(b),
        toml_convert::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
        toml_convert::Value::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(toml_to_json)
                .collect::<Result<_, _>>()?,
        ),
        toml_convert::Value::Table(table) => serde_json::Value::Object(
            table
                .into_iter()
                .map(|(k, v)| Ok((k, toml_to_json(v)?)))
                .collect::<Result<_, String>>()?,
        ),
    })
}

fn emit_format(
    to: &str,
    value: &serde_json::Value,
    has_header: bool,
    formula_guard: bool,
) -> Result<serde_json::Value, String> {
    let text = match to {
        "json" => return Ok(value.clone()),
        "yaml" => serde_yaml_bw::to_string(value).map_err(|e| format!("to YAML: {e}"))?,
        "toml" => toml_convert::to_string(value).map_err(|e| format!("to TOML: {e}"))?,
        "csv" => emit_csv(value, has_header, formula_guard)?,
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
fn emit_csv(
    value: &serde_json::Value,
    has_header: bool,
    formula_guard: bool,
) -> Result<String, String> {
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
        // A header key is attacker-influenced too (JSON object keys) — guard it
        // after sorting, at write time, so the guard never perturbs dedup/order.
        let hdr: Vec<String> = headers
            .iter()
            .map(|h| guard_formula(h.clone(), formula_guard))
            .collect();
        writer.write_record(&hdr).map_err(|e| e.to_string())?;
    }
    for row in rows {
        let obj = row
            .as_object()
            .ok_or_else(|| "every CSV row must be an object".to_owned())?;
        let cells: Vec<String> = headers
            .iter()
            .map(|h| {
                let raw = match obj.get(h) {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(other) => other.to_string(),
                    None => String::new(),
                };
                guard_formula(raw, formula_guard)
            })
            .collect();
        writer.write_record(&cells).map_err(|e| e.to_string())?;
    }
    let bytes = writer.into_inner().map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
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

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
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

/// Standard base64 decode (RFC 4648 · the encoder's mirror) — strict:
/// canonical alphabet only, correct `=` padding, no whitespace. The
/// consumer is `nika:write`'s binary-content clause, whose input is OUR
/// OWN `base64_encode` output (`nika:read binary: true`) — strictness
/// is therefore a round-trip invariant, not user hostility.
pub(crate) fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Result<u32, String> {
        match c {
            b'A'..=b'Z' => Ok(u32::from(c - b'A')),
            b'a'..=b'z' => Ok(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Ok(u32::from(c - b'0') + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            other => Err(format!("invalid base64 byte 0x{other:02x}")),
        }
    }
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("base64 length must be a multiple of 4".to_owned());
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (i, quad) in bytes.chunks_exact(4).enumerate() {
        let last = (i + 1) * 4 == bytes.len();
        // 4-byte window — a bytecount dep for this is the lint's blind
        // spot, not ours.
        #[allow(clippy::naive_bytecount)]
        let pads = quad.iter().filter(|&&c| c == b'=').count();
        // `=` is legal only as the final 1-2 bytes of the final quad.
        let pads_ok = match pads {
            0 => true,
            1 => last && quad[3] == b'=',
            2 => last && quad[2] == b'=' && quad[3] == b'=',
            _ => false,
        };
        if !pads_ok {
            return Err("malformed base64 padding".to_owned());
        }
        // MUTATION (equivalent): `|` vs `^` — the four 6-bit values land
        // on DISJOINT bit ranges (<<18, <<12, <<6, <<0), so OR and XOR
        // pack identically (the encoder's documented mirror class).
        let n = (val(quad[0])? << 18)
            | (val(quad[1])? << 12)
            | (if pads >= 2 { 0 } else { val(quad[2])? << 6 })
            | (if pads >= 1 { 0 } else { val(quad[3])? });
        #[allow(clippy::cast_possible_truncation)] // each shift isolates one byte
        {
            out.push((n >> 16) as u8);
            if pads < 2 {
                out.push((n >> 8) as u8);
            }
            if pads < 1 {
                out.push(n as u8);
            }
        }
    }
    Ok(out)
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
    fn jq_cannot_read_the_ambient_environment() {
        // D-2026-08-11-N26 · an expression sees only its INPUT. Measured on the
        // shipped 0.108.0 binary (2026-08-15) this returned the operator's
        // variable under an ABSENT `permits:` block, and the check certificate
        // called the body « pure compute · nothing escapes ».
        let err = jq(&args(serde_json::json!({
            "expression": "env.PATH",
            "input": {}
        })))
        .expect_err("withheld");
        assert!(
            err.message.contains("ambient process environment"),
            "the refusal must NAME the class · got: {}",
            err.message
        );
        // Bare `env` (the whole map) is the same refusal.
        let bare = jq(&args(
            serde_json::json!({ "expression": "env", "input": {} }),
        ))
        .expect_err("withheld");
        assert!(bare.message.contains("ambient process environment"));
    }

    /// THE CLOCK IS STILL OPEN — pinned, not forgotten.
    ///
    /// `now` reads the wall clock here today. D-2026-08-11-N27 (active) owns it
    /// and prescribes a REBINDING (resolve to the run's start instant, already
    /// in the trace, so a replay yields the same value forever), not the
    /// subtraction N26 applies to the environment. Measured 2026-08-15: zero
    /// call sites in a 184-program corpus — the cost of either remedy is nil;
    /// the CHOICE of remedy belongs to N27.
    ///
    /// When N27 ships, this test goes red. That is its whole job.
    #[test]
    fn the_clock_is_a_named_open_debt_owned_by_n27() {
        let out = jq(&args(
            serde_json::json!({ "expression": "now", "input": {} }),
        ))
        .expect("the clock still reads today");
        assert!(
            out.as_f64().is_some_and(|t| t > 1_700_000_000.0),
            "`now` returned {out} — if this stopped being a wall-clock read, \
             D-2026-08-11-N27 shipped: rebind the test to the run's start instant"
        );
    }

    #[test]
    fn jq_keeps_the_pure_date_family() {
        // The subtraction is SCOPED: a function of its own argument stays.
        let out = jq(&args(serde_json::json!({
            "expression": ".t | gmtime | .[0]",
            "input": { "t": 0 }
        })))
        .expect("gmtime is pure");
        assert_eq!(out, serde_json::json!(1970));
        let fmt = jq(&args(serde_json::json!({
            "expression": ".t | strftime(\"%Y\")",
            "input": { "t": 0 }
        })))
        .expect("strftime is pure");
        assert_eq!(fmt, serde_json::json!("1970"));
    }

    #[test]
    fn jq_keeps_jaqs_wording_for_a_typo() {
        // A typo is NOT dressed up as a boundary refusal.
        let err = jq(&args(
            serde_json::json!({ "expression": "envv", "input": {} }),
        ))
        .expect_err("undefined");
        assert!(
            err.message.contains("undefined filter or variable `envv`"),
            "{}",
            err.message
        );
        assert!(!err.message.contains("withheld"), "{}", err.message);
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

        // The error-render fns surface the CAUSE, not a stub: a syntax error
        // renders « expected … » (render_jq_load → jq_syntax_msg) and an
        // undefined filter is NAMED (render_jq_compile) — assert the MESSAGE.
        let syn = jq(&args(
            serde_json::json!({ "expression": ". |", "input": 1 }),
        ));
        assert!(
            matches!(&syn, Err(f) if f.message.contains("expected")),
            "{syn:?}"
        );
        let undef = jq(&args(
            serde_json::json!({ "expression": "undefined_func", "input": 1 }),
        ));
        assert!(
            matches!(&undef, Err(f) if f.message.contains("undefined filter or variable")),
            "{undef:?}"
        );
    }

    /// The upstream divergence (jaq-std 3.0.1, unfixed on `main` 2026-07-29):
    /// jq's `scan` is GLOBAL by definition (`match(re; "g"+flags)`), jaq
    /// forgets the `"g"` — `[.s | scan("\\S+")]` on "one two three" yielded
    /// `["one"]` (green check · green run · every number wrong). The engine
    /// shadows the def with the jq-correct one; this test is the lock.
    #[test]
    fn jq_scan_is_global_like_jq() {
        let out = jq(&args(serde_json::json!({
            "expression": "[.s | scan(\"\\\\S+\")]",
            "input": { "s": "one two three" }
        })))
        .expect("scan collects every match");
        assert_eq!(out, serde_json::json!(["one", "two", "three"]));

        // Author flags ride ALONGSIDE the forced global (jq's "g"+flags).
        let out = jq(&args(serde_json::json!({
            "expression": "[.s | scan(\"TWO\"; \"i\")]",
            "input": { "s": "one two three" }
        })))
        .expect("scan with flags still matches case-insensitively");
        assert_eq!(out, serde_json::json!(["two"]));
    }

    #[test]
    fn jq_bounds_the_rendered_output_and_names_non_finite() {
        // A single value past the 16 MiB rendered ceiling is a typed
        // failure, not an allocation storm ("x" * 17M is one output).
        let huge = jq(&args(serde_json::json!({
            "expression": "\"x\" * 17000000", "input": null
        })));
        assert!(
            matches!(&huge, Err(f) if f.code == "NIKA-BUILTIN-JQ-001" && f.message.contains("ceiling")),
            "{huge:?}"
        );
        // Non-finite numbers (valid jq · invalid JSON) get the actionable
        // cause, not a raw parser offset.
        let inf = jq(&args(
            serde_json::json!({ "expression": "1e308 * 10", "input": null }),
        ));
        assert!(
            matches!(&inf, Err(f) if f.message.contains("non-finite")),
            "{inf:?}"
        );
        // An under-ceiling collect still works (the law allows one array).
        let ok = jq(&args(serde_json::json!({
            "expression": "[range(3)]", "input": null
        })))
        .expect("ok");
        assert_eq!(ok, serde_json::json!([0, 1, 2]));
        // A mid-size output (2 MB) is comfortably UNDER the 16 MiB ceiling
        // (kills the constant-arithmetic mutants that shrink it to ~1 MB).
        let mid = jq(&args(serde_json::json!({
            "expression": "\"x\" * 2000000 | length", "input": null
        })))
        .expect("2 MB renders fine");
        assert_eq!(mid, serde_json::json!(2_000_000));
        let mid_value = jq(&args(serde_json::json!({
            "expression": "\"x\" * 2000000", "input": null
        })));
        assert!(mid_value.is_ok(), "2 MB value is under the ceiling");
        // The exact boundary: a rendered output of EXACTLY the ceiling
        // passes (`>` not `>=`) — quotes add 2 bytes to the raw repeat.
        let exact = jq(&args(serde_json::json!({
            "expression": format!("\"x\" * {}", MAX_JQ_OUTPUT_BYTES - 2), "input": null
        })));
        assert!(exact.is_ok(), "len == ceiling is allowed");
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
    fn validate_errors_are_structured_repair_handles() {
        // Each error is { path, schema_path, message } — the JSON Pointer
        // a repair step branches on, not a prose blob to re-parse.
        let schema = serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "age": { "type": "integer" },
                "name": { "type": "string" }
            }
        });
        let report = validate(&args(serde_json::json!({
            "data": { "age": "not-a-number" }, "schema": schema
        })))
        .expect("report");
        assert_eq!(report["valid"], false);
        let errors = report["errors"].as_array().expect("array");
        assert_eq!(errors.len(), 2, "missing name + wrong-type age");
        // The wrong-type error points AT the failing value.
        let age_err = errors
            .iter()
            .find(|e| e["path"] == "/age")
            .expect("age error present");
        assert!(
            age_err["schema_path"]
                .as_str()
                .expect("schema_path")
                .contains("type"),
            "{age_err}"
        );
        assert!(
            age_err["message"]
                .as_str()
                .expect("message")
                .contains("integer")
        );
        // The missing-required error points at the ROOT.
        let root_err = errors
            .iter()
            .find(|e| e["path"] == "")
            .expect("root error present");
        assert!(
            root_err["schema_path"]
                .as_str()
                .expect("schema_path")
                .contains("required"),
            "{root_err}"
        );
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
    fn validate_never_fetches_a_remote_ref() {
        // SSRF FLOOR: jsonschema is compiled `default-features = false`
        // (no resolve-http / resolve-file), so a remote `$ref` CANNOT be
        // fetched — it must surface loudly as a compile error, never a
        // network call or a silent pass. This pins the structurally-closed
        // property so a future `default-features` flip is caught here, not
        // in the wild. (Proven at the binary: the remote $ref returns
        // VALIDATE-001 with no hang.)
        for uri in [
            "https://example.com/evil.json",
            "http://169.254.169.254/latest/meta-data",
            "file:///etc/passwd",
        ] {
            let refused = validate(&args(serde_json::json!({
                "data": {}, "schema": { "$ref": uri }
            })));
            assert!(
                matches!(&refused, Err(f) if f.code == "NIKA-BUILTIN-VALIDATE-001"),
                "remote/file $ref `{uri}` must refuse (SSRF/LFI floor), got {refused:?}"
            );
        }
    }

    #[test]
    fn validate_deep_yaml_is_bounded_never_a_stack_overflow() {
        // TOTALITY: a deeply-nested YAML `data:` string must not overflow
        // the parser's stack (the classic serde_yaml crash class). The
        // `serde_yaml_bw` fork bounds nesting, so this returns a clean
        // parse error, never a SIGSEGV. Pinned across a depth sweep so a
        // dep swap that reintroduces the crash is caught. (Proven at the
        // binary: 200 → 50k deep, all clean exits, zero crashes.)
        let schema = serde_json::json!({ "type": "array" });
        for depth in [128_usize, 1_000, 20_000] {
            let deep = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
            let out = validate(&args(serde_json::json!({
                "data": deep, "schema": schema, "format": "yaml"
            })));
            // Either it parses (bounded-ok) or it refuses (VALIDATE-002) —
            // both are total; the ONLY unacceptable outcome is a crash,
            // which this call returning at all disproves.
            assert!(
                out.is_ok() || matches!(&out, Err(f) if f.code == "NIKA-BUILTIN-VALIDATE-002"),
                "depth {depth} must be total (ok or -002), got {out:?}"
            );
        }
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
    fn convert_has_header_is_a_strict_bool_not_silently_coerced() {
        // F1 · the silent-data-corruption footgun: a non-bool `has_header`
        // must be a LOUD error, never silently read as the (true) default.

        // has_header: false → headerless · the first row is DATA (an array
        // of arrays), the header is NOT consumed.
        let headerless = convert(&args(serde_json::json!({
            "input": "name,age\nada,36", "from": "csv", "to": "json", "has_header": false
        })))
        .expect("ok");
        assert_eq!(
            headerless,
            serde_json::json!([["name", "age"], ["ada", "36"]]),
            "has_header: false keeps every row as data"
        );

        // has_header: true → the first row is the header (array of objects).
        let with_header = convert(&args(serde_json::json!({
            "input": "name,age\nada,36", "from": "csv", "to": "json", "has_header": true
        })))
        .expect("ok");
        assert_eq!(
            with_header,
            serde_json::json!([{"name": "ada", "age": "36"}]),
            "has_header: true consumes the header row"
        );

        // has_header: "false" (a STRING) is a LOUD CONVERT-001 — NOT the
        // old silent-true that produced header-aware output (the opposite
        // of the author's intent, with no error).
        let string_false = convert(&args(serde_json::json!({
            "input": "name,age\nada,36", "from": "csv", "to": "json", "has_header": "false"
        })));
        assert!(
            matches!(&string_false, Err(f) if f.code == "NIKA-BUILTIN-CONVERT-001"
                && f.message.contains("must be a boolean")),
            "string has_header is loud, not silent-true: {string_false:?}"
        );

        // has_header: 0 (a NUMBER) is likewise loud.
        let number_zero = convert(&args(serde_json::json!({
            "input": "name,age\nada,36", "from": "csv", "to": "json", "has_header": 0
        })));
        assert!(
            matches!(&number_zero, Err(f) if f.code == "NIKA-BUILTIN-CONVERT-001"),
            "numeric has_header is loud: {number_zero:?}"
        );

        // The strict check ALSO guards the emit direction (json → csv).
        let emit_bad = convert(&args(serde_json::json!({
            "input": [{"a": "1"}], "from": "json", "to": "csv", "has_header": "0"
        })));
        assert!(
            matches!(&emit_bad, Err(f) if f.code == "NIKA-BUILTIN-CONVERT-001"),
            "emit-side has_header is strict too: {emit_bad:?}"
        );
    }

    #[test]
    fn convert_toml_datetime_is_an_iso_string_not_a_sentinel() {
        // The typed bridge: toml dates render as ISO strings — the serde
        // path would leak `$__toml_private_datetime` into user output.
        let out = convert(&args(serde_json::json!({
            "input": "when = 2026-01-01T00:00:00Z\nn = 3\npi = 1.5\nok = true",
            "from": "toml", "to": "json"
        })))
        .expect("ok");
        assert_eq!(out["when"], serde_json::json!("2026-01-01T00:00:00Z"));
        assert_eq!(out["n"], serde_json::json!(3));
        assert_eq!(out["pi"], serde_json::json!(1.5));
        assert_eq!(out["ok"], serde_json::json!(true));
        assert!(
            !out.to_string().contains("$__toml"),
            "no private sentinel: {out}"
        );
        // Nested tables + arrays walk through the same bridge.
        let nested = convert(&args(serde_json::json!({
            "input": "[a]\nwhen = 2026-01-01\nlist = [1, 2]",
            "from": "toml", "to": "json"
        })))
        .expect("ok");
        assert_eq!(nested["a"]["when"], serde_json::json!("2026-01-01"));
        assert_eq!(nested["a"]["list"], serde_json::json!([1, 2]));
        // A TOML non-finite float is not JSON-representable → typed error.
        let nan = convert(&args(serde_json::json!({
            "input": "x = nan", "from": "toml", "to": "json"
        })));
        assert!(matches!(nan, Err(f) if f.message.contains("non-finite")));
    }

    #[test]
    fn convert_formula_guard_off_by_default_preserves_dangerous_cells() {
        // Default (no flag): a `=cmd` cell rides through VERBATIM — round-trip
        // fidelity is the default, matching the Rust/Python csv ecosystem. The
        // spreadsheet-injection risk is the caller's to opt into.
        let csv = convert(&args(serde_json::json!({
            "input": [{"formula": "=HYPERLINK(\"http://evil\")"}],
            "from": "json", "to": "csv"
        })))
        .expect("ok");
        let text = csv.as_str().expect("string");
        assert!(
            text.contains("=HYPERLINK"),
            "default is verbatim (no guard): {text}"
        );
        assert!(!text.contains("'=HYPERLINK"), "no quote prefix by default");
    }

    #[test]
    fn convert_formula_guard_on_neutralizes_every_owasp_trigger() {
        // With the flag, each of the canonical OWASP trigger characters
        // (`= + - @` and the `\t`/`\r` control chars) leading a cell is
        // prefixed with `'` — the spreadsheet renders it as literal text.
        for trigger in ["=cmd", "+cmd", "-cmd", "@cmd", "\tcmd", "\rcmd"] {
            let csv = convert(&args(serde_json::json!({
                "input": [{"c": trigger}],
                "from": "json", "to": "csv", "formula_guard": true
            })))
            .expect("ok");
            let text = csv.as_str().expect("string");
            // The data row is the second line (after the `c` header). It must
            // carry the leading quote before the trigger.
            let data = text.lines().nth(1).unwrap_or("");
            assert!(
                data.contains(&format!("'{trigger}"))
                    || data.contains("'\t")
                    || data.contains('\r'),
                "trigger {trigger:?} neutralized with a leading quote: {data:?}"
            );
        }
    }

    #[test]
    fn convert_formula_guard_on_catches_the_leading_whitespace_bypass() {
        // The subtle bypass: a spreadsheet (Google Sheets · some Excel paths)
        // trims leading whitespace BEFORE formula detection, so ` =cmd` and
        // `\t=cmd` execute. A naive first-BYTE check would miss them (it sees
        // the space/tab). The guard scans the first NON-whitespace byte.
        for payload in [" =SUM(1)", "  =cmd", "\t=cmd", " \t -evil"] {
            let csv = convert(&args(serde_json::json!({
                "input": [{"c": payload}],
                "from": "json", "to": "csv", "formula_guard": true
            })))
            .expect("ok");
            let text = csv.as_str().expect("string");
            let data = text.lines().nth(1).unwrap_or("");
            // The leading quote sits at the very start (before the whitespace).
            assert!(
                data.contains(&format!("\"'{payload}\"")) || data.contains('\''),
                "whitespace-prefixed formula {payload:?} is guarded: {data:?}"
            );
        }
    }

    #[test]
    fn convert_formula_guard_on_leaves_safe_cells_untouched() {
        // A safe cell (normal text, or a number not leading with a trigger)
        // is byte-identical with the guard on — the guard is surgical.
        let csv = convert(&args(serde_json::json!({
            "input": [{"name": "ada", "n": "36", "note": "hi=there"}],
            "from": "json", "to": "csv", "formula_guard": true
        })))
        .expect("ok");
        let text = csv.as_str().expect("string");
        assert!(!text.contains('\''), "no quote added to safe cells: {text}");
        assert!(text.contains("hi=there"), "an interior = is not a trigger");
    }

    #[test]
    fn convert_formula_guard_on_guards_header_keys_too() {
        // A JSON object key is attacker-influenced (it comes from the input),
        // so a `=cmd` HEADER is neutralized just like a data cell.
        let csv = convert(&args(serde_json::json!({
            "input": [{"=evil": "1"}],
            "from": "json", "to": "csv", "formula_guard": true
        })))
        .expect("ok");
        let text = csv.as_str().expect("string");
        let header = text.lines().next().unwrap_or("");
        assert!(header.contains("'=evil"), "header key guarded: {header:?}");
    }

    #[test]
    fn convert_formula_guard_survives_the_csv_quoting_layer_round_trip() {
        // The one place two escaping layers stack: a cell that is BOTH a
        // formula trigger AND contains a comma. The guard prefixes `'`, then
        // the csv writer quotes the field (for the comma). Round-tripping the
        // output back through `from: csv` must recover a cell that STILL starts
        // with the guard quote — proving CSV transport-quoting is stripped by
        // the reader without undoing the formula guard (the refuter's item 3/4).
        let csv = convert(&args(serde_json::json!({
            "input": [{"c": "=cmd,evil"}],
            "from": "json", "to": "csv", "formula_guard": true
        })))
        .expect("emit ok");
        let text = csv.as_str().expect("string").to_owned();
        // The emitted field is quoted (it has a comma) AND guarded.
        assert!(text.contains("\"'=cmd,evil\""), "quoted + guarded: {text}");
        // Read it back: the recovered cell keeps the leading quote (the CSV
        // quotes are transport, stripped by the reader; the `'` is content).
        let back = convert(&args(serde_json::json!({
            "input": text, "from": "csv", "to": "json"
        })))
        .expect("parse ok");
        assert_eq!(
            back[0]["c"],
            serde_json::json!("'=cmd,evil"),
            "the guard apostrophe survives the quoting round-trip: {back}"
        );
    }

    #[test]
    fn convert_formula_guard_on_is_the_accepted_fidelity_cost() {
        // The documented tradeoff: with the guard on, a legitimate negative
        // number `-5` becomes the text `'-5`. This test PINS that cost so it
        // is never a surprise — it is why the guard is opt-in, not default.
        let csv = convert(&args(serde_json::json!({
            "input": [{"balance": "-5"}],
            "from": "json", "to": "csv", "formula_guard": true
        })))
        .expect("ok");
        let text = csv.as_str().expect("string");
        assert!(text.contains("'-5"), "negative number becomes text: {text}");
    }

    #[test]
    fn convert_formula_guard_is_a_strict_bool_not_silently_coerced() {
        // Same F1 floor as has_header: a non-bool `formula_guard` is a LOUD
        // arg error, never silently read as false (which would ship an
        // unguarded CSV while the author believed the guard was on).
        let bad = convert(&args(serde_json::json!({
            "input": [{"c": "=x"}], "from": "json", "to": "csv", "formula_guard": "true"
        })));
        assert!(
            matches!(&bad, Err(f) if f.code == "NIKA-BUILTIN-CONVERT-001"
                && f.message.contains("formula_guard")),
            "non-bool formula_guard is a loud CONVERT-001: {bad:?}"
        );
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

    #[test]
    fn base64_decoder_mirrors_the_encoder_and_rejects_malformed() {
        // Round-trip over the encoder's own vectors (the consumer
        // contract: read binary → write).
        for bytes in [
            &b""[..],
            b"f",
            b"fo",
            b"foo",
            b"foobar",
            &[0xff, 0xff, 0xff],
            &[0xfb, 0xf0],
            &[0x00, 0x01, 0x02, 0x03, 0xfe],
        ] {
            assert_eq!(
                base64_decode(&base64_encode(bytes)).expect("round-trips"),
                bytes,
                "{bytes:?}"
            );
        }
        // Strictness: bad length · bad byte · interior/misplaced padding.
        assert!(base64_decode("Zg=").is_err(), "length not multiple of 4");
        assert!(base64_decode("Zg!=").is_err(), "non-alphabet byte");
        assert!(base64_decode("=g==").is_err(), "leading pad");
        assert!(base64_decode("Zg==Zm8=").is_err(), "interior padding quad");
        assert!(base64_decode("Z===").is_err(), "triple pad");
        // "A=B=": two pads but quad[2] is NOT '=' — this MUST be the
        // PADDING error, not the later invalid-byte error a mutated
        // `&&`→`||` arm would fall through to (the message IS the pin).
        let split_pads = base64_decode("A=B=").expect_err("split pads rejected");
        assert!(
            split_pads.contains("padding"),
            "rejected AT the padding gate: {split_pads}"
        );
    }

    proptest::proptest! {
        /// decode ∘ encode = identity over arbitrary bytes (the binary
        /// write clause rides this exact round-trip).
        #[test]
        fn base64_round_trips_arbitrary_bytes(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..256)) {
            proptest::prop_assert_eq!(
                base64_decode(&base64_encode(&bytes)).expect("round-trips"),
                bytes
            );
        }
    }

    // ── Gate 6 · property tests (crate spec §5) ─────────────────────────

    /// Arbitrary JSON values whose textual form round-trips exactly:
    /// full-range integers (jaq carries them losslessly), finite floats
    /// (Display is shortest-round-trip), and `-0.0` excluded (its sign
    /// is not observable through `Value` equality).
    fn arb_json() -> impl proptest::strategy::Strategy<Value = serde_json::Value> {
        use proptest::prelude::*;
        let leaf = prop_oneof![
            Just(serde_json::Value::Null),
            any::<bool>().prop_map(serde_json::Value::Bool),
            any::<i64>().prop_map(|n| serde_json::json!(n)),
            // Floats restricted to serde_json's OWN round-trip set: on
            // rare 17-digit edges serde's printer and parser disagree by
            // 1 ULP (verified empirically on 118132816.07034513 ·
            // serde_json 1.0.149) — that ecosystem caveat is not this
            // bridge's contract. Text fidelity through jaq is verbatim
            // either way (Num::Dec carries the literal).
            (-1.0e9f64..1.0e9)
                .prop_filter("skip negative zero", |f| !(*f == 0.0
                    && f.is_sign_negative()))
                .prop_filter("serde_json self-round-trips", |f| {
                    serde_json::to_string(f)
                        .ok()
                        .and_then(|s| serde_json::from_str::<f64>(&s).ok())
                        == Some(*f)
                })
                .prop_map(|f| serde_json::json!(f)),
            "[a-zA-Z0-9 _.-]{0,12}".prop_map(serde_json::Value::String),
        ];
        leaf.prop_recursive(3, 24, 4, |inner| {
            prop_oneof![
                proptest::collection::vec(inner.clone(), 0..4).prop_map(serde_json::Value::Array),
                proptest::collection::btree_map("[a-z]{1,6}", inner, 0..4)
                    .prop_map(|m| serde_json::Value::Object(m.into_iter().collect())),
            ]
        })
    }

    proptest::proptest! {
        /// The exactly-one-output law over ARBITRARY input: the identity
        /// program always emits exactly one value, and the jaq round-trip
        /// (serde → Val → Display → serde) is lossless.
        #[test]
        fn jq_identity_round_trips_arbitrary_json(value in arb_json()) {
            let out = jq(&args(serde_json::json!({
                "expression": ".", "input": value.clone()
            })))
            .expect("identity emits exactly one value");
            proptest::prop_assert_eq!(out, value);
        }
    }
}

/// The EXPRESSION BOUNDARY at the `nika:jq` seam — the ratchet that survives
/// the next `jaq` release.
///
/// D-2026-08-11-N26 says an expression sees only its input. A blocklist alone
/// would let a future `jaq` ship a new ambient native and reopen the hole IN
/// SILENCE, so the guard here is a PINNED INVENTORY: the full native set the
/// workspace-pinned stack exposes, asserted as a set. Grow it, rename one,
/// drop one — this goes red and a human triages the newcomer into
/// `nika_cap::WITHHELD_JQ_NATIVES` or into the pin.
#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod expression_boundary {
    use jaq_core::data::JustLut;
    use jaq_json::Val;

    /// Every native the pinned stack exposes — jaq-core 3.1 (10) · jaq-std 3.0
    /// (96) · jaq-json 2.0 (8). Derived by running `funs()` on 2026-08-15, not
    /// transcribed from upstream docs.
    ///
    /// `debug_empty` · `stderr_empty` · `halt` are present ON PURPOSE: they
    /// EMIT to the host or act on the process rather than SEE beyond the input,
    /// which is a different class from N26's subtraction, and jaq-std's own
    /// `defs.jq` builds `debug`/`stderr`/`halt_error` on them. Named here so
    /// the next reader knows they were considered, not missed.
    const PINNED_NATIVES: &[&str] = &[
        // jaq-core
        "error_empty",
        "first",
        "key_values",
        "keys_unsorted",
        "last",
        "limit",
        "path",
        "path_value",
        "range",
        "skip",
        // jaq-std
        "acos",
        "acosh",
        "ascii_downcase",
        "ascii_upcase",
        "asin",
        "asinh",
        "atan",
        "atan2",
        "atanh",
        "cbrt",
        "ceil",
        "copysign",
        "cos",
        "cosh",
        "debug_empty",
        "decode_base64",
        "decode_uri",
        "encode_base64",
        "encode_uri",
        "endswith",
        "env",
        "erf",
        "erfc",
        "escape_html",
        "escape_sh",
        "exp",
        "exp10",
        "exp2",
        "explode",
        "expm1",
        "fabs",
        "fdim",
        "floor",
        "fma",
        "fmax",
        "fmin",
        "fmod",
        "frexp",
        "fromdateiso8601",
        "gmtime",
        "group_by",
        "halt",
        "hypot",
        "ilogb",
        "implode",
        "j0",
        "j1",
        "jn",
        "ldexp",
        "lgamma",
        "localtime",
        "log",
        "log10",
        "log1p",
        "log2",
        "ltrim",
        "ltrimstr",
        "matches",
        "max_by_or_empty",
        "min_by_or_empty",
        "mktime",
        "modf",
        "nearbyint",
        "nextafter",
        "now",
        "pow",
        "remainder",
        "reverse",
        "rint",
        "round",
        "rtrim",
        "rtrimstr",
        "scalbln",
        "sin",
        "sinh",
        "sort",
        "sort_by",
        "split_",
        "split_matches",
        "sqrt",
        "startswith",
        "stderr_empty",
        "strflocaltime",
        "strftime",
        "strptime",
        "tan",
        "tanh",
        "tgamma",
        "todateiso8601",
        "trim",
        "trunc",
        "unescape_html",
        "utf8bytelength",
        "y0",
        "y1",
        "yn",
        // jaq-json
        "bsearch",
        "contains",
        "fromjson",
        "has",
        "indices",
        "length",
        "tobytes",
        "tojson",
    ];

    fn exposed() -> std::collections::BTreeSet<&'static str> {
        jaq_core::funs::<JustLut<Val>>()
            .chain(jaq_std::funs())
            .chain(jaq_json::funs())
            .map(|f| f.0)
            .collect()
    }

    #[test]
    fn the_native_inventory_is_pinned() {
        let pinned: std::collections::BTreeSet<&str> = PINNED_NATIVES.iter().copied().collect();
        assert_eq!(
            pinned.len(),
            PINNED_NATIVES.len(),
            "the pin lists a name twice"
        );
        let live = exposed();
        let added: Vec<_> = live.difference(&pinned).copied().collect();
        let gone: Vec<_> = pinned.difference(&live).copied().collect();
        assert!(
            added.is_empty() && gone.is_empty(),
            "the jaq native set MOVED · new: {added:?} · gone: {gone:?}\n\
             Triage every newcomer: does it read the process, the clock, the disk \
             or the environment? If so it belongs in nika_cap::WITHHELD_JQ_NATIVES \
             (D-2026-08-11-N26 · an expression sees only its input). Otherwise add \
             it to PINNED_NATIVES with that judgment recorded in the commit."
        );
    }

    #[test]
    fn every_withheld_name_really_exists_upstream() {
        // A withheld name that jaq does not define would be a dead entry
        // pretending to guard something — the list must bite.
        let live = exposed();
        for w in nika_cap::WITHHELD_JQ_NATIVES {
            assert!(
                live.contains(w.name),
                "`{}` is withheld but jaq no longer defines it — the row guards nothing",
                w.name
            );
        }
    }

    #[test]
    fn the_compiled_function_set_is_the_inventory_minus_the_withheld() {
        let live = exposed();
        let withheld: std::collections::BTreeSet<&str> = nika_cap::WITHHELD_JQ_NATIVES
            .iter()
            .map(|w| w.name)
            .collect();
        let compiled: std::collections::BTreeSet<&str> = jaq_core::funs::<JustLut<Val>>()
            .chain(jaq_std::funs())
            .chain(jaq_json::funs())
            .filter(|f| !nika_cap::is_withheld_jq_native(f.0))
            .map(|f| f.0)
            .collect();
        let expected: std::collections::BTreeSet<&str> =
            live.difference(&withheld).copied().collect();
        assert_eq!(compiled, expected, "the filter is not the subtraction");
        assert!(!compiled.contains("env"), "env reached the compiler");
    }
}
