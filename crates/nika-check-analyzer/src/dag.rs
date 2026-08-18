// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG topology rules — spec `03-dag.md` §the four graphs (W2).
//!
//! Every rule runs over the DERIVED edges (`analyzer::edges` · the one
//! computation) — never a private AST re-walk:
//!
//! - `NIKA-DAG-002` · every `with:` reference and `after:` target
//!   resolves to a declared id (an edge to nowhere).
//! - `NIKA-DAG-001` · `G_p` = `E_d` ∪ `E_c` MUST be acyclic (incl. 1-cycles ·
//!   self-edges through `after:` or a self-referencing binding).
//! - `NIKA-DAG-004` · `on_error.recover` must not reference a task that
//!   transitively depends on the declaring task through `G_p` — the
//!   recovery-time await would deadlock (spec `05-errors.md` §recover
//!   resolution · the recovery surface is exempt from EDGES, not from
//!   acyclicity).
//! - Topological waves · « group tasks into parallel execution waves »
//!   over `G_p` (roles never change precedence · only admission).

use std::collections::BTreeMap;

use super::edges::{Edge, task_refs_in_value};
use nika_schema::error::SchemaError;
use nika_schema::raw::RawTask;
use nika_schema::source::Spanned;
use nika_schema::types::OnErrorAction;

/// Above this many candidate ids, unresolved-edge findings stop carrying a
/// did-you-mean (the O(n²·L²) wall — see the budget note in the function).
/// Real workflows sit far under it; only adversarial generators cross it.
const MAX_SUGGEST_CANDIDATES: usize = 256;

/// `NIKA-DAG-002` — collect unresolved edge targets (`after:` entries
/// and `with:` references naming a task that does not exist).
pub(super) fn check_edge_targets_resolve(
    tasks: &[Spanned<RawTask>],
    ids: &BTreeMap<String, usize>,
    errors: &mut Vec<SchemaError>,
) {
    // THE SUGGESTION BUDGET (Gate-11 security finding F2). did_you_mean runs
    // Damerau-Levenshtein against EVERY task id; n unknown refs × n ids is
    // O(n²·L²), and MAX_TASKS is 10,000 — measured 28s of synchronous CPU
    // native, ~80s in wasm, inside the declared caps. The suggestion is a
    // NICETY, never the verdict: past the budget the finding still fires,
    // it just stops guessing. 256 ids × a typo'd ref stays instant; a
    // 10,000-task adversarial file stops being a CPU bomb.
    let suggest: &dyn Fn(&str) -> Option<String> = if ids.len() <= MAX_SUGGEST_CANDIDATES {
        &|target| {
            nika_types::suggest::did_you_mean(target, ids.keys().map(String::as_str))
                .map(str::to_owned)
        }
    } else {
        &|_| None
    };
    for task in tasks {
        for (target, _pred) in &task.value.after {
            if !ids.contains_key(&target.value) {
                errors.push(SchemaError::UnknownDependency {
                    from: task.value.id.value.clone(),
                    to: target.value.clone(),
                    suggestion: suggest(&target.value),
                    span: Some(target.span),
                });
            }
        }
        for (key, value) in &task.value.with {
            let mut refs = Vec::new();
            task_refs_in_value(&value.value, &mut refs);
            refs.sort();
            refs.dedup();
            for (id, _field) in refs {
                // A bare `${{ tasks }}` yields an EMPTY id · it names no
                // dependency, it names the envelope, and that class has
                // its own code (`NIKA-VAR-020` · BareTaskEnvelope, from
                // the scanner). Reporting DAG-002 here would accuse the
                // author of a typo'd task named `` and bury the real
                // teaching under it.
                if id.is_empty() {
                    continue;
                }
                if !ids.contains_key(&id) {
                    errors.push(SchemaError::UnknownDependency {
                        from: task.value.id.value.clone(),
                        to: id.clone(),
                        suggestion: suggest(&id),
                        span: Some(value.span),
                    });
                }
                let _ = key;
            }
        }
    }
}

