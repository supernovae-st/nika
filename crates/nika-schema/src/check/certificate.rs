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
//! **Certifying-algorithm form** (round 4): the certificate carries its
//! WITNESS — the per-task `derivation` rows — and [`RunCertificate::audit`]
//! is the independent checker: simpler than the analysis — field
//! equality and one shared fold, no enumeration — per the discipline
//! that a certifying algorithm outputs a result WITH a witness and a
//! checker the user can trust without trusting the solver (Shokry,
//! Elmasry,
//! Khalafallah & Aly 2024 *Verifying Shortest Paths in Linear Time* ·
//! arxiv.org/abs/2412.06121). The execution-side architecture this
//! feeds is Proposal–Certification–Execution — « generation is not
//! permission » (Liu Yanglet, Wang & Capponi 2026 *No Certificate, No
//! Execution* · arxiv.org/abs/2605.24462): a foreign certificate (a
//! marketplace artifact · a CI gate input) is re-checkable LOCALLY
//! against the workflow it claims to bound.
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
    /// The PARAMETRIC spend bound, in micro-USD (integer keeps the
    /// wire exact) — the deepening of the cost ceiling: a
    /// `for_each`-expression fan-out that the COST section must call
    /// « unknown iterations » is HERE a degree-1 term (`$0.012·|fan|`).
    /// `None` when any spender is unpriceable (no token bound · no
    /// catalog price — the COST report names which and why).
    pub usd_micros: Option<Bound>,
    /// The WITNESS: per-task contribution rows. The bounds above are
    /// derived from these rows by ONE shared fold — and
    /// [`RunCertificate::audit`] re-derives + compares, so a foreign
    /// certificate whose bounds disagree with its own derivation (or
    /// whose derivation disagrees with the workflow) is rejected
    /// locally, no analysis re-run needed.
    pub derivation: Vec<TaskContribution>,
}

impl RunCertificate {
    /// The independent checker (certifying-algorithm discipline): a
    /// certificate is accepted iff (a) every derivation row matches the
    /// workflow's declared structure — direct field equality, no
    /// analysis machinery — and (b) the rows re-fold to exactly the
    /// claimed bounds. Price VALUES inside rows are trusted (re-pricing
    /// would need the catalog — the checker stays catalog-free); what
    /// cannot be tampered with is the arithmetic and the structure.
    ///
    /// # Errors
    /// A human-readable description of the first mismatch.
    pub fn audit(&self, wf: &RawWorkflow) -> Result<(), String> {
        if wf.tasks.len() != self.derivation.len() {
            return Err(format!(
                "derivation has {} rows but the workflow has {} tasks",
                self.derivation.len(),
                wf.tasks.len()
            ));
        }
        for (task, row) in wf.tasks.iter().zip(&self.derivation) {
            check_row(&task.value, row)?;
        }
        let refolded = fold_rows(&self.derivation);
        if refolded.task_attempts != self.task_attempts
            || refolded.llm_calls != self.llm_calls
            || refolded.effect_calls != self.effect_calls
            || refolded.usd_micros != self.usd_micros
        {
            return Err("the claimed bounds do not match the derivation".into());
        }
        Ok(())
    }
}

/// Row ↔ workflow structural equality (the LOCAL half of the checker).
fn check_row(t: &crate::raw::RawTask, row: &TaskContribution) -> Result<(), String> {
    let id = t.id.value.as_str();
    if row.task != id {
        return Err(format!("row names `{}` but the task is `{id}`", row.task));
    }
    let attempts = t
        .retry
        .as_ref()
        .map_or(1, |r| u64::from(r.value.max_attempts).max(1));
    if row.attempts != attempts {
        return Err(format!(
            "`{id}`: row claims {} attempts, workflow says {attempts}",
            row.attempts
        ));
    }
    let fanout = match t.for_each.as_ref().map(|f| &f.value) {
        None => FanOut::Known(1),
        Some(ForEachValue::List(arr)) => FanOut::Known(arr.as_array().map_or(1, Vec::len) as u64),
        Some(ForEachValue::Expression(_)) => FanOut::Collection,
    };
    if row.fanout != fanout {
        return Err(format!("`{id}`: fan-out shape mismatch"));
    }
    let (llm, effect) = action_calls(&t.action);
    if row.main_llm != llm || row.main_effect != effect {
        return Err(format!("`{id}`: main-action call counts mismatch"));
    }
    let (mut f_llm, mut f_effect) = (0u64, 0u64);
    for cleanup in &t.on_finally {
        let (l, e) = action_calls(&cleanup.value.action);
        f_llm += l;
        f_effect += e;
    }
    if row.finally_llm != f_llm || row.finally_effect != f_effect {
        return Err(format!("`{id}`: on_finally call counts mismatch"));
    }
    Ok(())
}

