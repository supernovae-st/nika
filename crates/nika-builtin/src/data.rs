// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Data builtins (6) — `jq` is THE data language (stdlib §Data).
//!
//! Pure functions of args (the clock/entropy-edged `date`/`uuid` live in
//! [`crate::date`]). Every contract — codes · defaults · the
//! exactly-one-output law — is cited from `nika-spec
//! stdlib/builtins-v0.1.md`, never restated.

use jaq_core::load::{Arena, Error as JqLoadError, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data as jaq_data};
use jaq_json::{Val, read};
use nika_cap::JqClock;
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
pub(crate) fn jq_with_clock(args: &Args, clock: JqClock) -> BuiltinOutcome {
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
        .chain(
            jaq_std::defs().filter(|definition| nika_cap::install_jq_definition(definition.name)),
        )
        .chain(jaq_json::defs())
        .chain(jq_std_corrections()?)
        .chain(jq_clock_defs()?);
    // The same typed policy as check/dataflow removes all host-reaching
    // symbols. Accepted clock forms are pure defs over the caller's run-start
    // value, never a read performed in this evaluator.
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs())
        .filter(|f| nika_cap::install_jq_native(f.0));
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
        .with_global_vars([nika_cap::JQ_RUN_START_VAR])
        .compile(modules)
        .map_err(|errs| {
            BuiltinFailure::new(
                C,
                format!("jq compile error — {}", render_jq_compile(&errs)),
            )
        })?;

    let ctx = Ctx::<jaq_data::JustLut<Val>>::new(
        &filter.lut,
        Vars::new([Val::from(clock.unix_seconds())]),
    );
    let mut single: Option<serde_json::Value> = None;
    for result in filter.id.run((ctx, val)) {
        let value = unwrap_without_exit(result)?;
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

#[cfg(test)]
pub(crate) fn jq(args: &Args) -> BuiltinOutcome {
    jq_with_clock(
        args,
        JqClock::at(nika_types::timestamp::Timestamp::from_unix_ns(
            1_700_000_000_125_000_000,
        )),
    )
}

fn unwrap_without_exit(result: jaq_core::ValX<'_, Val>) -> Result<Val, BuiltinFailure> {
    const C: &str = "NIKA-BUILTIN-JQ-001";
    match result {
        Ok(value) => Ok(value),
        Err(exception) => match exception.get_err() {
            Ok(error) => Err(BuiltinFailure::new(C, format!("jq runtime error: {error}"))),
            Err(exception) => match exception.get_halt() {
                Ok(exit_code) => Err(BuiltinFailure::new(
                    C,
                    format!("jq process control is withheld (halt code {exit_code})"),
                )),
                Err(exception) => Err(BuiltinFailure::new(
                    C,
                    format!("jq internal control-flow exception: {exception:?}"),
                )),
            },
        },
    }
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

fn jq_clock_defs() -> Result<Vec<jaq_core::load::parse::Def<&'static str>>, BuiltinFailure> {
    jaq_core::load::parse(nika_cap::JQ_CLOCK_DEFS, |parser| parser.defs()).ok_or_else(|| {
        BuiltinFailure::new(
            "NIKA-BUILTIN-JQ-001",
            "internal: the canonical jq clock definitions failed to parse",
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
            nika_cap::withheld_jq_policy_reason(name)
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
    let schema = parse_schema_arg(args, C1)?;
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

    let validator = jsonschema::validator_for(&schema).map_err(|e| {
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

/// `schema:` may be a JSON object OR a string `nika:read` just handed
/// over (the 2026-08-19 authoring miss: a `.json` file is text, and
/// treating that text as the schema value itself fails
/// `validator_for` with "not of types boolean, object").
fn parse_schema_arg(args: &Args, code: &'static str) -> Result<serde_json::Value, BuiltinFailure> {
    match args.get("schema") {
        None => Err(BuiltinFailure::new(code, "`schema:` is required")),
        Some(serde_json::Value::String(text)) => serde_json::from_str(text)
            .or_else(|_| {
                serde_yaml_bw::from_str(text).map_err(|yaml_err| {
                    BuiltinFailure::new(
                        code,
                        format!("`schema:` is a string that is neither JSON nor YAML: {yaml_err}"),
                    )
                })
            })
            .and_then(|parsed| match parsed {
                serde_json::Value::Object(_) | serde_json::Value::Bool(_) => Ok(parsed),
                other => Err(BuiltinFailure::new(
                    code,
                    format!("`schema:` string parsed as {other}, not a JSON Schema object/boolean"),
                )),
            }),
        Some(value) => Ok(value.clone()),
    }
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

/// Bytes under `content:`. A string is hashed as-is (the receipt of
/// prose). Any other JSON value is hashed as compact JSON — the 2026-08-19
/// miss was interpolating a task's object output and getting
/// `content: (string) is required` instead of a digest.
fn content_bytes(args: &Args, code: &'static str) -> Result<String, BuiltinFailure> {
    match args.get("content") {
        None | Some(serde_json::Value::Null) => {
            Err(BuiltinFailure::new(code, "`content:` is required"))
        }
        Some(serde_json::Value::String(text)) => Ok(text.clone()),
        Some(serde_json::Value::Number(n)) => Ok(n.to_string()),
        Some(serde_json::Value::Bool(flag)) => Ok(flag.to_string()),
        Some(other) => serde_json::to_string(other).map_err(|e| {
            BuiltinFailure::new(code, format!("`content:` could not be serialized: {e}"))
        }),
    }
}

/// Hash `content:` (blake3 default · sha256/sha512). md5/sha1 refused.
pub(crate) fn hash(args: &Args) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-HASH-001";
    let content = content_bytes(args, C)?;
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
mod tests;