/// `NIKA-DAG-001` — tricolor DFS cycle detection over the derived
/// `G_p` = `E_d` ∪ `E_c` · reports the cycle path (`a → b → a`).
pub(super) fn check_cycles(
    tasks: &[Spanned<RawTask>],
    _ids: &BTreeMap<String, usize>,
    edges: &[Edge],
    errors: &mut Vec<SchemaError>,
) {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    // consumer → its producers (the direction the gate awaits).
    // `G_p` ONLY: an `unwind` edge is the E_f attachment and never
    // participates in cycle detection (spec 03 · a cleanup task folding
    // back on its producer is not a cycle, it is the whole construct).
    let mut producers: Vec<Vec<usize>> = vec![Vec::new(); tasks.len()];
    for e in edges.iter().filter(|e| e.kind.is_scheduling()) {
        producers[e.to].push(e.from);
    }

    let mut color = vec![Color::White; tasks.len()];
    // Iterative DFS with an explicit stack of (node, next-edge-index)
    // — no recursion depth limit on adversarial inputs.
    for start in 0..tasks.len() {
        if color[start] != Color::White {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        color[start] = Color::Gray;
        while let Some(&mut (node, ref mut edge)) = stack.last_mut() {
            let deps = &producers[node];
            if *edge >= deps.len() {
                color[node] = Color::Black;
                stack.pop();
                continue;
            }
            let next = deps[*edge];
            *edge += 1;
            match color[next] {
                Color::White => {
                    color[next] = Color::Gray;
                    stack.push((next, 0));
                }
                Color::Gray => {
                    // Back-edge → cycle. Reconstruct the path from the
                    // gray stack segment starting at `next`.
                    let mut cycle: Vec<String> = Vec::new();
                    let from = stack
                        .iter()
                        .position(|&(n, _)| n == next)
                        .unwrap_or_default();
                    for &(n, _) in &stack[from..] {
                        cycle.push(tasks[n].value.id.value.clone());
                    }
                    cycle.push(tasks[next].value.id.value.clone());
                    errors.push(SchemaError::Cycle { cycle });
                    return; // one cycle report is enough — fix + re-run
                }
                Color::Black => {}
            }
        }
    }
}

/// `NIKA-DAG-004` — an `on_error.recover` reference to a task that
/// transitively depends on the declaring task through `G_p` (spec
/// `05-errors.md` §recover resolution · the await would deadlock ·
/// runs after DAG-001/002 so the walk is over a sane graph).
pub(super) fn check_recover_acyclic(
    tasks: &[Spanned<RawTask>],
    ids: &BTreeMap<String, usize>,
    edges: &[Edge],
    errors: &mut Vec<SchemaError>,
) {
    // consumer → producers over `G_p` (E_f excluded · spec 03)
    let mut producers: Vec<Vec<usize>> = vec![Vec::new(); tasks.len()];
    for e in edges.iter().filter(|e| e.kind.is_scheduling()) {
        producers[e.to].push(e.from);
    }

    for (declaring_ix, task) in tasks.iter().enumerate() {
        let Some(on_error) = &task.value.on_error else {
            continue;
        };
        let OnErrorAction::Recover(value) = &on_error.value.action else {
            continue;
        };
        let declaring = task.value.id.value.as_str();
        let mut refs = Vec::new();
        task_refs_in_value(&value.value, &mut refs);
        let mut targets: Vec<String> = refs.into_iter().map(|(id, _)| id).collect();
        targets.sort();
        targets.dedup();
        for target in targets {
            let Some(&start) = ids.get(&target) else {
                continue; // unresolved · the scan layer reports it
            };
            if reaches(&producers, start, declaring_ix) {
                errors.push(SchemaError::RecoverAwaitDeadlock {
                    task: declaring.to_owned(),
                    target,
                    span: Some(value.span),
                });
            }
        }
    }
}

/// Whether `start`'s transitive producer closure over `G_p` contains
/// `needle` (i.e. `start` depends — transitively — on `needle`).
fn reaches(producers: &[Vec<usize>], start: usize, needle: usize) -> bool {
    let mut seen = vec![false; producers.len()];
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        if seen[node] {
            continue;
        }
        seen[node] = true;
        for &p in &producers[node] {
            if p == needle {
                return true;
            }
            stack.push(p);
        }
    }
    false
}

