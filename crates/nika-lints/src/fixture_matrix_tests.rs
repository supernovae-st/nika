// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Engine-owned regression matrix for every stable linter rule id.
//! These fixtures prove this implementation's diagnostics; they do not
//! claim cross-engine spec conformance (that corpus remains in nika-spec).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use nika_schema::{FileId, ParseMode, parse};

use crate::{arg_injection, native_first, one_obvious_way};

const RULES: [&str; 16] = [
    "one-obvious-way/001",
    "one-obvious-way/002",
    "one-obvious-way/003",
    "one-obvious-way/004",
    "one-obvious-way/005",
    "one-obvious-way/006",
    "one-obvious-way/007",
    "one-obvious-way/008",
    "one-obvious-way/009",
    "one-obvious-way/010",
    "native-first/001",
    "native-first/002",
    "native-first/003",
    "native-first/004",
    "native-first/005",
    "arg-injection/001",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture(rule: &str, polarity: &str) -> PathBuf {
    let lane = if rule == "one-obvious-way/001" {
        "compiler"
    } else {
        "lint"
    };
    root()
        .join(lane)
        .join(rule)
        .join(format!("{polarity}.nika.yaml"))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn discovered_rules() -> BTreeSet<String> {
    let mut rules = BTreeSet::new();
    for lane in ["compiler", "lint"] {
        let lane_path = root().join(lane);
        for family in
            std::fs::read_dir(&lane_path).unwrap_or_else(|e| panic!("{}: {e}", lane_path.display()))
        {
            let family = family.expect("fixture family entry");
            if !family.path().is_dir() {
                continue;
            }
            for id in std::fs::read_dir(family.path()).expect("rule directories") {
                let id = id.expect("rule entry");
                if id.path().is_dir() {
                    rules.insert(format!(
                        "{}/{}",
                        family.file_name().to_string_lossy(),
                        id.file_name().to_string_lossy()
                    ));
                }
            }
        }
    }
    rules
}

fn rules_for(yaml: &str, family: &str) -> Vec<&'static str> {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("lint fixture parses");
    match family {
        "one-obvious-way" => one_obvious_way(&wf),
        "native-first" => native_first(&wf),
        "arg-injection" => arg_injection(&wf),
        other => panic!("unknown lint family {other}"),
    }
    .into_iter()
    .map(|lint| lint.rule)
    .collect()
}

#[test]
fn every_stable_rule_has_a_positive_and_a_neighbor_negative() {
    let expected: BTreeSet<String> = RULES.iter().map(|rule| (*rule).to_owned()).collect();
    assert_eq!(
        discovered_rules(),
        expected,
        "filesystem discovery must yield the exact 16-rule matrix"
    );
    let mut seen = Vec::new();
    for rule in RULES {
        let positive_path = fixture(rule, "positive");
        let negative_path = fixture(rule, "negative");
        assert!(
            positive_path.is_file(),
            "missing {}",
            positive_path.display()
        );
        assert!(
            negative_path.is_file(),
            "missing {}",
            negative_path.display()
        );
        let positive = read(&positive_path);
        let negative = read(&negative_path);

        if rule == "one-obvious-way/001" {
            let dirty = parse(&positive, FileId::new(0), ParseMode::Strict)
                .expect("raw syntax parses before the compiler judgment");
            let report = nika_check::check(&dirty);
            assert!(
                report
                    .conformance
                    .iter()
                    .any(|finding| finding.code == "NIKA-VAR-021"),
                "retired /001 form must be a coded compiler refusal: {:?}",
                report.conformance
            );
            assert!(
                one_obvious_way(&dirty)
                    .into_iter()
                    .all(|lint| lint.rule != rule),
                "retired /001 must never resurrect beside the compiler refusal"
            );
            let wf = parse(&negative, FileId::new(0), ParseMode::Strict)
                .expect("the repaired boundary form parses");
            assert!(
                nika_check::check(&wf)
                    .conformance
                    .iter()
                    .all(|finding| finding.code != "NIKA-VAR-021"),
                "the boundary repair clears NIKA-VAR-021"
            );
            assert!(
                one_obvious_way(&wf)
                    .into_iter()
                    .all(|lint| lint.rule != rule),
                "retired /001 must never resurrect as a warning"
            );
        } else {
            let family = rule.split('/').next().expect("rule family");
            let fired = rules_for(&positive, family);
            let silent = rules_for(&negative, family);
            assert!(
                fired.contains(&rule),
                "{rule} positive did not fire: {fired:?}"
            );
            assert!(
                !silent.contains(&rule),
                "{rule} neighbor negative false-fired: {silent:?}"
            );
        }
        seen.push(rule);
    }
    assert_eq!(seen, RULES, "the matrix is the exact stable 16-rule set");
}