/// A task's fan-out, witness-shaped (serializable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FanOut {
    /// Plain task or literal list: a known multiplier.
    Known(u64),
    /// `for_each` over an expression: ×|task| at runtime.
    Collection,
}

/// One derivation row — the per-task WITNESS the checker re-verifies.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct TaskContribution {
    /// The task id.
    pub task: String,
    /// `retry: max_attempts` (1 when absent).
    pub attempts: u64,
    /// The fan-out shape.
    pub fanout: FanOut,
    /// LLM calls per body run (main action).
    pub main_llm: u64,
    /// Effect calls per body run (main action).
    pub main_effect: u64,
    /// Spend per body run in micro-USD (`None` = unpriceable).
    pub main_spend_micros: Option<u64>,
    /// LLM calls per ITERATION from `on_finally` cleanups.
    pub finally_llm: u64,
    /// Effect calls per ITERATION from `on_finally` cleanups.
    pub finally_effect: u64,
    /// `on_finally` spend per iteration (`None` = unpriceable).
    pub finally_spend_micros: Option<u64>,
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

/// ONE body run's worst-case spend in micro-USD — `Ok(0)` for
/// non-spenders, `Err(())` when the spend exists but cannot be priced
/// (no token bound · no catalog price · the COST report carries the
/// reason; the certificate's spend axis just goes `None`).
fn action_spend_micros(action: &RawAction, default_model: Option<&str>) -> Result<u64, ()> {
    let (model, tokens) = match action {
        RawAction::Infer(a) => (
            a.model.as_ref().map(|m| m.value.as_str()),
            a.max_tokens.as_ref().map(|t| u64::from(t.value)),
        ),
        // the agent budget is CUMULATIVE across turns — one body run
        // spends at most max_tokens_total, never ×turns
        RawAction::Agent(a) => (
            a.model.as_ref().map(|m| m.value.as_str()),
            a.max_tokens_total.as_ref().map(|t| t.value),
        ),
        RawAction::Exec(_) | RawAction::Invoke(_) => return Ok(0),
    };
    let Some(tokens) = tokens else { return Err(()) };
    let Some(price) = model
        .or(default_model)
        .and_then(super::cost::output_price_per_million)
    else {
        return Err(());
    };
    // micro-USD: tokens × price-per-million-tokens = micro-USD exactly.
    // Casts justified: token budgets are ≪ 2^52 (f64-exact) and the
    // product is non-negative + rounded before the narrowing.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    Ok(((tokens as f64) * price).round() as u64)
}

/// Compute the certificate — build the witness rows, then derive the
/// bounds from them by the ONE shared fold (the bounds cannot disagree
/// with the derivation by construction; `audit` re-runs the same fold
/// on foreign certificates).
pub(crate) fn certify(wf: &RawWorkflow) -> RunCertificate {
    let default_model = wf.model.as_ref().map(|m| m.value.as_str());
    let derivation: Vec<TaskContribution> = wf
        .tasks
        .iter()
        .map(|task| contribution(&task.value, default_model))
        .collect();
    let mut cert = fold_rows(&derivation);
    cert.derivation = derivation;
    cert
}

/// One task's witness row.
fn contribution(t: &crate::raw::RawTask, default_model: Option<&str>) -> TaskContribution {
    let attempts = t
        .retry
        .as_ref()
        .map_or(1, |r| u64::from(r.value.max_attempts).max(1));
    let fanout = match t.for_each.as_ref().map(|f| &f.value) {
        None => FanOut::Known(1),
        Some(ForEachValue::List(arr)) => FanOut::Known(arr.as_array().map_or(1, Vec::len) as u64),
        Some(ForEachValue::Expression(_)) => FanOut::Collection,
    };
    let (main_llm, main_effect) = action_calls(&t.action);
    let main_spend_micros = action_spend_micros(&t.action, default_model).ok();
    let (mut finally_llm, mut finally_effect) = (0u64, 0u64);
    let mut finally_spend_micros = Some(0u64);
    for cleanup in &t.on_finally {
        let (l, e) = action_calls(&cleanup.value.action);
        finally_llm += l;
        finally_effect += e;
        match (
            finally_spend_micros,
            action_spend_micros(&cleanup.value.action, default_model),
        ) {
            (Some(acc), Ok(m)) => finally_spend_micros = Some(acc.saturating_add(m)),
            _ => finally_spend_micros = None,
        }
    }
    TaskContribution {
        task: t.id.value.clone(),
        attempts,
        fanout,
        main_llm,
        main_effect,
        main_spend_micros,
        finally_llm,
        finally_effect,
        finally_spend_micros,
    }
}

