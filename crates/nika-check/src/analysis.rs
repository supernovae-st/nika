// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The DAG engineering read — the parallelism rung of the check ladder.
//!
//! Wave counts answer « what does the wave scheduler do »; this module
//! answers the scheduler-INDEPENDENT questions an engineer asks of a
//! pipeline:
//!
//! - **exact maximum parallelism** — the maximum antichain (Dilworth
//!   1950), via Fulkerson's reduction (1956) to bipartite matching
//!   (Hopcroft-Karp 1973) on the transitive closure, with a König
//!   witness: a concrete set of tasks that CAN run together;
//! - **pinch points** — tasks comparable to every other task: the DAG
//!   narrows to width 1 there, nothing can overlap them. (Dominator
//!   analysis is the WRONG tool under AND-join semantics — every task
//!   executes, so « unavoidable to reach the sink » is trivial;
//!   witness: `a→v→b` plus `a→b` — `v` dominates nothing, yet nothing
//!   can run beside it.)
//! - **blast radius** — AND-join makes failure analysis exact: a failed
//!   task blocks EVERY transitive dependent;
//! - **write-write conflicts** (F-P15 · NEP-0014 law 1) — two tasks that
//!   CAN run concurrently (incomparable) and both write the same STATIC
//!   `nika:write`/`nika:edit` key (literal · resolved bare ref ·
//!   identical immutable ref — [`static_write_key`]), compared under
//!   LEXICAL normalization (`./out/x.md` ≡ `out//x.md` ≡
//!   `out/d/../x.md` ≡ `out/x.md` — pure text, no filesystem claim; see
//!   [`normalize_lexical`]): last-writer-wins is a race the file never
//!   declares. The LAW (a `NIKA-SEC-012` finding · deterministic) —
//!   promoted from its advisory-hint era 2026-07-29: parallelism is safe
//!   exactly where the effects are provably disjoint, and a hint is not
//!   a boundary. Above [`ANALYSIS_TASK_CAP`] the closure-based pair scan
//!   is skipped and the skip is STATED ([`DagRead::stated_miss`] — the
//!   report carries it as a hint, never a silent no-claim); the
//!   closure-free `for_each` same-path flavor still judges.
//!
//! Width can EXCEED the largest wave — witness `p→a1→x2 · p→x1 ·
//! isolated x0`: waves peak at 2, width is 3 (`{x0, x1, x2}`). The
//! wave-barrier runtime leaves that parallelism unused; the report says
//! so honestly (`max parallelism` = the wave peak as executed · `width`
//! = what the DAG permits).

use std::collections::BTreeMap;

use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use nika_schema::source::Spanned;

/// The write-write law's wire code (F-P15 · NEP-0014 law 1) — the
/// security class: two unordered writers racing one path is an effect
/// overlap the boundary never sanctioned.
const WRITE_CONFLICT_CODE: &str = "NIKA-SEC-012";

/// One write-write conflict (F-P15 · NEP-0014 law 1): two tasks
/// incomparable in the DAG closure whose STATIC `nika:write` /
/// `nika:edit` keys collide under lexical normalization, or a
/// `for_each` fan writing one constant path — the last-writer-wins
/// race, refused at check.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct WriteConflict {
    /// The first writer's task id (the fan itself for the `for_each`
    /// flavor).
    pub task: String,
    /// The second writer — `None` for the `for_each` flavor (the task
    /// races its own iterations).
    pub other: Option<String>,
    /// The colliding key in its LEXICAL normal form (the pair flavor —
    /// both keys reduce to it) · the key as spelled (the `for_each`
    /// flavor — nothing is compared). A key is a literal path, or the
    /// canonical bare-ref form (`${{ inputs.f }}`) when the shared
    /// value is not statically known (`static_write_key`).
    pub path: String,
    /// The witness sentence (the race, named — both spellings when the
    /// literals differ textually).
    pub detail: String,
    /// The one repair.
    pub fix: String,
}

impl WriteConflict {
    /// The canonical spec code this finding stamps.
    #[must_use]
    pub fn wire_code(&self) -> &'static str {
        WRITE_CONFLICT_CODE
    }
}

/// The scheduler-independent DAG read (additive · `report_version`
/// stays 1). `None` when conformance fails — no valid order exists, so
/// no claim is made (skipped, never wrong).
#[derive(Debug, Clone, serde::Serialize)]
#[non_exhaustive]
pub struct DagAnalysis {
    /// Exact maximum parallelism — the size of a maximum antichain.
    pub width: usize,
    /// One maximum antichain (task ids) — tasks that CAN run together.
    pub width_witness: Vec<String>,
    /// Tasks comparable to every other task (the DAG serializes there).
    /// Empty for single-task workflows (trivially true ⇒ noise).
    pub pinch_points: Vec<String>,
    /// Per task: how many downstream tasks a failure blocks (AND-join ·
    /// transitive dependents). Only tasks with `blocks > 0`, widest
    /// first then id order — deterministic.
    pub blast_radius: Vec<TaskBlast>,
}

/// One task's failure blast radius.
#[derive(Debug, Clone, serde::Serialize)]
#[non_exhaustive]
pub struct TaskBlast {
    /// The task id.
    pub task: String,
    /// Transitive dependents blocked if this task fails.
    pub blocks: usize,
}

/// What the analysis pass hands back to the report builder.
pub(super) struct DagRead {
    /// The engineering read (`None` when no valid order exists).
    pub analysis: Option<DagAnalysis>,
    /// Write-write conflicts (F-P15 · the law, never advisory).
    pub conflicts: Vec<WriteConflict>,
    /// The STATED MISS when the closure-based read was skipped for width
    /// (H6 · the verdict-coverage law: a law that did not judge says so,
    /// in the report's own surface — the builder folds it as a hint).
    /// `None` when the read ran, and on the conformance skip — that one
    /// is already stated by the CONFORM rows themselves.
    pub stated_miss: Option<String>,
}

