// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_compile::surface::sha256;
use serde_json::Value;

use super::*;
use crate::foundry::release::grammar::{blank, printable};

fn export() -> Value {
    serde_json::from_str(profile::EXPORT).unwrap()
}

#[test]
fn the_embedded_export_is_the_producers_profile_byte_for_byte() {
    assert_eq!(sha256(profile::EXPORT), profile::EXPORT_SHA256);
    assert!(profile::EXPORT.ends_with("}\n") && profile::EXPORT.lines().count() == 1);
}

#[test]
fn every_kind_keeps_its_role_and_only_a_block_executes() {
    let profile = profile().unwrap();
    let roles: Vec<(&str, &str, &str, Option<&str>, bool)> = profile
        .kinds()
        .iter()
        .map(|kind| {
            (
                kind.name(),
                kind.prefix(),
                kind.role(),
                kind.body_directory(),
                kind.executable(),
            )
        })
        .collect();
    assert_eq!(
        roles,
        [
            ("block", "block", "component", Some("blocks"), true),
            ("callable", "callable", "contract", None, false),
            (
                "capability_interface",
                "capability",
                "contract",
                None,
                false
            ),
            ("construct", "construct", "contract", None, false),
            (
                "counterexample",
                "counterexample",
                "boundary",
                Some("counterexamples"),
                false
            ),
            ("diagnostic", "diagnostic", "reference", None, false),
            ("example", "example", "case", Some("examples"), false),
            ("family", "family", "need", None, false),
            ("intent_facet", "facet", "vocabulary", None, false),
            ("pattern", "pattern", "structure", None, false),
            ("pattern_pack", "pack", "structure", None, false),
            ("repair_principle", "repair", "repair", None, false),
            ("skeleton", "skeleton", "structure", None, false),
            ("skill", "skill", "method", Some("skills"), false),
            ("source_artifact", "src", "provenance", None, false),
        ]
    );
    let stems: Vec<&str> = profile.kinds().iter().map(Kind::stem).collect();
    assert_eq!(stems[1], "callables");
    assert_eq!(stems[8], "intent_facets");
    assert_eq!(
        profile.kind_of_id("src:x").map(Kind::name),
        Some("source_artifact")
    );
    assert_eq!(
        profile.kind_of_id("facet:x").map(Kind::name),
        Some("intent_facet")
    );
    assert_eq!(profile.kind_of_id("nothing:x"), None);
}

#[test]
fn the_door_speaks_the_profiles_codes_in_its_order() {
    assert_eq!(export()["codes"], serde_json::json!(CODES.as_slice()));
}

#[test]
fn the_forbidden_table_and_white_space_are_the_shared_grammars() {
    let doc = export();
    let ranges: Vec<(u32, u32)> = doc["forbidden_code_points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            let at = |i: usize| u32::try_from(pair[i].as_u64().unwrap()).unwrap();
            (at(0), at(1))
        })
        .collect();
    let white: Vec<u32> = doc["white_space"]
        .as_array()
        .unwrap()
        .iter()
        .map(|point| u32::try_from(point.as_u64().unwrap()).unwrap())
        .collect();
    for c in (0..=0x0010_FFFF_u32).filter_map(char::from_u32) {
        let point = u32::from(c);
        let forbidden = ranges
            .iter()
            .any(|(low, high)| (*low..=*high).contains(&point));
        assert_eq!(forbidden, !printable(c, true), "U+{point:04X}");
        assert_eq!(
            white.contains(&point),
            blank(&c.to_string()),
            "U+{point:04X}"
        );
    }
}

