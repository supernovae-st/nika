// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

fn evaluate(program: &str, input: &serde_json::Value) -> Result<serde_json::Value, super::Refusal> {
    super::run(program, input)
}

#[test]
fn tonumber_rejects_an_empty_operand_inside_an_aggregate() {
    assert!(evaluate("[.[] | tonumber] | add", &serde_json::json!(["", "2"])).is_err());
}

#[test]
fn tonumber_rejects_a_whitespace_operand_inside_an_aggregate() {
    assert!(evaluate("[.[] | tonumber] | add", &serde_json::json!([" \t\n", "2"])).is_err());
}

#[test]
fn tonumber_rejects_several_numbers_inside_one_operand() {
    assert!(evaluate("[.[] | tonumber] | add", &serde_json::json!(["1 2", "3"])).is_err());
}

#[test]
fn tonumber_keeps_valid_numbers_and_uint64_values_exact() {
    for (input, expected) in [
        (serde_json::json!("0"), serde_json::json!(0)),
        (serde_json::json!(" 12 "), serde_json::json!(12)),
        (serde_json::json!("-2.5"), serde_json::json!(-2.5)),
        (serde_json::json!("1e2"), serde_json::json!(100.0)),
        (
            serde_json::json!("18446744073709551615"),
            serde_json::json!(u64::MAX),
        ),
        (serde_json::json!(42), serde_json::json!(42)),
    ] {
        assert_eq!(evaluate("tonumber", &input).expect("one number"), expected);
    }
}

#[test]
fn tonumber_rejects_non_numbers_without_coercing_them() {
    for input in [
        serde_json::json!("abc"),
        serde_json::json!("null"),
        serde_json::json!("true"),
        serde_json::json!("[]"),
        serde_json::json!(null),
        serde_json::json!(false),
        serde_json::json!([]),
        serde_json::json!({}),
    ] {
        assert!(evaluate("tonumber", &input).is_err());
    }
}

#[test]
fn tonumber_preserves_an_explicit_try_policy() {
    assert_eq!(
        evaluate(
            "[.[] | try tonumber] | add",
            &serde_json::json!(["", "abc", "2"])
        )
        .expect("the authored try deliberately skips errors"),
        serde_json::json!(2)
    );
}

#[test]
fn tonumber_does_not_change_the_fromjson_stream() {
    assert_eq!(
        evaluate("[fromjson]", &serde_json::json!("1 2")).expect("a collected JSON stream"),
        serde_json::json!([1, 2])
    );
}

/// The shared conformance probes hold in the verifier: each program emits its one expected
/// value, or a refusal naming the expected fault.
#[test]
fn the_shared_shadow_probes_hold_in_the_verifier() {
    let set: serde_json::Value =
        serde_json::from_str(nika_cap::JQ_STD_SHADOW_PROBES).expect("the probe set is JSON");
    let failed: Vec<String> = set["probes"]
        .as_array()
        .expect("probes")
        .iter()
        .filter_map(|probe| {
            let program = probe["program"].as_str().expect("a program");
            let result = evaluate(program, &probe["input"]);
            let held = match probe["error"].as_str() {
                Some(fault) => result
                    .as_ref()
                    .is_err_and(|refusal| refusal.0.contains(fault)),
                None => result.as_ref().ok() == Some(&probe["output"]),
            };
            (!held).then(|| format!("{}: {result:?}", probe["name"]))
        })
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
}

/// The expression of `task` in the READY candidate `request` compiles to, as the runtime runs it.
fn emitted(request: &nika_compile::CompileRequest, task: &str) -> String {
    let out = nika_compile::compile(request).expect("the request compiles");
    assert_eq!(out.status, nika_compile::CompileStatus::Ready, "{out:#?}");
    let doc: serde_json::Value =
        serde_yaml_bw::from_str(out.candidate.as_deref().expect("a candidate")).expect("YAML");
    doc["tasks"][task]["invoke"]["args"]["expression"]
        .as_str()
        .expect("the task's expression")
        .to_owned()
}

/// A request over ./rows.json as the host observed it: its keys and its value kinds.
fn observed(intent: &str, rows: &[serde_json::Value]) -> nika_compile::CompileRequest {
    let sample = nika_compile::observation::records(rows);
    let source = serde_json::json!({"path": "./rows.json", "state": "observed", "complete": false,
        "kind": "json", "columns": sample.columns, "common_columns": sample.common});
    nika_compile::CompileRequest::create(intent).with_knowledge(
        serde_json::json!({"observed": [source], "kinds": {"./rows.json": sample.kinds}}),
    )
}

const ABOVE: &str =
    "Read ./rows.json, keep only the rows whose points is above 1 and write them to ./above.json";

