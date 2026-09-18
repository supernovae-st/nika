// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as gate_matrix_conformance: this suite's WHOLE JOB is
// the shipped verify surface, and NIKA_SPEC_DIR is harness plumbing.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

//! The spec `runtime/trace` contract fixtures replayed through the REAL
//! `nika trace verify` (spec 17 · NEP-0007): each fixture's
//! `expected-verify.json` names the verdict — `clean` (OK · no finding
//! line) · `finding` (OK · the REQUIRED-witness FINDING rides) ·
//! `forged` (FILE · the chain walk breaks) · `incomplete` (the missing end is
//! named) · `refused` (the journal is rejected before a walk). Optional
//! `cost_replay` expectations pin replayed/refused/unrecorded rendering. The
//! expectation is READ from the fixture, never hand-written here — the golden
//! swap that flips 001 from `finding` to `clean` flips this suite with it.
//!
//! The spec dir resolves from `$NIKA_SPEC_DIR` or the sibling checkout
//! (`<engine>/../spec`) — the suite HARD-FAILS when missing (the
//! conformance gate must never silently skip).

use std::path::PathBuf;

fn spec_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NIKA_SPEC_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../spec")
}

#[test]
fn runtime_trace_fixtures_hold_their_verify_verdict() {
    let root = spec_dir().join("conformance/tests/runtime/trace");
    assert!(
        root.is_dir(),
        "conformance dir missing: {} — set NIKA_SPEC_DIR",
        root.display()
    );
    let mut seen = 0_usize;
    let mut entries: Vec<_> = std::fs::read_dir(&root)
        .expect("readable fixture dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    for dir in entries {
        let expected_path = dir.join("expected-verify.json");
        let trace = dir.join("trace.ndjson");
        if !expected_path.is_file() || !trace.is_file() {
            continue;
        }
        seen += 1;
        let expected: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&expected_path).expect("expected json"))
                .expect("valid expected-verify.json");
        let verdict = expected["verdict"].as_str().expect("a verdict string");
        let out = nika_cli::verbs::trace_verify::verify(&trace.to_string_lossy());
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        match verdict {
            "clean" => {
                assert_eq!(out.code, 0, "{name}: clean verdict exits OK: {}", out.text);
                assert!(
                    !out.text.contains("FINDING"),
                    "{name}: clean carries no finding line: {}",
                    out.text
                );
            }
            "finding" => {
                assert_eq!(out.code, 0, "{name}: a finding never fails: {}", out.text);
                assert!(
                    out.text.contains("FINDING"),
                    "{name}: the REQUIRED-witness finding rides: {}",
                    out.text
                );
            }
            "forged" => {
                assert_eq!(
                    out.code, 2,
                    "{name}: forged is the FILE class: {}",
                    out.text
                );
            }
            "incomplete" => {
                assert_eq!(
                    out.code, 5,
                    "{name}: incomplete exits its own class (ADR-129) — never OK, never tampered: {}",
                    out.text
                );
                assert!(
                    out.text.contains("INCOMPLETE"),
                    "{name}: the reader names the missing end: {}",
                    out.text
                );
            }
            "refused" => {
                assert_eq!(
                    out.code, 2,
                    "{name}: a refusal exits nonzero, before any walk: {}",
                    out.text
                );
                assert!(
                    !out.text.contains("chain intact"),
                    "{name}: a refused journal has no walk verdict: {}",
                    out.text
                );
            }
            other => panic!("{name}: unknown fixture verdict {other}"),
        }
        assert_cost_projection(&expected, &out.text, &name);
        assert_prologue_projection(&expected, &trace, &name);
        if let Some(items) = expected["items"].as_object() {
            assert_item_projection(&trace, &name, items);
        }
    }
    assert!(
        seen >= 9,
        "the spec runtime/trace corpus has >= 9 fixtures (saw {seen})"
    );
}

fn assert_prologue_projection(expected: &serde_json::Value, trace: &std::path::Path, name: &str) {
    if let Some(prologue) = expected.get("prologue") {
        use std::io::BufRead;
        let file = std::fs::File::open(trace).expect("readable journal");
        let first = std::io::BufReader::new(file)
            .lines()
            .next()
            .expect("journal has a prologue")
            .expect("readable first frame");
        let frame = serde_json::from_str(&first).expect("prologue JSON");
        assert_eq!(
            prologue_difference(prologue, &frame),
            None,
            "{name}: boot facts"
        );
    }
}

fn assert_item_projection(
    trace: &std::path::Path,
    name: &str,
    items: &serde_json::Map<String, serde_json::Value>,
) {
    let outputs = nika_cli::verbs::trace::outputs_json(&trace.to_string_lossy());
    assert_eq!(outputs.code, 0, "{name}: {}", outputs.text);
    let document: serde_json::Value =
        serde_json::from_str(&outputs.text).expect("trace outputs JSON");
    let tasks = document["tasks"].as_array().expect("task projections");
    for (id, rows) in items {
        let task = tasks.iter().find(|task| task["id"] == *id).expect("task");
        assert_eq!(&task["items"], rows, "{name}: {id} item evidence");
    }
}

