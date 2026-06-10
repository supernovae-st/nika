// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG topology rules — spec `03-dag.md` §execution model.
//!
//! - `NIKA-DAG-002` · every `depends_on` entry resolves to a declared id.
//! - `NIKA-DAG-001` · « The engine MUST reject any workflow with cyclic
//!   dependencies at parse time with a clear error » (incl. 1-cycles ·
//!   self-dependency).
//! - Topological waves · « group tasks into parallel execution waves »
//!   (steps 2-3 of the execution model).

use std::collections::BTreeMap;

use crate::error::SchemaError;
use crate::raw::RawTask;
use crate::source::Spanned;

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

    const HEADER: &str = "nika: v1\nworkflow: t\n";

    #[test]
    fn cycle_two_nodes() {
        // Conformance fixture dag-topology/001-cycle.
        let yaml = format!(
            "{HEADER}tasks:
  - id: a
    depends_on: [b]
    exec: {{ command: echo }}
  - id: b
    depends_on: [a]
    exec: {{ command: echo }}
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
    fn self_dependency_is_a_cycle() {
        // Conformance fixture dag-topology/004-self-dependency · « a
        // 1-cycle (a depends_on [a]) is a cycle ».
        let yaml = format!(
            "{HEADER}tasks:
  - id: a
    depends_on: [a]
    exec: {{ command: echo }}
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
  - id: a
    depends_on: [ghost]
    exec: {{ command: echo }}
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
  - id: a
    exec: {{ command: echo }}
  - id: b
    depends_on: [a]
    exec: {{ command: echo }}
  - id: c
    depends_on: [a]
    exec: {{ command: echo }}
  - id: d
    depends_on: [b, c]
    exec: {{ command: echo }}
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
  - id: x
    exec: {{ command: echo }}
  - id: y
    exec: {{ command: echo }}
"
        );
        let analyzed = analyze_yaml(&yaml).expect("valid");
        assert_eq!(analyzed.topo_waves, vec![vec![0, 1]]);
    }
}
