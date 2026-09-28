// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::{csv, jsonl, number_text, records};
use serde_json::json;

#[test]
fn the_number_text_law_reads_a_json_number_and_nothing_else() {
    // A plain decimal, and an exponent the JSON number grammar allows (R4 A6: `1.5e2` is 150).
    for text in [
        "0",
        "-0",
        "150",
        "120.50",
        "-3.5",
        "0.5",
        " 150 ",
        "\t42",
        "1.5e2",
        "1E+3",
        "-1e3",
        "0.1e3",
        "1e-999",
        "2E-1",
        " 2e1 ",
        "12345678901234567890",
    ] {
        assert!(number_text(text), "{text:?}");
    }
    for text in [
        "",
        " ",
        "007",
        "01.5",
        "+5",
        "+1e3",
        "1e",
        "1e+",
        "1.e3",
        ".5e3",
        "1e3.5",
        "1e2e3",
        "1e999",
        "-1e999",
        "1,5",
        "1 000",
        "Infinity",
        "-Infinity",
        "NaN",
        "nan",
        "0x10",
        ".5",
        "5.",
        "-",
        "--5",
        "n-a",
        "true",
        "null",
        "1.2.3",
        "\u{a0}150",
    ] {
        assert!(!number_text(text), "{text:?}");
    }
}

#[test]
fn a_record_sample_counts_every_raw_kind_and_quotes_no_value() {
    let rows = vec![
        json!({"id": 1, "amount": 120, "note": "sk-live-9f3a"}),
        json!({"id": 2, "amount": "150"}),
        json!({"id": 3, "amount": null}),
        json!({"id": 4, "amount": true}),
        json!({"id": 5, "amount": "n-a"}),
        json!({"id": 6, "amount": [2]}),
        json!({"id": 7, "amount": {"v": 1}}),
        json!({"id": 8, "amount": " "}),
        json!({"id": 9}),
        json!(10),
    ];
    let sample = records(&rows);
    assert_eq!(sample.columns, ["amount", "id", "note"]);
    assert_eq!(sample.kinds["sampled"], 10);
    assert_eq!(sample.kinds["nonobject"], 1);
    assert_eq!(
        sample.kinds["keys"]["amount"],
        json!({"number": 1, "number_text": 1, "null": 1, "boolean": 1, "text": 1, "array": 1,
            "object": 1, "empty": 1, "absent": 1})
    );
    assert_eq!(sample.kinds["keys"]["id"], json!({"number": 9}));
    assert_eq!(
        sample.kinds["keys"]["note"],
        json!({"text": 1, "absent": 8})
    );
    let text = sample.kinds.to_string();
    for value in ["sk-live-9f3a", "n-a", "150", "120"] {
        assert!(
            !text.contains(value),
            "a kind count never quotes a value: {text}"
        );
    }
    // Zero, false and a missing key stay three different facts.
    let zero = records(&[json!({"q": 0}), json!({"q": false}), json!({})]);
    assert_eq!(
        zero.kinds["keys"]["q"],
        json!({"number": 1, "boolean": 1, "absent": 1})
    );
}

#[test]
fn a_csv_sample_counts_its_columns_kinds_beside_the_same_categorical_values() {
    let head = "id;client;montant;statut\n1;Acme;1480.5;payé\n2;Bolt;135.25;impayé\n3;Cora;n-a;payé\n4;Dune;;payé\n";
    let sample = csv(head, false);
    assert_eq!(sample.columns, ["id", "client", "montant", "statut"]);
    assert_eq!(sample.delimiter, Some(';'));
    assert_eq!(sample.common, None);
    let statut = sample
        .values
        .iter()
        .find(|(k, _)| k == "statut")
        .map(|(_, v)| v);
    assert_eq!(statut, Some(&json!(["payé", "impayé"])));
    assert!(
        sample.values.iter().all(|(k, _)| k != "client"),
        "free text is never categorical"
    );
    assert_eq!(sample.kinds["sampled"], 4);
    assert_eq!(
        sample.kinds["keys"]["montant"],
        json!({"number_text": 2, "text": 1, "empty": 1})
    );
    assert_eq!(sample.kinds["keys"]["id"], json!({"number_text": 4}));
    // A row whose field count differs from the header's is skipped, as the values skip it.
    let cut = csv("a,b\n1,2\n3\n4,5\n", false);
    assert_eq!(cut.kinds["sampled"], 2);
    // A decomposed spelling is a categorical value exactly as the file spells it.
    let nfd = csv(
        "id,statut\n1,livre\u{301}\n2,en cours\n3,livre\u{301}\n",
        false,
    );
    let statut = nfd
        .values
        .iter()
        .find(|(k, _)| k == "statut")
        .map(|(_, v)| v);
    assert_eq!(statut, Some(&json!(["livre\u{301}", "en cours"])));
}

#[test]
fn jsonl_heads_are_parsed_line_by_line_and_a_cut_line_is_skipped() {
    let rows = jsonl("{\"k\": 1}\n\n{\"k\": \"x\"}\n{\"k\": ");
    assert_eq!(rows, [json!({"k": 1}), json!({"k": "x"})]);
    let sample = records(&rows);
    assert_eq!(sample.common, Some(vec!["k".to_owned()]));
    assert_eq!(sample.kinds["keys"]["k"], json!({"number": 1, "text": 1}));
}

#[test]
fn mixed_records_are_positive_keys_with_a_separate_common_set() {
    let rows = vec![json!({"id":1}), json!({"id":2, "status":"open"})];
    let sample = records(&rows);
    assert_eq!(sample.columns, ["id", "status"]);
    assert_eq!(sample.common, Some(vec!["id".to_owned()]));
    assert_eq!(
        records(&[json!({"id":1}), json!(null)]).common,
        Some(Vec::new())
    );
    assert!(records(&[]).columns.is_empty());
}

#[test]
fn an_empty_or_partial_sample_does_not_claim_a_complete_schema() {
    // Parsing a partial JSON document yields no records, never an empty schema.
    assert_eq!(jsonl("{\"id\":1}\n{\"status\":").len(), 1);
    assert_eq!(records(&jsonl("\n")).common, Some(Vec::new()));
}