#[test]
fn an_export_naming_another_grammar_or_bound_is_not_read() {
    for (from, to) in [
        (r#""line_bytes":2097152"#, r#""line_bytes":2097153"#),
        (r#""id_name":"[A-Za-z0-9]"#, r#""id_name":"[a-z0-9]"#),
        (
            r#""directories":["LICENSES","#,
            r#""directories":["LICENCES","#,
        ),
    ] {
        assert_eq!(profile::EXPORT.matches(from).count(), 1, "{from}");
        let changed = profile::EXPORT.replace(from, to);
        assert!(profile::read_export(&changed).is_err(), "{to}");
    }
    assert!(profile::read_export(profile::EXPORT).is_ok());
}

#[test]
fn a_body_path_is_its_kinds_directory_a_slug_and_its_extension() {
    let profile = profile().unwrap();
    for (path, kind) in [
        ("blocks/csv-filter.nika", Some("block")),
        (
            "counterexamples/total-r0-19c2e1a5.nika",
            Some("counterexample"),
        ),
        ("examples/stale.nika", Some("example")),
        ("skills/aggregate.md", Some("skill")),
        ("skills/aggregate.nika", None),
        ("blocks/.nika", None),
        ("blocks/Upper.nika", None),
        ("blocks/a/b.nika", None),
        ("knowledge/blocks.jsonl", None),
    ] {
        assert_eq!(profile.body_kind(path).map(Kind::name), kind, "{path}");
    }
    assert!(profile.in_layout("LICENSES/Apache-2.0.txt"));
    assert!(!profile.in_layout("LICENSES/MIT.txt"));
    assert!(profile.in_layout("NOTICE.md"));
    assert!(!profile.in_layout("README.md"));
}

/// A block's row whose check and outcome receipts both bind to `target`.
fn proven_block(target: &Value) -> Value {
    let bytes = "a".repeat(64);
    serde_json::json!({
        "file_sha256": bytes,
        "check_receipt": {"verifier_sha256": target["verifier_sha256"],
            "spec_sha": target["spec_sha"], "sha256": bytes, "verdict": "CURRENT_CHECKED",
            "codes": []},
        "outcome_receipt": {"engines": ["nika 0.123.0 (b9fd7a681)", target["binary"]],
            "oracle_sha256": "b".repeat(64), "sha256": bytes, "spec_sha": target["spec_sha"],
            "worlds": 17, "correct": 17, "runs": 34, "verdict": "OUTCOME_CORRECT"},
    })
}

/// F9 (amended 2026-10-10): a block proven on recorded worlds carries its check receipt and an
/// outcome receipt, both bound; the outcome receipt belongs to that level alone.
#[test]
fn a_block_proven_on_recorded_worlds_carries_both_receipts_bound() {
    let profile = profile().unwrap();
    let block = profile
        .kinds()
        .iter()
        .find(|kind| kind.name() == "block")
        .unwrap();
    assert_eq!(block.proof_levels(), ["CHECKED", "OUTCOME_PROVEN"]);
    let target = serde_json::json!({"binary": "nika 0.123.0 (a3017c495)",
        "spec_sha": "0".repeat(40), "verifier_sha256": "c".repeat(64)});
    let row = proven_block(&target);
    assert_eq!(
        evidence(block, "block:x", &row, &target, "OUTCOME_PROVEN"),
        Ok(())
    );
    let refused = evidence(block, "block:x", &row, &target, "CHECKED").unwrap_err();
    assert_eq!((refused.step, refused.code), ("F9", "ROW_EVIDENCE"));
    let mut checked = row.clone();
    checked["outcome_receipt"] = Value::Null;
    assert_eq!(
        evidence(block, "block:x", &checked, &target, "CHECKED"),
        Ok(())
    );
    // Each binding breaks the proof: other bytes, another spec, never the target binary, a world
    // judged wrong, a world never run, no world at all, no check receipt, no outcome receipt.
    for (pointer, value) in [
        ("/outcome_receipt/sha256", serde_json::json!("d".repeat(64))),
        (
            "/outcome_receipt/spec_sha",
            serde_json::json!("1".repeat(40)),
        ),
        (
            "/outcome_receipt/engines",
            serde_json::json!(["nika 0.123.0 (b9fd7a681)"]),
        ),
        ("/outcome_receipt/correct", serde_json::json!(16)),
        ("/outcome_receipt/runs", serde_json::json!(16)),
        ("/outcome_receipt/worlds", serde_json::json!(0)),
        ("/check_receipt", Value::Null),
        ("/outcome_receipt", Value::Null),
    ] {
        let mut broken = row.clone();
        *broken.pointer_mut(pointer).unwrap() = value;
        let refused = evidence(block, "block:x", &broken, &target, "OUTCOME_PROVEN");
        assert_eq!(
            refused.map_err(|r| r.code),
            Err("ROW_EVIDENCE"),
            "{pointer}"
        );
    }
}
