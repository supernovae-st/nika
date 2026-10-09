// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Source compatibility of `nika_compile_fidelity::behavior` and
//! `nika_compile_fidelity::fidelity::instant_shape` after the behavioural contract and the
//! date-time shape classifier descended to the size-cap member below the candidate laws,
//! `nika_compile_behavior` (2026-10-09 · ADR-149). This file is an external consumer: it
//! compiles against the fidelity paths, and they name the very same items — the same types (a
//! value of one path IS a value of the other) and the same functions (their signatures carry the
//! member's types) — never copies. The one observable difference is a type's name at run time,
//! which now names the member.

use std::any::type_name;
use std::collections::BTreeMap;

use nika_compile_behavior as member;
use nika_compile_fidelity::behavior::{
    Budget, Consumed, Contract, Coverage, Limits, Presence, ReadBack, Report, Run, RunEnd, Usage,
    Verdict, contract_of_request, judge,
};
use nika_compile_fidelity::fidelity::instant_shape;

/// Each function of a fidelity path, typed with the member's types: a fidelity-side copy of these
/// items could not be assigned here.
const CONTRACT: fn(&str, &BTreeMap<String, String>) -> member::behavior::Contract =
    contract_of_request;
const JUDGE: fn(
    &member::behavior::Contract,
    &[member::behavior::Run],
    &mut member::behavior::Budget,
) -> member::behavior::Report = judge;
const SHAPE: fn(&str) -> Option<(String, String)> = instant_shape;

const SOURCE: &str = "./data/input.csv";
const RESULT: &str = "./out/result.json";
const COUNTED: &str = "read ./data/input.csv, count the rows where status is paid, write the count to ./out/result.json";
const ROWS: &str = "id,amount_usd,status\n1,5,paid\n2,20,late\n3,12,paid\n";

fn budget() -> Budget {
    Budget::new(
        Limits::new(16, 16, 1_000_000, 60_000),
        Limits::new(64, 64, 4_000_000, 240_000),
        Usage::default(),
    )
}

#[test]
fn the_fidelity_paths_name_the_members_items() {
    // One contract under two paths: the value the fidelity path states IS the member's.
    let contract: member::behavior::Contract = CONTRACT(COUNTED, &BTreeMap::new());
    assert_eq!(
        contract,
        member::behavior::contract_of_request(COUNTED, &BTreeMap::new())
    );
    let written: Vec<&Presence> = (contract.obligations.iter())
        .filter(|obligation| obligation.target.is_some())
        .map(|obligation| &obligation.presence)
        .collect();
    assert_eq!(written, [&member::behavior::Presence::Required]);
    // A run built through the fidelity path, judged under both paths alike.
    let usage: member::behavior::Usage = Usage::new(1, 1, 256, 256, 20);
    let run: member::behavior::Run = Run::new("observed", RunEnd::Completed, usage)
        .with_consumed(Consumed::new(SOURCE, ROWS, Coverage::Complete))
        .with_read_back(ReadBack::new(RESULT, r#"{"count":2}"#));
    let report: Report = JUDGE(&contract, std::slice::from_ref(&run), &mut budget());
    assert_eq!(report.verdict(), Verdict::Certified);
    assert_eq!(
        report,
        member::behavior::judge(&contract, &[run], &mut budget())
    );
    // The classifier: one function, under the path Law 25 always used and its owner's.
    let shape = |form: &str, offset: &str| Some((form.to_owned(), offset.to_owned()));
    assert_eq!(
        SHAPE("2026-09-01T02:30:00+02:00"),
        shape("9999-99-99T99:99:99", "+02:00")
    );
    assert_eq!(
        member::instant_shape("2026-09-01T02:30:00+02:00"),
        shape("9999-99-99T99:99:99", "+02:00")
    );
    assert_eq!(instant_shape("2026-09-01"), None);
}

/// What changed for a consumer that looks at metadata: a type's run-time name names the member
/// (the same name under both paths, and no longer the fidelity crate's).
#[test]
fn only_the_run_time_type_name_names_the_member() {
    for (fidelity, owner) in [
        (
            type_name::<Contract>(),
            type_name::<member::behavior::Contract>(),
        ),
        (
            type_name::<Report>(),
            type_name::<member::behavior::Report>(),
        ),
        (type_name::<Usage>(), type_name::<member::behavior::Usage>()),
    ] {
        assert_eq!(fidelity, owner);
        assert!(
            owner.starts_with("nika_compile_behavior::behavior::"),
            "{owner}"
        );
    }
}
