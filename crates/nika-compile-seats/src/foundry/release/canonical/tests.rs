// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::json;

use super::*;

/// The producer's own bytes for this row, as Python's
/// `json.dumps(row, sort_keys=True, ensure_ascii=False, separators=(",", ":"))` wrote them, and
/// their sha256: the reader recomputes exactly these, or the row is refused.
const PRODUCER_TEXT: &str = r#"{"alpha":{"a":null,"b":true},"id":"pattern:x","kind":"pattern","title":"Résumé \"quoted\" \\ back\nline\ttab","zeta":[3,-7,18446744073709551615]}"#;
const PRODUCER_DIGEST: &str = "7e7c6bf3ecee4238907d6483abd5eea3df223a8c6a9c89dce6e6bfbc33cde119";

#[test]
fn a_rows_canonical_text_and_digest_are_the_producers_byte_for_byte() {
    let row = json!({
        "title": "Résumé \"quoted\" \\ back\nline\ttab",
        "id": "pattern:x",
        "kind": "pattern",
        "zeta": [3, -7, u64::MAX],
        "alpha": {"b": true, "a": null},
        "sha256": "the signature is never part of what it signs",
    });
    let mut unsigned = row.clone();
    unsigned.as_object_mut().unwrap().remove("sha256");
    assert_eq!(canonical_json(&unsigned).as_deref(), Some(PRODUCER_TEXT));
    assert_eq!(row_digest(&row).as_deref(), Some(PRODUCER_DIGEST));
    assert_eq!(
        row_digest(&unsigned).as_deref(),
        Some(PRODUCER_DIGEST),
        "the `sha256` field is left out of the digest"
    );
}