impl DagRead {
    /// The skipped read (conformance failed — claim nothing; the
    /// CONFORM rows state why, so no extra miss rides).
    pub(super) fn skipped() -> Self {
        Self {
            analysis: None,
            conflicts: Vec::new(),
            stated_miss: None,
        }
    }
}

/// Above this task count the exact read is SKIPPED — and the skip is a
/// STATED MISS ([`DagRead::stated_miss`]), never silent; the
/// closure-free `for_each` same-path flavor of the write-write law
/// still runs (H6). The materialized closure is O(n²) and the matching
/// phases O(E√V): at the parser's 10k-task cap that is ~400 MB +
/// minutes of CPU from a few-KB YAML — a `DoS` surface on a checker
/// that markets static safety (house precedent: the marked-yaml depth
/// caps · the 64 MiB ceilings). At 2 000 tasks the worst case stays
/// ~60 MB · milliseconds typical — far above any real workflow. The cap
/// also bounds the augmenting DFS recursion (≤ n frames · see
/// [`hk_dfs`]).
pub(crate) const ANALYSIS_TASK_CAP: usize = 2_000;

/// Run the engineering read over a conformant workflow.
pub(super) fn read_dag(wf: &RawWorkflow, topo_waves: &[Vec<usize>]) -> DagRead {
    let n = wf.tasks.len();
    if n == 0 {
        return DagRead {
            analysis: Some(DagAnalysis {
                width: 0,
                width_witness: Vec::new(),
                pinch_points: Vec::new(),
                blast_radius: Vec::new(),
            }),
            conflicts: Vec::new(),
            stated_miss: None,
        };
    }
    if n > ANALYSIS_TASK_CAP {
        // H6 · the verdict-coverage law at the cap: the skip is STATED
        // (never a silent no-claim), and the closure-free flavor of the
        // write-write law still judges — a fan racing its own iterations
        // needs no closure.
        return DagRead {
            analysis: None,
            conflicts: scan_for_each_fans(wf),
            stated_miss: Some(format!(
                "{n} tasks is over the {ANALYSIS_TASK_CAP}-task analysis cap (the O(n²) closure \
                 stays a DoS floor): width · pinch points · blast radius made NO claim, and the \
                 write-write law judged only the `for_each` same-path flavor — the pair scan did \
                 not run"
            )),
        };
    }

    let down = downstream_adjacency(&wf.tasks);
    let desc = descendant_closure(n, &down, topo_waves);
    let closure_adj: Vec<Vec<usize>> = desc.iter().map(|row| set_bits(row)).collect();

    let (match_left, match_right, matched) = hopcroft_karp(n, &closure_adj);
    let witness_idx = koenig_witness(n, &closure_adj, &match_left, &match_right);

    let id_of = |i: usize| wf.tasks[i].value.id.value.clone();
    let desc_count: Vec<usize> = desc
        .iter()
        .map(|row| row.iter().map(|w| w.count_ones() as usize).sum())
        .collect();
    let mut anc_count = vec![0usize; n];
    for row in &closure_adj {
        for &v in row {
            anc_count[v] += 1;
        }
    }

    let pinch_points: Vec<String> = if n < 2 {
        Vec::new()
    } else {
        (0..n)
            .filter(|&i| anc_count[i] + desc_count[i] == n - 1)
            .map(id_of)
            .collect()
    };

    let mut blast: Vec<TaskBlast> = (0..n)
        .filter(|&i| desc_count[i] > 0)
        .map(|i| TaskBlast {
            task: id_of(i),
            blocks: desc_count[i],
        })
        .collect();
    blast.sort_by(|a, b| b.blocks.cmp(&a.blocks).then_with(|| a.task.cmp(&b.task)));

    DagRead {
        analysis: Some(DagAnalysis {
            width: n - matched,
            width_witness: witness_idx.into_iter().map(id_of).collect(),
            pinch_points,
            blast_radius: blast,
        }),
        conflicts: scan_parallel_writers(wf, &desc),
        stated_miss: None,
    }
}

/// `G_p` as downstream adjacency (producer → dependent), indices into
/// `wf.tasks` — derived from the `with:`/`after:` boundary (the one edge
/// computation). Unresolved targets are conformance errors and this
/// pass only runs on conformant workflows — still skipped defensively.
fn downstream_adjacency(tasks: &[Spanned<RawTask>]) -> Vec<Vec<usize>> {
    let ids: BTreeMap<&str, usize> = tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.value.id.value.as_str(), i))
        .collect();
    let mut down = vec![Vec::new(); tasks.len()];
    for (i, task) in tasks.iter().enumerate() {
        for producer in crate::analyzer::edges::producer_ids(&task.value) {
            if let Some(&from) = ids.get(producer.as_str()) {
                down[from].push(i);
            }
        }
    }
    down
}

/// Per-task descendant sets as u64-word bitsets, accumulated in reverse
/// topological order: `desc(u) = ∪ child (child ∪ desc(child))`.
fn descendant_closure(n: usize, down: &[Vec<usize>], topo: &[Vec<usize>]) -> Vec<Vec<u64>> {
    let words = n.div_ceil(64);
    let mut desc = vec![vec![0u64; words]; n];
    for wave in topo.iter().rev() {
        for &u in wave {
            let mut row = vec![0u64; words];
            for &child in &down[u] {
                row[child / 64] |= 1u64 << (child % 64);
                for w in 0..words {
                    row[w] |= desc[child][w];
                }
            }
            desc[u] = row;
        }
    }
    desc
}

