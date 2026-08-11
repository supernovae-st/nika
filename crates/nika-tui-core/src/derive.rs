// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The derivations — EVERYTHING a screen shows passes through here.
//! Ported line for line from the studio's law (the same thresholds, the
//! same accumulation order — parity is measured on the bits, so the port
//! keeps the original arithmetic exactly).
//!
//! Two numbers carry their history in comments because both were once
//! written by hand and both lied: the run's duration and the bottleneck's
//! idle sum. Derived, never written.

use std::collections::BTreeSet;

use crate::model::{Group, Run, Task, Touch, Verb, Workflow};

/// A task's wave — the depth of its dependency chain. A cycle answers 0
/// instead of looping (the check catches the cycle; the renderer must not
/// hang on a file it refused).
fn wave_of_rec(wf: &Workflow, id: &str, seen: &mut BTreeSet<String>) -> usize {
    if !seen.insert(id.to_owned()) {
        return 0;
    }
    let Some(t) = wf.tasks.iter().find(|x| x.id == id) else {
        return 0;
    };
    if t.needs.is_empty() {
        return 0;
    }
    1 + t
        .needs
        .iter()
        .map(|n| wave_of_rec(wf, n, seen))
        .max()
        .unwrap_or(0)
}

/// A task's wave — fresh guard per call (the studio's default-parameter
/// semantics, kept: a wave question never inherits another's guard).
#[must_use]
pub fn wave_of(wf: &Workflow, id: &str) -> usize {
    wave_of_rec(wf, id, &mut BTreeSet::new())
}

/// The tasks grouped by wave — the real execution order, declared order
/// inside each wave.
#[must_use]
pub fn waves(wf: &Workflow) -> Vec<Vec<Task>> {
    let mut out: Vec<Vec<Task>> = Vec::new();
    for t in &wf.tasks {
        let w = wave_of(wf, &t.id);
        if out.len() <= w {
            out.resize_with(w + 1, Vec::new);
        }
        out[w].push(t.clone());
    }
    out
}

/// When a wave ends — the max of its ends. Steps never started do not
/// extend it (they carry no end; the max of an empty wave is 0).
#[must_use]
pub fn wave_end(wf: &Workflow, run: &Run, w: usize) -> f64 {
    let by_wave = waves(wf);
    let ids: BTreeSet<&str> = by_wave
        .get(w)
        .map(|g| g.iter().map(|t| t.id.as_str()).collect())
        .unwrap_or_default();
    run.steps
        .iter()
        .filter(|s| ids.contains(s.id.as_str()))
        .map(|s| s.start + s.dur)
        .fold(0.0, f64::max)
}

/// The time a step spends WAITING for its own wave to end.
#[must_use]
pub fn idle_of(wf: &Workflow, run: &Run, id: &str) -> f64 {
    let Some(s) = run.steps.iter().find(|x| x.id == id) else {
        return 0.0;
    };
    let Some(t) = wf.tasks.iter().find(|x| x.id == id) else {
        return 0.0;
    };
    let end = wave_end(wf, run, wave_of(wf, &t.id));
    (end - (s.start + s.dur)).max(0.0)
}

/// ⭐ THE BOTTLENECK — the step that holds its wave alone, and the time
/// the others spend waiting for it. `idle_total` is not decorative:
/// without naming the blocked, the bench printed « 420,4 s d'attente »
/// under a 72,5 s run — eleven steps wait in parallel, so name them.
///
/// ⭐ A bottleneck that costs nothing ISN'T one — between two near-equal
/// checks one finishes "last" and was getting crowned. At least ONE step
/// must be waiting (`blocked > 0`), or the wave reports nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Neck {
    /// The holder's id.
    pub id: String,
    /// The summed idle of the wave.
    pub idle_total: f64,
    /// How many steps actually waited (> 0.05 s).
    pub blocked: usize,
}

/// The worst holder across waves — by summed idle.
#[must_use]
pub fn bottleneck(wf: &Workflow, run: &Run) -> Option<Neck> {
    let mut best: Option<Neck> = None;
    for (w, group) in waves(wf).iter().enumerate() {
        if group.len() < 2 {
            continue;
        }
        let end = wave_end(wf, run, w);
        let Some(holder) = group.iter().find(|t| {
            run.steps
                .iter()
                .find(|x| x.id == t.id)
                .is_some_and(|s| (s.start + s.dur - end).abs() < 1e-6)
        }) else {
            continue;
        };
        let idle_total = group.iter().map(|t| idle_of(wf, run, &t.id)).sum();
        let blocked = group
            .iter()
            .filter(|t| idle_of(wf, run, &t.id) > 0.05)
            .count();
        if blocked == 0 {
            continue;
        }
        if best.as_ref().is_none_or(|b| idle_total > b.idle_total) {
            best = Some(Neck {
                id: holder.id.clone(),
                idle_total,
                blocked,
            });
        }
    }
    best
}

/// The run's spend — derived, never written.
#[must_use]
pub fn total_cost(run: &Run) -> f64 {
    run.steps.iter().map(|s| s.cost.unwrap_or(0.0)).sum()
}

/// The run's DURATION — derived, never written. A never-born step has no
/// end and does not extend it. (It was hand-written once, and wrong: 1,5 s
/// announced for a run ending at 1,6 s.)
#[must_use]
pub fn total_time(run: &Run) -> f64 {
    run.steps
        .iter()
        .filter(|s| s.never_born != Some(true))
        .map(|s| s.start + s.dur)
        .fold(0.0, f64::max)
}

/// The verbs actually used — an absent verb on screen is noise. Declared
/// order, deduplicated.
#[must_use]
pub fn verbs_used(wf: &Workflow) -> Vec<Verb> {
    let mut out: Vec<Verb> = Vec::new();
    for t in &wf.tasks {
        if !out.contains(&t.verb) {
            out.push(t.verb);
        }
    }
    out
}

