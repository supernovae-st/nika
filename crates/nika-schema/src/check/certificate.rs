// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run certificate — termination + parametric resource bounds
//! (ADR-092 ladder #7 · the AARA degree-1 slice).
//!
//! Grounded in automatic amortized resource analysis: bounds are
//! *resource polynomials parametric in input sizes* (Hoffmann, Das &
//! Weng 2016 *Towards Automatic Resource Bound Analysis for OCaml* ·
//! arxiv.org/abs/1611.00692 · the line of work still active in
//! Chu, Guo & Hoffmann 2026 · arxiv.org/abs/2603.02260). General AARA
//! infers coefficients by LP over typing derivations; Nika needs no
//! solver — the workflow IS its own derivation: acyclic, every loop a
//! `for_each` over one collection, every retry capped, every agent
//! turn-capped (default 10 · spec field table). The bound coefficients
//! read directly off the structure, degree 1 (no nested `for_each` in
//! v0.1 — the day nesting lands, products of sizes appear and this
//! module grows degree-2 terms).
//!
//! Termination is a THEOREM of the language, not an analysis result:
//! a certificate always exists. What the certificate adds is the
//! quantitative envelope — « this run performs at most `5 + 2·|fetch|`
//! task-attempts, at most `1 + |fetch|` LLM calls » — where `|t|` is
//! the runtime size of task `t`'s `for_each` collection. No other
//! workflow engine can state this before running (Turing-complete
//! competitors cannot state it at all).
//!
//! Counting model (sound worst-case) ·
//! - a task body runs ≤ `attempts × iterations` times (`retry:` re-runs
//!   the body · `for_each` fans it out per element)
//! - `on_finally` cleanups run once per ITERATION (after the terminal
//!   attempt — never per attempt)
//! - `infer` = 1 LLM call per body run · `agent` ≤ `max_turns` (default
//!   10) per body run · `exec`/`invoke` = 1 effect call per body run

use crate::raw::{ForEachValue, RawAction, RawWorkflow};

/// One parametric term: `coeff · |task|` (the `for_each` collection
/// size of `task`, known only at runtime).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct CertTerm {
    /// The task whose `for_each` collection size parameterizes the bound.
    pub task: String,
    /// The multiplier on that size.
    pub coeff: u64,
}

/// A degree-1 resource polynomial: `constant + Σ coeff·|task|`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct Bound {
    /// The constant part.
    pub constant: u64,
    /// The parametric terms (at most one per `for_each`-expression task).
    pub terms: Vec<CertTerm>,
}

impl Bound {
    /// Add `coeff` constant occurrences.
    fn add_const(&mut self, coeff: u64) {
        self.constant = self.constant.saturating_add(coeff);
    }

    /// Add a `coeff·|task|` term (merging with an existing term for the
    /// same task).
    fn add_term(&mut self, task: &str, coeff: u64) {
        if coeff == 0 {
            return;
        }
        if let Some(t) = self.terms.iter_mut().find(|t| t.task == task) {
            t.coeff = t.coeff.saturating_add(coeff);
        } else {
            self.terms.push(CertTerm {
                task: task.to_owned(),
                coeff,
            });
        }
    }

    /// Exactly zero (renderers print `0` instead of `≤ 0`).
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.constant == 0 && self.terms.is_empty()
    }
}

/// The termination + resource certificate (always exists — see the
/// module doc; the language admits no unbounded run).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct RunCertificate {
    /// Upper bound on task-body executions (attempts × fan-out, summed).
    pub task_attempts: Bound,
    /// Upper bound on LLM calls (`infer` + `agent` turns).
    pub llm_calls: Bound,
    /// Upper bound on effect calls (`exec` + `invoke` dispatches).
    pub effect_calls: Bound,
}

/// A task's fan-out multiplier.
enum Multiplier<'t> {
    /// Plain task or literal `for_each` list of known length.
    Const(u64),
    /// `for_each` over an expression — `|task|`, runtime-sized.
    Param(&'t str),
}

/// How many calls of each class ONE body run of `action` performs.
/// Returns `(llm, effect)`.
fn action_calls(action: &RawAction) -> (u64, u64) {
    match action {
        RawAction::Infer(_) => (1, 0),
        // the loop limit is the LLM-call bound (default 10 · spec)
        RawAction::Agent(a) => (u64::from(a.max_turns.as_ref().map_or(10, |t| t.value)), 0),
        // exhaustive ON PURPOSE (we are the defining crate): a future
        // verb fails compilation HERE until it declares its call class —
        // the certificate can never silently under-count a new verb
        RawAction::Exec(_) | RawAction::Invoke(_) => (0, 1),
    }
}

/// Compute the certificate — one linear pass, conformance-independent
/// (a pure sum over tasks, like the cost ceiling).
pub(crate) fn certify(wf: &RawWorkflow) -> RunCertificate {
    let mut cert = RunCertificate::default();

    for task in &wf.tasks {
        let t = &task.value;
        let id = t.id.value.as_str();
        let attempts = t
            .retry
            .as_ref()
            .map_or(1, |r| u64::from(r.value.max_attempts).max(1));
        let mult = match t.for_each.as_ref().map(|f| &f.value) {
            None => Multiplier::Const(1),
            Some(ForEachValue::List(arr)) => {
                Multiplier::Const(arr.as_array().map_or(1, Vec::len) as u64)
            }
            Some(ForEachValue::Expression(_)) => Multiplier::Param(id),
        };

        // body runs: attempts × fan-out
        add(&mut cert.task_attempts, &mult, attempts);

        // calls from the main action: per body run
        let (llm, effect) = action_calls(&t.action);
        add(&mut cert.llm_calls, &mult, llm.saturating_mul(attempts));
        add(
            &mut cert.effect_calls,
            &mult,
            effect.saturating_mul(attempts),
        );

        // on_finally: once per iteration (after the terminal attempt)
        for cleanup in &t.on_finally {
            let (llm, effect) = action_calls(&cleanup.value.action);
            add(&mut cert.llm_calls, &mult, llm);
            add(&mut cert.effect_calls, &mult, effect);
        }
    }
    cert
}