fn assert_cost_projection(expected: &serde_json::Value, text: &str, name: &str) {
    if let Some(cost) = expected["cost_replay"].as_str() {
        let marker = match cost {
            "replayed" => "COST-REPLAY — the pinned pricing table is this engine's",
            "refused" => "COST-REPLAY — REFUSED",
            "unrecorded" => "COST-REPLAY — unrecorded",
            other => panic!("{name}: unknown cost_replay claim {other}"),
        };
        assert!(
            text.contains(marker),
            "{name}: cost_replay `{cost}` renders `{marker}`: {text}"
        );
    }
}

/// Semantic boot claims are independent of an intact unkeyed chain.
fn prologue_difference(expected: &serde_json::Value, frame: &serde_json::Value) -> Option<String> {
    if frame["kind"] != "workflow_started" {
        return Some("missing initial workflow_started".into());
    }
    let Some(fields) = frame["fields"].as_array() else {
        return Some("malformed prologue fields".into());
    };
    let mut values = std::collections::BTreeMap::new();
    for field in fields {
        let Some(key) = field["key"].as_str() else {
            return Some("malformed prologue field".into());
        };
        if values.insert(key, &field["value"]).is_some() {
            return Some(format!("duplicate prologue field {key}"));
        }
    }
    for (property, present) in [("present", true), ("absent", false)] {
        if let Some(keys) = expected[property].as_array() {
            for key in keys {
                let key = key.as_str().expect("fixture field name");
                if values.contains_key(key) != present {
                    return Some(format!("prologue {key}: expected {property}"));
                }
            }
        }
    }
    if let Some(want) = expected.get("input_origins") {
        let Some(raw) = values.get("inputs").and_then(|value| value.as_str()) else {
            return Some("missing inputs origin map".into());
        };
        let Ok(origins) = unique_origins(raw) else {
            return Some("malformed inputs origin map".into());
        };
        let got = serde_json::to_value(origins).expect("origin map serializes");
        if &got != want {
            return Some(format!("input origins: want {want}, got {got}"));
        }
    }
    None
}

fn unique_origins(
    raw: &str,
) -> Result<std::collections::BTreeMap<String, String>, serde_json::Error> {
    struct OriginMap;
    impl<'de> serde::de::Visitor<'de> for OriginMap {
        type Value = std::collections::BTreeMap<String, String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a string map with unique input names")
        }

        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut origins = std::collections::BTreeMap::new();
            while let Some((name, origin)) = map.next_entry::<String, String>()? {
                if origins.insert(name, origin).is_some() {
                    return Err(serde::de::Error::custom("duplicate input origin"));
                }
            }
            Ok(origins)
        }
    }
    let mut decoder = serde_json::Deserializer::from_str(raw);
    let origins = serde::Deserializer::deserialize_map(&mut decoder, OriginMap)?;
    decoder.end()?;
    Ok(origins)
}

#[test]
fn origin_judge_detects_misattribution_and_malformed_boot_facts() {
    use serde_json::json;
    let expected = json!({"present": ["inputs"], "absent": ["seed"],
        "input_origins": {"supplied": "api-caller", "defaulted": "file"}});
    let boot = |raw: serde_json::Value| {
        json!({"kind": "workflow_started",
        "fields": [{"key": "inputs", "value": raw}]})
    };
    let valid = json!({"supplied": "api-caller", "defaulted": "file"}).to_string();
    assert_eq!(prologue_difference(&expected, &boot(json!(valid))), None);
    for wrong in ["cli-operator", "ci-context", "env", "file"] {
        let raw = json!({"supplied": wrong, "defaulted": "file"}).to_string();
        assert!(prologue_difference(&expected, &boot(json!(raw))).is_some());
    }
    for raw in [
        "null",
        "[]",
        "{}",
        "{",
        "{\"supplied\":true}",
        "{\"supplied\":\"file\",\"supplied\":\"api-caller\",\"defaulted\":\"file\"}",
        "{\"supplied\":\"api-caller\",\"defaulted\":\"file\",\"extra\":\"file\"}",
    ] {
        assert!(prologue_difference(&expected, &boot(json!(raw))).is_some());
    }
    assert!(prologue_difference(&expected, &boot(json!(null))).is_some());
    let mut duplicate = boot(json!(valid));
    duplicate["fields"]
        .as_array_mut()
        .expect("fields")
        .push(json!({"key": "inputs", "value": valid}));
    assert!(prologue_difference(&expected, &duplicate).is_some());
    let mut missing = boot(json!(valid));
    missing["kind"] = json!("task_started");
    assert!(prologue_difference(&expected, &missing).is_some());
    let mut unexpected = boot(json!(valid));
    unexpected["fields"]
        .as_array_mut()
        .expect("fields")
        .push(json!({"key": "seed", "value": 0}));
    assert!(prologue_difference(&expected, &unexpected).is_some());
}
