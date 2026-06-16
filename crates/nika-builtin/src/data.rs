// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Data builtins (8) — `jq` is THE data language (stdlib §Data).
//!
//! Pure functions of args (`date`/`uuid` ride the clock/entropy edges).
//! Every contract — codes · defaults · the exactly-one-output law — is
//! cited from `nika-spec stdlib/builtins-v0.1.md`, never restated.

use jaq_core::load::{Arena, Error as JqLoadError, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data, unwrap_valr};
use jaq_json::{Val, read};
use nika_kernel::io::clock::ClockDyn;
use sha2::Digest;

use crate::{Args, BuiltinFailure, BuiltinOutcome, opt_str, req_str};

// ─── nika:jq · the transform + extraction primitive ─────────────────────

/// The rendered-output ceiling for one jq value (16 MiB). Bounds the
/// string + re-parse allocations on a model-controlled `expression:`;
/// jaq's INTERNAL evaluation cost (a `[range(1e9)]` materializes inside
/// the engine before any output is yielded) is the engine's task-level
/// supervision concern — same delegation class as SSRF→L1 http (crate
/// spec §4 honest gaps).
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

/// Render a jaq COMPILE error set (undefined filters/variables) as one line.
#[allow(clippy::type_complexity)] // the shape is jaq's `compile::Errors`, not ours
fn render_jq_compile<U>(errs: &[(File<&str, ()>, Vec<(&str, U)>)]) -> String {
    errs.first().and_then(|(_, v)| v.first()).map_or_else(
        || "compile error".to_owned(),
        |(name, _)| format!("undefined filter or variable `{name}`"),
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
    // Structured error objects, not bare strings — `path` (JSON Pointer
    // to the failing value) + `schema_path` (the violated keyword) are
    // what a repair step branches on; the human `message` rides along.
    // The spec pins `errors: [...]` without an element shape (stdlib
    // §validate) — this is the machine-readable refinement.
    let errors: Vec<serde_json::Value> = validator
        .iter_errors(&data)
        .map(|e| {
            serde_json::json!({
                "path": e.instance_path.to_string(),
                "schema_path": e.schema_path.to_string(),
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
    // silent coercion `crate::opt_bool` would do (reading every non-bool as
    // the default → header-AWARE output for `has_header: "false"`, the
    // opposite of intent · the F1 silent-data-corruption footgun). It only
    // matters for the csv direction, but validating unconditionally is the
    // loud floor — a non-bool flag is an authoring bug regardless.
    let has_header = strict_has_header(args, C1)?;

    let value = parse_format(from, input, has_header).map_err(|e| BuiltinFailure::new(C2, e))?;
    // Emit the target format.
    emit_format(to, &value, has_header).map_err(|e| BuiltinFailure::new(C1, e))
}

/// CSV `has_header:` — absent OR a real boolean (default true); anything
/// else is a LOUD arg error. The general `crate::opt_bool` reads every
/// non-bool as the default, which for `has_header` silently INVERTS intent
/// (`"false"` → header-aware) — so this builtin needs the strict reading.
fn strict_has_header(args: &Args, code: &'static str) -> Result<bool, BuiltinFailure> {
    match args.get("has_header") {
        None => Ok(true),
        Some(serde_json::Value::Bool(b)) => Ok(*b),
        Some(other) => Err(BuiltinFailure::new(
            code,
            format!("`has_header:` must be a boolean (true/false), not {other}"),
        )),
    }
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
) -> Result<serde_json::Value, String> {
    let text = match to {
        "json" => return Ok(value.clone()),
        "yaml" => serde_yaml_bw::to_string(value).map_err(|e| format!("to YAML: {e}"))?,
        "toml" => toml_convert::to_string(value).map_err(|e| format!("to TOML: {e}"))?,
        "csv" => emit_csv(value, has_header)?,
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

/// The `nika:date` code (stdlib §date · unparseable input / unknown op /
/// bad tz · `validation_error`).
const DATE_CODE: &str = "NIKA-BUILTIN-DATE-001";

/// `op`-discriminated time builtin — the spec's full six (now · add ·
/// subtract · format · parse · diff). `now` rides the injected
/// [`ClockDyn`] wall clock (`system_now`) for test hermeticity;
/// `format`/`parse` speak the strftime field grammar.
pub(crate) fn date<C: ClockDyn>(clock: &C, args: &Args) -> BuiltinOutcome {
    let op = req_str(args, "op", DATE_CODE)?;
    match op {
        "now" => date_now(clock, args),
        "add" | "subtract" => date_shift(op, args),
        "format" => date_format(args),
        "parse" => date_parse(args),
        "diff" => date_diff(args),
        other => Err(BuiltinFailure::new(
            DATE_CODE,
            format!("unknown op `{other}` (now|add|subtract|format|parse|diff)"),
        )),
    }
}

fn parse_ts(args: &Args, key: &str) -> Result<jiff::Timestamp, BuiltinFailure> {
    req_str(args, key, DATE_CODE)?
        .parse()
        .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("`{key}:` unparseable: {e}")))
}

/// `op: now { tz }` — the injected wall clock, ISO 8601 out (UTC `Z`
/// form by default · offset form in an IANA `tz:`).
fn date_now<C: ClockDyn>(clock: &C, args: &Args) -> BuiltinOutcome {
    let now = jiff::Timestamp::try_from(clock.system_now())
        .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("clock out of range: {e}")))?;
    match opt_str(args, "tz", DATE_CODE)? {
        None => Ok(serde_json::Value::String(now.to_string())),
        Some(tz) => {
            let zoned = now
                .in_tz(tz)
                .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("bad `tz:` `{tz}`: {e}")))?;
            let text = jiff::fmt::strtime::format("%Y-%m-%dT%H:%M:%S%.f%:z", &zoned)
                .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("render failed: {e}")))?;
            Ok(serde_json::Value::String(text))
        }
    }
}