/// Topological waves (Kahn levels) over `G_p` — wave N may run in
/// parallel once wave N-1 completed (spec `03-dag.md` §execution
/// model). Roles never change precedence — every edge orders; the
/// pass-sets only gate admission.
///
/// Only meaningful on an acyclic graph with resolved targets — callers
/// invoke it after DAG-001/002 pass.
pub(super) fn topo_waves(task_count: usize, edges: &[Edge]) -> Vec<Vec<usize>> {
    let mut indegree = vec![0_usize; task_count];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); task_count];
    // Parallel edges between the same pair count once for precedence.
    let mut seen = std::collections::BTreeSet::new();
    // `unwind` edges do not enter wave assignment — the cleanup runs off
    // the producer's settle, not off a wave (spec 03).
    for e in edges.iter().filter(|e| e.kind.is_scheduling()) {
        if seen.insert((e.from, e.to)) {
            indegree[e.to] += 1;
            dependents[e.from].push(e.to);
        }
    }
    // …and neither do the cleanup TASKS themselves. Excluding only the
    // EDGE leaves the task at indegree 0, so it lands in wave 0 and the
    // gate cancels it — the producer has no record yet (measured at the
    // binary: `⊘ cleanup · gate: an edge did not admit`). A task is an
    // unwind task exactly when something unwinds INTO it, which the edge
    // set already says — no extra parameter needed.
    let unwinds: std::collections::BTreeSet<usize> = edges
        .iter()
        .filter(|e| !e.kind.is_scheduling())
        .map(|e| e.to)
        .collect();
    let mut waves = Vec::new();
    let mut current: Vec<usize> = (0..task_count)
        .filter(|&i| indegree[i] == 0 && !unwinds.contains(&i))
        .collect();
    while !current.is_empty() {
        let mut next = Vec::new();
        for &node in &current {
            for &dependent in &dependents[node] {
                indegree[dependent] -= 1;
                // an unwind task never enters a wave, even when it also
                // carries a value edge from the producer it cleans up
                if indegree[dependent] == 0 && !unwinds.contains(&dependent) {
                    next.push(dependent);
                }
            }
        }
        waves.push(std::mem::take(&mut current));
        current = next;
    }
    waves
}

