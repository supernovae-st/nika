// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG topology rules — spec `03-dag.md` §execution model.
//!
//! - `NIKA-DAG-002` · every `depends_on` entry resolves to a declared id.
//! - `NIKA-DAG-001` · « The engine MUST reject any workflow with cyclic
//!   dependencies at parse time with a clear error » (incl. 1-cycles ·
//!   self-dependency).
//! - `NIKA-DAG-004` · `on_error.recover` must not reference a task that
//!   transitively depends on the declaring task — the recovery-time
//!   await would deadlock (spec `05-errors.md` §recover resolution ·
//!   the 03 carve-out exempts `recover:` from EDGES, not acyclicity).
//! - Topological waves · « group tasks into parallel execution waves »
//!   (steps 2-3 of the execution model).

use std::collections::BTreeMap;

use crate::error::SchemaError;
use crate::expression::{NamespaceRef, expr_refs, scan_templates};
use crate::raw::RawTask;
use crate::source::Spanned;
use crate::types::OnErrorAction;

/// `NIKA-DAG-002` — collect unresolved `depends_on` references.
pub(super) fn check_depends_on_resolve(
    tasks: &[Spanned<RawTask>],
    ids: &BTreeMap<String, usize>,
    errors: &mut Vec<SchemaError>,
) {
    for task in tasks {
        for dep in &task.value.depends_on {
            if !ids.contains_key(&dep.value) {
                errors.push(SchemaError::UnknownDependency {
                    from: task.value.id.value.clone(),
                    to: dep.value.clone(),
                    suggestion: crate::suggest::did_you_mean(
                        &dep.value,
                        ids.keys().map(String::as_str),
                    )
                    .map(str::to_owned),
                    span: Some(dep.span),
                });
            }
        }
    }
}

/// `NIKA-DAG-001` — tricolor DFS cycle detection · reports the cycle
/// path (`a → b → a`). Unresolvable deps are skipped (DAG-002's job).
pub(super) fn check_cycles(
    tasks: &[Spanned<RawTask>],
    ids: &BTreeMap<String, usize>,
    errors: &mut Vec<SchemaError>,
) {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
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
            let deps = &tasks[node].value.depends_on;
            if *edge >= deps.len() {
                color[node] = Color::Black;
                stack.pop();
                continue;
            }
            let dep_name = &deps[*edge].value;
            *edge += 1;
            let Some(&next) = ids.get(dep_name) else {
                continue; // unresolved · DAG-002 reports it
            };
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
/// transitively depends on the declaring task (spec `05-errors.md`
/// §recover resolution · the await would deadlock · runs after
/// DAG-001/002 so the walk is over a sane graph).
pub(super) fn check_recover_acyclic(
    tasks: &[Spanned<RawTask>],
    ids: &BTreeMap<String, usize>,
    errors: &mut Vec<SchemaError>,
) {
    for task in tasks {
        let Some(on_error) = &task.value.on_error else {
            continue;
        };
        let OnErrorAction::Recover(value) = &on_error.value.action else {
            continue;
        };
        let declaring = task.value.id.value.as_str();
        for target in recover_task_refs(&value.value) {
            let Some(&start) = ids.get(&target) else {
                continue; // unresolved · the scan layer reports it
            };
            if depends_transitively_on(tasks, ids, start, declaring) {
                errors.push(SchemaError::RecoverAwaitDeadlock {
                    task: declaring.to_owned(),
                    target,
                    span: Some(value.span),
                });
            }
        }
    }
}

/// Every `tasks.<id>` referenced inside the recover value's `${{ }}`
/// islands (strings nested anywhere in the JSON value).
fn recover_task_refs(value: &serde_json::Value) -> Vec<String> {
    fn strings<'v>(v: &'v serde_json::Value, out: &mut Vec<&'v str>) {
        match v {
            serde_json::Value::String(s) => out.push(s),
            serde_json::Value::Array(items) => items.iter().for_each(|i| strings(i, out)),
            serde_json::Value::Object(map) => map.values().for_each(|i| strings(i, out)),
            _ => {}
        }
    }
    let mut raw = Vec::new();
    strings(value, &mut raw);
    let mut refs = Vec::new();
    for s in raw {
        let Ok(islands) = scan_templates(s) else {
            continue; // malformed · the scan layer reports it
        };
        for island in islands {
            for r in expr_refs(&island.expr) {
                if let NamespaceRef::Tasks { id, .. } = r {
                    refs.push(id);
                }
            }
        }
    }
    refs.sort();
    refs.dedup();
    refs
}

/// Whether `start`'s transitive `depends_on` closure contains `needle`.
fn depends_transitively_on(
    tasks: &[Spanned<RawTask>],
    ids: &BTreeMap<String, usize>,
    start: usize,
    needle: &str,
) -> bool {
    let mut seen = vec![false; tasks.len()];
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        if seen[node] {
            continue;
        }
        seen[node] = true;
        for dep in &tasks[node].value.depends_on {
            if dep.value == needle {
                return true;
            }
            if let Some(&next) = ids.get(&dep.value) {
                stack.push(next);
            }
        }
    }
    false
}