/// `op: add|subtract { base, duration, tz }` — ISO 8601 span arithmetic.
/// The span is applied through a [`jiff::Zoned`] (the `tz:` arg · default
/// UTC) rather than the bare [`jiff::Timestamp`]: a `Timestamp` has no
/// calendar/zone context, so its arithmetic only supports units ≤ hours
/// and rejects weeks/days/months/years. Routing through `Zoned` makes BOTH
/// clock and calendar units work and is DST-aware (`add 2 weeks` lands on
/// the civil-correct instant across a DST boundary). The result converts
/// back to a `Timestamp` for the canonical ISO 8601 (`Z`) output.
fn date_shift(op: &str, args: &Args) -> BuiltinOutcome {
    let zoned = date_shift_base_zoned(args)?;
    let span: jiff::Span = req_str(args, "duration", DATE_CODE)?
        .parse()
        .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("`duration:` unparseable: {e}")))?;
    let out = if op == "add" {
        zoned.checked_add(span)
    } else {
        zoned.checked_sub(span)
    }
    .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("{op} overflow: {e}")))?;
    Ok(serde_json::Value::String(out.timestamp().to_string()))
}

/// The `base:` instant zoned for calendar-aware shifting — `tz:` (IANA ·
/// default UTC), mirroring [`date_format`]'s zone resolution.
fn date_shift_base_zoned(args: &Args) -> Result<jiff::Zoned, BuiltinFailure> {
    let ts = parse_ts(args, "base")?;
    match opt_str(args, "tz", DATE_CODE)? {
        None => Ok(ts.to_zoned(jiff::tz::TimeZone::UTC)),
        Some(tz) => ts
            .in_tz(tz)
            .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("bad `tz:` `{tz}`: {e}"))),
    }
}

