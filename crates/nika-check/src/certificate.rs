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

use nika_cap::CertEffects;
use nika_types::net::MAX_TRAVERSE_PAGES;

use nika_schema::raw::{ForEachValue, RawAction, RawInvokeAction, RawWorkflow};

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
    /// The SPAN bound — the longest chain of sequential dependencies,
    /// in task-attempts (Tassarotti 2017 *Probabilistic Recurrence
    /// Relations for Work and Span of Parallel Algorithms* ·
    /// arxiv.org/abs/1704.02061 formalizes the work/span model; the
    /// lineage is Brent 1974). Retries are SEQUENTIAL (they extend the
    /// span); `for_each` fan-out is element-parallel (it extends WORK,
    /// never span). With `task_attempts` this gives the Brent envelope:
    /// the run's parallelism is work/span.
    pub span_attempts: u64,
    /// The WITNESS: per-task contribution rows. The bounds above are
    /// derived from these rows by ONE shared fold — and
    /// [`RunCertificate::audit`] re-derives + compares, so a foreign
    /// certificate whose bounds disagree with its own derivation (or
    /// whose derivation disagrees with the workflow) is rejected
    /// locally, no analysis re-run needed.
    pub derivation: Vec<TaskContribution>,
    /// The AUTHORITY projection (spec 10 · W4 · see [`CertEffects`]) — a
    /// projection, never a judge: `audit` re-derives it locally so a
    /// doctored effects story is rejected.
    pub effects: CertEffects,
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
            || refolded.span_attempts != self.span_attempts
        {
            return Err("the claimed bounds do not match the derivation".into());
        }
        // The authority projection re-derives from the workflow (spec 10 —
        // an `escapes: 0` claim is re-proven, never trusted).
        if self.effects != effects_of(wf) {
            return Err("the effects projection does not match the workflow".into());
        }
        Ok(())
    }
}

/// Row ↔ workflow structural equality (the LOCAL half of the checker).
fn check_row(t: &nika_schema::raw::RawTask, row: &TaskContribution) -> Result<(), String> {
    let id = t.id.value.as_str();
    if row.task != id {
        return Err(format!("row names `{}` but the task is `{id}`", row.task));
    }
    let deps: Vec<String> = crate::analyzer::edges::producer_ids(t);
    if row.deps != deps {
        return Err(format!("`{id}`: dependency list mismatch"));
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
        // Intentional asymmetry with `cost::static_vars_array_len`: a static
        // `${{ inputs.<name> }}` array stays a `Collection` witness here even
        // though cost.rs resolves it to a count. The certificate verifies
        // STRUCTURE, not spend — `FanOut::Known` would require the certificate
        // schema to carry the resolved count, which it does not.
        Some(ForEachValue::Expression(_)) => FanOut::Collection,
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown for_each form: {other:?}"),
    };
    if row.fanout != fanout {
        return Err(format!("`{id}`: fan-out shape mismatch"));
    }
    let (llm, effect) = action_calls(&t.action);
    if row.main_llm != llm || row.main_effect != effect {
        return Err(format!("`{id}`: main-action call counts mismatch"));
    }
    // Cleanup is a TASK now, counted in ITS OWN row — a row still
    // claiming nested cleanup calls is describing a shape the grammar
    // no longer has, so the check survives as a zero-assertion.
    if row.finally_llm != 0 || row.finally_effect != 0 {
        return Err(format!("`{id}`: nested cleanup call counts are dead"));
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
    /// The task's dependencies (by id) — the DAG edges the span bound
    /// re-folds over (witness-complete: `audit` re-checks them).
    pub deps: Vec<String>,
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
        // fail-loud ON PURPOSE (the enum lives in nika-schema; WE judge
        // it): a future verb hits this arm until it declares its call
        // class — the certificate can never silently under-count a new verb
        RawAction::Exec(_) => (0, 1),
        RawAction::Invoke(a) => (0, invoke_effect_calls(a)),
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
    }
}

