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
//! - **parallel-writers conflicts** — two tasks that CAN run
//!   concurrently (incomparable) and both write the same literal
//!   `nika:write` path: last-writer-wins is a race the author almost
//!   never means. Surfaced as hints (advisory · deterministic).
//!
//! Width can EXCEED the largest wave — witness `p→a1→x2 · p→x1 ·
//! isolated x0`: waves peak at 2, width is 3 (`{x0, x1, x2}`). The
//! wave-barrier runtime leaves that parallelism unused; the report says
//! so honestly (`max parallelism` = the wave peak as executed · `width`
//! = what the DAG permits).

use std::collections::BTreeMap;

use crate::raw::{RawAction, RawTask, RawWorkflow};
use crate::source::Spanned;

use super::hints::Hint;

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
    /// Parallel-writers conflicts, as advisory hints.
    pub conflicts: Vec<Hint>,
}

impl DagRead {
    /// The skipped read (conformance failed — claim nothing).
    pub(super) fn skipped() -> Self {
        Self {
            analysis: None,
            conflicts: Vec::new(),
        }
    }
}

/// Above this task count the exact read is SKIPPED (honest, never
/// wrong — same posture as the conformance gate). The materialized
/// closure is O(n²) and the matching phases O(E√V): at the parser's
/// 10k-task cap that is ~400 MB + minutes of CPU from a few-KB YAML —
/// a `DoS` surface on a checker that markets static safety (house
/// precedent: the marked-yaml depth caps · the 64 MiB ceilings). At
/// 2 000 tasks the worst case stays ~60 MB · milliseconds typical —
/// far above any real workflow. The cap also bounds the augmenting
/// DFS recursion (≤ n frames · see [`hk_dfs`]).
const ANALYSIS_TASK_CAP: usize = 2_000;

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
        };
    }
    if n > ANALYSIS_TASK_CAP {
        return DagRead::skipped();
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

/// The literal `nika:write` target of a task, when statically known —
/// island-bearing paths are dynamic and make no static claim.
fn literal_write_path(task: &RawTask) -> Option<&str> {
    let RawAction::Invoke(invoke) = &task.action else {
        return None;
    };
    if invoke.tool.value != "nika:write" {
        return None;
    }
    invoke
        .args
        .as_ref()?
        .value
        .get("path")?
        .as_str()
        .filter(|p| !p.contains("${{"))
}

/// Write-write races, statically: two tasks that CAN run concurrently
/// (incomparable in the closure) and both write the same literal path —
/// plus the fan-out flavor (`for_each` over a constant path: every
/// iteration overwrites the same file).
fn scan_parallel_writers(wf: &RawWorkflow, desc: &[Vec<u64>]) -> Vec<Hint> {
    let writers: Vec<(usize, &str)> = wf
        .tasks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| literal_write_path(&t.value).map(|p| (i, p)))
        .collect();

    let comparable = |a: usize, b: usize| -> bool {
        desc[a][b / 64] & (1u64 << (b % 64)) != 0 || desc[b][a / 64] & (1u64 << (a % 64)) != 0
    };

    let mut hints = Vec::new();
    for t in &wf.tasks {
        if t.value.for_each.is_some()
            && let Some(path) = literal_write_path(&t.value)
        {
            hints.push(Hint {
                kind: "parallel-writers",
                task: t.value.id.value.clone(),
                advice: format!(
                    "every `for_each` iteration of `{}` writes the SAME literal \
                     path `{path}` — the last iteration silently wins; derive the \
                     path from `${{{{ item }}}}` or drop `for_each`",
                    t.value.id.value
                ),
            });
        }
    }
    for (a, &(ai, ap)) in writers.iter().enumerate() {
        for &(bi, bp) in writers.iter().skip(a + 1) {
            if ap != bp || comparable(ai, bi) {
                continue;
            }
            let (first, second) = (&wf.tasks[ai].value.id.value, &wf.tasks[bi].value.id.value);
            hints.push(Hint {
                kind: "parallel-writers",
                task: first.clone(),
                advice: format!(
                    "`{first}` and `{second}` can run CONCURRENTLY and both write \
                     `{ap}` — last-writer-wins is a race; order them with \
                     `depends_on` or merge the writes"
                ),
            });
        }
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn read(yaml: &str) -> DagRead {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let analyzed = crate::analyzer::analyze(&wf).expect("conformant");
        read_dag(&wf, &analyzed.topo_waves)
    }

    fn analysis(yaml: &str) -> DagAnalysis {
        read(yaml).analysis.expect("analysis present")
    }

    const HEADER: &str = "nika: v1\nworkflow:\n  id: t\n\nmodel: mock/echo\n\ntasks:\n";

    fn infer_task(id: &str, deps: &[&str]) -> String {
        let dep_line = if deps.is_empty() {
            String::new()
        } else {
            let entries: Vec<String> = deps.iter().map(|d| format!("{d}: succeeded")).collect();
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
    fn parallel_writers_same_literal_path_is_flagged() {
        let yaml = format!(
            "{HEADER}  left:\n    invoke:\n      tool: nika:write\n      args:\n        path: out/report.md\n        content: \"a\"\n  right:\n    invoke:\n      tool: nika:write\n      args:\n        path: out/report.md\n        content: \"b\"\n"
        );
        let hints = read(&yaml).conflicts;
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].kind, "parallel-writers");
        assert!(hints[0].advice.contains("out/report.md"));
        assert!(hints[0].advice.contains("left") && hints[0].advice.contains("right"));
    }

    #[test]
    fn ordered_writers_are_not_a_race() {
        let yaml = format!(
            "{HEADER}  first:\n    invoke:\n      tool: nika:write\n      args:\n        path: out/report.md\n        content: \"a\"\n  second:\n    after: {{ first: succeeded }}\n    invoke:\n      tool: nika:write\n      args:\n        path: out/report.md\n        content: \"b\"\n"
        );
        assert!(read(&yaml).conflicts.is_empty());
    }

    #[test]
    fn distinct_or_dynamic_paths_make_no_claim() {
        let yaml = "nika: v1\nworkflow:\n  id: t\n\nmodel: mock/echo\n\nvars:\n  name: report\n\ntasks:\n  a:\n    invoke:\n      tool: nika:write\n      args:\n        path: out/a.md\n        content: \"a\"\n  b:\n    invoke:\n      tool: nika:write\n      args:\n        path: \"out/${{ vars.name }}.md\"\n        content: \"b\"\n";
        assert!(read(yaml).conflicts.is_empty());
    }

    #[test]
    fn for_each_over_a_constant_path_is_flagged() {
        let yaml = format!(
            "{HEADER}  fan:\n    for_each: [1, 2, 3]\n    invoke:\n      tool: nika:write\n      args:\n        path: out/same.md\n        content: \"x\"\n"
        );
        let hints = read(&yaml).conflicts;
        assert_eq!(hints.len(), 1);
        assert!(hints[0].advice.contains("for_each"));
        assert!(hints[0].advice.contains("out/same.md"));
    }

    #[test]
    fn empty_workflow_reads_as_width_zero() {
        let parsed = parse(
            "nika: v1\nworkflow:\n  id: t\n\nmodel: mock/echo\n\ntasks: []\n",
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
        // wrong, same posture as the conformance gate.
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