#[test]
fn keys_sort_by_code_point_as_the_producer_sorts_them() {
    // Python's sorted() over str: code points, not locale, not case-folded.
    let value = json!({"bé": 1, "bê": 2, "B": 3, "a": 4, "☃": 5});
    assert_eq!(
        canonical_json(&value).as_deref(),
        Some(r#"{"B":3,"a":4,"bé":1,"bê":2,"☃":5}"#)
    );
    assert_eq!(
        sha256(&canonical_json(&value).unwrap()),
        "9f753a3553f89d1f07c41da57d331228328199c851239b6ba8c2432947c29455"
    );
}

#[test]
fn a_number_that_is_not_an_integer_has_no_canonical_form() {
    for value in [json!(1.5), json!({"a": [0.1]}), json!(1e2), json!(-0.0)] {
        assert_eq!(canonical_json(&value), None, "{value}");
    }
    assert_eq!(row_digest(&json!({"id": "x", "weight": 0.5})), None);
    assert_eq!(row_digest(&json!(["not", "an", "object"])), None);
    assert_eq!(canonical_json(&json!(-7)).as_deref(), Some("-7"));
}

#[test]
fn strict_json_refuses_a_key_stated_twice_at_any_depth() {
    for (text, key) in [
        (r#"{"a": 1, "a": 2}"#, "a"),
        (r#"{"outer": {"inner": 1, "inner": 1}}"#, "inner"),
        (r#"[{"k": "v", "k": "v"}]"#, "k"),
    ] {
        assert_eq!(
            strict_json(text, 64),
            Err(StrictJsonError::DuplicateKey(key.to_owned())),
            "{text}"
        );
    }
}

#[test]
fn strict_json_refuses_what_is_not_one_json_value() {
    for text in [
        "",
        "{",
        r#"{"a": 1} trailing"#,
        r#"{"a": 1}{"b": 2}"#,
        "{'single': 'quotes'}",
        r#"{"a": NaN}"#,
        r#"{"a": "\ud800"}"#,
    ] {
        assert!(
            matches!(strict_json(text, 64), Err(StrictJsonError::Malformed(_))),
            "{text:?}"
        );
    }
    assert_eq!(
        strict_json(r#" {"b": [true, null, "x"], "a": -1} "#, 64),
        Ok(json!({"a": -1, "b": [true, null, "x"]}))
    );
}

#[test]
fn depth_counts_arrays_and_objects_only() {
    // Sixteen nested arrays put the innermost at depth 16; a string inside it adds no depth.
    let sixteen = format!("{}\"x\"{}", "[".repeat(16), "]".repeat(16));
    assert!(strict_json(&sixteen, 64).is_ok(), "{sixteen}");
    let seventeen = format!("{}{}", "[".repeat(17), "]".repeat(17));
    assert!(
        matches!(
            strict_json(&seventeen, 64),
            Err(StrictJsonError::Malformed(_))
        ),
        "{seventeen}"
    );
}

#[test]
fn a_text_holds_at_most_its_values_member_names_included() {
    // One object, three member names and three strings: seven values.
    let text = r#"{"a": "x", "b": "y", "c": "z"}"#;
    assert!(strict_json(text, 7).is_ok());
    assert!(matches!(
        strict_json(text, 6),
        Err(StrictJsonError::Malformed(_))
    ));
    // A key stated twice counts twice, and so does the value it first held: five values here.
    let twice = r#"{"a": "x", "a": "y"}"#;
    assert_eq!(
        strict_json(twice, 5),
        Err(StrictJsonError::DuplicateKey("a".to_owned()))
    );
    assert!(matches!(
        strict_json(twice, 4),
        Err(StrictJsonError::Malformed(_))
    ));
}

#[test]
fn a_duplicate_key_is_reported_only_in_a_text_otherwise_valid_within_its_bounds() {
    let deep = format!(r#"{{"a": "x", "a": {}{}}}"#, "[".repeat(17), "]".repeat(17));
    assert!(
        matches!(strict_json(&deep, 64), Err(StrictJsonError::Malformed(_))),
        "a bound comes before the duplicate"
    );
    assert!(
        matches!(
            strict_json(r#"{"a": "x", "a": "y""#, 64),
            Err(StrictJsonError::Malformed(_))
        ),
        "a syntax fault comes before the duplicate"
    );
    assert_eq!(
        strict_json(r#"{"a": "x", "a": "y"}"#, 64),
        Err(StrictJsonError::DuplicateKey("a".to_owned()))
    );
    // The first value of a key stated again is judged in full: a fault there is the text's.
    let lone = format!("{{\"a\": \"{}ud800\", \"a\": \"x\"}}", '\\');
    let first_deep = format!(r#"{{"a": {}{}, "a": "x"}}"#, "[".repeat(16), "]".repeat(16));
    // The object, its first name and the array, then 62 strings: the bound breaks inside the
    // first value.
    let first_many = format!(r#"{{"a": [{}], "a": "x"}}"#, vec![r#""""#; 62].join(","));
    for masked in [lone, first_deep, first_many] {
        assert!(
            matches!(strict_json(&masked, 64), Err(StrictJsonError::Malformed(_))),
            "{masked}"
        );
    }
}

#[test]
fn a_line_holds_nothing_after_its_value() {
    assert_eq!(strict_line(r#"{"a":"x"}"#, 8), Ok(json!({"a": "x"})));
    for line in [r#"{"a":"x"} "#, "{\"a\":\"x\"}\t", "{\"a\":\"x\"}\r"] {
        assert!(
            matches!(strict_line(line, 8), Err(StrictJsonError::Malformed(_))),
            "{line:?}"
        );
    }
    // The grammar of the line comes before a key stated twice.
    assert!(matches!(
        strict_line(r#"{"a":"x","a":"y"} "#, 8),
        Err(StrictJsonError::Malformed(_))
    ));
    assert_eq!(
        strict_line(r#"{"a":"x","a":"y"}"#, 8),
        Err(StrictJsonError::DuplicateKey("a".to_owned()))
    );
}

#[test]
fn a_number_anywhere_is_found() {
    assert!(holds_number(&json!({"a": [{"b": 0}]})));
    assert!(holds_number(&json!(-0.0)));
    assert!(!holds_number(&json!({"a": ["x", true, null, {"b": "1"}]})));
}

#[test]
fn es_number_is_the_rfc_8785_text_of_a_double() {
    // RFC 8785 Appendix B, then the layout's edges; every text is the producer's `es_number`.
    let cases: [(u64, &str); 41] = [
        (0x0000_0000_0000_0000, "0"),
        (0x8000_0000_0000_0000, "0"),
        (0x0000_0000_0000_0001, "5e-324"),
        (0x8000_0000_0000_0001, "-5e-324"),
        (0x7fef_ffff_ffff_ffff, "1.7976931348623157e+308"),
        (0xffef_ffff_ffff_ffff, "-1.7976931348623157e+308"),
        (0x4340_0000_0000_0000, "9007199254740992"),
        (0xc340_0000_0000_0000, "-9007199254740992"),
        (0x4430_0000_0000_0000, "295147905179352830000"),
        (0x44b5_2d02_c7e1_4af5, "9.999999999999997e+22"),
        (0x44b5_2d02_c7e1_4af6, "1e+23"),
        (0x44b5_2d02_c7e1_4af7, "1.0000000000000001e+23"),
        (0x444b_1ae4_d6e2_ef4e, "999999999999999700000"),
        (0x444b_1ae4_d6e2_ef4f, "999999999999999900000"),
        (0x444b_1ae4_d6e2_ef50, "1e+21"),
        (0x3eb0_c6f7_a0b5_ed8c, "9.999999999999997e-7"),
        (0x3eb0_c6f7_a0b5_ed8d, "0.000001"),
        (0x41b3_de43_5555_5553, "333333333.3333332"),
        (0x41b3_de43_5555_5554, "333333333.33333325"),
        (0x41b3_de43_5555_5555, "333333333.3333333"),
        (0x41b3_de43_5555_5556, "333333333.3333334"),
        (0x41b3_de43_5555_5557, "333333333.33333343"),
        (0xbecb_f647_612f_3696, "-0.0000033333333333333333"),
        (0x4314_3ff3_c1cb_0959, "1424953923781206.2"),
        (0x3ff0_0000_0000_0000, "1"),
        (0xbff8_0000_0000_0000, "-1.5"),
        (0x3fb9_9999_9999_999a, "0.1"),
        (0x4059_0000_0000_0000, "100"),
        (0x4415_af1d_78b5_8c40, "100000000000000000000"),
        (0x4454_542b_a12a_337c, "1.5e+21"),
        (0x3e7a_d7f2_9abc_af48, "1e-7"),
        (0x3e84_21f5_f40d_8376, "1.5e-7"),
        (0x441a_c53a_7e04_bcda, "123456789012345680000"),
        (0x3eb4_b3fd_5942_cd96, "0.000001234"),
        (0x4340_0000_0000_0001, "9007199254740994"),
        (0x7e41_eb2d_6600_5835, "1.5e+300"),
        (0x0010_0000_0000_0000, "2.2250738585072014e-308"),
        (0x3fd3_3333_3333_3334, "0.30000000000000004"),
        (0x3ff0_0000_0000_0001, "1.0000000000000002"),
        (0x000f_ffff_ffff_ffff, "2.225073858507201e-308"),
        (0x4024_0000_0000_0000, "10"),
    ];
    for (bits, text) in cases {
        assert_eq!(
            es_number(f64::from_bits(bits)).as_deref(),
            Some(text),
            "{bits:#018x}"
        );
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(es_number(value), None);
    }
}

#[test]
fn a_literal_is_canonical_exactly_when_jcs_writes_it_back() {
    // The producer's `canonical_number_literal` verdict for each literal.
    let cases = [
        ("0", true),
        ("-0", false),
        ("1", true),
        ("-1", true),
        ("1.0", false),
        ("1e3", false),
        ("1E3", false),
        ("1e+21", true),
        ("1e21", false),
        ("0.1", true),
        ("0.10", false),
        ("1.5", true),
        ("-1.5", true),
        ("9007199254740991", true),
        ("9007199254740992", true),
        ("9007199254740993", false),
        ("-9007199254740993", false),
        ("15000000000000000", true),
        ("100000000000000000000", true),
        ("1000000000000000000000", false),
        ("1e+300", true),
        ("1.5e+300", true),
        ("1e-7", true),
        ("0.0000001", false),
        ("0.000001", true),
        ("1e-6", false),
        ("5e-324", true),
        ("1e-400", false),
        ("1e400", false),
        ("18446744073709551616", false),
        ("123456789012345680000", true),
        ("0.30000000000000004", true),
        ("1.7976931348623157e+308", true),
        ("1.7976931348623159e+308", false),
        ("2e-7", true),
        ("-0.0", false),
    ];
    for (literal, canonical) in cases {
        let written = strict_json(literal, 8).ok().map(|value| jcs_json(&value));
        assert_eq!(
            written.as_deref() == Some(literal),
            canonical,
            "{literal} → {written:?}"
        );
    }
}

#[test]
fn a_number_is_the_correctly_rounded_double_of_its_literal() {
    // Halfway cases and the largest subnormal: a best-effort float parse may miss them.
    for (literal, bits) in [
        (
            "1.00000000000000011102230246251565404236316680908203125",
            0x3ff0_0000_0000_0000_u64,
        ),
        (
            "1.00000000000000011102230246251565404236316680908203126",
            0x3ff0_0000_0000_0001,
        ),
        ("2.2250738585072011e-308", 0x000f_ffff_ffff_ffff),
        ("9007199254740993.0", 0x4340_0000_0000_0000),
        ("-18446744073709551617", 0xc3f0_0000_0000_0000),
    ] {
        let value = strict_json(&format!("[{literal}]"), 8).unwrap();
        assert_eq!(value[0].as_f64().map(f64::to_bits), Some(bits), "{literal}");
    }
    let overflow = strict_json("[1.7976931348623159e+308]", 8);
    assert!(
        matches!(overflow, Err(StrictJsonError::Malformed(_))),
        "a literal past the largest double is not a number: {overflow:?}"
    );
}

#[test]
fn every_number_literal_is_found_outside_strings_in_document_order() {
    let text = r#"{"a\"1":[-1.5e+3,{"b":"2\\"},3],"c4":"5",  "d":0}"#;
    assert_eq!(number_literals(text), ["-1.5e+3", "3", "0"]);
    assert_eq!(
        strict_json(text, 64),
        Ok(json!({"a\"1": [-1500.0, {"b": "2\\"}, 3], "c4": "5", "d": 0}))
    );
}

#[test]
fn a_profile_r2_row_digest_is_the_producers_over_jcs_numbers() {
    // The producer's `canonical` and `row_digest` of this exact line (profile r2).
    let line = r#"{"attrs":{"confidence":0.85,"observed_count":15000000000000000},"big":1e+21,"id":"pattern:x","kind":"pattern","neg":-1.5,"sha256":"ignored","small":1e-7,"title":"Résumé"}"#;
    let value = strict_line(line, 64).unwrap();
    assert_eq!(jcs_json(&value), line);
    assert_eq!(
        jcs_row_digest(&value).as_deref(),
        Some("64cbfdf158a46dc2b71410d6fb0fb4d9006aa50a47127ab616ccfffafaaf566c")
    );
    assert_eq!(jcs_row_digest(&json!([1])), None);
    assert_eq!(
        canonical_json(&value),
        None,
        "profile r1 has no fractional numbers"
    );
}

/// Differential evidence against the producer's `es_number`: build with `NIKA_ES_DIFFERENTIAL`
/// naming a file of `<bits hex> <text>` lines it wrote, and run the ignored tests.
#[test]
#[ignore = "differential evidence: needs NIKA_ES_DIFFERENTIAL at build time"]
fn es_number_matches_the_producer_on_a_differential_corpus() {
    let Some(path) = option_env!("NIKA_ES_DIFFERENTIAL") else {
        panic!("NIKA_ES_DIFFERENTIAL names the corpus at build time");
    };
    let corpus = std::fs::read_to_string(path).unwrap();
    let mut compared = 0_usize;
    for line in corpus.lines() {
        let (bits, text) = line.split_once(' ').unwrap();
        let value = f64::from_bits(u64::from_str_radix(bits, 16).unwrap());
        assert_eq!(es_number(value).as_deref(), Some(text), "{bits}");
        compared += 1;
    }
    assert!(compared > 300_000, "{compared} doubles compared");
}
