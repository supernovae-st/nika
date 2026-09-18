// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The settled-state domain — the GATE-v2 pass-sets as a bitset, and the
//! ONE topological fold over it (substrate half of the check lanes).
//!
//! Descended from `nika-check`'s `reach.rs` when a second lane (affirmative
//! consent · `NIKA-SEC-014`) needed the same abstract domain: two private
//! copies of « which states does this edge admit » is how two judges come
//! to disagree about one graph. The dead-gate lane keeps its own pass (it
//! narrates a finding per dead edge); both read THIS domain.
//!
//! The admission law it folds (spec `03-dag.md` §gate algebra v2): a task
//! is admitted iff EVERY incoming scheduling edge's producer settled
//! inside that edge's pass-set, and the edges from ONE producer compose by
//! INTERSECTION — that producer settles one state, and every such edge
//! must admit it. An admission is an AND, never an OR: one admitting edge
//! proves nothing about the task.

use std::collections::BTreeMap;

use crate::edges::{Edge, EdgeKind, SettledState};

/// Bit per terminal status (spec `03-dag.md` §Task states) — the ONE
/// settled-state bitset every abstract pass over the derived graph shares.
pub const S_SUCCESS: u8 = 1;
/// The producer failed unrecovered.
pub const S_FAILURE: u8 = 2;
/// The producer was skipped (`when:` false · empty `for_each` · `on_error: skip`).
pub const S_SKIPPED: u8 = 4;
/// The producer was cancelled (a gate did not admit · workflow cancellation).
pub const S_CANCELLED: u8 = 8;
/// Every terminal status — « unknown »: the set that proves nothing.
pub const S_ALL: u8 = S_SUCCESS | S_FAILURE | S_SKIPPED | S_CANCELLED;

/// What [`fold_settled`] found — opaque on purpose (FCI-014): a judge asks
/// it about ONE task, it never walks the table.
#[derive(Debug, Clone)]
pub struct Settled(Vec<u8>);

impl Settled {
    /// The terminal states `task` can settle in the folded world —
    /// [`S_ALL`] (unknown) for an index the fold never saw.
    #[must_use]
    pub fn of(&self, task: usize) -> u8 {
        self.0.get(task).copied().unwrap_or(S_ALL)
    }
}

/// The bit-mask of settled states an edge admits (GATE-v2 pass-sets ·
/// [`EdgeKind::admits`] projected onto the bitset).
#[must_use]
pub fn edge_mask(kind: EdgeKind) -> u8 {
    let mut mask = 0;
    for (state, bit) in [
        (SettledState::Success, S_SUCCESS),
        (SettledState::Failure, S_FAILURE),
        (SettledState::Skipped, S_SKIPPED),
        (SettledState::Cancelled, S_CANCELLED),
    ] {
        if kind.admits(state) {
            mask |= bit;
        }
    }
    mask
}