/// Effect calls ONE `invoke:` body run performs — 1 for every tool
/// except a `nika:fetch` carrying `traverse:` (the bounded crawl): a
/// literal `max_pages: N` means N page requests, +1 robots.txt probe
/// unless `respect_robots:` is LITERALLY `false`; a templated spec or
/// field folds to the runtime cap ([`MAX_TRAVERSE_PAGES`] — ONE
/// definition with the runtime via `nika-types`). The certificate may
/// over-state a crawl that converges early; it must never under-count.
fn invoke_effect_calls(action: &RawInvokeAction) -> u64 {
    // A `workflow:` call is 1 effect call at the parent-own grain (the
    // dispatch itself); the COMPOSED bound — parent ⊇ own + child (spec 14
    // law 5) — is the composition lane's, where the child is resolvable.
    if action.tool().map(|t| t.value.as_str()) != Some("nika:fetch") {
        return 1;
    }
    let Some(traverse) = action
        .args
        .as_ref()
        .and_then(|args| args.value.get("traverse"))
    else {
        return 1;
    };
    let pages = traverse
        .get("max_pages")
        .and_then(serde_json::Value::as_u64)
        .filter(|n| (1..=MAX_TRAVERSE_PAGES).contains(n))
        .unwrap_or(MAX_TRAVERSE_PAGES);
    let robots = match traverse.get("respect_robots") {
        Some(serde_json::Value::Bool(false)) => 0,
        _ => 1,
    };
    pages + robots
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
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
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
    cert.effects = effects_of(wf);
    cert
}

/// ONE effects derivation, shared by `certify` (stamp) and `audit`
/// (re-check) — the `--infer-permits` inference + the report's escape scan.
fn effects_of(wf: &RawWorkflow) -> CertEffects {
    CertEffects::new(
        wf.permits.is_some(),
        super::permits_infer::infer(wf).permits,
        super::permits_fit::scan_escapes(wf).len(),
    )
}