/// Topological waves (Kahn levels) — wave N may run in parallel once
/// wave N-1 completed (spec `03-dag.md` execution model steps 2-3).
///
/// Only meaningful on an acyclic graph with resolved deps — callers
/// invoke it after DAG-001/002 pass.
pub(super) fn topo_waves(
    tasks: &[Spanned<RawTask>],
    ids: &BTreeMap<String, usize>,
) -> Vec<Vec<usize>> {
    let n = tasks.len();
    let mut indegree = vec![0_usize; n];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, task) in tasks.iter().enumerate() {
        for dep in &task.value.depends_on {
            if let Some(&from) = ids.get(&dep.value) {
                indegree[i] += 1;
                dependents[from].push(i);
            }
        }
    }
    let mut waves = Vec::new();
    let mut current: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    while !current.is_empty() {
        let mut next = Vec::new();
        for &node in &current {
            for &dependent in &dependents[node] {
                indegree[dependent] -= 1;
                if indegree[dependent] == 0 {
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
    use crate::analyzer::analyze;
    use crate::error::SchemaError;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn analyze_yaml(yaml: &str) -> Result<crate::analyzer::AnalyzedWorkflow, Vec<SchemaError>> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        analyze(&wf)
    }

    const HEADER: &str = "nika: v1\nworkflow:\n  id: t\n";

    #[test]
    fn recover_deadlock_transitive_and_nested_refs() {
        // 05 §recover resolution · DAG-004 — a 2-hop transitive
        // dependent deadlocks the await · refs nest anywhere in the
        // recover JSON value.
        let yaml = format!(
            "{HEADER}tasks:
  fetch:
    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"https://x.example\" }} }}
    on_error:
      recover:
        stale: true
        body: \"${{{{ tasks.report.output }}}}\"
  mid:
    depends_on: [fetch]
    exec: {{ command: [echo] }}
  report:
    depends_on: [mid]
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
        // Kills `recover_task_refs::strings` 148 — delete the
        // `Value::Array(items)` arm. The recover value is a JSON ARRAY whose
        // element is the `${{ tasks.report.output }}` island; `report`
        // transitively depends on the declaring task `fetch` (report → mid →
        // fetch). The string walker MUST descend into the array to find the
        // ref — deleting the Array arm drops it, so no `tasks.report` is
        // collected and the deadlock goes unreported.
        let yaml = format!(
            "{HEADER}tasks:
  fetch:
    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"https://x.example\" }} }}
    on_error:
      recover:
        - \"${{{{ tasks.report.output }}}}\"
  mid:
    depends_on: [fetch]
    exec: {{ command: [echo] }}
  report:
    depends_on: [mid]
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
        // Kills `depends_transitively_on` 188 `==`→`!=`. `fetch` declares a
        // recover referencing `other`, which depends on `base` but NOT on
        // `fetch`. The transitive walk over `other`'s closure (other → base)
        // never reaches `fetch`, so the original returns FALSE — a legal
        // workflow, no error. With `==`→`!=`, the very first dep compare
        // (`"base" != "fetch"` → true) returns true immediately, fabricating
        // a spurious `RecoverAwaitDeadlock`. Asserting the workflow ANALYZES
        // CLEAN separates the two.
        let yaml = format!(
            "{HEADER}tasks:
  base:
    exec: {{ command: [echo] }}
  other:
    depends_on: [base]
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
        // 03 §carve-out · an independent recovery source needs NO edge
        // and passes acyclicity (the example-22 fetch-chain shape).
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
    fn cycle_two_nodes() {
        // Conformance fixture dag-topology/001-cycle.
        let yaml = format!(
            "{HEADER}tasks:
  a:
    depends_on: [b]
    exec: {{ command: [echo] }}
  b:
    depends_on: [a]
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
    fn cycle_path_is_reconstructed_from_the_gray_segment() {
        // Kills `check_cycles` 95 `==`→`!=` in `position(|&(n, _)| n == next)`.
        // The DFS starts at `a`, descends `a → b`, and hits the back-edge
        // `b → a`. The cycle path is rebuilt from the gray-stack segment
        // STARTING at the back-edge target (`a`): the slice `[a, b]` plus the
        // re-appended target `a` ⇒ exactly `[a, b, a]`. With `==`→`!=`,
        // `position` returns the first node ≠ `a` (index 1 = `b`), so the
        // slice becomes `[b]` and the path degrades to `[b, a]` — a different,
        // shorter, wrong reconstruction. Asserting the EXACT path separates
        // the two.
        let yaml = format!(
            "{HEADER}tasks:
  a:
    depends_on: [b]
    exec: {{ command: [echo] }}
  b:
    depends_on: [a]
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
        // Conformance fixture dag-topology/004-self-dependency · « a
        // 1-cycle (a depends_on [a]) is a cycle ».
        let yaml = format!(
            "{HEADER}tasks:
  a:
    depends_on: [a]
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
    fn unresolved_depends_on() {
        // Conformance fixture dag-topology/002-unresolved-depends-on.
        let yaml = format!(
            "{HEADER}tasks:
  a:
    depends_on: [ghost]
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
    fn diamond_is_valid_with_two_parallel_waves() {
        // Conformance fixture dag-topology/008-valid-diamond · a→b,c→d.
        let yaml = format!(
            "{HEADER}tasks:
  a:
    exec: {{ command: [echo] }}
  b:
    depends_on: [a]
    exec: {{ command: [echo] }}
  c:
    depends_on: [a]
    exec: {{ command: [echo] }}
  d:
    depends_on: [b, c]
    exec: {{ command: [echo] }}
"
        );
        let analyzed = analyze_yaml(&yaml).expect("valid diamond");
        assert_eq!(analyzed.topo_waves.len(), 3);
        assert_eq!(analyzed.topo_waves[0], vec![0]); // a
        assert_eq!(analyzed.topo_waves[1], vec![1, 2]); // b ∥ c
        assert_eq!(analyzed.topo_waves[2], vec![3]); // d
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
}
