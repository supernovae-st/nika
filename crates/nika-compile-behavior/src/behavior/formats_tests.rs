// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The canonical readings: strict JSON with exact numbers, the `nika:convert` CSV reading, the
//! records of each format, and the sha256 every reading is bound to.

use super::formats::{
    Coverage, Format, Refusal, csv_records, json_document, json_lines, records, sha256_hex,
};
use super::numbers::Decimal;
use super::values::{Datum, value_at};

fn number(text: &str) -> Datum {
    Datum::Number(Decimal::from_law(text).expect("a number the law reads"))
}

#[test]
fn json_is_read_strictly_and_its_numbers_exactly() {
    let text = r#"{"a": 0.1000000000000000055511151231257827, "b": [1, 2.50, -0]}"#;
    let Ok(Datum::Record(fields)) = json_document(text) else {
        panic!("one well-formed object");
    };
    assert_eq!(
        fields.get("a"),
        Some(&number("0.1000000000000000055511151231257827"))
    );
    assert_ne!(fields.get("a"), Some(&number("0.1")));
    assert_eq!(
        fields.get("b"),
        Some(&Datum::List(vec![number("1"), number("2.5"), number("0")]))
    );
    assert_eq!(
        json_document("\"\\ud83d\\ude00\""),
        Ok(Datum::Text("\u{1f600}".to_owned()))
    );
    for bad in [
        r#"{"a": 1, "a": 2}"#,
        "[1, 2] x",
        "[1,]",
        "{\"a\" 1}",
        "01",
        "1e999",
        "\"\u{1}\"",
        "[\"\\ud800\"]",
        "nul",
        "",
    ] {
        assert!(
            matches!(json_document(bad), Err(Refusal::Malformed(_))),
            "{bad:?}"
        );
    }
}

#[test]
fn a_csv_is_read_as_nika_convert_reads_it() {
    let rows = csv_records("id,amount\n1,20\n2,\"1,5\"\n").expect("a well-formed table");
    assert_eq!(rows.len(), 2);
    assert_eq!(value_at(&rows[1], "amount"), &Datum::Text("1,5".to_owned()));
    assert_eq!(value_at(&rows[0], "amount"), &Datum::Text("20".to_owned()));
    assert!(rows.iter().all(|row| row.values().all(|cell| cell.loose)));
    assert_eq!(
        csv_records("id,amount\n").expect("a header alone"),
        Vec::new()
    );
    for bad in ["id,id\n1,2\n", "id,amount\n1\n", "id,amount\n1,2,3\n"] {
        assert!(
            matches!(csv_records(bad), Err(Refusal::Malformed(_))),
            "{bad:?}"
        );
    }
}

#[test]
fn records_are_an_array_of_objects_or_one_object_per_line() {
    assert_eq!(records(Format::Json, "[]").expect("no record"), Vec::new());
    for not_rows in [r#"{"rows": []}"#, "[1, 2]", "70"] {
        assert!(
            matches!(records(Format::Json, not_rows), Err(Refusal::NotRecords(_))),
            "{not_rows}"
        );
    }
    let lines = records(Format::JsonLines, "{\"a\": 1}\r\n\n{\"a\": 2}\n").expect("two lines");
    assert_eq!(lines.len(), 2);
    assert!(matches!(
        records(Format::JsonLines, "{\"a\": 1}\n[2]\n"),
        Err(Refusal::NotRecords(_))
    ));
    assert!(matches!(
        records(Format::Text, "a\nb\n"),
        Err(Refusal::NotRecords(_))
    ));
    assert_eq!(json_lines("").expect("no line"), Vec::new());
}

#[test]
fn every_reading_is_bound_to_the_sha256_of_its_bytes() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(Format::of_path("./out/top.JSON"), Some(Format::Json));
    assert_eq!(
        Format::of_path("./in/events.ndjson"),
        Some(Format::JsonLines)
    );
    assert_eq!(Format::of_path("./out/report.pdf"), None);
    assert_eq!(Coverage::Sampled.word(), "a sample");
}