/// Indices of the set bits in a bitset row, ascending.
fn set_bits(row: &[u64]) -> Vec<usize> {
    let mut out = Vec::new();
    for (w, &word) in row.iter().enumerate() {
        let mut bits = word;
        while bits != 0 {
            let tz = bits.trailing_zeros() as usize;
            out.push(w * 64 + tz);
            bits &= bits - 1;
        }
    }
    out
}

/// Hopcroft-Karp maximum bipartite matching (1973 · O(E√V)) over the
/// comparability graph: left `u` → right `v` for every `u ≺ v` in the
/// closure. Dilworth: width = n − |maximum matching|.
///
/// The √V phase bound needs BOTH textbook details: the BFS truncates at
/// the first layer reaching a free right vertex (`free_layer`), and the
/// DFS accepts a free right vertex only at exactly that layer — without
/// them phases may augment along non-shortest paths and the bound
/// degrades to O(V·E) (review finding · the matching stays maximum
/// either way by Berge, only the complexity claim breaks).
fn hopcroft_karp(n: usize, adj: &[Vec<usize>]) -> (Vec<Option<usize>>, Vec<Option<usize>>, usize) {
    let mut match_left: Vec<Option<usize>> = vec![None; n];
    let mut match_right: Vec<Option<usize>> = vec![None; n];
    let mut matched = 0usize;

    while let Some((mut dist, free_layer)) = hk_bfs(n, adj, &match_left, &match_right) {
        let mut progressed = false;
        for u in 0..n {
            if match_left[u].is_none()
                && hk_dfs(
                    u,
                    adj,
                    &mut dist,
                    free_layer,
                    &mut match_left,
                    &mut match_right,
                )
            {
                matched += 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    (match_left, match_right, matched)
}

/// BFS layering from unmatched left vertices. Returns the per-vertex
/// layers AND the layer of the nearest free right vertex; `None` = no
/// augmenting path exists (the matching is maximum). Exploration stops
/// at `free_layer` — deeper layers cannot start a SHORTEST augmenting
/// path and visiting them is what forfeits the √V bound.
fn hk_bfs(
    n: usize,
    adj: &[Vec<usize>],
    match_left: &[Option<usize>],
    match_right: &[Option<usize>],
) -> Option<(Vec<usize>, usize)> {
    const INF: usize = usize::MAX;
    let mut dist = vec![INF; n];
    let mut queue = std::collections::VecDeque::new();
    for u in 0..n {
        if match_left[u].is_none() {
            dist[u] = 0;
            queue.push_back(u);
        }
    }
    let mut free_layer = INF;
    while let Some(u) = queue.pop_front() {
        if dist[u] >= free_layer {
            continue;
        }
        for &v in &adj[u] {
            match match_right[v] {
                None => free_layer = free_layer.min(dist[u].saturating_add(1)),
                Some(next) => {
                    if dist[next] == INF {
                        dist[next] = dist[u].saturating_add(1);
                        queue.push_back(next);
                    }
                }
            }
        }
    }
    (free_layer != INF).then_some((dist, free_layer))
}

/// Layered augmenting DFS (the Hopcroft-Karp phase step). Recursion
/// depth ≤ augmenting-path length ≤ [`ANALYSIS_TASK_CAP`] frames — the
/// `read_dag` size gate is what makes the recursion safe everywhere
/// (including a 2 MiB worker stack).
fn hk_dfs(
    u: usize,
    adj: &[Vec<usize>],
    dist: &mut [usize],
    free_layer: usize,
    match_left: &mut [Option<usize>],
    match_right: &mut [Option<usize>],
) -> bool {
    // Indexed loop on purpose: the recursive call needs `dist`/matches
    // mutably while `adj[u]` would be held by a `for &v in` borrow.
    for i in 0..adj[u].len() {
        let v = adj[u][i];
        let advance = match match_right[v] {
            // Free right vertex: accept only at the shortest-path layer.
            None => dist[u].saturating_add(1) == free_layer,
            Some(next) => {
                dist[next] == dist[u].saturating_add(1)
                    && hk_dfs(next, adj, dist, free_layer, match_left, match_right)
            }
        };
        if advance {
            match_left[u] = Some(v);
            match_right[v] = Some(u);
            return true;
        }
    }
    dist[u] = usize::MAX;
    false
}

/// König construction: `Z` = vertices reachable from unmatched LEFT
/// vertices by alternating paths (left→right via non-matching edges ·
/// right→left via matching edges). The maximum antichain is
/// `{ v : v_left ∈ Z ∧ v_right ∉ Z }` — size n − |matching|, pairwise
/// incomparable by the alternating-path argument.
fn koenig_witness(
    n: usize,
    adj: &[Vec<usize>],
    match_left: &[Option<usize>],
    match_right: &[Option<usize>],
) -> Vec<usize> {
    let mut z_left = vec![false; n];
    let mut z_right = vec![false; n];
    let mut queue = std::collections::VecDeque::new();
    for u in 0..n {
        if match_left[u].is_none() {
            z_left[u] = true;
            queue.push_back(u);
        }
    }
    while let Some(u) = queue.pop_front() {
        for &v in &adj[u] {
            if match_left[u] == Some(v) || z_right[v] {
                continue;
            }
            z_right[v] = true;
            if let Some(back) = match_right[v]
                && !z_left[back]
            {
                z_left[back] = true;
                queue.push_back(back);
            }
        }
    }
    (0..n).filter(|&i| z_left[i] && !z_right[i]).collect()
}

/// The STATIC write key of a task's `nika:write`/`nika:edit` target —
/// two equal keys provably denote the same runtime path:
///
/// - a **literal** path (no template) — the key is the path;
/// - a bare immutable-authority ref whose declaration carries a string
///   literal — resolved through the ONE shared resolver
///   ([`crate::static_literal_of`]), so a literal writer and a
///   `${{ const.p }}` writer on the same path collide;
/// - a bare immutable-authority ref with NO declared literal — the key
///   is its canonical form: two IDENTICAL `${{ inputs.f }}` writers
///   target the same file even while its value is unknown (inputs bind
///   once per run · const/config never change). Measured 2026-07-30:
///   this exact shape rendered green while its literal twin was
///   refused.
///
/// Anything else (task refs · `with:` bindings · `${{ item }}` ·
/// concatenations) is dynamic and makes no static claim. (H4 · both
/// writer builtins take `path:` identically, and the edit's
/// read-modify-write loses updates to a concurrent writer exactly like
/// the write's rename does.)
fn static_write_key(wf: &RawWorkflow, task: &RawTask) -> Option<String> {
    let RawAction::Invoke(invoke) = &task.action else {
        return None;
    };
    if !matches!(
        invoke.tool().map(|t| t.value.as_str()),
        Some("nika:write" | "nika:edit")
    ) {
        return None;
    }
    let path = invoke.args.as_ref()?.value.get("path")?.as_str()?;
    if !path.contains("${{") {
        return Some(path.to_owned());
    }
    if let Some(lit) = crate::static_literal_of(wf, path).and_then(|v| v.as_str()) {
        return Some(lit.to_owned());
    }
    crate::walk::bare_static_ref(path)
        .map(|(authority, name)| format!("${{{{ {authority}{name} }}}}"))
}

/// The LEXICAL normal form two write paths are compared under (H5 ·
/// F-P15) — component-wise, pure text, no filesystem access:
///
/// - duplicate separators collapse (`out//x.md` ≡ `out/x.md`) and a
///   trailing separator drops;
/// - `.` components drop (`./out/x.md` ≡ `out/x.md`);
/// - `..` pops the last kept component (`out/d/../x.md` ≡ `out/x.md`) —
///   except a `..` with nothing to pop, which is KEPT verbatim (a
///   relative path's escape stays its own: `../x.md` ≡ only `../x.md`),
///   as is one past an absolute root (`/..` ≡ `/`).
///
/// The claim is LEXICAL, not filesystem: two spellings sharing a normal
/// form name one file UNLESS a `..` crossed a symlinked directory (the
/// kernel resolves `..` against the link target's parent, not the
/// spelled one) or the filesystem folds case. The error direction is
/// the safe one — a symlink false positive merely orders two writers
/// that might have differed; the spelling false negative this fixes
/// raced two writers on one file.
fn normalize_lexical(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut kept: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => match kept.last() {
                Some(&last) if last != ".." => {
                    kept.pop();
                }
                // Absolute: the root absorbs the `..`. Relative: the
                // escape is the path's own — kept.
                _ if absolute => {}
                _ => kept.push(".."),
            },
            c => kept.push(c),
        }
    }
    let joined = kept.join("/");
    match (absolute, joined.is_empty()) {
        (true, true) => "/".to_owned(),
        (true, false) => format!("/{joined}"),
        // The empty and all-dot spellings share one normal form.
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    }
}

/// Write-write races, statically (F-P15 · NEP-0014 law 1): two tasks
/// that CAN run concurrently (incomparable in the closure) and both
/// write the same STATIC key ([`static_write_key`] — a literal, a
/// resolved bare ref, or two identical bare immutable refs) under
/// lexical normalization — plus the fan-out flavor
/// ([`scan_for_each_fans`]). The LAW: each conflict is a finding (an
/// ordering edge — `after:` / `with:` — discharges it; parallelism is
/// safe exactly where the writes are provably disjoint).
fn scan_parallel_writers(wf: &RawWorkflow, desc: &[Vec<u64>]) -> Vec<WriteConflict> {
    let mut conflicts = scan_for_each_fans(wf);
    // (task index · the key as spelled · its lexical normal form).
    let writers: Vec<(usize, String, String)> = wf
        .tasks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            static_write_key(wf, &t.value).map(|p| {
                let n = normalize_lexical(&p);
                (i, p, n)
            })
        })
        .collect();

    let comparable = |a: usize, b: usize| -> bool {
        desc[a][b / 64] & (1u64 << (b % 64)) != 0 || desc[b][a / 64] & (1u64 << (a % 64)) != 0
    };

    for (a, (ai, ap, an)) in writers.iter().enumerate() {
        for (bi, bp, bn) in writers.iter().skip(a + 1) {
            if an != bn || comparable(*ai, *bi) {
                continue;
            }
            let (first, second) = (&wf.tasks[*ai].value.id.value, &wf.tasks[*bi].value.id.value);
            // Both literals are named when the spellings differ — the
            // collision is exactly that they reduce to one file.
            let detail = if ap == bp {
                format!(
                    "`{first}` and `{second}` can run CONCURRENTLY and both write \
                     `{ap}` — last-writer-wins is a race the file never declares"
                )
            } else {
                format!(
                    "`{first}` and `{second}` can run CONCURRENTLY and write the SAME file \
                     spelled `{ap}` and `{bp}` — last-writer-wins is a race the file never \
                     declares"
                )
            };
            conflicts.push(WriteConflict {
                task: first.clone(),
                other: Some(second.clone()),
                path: an.clone(),
                detail,
                fix: "order them with `after:` (one writer after the other) · or \
                      merge the writes into one task"
                    .to_owned(),
            });
        }
    }
    conflicts
}

