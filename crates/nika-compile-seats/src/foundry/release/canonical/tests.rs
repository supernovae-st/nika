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