/// `op: format { input, format, tz }` — render an instant through the
/// strftime grammar (`%Y-%m-%d`). Fields render in `tz:` (IANA ·
/// default UTC) — the `ToolDef` declared `tz:` all along; the impl
/// silently hardcoded UTC (a Paris-display request got UTC fields with
/// no error · the ambition-audit #5 fix).
fn date_format(args: &Args) -> BuiltinOutcome {
    let ts = parse_ts(args, "input")?;
    let fmt = req_str(args, "format", DATE_CODE)?;
    let zoned = match opt_str(args, "tz", DATE_CODE)? {
        None => ts.to_zoned(jiff::tz::TimeZone::UTC),
        Some(tz) => ts
            .in_tz(tz)
            .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("bad `tz:` `{tz}`: {e}")))?,
    };
    let text = jiff::fmt::strtime::format(fmt, &zoned)
        .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("`format:` failed: {e}")))?;
    Ok(serde_json::Value::String(text))
}

/// `op: parse { input, format }` — strftime → ISO 8601 instant. An
/// input that carries no offset is read as UTC (the spec default tz).
fn date_parse(args: &Args) -> BuiltinOutcome {
    let input = req_str(args, "input", DATE_CODE)?;
    let fmt = req_str(args, "format", DATE_CODE)?;
    let broken = jiff::fmt::strtime::parse(fmt, input)
        .map_err(|e| BuiltinFailure::new(DATE_CODE, format!("`parse` failed: {e}")))?;
    let ts = broken.to_timestamp().or_else(|_| {
        broken
            .to_datetime()
            .and_then(|dt| dt.to_zoned(jiff::tz::TimeZone::UTC))
            .map(|z| z.timestamp())
    });
    let ts =
        ts.map_err(|e| BuiltinFailure::new(DATE_CODE, format!("parsed fields incomplete: {e}")))?;
    Ok(serde_json::Value::String(ts.to_string()))
}