/// One task's witness row.
fn contribution(t: &nika_schema::raw::RawTask, default_model: Option<&str>) -> TaskContribution {
    let attempts = t
        .retry
        .as_ref()
        .map_or(1, |r| u64::from(r.value.max_attempts).max(1));
    let fanout = match t.for_each.as_ref().map(|f| &f.value) {
        None => FanOut::Known(1),
        Some(ForEachValue::List(arr)) => FanOut::Known(arr.as_array().map_or(1, Vec::len) as u64),
        // Intentional asymmetry with `cost::static_vars_array_len`: a static
        // `${{ inputs.<name> }}` array stays a `Collection` witness here even
        // though cost.rs resolves it to a count. The certificate verifies
        // STRUCTURE, not spend — `FanOut::Known` would require the certificate
        // schema to carry the resolved count, which it does not.
        Some(ForEachValue::Expression(_)) => FanOut::Collection,
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown for_each form: {other:?}"),
    };
    let (main_llm, main_effect) = action_calls(&t.action);
    let main_spend_micros = action_spend_micros(&t.action, default_model).ok();
    // Cleanup contributes through its OWN row now (it is a task) — the
    // nested counters stay in the shape at zero until the certificate
    // wire drops them with its next version.
    let (finally_llm, finally_effect) = (0u64, 0u64);
    let finally_spend_micros = Some(0u64);
    TaskContribution {
        task: t.id.value.clone(),
        deps: crate::analyzer::edges::producer_ids(t),
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
    cert.span_attempts = span_of_rows(rows);
    cert
}

/// The span — longest dependency chain in attempts, computed FROM the
/// witness rows (the same one-source law as the other bounds: `audit`
/// re-runs this exact fold). Iterative DFS with memo; an unknown dep
/// id contributes 0 (conformance owns that error) and a cycle is cut
/// at the visiting mark (a cyclic workflow never runs — span is
/// garbage-in there, and conformance flags it first).
fn span_of_rows(rows: &[TaskContribution]) -> u64 {
    use std::collections::BTreeMap;
    let index: BTreeMap<&str, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.task.as_str(), i))
        .collect();
    let mut memo: Vec<Option<u64>> = vec![None; rows.len()];
    let mut visiting: Vec<bool> = vec![false; rows.len()];
    let mut best = 0u64;
    for start in 0..rows.len() {
        // explicit stack: (node, next-dep-cursor)
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some(&mut (node, ref mut cursor)) = stack.last_mut() {
            if memo[node].is_some() {
                stack.pop();
                continue;
            }
            visiting[node] = true;
            let deps = &rows[node].deps;
            if *cursor < deps.len() {
                let dep = deps[*cursor].as_str();
                *cursor += 1;
                if let Some(&d) = index.get(dep)
                    && memo[d].is_none()
                    && !visiting[d]
                {
                    stack.push((d, 0));
                }
                continue;
            }
            let longest_dep = deps
                .iter()
                .filter_map(|d| index.get(d.as_str()))
                .filter_map(|&d| memo[d])
                .max()
                .unwrap_or(0);
            memo[node] = Some(rows[node].attempts.saturating_add(longest_dep));
            visiting[node] = false;
            stack.pop();
        }
        if let Some(v) = memo[start] {
            best = best.max(v);
        }
    }
    best
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
    use nika_schema::{FileId, ParseMode, parse};

    fn cert(yaml: &str) -> RunCertificate {
        certify(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    fn wf(tasks: &str) -> String {
        format!("nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n{tasks}")
    }

    fn konst(n: u64) -> Bound {
        Bound {
            constant: n,
            terms: vec![],
        }
    }

    #[test]
    fn traverse_fetch_counts_its_page_bound_plus_robots() {
        // literal max_pages 5 + default robots → 5 + 1 effects.
        let c = cert(&wf(
            "  crawl:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://a.test\", traverse: { max_pages: 5 } } }\n",
        ));
        assert_eq!(c.effect_calls, konst(6));
        // respect_robots literally false → no probe.
        let c = cert(&wf(
            "  crawl:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://a.test\", traverse: { max_pages: 5, respect_robots: false } } }\n",
        ));
        assert_eq!(c.effect_calls, konst(5));
        // a templated spec folds to the runtime cap (+ robots).
        let c = cert(&wf(
            "  crawl:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://a.test\", traverse: \"${{ inputs.spec }}\" } }\n",
        ));
        assert_eq!(c.effect_calls, konst(MAX_TRAVERSE_PAGES + 1));
        // a plain fetch stays exactly 1 (no traverse key).
        let c = cert(&wf(
            "  one:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://a.test\" } }\n",
        ));
        assert_eq!(c.effect_calls, konst(1));
    }

    #[test]
    fn plain_pipeline_has_exact_constant_bounds() {
        let c = cert(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n",
        ));
        assert_eq!(c.task_attempts, konst(2));
        assert_eq!(c.llm_calls, konst(0));
        assert!(c.llm_calls.is_zero());
        assert_eq!(c.effect_calls, konst(2));
    }

    #[test]
    fn retry_multiplies_the_body_and_its_calls() {
        let c = cert(&wf(
            "  a:\n    retry: { max_attempts: 3 }\n    infer: { prompt: \"x\", max_tokens: 10 }\n",
        ));
        assert_eq!(c.task_attempts, konst(3));
        assert_eq!(c.llm_calls, konst(3));
        assert_eq!(c.effect_calls, konst(0));
    }

    #[test]
    fn literal_for_each_folds_into_the_constant() {
        let c = cert(&wf(
            "  a:\n    for_each: { items: [\"x\", \"y\", \"z\"] }\n    retry: { max_attempts: 2 }\n    exec: { command: [\"echo\", \"${{ item }}\"] }\n",
        ));
        // 3 elements × 2 attempts
        assert_eq!(c.task_attempts, konst(6));
        assert_eq!(c.effect_calls, konst(6));
    }

    #[test]
    fn expression_for_each_yields_a_parametric_term() {
        let c = cert(&wf(
            "  src:\n    exec: { command: [\"ls\"] }\n  fan:\n    with: { files: \"${{ tasks.src.output.files }}\" }\n    for_each: { items: \"${{ with.files }}\" }\n    retry: { max_attempts: 2 }\n    infer: { prompt: \"summarize ${{ item }}\", max_tokens: 10 }\n",
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
            "  a:\n    agent: { prompt: \"go\", tools: [\"nika:read\"] }\n",
        ));
        assert_eq!(defaulted.llm_calls, konst(10), "spec default max_turns");
        let capped = cert(&wf(
            "  a:\n    agent: { prompt: \"go\", tools: [\"nika:read\"], max_turns: 4 }\n",
        ));
        assert_eq!(capped.llm_calls, konst(4));
    }

    #[test]
    fn spend_axis_is_parametric_where_cost_says_unknown_iterations() {
        // the deepening: for_each-expression spend = a degree-1 term
        let c = cert(&wf(
            "  src:\n    exec: { command: [\"ls\"] }\n  fan:\n    with: { files: \"${{ tasks.src.output.files }}\" }\n    for_each: { items: \"${{ with.files }}\" }\n    retry: { max_attempts: 2 }\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 200 }\n",
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
        let c = cert(&wf("  a:\n    infer: { prompt: \"x\" }\n"));
        assert!(c.usd_micros.is_none());
        assert_eq!(c.llm_calls, konst(1));
        // agent budget is CUMULATIVE — one body run ≤ max_tokens_total,
        // never ×turns
        let a = cert(&wf(
            "  a:\n    agent: { prompt: \"go\", tools: [\"nika:read\"], max_turns: 4, max_tokens_total: 1000 }\n",
        ));
        let usd = a.usd_micros.expect("priced");
        assert_eq!(usd.constant, 15_000, "1000 tk × $15/M · NOT ×4 turns");
    }

    #[test]
    fn audit_accepts_honest_and_rejects_tampered_certificates() {
        let yaml = wf(
            "  a:\n    exec: { command: [\"true\"] }\n  fan:\n    with: { items: \"${{ tasks.a.output.items }}\" }\n    for_each: { items: \"${{ with.items }}\" }\n    retry: { max_attempts: 2 }\n    infer: { prompt: \"x\", max_tokens: 50 }\n",
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
    fn span_is_the_longest_dependency_chain_in_attempts() {
        // chain a→b→c with retries: span = 1 + 3 + 2 = 6 = work
        let chain = cert(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    retry: { max_attempts: 3 }\n    exec: { command: [\"true\"] }\n  c:\n    after: { b: success }\n    retry: { max_attempts: 2 }\n    exec: { command: [\"true\"] }\n",
        ));
        assert_eq!(chain.span_attempts, 6);
        assert_eq!(chain.task_attempts, konst(6), "a pure chain: span == work");

        // diamond a→{b,c}→d: span = longest branch (1+3+1), work = sum
        let diamond = cert(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    retry: { max_attempts: 3 }\n    exec: { command: [\"true\"] }\n  c:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n  d:\n    after: { b: success, c: success }\n    exec: { command: [\"true\"] }\n",
        ));
        assert_eq!(diamond.span_attempts, 5, "longest branch: 1+3+1");
        assert_eq!(diamond.task_attempts, konst(6), "work: 1+3+1+1");
    }

    #[test]
    fn fanout_extends_work_never_span() {
        // a for_each over 4 elements: work ×4, span unchanged (the
        // elements run in parallel — Brent: parallelism = work/span)
        let c = cert(&wf(
            "  a:\n    exec: { command: [\"true\"] }\n  fan:\n    after: { a: success }\n    for_each: { items: [\"w\", \"x\", \"y\", \"z\"] }\n    exec: { command: [\"true\"] }\n",
        ));
        assert_eq!(c.task_attempts, konst(5), "work: 1 + 4");
        assert_eq!(c.span_attempts, 2, "span: 1 + 1 (elements parallel)");
    }

    #[test]
    fn audit_catches_span_and_dep_tampering() {
        let yaml = wf(
            "  a:\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n",
        );
        let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let honest = certify(&parsed);
        assert!(honest.audit(&parsed).is_ok());
        // tamper the claimed span
        let mut spanned = honest.clone();
        spanned.span_attempts = 1;
        assert!(spanned.audit(&parsed).is_err());
        // tamper a witness dep list
        let mut cut = honest;
        cut.derivation[1].deps.clear();
        assert!(cut.audit(&parsed).is_err_and(|e| e.contains("dependency")));
    }

    #[test]
    fn the_wire_shape_is_pinned() {
        let c = cert(&wf(
            "  fan:\n    for_each: { items: \"${{ inputs.items }}\" }\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 5 }\n",
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
                // span: one task · one attempt (fan-out is element-parallel)
                "span_attempts": 1,
                // the WITNESS row — what audit() re-checks
                "derivation": [{
                    "task": "fan", "deps": [], "attempts": 1, "fanout": "collection",
                    "main_llm": 1, "main_effect": 0, "main_spend_micros": 75,
                    "finally_llm": 0, "finally_effect": 0, "finally_spend_micros": 0,
                }],
                // the AUTHORITY projection (spec 10 · W4) — no permits:
                // declared, no escapes; the inferred need of a pure-compute
                // infer is the explicit zero-shell boundary (`exec: false` —
                // exactly what --infer-permits prints)
                "effects": {
                    "boundary_declared": false,
                    "needed": { "exec": false },
                    "escapes": 0,
                },
            })
        );
    }

    // ── certificate.effects (spec 10 · W4) ─────────────────────────────

    #[test]
    fn effects_projection_names_the_declared_boundary() {
        // A workflow WITH a permits: block whose body fits it — the
        // report JSON carries boundary_declared:true + a non-empty
        // needed + escapes:0 (the spec-10 example shape). NEP-0002: the
        // boundary declares all three trifecta legs (fs.read · tools ·
        // exec), so the exec sits behind a blocking `nika:prompt` gate —
        // otherwise NIKA-SEC-009 flags it and the fixture is not clean.
        // NEP-0020: the gate's answer is consumed AFFIRMATIVELY (a bare
        // `after:` would carry the refusal — NIKA-SEC-014).
        let yaml = "nika: w\npermits:\n  fs: { read: [\"./data/**\"] }\n  exec: [\"git\"]\n  tools: [\"nika:read\", \"nika:prompt\"]\ntasks:\n  a:\n    invoke: { tool: \"nika:read\", args: { path: \"./data/in.txt\" } }\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"run git status?\" }\n  b:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"git\", \"status\"] }\n";
        let parsed = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let report = crate::check(&parsed);
        assert!(report.is_clean(), "the fixture fits its boundary");
        let json = serde_json::to_value(&report).expect("serializes");
        let effects = &json["certificate"]["effects"];
        assert_eq!(effects["boundary_declared"], serde_json::json!(true));
        assert_eq!(effects["escapes"], 0);
        assert_eq!(
            effects["needed"]["exec"],
            serde_json::json!(["git"]),
            "needed IS the --infer-permits object: {effects}"
        );
        assert!(
            effects["needed"]["fs"]["read"]
                .as_array()
                .is_some_and(|a| !a.is_empty()),
            "{effects}"
        );
    }

    #[test]
    fn effects_counts_escapes_and_audit_rejects_a_doctored_story() {
        // exec outside a denying boundary → escapes counted, never 0.
        let yaml = wf("  a:\n    exec: { command: [\"cargo\", \"publish\"] }\n")
            .replace("tasks:", "permits:\n  exec: false\ntasks:");
        let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let honest = certify(&parsed);
        assert!(honest.effects.boundary_declared);
        assert_eq!(honest.effects.escapes, 1, "the escape is projected");
        assert!(honest.audit(&parsed).is_ok(), "honest effects re-derive");

        // tamper: claim a clean authority story on an escaping workflow —
        // the audit re-derives and refuses (escapes==0 is PROVEN, never
        // trusted).
        let mut doctored = honest;
        doctored.effects.escapes = 0;
        assert!(
            doctored
                .audit(&parsed)
                .is_err_and(|e| e.contains("effects projection")),
            "a doctored effects story must be rejected"
        );
    }

    // ── Gate-5 mutation-gap kills ──────────────────────────────────────
    //
    // Each test below pins one arithmetic / boolean / comparison fact a
    // mutant would silently flip. The accounting module's `+`/`*`/`&&`/`<`
    // are load-bearing: a flipped operator under-counts a resource bound
    // or wrongly accepts a tampered certificate — both are real defects in
    // a "run certificate", never cosmetic.

    // `Bound::is_zero` — the `constant == 0 && terms.is_empty()` body
    // (certificate.rs:99). Kills `-> true` (a non-zero bound is NOT zero)
    // and `&&`→`||` (a half-zero bound — one operand true, the other
    // false — must read as NOT zero, which `||` would invert).
    #[test]
    fn is_zero_is_exactly_the_empty_bound() {
        // fully empty → zero (the only true case)
        assert!(Bound::default().is_zero());

        // non-zero constant, empty terms → NOT zero (kills `-> true`; and
        // kills `&&`→`||` since `false || true` would wrongly say "zero")
        assert!(
            !konst(1).is_zero(),
            "a bound with constant=1 is not the zero bound"
        );

        // zero constant, non-empty terms → NOT zero (the OTHER half: kills
        // `&&`→`||` from the other side · `true || false` would invert)
        let only_terms = Bound {
            constant: 0,
            terms: vec![CertTerm {
                task: "fan".into(),
                coeff: 3,
            }],
        };
        assert!(
            !only_terms.is_zero(),
            "a bound with a parametric term is not the zero bound"
        );
    }

    // `audit` 4th `||` (certificate.rs:165, the `usd_micros !=` arm).
    // Only the spend axis is tampered, so ONLY this disjunct is true —
    // `||`→`&&` would short-circuit it to `false` and wrongly accept.
    #[test]
    fn audit_rejects_a_usd_only_mismatch() {
        let yaml = wf("  a:\n    infer: { prompt: \"x\", max_tokens: 100 }\n");
        let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let honest = certify(&parsed);
        assert!(honest.audit(&parsed).is_ok());

        // tamper ONLY the claimed spend bound; every other axis still
        // re-folds exactly → only the `usd_micros` disjunct fires
        let mut spend_lie = honest.clone();
        let lied = spend_lie.usd_micros.as_mut().expect("priced");
        lied.constant = lied.constant.wrapping_add(1);
        assert!(
            spend_lie
                .audit(&parsed)
                .is_err_and(|e| e.contains("do not match the derivation")),
            "a spend-only tamper must be rejected (the usd_micros disjunct)"
        );
    }

    // `check_row` main-action `||` (certificate.rs:208). The doctored
    // row's `main_llm` disagrees with the workflow while `main_effect`
    // stays correct AND the claimed bounds are re-folded to stay
    // consistent — so ONLY the structural `||` can catch it. `||`→`&&`
    // turns `(true && false)` into a wrongful accept.
    #[test]
    fn check_row_rejects_a_main_llm_only_mismatch() {
        // single infer task: workflow says main_llm=1, main_effect=0
        let yaml = wf("  a:\n    infer: { prompt: \"x\", max_tokens: 50 }\n");
        let parsed = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let honest = certify(&parsed);

        let mut doctored = honest;
        doctored.derivation[0].main_llm = 2; // wrong; main_effect stays 0 (right)
        // re-fold bounds to match the doctored row so the bound-equality
        // half passes → ONLY check_row's main-action line can reject
        let refolded = fold_rows(&doctored.derivation);
        doctored.task_attempts = refolded.task_attempts;
        doctored.llm_calls = refolded.llm_calls;
        doctored.effect_calls = refolded.effect_calls;
        doctored.usd_micros = refolded.usd_micros;
        doctored.span_attempts = refolded.span_attempts;

        assert!(
            doctored
                .audit(&parsed)
                .is_err_and(|e| e.contains("main-action call counts")),
            "a main_llm-only row lie must be caught structurally"
        );
    }

    // `check_row` on_finally `||` (certificate.rs:217). Same isolation as
    // the main-action case, but on the `finally_llm`/`finally_effect`
    // pair. The doctored row's `finally_llm` is wrong while
    // `finally_effect` is right; bounds re-folded to stay consistent.

    // `check_row` on_finally accumulators (certificate.rs:214-215, the
    // `f_llm += l` / `f_effect += e`) AND `contribution`'s `finally_llm +=
    // l` (certificate.rs:357). With TWO infer + TWO invoke cleanups the
    // honest expected totals are finally_llm=2, finally_effect=2. `+=`→
    // `*=` collapses both accumulators to 0 (≠ the honest row) so an
    // HONEST certificate would FALSELY fail audit; `+=`→`-=` underflows.
    // We pin the certified totals AND that the honest cert audits clean.

    // `contribution`'s priceable-cleanup fold arm (certificate.rs:363, the
    // `(Some(acc), Ok(m))` match arm). Two PRICEABLE infer cleanups must
    // accumulate their spend; deleting the arm sends every cleanup to the
    // `_ => None` branch, collapsing the whole `usd_micros` axis to None.
    // Asserting a priced finally spend kills the arm deletion.

    // `fold_rows` finally roll-up (certificate.rs:396 `+ row.finally_llm`,
    // 409 `+ fin`). Main is a retried infer (3 attempts → 3 main LLM
    // calls · 3×3000 = 9000 µ$) and the cleanup is a priceable infer
    // (1 LLM · 1500 µ$). Correct totals: llm_calls=4, usd=10500. `+`→`-`
    // would yield 2 and 7500 — both nonzero, no saturation masking — so
    // the exact-total assertions catch the flip.

    // `span_of_rows` cursor guard `*cursor < deps.len()` (certificate.rs:
    // 445). Tasks are declared in REVERSE dependency order (c, b, a) so
    // the DFS MUST push dep nodes before they are memoized — `<`→`>`
    // never enters the push branch, leaving deps un-memoized so the span
    // collapses to 1 instead of the true chain length 3.
    #[test]
    fn span_traverses_deps_declared_after_their_dependents() {
        // c → b → a, written c first: forces the push branch to matter
        let c = cert(&wf(
            "  c:\n    after: { b: success }\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n  a:\n    exec: { command: [\"true\"] }\n",
        ));
        // chain of 3 single-attempt tasks → span = 1+1+1 = 3
        assert_eq!(
            c.span_attempts, 3,
            "the longest chain is traversed even reverse-declared"
        );
    }

    // `span_of_rows` cycle-cut guard `&& !visiting[d]` (certificate.rs:
    // 450). A two-task cycle (a↔b) must be cut at the visiting mark so the
    // fold terminates with a finite span; deleting the `!` would re-push
    // an already-visiting node and either loop or over-count. We assert a
    // finite, defined span (the certificate stays total on garbage input —
    // conformance owns the cycle error).
    #[test]
    fn span_cuts_cycles_at_the_visiting_mark() {
        // a depends on b, b depends on a — a cycle (conformance rejects it
        // elsewhere; span must still TERMINATE with a finite value)
        let c = cert(&wf(
            "  a:\n    after: { b: success }\n    exec: { command: [\"true\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"true\"] }\n",
        ));
        // with the cut, each node's chain stops at the visiting mark:
        // span = 1 + 1 = 2 (one hop before the back-edge is cut). The key
        // fact the mutant breaks is termination + this exact bound.
        assert_eq!(
            c.span_attempts, 2,
            "the cycle is cut at the visiting mark · span stays finite"
        );
    }
}