/// Under SKIP the number law reads its finiteness through `fromjson`, not the runtime's
/// refusing `tonumber`: an overflowing text is no number, its record is left out and the run
/// continues.
#[test]
fn skip_leaves_an_overflowing_text_out_and_the_run_continues() {
    let rows = [
        serde_json::json!({"name": "a", "points": "3"}),
        serde_json::json!({"name": "x", "points": "1e999"}),
        serde_json::json!({"name": "b", "points": 2}),
    ];
    let skip = observed(ABOVE, &rows).answer("const.rule_number_1", r#""skip""#);
    let kept = evaluate(
        &emitted(&skip, "compute"),
        &serde_json::json!({"records": rows}),
    )
    .expect("the run continues");
    let names: Vec<&str> = kept
        .as_array()
        .expect("the kept rows")
        .iter()
        .filter_map(|row| row["name"].as_str())
        .collect();
    assert_eq!(names, ["a", "b"]);
}

/// Under FAIL the number law still names the field and the value it cannot read: an
/// overflowing text stops the run with the law's own message, not the runtime's refusal.
#[test]
fn fail_names_the_field_of_an_overflowing_text() {
    let sample = [
        serde_json::json!({"name": "a", "points": 3}),
        serde_json::json!({"name": "b", "points": 2}),
    ];
    let compute = emitted(&observed(ABOVE, &sample), "compute");
    let later = serde_json::json!({"records": [{"name": "a", "points": 3}, {"name": "x", "points": "1e999"}]});
    let why = evaluate(&compute, &later).expect_err("the run stops").0;
    assert!(
        why.contains(r#"`points` is \"1e999\", not a number"#),
        "{why}"
    );
}

/// A JSON source holding an integer no finite f64 carries stops at the transport guard with its
/// own named message: the guard reads the transport text through `fromjson`.
#[test]
fn the_transport_guard_names_an_integer_no_finite_number_carries() {
    let parse = emitted(
        &observed(
            "Read ./rows.json and write it to ./copy.csv",
            &[serde_json::json!({"name": "a", "id": 1})],
        ),
        "parse_source",
    );
    let big = format!("1{}", "0".repeat(400));
    let source = serde_json::json!(format!(r#"[{{"name": "a", "id": {big}}}]"#));
    let why = evaluate(&parse, &source).expect_err("the run stops").0;
    assert!(
        why.contains(&format!("the number at 0.id is {big}"))
            && why.contains("carry it as no finite number")
            && why.contains("nothing is written"),
        "{why}"
    );
}

/// The law's finiteness test reads every text the number grammar accepts as it read it through
/// `tonumber`, and the grammar still rejects what it rejected: only a non-finite reading moves.
#[test]
fn the_law_reads_grammar_positives_and_rejections_as_before() {
    let text = serde_json::json!(nika_compile_reader::text::NUMBER_TEXT);
    let law = |read: &str| {
        format!(
            "[.[] | (type == \"number\" and (isinfinite or isnan | not)) or (type == \"string\" and test({text}) and ({read} | isinfinite or isnan | not))]"
        )
    };
    let positives = [
        "0", "-0", "12.50", "1e308", "-1e308", "5e-324", "1E5", "1e+5", " 7 ",
    ];
    let rejections = [
        "", " ", "1 2", "abc", "NaN", "Infinity", "+1", ".5", "5.", "01", "1,5",
    ];
    for (texts, reading) in [(&positives[..], true), (&rejections[..], false)] {
        let expected = serde_json::json!(vec![reading; texts.len()]);
        let texts = serde_json::json!(texts);
        assert_eq!(evaluate(&law("tonumber"), &texts), Ok(expected.clone()));
        assert_eq!(evaluate(&law("fromjson"), &texts), Ok(expected));
    }
}

/// An exact total or average whose value no finite f64 carries stops with the law's named
/// message, never with a parse failure of the infinity's own transport text.
#[test]
fn an_exact_aggregate_beyond_f64_is_named() {
    let sample = [
        serde_json::json!({"points": 1}),
        serde_json::json!({"points": 2}),
    ];
    // A 401-digit integer is a finite JSON number to the law; its exact average is not an f64.
    let big = format!("1{}", "0".repeat(400));
    for (intent, points) in [
        (
            "Read ./rows.json, compute the total of the points column and write the total to ./total.txt",
            ["1e308".to_owned(), "1e308".to_owned()],
        ),
        (
            "Read ./rows.json, compute the average of the points column and write the average to ./average.txt",
            [big.clone(), big.clone()],
        ),
    ] {
        let compute = emitted(&observed(intent, &sample), "compute");
        let rows = serde_json::json!({"records": [{"points": points[0]}, {"points": points[1]}]});
        let why = evaluate(&compute, &rows).expect_err("the run stops").0;
        assert!(
            why.contains("which no JSON number carries here (it would be written as no finite number), so nothing is written"),
            "{intent}: {why}"
        );
    }
}
