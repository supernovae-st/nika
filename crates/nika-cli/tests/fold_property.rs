// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Gate 6 · the fold property battery (spec §3 · §11).
//!
//! "The animation IS the data": a frame renders a PURE fold of the event
//! stream, so the TERMINAL data must not depend on the order independent task
//! blocks arrive — the runtime interleaves events across a parallel wave, yet
//! the verdict the operator reads must be identical to the sequential telling.
//! This battery proves the monoid-like invariants of `RunView::apply`:
//!
//! - cost conservation — `cost_usd` is the exact sum of every block's cost;
//! - one-row-per-task — the upsert is keyed by id (N distinct tasks → N rows);
//! - permutation-invariance — reversing or interleaving the blocks does not
//!   change the terminal cost, done-count, or the rendered set of tasks.

use std::collections::{BTreeMap, BTreeSet};

use nika_cli::RunView;
use nika_cli::demo::bare_event;
use nika_event::{Event, EventKind};
use nika_types::resource::{KeyValue, Value};
use proptest::prelude::*;

fn started(task: &str, ms: u64) -> Event {
    bare_event(EventKind::TaskStarted, ms)
        .with_field(KeyValue::new("task", Value::String(task.to_owned())))
}

fn completed(task: &str, cost: f64, ms: u64) -> Event {
    bare_event(EventKind::TaskCompleted, ms)
        .with_field(KeyValue::new("task", Value::String(task.to_owned())))
        .with_field(KeyValue::new("cost_usd", Value::Float(cost)))
}

/// Fold a list of (task, cost) blocks — each task's `[Started, Completed]`
/// applied in order — into the terminal view.
fn fold_blocks(blocks: &[(String, f64)]) -> RunView {
    let mut view = RunView::new();
    let mut ms = 0u64;
    for (id, cost) in blocks {
        view.apply(&started(id, ms));
        ms += 1;
        view.apply(&completed(id, *cost, ms));
        ms += 1;
    }
    view
}

/// Distinct (task-id, cost) blocks from a raw generated list — the upsert is
/// keyed by id, so collisions keep the first cost (mirrors a re-emitted id).
fn distinct_blocks(raw: Vec<(u16, f64)>) -> Vec<(String, f64)> {
    let mut seen: BTreeMap<u16, f64> = BTreeMap::new();
    for (id, cost) in raw {
        seen.entry(id).or_insert(cost);
    }
    seen.into_iter()
        .map(|(id, cost)| (format!("t{id}"), cost))
        .collect()
}

fn ids(view: &RunView) -> BTreeSet<&str> {
    view.rows().iter().map(|r| r.id.as_str()).collect()
}

proptest! {
    /// The terminal fold is invariant under REVERSAL of the block order: the
    /// same multiset of completed tasks folds to the same cost, done-count,
    /// and set of rows, forward or backward.
    #[test]
    fn fold_is_order_independent_over_blocks(
        raw in prop::collection::vec((any::<u16>(), 0.0f64..1000.0), 1..16)
    ) {
        let forward = distinct_blocks(raw);
        let total: f64 = forward.iter().map(|(_, c)| c).sum();
        let n = forward.len();
        let mut reversed = forward.clone();
        reversed.reverse();

        let f = fold_blocks(&forward);
        let b = fold_blocks(&reversed);

        prop_assert!((f.cost_usd - total).abs() < 1e-9, "cost = Σ blocks");
        prop_assert!((f.cost_usd - b.cost_usd).abs() < 1e-9, "cost is order-free");
        prop_assert_eq!(f.rows().len(), n, "one row per distinct task");
        prop_assert_eq!(b.rows().len(), n);
        prop_assert_eq!(f.done_count(), n, "every block reached a terminal state");
        prop_assert_eq!(b.done_count(), n);
        prop_assert_eq!(ids(&f), ids(&b), "the rendered task set is order-free");
    }

    /// The SEQUENTIAL telling (`[Sa Da Sb Db …]`) and the INTERLEAVED telling
    /// (`[Sa Sb … Da Db …]` — the parallel-wave shape the runtime emits) fold
    /// to the SAME terminal aggregate: the fold does not care that a wave's
    /// tasks all start before any of them finish.
    #[test]
    fn interleaved_wave_folds_like_the_sequential_telling(
        raw in prop::collection::vec((any::<u16>(), 0.0f64..1000.0), 1..16)
    ) {
        let tasks = distinct_blocks(raw);
        let total: f64 = tasks.iter().map(|(_, c)| c).sum();
        let n = tasks.len();

        let seq = fold_blocks(&tasks);

        // The interleaved wave: all Starts, THEN all Completions.
        let mut inter = RunView::new();
        let mut ms = 0u64;
        for (id, _) in &tasks {
            inter.apply(&started(id, ms));
            ms += 1;
        }
        for (id, cost) in &tasks {
            inter.apply(&completed(id, *cost, ms));
            ms += 1;
        }

        prop_assert!((seq.cost_usd - total).abs() < 1e-9);
        prop_assert!((inter.cost_usd - total).abs() < 1e-9, "interleaving conserves cost");
        prop_assert_eq!(inter.rows().len(), n);
        prop_assert_eq!(inter.done_count(), n, "every wave task reached Ok");
        prop_assert_eq!(ids(&seq), ids(&inter), "same task set either telling");
    }
}