/// Fold `coeff` occurrences through the fan-out multiplier.
fn add(bound: &mut Bound, mult: &Multiplier<'_>, coeff: u64) {
    if coeff == 0 {
        return;
    }
    match mult {
        Multiplier::Const(n) => bound.add_const(coeff.saturating_mul(*n)),
        Multiplier::Param(task) => bound.add_term(task, coeff),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileId, ParseMode, parse};

    fn cert(yaml: &str) -> RunCertificate {
        certify(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    fn wf(tasks: &str) -> String {
        format!("nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n{tasks}")
    }

    fn konst(n: u64) -> Bound {
        Bound {
            constant: n,
            terms: vec![],
        }
    }

    #[test]
    fn plain_pipeline_has_exact_constant_bounds() {
        let c = cert(&wf(
            "  - id: a\n    exec: { command: \"true\" }\n  - id: b\n    depends_on: [a]\n    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n",
        ));
        assert_eq!(c.task_attempts, konst(2));
        assert_eq!(c.llm_calls, konst(0));
        assert!(c.llm_calls.is_zero());
        assert_eq!(c.effect_calls, konst(2));
    }

    #[test]
    fn retry_multiplies_the_body_and_its_calls() {
        let c = cert(&wf(
            "  - id: a\n    retry: { max_attempts: 3 }\n    infer: { prompt: \"x\", max_tokens: 10 }\n",
        ));
        assert_eq!(c.task_attempts, konst(3));
        assert_eq!(c.llm_calls, konst(3));
        assert_eq!(c.effect_calls, konst(0));
    }

    #[test]
    fn literal_for_each_folds_into_the_constant() {
        let c = cert(&wf(
            "  - id: a\n    for_each: [\"x\", \"y\", \"z\"]\n    retry: { max_attempts: 2 }\n    exec: { command: \"echo ${{ item }}\" }\n",
        ));
        // 3 elements × 2 attempts
        assert_eq!(c.task_attempts, konst(6));
        assert_eq!(c.effect_calls, konst(6));
    }

    #[test]
    fn expression_for_each_yields_a_parametric_term() {
        let c = cert(&wf(
            "  - id: src\n    exec: { command: \"ls\" }\n  - id: fan\n    depends_on: [src]\n    for_each: ${{ tasks.src.output.files }}\n    retry: { max_attempts: 2 }\n    infer: { prompt: \"summarize ${{ item }}\", max_tokens: 10 }\n",
        ));
        // src: 1 attempt · fan: 2·|fan| body runs
        assert_eq!(c.task_attempts.constant, 1);
        assert_eq!(
            c.task_attempts.terms,
            vec![CertTerm {
                task: "fan".into(),
                coeff: 2
            }]
        );
        // LLM calls: parametric only (src is exec)
        assert_eq!(c.llm_calls.constant, 0);
        assert_eq!(
            c.llm_calls.terms,
            vec![CertTerm {
                task: "fan".into(),
                coeff: 2
            }]
        );
        assert_eq!(c.effect_calls, konst(1));
    }

    #[test]
    fn agent_counts_its_turn_cap_default_ten() {
        let defaulted = cert(&wf(
            "  - id: a\n    agent: { prompt: \"go\", tools: [\"nika:read\"] }\n",
        ));
        assert_eq!(defaulted.llm_calls, konst(10), "spec default max_turns");
        let capped = cert(&wf(
            "  - id: a\n    agent: { prompt: \"go\", tools: [\"nika:read\"], max_turns: 4 }\n",
        ));
        assert_eq!(capped.llm_calls, konst(4));
    }

    #[test]
    fn on_finally_counts_once_per_iteration_not_per_attempt() {
        let c = cert(&wf(
            "  - id: a\n    retry: { max_attempts: 5 }\n    exec: { command: \"true\" }\n    on_finally:\n      - invoke: { tool: \"nika:log\", args: { message: \"done\" } }\n",
        ));
        // body: 5 attempts · cleanup: ONCE (after the terminal attempt)
        assert_eq!(c.task_attempts, konst(5));
        assert_eq!(c.effect_calls, konst(5 + 1));
    }

    #[test]
    fn the_wire_shape_is_pinned() {
        let c = cert(&wf(
            "  - id: fan\n    for_each: ${{ vars.items }}\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 5 }\n",
        ));
        let json = serde_json::to_value(&c).expect("serializes");
        assert_eq!(
            json,
            serde_json::json!({
                "task_attempts": { "constant": 0, "terms": [{ "task": "fan", "coeff": 1 }] },
                "llm_calls": { "constant": 0, "terms": [{ "task": "fan", "coeff": 1 }] },
                "effect_calls": { "constant": 0, "terms": [] },
            })
        );
    }
}
