// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Metamorphic conformance (ADR-092 ladder #9 · the first slice).
//!
//! Differential testing classically diffs N engines on one input; we
//! have one engine — so we diff ONE engine against ITSELF across
//! *equivalence transformations*, the single-system reformulation of
//! differential testing (Wu, Zheng, Yang & Yu 2025 *Compiler
//! Optimization Testing Based on Optimization-Guided Equivalence
//! Transformations* · arxiv.org/abs/2504.04321; methodology lineage:
//! Ba, Jiang & Rigger 2025 *Metamorphic Coverage* ·
//! arxiv.org/abs/2508.16307 — a metamorphic test examines an expected
//! relation between PAIRS of executions, no external oracle needed).
//!
//! The generator emits random VALID workflows as YAML text — through
//! the real front door (`parse` → `analyze` → `check`), not by
//! constructing ASTs — so relation R0 is itself a differential: the
//! generator's validity model vs the engine's. The relations ·
//!
//! - **R0 generator↔engine** — every generated workflow parses and
//!   analyzes clean (a disagreement is a bug in one of them).
//! - **R1 permutation invariance** — the YAML order of task blocks is
//!   not semantics: reversing the list preserves the whole verdict
//!   (conformance · `is_clean` · certificate · cost total · finding
//!   counts). Wave indices may differ; ids may not.
//! - **R2 alpha-renaming** — renaming every task id (same structure,
//!   different names) preserves everything modulo the names
//!   themselves: certificates map term-for-term.

use std::fmt::Write as _;

use proptest::prelude::*;

use nika_check::{Bound, CheckReport, RunCertificate, check};
use nika_schema::{FileId, ParseMode, parse};

/// One generated task — a STRUCTURE, rendered to YAML with any id
/// prefix (that is what makes R2 trivial and exact).
#[derive(Debug, Clone)]
struct TaskSpec {
    /// Dependencies, as indices < this task's index (acyclic by
    /// construction).
    deps: Vec<usize>,
    /// Which verb body (0 = exec · 1 = infer · 2 = invoke).
    verb: u8,
    /// `retry: max_attempts` (1 = no retry block).
    attempts: u8,
    /// `when:` gate: 0 = none · 1 = `true` · 2 = `== 'success'` on the
    /// first dep (only when deps exist).
    gate: u8,
    /// `for_each`: 0 = none · 1 = literal 2-list · 2 = expression over
    /// the first dep's output (only when deps exist).
    fan: u8,
}

fn task_strategy(index: usize) -> impl Strategy<Value = TaskSpec> {
    let deps = if index == 0 {
        proptest::collection::vec(0..1usize, 0).boxed()
    } else {
        proptest::collection::vec(0..index, 0..=2usize.min(index)).boxed()
    };
    (deps, 0..3u8, 1..=3u8, 0..3u8, 0..3u8).prop_map(|(mut deps, verb, attempts, gate, fan)| {
        deps.sort_unstable();
        deps.dedup();
        TaskSpec {
            deps,
            verb,
            attempts,
            gate,
            fan,
        }
    })
}

fn workflow_strategy() -> impl Strategy<Value = Vec<TaskSpec>> {
    (1..=5usize).prop_flat_map(|n| {
        let tasks: Vec<_> = (0..n).map(task_strategy).collect();
        tasks
    })
}

/// Render the structure to YAML with `prefix` naming the tasks.
fn to_yaml(tasks: &[TaskSpec], prefix: &str) -> String {
    let mut y = String::from("nika: meta\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n");
    for (i, t) in tasks.iter().enumerate() {
        let id = format!("{prefix}{i}");
        let _ = writeln!(y, "  {id}:");
        // W2 « the flow » — the first dep becomes a with: VALUE binding
        // when a gate/fan variant consumes it (the binding IS the edge);
        // the remaining deps become after: control entries.
        let bound_dep = t.deps.first().filter(|_| t.gate == 2 || t.fan == 2);
        if let Some(d) = bound_dep {
            let _ = writeln!(y, "    with:");
            let _ = writeln!(y, "      upstream: \"${{{{ tasks.{prefix}{d}.output }}}}\"");
        }
        let control: Vec<usize> = t
            .deps
            .iter()
            .copied()
            .filter(|d| Some(d) != bound_dep)
            .collect();
        if !control.is_empty() {
            let _ = writeln!(y, "    after:");
            for d in control {
                let _ = writeln!(y, "      {prefix}{d}: success");
            }
        }
        if t.attempts > 1 {
            let _ = writeln!(y, "    retry: {{ max_attempts: {} }}", t.attempts);
        }
        match (t.gate, bound_dep) {
            (1, _) => y.push_str("    when: true\n"),
            (2, Some(_)) => {
                y.push_str("    when: ${{ with.upstream != null }}\n");
            }
            _ => {}
        }
        match (t.fan, bound_dep) {
            (1, _) => y.push_str("    for_each: { items: [\"a\", \"b\"] }\n"),
            (2, Some(_)) => {
                y.push_str("    for_each: { items: \"${{ with.upstream }}\" }\n");
            }
            _ => {}
        }
        match t.verb {
            0 => y.push_str("    exec: { command: [\"true\"] }\n"),
            1 => y.push_str("    infer: { prompt: \"go\", max_tokens: 50 }\n"),
            _ => y.push_str("    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n"),
        }
    }
    y
}

