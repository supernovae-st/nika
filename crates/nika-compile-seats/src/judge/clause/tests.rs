// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The question that asks one part of the request alone: how many options it offers sets the
//! confidence its « missing » needs ([`crate::judge::Doubt::part_decides`]).

use super::{CLAUSE, NO_OPERATION, PART, RESTRICTING, SUPERSEDED, part_question};
use serde_json::json;

/// The part a question asks.
const ASKED: &str = "keep only the open rows";

/// A part offers carried and missing, superseded when a later part follows, no operation when the
/// request has other parts and this one restricts nothing, then NONE: three to five options. It
/// is told the clause, that it is a part asked alone, and when it restricts.
#[test]
fn a_part_offers_what_its_place_and_its_restriction_allow() {
    let base = json!({"request": "Read ./in/rows.json, keep only the open rows",
        "candidate_nika": "nika: rows\n"});
    let offered = |k, count, restricting| -> Vec<String> {
        let question = part_question(k, count, ASKED, restricting, &base, "REFERENCE");
        question
            .options
            .into_iter()
            .map(|option| option.key)
            .collect()
    };
    let open = ["carried", "missing", "superseded", "no_operation", "none"];
    assert_eq!(offered(0, 2, false), open);
    assert_eq!(
        offered(1, 2, false),
        ["carried", "missing", "no_operation", "none"]
    );
    assert_eq!(
        offered(0, 2, true),
        ["carried", "missing", "superseded", "none"]
    );
    assert_eq!(offered(0, 1, false), ["carried", "missing", "none"]);
    let restricting = part_question(0, 2, ASKED, true, &base, "REFERENCE");
    assert_eq!(restricting.id, "verify-part-0");
    assert_eq!(restricting.state["clause"], json!({"text": ASKED}));
    assert_eq!(restricting.state["request"], base["request"]);
    assert_eq!(restricting.options[2].description, SUPERSEDED);
    let told = format!("REFERENCE\n\n{CLAUSE} {PART} {RESTRICTING} ");
    assert!(
        restricting.instructions.starts_with(&told),
        "{}",
        restricting.instructions
    );
    let first = part_question(0, 2, ASKED, false, &base, "REFERENCE");
    assert_eq!(first.id, "verify-part-0");
    assert_eq!(first.options[3].description, NO_OPERATION);
    let told = format!("REFERENCE\n\n{CLAUSE} {PART} ");
    assert!(
        first.instructions.starts_with(&told),
        "{}",
        first.instructions
    );
    assert!(
        !first.instructions.contains(RESTRICTING),
        "{}",
        first.instructions
    );
}
