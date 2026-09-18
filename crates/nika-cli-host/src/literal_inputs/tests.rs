// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use serde_json::json;

fn parse(bytes: &[u8]) -> Result<BTreeMap<String, Value>, Refusal> {
    read(bytes)
}

#[test]
fn json_values_survive_without_interpretation() {
    let value = json!({"text":"@env:SECRET ${{ inputs.other }} café 🦋", "number":42,
        "string":"42", "nested":{"list":[true, null, 4.25, {"a":"b"}]}, "max":u64::MAX});
    let bytes = serde_json::to_vec(&value).expect("JSON");
    assert_eq!(
        serde_json::to_value(parse(&bytes).expect("literal map")).expect("JSON"),
        value
    );
}

#[test]
fn malformed_roots_bytes_and_duplicate_keys_refuse() {
    for bytes in [b"null".as_slice(), b"[]", b"42", b"true", br#""text""#] {
        assert_eq!(
            parse(bytes).expect_err("object required").code,
            "invalid_inputs_root"
        );
    }
    for bytes in [
        b"".as_slice(),
        b"{",
        b"{}{}",
        br#"{"a":1,"a":2}"#,
        br#"{"a":{"x":1,"x":2}}"#,
        br#"{"a":[{"x":1,"\u0078":2}]}"#,
    ] {
        assert_eq!(
            parse(bytes).expect_err("invalid JSON").code,
            "invalid_inputs_json"
        );
    }
    assert_eq!(
        parse(b"{\"a\":\"\xff\"}").expect_err("UTF-8").code,
        "invalid_inputs_utf8"
    );
}

#[test]
fn ceiling_is_inclusive_and_reader_consumption_is_bounded() {
    let mut exact = b"{}".to_vec();
    exact.resize(MAX_BYTES, b' ');
    assert!(parse(&exact).expect("exact ceiling").is_empty());
    exact.push(b' ');
    assert_eq!(
        parse(&exact).expect_err("one byte over").code,
        "inputs_too_large"
    );
    // An endless source must terminate at limit+1, never read until EOF.
    let mut source = std::io::repeat(b' ');
    assert_eq!(
        read(&mut source).expect_err("bounded endless input").code,
        "inputs_too_large"
    );
    let bytes = vec![b' '; MAX_BYTES * 2];
    let mut cursor = std::io::Cursor::new(bytes);
    assert!(read(&mut cursor).is_err());
    assert_eq!(cursor.position(), (MAX_BYTES + 1) as u64);
}

#[test]
fn reader_failure_is_typed() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("injected stdin failure"))
        }
    }
    assert_eq!(
        read(Broken).expect_err("read failure").code,
        "input_read_failed"
    );
}

const WF: &str = "nika: values\ninputs:\n  text: {type: string, required: true}\n  count: {type: integer, default: 7}\n  absent: {type: string}\n  nothing: {type: null}\ntasks:\n  echo:\n    infer: {prompt: hi}\n";
fn workflow() -> RawWorkflow {
    nika_schema::parse(
        WF,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture")
}

#[test]
fn canonical_fit_required_and_origins_are_used() {
    let wf = workflow();
    let values = parse(br#"{"text":"42","nothing":null}"#).expect("JSON");
    let bound = validate(&values, &wf).expect("bind");
    assert_eq!(bound.values, values);
    assert_eq!(bound.origins["text"], nika_runtime::InputOrigin::ApiCaller);
    assert_eq!(
        bound.origins["nothing"],
        nika_runtime::InputOrigin::ApiCaller
    );
    assert_eq!(bound.origins["count"], nika_runtime::InputOrigin::File);
    assert!(!bound.origins.contains_key("absent"));
    for (bytes, code) in [
        (b"{}".as_slice(), "NIKA-1708"),
        (br#"{"text":42}"#, "input_type_mismatch"),
        (br#"{"text":null}"#, "input_type_mismatch"),
        (br#"{"text":"x","count":"42"}"#, "input_type_mismatch"),
        (br#"{"text":"x","extra":1}"#, "unknown_input"),
    ] {
        assert_eq!(
            validate(&parse(bytes).expect("JSON"), &wf)
                .expect_err("refuse")
                .code,
            code
        );
    }
}
