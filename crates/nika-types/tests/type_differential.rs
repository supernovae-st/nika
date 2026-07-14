// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// Test-harness plumbing: NIKA_SPEC_DIR is the same path override the
// schema conformance tiers read.
#![allow(clippy::disallowed_methods)]

//! The Python↔Rust type-core differential (spec 09 · the
//! second-evaluator law).
//!
//! Reads the COMMITTED corpus (`conformance/type-corpus/corpus.jsonl` ·
//! judged by `conformance/type_core.py`, written BY HAND against the
//! spec prose) and re-judges every row through THIS crate's
//! hand-written implementation: 400 lowered types must match
//! byte-canonically, 4000 pairs must agree on all THREE relations.
//! The implementations share no code — a common bug would have to be
//! born twice. HARD-FAILS when the spec dir is missing (a
//! silently-skipped differential is the guard-blind class).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nika_types::types::{NikaType, assignable, consistent, lower, parse_type, subtype};
use serde_json::Value;

fn spec_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NIKA_SPEC_DIR") {
        return PathBuf::from(dir);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .parent()
        .expect("engine parent")
        .join("spec")
}

/// Canonical JSON — objects with sorted keys, no spaces (the corpus's
/// own `json.dumps(sort_keys=True, separators=(",", ":"))`).
fn canonical(v: &Value) -> String {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).expect("key"),
                        canonical(&map[k])
                    )
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(canonical).collect();
            format!("[{}]", inner.join(","))
        }
        other => serde_json::to_string(other).expect("scalar"),
    }
}

/// Parse a corpus expression — the `{"$unknown": true}` marker spells
/// the internal gradual type (no authorable surface · both evaluators
/// read the marker identically).
fn parse_corpus_expr(expr: &Value, where_: &str) -> NikaType {
    if expr.get("$unknown").and_then(Value::as_bool) == Some(true) {
        return NikaType::Unknown;
    }
    parse_type(expr, &std::collections::BTreeSet::new(), where_)
        .unwrap_or_else(|e| panic!("{where_}: corpus expr must parse — {}", e.detail))
}

#[test]
fn python_and_rust_agree_over_the_committed_corpus() {
    let corpus = spec_dir().join("conformance/type-corpus/corpus.jsonl");
    assert!(
        corpus.is_file(),
        "type corpus missing: {} — set NIKA_SPEC_DIR (a skipped differential is guard-blind)",
        corpus.display()
    );
    let text = std::fs::read_to_string(&corpus).expect("read corpus");

    let named: BTreeMap<String, NikaType> = BTreeMap::new();
    let mut types: BTreeMap<u64, NikaType> = BTreeMap::new();
    let mut n_types = 0usize;
    let mut n_pairs = 0usize;
    let mut divergences: Vec<String> = Vec::new();

    for (ln, line) in text.lines().enumerate() {
        let row: Value = serde_json::from_str(line).expect("corpus row parses");
        match row["kind"].as_str() {
            Some("type") => {
                let i = row["i"].as_u64().expect("i");
                let t = parse_corpus_expr(&row["expr"], &format!("corpus[{i}]"));
                let ours = canonical(&lower(&t, &named));
                let theirs = row["lowered"].as_str().expect("lowered");
                if ours != theirs {
                    divergences.push(format!(
                        "type {i} (line {ln}): lowering differs\n  py: {theirs}\n  rs: {ours}"
                    ));
                }
                types.insert(i, t);
                n_types += 1;
            }
            Some("pair") => {
                let a = &types[&row["a"].as_u64().expect("a")];
                let b = &types[&row["b"].as_u64().expect("b")];
                let checks = [
                    ("subtype", subtype(a, b, &named)),
                    ("consistent", consistent(a, b, &named)),
                    ("assignable", assignable(a, b, &named)),
                ];
                for (rel, ours) in checks {
                    let theirs = row[rel].as_bool().expect(rel);
                    if ours != theirs {
                        divergences.push(format!(
                            "pair a={} b={} (line {ln}): {rel} py={theirs} rs={ours}",
                            row["a"], row["b"]
                        ));
                    }
                }
                n_pairs += 1;
            }
            other => panic!("unknown corpus row kind: {other:?}"),
        }
        if divergences.len() >= 10 {
            break;
        }
    }

    assert!(
        divergences.is_empty(),
        "{} divergence(s) — the first 10:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    // the corpus floor (gen-type-corpus.py commits 400 types · 4000 pairs)
    assert!(
        n_types >= 400,
        "only {n_types} types walked — corpus drift?"
    );
    assert!(
        n_pairs >= 4000,
        "only {n_pairs} pairs walked — corpus drift?"
    );
}
