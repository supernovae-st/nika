// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

fn evaluate(
    program: &str,
    input: &serde_json::Value,
) -> Result<serde_json::Value, super::DataflowError> {
    super::eval_binding(
        "converted",
        program,
        input,
        nika_cap::JqClock::at(nika_types::timestamp::Timestamp::from_unix_ns(
            1_700_000_000_125_000_000,
        )),
    )
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
