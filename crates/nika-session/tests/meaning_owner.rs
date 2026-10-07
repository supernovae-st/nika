// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika_session::meaning` owns the Meaning view again (2026-10-07): the session is its only
//! reader, and the view moved back from `nika_onboard::compile::meaning`, where it sat beside
//! the compiler's ledger from 2026-09-28. This file is an external consumer: it compiles
//! against the session path, reads the view's answers over a ledger and over a compiled
//! outcome, and proves the session owns the items (their run-time names are the session's).
//! Derived `Debug` output carries no path and reads as before the moves.
#![allow(clippy::expect_used)]

use std::any::type_name;

use nika_session::meaning::{self, Clause, Disposition};
use serde_json::{Value, json};

/// Each function of the session path, typed with the session's own types.
const CLAUSES_OF: fn(&Value) -> Vec<Clause> = meaning::clauses_of;
const ASSURANCE: fn(&Clause, Option<&str>) -> &'static str = meaning::assurance;
const CLAUSES: fn(&nika_onboard::compile::CompileOutcome) -> Option<Vec<Clause>> = meaning::clauses;

#[test]
fn the_session_owns_the_meaning_view_and_reads_a_ledger_through_it() {
    let ledger = json!([
        {"kind": "effect", "state": "realized", "evidence": "écris-le", "realized_by": "t"},
        {"kind": "trigger", "state": "realized", "evidence": "daily", "realized_by": "requested_trigger"},
        {"kind": "mystery", "state": "invented"}
    ]);
    let clauses = CLAUSES_OF(&ledger);
    assert_eq!(clauses.len(), 2, "an unknown state is still left out");
    assert_eq!(clauses[0].disposition, Disposition::Represented);
    assert_eq!(ASSURANCE(&clauses[0], None), "carried by the program");
    let candidate = Some("nika: w\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n");
    let view = meaning::render_ledger(&ledger, candidate);
    assert!(view.contains("écris-le"), "{view}");
    assert!(view.contains("daily"), "{view}");
    assert!(
        meaning::delta(&ledger, &json!([])).is_some(),
        "a revision that drops every clause says so"
    );
    assert!(meaning::UNAVAILABLE.starts_with("Meaning · unavailable"));
    // Over a compiled outcome: no ledger is never an invented coverage; a ledger is read.
    let mut out = nika_onboard::compile::compile(&nika_onboard::compile::CompileRequest::create(
        "aggregate-by-key",
    ))
    .expect("compiles");
    out.provenance.decision = Some(json!({ "ledger": ledger }));
    assert_eq!(CLAUSES(&out), Some(clauses));
    assert!(meaning::render(&out).is_some_and(|view| view.contains("daily")));
}

/// The items' run-time names are the session's, no longer the onboarding surface's; derived
/// `Debug` output carries no path and reads as before.
#[test]
fn the_run_time_type_names_name_the_session() {
    for name in [type_name::<Clause>(), type_name::<Disposition>()] {
        assert!(name.starts_with("nika_session::meaning::"), "{name}");
    }
    assert_eq!(format!("{:?}", Disposition::NeedsAnswer), "NeedsAnswer");
    let clause =
        CLAUSES_OF(&json!([{"kind": "gate", "state": "unresolved", "evidence": "ok?"}])).remove(0);
    assert_eq!(
        format!("{clause:?}"),
        "Clause { evidence: \"ok?\", kind: \"gate\", disposition: NeedsAnswer, carrier: None, note: None }"
    );
}