/// Did the run break — so no ⟦ bottleneck ⟧ gets painted over a failure.
#[must_use]
pub fn has_failed(run: &Run) -> bool {
    run.steps.iter().any(|s| s.failed.is_some())
}

/// Cost per verb — answers ⟦ where does the money go ⟧.
#[must_use]
pub fn cost_by_verb(wf: &Workflow, run: &Run) -> [(&'static str, f64); 4] {
    let mut sums = [
        ("infer", 0.0),
        ("exec", 0.0),
        ("invoke", 0.0),
        ("agent", 0.0),
    ];
    for s in &run.steps {
        let Some(t) = wf.tasks.iter().find(|x| x.id == s.id) else {
            continue;
        };
        let Some(c) = s.cost else { continue };
        let idx = match t.verb {
            Verb::Infer => 0,
            Verb::Exec => 1,
            Verb::Invoke => 2,
            Verb::Agent => 3,
        };
        sums[idx].1 += c;
    }
    sums
}

/// The workflow's whole blast radius — the union of what its steps touch.
#[must_use]
pub fn blast_radius(wf: &Workflow) -> Vec<Touch> {
    let mut out: Vec<Touch> = Vec::new();
    for t in &wf.tasks {
        for c in t.touches.as_deref().unwrap_or(&[]) {
            if !out.contains(c) {
                out.push(*c);
            }
        }
    }
    out
}

/// ⭐ COHERENCE — what the steps TOUCH versus what the file DECLARES. An
/// undeclared touch is a NIKA-AUTH-006 waiting for its hour — and the lens
/// can say it BEFORE the run, not after.
#[must_use]
pub fn undeclared(wf: &Workflow) -> Vec<Touch> {
    blast_radius(wf)
        .into_iter()
        .filter(|c| {
            let key = c.permit_key();
            !wf.permits
                .iter()
                .any(|p| p.trim_start().starts_with(&format!("{key}:")))
        })
        .collect()
}

/// ⭐⭐ THE FAN-OUT — twelve translations are not twelve steps; they are
/// ONE declared step with twelve items. Group the tasks sharing verb,
/// tool AND dependencies — exactly the signature of a `for_each`.
/// Derived, never annotated.
#[must_use]
pub fn groups_of(wf: &Workflow) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    let mut taken: BTreeSet<&str> = BTreeSet::new();
    for t in &wf.tasks {
        if taken.contains(t.id.as_str()) {
            continue;
        }
        let sig = signature(t);
        let kin: Vec<&Task> = wf
            .tasks
            .iter()
            .filter(|o| !taken.contains(o.id.as_str()) && signature(o) == sig)
            .collect();
        if kin.len() >= 3 {
            for k in &kin {
                taken.insert(k.id.as_str());
            }
            // the common prefix — what the `for_each` named once
            let first = kin[0].id.clone();
            let name = match first.rfind(['-', '_']) {
                Some(at) => first[..at].to_owned(),
                None => first,
            };
            out.push(Group::Fanout {
                name,
                members: kin.into_iter().cloned().collect(),
            });
        } else {
            taken.insert(t.id.as_str());
            out.push(Group::Single(t.clone()));
        }
    }
    out
}

/// The grouping signature — verb · tool · the SORTED needs (order is not
/// meaning here).
fn signature(t: &Task) -> String {
    let mut needs: Vec<&str> = t.needs.iter().map(String::as_str).collect();
    needs.sort_unstable();
    format!(
        "{}|{}|{}",
        verb_name(t.verb),
        t.tool.as_deref().unwrap_or(""),
        needs.join(",")
    )
}

/// The verb's lowercase spelling (the fixture's spelling).
fn verb_name(v: Verb) -> &'static str {
    match v {
        Verb::Infer => "infer",
        Verb::Exec => "exec",
        Verb::Invoke => "invoke",
        Verb::Agent => "agent",
    }
}

/// A group's span — the shortest, the longest, the total spent.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupSpan {
    /// Members with a recorded step.
    pub n: usize,
    /// The shortest member duration.
    pub min: f64,
    /// The longest member duration.
    pub max: f64,
    /// The summed member cost.
    pub cost: f64,
    /// The slowest member's id.
    pub slowest: String,
}

/// The span of one fan-out group over a run.
#[must_use]
pub fn group_span(members: &[Task], run: &Run) -> Option<GroupSpan> {
    let steps: Vec<&crate::model::Step> = members
        .iter()
        .filter_map(|m| run.steps.iter().find(|s| s.id == m.id))
        .collect();
    let first = steps.first()?;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut cost = 0.0;
    let mut slowest = *first;
    for s in &steps {
        min = min.min(s.dur);
        max = max.max(s.dur);
        cost += s.cost.unwrap_or(0.0);
        if s.dur > slowest.dur {
            slowest = s;
        }
    }
    Some(GroupSpan {
        n: steps.len(),
        min,
        max,
        cost,
        slowest: slowest.id.clone(),
    })
}

/// The steps touching a given class — ⟦ who can write at my place? ⟧
#[must_use]
pub fn tasks_touching(wf: &Workflow, c: Touch) -> Vec<&Task> {
    wf.tasks
        .iter()
        .filter(|t| t.touches.as_deref().unwrap_or(&[]).contains(&c))
        .collect()
}

/// The foreign code — what you didn't write and the binary doesn't embed.
#[must_use]
pub fn foreign_tasks(wf: &Workflow) -> Vec<&Task> {
    wf.tasks
        .iter()
        .filter(|t| {
            matches!(
                t.origin,
                Some(crate::model::Origin::Mcp | crate::model::Origin::Registry)
            )
        })
        .collect()
}