/// One topological fold of GATE-v2 admission over the settled-state
/// bitset: `out[n]` is the set of terminal states task `n` can settle in
/// the world the caller describes.
///
/// `admitted(n)` IS that description — the states `n` can settle ONCE THE
/// GATE ADMITS IT (its `when:` · its verb · whatever the caller's world
/// fixes). `fact` is the ONE settle the world holds as given, whatever the
/// task's own producers did (a gate that was answered DID run) — its edges
/// in are not consulted. The fold adds what admission itself decides ·
///
/// - some producer can settle NO state its edges admit → `n` is cancelled
///   in every such world: exactly [`S_CANCELLED`];
/// - every producer can ONLY settle states its edges admit → `n` is
///   admitted for certain: exactly `admitted(n)`;
/// - otherwise both can happen: `admitted(n) | S_CANCELLED`.
///
/// Sound in the two directions a judge may need, and they are NOT the same
/// reading. Feed it OVER-approximate sets and `out[n] == S_CANCELLED`
/// proves `n` never runs (the independent product of the producer sets ⊇
/// the true joint set) — while a wider `out[n]` proves NOTHING: a state in
/// a may-set is uncertainty, never a witness that it happens. Feed it the
/// EXACT sets of one world and a singleton `out[n]` is what `n` settles
/// there. A task the order does not cover (a cleanup unit never enters a
/// wave · spec 03 §unwind) stays [`S_ALL`] — unknown in both readings.
#[must_use]
pub fn fold_settled(
    tasks: usize,
    edges: &[Edge],
    topo_waves: &[Vec<usize>],
    fact: Option<(usize, u8)>,
    admitted: impl Fn(usize) -> u8,
) -> Settled {
    let mut incoming: Vec<BTreeMap<usize, u8>> = vec![BTreeMap::new(); tasks];
    for e in edges.iter().filter(|e| e.kind.is_scheduling()) {
        if let Some(producers) = incoming.get_mut(e.to) {
            *producers.entry(e.from).or_insert(S_ALL) &= edge_mask(e.kind);
        }
    }
    let mut out = vec![S_ALL; tasks];
    for &n in topo_waves.iter().flatten() {
        let Some(producers) = incoming.get(n) else {
            continue;
        };
        let (mut rejected, mut certain) = (false, true);
        for (&from, &mask) in producers {
            let state = out.get(from).copied().unwrap_or(S_ALL);
            rejected |= state & mask == 0;
            certain &= state & !mask == 0;
        }
        let settled = if let Some((_, given)) = fact.filter(|&(task, _)| task == n) {
            given
        } else if rejected {
            S_CANCELLED
        } else if certain {
            admitted(n)
        } else {
            admitted(n) | S_CANCELLED
        };
        if let Some(slot) = out.get_mut(n) {
            *slot = settled;
        }
    }
    Settled(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;
    use nika_schema::types::AfterPredicate;

    /// Fold `yaml` with one `admitted` reading: the task named `stage`
    /// settles `stage` once admitted (a `when:` the world decides), every
    /// other task `ran` — and a task named `gate` is the world's FACT
    /// (`success`).
    fn fold(yaml: &str, stage: u8, ran: u8) -> BTreeMap<String, u8> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let analyzed = crate::analyze(&wf).expect("fixture analyzes");
        let id = |n: usize| wf.tasks[n].value.id.value.clone();
        let fact = (0..wf.tasks.len()).find(|&n| id(n) == "gate");
        let out = fold_settled(
            wf.tasks.len(),
            &analyzed.edges,
            &analyzed.topo_waves,
            fact.map(|n| (n, S_SUCCESS)),
            |n| if id(n) == "stage" { stage } else { ran },
        );
        (0..wf.tasks.len()).map(|n| (id(n), out.of(n))).collect()
    }

    const STEP: &str = "    exec: { command: [\"true\"] }\n";

    #[test]
    fn the_mask_is_the_pass_set_of_every_edge_kind() {
        assert_eq!(edge_mask(EdgeKind::Value), S_SUCCESS | S_SKIPPED);
        assert_eq!(
            edge_mask(EdgeKind::FailureObservation),
            S_FAILURE | S_SKIPPED
        );
        assert_eq!(edge_mask(EdgeKind::TerminalObservation), S_ALL);
        assert_eq!(edge_mask(EdgeKind::FanIn), S_ALL);
        assert_eq!(
            edge_mask(EdgeKind::Control(AfterPredicate::Success)),
            S_SUCCESS
        );
        assert_eq!(
            edge_mask(EdgeKind::Control(AfterPredicate::Failure)),
            S_FAILURE
        );
        assert_eq!(
            edge_mask(EdgeKind::Control(AfterPredicate::Skipped)),
            S_SKIPPED
        );
        assert_eq!(
            edge_mask(EdgeKind::Control(AfterPredicate::Terminal)),
            S_ALL
        );
    }

    /// Two predicates on ONE producer intersect: `{success, skipped}` ∩
    /// `{success}` leaves `{success}`, which a never-success producer can
    /// not settle — cancelled in every world, whatever the value edge alone
    /// would have admitted.
    #[test]
    fn two_edges_from_one_producer_compose_by_intersection() {
        let yaml = format!(
            "nika: t\ntasks:\n  stage:\n{STEP}  reads:\n    with: {{ v: \"${{{{ tasks.stage.output }}}}\" }}\n{STEP}  both:\n    with: {{ v: \"${{{{ tasks.stage.output }}}}\" }}\n    after: {{ stage: success }}\n{STEP}"
        );
        let out = fold(&yaml, S_SKIPPED, S_ALL);
        assert_eq!(out["both"], S_CANCELLED, "the AND rejects: {out:?}");
        assert_eq!(out["reads"], S_ALL, "a value edge admits the skip: {out:?}");
    }

    /// « Never `success` » is not « `skipped` »: a stage whose gate may
    /// ERROR settles `failure`, which a value edge does not admit. Its
    /// reader is then neither proven cancelled nor certainly admitted —
    /// only the stage that certainly skips hands the reader a sure run.
    #[test]
    fn a_stage_that_may_fail_does_not_certainly_feed_its_reader() {
        let yaml = format!(
            "nika: t\ntasks:\n  stage:\n{STEP}  reads:\n    with: {{ v: \"${{{{ tasks.stage.output }}}}\" }}\n{STEP}"
        );
        let unsure = fold(&yaml, S_SKIPPED | S_FAILURE, S_SUCCESS);
        assert_eq!(unsure["reads"], S_SUCCESS | S_CANCELLED, "{unsure:?}");
        let sure = fold(&yaml, S_SKIPPED, S_SUCCESS);
        assert_eq!(sure["reads"], S_SUCCESS, "{sure:?}");
    }

    /// An unknown producer proves nothing: `idle` may fail, so the task
    /// that waits for its failure is NOT proven cancelled — and in the one
    /// world where every task succeeds it IS cancelled. The two readings
    /// disagree by design; neither is the other's proof.
    #[test]
    fn a_may_set_never_proves_and_an_exact_world_never_generalizes() {
        let yaml = format!(
            "nika: t\ntasks:\n  idle:\n{STEP}  on_failure:\n    after: {{ idle: failure }}\n{STEP}  on_success:\n    after: {{ idle: success }}\n{STEP}"
        );
        let may = fold(&yaml, S_SKIPPED, S_ALL);
        assert_eq!(may["on_failure"], S_ALL, "not provably cancelled: {may:?}");
        let exact = fold(&yaml, S_SKIPPED, S_SUCCESS);
        assert_eq!(exact["on_failure"], S_CANCELLED, "{exact:?}");
        assert_eq!(exact["on_success"], S_SUCCESS, "{exact:?}");
    }

    /// A cancellation cascades over a value edge (its pass-set holds no
    /// `cancelled`), and stops at an edge that observes every outcome.
    #[test]
    fn a_cancellation_cascades_until_an_edge_admits_it() {
        let yaml = format!(
            "nika: t\ntasks:\n  stage:\n{STEP}  mid:\n    after: {{ stage: success }}\n{STEP}  reads_mid:\n    with: {{ v: \"${{{{ tasks.mid.output }}}}\" }}\n{STEP}  observes_mid:\n    after: {{ mid: terminal }}\n{STEP}"
        );
        let out = fold(&yaml, S_SKIPPED, S_ALL);
        assert_eq!(out["mid"], S_CANCELLED, "{out:?}");
        assert_eq!(out["reads_mid"], S_CANCELLED, "{out:?}");
        assert_eq!(out["observes_mid"], S_ALL, "{out:?}");
    }

    /// A FACT is held whatever its own producers could settle: the gate
    /// below waits for a success nobody can promise, yet it DID run — so a
    /// reader of its value is certainly admitted. Without the fact the
    /// same reader inherits the doubt.
    #[test]
    fn a_fact_is_not_doubted_by_its_own_producers() {
        let reached = S_SUCCESS | S_FAILURE | S_SKIPPED;
        let yaml = |gate: &str| {
            format!(
                "nika: t\ntasks:\n  build:\n{STEP}  {gate}:\n    after: {{ build: success }}\n{STEP}  reads:\n    with: {{ v: \"${{{{ tasks.{gate}.output }}}}\" }}\n{STEP}"
            )
        };
        let given = fold(&yaml("gate"), S_SKIPPED, reached);
        assert_eq!(given["gate"], S_SUCCESS, "{given:?}");
        assert_eq!(given["reads"], reached, "certainly admitted: {given:?}");
        let doubted = fold(&yaml("other"), S_SKIPPED, reached);
        assert_eq!(doubted["reads"], reached | S_CANCELLED, "{doubted:?}");
    }

    /// A cleanup unit never enters a wave — the fold leaves it unknown
    /// rather than guess (spec 03 §unwind: it fires off its producer's
    /// settle, not off the gate).
    #[test]
    fn a_task_the_order_does_not_cover_stays_unknown() {
        let yaml = format!(
            "nika: t\ntasks:\n  stage:\n{STEP}  cleanup:\n    after: {{ stage: unwind }}\n{STEP}"
        );
        let out = fold(&yaml, S_SKIPPED, S_SUCCESS);
        assert_eq!(out["cleanup"], S_ALL, "{out:?}");
    }
}
