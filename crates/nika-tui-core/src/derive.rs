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

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Group, Run, Step, Task, Touch, Verb, Workflow};

/// The wave assignment, computed ONCE per workflow and shared by every
/// derivation below (the swarm's F2 · recomputing `waves()` per question
/// was O(n³) on a chain — one pass, then reads).
///
/// NO RECURSION — an explicit stack walks each dependency chain, so a
/// hostile multi-thousand-task file cannot trap the 1 MiB wasm stack
/// (the swarm's F1 · the sibling crate's jq-bomb class). The law is the
/// TRUE longest path — amended upstream on 2026-08-12 after the swarm
/// proved the studio's shared-guard quirk could seat a task in its own
/// dependency's wave (the sonde-diamant fixture pins the class forever).
/// The cycle guard answers 0, because the check refuses cycles before any
/// render and a refused file must never hang the surface.
#[derive(Debug, Clone)]
pub struct Waves {
    by_wave: Vec<Vec<Task>>,
    of_task: BTreeMap<String, usize>,
}

impl Waves {
    /// The tasks grouped by wave — the real execution order, declared
    /// order inside each wave.
    #[must_use]
    pub fn groups(&self) -> &[Vec<Task>] {
        &self.by_wave
    }

    /// A task's wave index (0 when unknown).
    #[must_use]
    pub fn of(&self, id: &str) -> usize {
        self.of_task.get(id).copied().unwrap_or(0)
    }

    /// Is this id a declared task? (a ghost step in an old journal is not
    /// one — the fold reads journals written against older files).
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.of_task.contains_key(id)
    }

    /// The wave count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_wave.len()
    }

    /// No waves at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_wave.is_empty()
    }
}

/// One task's wave, walked ITERATIVELY from the root — post-order DFS on
/// an explicit stack. `visiting` is the fresh per-root guard: a revisited
/// id (a cycle) answers 0, exactly the studio's `seen` set.
fn wave_of_walk<'a>(
    tasks: &BTreeMap<&'a str, &'a Task>,
    root: &'a str,
    memo: &mut BTreeMap<&'a str, usize>,
) -> usize {
    enum Frame<'a> {
        Enter(&'a str),
        Exit(&'a str),
    }
    // `visiting` stays PER ROOT: it is this walk's cycle guard, and sharing it
    // would make an earlier walk's path look like a cycle to a later one.
    // Only the answers are shared, never the in-flight state.
    let mut visiting: BTreeSet<&str> = BTreeSet::new();
    let mut stack = vec![Frame::Enter(root)];
    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Enter(id) => {
                if memo.contains_key(id) {
                    continue;
                }
                if !visiting.insert(id) {
                    continue; // a cycle answers 0, never loops
                }
                let Some(t) = tasks.get(id) else {
                    memo.insert(id, 0);
                    continue;
                };
                if t.needs.is_empty() {
                    memo.insert(id, 0);
                    continue;
                }
                stack.push(Frame::Exit(id));
                for n in t.needs.iter().rev() {
                    stack.push(Frame::Enter(n.as_str()));
                }
            }
            Frame::Exit(id) => {
                visiting.remove(id);
                let w = tasks.get(id).map_or(0, |t| {
                    1 + t
                        .needs
                        .iter()
                        .map(|n| memo.get(n.as_str()).copied().unwrap_or(0))
                        .max()
                        .unwrap_or(0)
                });
                memo.insert(id, w);
            }
        }
    }
    memo.get(root).copied().unwrap_or(0)
}

/// The tasks' waves, computed once — the real execution order.
#[must_use]
pub fn waves(wf: &Workflow) -> Waves {
    let tasks: BTreeMap<&str, &Task> = wf.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut by_wave: Vec<Vec<Task>> = Vec::new();
    let mut of_task: BTreeMap<String, usize> = BTreeMap::new();
    // The memo is hoisted OUT of the per-root walk. It used to be built fresh
    // for every task, so an n-task chain walked n times: a measured 5.1 s on
    // the 5000-chain the security test pins, and quadratic by construction.
    // The doc above already promised "computed ONCE"; this is the line that
    // makes the promise true.
    //
    // ⚠️ DECLARED semantic change · a CYCLE now answers whatever the FIRST
    // walk that met it recorded, instead of being re-decided per root. The
    // previous answer was already arbitrary (whichever root arrived first
    // within its own walk), and a cyclic workflow never reaches this code in
    // production because `nika check` refuses it. Deterministic-and-different
    // beats arbitrary-and-recomputed, and it is written here rather than
    // discovered later.
    let mut memo: BTreeMap<&str, usize> = BTreeMap::new();
    for t in &wf.tasks {
        let w = wave_of_walk(&tasks, &t.id, &mut memo);
        of_task.insert(t.id.clone(), w);
        if by_wave.len() <= w {
            by_wave.resize_with(w + 1, Vec::new);
        }
        by_wave[w].push(t.clone());
    }
    Waves { by_wave, of_task }
}

