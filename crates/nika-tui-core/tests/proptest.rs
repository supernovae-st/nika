// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

//! Gate 6 — the derivation invariants under arbitrary shapes. The
//! properties are the law's promises, stated so the machine can try to
//! break them: waves partition, idleness never goes negative, the run's
//! duration covers every step, a crowned bottleneck always has waiters,
//! and a cycle never hangs the wave walker.

use nika_tui_core::derive;
use nika_tui_core::model::{Run, Step, Task, Verb, Workflow};
use proptest::prelude::*;

/// Random tasks: ids `t0..tn`, needs pointing ANYWHERE (cycles included —
/// the guard must hold, the check refuses cycles before we ever render).
fn arb_workflow() -> impl Strategy<Value = Workflow> {
    (1usize..12).prop_flat_map(|n| {
        prop::collection::vec(prop::collection::vec(0usize..n, 0..3), n..=n).prop_map(
            move |needs| {
                Workflow::new(
                    "prop.nika.yaml".to_owned(),
                    "test".to_owned(),
                    String::new(),
                    Vec::new(),
                    String::new(),
                    needs
                        .into_iter()
                        .enumerate()
                        .map(|(i, ns)| {
                            Task::new(
                                format!("t{i}"),
                                match i % 4 {
                                    0 => Verb::Infer,
                                    1 => Verb::Exec,
                                    2 => Verb::Invoke,
                                    _ => Verb::Agent,
                                },
                                "·".to_owned(),
                                ns.into_iter().map(|j| format!("t{j}")).collect(),
                            )
                        })
                        .collect(),
                )
            },
        )
    })
}

/// A run whose steps shadow a subset of the workflow's tasks.
///
/// ⚠️ `never_born` and `skipped` used to be hardcoded `None` here, so the
/// whole suite was STRUCTURALLY blind to the flags: `total_time_covers_
/// every_step`'s never-born guard could never fire, and a green run proved
/// nothing about that boundary. A Gate-11 lens found a P0 living exactly
/// there — `wave_end` did not apply the law — and the reason no property
/// caught it was this generator, not the properties.
///
/// They are GENERATED now, and generated INDEPENDENTLY of the timings: a
/// step marked never-born while carrying real `start`/`dur` is precisely
/// the shape the journal fold never produces and the wasm door accepts.
fn arb_run(ids: Vec<String>) -> impl Strategy<Value = Run> {
    prop::collection::vec(
        (
            0.0f64..100.0,
            0.0f64..50.0,
            prop::option::of(0.0f64..5.0),
            prop::option::of(prop::bool::ANY),
            prop::option::of(prop::bool::ANY),
        ),
        0..=ids.len(),
    )
    .prop_map(move |rows| {
        Run::new(
            "prop".to_owned(),
            "recorded".to_owned(),
            String::new(),
            rows.into_iter()
                .enumerate()
                .map(|(i, (start, dur, cost, never_born, skipped))| {
                    let id = ids.get(i).cloned().unwrap_or_else(|| "ghost".to_owned());
                    let mut step = Step::new(id, start, dur);
                    step.cost = cost;
                    step.never_born = never_born;
                    step.skipped = skipped;
                    step
                })
                .collect(),
        )
    })
}

/// The (workflow, run) pair, owned on both sides (the strategy borrows
/// nothing — a borrowed strategy expires with its lender).
fn arb_pair() -> impl Strategy<Value = (Workflow, Run)> {
    arb_workflow().prop_flat_map(|wf| {
        let ids: Vec<String> = wf.tasks.iter().map(|t| t.id.clone()).collect();
        (Just(wf), arb_run(ids))
    })
}

proptest! {
    /// waves() partitions: every task appears exactly once, across waves.
    #[test]
    fn waves_partition_every_task(wf in arb_workflow()) {
        let ws = derive::waves(&wf);
        let mut seen: Vec<&str> = ws.groups().iter().flat_map(|g| g.iter().map(|t| t.id.as_str())).collect();
        seen.sort_unstable();
        let mut want: Vec<&str> = wf.tasks.iter().map(|t| t.id.as_str()).collect();
        want.sort_unstable();
        prop_assert_eq!(seen, want);
    }

    /// Idleness never goes negative, whatever the timings.
    #[test]
    fn idle_is_never_negative((wf, run) in arb_pair()) {
        let ws = derive::waves(&wf);
        for s in &run.steps {
            prop_assert!(derive::idle_of(&ws, &run, &s.id) >= 0.0, "{} idle < 0", s.id);
        }
    }

    /// The run's duration covers the end of every step that RAN.
    ///
    /// This re-listed the flags by hand (`never_born == Some(true)`) and so
    /// encoded the OLD contract — it kept passing after `total_time` learned
    /// to exclude SKIPPED steps too, and the generator's hardcoded `None`
    /// meant it could never notice. It rides `derive::ran` now: the same
    /// predicate the derivation uses, so the property cannot drift from the
    /// law it is supposed to guard.
    #[test]
    fn total_time_covers_every_step_that_ran((_wf, run) in arb_pair()) {
        let total = derive::total_time(&run);
        for s in &run.steps {
            if !derive::ran(s) {
                continue;
            }
            prop_assert!(total >= s.start + s.dur, "total {total} < end of {}", s.id);
        }
    }

    /// A crowned bottleneck ALWAYS has at least one waiter.
    #[test]
    fn a_crowned_neck_has_waiters((wf, run) in arb_pair()) {
        let ws = derive::waves(&wf);
        if let Some(neck) = derive::bottleneck(&ws, &run) {
            prop_assert!(neck.blocked >= 1, "a free neck is crowned");
            prop_assert!(neck.idle_total > 0.0, "a costless neck is crowned");
        }
    }

    /// groups_of keeps every task exactly once — nothing lost, nothing
    /// doubled (the fan-out fold is a partition too).
    #[test]
    fn groups_partition_every_task(wf in arb_workflow()) {
        let mut count = 0;
        for g in derive::groups_of(&wf) {
            match g {
                nika_tui_core::model::Group::Single(_) => count += 1,
                nika_tui_core::model::Group::Fanout { members, .. } => count += members.len(),
                _ => unreachable!("two variants today · a third is a deliberate law change"),
            }
        }
        prop_assert_eq!(count, wf.tasks.len());
    }
}
