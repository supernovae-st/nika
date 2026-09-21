// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `data` batteries — a child module of [`super`], so they run under
//! `--lib`. They live in their own file for the reason the sibling
//! `permits_fit/tests.rs` does: `wc -l` on `data.rs` is the file-LOC gate
//! measure and it does NOT subtract `#[cfg(test)]`, so 1000 lines of
//! battery pushed a 646-line production module through the 1500 ceiling.
//! The basename `tests.rs` is also what the crate-size counter excludes —
//! test mass stops being charged against the 15k production budget.

use super::*;

pub(super) fn args(v: serde_json::Value) -> Args {
    match v {
        serde_json::Value::Object(map) => map,
        other => panic!("test arg must be an object, got {other}"),
    }
}

#[test]
fn jq_json_string_input_error_teaches_explicit_decoding_without_echoing_data() {
    for (input, expression) in [
        (r#"{"private":"secret-marker","items":[1]}"#, ".items"),
        (r#"["secret-marker"]"#, ".[]"),
    ] {
        let err = jq(&args(
            serde_json::json!({ "input": input, "expression": expression }),
        ))
        .expect_err("encoded containers remain strings");
        assert_eq!(err.code, "NIKA-BUILTIN-JQ-001");
        assert!(err.message.contains("`input` is a string"), "{err:?}");
        assert!(err.message.contains("fromjson"), "{err:?}");
        assert!(err.message.contains("nika:read"), "{err:?}");
        assert!(!err.message.contains("secret-marker"), "{err:?}");
    }
}

#[test]
fn jq_json_string_input_keeps_string_operations_and_explicit_fromjson() {
    let input = r#"{"items":[1,2]}"#;
    for (expression, expected) in [
        (".", serde_json::json!(input)),
        ("type", serde_json::json!("string")),
        ("length", serde_json::json!(input.len())),
        ("fromjson | .items", serde_json::json!([1, 2])),
    ] {
        let out = jq(&args(
            serde_json::json!({ "input": input, "expression": expression }),
        ))
        .expect("the caller controls decoding");
        assert_eq!(out, expected);
    }
}

#[test]
fn jq_json_string_input_does_not_relabel_unrelated_errors() {
    for (input, expression) in [
        ("plain", ".items"),
        ("123", ".items"),
        ("{broken", ".items"),
        (r#"{"items":1}"#, ". |"),
        (r#"{"items":1}"#, "fromjson | .items.x"),
        (r#"{"items":1}"#, "floor"),
        (r#"{"items":1}"#, "halt"),
    ] {
        let err = jq(&args(
            serde_json::json!({ "input": input, "expression": expression }),
        ))
        .expect_err("the original error still occurs");
        assert!(!err.message.contains("`input` is a string"), "{err:?}");
    }
}

/// #1578 · the keys of an OBJECT `input:` are bound as jq variables
/// (`$rows` · `$batch`), so the n8n/Zapier « stamp a sibling onto every
/// row » shape compiles — while `.` stays the whole input and `.batch`
/// inside `map` keeps jq's meaning (the element's own field).
#[test]
fn jq_binds_object_input_keys_as_variables() {
    let input = serde_json::json!({ "rows": [{"a": 1}, {"a": 2}], "batch": "B1", "n": 2 });
    for (expression, expected) in [
        (
            "$rows | map(. + {batch: $batch})",
            serde_json::json!([{"a": 1, "batch": "B1"}, {"a": 2, "batch": "B1"}]),
        ),
        ("$batch", serde_json::json!("B1")),
        ("($rows | length) == $n", serde_json::json!(true)),
        (
            ".rows | map(. + {batch: .batch}) | .[0].batch",
            serde_json::Value::Null,
        ),
        ("keys", serde_json::json!(["batch", "n", "rows"])),
    ] {
        let out = jq(&args(
            serde_json::json!({ "input": input, "expression": expression }),
        ))
        .unwrap_or_else(|e| panic!("{expression}: {e:?}"));
        assert_eq!(out, expected, "{expression}");
    }
}

/// Only identifier-shaped keys become variables · a non-object input binds
/// nothing · the run-start clock variable is never shadowed by a key.
#[test]
fn jq_binds_only_identifier_keys_and_never_shadows_the_clock() {
    for (input, expression, undefined) in [
        (
            serde_json::json!({ "batch-id": 1 }),
            "$batch_id",
            "$batch_id",
        ),
        (serde_json::json!([1]), "$rows", "$rows"),
        (serde_json::json!({ "rows": [1] }), "$rowz", "$rowz"),
    ] {
        let err = jq(&args(
            serde_json::json!({ "input": input, "expression": expression }),
        ))
        .expect_err(expression);
        assert_eq!(err.code, "NIKA-BUILTIN-JQ-001");
        assert!(
            err.message
                .contains(&format!("undefined filter or variable `{undefined}`")),
            "{err:?}"
        );
    }
    let clock = |input: serde_json::Value| {
        jq(&args(
            serde_json::json!({ "input": input, "expression": "$nika_run_start" }),
        ))
        .expect("the clock variable is always bound")
    };
    let unshadowed = clock(serde_json::json!(1));
    assert_ne!(unshadowed, serde_json::json!(0));
    assert_eq!(
        clock(serde_json::json!({ "nika_run_start": 0 })),
        unshadowed
    );
}

#[test]
fn validate_json_string_input_reports_shape_and_preserves_error_handles() {
    for (data, kind) in [
        (r#"{"private":"secret-marker"}"#, "object"),
        (r#"["secret-marker"]"#, "array"),
    ] {
        let report = validate(&args(serde_json::json!({
            "data": data, "schema": { "type": kind }
        })))
        .expect("invalid data is still a report");
        assert_eq!(report["valid"], false);
        let error = &report["errors"][0];
        assert_eq!(error["path"], "");
        assert_eq!(error["schema_path"], "/type");
        let message = error["message"].as_str().expect("message");
        assert!(message.contains("`data` is a string"), "{error}");
        assert!(message.contains("fromjson"), "{error}");
        assert!(!message.contains("secret-marker"), "{error}");
    }
}

#[test]
fn validate_json_string_input_keeps_string_schemas_and_explicit_yaml() {
    for (data, format, schema) in [
        (
            serde_json::json!(r#"{"items":[1]}"#),
            "json",
            serde_json::json!({ "type": "string" }),
        ),
        (
            serde_json::json!({ "items": [1] }),
            "json",
            serde_json::json!({ "type": "object" }),
        ),
        (
            serde_json::json!(r#"{"items":[1]}"#),
            "yaml",
            serde_json::json!({ "type": "object" }),
        ),
    ] {
        let report = validate(&args(serde_json::json!({
            "data": data, "format": format, "schema": schema
        })))
        .expect("report");
        assert_eq!(report, serde_json::json!({ "valid": true, "errors": [] }));
    }
}

#[test]
fn validate_json_string_input_does_not_relabel_other_validation_errors() {
    for (data, schema) in [
        (
            serde_json::json!("plain"),
            serde_json::json!({ "type": "object" }),
        ),
        (
            serde_json::json!(r#"{"x":1}"#),
            serde_json::json!({ "type": "integer" }),
        ),
        (
            serde_json::json!(r#"{"x":1}"#),
            serde_json::json!({ "type": "string", "maxLength": 1 }),
        ),
        (
            serde_json::json!({ "x": r#"{"y":1}"# }),
            serde_json::json!({ "properties": { "x": { "type": "object" } } }),
        ),
    ] {
        let report =
            validate(&args(serde_json::json!({ "data": data, "schema": schema }))).expect("report");
        assert_eq!(report["valid"], false);
        assert!(
            !report.to_string().contains("`data` is a string"),
            "{report}"
        );
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

#[test]
fn the_clock_is_the_caller_bound_run_start() {
    let out = jq(&args(
        serde_json::json!({ "expression": "now", "input": {} }),
    ))
    .expect("the accepted clock form is rebound");
    assert_eq!(out, serde_json::json!(1_700_000_000.125));
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

/// #1584 · `from: json` on a STRING input reads it as JSON text — the way
/// every other `from:` format reads its text — so an exec/read/recover
/// string that IS a JSON document converts; a string that is not JSON
/// text stays the string value it always was.
#[test]
fn convert_from_json_parses_json_text_input() {
    let csv = convert(&args(serde_json::json!({
        "input": r#"[{"sku":"NIKA-CLI","qty":2},{"sku":"AUDIT","qty":1}]"#,
        "from": "json", "to": "csv"
    })))
    .expect("JSON text is parsed");
    assert_eq!(csv, serde_json::json!("qty,sku\n2,NIKA-CLI\n1,AUDIT\n"));
    let yaml = convert(&args(serde_json::json!({
        "input": r#"{"a": 1}"#, "from": "json", "to": "yaml"
    })))
    .expect("ok");
    assert_eq!(yaml, serde_json::json!("a: 1\n"));
    let plain = convert(&args(serde_json::json!({
        "input": "hello", "from": "json", "to": "yaml"
    })))
    .expect("a non-JSON string is still a string value");
    assert_eq!(plain, serde_json::json!("hello\n"));
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
            data.contains(&format!("'{trigger}")) || data.contains("'\t") || data.contains('\r'),
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