/// `op: diff { start, end, unit }` — an integer in `unit:` (seconds
/// default · negative when `end` precedes `start`).
fn date_diff(args: &Args) -> BuiltinOutcome {
    let start = parse_ts(args, "start")?;
    let end = parse_ts(args, "end")?;
    let dur = end.duration_since(start);
    let unit = opt_str(args, "unit", DATE_CODE)?.unwrap_or("seconds");
    let value = match unit {
        "seconds" => dur.as_secs(),
        "milliseconds" => i64::try_from(dur.as_millis())
            .map_err(|_| BuiltinFailure::new(DATE_CODE, "diff out of i64 millisecond range"))?,
        "minutes" => dur.as_secs() / 60,
        "hours" => dur.as_secs() / 3600,
        "days" => dur.as_secs() / 86_400,
        // weeks is a fixed 7-day span (a calendar-independent unit · like
        // days). months/years are deliberately absent — they are not a
        // fixed Duration (a reference instant decides their length), so
        // diff cannot answer them; `add`/`subtract` take ISO 8601 P1M/P1Y.
        "weeks" => dur.as_secs() / 604_800,
        other => {
            return Err(BuiltinFailure::new(
                DATE_CODE,
                format!("unknown unit `{other}` (seconds|milliseconds|minutes|hours|days|weeks)"),
            ));
        }
    };
    Ok(serde_json::Value::Number(value.into()))
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
    fn date_now_rides_the_injected_clock() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        let parse = |v: serde_json::Value| -> jiff::Timestamp {
            v.as_str().expect("string").parse().expect("ISO 8601")
        };
        let t0 = parse(date(&clock, &args(serde_json::json!({ "op": "now" }))).expect("ok"));
        clock.advance(std::time::Duration::from_secs(3600));
        let t1 = parse(date(&clock, &args(serde_json::json!({ "op": "now" }))).expect("ok"));
        // The mock clock IS the time source — exactly the advanced hour.
        assert_eq!(t1.duration_since(t0).as_secs(), 3600);

        // tz renders the offset form (fixed-offset zone = deterministic).
        let zoned = date(
            &clock,
            &args(serde_json::json!({ "op": "now", "tz": "Etc/GMT-2" })),
        )
        .expect("ok");
        assert!(zoned.as_str().expect("s").ends_with("+02:00"), "{zoned}");
        let bad_tz = date(
            &clock,
            &args(serde_json::json!({ "op": "now", "tz": "Mars/Olympus" })),
        );
        assert!(matches!(bad_tz, Err(f) if f.code == "NIKA-BUILTIN-DATE-001"));
    }

    #[test]
    fn date_format_and_parse_speak_strftime() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        let formatted = date(
            &clock,
            &args(serde_json::json!({
                "op": "format", "input": "2026-01-02T03:04:05Z", "format": "%Y-%m-%d %H:%M"
            })),
        )
        .expect("ok");
        assert_eq!(formatted, serde_json::json!("2026-01-02 03:04"));

        // parse without an offset reads as UTC (spec default tz).
        let parsed = date(
            &clock,
            &args(serde_json::json!({
                "op": "parse", "input": "2026-01-02", "format": "%Y-%m-%d"
            })),
        )
        .expect("ok");
        assert_eq!(parsed, serde_json::json!("2026-01-02T00:00:00Z"));

        // parse WITH an offset is exact-instant.
        let offset = date(
            &clock,
            &args(serde_json::json!({
                "op": "parse", "input": "2026-01-02 03:00 +0200", "format": "%Y-%m-%d %H:%M %z"
            })),
        )
        .expect("ok");
        assert_eq!(offset, serde_json::json!("2026-01-02T01:00:00Z"));

        // format honors `tz:` (the ToolDef declared it all along — the
        // impl hardcoded UTC silently · ambition-audit #5). A fixed-
        // offset zone keeps the pin deterministic.
        let paris_ish = date(
            &clock,
            &args(serde_json::json!({
                "op": "format", "input": "2026-01-02T03:04:05Z",
                "format": "%Y-%m-%d %H:%M", "tz": "Etc/GMT-2"
            })),
        )
        .expect("ok");
        assert_eq!(paris_ish, serde_json::json!("2026-01-02 05:04"));
        let bad_tz = date(
            &clock,
            &args(serde_json::json!({
                "op": "format", "input": "2026-01-02T03:04:05Z",
                "format": "%H", "tz": "Mars/Olympus"
            })),
        );
        assert!(
            matches!(&bad_tz, Err(f) if f.code == "NIKA-BUILTIN-DATE-001"),
            "{bad_tz:?}"
        );

        let bad = date(
            &clock,
            &args(serde_json::json!({
                "op": "parse", "input": "abc", "format": "%Y-%m-%d"
            })),
        );
        assert!(matches!(bad, Err(f) if f.code == "NIKA-BUILTIN-DATE-001"));
    }

    #[test]
    fn date_diff_units_are_the_closed_set() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        let diff_in = |unit: &str| {
            date(
                &clock,
                &args(serde_json::json!({
                    "op": "diff", "start": "2026-01-01T00:00:00Z",
                    "end": "2026-01-02T01:30:00Z", "unit": unit
                })),
            )
        };
        assert_eq!(diff_in("seconds").expect("ok"), serde_json::json!(91_800));
        assert_eq!(
            diff_in("milliseconds").expect("ok"),
            serde_json::json!(91_800_000)
        );
        assert_eq!(diff_in("minutes").expect("ok"), serde_json::json!(1530));
        assert_eq!(diff_in("hours").expect("ok"), serde_json::json!(25));
        assert_eq!(diff_in("days").expect("ok"), serde_json::json!(1));
        // weeks is a fixed 7-day span (this 25h30m fixture floors to 0).
        assert_eq!(diff_in("weeks").expect("ok"), serde_json::json!(0));
        assert!(diff_in("fortnights").is_err());
        // A genuine multi-week span floors to whole weeks.
        let three_weeks = date(
            &clock,
            &args(serde_json::json!({
                "op": "diff", "start": "2026-01-01T00:00:00Z",
                "end": "2026-01-23T00:00:00Z", "unit": "weeks"
            })),
        )
        .expect("ok");
        assert_eq!(three_weeks, serde_json::json!(3), "22 days = 3 whole weeks");
        // Negative when end precedes start (signed integer semantics).
        let negative = date(
            &clock,
            &args(serde_json::json!({
                "op": "diff", "start": "2026-01-02T00:00:00Z", "end": "2026-01-01T00:00:00Z"
            })),
        )
        .expect("ok");
        assert_eq!(negative, serde_json::json!(-86_400));
    }

    #[test]
    fn date_shift_handles_calendar_units() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        let shift = |op: &str, duration: &str| -> serde_json::Value {
            date(
                &clock,
                &args(serde_json::json!({
                    "op": op, "base": "2026-01-01T00:00:00Z", "duration": duration
                })),
            )
            .expect("ok")
        };
        // Calendar units (the bug): a bare Timestamp rejected these because
        // weeks/days/months/years have no fixed length without a zone.
        // Routing through a Zoned makes them all land.
        assert_eq!(
            shift("add", "2 weeks"),
            serde_json::json!("2026-01-15T00:00:00Z")
        );
        assert_eq!(
            shift("add", "14 days"),
            serde_json::json!("2026-01-15T00:00:00Z")
        );
        assert_eq!(
            shift("add", "1 month"),
            serde_json::json!("2026-02-01T00:00:00Z")
        );
        // Mixed calendar + clock units.
        assert_eq!(
            shift("add", "1 day 2 hours"),
            serde_json::json!("2026-01-02T02:00:00Z")
        );
        // Clock-only still works (the path that worked before the fix).
        assert_eq!(
            shift("add", "48 hours"),
            serde_json::json!("2026-01-03T00:00:00Z")
        );
        assert_eq!(
            shift("add", "PT1h"),
            serde_json::json!("2026-01-01T01:00:00Z")
        );
        // Subtract symmetry — add then subtract the same span round-trips.
        assert_eq!(
            shift("subtract", "1 month"),
            serde_json::json!("2025-12-01T00:00:00Z")
        );
        assert_eq!(
            shift("subtract", "2 weeks"),
            serde_json::json!("2025-12-18T00:00:00Z")
        );
    }

    #[test]
    fn date_shift_is_dst_aware_in_a_tz() {
        use nika_kernel_mock::MockClock;
        let clock = MockClock::new();
        // 2026-03-08T05:00:00Z is 2026-03-08T00:00:00-05:00 in New York,
        // hours before the spring-forward (DST starts 02:00 local that day).
        // Adding ONE CALENDAR DAY lands on the next civil midnight, now in
        // EDT (-04:00) → the instant is only 23h later: DST-correct.
        let civil = date(
            &clock,
            &args(serde_json::json!({
                "op": "add", "base": "2026-03-08T05:00:00Z",
                "duration": "1 day", "tz": "America/New_York"
            })),
        )
        .expect("ok");
        assert_eq!(civil, serde_json::json!("2026-03-09T04:00:00Z"));
        // Contrast: a fixed 24-CLOCK-HOUR span is a flat instant offset
        // (DST-blind) → 24h later exactly. Proves the Zoned path is doing
        // the calendar work, not measuring a flat duration.
        let clockwise = date(
            &clock,
            &args(serde_json::json!({
                "op": "add", "base": "2026-03-08T05:00:00Z",
                "duration": "24 hours", "tz": "America/New_York"
            })),
        )
        .expect("ok");
        assert_eq!(clockwise, serde_json::json!("2026-03-09T05:00:00Z"));
        // A bad tz: surfaces the canonical date code (not a silent UTC fallback).
        let bad_tz = date(
            &clock,
            &args(serde_json::json!({
                "op": "add", "base": "2026-03-08T05:00:00Z",
                "duration": "1 day", "tz": "Mars/Olympus"
            })),
        );
        assert!(
            matches!(&bad_tz, Err(f) if f.code == "NIKA-BUILTIN-DATE-001"),
            "{bad_tz:?}"
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