/// When a wave ends — the max of its ends. Steps never started do not
/// extend it (they carry no end; the max of an empty wave is 0).
#[must_use]
pub fn wave_end(ws: &Waves, run: &Run, w: usize) -> f64 {
    let ids: BTreeSet<&str> = ws
        .groups()
        .get(w)
        .map(|g| g.iter().map(|t| t.id.as_str()).collect())
        .unwrap_or_default();
    run.steps
        .iter()
        .filter(|s| ids.contains(s.id.as_str()))
        .map(|s| s.start + s.dur)
        .fold(0.0, f64::max)
}

/// The time a step spends WAITING for its own wave to end. A ghost step
/// (an id the workflow does not declare — an old journal against a
/// renamed file) has no wave and no idle: 0, never a phantom number.
#[must_use]
pub fn idle_of(ws: &Waves, run: &Run, id: &str) -> f64 {
    if !ws.contains(id) {
        return 0.0;
    }
    let Some(s) = run.steps.iter().find(|x| x.id == id) else {
        return 0.0;
    };
    idle_against(s, wave_end(ws, run, ws.of(id)))
}

/// One step's wait against a KNOWN wave end — the single place the rule
/// lives. `idle_of` looks the end up; `bottleneck` already holds it and
/// would otherwise pay for a rebuild per member per call.
///
/// A step that never STARTED never waited. The fold stamps a cancelled or
/// skipped step at 0/0 because it has no real instants, and that zero read
/// as "started at the top of the wave and idled until the end" — so a
/// Ctrl-C run had its dead steps accusing the wave's holder of the whole
/// wave. That is the « 420,4 s d'attente » class the doc below says is
/// closed, reopened by a different door. Measured by a Rust review,
/// 2026-08-13: {real 10 s · dead `never_born`} crowned a bottleneck with
/// blocked = 1, on a step nobody ever waited for.
///
/// The studio carries the same hole. This crate exists so the law is written
/// ONCE, so it is fixed here and the surfaces inherit it — which is also why
/// this helper exists rather than a second copy of the rule inside
/// `bottleneck`.
fn idle_against(s: &Step, end: f64) -> f64 {
    if s.never_born == Some(true) || s.skipped == Some(true) {
        return 0.0;
    }
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
pub fn bottleneck(ws: &Waves, run: &Run) -> Option<Neck> {
    let mut best: Option<Neck> = None;
    for (w, group) in ws.groups().iter().enumerate() {
        if group.len() < 2 {
            continue;
        }
        let end = wave_end(ws, run, w);
        let Some(holder) = group.iter().find(|t| {
            run.steps
                .iter()
                .find(|x| x.id == t.id)
                .is_some_and(|s| (s.start + s.dur - end).abs() < 1e-6)
        }) else {
            continue;
        };
        // `idle_of` was called TWICE per member here, and each call rebuilt
        // this wave's id-set and rescanned every step to recompute the `end`
        // already in hand two lines up. On the 5000-chain the security test
        // pins, that arithmetic is a measured 2.8 s. Compute each wait ONCE,
        // against the end we hold. Same rule, one copy, via `idle_against`.
        let idles: Vec<f64> = group
            .iter()
            .map(|t| {
                run.steps
                    .iter()
                    .find(|x| x.id == t.id)
                    .map_or(0.0, |s| idle_against(s, end))
            })
            .collect();
        let idle_total: f64 = idles.iter().sum();
        let blocked = idles.iter().filter(|i| **i > 0.05).count();
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
    // The signature was recomputed inside the inner scan, so every pair paid
    // a `format!` and a needs-sort: n² allocations. A Rust review measured
    // 2.8 s on the 5000-chain the security test already pins. Compute each
    // task's signature ONCE, then compare the cached strings.
    //
    // Same answers by construction: `signature` is a pure function of the
    // task, and nothing in this loop mutates a task.
    let sigs: Vec<String> = wf.tasks.iter().map(signature).collect();

    let mut out: Vec<Group> = Vec::new();
    let mut taken: BTreeSet<&str> = BTreeSet::new();
    for (i, t) in wf.tasks.iter().enumerate() {
        if taken.contains(t.id.as_str()) {
            continue;
        }
        let sig = &sigs[i];
        let kin: Vec<&Task> = wf
            .tasks
            .iter()
            .enumerate()
            .filter(|(j, o)| !taken.contains(o.id.as_str()) && &sigs[*j] == sig)
            .map(|(_, o)| o)
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
