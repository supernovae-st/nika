// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Selection by behaviour: several candidates, however differently written, judged on what
//! their rehearsals showed against one contract the request alone states. A candidate is its
//! rehearsals' runs and the identity of the bytes the host ran, never those bytes: no task, no
//! program and no name a candidate chose is read. The candidates are judged in the order the
//! door gives them, each as one round of the same turn's [`Budget`], and the first one the
//! contract certifies is selected. A defect, an open case, a fixture that did not run or an
//! invalid harness never selects, and neither does the absence of a defect; when the turn's
//! budget stops the judging first, the choice says so.
//!
//! Selection runs nothing and verifies no identity. It judges runs a host already made, and the
//! turn it reports counts only the candidates it judged: a door rehearses and judges one
//! candidate at a time, carries the turn forward, and rehearses no further candidate once one is
//! selected or the turn is spent. What it selects is an index into the slice it was given: the
//! door keeps each candidate bound to the bytes it rehearsed and to that index.

use super::{
    Admission, Axis, Budget, Contract, Limits, Report, Run, Usage, Verdict, judge, same_path,
};

/// One candidate the host rehearsed. Nothing here binds its runs to its bytes: the door that
/// builds it keeps that binding, from the bytes to the host's verified receipt to the runs.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Candidate {
    /// The identity of the bytes the host rehearsed (their sha256), never the bytes: carried for
    /// the record, never read or verified by the selection.
    pub id: String,
    /// One run per fixture: the observed world, then the discriminating ones.
    pub runs: Vec<Run>,
}

impl Candidate {
    /// The candidate whose rehearsed bytes are `id`, with its runs.
    #[must_use]
    pub fn new(id: impl Into<String>, runs: Vec<Run>) -> Self {
        Self {
            id: id.into(),
            runs,
        }
    }
}

/// What the selection decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Choice {
    /// The candidate at this index of the slice judged certified the contract: the first that
    /// did, in order. The index is a position, not an identity.
    Selected(usize),
    /// Every candidate was judged, and each showed a defect.
    RejectAll,
    /// No candidate certified the contract and not every one showed a defect: a case stays
    /// open, a fixture did not run, a harness is invalid, or there was no candidate.
    Unproven,
    /// The turn's budget stopped the judging on this axis, with candidates left unjudged.
    Spent(Axis),
}

/// The selection's choice and its evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ruling {
    pub choice: Choice,
    /// One report per candidate judged, in their order: the judging stops at the first one
    /// certified, or where the turn's budget stops it.
    pub reports: Vec<Report>,
    /// What the turn spent: the usage it started from and that of every candidate judged, never
    /// that of a candidate left unjudged, whatever the host already ran for it.
    pub turn: Usage,
}

/// Judge `candidates` in order against `contract`, each as one round of `round` limits in a
/// turn of `turn` limits that already spent `before`, and select the first one certified.
///
/// Nothing is run and no identity is verified: `candidates` are runs a host already made, each
/// bound by the caller to the bytes it rehearsed and to its index here. The ruling's turn counts
/// the candidates judged and no other, so a door rehearses and judges one candidate at a time:
/// it rehearses only while the turn is open, calls this with that candidate alone and the last
/// [`Ruling::turn`] as `before`, and stops before the next rehearsal once one is selected or the
/// turn is spent.
#[must_use]
pub fn select(
    contract: &Contract,
    candidates: &[Candidate],
    round: Limits,
    turn: Limits,
    before: Usage,
) -> Ruling {
    let mut spent = before;
    let mut ruled = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let mut budget = Budget::new(round, turn, spent);
        if let Admission::TurnSpent(axis) = budget.admission() {
            return Ruling {
                choice: Choice::Spent(axis),
                reports: ruled,
                turn: spent,
            };
        }
        let report = judge(contract, &candidate.runs, &mut budget);
        spent = budget.turn();
        let certified = report.certified();
        ruled.push(report);
        if certified {
            return Ruling {
                choice: Choice::Selected(index),
                reports: ruled,
                turn: spent,
            };
        }
    }
    let rejected = !ruled.is_empty()
        && ruled
            .iter()
            .all(|report| report.verdict() == Verdict::Defective);
    Ruling {
        choice: if rejected {
            Choice::RejectAll
        } else {
            Choice::Unproven
        },
        reports: ruled,
        turn: spent,
    }
}

/// The paths a host reads back after every attempt for `contract`, whatever a candidate
/// declares: every obligation's target, once (`./` or not), in order.
pub fn targets(contract: &Contract) -> impl Iterator<Item = String> {
    let mut found: Vec<String> = Vec::new();
    for target in contract
        .obligations
        .iter()
        .filter_map(|obligation| obligation.target.as_ref())
    {
        if !found.iter().any(|known| same_path(known, &target.path)) {
            found.push(target.path.clone());
        }
    }
    found.into_iter()
}