/// The `for_each` flavor, closure-free: a fan writing one constant path
/// races its OWN iterations — no DAG read is needed, so this judgment
/// runs even when the closure-based pair scan is capped out (H6 · every
/// iteration overwrites the same file, the last silently wins).
fn scan_for_each_fans(wf: &RawWorkflow) -> Vec<WriteConflict> {
    let mut conflicts = Vec::new();
    for t in &wf.tasks {
        if t.value.for_each.is_some()
            && let Some(path) = static_write_key(wf, &t.value)
        {
            let task = t.value.id.value.clone();
            conflicts.push(WriteConflict {
                task: task.clone(),
                other: None,
                path: path.clone(),
                detail: format!(
                    "every `for_each` iteration of `{task}` writes the SAME \
                     path `{path}` — the last iteration silently wins"
                ),
                fix: "derive the path from `${{ item }}` so each iteration writes \
                      its own file · or drop `for_each`"
                    .to_owned(),
            });
        }
    }
    conflicts
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn read(yaml: &str) -> DagRead {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let analyzed = crate::analyzer::analyze(&wf).expect("conformant");
        read_dag(&wf, &analyzed.topo_waves)
    }

    fn analysis(yaml: &str) -> DagAnalysis {
        read(yaml).analysis.expect("analysis present")
    }

    const HEADER: &str = "nika: t\n\nmodel: mock/echo\n\ntasks:\n";

    fn infer_task(id: &str, deps: &[&str]) -> String {
        let dep_line = if deps.is_empty() {
            String::new()
        } else {
            let entries: Vec<String> = deps.iter().map(|d| format!("{d}: success")).collect();
            format!("    after: {{ {} }}\n", entries.join(", "))
        };
        format!("  {id}:\n{dep_line}    infer:\n      prompt: \"x\"\n")
    }

    fn wf(tasks: &[(&str, &[&str])]) -> String {
        let mut out = HEADER.to_owned();
        for (id, deps) in tasks {
            out.push_str(&infer_task(id, deps));
        }
        out
    }

    #[test]
    fn chain_has_width_one() {
        let a = analysis(&wf(&[("a", &[]), ("b", &["a"]), ("c", &["b"])]));
        assert_eq!(a.width, 1);
        assert_eq!(a.width_witness.len(), 1);
        // A total order is pinch everywhere.
        assert_eq!(a.pinch_points, vec!["a", "b", "c"]);
    }

    #[test]
    fn diamond_width_two_with_branch_witness() {
        let a = analysis(&wf(&[
            ("a", &[]),
            ("b", &["a"]),
            ("c", &["a"]),
            ("d", &["b", "c"]),
        ]));
        assert_eq!(a.width, 2);
        let mut witness = a.width_witness.clone();
        witness.sort();
        assert_eq!(witness, vec!["b", "c"]);
        assert_eq!(a.pinch_points, vec!["a", "d"]);
        // Blast: a blocks 3, b and c block 1 each, d blocks none.
        assert_eq!(a.blast_radius[0].task, "a");
        assert_eq!(a.blast_radius[0].blocks, 3);
        assert!(a.blast_radius.iter().all(|b| b.task != "d"));
    }

    #[test]
    fn width_exceeds_the_largest_wave() {
        // p→a1→x2 · p→x1 · isolated x0: Kahn waves peak at 2 ({p,x0} ·
        // {a1,x1} · {x2}) but {x0, x1, x2} is a 3-antichain. THE reason
        // « max parallelism = max wave » under-reports.
        let yaml = wf(&[
            ("p", &[]),
            ("x0", &[]),
            ("a1", &["p"]),
            ("x1", &["p"]),
            ("x2", &["a1"]),
        ]);
        let wf_parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let analyzed = crate::analyzer::analyze(&wf_parsed).expect("conformant");
        let max_wave = analyzed.topo_waves.iter().map(Vec::len).max().unwrap_or(0);
        assert_eq!(max_wave, 2);

        let a = read_dag(&wf_parsed, &analyzed.topo_waves)
            .analysis
            .expect("analysis");
        assert_eq!(a.width, 3);
        let mut witness = a.width_witness.clone();
        witness.sort();
        assert_eq!(witness, vec!["x0", "x1", "x2"]);
    }

    #[test]
    fn witness_is_a_true_antichain() {
        // Property on a mixed fixture: no witness member descends from
        // another (checked against an independently-built closure).
        let yaml = wf(&[
            ("a", &[]),
            ("b", &["a"]),
            ("c", &["a"]),
            ("d", &["b"]),
            ("e", &[]),
            ("f", &["e", "c"]),
        ]);
        let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let analyzed = crate::analyzer::analyze(&parsed).expect("conformant");
        let a = read_dag(&parsed, &analyzed.topo_waves)
            .analysis
            .expect("analysis");

        let down = downstream_adjacency(&parsed.tasks);
        let desc = descendant_closure(parsed.tasks.len(), &down, &analyzed.topo_waves);
        let idx: BTreeMap<&str, usize> = parsed
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.value.id.value.as_str(), i))
            .collect();
        for u in &a.width_witness {
            for v in &a.width_witness {
                if u == v {
                    continue;
                }
                let (ui, vi) = (idx[u.as_str()], idx[v.as_str()]);
                assert_eq!(
                    desc[ui][vi / 64] & (1u64 << (vi % 64)),
                    0,
                    "{u} ≺ {v} inside the witness"
                );
            }
        }
        // And the witness size IS the width.
        assert_eq!(a.width_witness.len(), a.width);
    }

    #[test]
    fn width_bounds_every_wave() {
        let yaml = wf(&[
            ("a", &[]),
            ("b", &[]),
            ("c", &["a", "b"]),
            ("d", &["c"]),
            ("e", &["c"]),
            ("f", &["d", "e"]),
        ]);
        let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let analyzed = crate::analyzer::analyze(&parsed).expect("conformant");
        let a = read_dag(&parsed, &analyzed.topo_waves)
            .analysis
            .expect("analysis");
        for wave in &analyzed.topo_waves {
            assert!(a.width >= wave.len());
        }
    }

    #[test]
    fn single_task_has_no_pinch_noise() {
        let a = analysis(&wf(&[("only", &[])]));
        assert_eq!(a.width, 1);
        assert!(a.pinch_points.is_empty());
        assert!(a.blast_radius.is_empty());
    }

    #[test]
    fn parallel_writers_same_literal_path_is_a_finding() {
        // F-P15 · the law: two incomparable tasks writing one literal
        // path is a REFUSAL-shaped conflict (never an advisory hint).
        let yaml = format!(
            "{HEADER}  left:\n    invoke:\n      tool: nika:write\n      args:\n        path: out/report.md\n        content: \"a\"\n  right:\n    invoke:\n      tool: nika:write\n      args:\n        path: out/report.md\n        content: \"b\"\n"
        );
        let conflicts = read(&yaml).conflicts;
        assert_eq!(conflicts.len(), 1);
        let c = &conflicts[0];
        assert_eq!(c.task, "left");
        assert_eq!(c.other.as_deref(), Some("right"));
        assert_eq!(c.path, "out/report.md");
        assert_eq!(c.wire_code(), "NIKA-SEC-012");
        assert!(c.detail.contains("left") && c.detail.contains("right"));
    }

    #[test]
    fn ordered_writers_are_not_a_race() {
        let yaml = format!(
            "{HEADER}  first:\n    invoke:\n      tool: nika:write\n      args:\n        path: out/report.md\n        content: \"a\"\n  second:\n    after: {{ first: success }}\n    invoke:\n      tool: nika:write\n      args:\n        path: out/report.md\n        content: \"b\"\n"
        );
        assert!(read(&yaml).conflicts.is_empty());
    }

    #[test]
    fn distinct_or_dynamic_paths_make_no_claim() {
        let yaml = "nika: t\n\nmodel: mock/echo\n\nconst:\n  name: report\n\ntasks:\n  a:\n    invoke:\n      tool: nika:write\n      args:\n        path: out/a.md\n        content: \"a\"\n  b:\n    invoke:\n      tool: nika:write\n      args:\n        path: \"out/${{ const.name }}.md\"\n        content: \"b\"\n";
        assert!(read(yaml).conflicts.is_empty());
    }

    #[test]
    fn for_each_over_a_constant_path_is_a_finding() {
        // F-P15 · the fan flavor: every iteration overwrites the same
        // file — the task races its own fan-out (`other` is None).
        let yaml = format!(
            "{HEADER}  fan:\n    for_each: {{ items: [1, 2, 3] }}\n    invoke:\n      tool: nika:write\n      args:\n        path: out/same.md\n        content: \"x\"\n"
        );
        let conflicts = read(&yaml).conflicts;
        assert_eq!(conflicts.len(), 1);
        let c = &conflicts[0];
        assert_eq!(c.task, "fan");
        assert_eq!(c.other, None);
        assert_eq!(c.path, "out/same.md");
        assert!(c.detail.contains("for_each"));
        assert!(c.fix.contains("${{ item }}"));
    }

    #[test]
    fn parallel_editors_same_literal_path_is_a_finding() {
        // H4 · the canon text names BOTH writer builtins: `nika:edit`
        // takes `path:` identically, and its read-modify-write loses
        // updates to a concurrent edit exactly like `nika:write`.
        let yaml = format!(
            "{HEADER}  left:\n    invoke:\n      tool: nika:edit\n      args:\n        path: out/report.md\n        find: a\n        replace: b\n  right:\n    invoke:\n      tool: nika:edit\n      args:\n        path: out/report.md\n        find: c\n        replace: d\n"
        );
        let conflicts = read(&yaml).conflicts;
        assert_eq!(conflicts.len(), 1);
        let c = &conflicts[0];
        assert_eq!(c.task, "left");
        assert_eq!(c.other.as_deref(), Some("right"));
        assert_eq!(c.path, "out/report.md");
        assert_eq!(c.wire_code(), "NIKA-SEC-012");
    }

    #[test]
    fn a_write_and_an_edit_on_one_path_race() {
        // H4 · the mixed pair: the write's atomic rename and the edit's
        // read-modify-write have no order — one of them loses.
        let yaml = format!(
            "{HEADER}  left:\n    invoke:\n      tool: nika:write\n      args:\n        path: out/report.md\n        content: \"a\"\n  right:\n    invoke:\n      tool: nika:edit\n      args:\n        path: out/report.md\n        find: a\n        replace: b\n"
        );
        let conflicts = read(&yaml).conflicts;
        assert_eq!(conflicts.len(), 1, "write+edit on one path refuses");
        assert_eq!(conflicts[0].wire_code(), "NIKA-SEC-012");
    }

    #[test]
    fn path_spellings_collide_after_lexical_normalization() {
        // H5 · the raw-string compare let `./out/x.md` vs `out/x.md` vs
        // `out//x.md` vs `out/d/../x.md` evade the law — every spelling
        // names the same file and must refuse against the plain form.
        for (a, b) in [
            ("./out/x.md", "out/x.md"),
            ("out//x.md", "out/x.md"),
            ("out/d/../x.md", "out/x.md"),
        ] {
            let yaml = format!(
                "{HEADER}  left:\n    invoke:\n      tool: nika:write\n      args:\n        path: {a}\n        content: \"a\"\n  right:\n    invoke:\n      tool: nika:write\n      args:\n        path: {b}\n        content: \"b\"\n"
            );
            let conflicts = read(&yaml).conflicts;
            assert_eq!(
                conflicts.len(),
                1,
                "`{a}` and `{b}` name one file — the race must refuse"
            );
            let c = &conflicts[0];
            assert_eq!(c.path, "out/x.md", "the normal form rides: {a} vs {b}");
            assert!(
                c.detail.contains(a) && c.detail.contains(b),
                "both spellings named: {}",
                c.detail
            );
        }
    }

    #[test]
    fn normalize_lexical_is_component_wise_and_pure() {
        // H5 · the normal form's edges: dot components drop, duplicate
        // separators collapse, `..` pops lexically — and an escape past
        // the root is KEPT (lexically, nothing proves what it pops).
        assert_eq!(normalize_lexical("./out/x.md"), "out/x.md");
        assert_eq!(normalize_lexical("out//x.md"), "out/x.md");
        assert_eq!(normalize_lexical("out/./x.md"), "out/x.md");
        assert_eq!(normalize_lexical("out/d/../x.md"), "out/x.md");
        assert_eq!(normalize_lexical("out/x.md"), "out/x.md");
        assert_eq!(normalize_lexical("../x.md"), "../x.md");
        assert_eq!(normalize_lexical("a/../../x.md"), "../x.md");
        assert_eq!(normalize_lexical("/a//b/../c"), "/a/c");
        assert_eq!(normalize_lexical(""), ".");
        // Distinct files stay distinct (no merge-happy normalization).
        assert_ne!(normalize_lexical("out/x.md"), normalize_lexical("out/y.md"));
    }

    #[test]
    fn over_the_cap_the_miss_is_stated_and_the_fan_flavor_still_judges() {
        // H6 · the cap used to judge NOTHING silently. Now the skip is
        // a STATED MISS the report carries, and the closure-free
        // `for_each` same-path flavor — which needs no closure — still
        // refuses above the cap.
        let mut yaml = String::from(HEADER);
        yaml.push_str("  fan:\n    for_each: { items: [1, 2] }\n    invoke:\n      tool: nika:write\n      args:\n        path: out/same.md\n        content: \"x\"\n");
        for i in 0..ANALYSIS_TASK_CAP {
            yaml.push_str(&infer_task(&format!("t{i}"), &[]));
        }
        let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        assert_eq!(parsed.tasks.len(), ANALYSIS_TASK_CAP + 1);
        let analyzed = crate::analyzer::analyze(&parsed).expect("conformant");
        let read = read_dag(&parsed, &analyzed.topo_waves);
        assert!(
            read.analysis.is_none(),
            "over the cap the exact read claims nothing"
        );
        let miss = read
            .stated_miss
            .as_deref()
            .expect("the skip is STATED, never silent");
        assert!(
            miss.contains("the pair scan did not run"),
            "the miss names what did not judge: {miss}"
        );
        assert_eq!(read.conflicts.len(), 1, "the fan flavor needs no closure");
        assert_eq!(read.conflicts[0].task, "fan");
        assert_eq!(read.conflicts[0].other, None);
    }

    #[test]
    fn the_stated_miss_rides_the_report_hints() {
        // H6 · through `check()`: the miss lands as an `analysis` hint
        // (JSON `hints[]` + the console HINTS section carry it), and the
        // fan flavor's refusal lands as the law's finding.
        let mut yaml = String::from(HEADER);
        yaml.push_str("  fan:\n    for_each: { items: [1, 2] }\n    invoke:\n      tool: nika:write\n      args:\n        path: out/same.md\n        content: \"x\"\n");
        for i in 0..ANALYSIS_TASK_CAP {
            yaml.push_str(&infer_task(&format!("t{i}"), &[]));
        }
        let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let report = crate::check(&parsed);
        let hint = report
            .hints
            .iter()
            .find(|h| h.kind == "analysis")
            .expect("the stated miss rides hints[]");
        assert!(
            hint.advice.contains("the pair scan did not run"),
            "the hint names the miss: {}",
            hint.advice
        );
        assert_eq!(report.write_conflicts.len(), 1);
        assert_eq!(report.write_conflicts[0].task, "fan");
    }

    #[test]
    fn identical_immutable_ref_writers_are_a_finding() {
        // Probe 2026-07-30: two unordered writers on the IDENTICAL
        // `${{ inputs.f }}` rendered green while the literal twin was
        // refused — inputs bind once per run, so the two provably
        // target the same file even though its value is unknown.
        let yaml = "nika: t\n\nmodel: mock/echo\n\ninputs:\n  f: { type: string, required: true }\n\ntasks:\n  a:\n    invoke:\n      tool: nika:write\n      args:\n        path: \"${{ inputs.f }}\"\n        content: \"a\"\n  b:\n    invoke:\n      tool: nika:write\n      args:\n        path: \"${{ inputs.f }}\"\n        content: \"b\"\n";
        let conflicts = read(yaml).conflicts;
        assert_eq!(conflicts.len(), 1, "{conflicts:?}");
        let c = &conflicts[0];
        assert_eq!(c.path, "${{ inputs.f }}", "the canonical ref IS the key");
        assert_eq!(c.wire_code(), "NIKA-SEC-012");
    }

    #[test]
    fn a_literal_writer_and_a_resolved_ref_writer_collide() {
        // The shared resolver arm: `${{ const.p }}` declares the SAME
        // literal another task writes directly — one path, two spellings.
        let yaml = "nika: t\n\nmodel: mock/echo\n\nconst:\n  p: out/report.md\n\ntasks:\n  a:\n    invoke:\n      tool: nika:write\n      args:\n        path: out/report.md\n        content: \"a\"\n  b:\n    invoke:\n      tool: nika:write\n      args:\n        path: \"${{ const.p }}\"\n        content: \"b\"\n";
        let conflicts = read(yaml).conflicts;
        assert_eq!(conflicts.len(), 1, "{conflicts:?}");
        assert_eq!(
            conflicts[0].path, "out/report.md",
            "resolved to the literal"
        );
    }

    #[test]
    fn distinct_immutable_refs_make_no_claim() {
        // Two DIFFERENT bare refs may or may not collide at run — the
        // scan never guesses.
        let yaml = "nika: t\n\nmodel: mock/echo\n\ninputs:\n  f: { type: string, required: true }\n  g: { type: string, required: true }\n\ntasks:\n  a:\n    invoke:\n      tool: nika:write\n      args:\n        path: \"${{ inputs.f }}\"\n        content: \"a\"\n  b:\n    invoke:\n      tool: nika:write\n      args:\n        path: \"${{ inputs.g }}\"\n        content: \"b\"\n";
        assert!(read(yaml).conflicts.is_empty());
    }

    #[test]
    fn a_for_each_fan_over_one_immutable_ref_races_itself() {
        // The fan flavor reaches the ref class too: every iteration
        // writes `${{ inputs.f }}` — one file, N writers.
        let yaml = "nika: t\n\nmodel: mock/echo\n\ninputs:\n  f: { type: string, required: true }\n\ntasks:\n  fan:\n    for_each: { items: [1, 2] }\n    invoke:\n      tool: nika:write\n      args:\n        path: \"${{ inputs.f }}\"\n        content: \"x\"\n";
        let conflicts = read(yaml).conflicts;
        assert_eq!(conflicts.len(), 1, "{conflicts:?}");
        assert_eq!(conflicts[0].other, None);
        assert_eq!(conflicts[0].path, "${{ inputs.f }}");
    }

    #[test]
    fn empty_workflow_reads_as_width_zero() {
        let parsed = parse(
            "nika: t\n\nmodel: mock/echo\n\ntasks: []\n",
            FileId::new(0),
            ParseMode::Strict,
        );
        // An empty tasks list may be rejected upstream; when it parses,
        // the read stays total.
        if let Ok(wf) = parsed
            && let Ok(analyzed) = crate::analyzer::analyze(&wf)
        {
            let a = read_dag(&wf, &analyzed.topo_waves).analysis.expect("total");
            assert_eq!(a.width, 0);
        }
    }

    #[test]
    fn hk_matches_a_perfect_bipartite() {
        // u0→{v0,v1} · u1→{v0}: the augmenting path frees v0 for u1.
        let adj = vec![vec![0, 1], vec![0]];
        let (ml, _, size) = hopcroft_karp(2, &adj);
        assert_eq!(size, 2);
        assert_eq!(ml[1], Some(0));
        assert_eq!(ml[0], Some(1));
    }

    #[test]
    fn hk_staircase_needs_long_augmenting_paths() {
        // The staircase u_i → {v_i, v_{i+1}}-shape forces alternating
        // augmenting paths — the layered (free_layer-gated) phases must
        // still reach the maximum matching, not stall on greedy picks.
        let k = 40usize;
        let mut adj = vec![Vec::new(); k];
        for (u, row) in adj.iter_mut().enumerate() {
            row.push(u);
            if u + 1 < k {
                row.push(u + 1);
            }
        }
        let (_, _, size) = hopcroft_karp(k, &adj);
        assert_eq!(size, k); // perfect on this gadget
    }

    #[test]
    fn oversized_workflows_skip_the_exact_read_honestly() {
        // One task above the cap: the read is skipped (None · no claim)
        // instead of materializing an O(n²) closure — never slow, never
        // wrong. H6: the skip is no longer silent either — the read
        // carries its STATED MISS (no fan in this fixture, so the
        // closure-free flavor finds nothing).
        let mut yaml = String::from(HEADER);
        yaml.push_str(&infer_task("t0", &[]));
        for i in 1..=ANALYSIS_TASK_CAP {
            let prev = format!("t{}", i - 1);
            yaml.push_str(&infer_task(&format!("t{i}"), &[prev.as_str()]));
        }
        let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let analyzed = crate::analyzer::analyze(&parsed).expect("conformant");
        let read = read_dag(&parsed, &analyzed.topo_waves);
        assert!(
            read.analysis.is_none(),
            "above the cap the read claims nothing"
        );
        assert!(read.conflicts.is_empty());
        assert!(
            read.stated_miss.is_some(),
            "above the cap the skip is STATED, never silent"
        );
    }

    #[test]
    fn exactly_at_the_cap_still_reads() {
        // Boundary opposite to the oversized case: a workflow with EXACTLY
        // ANALYSIS_TASK_CAP tasks is the last size the read still honours
        // (`n > CAP` is strict). One fewer task than the skip threshold —
        // the analysis must be present, not skipped.
        let mut yaml = String::from(HEADER);
        yaml.push_str(&infer_task("t0", &[]));
        for i in 1..ANALYSIS_TASK_CAP {
            let prev = format!("t{}", i - 1);
            yaml.push_str(&infer_task(&format!("t{i}"), &[prev.as_str()]));
        }
        let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        assert_eq!(parsed.tasks.len(), ANALYSIS_TASK_CAP);
        let analyzed = crate::analyzer::analyze(&parsed).expect("conformant");
        let read = read_dag(&parsed, &analyzed.topo_waves);
        assert!(
            read.analysis.is_some(),
            "exactly at the cap the read still claims"
        );
    }

    #[test]
    fn two_task_chain_pinches_both() {
        // The pinch guard is `n < 2`: a 2-task chain (n == 2) is the first
        // size that computes pinch points. a→b makes both nodes articulation
        // points of the order. A `n <= 2` guard would wrongly empty this.
        let a = analysis(&wf(&[("a", &[]), ("b", &["a"])]));
        assert_eq!(a.pinch_points, vec!["a", "b"]);
    }
}