#[cfg(test)]
mod tests {
    use crate::analyze;
    use nika_schema::error::SchemaError;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn analyze_yaml(yaml: &str) -> Result<crate::AnalyzedWorkflow, Vec<SchemaError>> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        analyze(&wf)
    }

    const HEADER: &str = "nika: t\n";

    #[test]
    fn recover_deadlock_transitive_and_nested_refs() {
        // 05 §recover resolution · DAG-004 — a 2-hop transitive
        // dependent (through a value edge + a control edge) deadlocks
        // the await · refs nest anywhere in the recover JSON value.
        let yaml = format!(
            "{HEADER}tasks:
  fetch:
    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"https://x.example\" }} }}
    on_error:
      recover:
        stale: true
        body: \"${{{{ tasks.report.output }}}}\"
  mid:
    with: {{ page: \"${{{{ tasks.fetch.output }}}}\" }}
    exec: {{ command: [echo] }}
  report:
    after: {{ mid: success }}
    exec: {{ command: [echo] }}
"
        );
        let errors = analyze_yaml(&yaml).expect_err("deadlock");
        assert!(
            errors.iter().any(|e| matches!(
                e,
                SchemaError::RecoverAwaitDeadlock { task, target, .. }
                    if task == "fetch" && target == "report"
            )),
            "{errors:?}"
        );
    }

    #[test]
    fn recover_deadlock_ref_nested_in_an_array() {
        // The string walker MUST descend into a recover ARRAY to find
        // the ref (report → mid → fetch · mixed value/control chain).
        let yaml = format!(
            "{HEADER}tasks:
  fetch:
    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"https://x.example\" }} }}
    on_error:
      recover:
        - \"${{{{ tasks.report.output }}}}\"
  mid:
    after: {{ fetch: success }}
    exec: {{ command: [echo] }}
  report:
    with: {{ notes: \"${{{{ tasks.mid.output }}}}\" }}
    exec: {{ command: [echo] }}
"
        );
        let errors = analyze_yaml(&yaml).expect_err("deadlock via array-nested ref");
        assert!(
            errors.iter().any(|e| matches!(
                e,
                SchemaError::RecoverAwaitDeadlock { task, target, .. }
                    if task == "fetch" && target == "report"
            )),
            "ref nested inside the recover ARRAY must still be collected: {errors:?}"
        );
    }

    #[test]
    fn recover_independent_target_does_not_deadlock() {
        // `other` depends on `base` but NOT on `fetch` — the walk over
        // `other`'s producer closure never reaches `fetch` · legal.
        let yaml = format!(
            "{HEADER}tasks:
  base:
    exec: {{ command: [echo] }}
  other:
    after: {{ base: success }}
    exec: {{ command: [echo] }}
  fetch:
    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"https://x.example\" }} }}
    on_error:
      recover: ${{{{ tasks.other.output }}}}
"
        );
        analyze_yaml(&yaml)
            .expect("an independent recover target is NOT a transitive-dep deadlock");
    }

    #[test]
    fn recover_independent_source_is_legal() {
        // 05 §recover resolution · an independent recovery source needs
        // NO scheduling edge and passes acyclicity (the fetch-chain shape).
        let yaml = format!(
            "{HEADER}tasks:
  cached:
    invoke: {{ tool: \"nika:read\", args: {{ path: \"./cache.json\" }} }}
  fetch:
    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"https://x.example\" }} }}
    on_error:
      recover: ${{{{ tasks.cached.output }}}}
"
        );
        analyze_yaml(&yaml).expect("independent recover is legal");
    }

    #[test]
    fn cycle_two_nodes_control() {
        // Conformance fixture dag-topology/001-cycle · a after b ·
        // b after a.
        let yaml = format!(
            "{HEADER}tasks:
  a:
    after: {{ b: success }}
    exec: {{ command: [echo] }}
  b:
    after: {{ a: success }}
    exec: {{ command: [echo] }}
"
        );
        let errors = analyze_yaml(&yaml).expect_err("cycle");
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, SchemaError::Cycle { cycle } if cycle.len() >= 2)),
            "{errors:?}"
        );
    }

    #[test]
    fn cycle_mixed_value_and_control() {
        // Conformance fixture dag-topology/015 · a value edge b→a plus
        // a control edge a→b is a MIXED cycle — `G_p` is ONE graph.
        let yaml = format!(
            "{HEADER}tasks:
  a:
    with: {{ b_out: \"${{{{ tasks.b.output }}}}\" }}
    exec: {{ command: [echo] }}
  b:
    after: {{ a: success }}
    exec: {{ command: [echo] }}
"
        );
        let errors = analyze_yaml(&yaml).expect_err("mixed cycle");
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, SchemaError::Cycle { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn cycle_path_is_reconstructed_from_the_gray_segment() {
        // The cycle path is rebuilt from the gray-stack segment
        // STARTING at the back-edge target — exactly `[a, b, a]`.
        let yaml = format!(
            "{HEADER}tasks:
  a:
    after: {{ b: success }}
    exec: {{ command: [echo] }}
  b:
    after: {{ a: success }}
    exec: {{ command: [echo] }}
"
        );
        let errors = analyze_yaml(&yaml).expect_err("cycle");
        let cycle = errors
            .iter()
            .find_map(|e| match e {
                SchemaError::Cycle { cycle } => Some(cycle.clone()),
                _ => None,
            })
            .expect("a Cycle error");
        assert_eq!(
            cycle,
            vec!["a".to_owned(), "b".to_owned(), "a".to_owned()],
            "the gray-segment slice must start AT the back-edge target `a`"
        );
    }

    #[test]
    fn self_dependency_is_a_cycle() {
        // Conformance fixture dag-topology/004 · « a 1-cycle (a after
        // a) is a cycle ».
        let yaml = format!(
            "{HEADER}tasks:
  a:
    after: {{ a: success }}
    exec: {{ command: [echo] }}
"
        );
        let errors = analyze_yaml(&yaml).expect_err("self-cycle");
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, SchemaError::Cycle { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn unresolved_after_target() {
        // Conformance fixture dag-topology/002-unresolved-after-target.
        let yaml = format!(
            "{HEADER}tasks:
  a:
    after: {{ ghost: success }}
    exec: {{ command: [echo] }}
"
        );
        let errors = analyze_yaml(&yaml).expect_err("unresolved");
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, SchemaError::UnknownDependency { to, .. } if to == "ghost")),
            "{errors:?}"
        );
    }

    #[test]
    fn unresolved_with_reference_is_dag_002() {
        // The binding IS an edge — an edge to nowhere is a DAG error
        // (conformance variables/missing-task-ref-reject).
        let yaml = format!(
            "{HEADER}tasks:
  a:
    with: {{ st: \"${{{{ tasks.nonexistent.status }}}}\" }}
    exec: {{ command: [echo] }}
"
        );
        let errors = analyze_yaml(&yaml).expect_err("unresolved with-ref");
        assert!(
            errors.iter().any(
                |e| matches!(e, SchemaError::UnknownDependency { to, .. } if to == "nonexistent")
            ),
            "{errors:?}"
        );
    }

    #[test]
    fn diamond_is_valid_with_two_parallel_waves() {
        // Conformance fixture dag-topology/008 · a→b,c via control ·
        // b,c→d via value bindings.
        let yaml = format!(
            "{HEADER}tasks:
  a:
    exec: {{ command: [echo] }}
  b:
    after: {{ a: success }}
    exec: {{ command: [echo] }}
  c:
    after: {{ a: success }}
    exec: {{ command: [echo] }}
  d:
    with:
      b: \"${{{{ tasks.b.output }}}}\"
      c: \"${{{{ tasks.c.output }}}}\"
    exec: {{ command: [echo] }}
"
        );
        let analyzed = analyze_yaml(&yaml).expect("valid diamond");
        assert_eq!(analyzed.topo_waves.len(), 3);
        assert_eq!(analyzed.topo_waves[0], vec![0]); // a
        assert_eq!(analyzed.topo_waves[1], vec![1, 2]); // b ∥ c
        assert_eq!(analyzed.topo_waves[2], vec![3]); // d
        assert_eq!(analyzed.edges.len(), 4);
    }

    #[test]
    fn independent_tasks_share_wave_zero() {
        let yaml = format!(
            "{HEADER}tasks:
  x:
    exec: {{ command: [echo] }}
  y:
    exec: {{ command: [echo] }}
"
        );
        let analyzed = analyze_yaml(&yaml).expect("valid");
        assert_eq!(analyzed.topo_waves, vec![vec![0, 1]]);
    }

    #[test]
    fn depends_on_teaches_parse_024() {
        // Conformance fixture dag-topology/014-depends-on-dead — the
        // dead form is refused AT PARSE with the migration teaching.
        let yaml = format!(
            "{HEADER}tasks:
  a:
    exec: {{ command: [echo] }}
  b:
    depends_on: [a]
    exec: {{ command: [echo] }}
"
        );
        let err = parse(&yaml, FileId::new(0), ParseMode::Strict).expect_err("dead form");
        assert!(
            matches!(&err, SchemaError::W2DependsOnField { task, task_hint, .. }
                if task == "b" && task_hint == "a"),
            "{err:?}"
        );
        assert_eq!(err.spec_code().to_string(), "NIKA-PARSE-024");
    }

    #[test]
    fn unknown_after_predicate_is_dag_005() {
        // Conformance fixture dag-topology/016 · the closed set.
        let yaml = format!(
            "{HEADER}tasks:
  tests:
    exec: {{ command: [echo] }}
  deploy:
    after: {{ tests: passed }}
    exec: {{ command: [echo] }}
"
        );
        let err = parse(&yaml, FileId::new(0), ParseMode::Strict).expect_err("bad predicate");
        assert!(
            matches!(&err, SchemaError::UnknownAfterPredicate { predicate, .. }
                if predicate == "passed"),
            "{err:?}"
        );
        assert_eq!(err.spec_code().to_string(), "NIKA-DAG-005");
    }
}
