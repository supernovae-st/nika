// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source compatibility of `nika_session::meaning` after the Meaning projection moved to its
//! owner, `nika_onboard::compile::meaning` (2026-09-28). This file is an external consumer:
//! it compiles against the session path, and the session path names the very same items —
//! the same types (a value of one path IS a value of the other), the same functions (their
//! signatures carry the owner's types) and the same constant — never copies. The one
//! observable difference is a type's name at run time, which now names its owner; derived
//! `Debug` output does not carry the path and is unchanged.
#![allow(clippy::expect_used)]

use std::any::type_name;

use nika_onboard::compile::meaning as owner;
use nika_session::meaning::{self, Clause, Disposition};
use serde_json::{Value, json};

/// Each function of the session path, typed with the owner's types: a session-side copy of
/// these items could not be assigned here.
const CLAUSES_OF: fn(&Value) -> Vec<owner::Clause> = meaning::clauses_of;
const ASSURANCE: fn(&owner::Clause, Option<&str>) -> &'static str = meaning::assurance;
const CLAUSES: fn(&nika_onboard::compile::CompileOutcome) -> Option<Vec<owner::Clause>> =
    meaning::clauses;

#[test]
fn the_session_path_names_the_owners_items() {
    // One type under two paths: values move between them without conversion.
    let gap: owner::Disposition = Disposition::Gap;
    let back: Disposition = gap;
    assert_eq!(back, owner::Disposition::Gap);
    let ledger = json!([
        {"kind": "effect", "state": "realized", "evidence": "écris-le", "realized_by": "t"},
        {"kind": "trigger", "state": "realized", "evidence": "daily", "realized_by": "requested_trigger"},
        {"kind": "mystery", "state": "invented"}
    ]);
    let clauses: Vec<Clause> = CLAUSES_OF(&ledger);
    let owned: Vec<owner::Clause> = clauses.clone();
    assert_eq!(clauses, owned);
    assert_eq!(clauses.len(), 2, "an unknown state is still left out");
    assert_eq!(ASSURANCE(&clauses[0], None), "carried by the program");
    // The functions without the owner's types in their signatures answer as the owner's.
    let candidate = Some("nika: w\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n");
    assert_eq!(
        meaning::render_ledger(&ledger, candidate),
        owner::render_ledger(&ledger, candidate)
    );
    assert_eq!(
        meaning::delta(&ledger, &json!([])),
        owner::delta(&ledger, &json!([]))
    );
    assert_eq!(meaning::UNAVAILABLE, owner::UNAVAILABLE);
    let mut out = nika_onboard::compile::compile(&nika_onboard::compile::CompileRequest::create(
        "aggregate-by-key",
    ))
    .expect("compiles");
    out.provenance.decision = Some(json!({ "ledger": ledger }));
    assert_eq!(CLAUSES(&out), owner::clauses(&out));
    assert_eq!(meaning::render(&out), owner::render(&out));
}

/// What changed for a consumer that looks at metadata: a type's run-time name names the
/// owner (the same name under both paths, and no longer the session's); derived `Debug`
/// output carries no path and reads as before.
#[test]
fn only_the_run_time_type_name_names_the_new_owner() {
    assert_eq!(type_name::<Clause>(), type_name::<owner::Clause>());
    assert_eq!(
        type_name::<Disposition>(),
        type_name::<owner::Disposition>()
    );
    for name in [type_name::<Clause>(), type_name::<Disposition>()] {
        assert!(!name.contains("nika_session"), "{name}");
    }
    assert_eq!(format!("{:?}", Disposition::NeedsAnswer), "NeedsAnswer");
    let clause =
        CLAUSES_OF(&json!([{"kind": "gate", "state": "unresolved", "evidence": "ok?"}])).remove(0);
    assert_eq!(
        format!("{clause:?}"),
        "Clause { evidence: \"ok?\", kind: \"gate\", disposition: NeedsAnswer, carrier: None, note: None }"
    );
}