/// THE shared fold: witness rows → bounds. Used by `certify` (to build)
/// and by `audit` (to re-check) — the arithmetic cannot drift between
/// the two because it exists once.
fn fold_rows(rows: &[TaskContribution]) -> RunCertificate {
    let mut cert = RunCertificate::default();
    let mut spend = Some(Bound::default());
    for row in rows {
        let mult = match row.fanout {
            FanOut::Known(n) => Multiplier::Const(n),
            FanOut::Collection => Multiplier::Param(&row.task),
        };
        add(&mut cert.task_attempts, &mult, row.attempts);
        add(
            &mut cert.llm_calls,
            &mult,
            row.main_llm.saturating_mul(row.attempts) + row.finally_llm,
        );
        add(
            &mut cert.effect_calls,
            &mult,
            row.main_effect.saturating_mul(row.attempts) + row.finally_effect,
        );
        match (
            spend.as_mut(),
            row.main_spend_micros,
            row.finally_spend_micros,
        ) {
            (Some(b), Some(main), Some(fin)) => {
                add(b, &mult, main.saturating_mul(row.attempts) + fin);
            }
            _ => spend = None,
        }
    }
    cert.usd_micros = spend;
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
    fn spend_axis_is_parametric_where_cost_says_unknown_iterations() {
        // the deepening: for_each-expression spend = a degree-1 term
        let c = cert(&wf(
            "  - id: src\n    exec: { command: \"ls\" }\n  - id: fan\n    depends_on: [src]\n    for_each: ${{ tasks.src.output.files }}\n    retry: { max_attempts: 2 }\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 200 }\n",
        ));
        let usd = c.usd_micros.expect("priced");
        assert_eq!(usd.constant, 0);
        // 200 tk × $15/M = 3000 µ$ per call · ×2 attempts = 6000·|fan|
        assert_eq!(
            usd.terms,
            vec![CertTerm {
                task: "fan".into(),
                coeff: 6000
            }]
        );
    }

    #[test]
    fn unpriceable_spender_makes_the_spend_axis_none_only() {
        // no max_tokens → spend axis None; the COUNT axes stay exact
        let c = cert(&wf("  - id: a\n    infer: { prompt: \"x\" }\n"));
        assert!(c.usd_micros.is_none());
        assert_eq!(c.llm_calls, konst(1));
        // agent budget is CUMULATIVE — one body run ≤ max_tokens_total,
        // never ×turns
        let a = cert(&wf(
            "  - id: a\n    agent: { prompt: \"go\", tools: [\"nika:read\"], max_turns: 4, max_tokens_total: 1000 }\n",
        ));
        let usd = a.usd_micros.expect("priced");
        assert_eq!(usd.constant, 15_000, "1000 tk × $15/M · NOT ×4 turns");
    }

    #[test]
    fn audit_accepts_honest_and_rejects_tampered_certificates() {
        let yaml = wf(
            "  - id: a\n    exec: { command: \"true\" }\n  - id: fan\n    depends_on: [a]\n    for_each: ${{ tasks.a.output.items }}\n    retry: { max_attempts: 2 }\n    infer: { prompt: \"x\", max_tokens: 50 }\n",
        );
        let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let honest = certify(&parsed);
        assert!(honest.audit(&parsed).is_ok(), "honest cert must pass");

        // tamper 1: inflate a claimed bound — the derivation no longer
        // re-folds to it
        let mut inflated = honest.clone();
        inflated.llm_calls.constant += 5;
        assert!(
            inflated
                .audit(&parsed)
                .is_err_and(|e| e.contains("do not match the derivation")),
            "bound tampering must be caught"
        );

        // tamper 2: doctor a witness row — structure check catches it
        let mut doctored = honest.clone();
        doctored.derivation[1].attempts = 1;
        // keep the bounds consistent with the doctored row so ONLY the
        // structural half can catch it
        let mut refold = doctored.clone();
        refold.derivation[1].attempts = 1;
        assert!(
            doctored
                .audit(&parsed)
                .is_err_and(|e| e.contains("attempts")),
            "row/workflow mismatch must be caught"
        );

        // tamper 3: wrong row count
        let mut truncated = honest;
        truncated.derivation.pop();
        assert!(truncated.audit(&parsed).is_err());
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
                // 5 tokens × the catalog output price ($15/M) = 75 µ$ —
                // the PARAMETRIC spend the cost ceiling cannot express
                "usd_micros": { "constant": 0, "terms": [{ "task": "fan", "coeff": 75 }] },
                // the WITNESS row — what audit() re-checks
                "derivation": [{
                    "task": "fan", "attempts": 1, "fanout": "collection",
                    "main_llm": 1, "main_effect": 0, "main_spend_micros": 75,
                    "finally_llm": 0, "finally_effect": 0, "finally_spend_micros": 0,
                }],
            })
        );
    }
}