/// Front-door run: parse (strict) then the infallible check.
fn run(yaml: &str) -> Result<CheckReport, String> {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).map_err(|e| e.to_string())?;
    Ok(check(&wf))
}

/// A bound as an order-free multiset (terms sorted) for comparison
/// across permutations.
fn canon(b: &Bound) -> (u64, Vec<(String, u64)>) {
    let mut terms: Vec<(String, u64)> = b.terms.iter().map(|t| (t.task.clone(), t.coeff)).collect();
    terms.sort();
    (b.constant, terms)
}

/// Strip the id prefix from a canonical bound (for R2: certificates
/// must agree once names are erased).
fn deprefix(c: (u64, Vec<(String, u64)>), prefix: &str) -> (u64, Vec<(String, u64)>) {
    (
        c.0,
        c.1.into_iter()
            .map(|(task, coeff)| (task.trim_start_matches(prefix).to_owned(), coeff))
            .collect(),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// R0 — the generator's validity model and the engine agree.
    #[test]
    fn r0_generated_workflows_are_engine_valid(tasks in workflow_strategy()) {
        let yaml = to_yaml(&tasks, "t");
        let report = run(&yaml).expect("generated workflow must parse");
        prop_assert!(
            report.conformance.is_empty(),
            "generator/engine disagreement on:\n{yaml}\n{:?}",
            report.conformance
        );
    }

    /// R1 — YAML task order is not semantics: reversal preserves the
    /// whole verdict.
    #[test]
    fn r1_task_order_is_not_semantics(tasks in workflow_strategy()) {
        let forward = run(&to_yaml(&tasks, "t")).expect("forward parses");
        // reverse the BLOCKS (ids and deps unchanged — pure file order)
        let yaml_fwd = to_yaml(&tasks, "t");
        let header_end = yaml_fwd.find("tasks:\n").map_or(0, |i| i + 7);
        let (header, body) = yaml_fwd.split_at(header_end);
        // a block starts at each indent-2 bare `name:` key (the map form)
        let mut blocks: Vec<String> = Vec::new();
        for line in body.lines() {
            let is_key = line.strip_prefix("  ").is_some_and(|r| {
                !r.starts_with(' ')
                    && r.trim_end().ends_with(':')
                    && r.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            });
            if is_key || blocks.is_empty() {
                blocks.push(String::new());
            }
            if let Some(b) = blocks.last_mut() {
                b.push_str(line);
                b.push('\n');
            }
        }
        let mut reversed = String::from(header);
        for b in blocks.iter().rev() {
            reversed.push_str(b);
        }
        let backward = run(&reversed).expect("reversed parses");

        prop_assert_eq!(forward.is_clean(), backward.is_clean());
        prop_assert_eq!(forward.conformance.len(), backward.conformance.len());
        prop_assert_eq!(forward.gate_findings.len(), backward.gate_findings.len());
        prop_assert_eq!(forward.hints.len(), backward.hints.len());
        prop_assert_eq!(
            canon(&forward.certificate.task_attempts),
            canon(&backward.certificate.task_attempts)
        );
        prop_assert_eq!(
            canon(&forward.certificate.llm_calls),
            canon(&backward.certificate.llm_calls)
        );
        prop_assert_eq!(
            forward.certificate.usd_micros.as_ref().map(canon),
            backward.certificate.usd_micros.as_ref().map(canon)
        );
        // the cost ceiling total is order-free too
        prop_assert!(
            (forward.cost.bounded_total_usd - backward.cost.bounded_total_usd).abs()
                < f64::EPSILON
        );
    }

    /// R2 — alpha-renaming: same structure, different names, identical
    /// verdict modulo the names.
    #[test]
    fn r2_alpha_renaming_preserves_the_verdict(tasks in workflow_strategy()) {
        let a = run(&to_yaml(&tasks, "alpha")).expect("alpha parses");
        let b = run(&to_yaml(&tasks, "beta")).expect("beta parses");

        prop_assert_eq!(a.is_clean(), b.is_clean());
        prop_assert_eq!(a.conformance.len(), b.conformance.len());
        prop_assert_eq!(a.gate_findings.len(), b.gate_findings.len());
        prop_assert_eq!(a.hints.len(), b.hints.len());
        prop_assert_eq!(
            deprefix(canon(&a.certificate.task_attempts), "alpha"),
            deprefix(canon(&b.certificate.task_attempts), "beta")
        );
        prop_assert_eq!(
            deprefix(canon(&a.certificate.llm_calls), "alpha"),
            deprefix(canon(&b.certificate.llm_calls), "beta")
        );
        prop_assert_eq!(
            a.certificate.usd_micros.as_ref().map(|x| deprefix(canon(x), "alpha")),
            b.certificate.usd_micros.as_ref().map(|x| deprefix(canon(x), "beta"))
        );
    }
}

/// Total counts with every parametric term erased — for relations that
/// preserve TOTALS but not names (R3 unfolding).
fn totals(c: &RunCertificate) -> (u64, u64, u64, Option<u64>) {
    let flat = |b: &Bound| b.constant + b.terms.iter().map(|t| t.coeff).sum::<u64>();
    (
        flat(&c.task_attempts),
        flat(&c.llm_calls),
        flat(&c.effect_calls),
        c.usd_micros.as_ref().map(flat),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// R3 — unfolding: a literal 2-list `for_each` task equals TWO
    /// plain copies of the task (total attempts · calls · spend).
    #[test]
    fn r3_literal_fanout_unfolds_to_duplicated_tasks(verb in 0..3u8, attempts in 1..=3u8) {
        let spec = |fan: bool, n: usize| {
            let body = match verb {
                0 => "    exec: { command: [\"true\"] }\n",
                1 => "    infer: { prompt: \"go\", max_tokens: 50 }\n",
                _ => "    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n",
            };
            let retry = if attempts > 1 {
                format!("    retry: {{ max_attempts: {attempts} }}\n")
            } else {
                String::new()
            };
            let mut y = String::from(
                "nika: meta\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n",
            );
            for i in 0..n {
                use std::fmt::Write as _;
                let _ = writeln!(y, "  t{i}:");
                y.push_str(&retry);
                if fan {
                    y.push_str("    for_each: { items: [\"a\", \"b\"] }\n");
                }
                y.push_str(body);
            }
            y
        };
        let folded = run(&spec(true, 1)).expect("folded parses");
        let unfolded = run(&spec(false, 2)).expect("unfolded parses");
        prop_assert_eq!(totals(&folded.certificate), totals(&unfolded.certificate));
    }

    /// R4 — frame: adding an independent plain task changes the bounds
    /// by EXACTLY its own contribution (compositionality).
    #[test]
    fn r4_adding_an_independent_task_is_compositional(tasks in workflow_strategy()) {
        let base = run(&to_yaml(&tasks, "t")).expect("base parses");
        let mut extended_yaml = to_yaml(&tasks, "t");
        extended_yaml.push_str("  frame_extra:\n    exec: { command: [\"true\"] }\n");
        let extended = run(&extended_yaml).expect("extended parses");
        let (a1, l1, e1, _) = totals(&base.certificate);
        let (a2, l2, e2, _) = totals(&extended.certificate);
        // one plain exec task: +1 attempt · +0 llm · +1 effect
        prop_assert_eq!(a2, a1 + 1);
        prop_assert_eq!(l2, l1);
        prop_assert_eq!(e2, e1 + 1);
    }

    /// R5 — `retry: max_attempts: 1` is the identity (≡ no retry block).
    #[test]
    fn r5_retry_one_is_identity(tasks in workflow_strategy()) {
        // normalize every task to attempts=1 (the generator then emits
        // NO retry lines), and build the twin with an EXPLICIT retry-1
        // block inserted after each task header
        let mut ones = tasks.clone();
        for t in &mut ones {
            t.attempts = 1;
        }
        let plain = to_yaml(&ones, "t");
        let with_retry1 = plain
            .lines()
            .map(|l| {
                let is_key = l.strip_prefix("  ").is_some_and(|r| {
                    !r.starts_with(' ')
                        && r.trim_end().ends_with(':')
                        && r.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                });
                if is_key {
                    format!("{l}\n    retry: {{ max_attempts: 1 }}")
                } else {
                    l.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let a = run(&plain).expect("plain parses");
        let b = run(&with_retry1).expect("retry-1 parses");
        prop_assert_eq!(canon(&a.certificate.task_attempts), canon(&b.certificate.task_attempts));
        prop_assert_eq!(canon(&a.certificate.llm_calls), canon(&b.certificate.llm_calls));
        prop_assert_eq!(a.is_clean(), b.is_clean());
    }

    /// R6 — every honest certificate passes its own audit (the checker
    /// accepts what the analysis produces, over the whole generator).
    #[test]
    fn r6_honest_certificates_always_audit_clean(tasks in workflow_strategy()) {
        let yaml = to_yaml(&tasks, "t");
        let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = check(&wf);
        prop_assert!(
            report.certificate.audit(&wf).is_ok(),
            "honest certificate rejected on:\n{yaml}"
        );
    }
}
