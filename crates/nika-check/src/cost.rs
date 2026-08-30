// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cost ceiling — the spend envelope, computed statically (ADR-092 #5).
//!
//! For each `infer:`/`agent:` task with a declared output bound
//! (`max_tokens` · `max_tokens_total`), the per-call ceiling is
//! `tokens × output_price_per_million / 1e6`, priced from
//! `nika-catalog`. A task with NO declared token bound is **unbounded** —
//! the most likely « why did this cost $200 » surprise — and is reported
//! as such (the foot-gun is named, not hidden).
//!
//! The envelope is a STRUCTURAL interval — two ceilings, not one ·
//!
//! - **worst path** — every `when:` gate opens · every `retry:` attempt
//!   fires · full `for_each` fan-out → `tokens × n × attempts × price`.
//! - **cheapest path** — every gate closes (`when:` tasks skip · $0) and
//!   everything succeeds first-try → the UNAVOIDABLE budget exposure.
//!
//! Both ends are ceilings at the DECLARED per-call budget: a model may
//! emit fewer tokens than `max_tokens`, so the cheapest-path figure is
//! the floor of your *exposure*, not of the actual spend.

use nika_schema::raw::{ForEachValue, RawAction, RawWorkflow};

/// Per-task cost envelope.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[non_exhaustive]
pub struct TaskCost {
    /// The task id.
    pub task: String,
    /// The resolved `<provider>/<model>` (task override → workflow default).
    pub model: Option<String>,
    /// The token bound used (`max_tokens` / `max_tokens_total`).
    pub max_tokens: Option<u64>,
    /// `for_each` iteration multiplier · 1 for a plain task · N for a
    /// literal `for_each: [..N..]` (the worst-case spend is N× the per-call
    /// cost). An expression-source `for_each` has an unknown count and
    /// makes the task unbounded ([`UnboundedReason::UnknownIterations`]).
    pub iterations: u64,
    /// `retry:` attempt multiplier — `max_attempts` (1 when no `retry:`).
    /// Every attempt can spend the full per-call budget, so the worst
    /// case multiplies (ignoring it silently UNDERCOUNTED the ceiling).
    pub attempts: u64,
    /// Whether a `when:` gate can skip this task entirely (the cheapest
    /// structural path spends $0 on it).
    pub gated: bool,
    /// Cheapest-path USD — gates closed, first-try success, at the
    /// declared budget (`None` exactly when [`TaskCost::usd`] is).
    pub min_path_usd: Option<f64>,
    /// Worst-case USD (× `iterations` × `attempts`) · `None` = unbounded
    /// tokens, unknown iterations, OR the model has no catalog price
    /// (all reported, never silently zero).
    pub usd: Option<f64>,
    /// Why `usd` is `None`, when it is.
    pub unbounded_reason: Option<UnboundedReason>,
}

/// Why a task's cost could not be bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub enum UnboundedReason {
    /// No `max_tokens` / `max_tokens_total` — the token spend is unbounded.
    NoTokenLimit,
    /// The model string did not resolve to a catalog price.
    NoPrice,
    /// A `for_each:` over an EXPRESSION source — the iteration count (and
    /// thus the fan-out cost multiplier) is not statically known.
    UnknownIterations,
}

/// The whole-workflow cost envelope.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[non_exhaustive]
pub struct CostCeiling {
    /// Per-task breakdown (only `infer:`/`agent:` tasks).
    pub tasks: Vec<TaskCost>,
    /// Σ of the bounded WORST-path task costs in USD (the ceiling).
    pub bounded_total_usd: f64,
    /// Σ of the bounded CHEAPEST-path task costs — gates closed,
    /// first-try success (the unavoidable budget exposure).
    pub min_path_total_usd: f64,
    /// `true` when at least one inference task is unbounded — the total is
    /// a FLOOR, not a ceiling, and the report says so.
    pub has_unbounded: bool,
    /// Composed children's priced contribution (spec 14 · only
    /// [`crate::check_composed`] fills this — the reader-less [`crate::check`]
    /// never loads files, so the pure half stays child-blind BY DESIGN).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub composed: Vec<ComposedCost>,
}

/// One composed child's priced contribution — the call's task/target and
/// the child's OWN ceiling, folded into the parent's (the parent WILL make
/// the call: its unavoidable exposure and its ceiling both carry the
/// child's, and uncapped spend propagates).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[non_exhaustive]
pub struct ComposedCost {
    /// The calling task's id.
    pub task: String,
    /// The child target as written at the call site.
    pub target: String,
    /// The child's unavoidable spend (folded into the parent's floor).
    pub min_path_total_usd: f64,
    /// The child's bounded worst-path spend (folded into the parent's ceiling).
    pub bounded_total_usd: f64,
    /// The child has uncapped spend of its own — propagates to the parent.
    pub has_unbounded: bool,
}

impl CostCeiling {
    /// Fold one composed child's ceiling into this envelope (spec 14 · the
    /// 2026-07-29 composition finding: a parent whose child explained
    /// `≤$0.0011` alone printed `$0 model spend`). The calling task's OWN
    /// multipliers apply across the wall — the same law as the per-task
    /// arm of [`ceiling`]: a `for_each` fan makes N calls unconditionally
    /// (floor ×N), every retry attempt can re-run the whole child
    /// (ceiling ×N×attempts), a `when:` gate's cheapest path never calls,
    /// and an unknown-count fan-out is unbounded and priced at nothing.
    pub(crate) fn fold_composed(
        &mut self,
        task: String,
        target: String,
        child: &CostCeiling,
        iterations: Option<u64>,
        attempts: u64,
        gated: bool,
    ) {
        let (cheapest, worst) = match iterations {
            None => (0.0, 0.0),
            Some(n) => {
                #[allow(clippy::cast_precision_loss)]
                let (n, a) = (n as f64, attempts.max(1) as f64);
                (
                    if gated {
                        0.0
                    } else {
                        n * child.min_path_total_usd
                    },
                    n * a * child.bounded_total_usd,
                )
            }
        };
        self.min_path_total_usd += cheapest;
        self.bounded_total_usd += worst;
        let unbounded = child.has_unbounded || iterations.is_none();
        self.has_unbounded |= unbounded;
        self.composed.push(ComposedCost {
            task,
            target,
            min_path_total_usd: cheapest,
            bounded_total_usd: worst,
            has_unbounded: unbounded,
        });
    }
}

/// Compute the cost envelope for a workflow.
#[must_use]
pub(super) fn ceiling(wf: &RawWorkflow) -> CostCeiling {
    let default_model = wf.model.as_ref().map(|m| m.value.clone());
    let mut tasks = Vec::new();
    let mut bounded_total_usd = 0.0;
    let mut min_path_total_usd = 0.0;
    let mut has_unbounded = false;

    for task in &wf.tasks {
        let (model_override, max_tokens) = match &task.value.action {
            RawAction::Infer(a) => (
                a.model.as_ref().map(|m| m.value.clone()),
                a.max_tokens.as_ref().map(|t| u64::from(t.value)),
            ),
            RawAction::Agent(a) => (
                a.model.as_ref().map(|m| m.value.clone()),
                a.max_tokens_total.as_ref().map(|t| t.value),
            ),
            // exec/invoke spend nothing on inference
            RawAction::Exec(_) | RawAction::Invoke(_) => continue,
            #[allow(
                clippy::unreachable,
                reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
            )]
            other => unreachable!("unknown action: {other:?}"),
        };
        let model = model_override.or_else(|| default_model.clone());

        // `for_each` fan-out: a literal list is N calls (known multiplier);
        // an expression source is an unknown count → unbounded.
        let iterations = match task.value.for_each.as_ref().map(|f| &f.value) {
            None => Some(1),
            Some(ForEachValue::List(arr)) => Some(arr.as_array().map_or(1, Vec::len) as u64),
            // An expression source is unknown EXCEPT when it is a bare
            // `${{ <authority>.<name> }}` over a literal array — that count is
            // statically known, so the cost is bounded (parity with a List).
            Some(ForEachValue::Expression(expr)) => static_vars_array_len(wf, expr),
            #[allow(
                clippy::unreachable,
                reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
            )]
            other => unreachable!("unknown for_each form: {other:?}"),
        };
        // every retry attempt can spend the full per-call budget
        let attempts = task
            .value
            .retry
            .as_ref()
            .map_or(1, |r| u64::from(r.value.max_attempts.max(1)));
        let gated = task.value.when.is_some();

        let (usd, min_path_usd, unbounded_reason) = match (max_tokens, iterations) {
            (None, _) => (None, None, Some(UnboundedReason::NoTokenLimit)),
            (Some(_), None) => (None, None, Some(UnboundedReason::UnknownIterations)),
            (Some(tokens), Some(n)) => match model.as_deref().and_then(output_price_per_million) {
                Some(price) => {
                    #[allow(clippy::cast_precision_loss)]
                    let per_call = (tokens as f64) * price / 1_000_000.0;
                    #[allow(clippy::cast_precision_loss)]
                    let worst = per_call * (n as f64) * (attempts as f64);
                    #[allow(clippy::cast_precision_loss)]
                    let cheapest = if gated { 0.0 } else { per_call * (n as f64) };
                    bounded_total_usd += worst;
                    min_path_total_usd += cheapest;
                    (Some(worst), Some(cheapest), None)
                }
                None => (None, None, Some(UnboundedReason::NoPrice)),
            },
        };
        if usd.is_none() {
            has_unbounded = true;
        }
        tasks.push(TaskCost {
            task: task.value.id.value.clone(),
            model,
            max_tokens,
            iterations: iterations.unwrap_or(0),
            attempts,
            gated,
            min_path_usd,
            usd,
            unbounded_reason,
        });
    }

    CostCeiling {
        tasks,
        bounded_total_usd,
        min_path_total_usd,
        has_unbounded,
        composed: Vec::new(),
    }
}

/// Output price (USD per million tokens) for a `<provider>/<model>` string.
///
/// Resolves through the catalog's ONE resolved-string lookup
/// (`find_pricing_for`): a `provider/` prefix scopes the search so a bare
/// cross-provider substring match (`"my-gpt-4-finetune".contains("gpt-4")`)
/// can never misprice an unrelated model at another provider's rate; a
/// bare model id falls back to the unscoped lookup. The runtime's
/// task-completion pricing resolves through the SAME function — the floor
/// and the actual can never disagree on which row prices a model. Returns
/// `None` for local/unknown models — sovereign zero-price models are
/// « unpriced », never « free ».
pub(super) fn output_price_per_million(model: &str) -> Option<f64> {
    // The mock plane is the engine's own rehearsal: the echo never leaves
    // the process — no tokens are bought, no API is reached. A PROVEN
    // zero, priced $0.00 (A-02: the ⚠ COST UNBOUNDED on the first
    // artifact every beginner checks taught that the free rehearsal was
    // dangerous — and the run card already reports the truth, `$0.00`).
    // The sovereign locals stay unpriced, never free: their watts are
    // real; mock has no model at all.
    if model == "mock" || model.starts_with("mock/") {
        return Some(0.0);
    }
    nika_catalog::find_pricing_for(model).map(|p| p.output_per_million)
}

/// A `for_each:` count that is statically known: the expression resolves
/// through the ONE shared resolver ([`crate::static_literal_of`] — a bare
/// `${{ <authority>.<name> }}` whose declaration carries a literal) to an
/// ARRAY literal. Returns `None` for anything else — a task-output ref, a
/// computed/navigated expression, a typed input with no default, or a
/// non-array value — which stays an unknown count (`UnknownIterations`).
pub(super) fn static_vars_array_len(wf: &RawWorkflow, expr: &str) -> Option<u64> {
    crate::static_literal_of(wf, expr)?
        .as_array()
        .map(|a| a.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn ceiling_of(yaml: &str) -> CostCeiling {
        ceiling(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn bounded_infer_is_priced() {
        let c = ceiling_of(
            "\
nika: priced
model: anthropic/claude-sonnet-4-6
tasks:
  ask:
    infer: { prompt: \"hi\", max_tokens: 1000 }
",
        );
        assert_eq!(c.tasks.len(), 1);
        assert!(!c.has_unbounded, "a token-bounded priced task is bounded");
        assert!(c.bounded_total_usd > 0.0, "sonnet has a price");
        assert_eq!(c.tasks[0].max_tokens, Some(1000));
    }

    /// B20 / issue 1297: `gemini/gemini-2.5-flash` with a token bound is a
    /// priced cloud ceiling, never `NoPrice` / `est unbounded`. Check JSON
    /// serializes that honesty (`priced: true`, no `NoPrice` reason).
    #[test]
    fn gemini_flash_with_max_tokens_is_priced_not_unbounded() {
        let yaml = "\
nika: gemini-b20
model: gemini/gemini-2.5-flash
permits: {}
tasks:
  ping:
    infer: { prompt: \"PONG\", max_tokens: 256 }
";
        let c = ceiling_of(yaml);
        assert_eq!(c.tasks.len(), 1, "{c:?}");
        assert!(
            !c.has_unbounded,
            "priced flash + max_tokens is a ceiling, not unbounded: {c:?}"
        );
        assert_eq!(c.tasks[0].unbounded_reason, None);
        assert!(
            c.tasks[0].usd.is_some_and(|usd| usd > 0.0),
            "output-token ceiling is a real snapshot rate: {c:?}"
        );
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let report = crate::check(&wf);
        let ep = report
            .data_journey
            .model_endpoints
            .iter()
            .find(|e| e.task == "ping")
            .expect("flash endpoint");
        assert!(ep.priced, "check JSON priced: true, got {ep:?}");
        assert_eq!(ep.locus, crate::EndpointLocus::Cloud);
        let json = serde_json::to_string(&report).expect("report json");
        assert!(
            !json.contains("NoPrice"),
            "check JSON must not carry NoPrice for a priced cloud seat: {json}"
        );
        assert!(
            !json.contains("est unbounded"),
            "check JSON must not print est unbounded for priced flash: {json}"
        );
    }

    #[test]
    fn for_each_over_a_literal_vars_array_is_bounded() {
        // NEW-5: a static `${{ const.items }}` over a literal array has a known
        // count → bounded cost (parity with an inline list), not UnknownIterations.
        let c = ceiling_of(
            "\
nika: fe-vars
model: anthropic/claude-sonnet-4-6
const:
  items: [\"a\", \"b\", \"c\"]
tasks:
  fan:
    for_each: { items: \"${{ const.items }}\" }
    infer: { prompt: \"x\", max_tokens: 100 }
",
        );
        assert!(
            !c.has_unbounded,
            "literal const-array count is statically known"
        );
        assert_eq!(c.tasks[0].iterations, 3);
        assert!(c.tasks[0].usd.is_some(), "bounded → priced");
    }

    #[test]
    fn for_each_over_a_typed_var_without_default_stays_unknown() {
        // No literal at check time → still UnknownIterations (correct).
        let c = ceiling_of(
            "\
nika: fe-typed
model: anthropic/claude-sonnet-4-6
inputs:
  items: { type: { array: string }, required: true }
tasks:
  fan:
    for_each: { items: \"${{ inputs.items }}\" }
    infer: { prompt: \"x\", max_tokens: 100 }
",
        );
        assert!(c.has_unbounded);
        assert_eq!(
            c.tasks[0].unbounded_reason,
            Some(UnboundedReason::UnknownIterations)
        );
    }

    #[test]
    fn for_each_over_a_typed_array_with_default_is_bounded() {
        // The typed-array-with-literal-`default:` path also resolves to a count.
        let c = ceiling_of(
            "\
nika: fe-typed-default
model: anthropic/claude-sonnet-4-6
inputs:
  items: { type: { array: string }, default: [\"a\", \"b\"] }
tasks:
  fan:
    for_each: { items: \"${{ inputs.items }}\" }
    infer: { prompt: \"x\", max_tokens: 100 }
",
        );
        assert!(!c.has_unbounded, "typed-array default is a known count");
        assert_eq!(c.tasks[0].iterations, 2);
    }

    #[test]
    fn missing_max_tokens_is_unbounded() {
        let c = ceiling_of(
            "\
nika: unbounded
model: anthropic/claude-sonnet-4-6
tasks:
  ask:
    infer: { prompt: \"hi\" }
",
        );
        assert!(c.has_unbounded, "no max_tokens = unbounded — the foot-gun");
        assert_eq!(
            c.tasks[0].unbounded_reason,
            Some(UnboundedReason::NoTokenLimit)
        );
        assert_eq!(c.tasks[0].usd, None);
    }

    #[test]
    fn local_model_is_unpriced_not_free() {
        let c = ceiling_of(
            "\
nika: local
model: ollama/llama3
tasks:
  ask:
    infer: { prompt: \"hi\", max_tokens: 1000 }
",
        );
        assert!(
            c.has_unbounded,
            "local has no cloud price → reported, not $0"
        );
        assert_eq!(c.tasks[0].unbounded_reason, Some(UnboundedReason::NoPrice));
    }

    /// A-02 · the mock plane is a PROVEN zero, never an unknown: the
    /// echo never leaves the process, so a capped mock task is bounded
    /// at $0.00 — the ⚠ COST UNBOUNDED on the first artifact every
    /// beginner checks was teaching that the free rehearsal is
    /// dangerous. The sister law stays: locals are unpriced (their
    /// watts are real — `local_model_is_unpriced_not_free` above).
    #[test]
    fn mock_is_a_proven_zero_never_unpriced() {
        let c = ceiling_of(
            "\
nika: rehearsal
model: mock/echo
tasks:
  ask:
    infer: { prompt: \"hi\", max_tokens: 1000 }
",
        );
        assert!(!c.has_unbounded, "mock is $0 by construction: {c:?}");
        assert_eq!(c.tasks[0].unbounded_reason, None);
        assert_eq!(c.tasks[0].usd, Some(0.0));
        assert_eq!(c.bounded_total_usd, 0.0);
        // The cap habit still teaches: an UNCAPPED mock task keeps the
        // NoTokenLimit report (true — and the habit survives the swap
        // to a real model).
        let uncapped = ceiling_of(
            "\
nika: rehearsal-uncapped
model: mock/echo
tasks:
  ask:
    infer: { prompt: \"hi\" }
",
        );
        assert_eq!(
            uncapped.tasks[0].unbounded_reason,
            Some(UnboundedReason::NoTokenLimit)
        );
    }

    #[test]
    fn exec_and_invoke_cost_nothing() {
        let c = ceiling_of(
            "\
nika: noinfer
tasks:
  sh:
    exec: { command: [\"true\"] }
  tool:
    invoke: { tool: \"nika:read\", args: { path: \"x\" } }
",
        );
        assert!(c.tasks.is_empty(), "no inference tasks → no cost rows");
        assert_eq!(c.bounded_total_usd, 0.0);
    }

    #[test]
    fn retry_multiplies_the_worst_path_not_the_cheapest() {
        // Ignoring retry: silently UNDERCOUNTED the ceiling — 3 attempts
        // can each spend the full budget; first-try success spends 1×.
        let plain = ceiling_of(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
        );
        let retried = ceiling_of(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    retry: { max_attempts: 3 }\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
        );
        assert_eq!(retried.tasks[0].attempts, 3);
        let one = plain.bounded_total_usd;
        assert!(
            (retried.bounded_total_usd - one * 3.0).abs() < 1e-9,
            "worst path is 3×: {} vs {}",
            retried.bounded_total_usd,
            one * 3.0
        );
        assert!(
            (retried.min_path_total_usd - one).abs() < 1e-9,
            "cheapest path is first-try 1×"
        );
    }

    #[test]
    fn when_gated_task_zeroes_the_cheapest_path() {
        let c = ceiling_of(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n  t:\n    with: { a_status: \"${{ tasks.a.status }}\" }\n    when: ${{ with.a_status == 'success' }}\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
        );
        assert!(c.tasks[0].gated);
        assert_eq!(c.tasks[0].min_path_usd, Some(0.0), "gate closed → $0");
        assert!(
            c.bounded_total_usd > 0.0 && c.min_path_total_usd == 0.0,
            "interval is [0, worst]"
        );
    }

    #[test]
    fn ungated_unretried_task_has_a_degenerate_interval() {
        let c = ceiling_of(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
        );
        assert!(
            (c.min_path_total_usd - c.bounded_total_usd).abs() < 1e-12,
            "no structure to vary → min == max"
        );
    }

    #[test]
    fn agent_uses_max_tokens_total() {
        let c = ceiling_of(
            "\
nika: agentcost
model: anthropic/claude-sonnet-4-6
tasks:
  loop:
    agent: { prompt: \"go\", max_tokens_total: 50000 }
",
        );
        assert_eq!(c.tasks[0].max_tokens, Some(50000));
        assert!(!c.has_unbounded && c.bounded_total_usd > 0.0);
    }
}

#[cfg(test)]
mod regression {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn ceiling_of(yaml: &str) -> CostCeiling {
        ceiling(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn cross_provider_substring_does_not_misprice() {
        // `ollama/my-gpt-4-finetune` must NOT pick up the OpenAI gpt-4 rate
        // via a bare `.contains("gpt-4")` — provider-scoped lookup fails on
        // the unknown local model → reported NoPrice, not mispriced.
        let c = ceiling_of(
            "\
nika: w
model: ollama/my-gpt-4-finetune
tasks:
  t:
    infer: { prompt: \"hi\", max_tokens: 1000 }
",
        );
        assert!(
            c.has_unbounded,
            "unknown local model is unpriced, not mispriced"
        );
        assert_eq!(c.tasks[0].unbounded_reason, Some(UnboundedReason::NoPrice));
    }
}

#[cfg(test)]
mod for_each_fanout {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn ceiling_of(yaml: &str) -> CostCeiling {
        ceiling(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn literal_for_each_multiplies_cost_by_count() {
        // 5-element literal list = 5× the per-call cost (was a 5× undercount).
        let single = ceiling_of(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
        );
        let batch = ceiling_of(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    for_each: { items: [1, 2, 3, 4, 5] }\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 1000 }\n",
        );
        assert_eq!(batch.tasks[0].iterations, 5);
        let one = single.bounded_total_usd;
        assert!(
            (batch.bounded_total_usd - one * 5.0).abs() < 1e-9,
            "5 iterations cost 5× a single call: {} vs {}",
            batch.bounded_total_usd,
            one * 5.0
        );
    }

    #[test]
    fn expression_for_each_is_unbounded() {
        // ${{ inputs.items }} source → unknown iteration count → unbounded.
        let c = ceiling_of(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ninputs: { items: { type: string, required: true } }\ntasks:\n  t:\n    for_each: { items: \"${{ inputs.items }}\" }\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 1000 }\n",
        );
        assert!(c.has_unbounded);
        assert_eq!(
            c.tasks[0].unbounded_reason,
            Some(UnboundedReason::UnknownIterations)
        );
    }
}

#[cfg(test)]
mod ceiling_arithmetic {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn ceiling_of(yaml: &str) -> CostCeiling {
        ceiling(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    fn single_task(max_tokens: u32) -> CostCeiling {
        ceiling_of(&format!(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer: {{ prompt: \"x\", max_tokens: {max_tokens} }}\n",
        ))
    }

    #[test]
    fn per_call_cost_stays_a_small_fraction_of_a_token_budget() {
        // per_call = tokens × price / 1e6. For a 100-token sonnet call the
        // real figure is sub-cent. The `/`→`%` mutant (tokens×price % 1e6) and
        // the `/`→`*` mutant (tokens×price×1e6) on line 137 both blow the
        // figure past $1 — pinned by this upper bound.
        let c = single_task(100);
        assert!(c.bounded_total_usd > 0.0, "sonnet is priced");
        assert!(
            c.bounded_total_usd < 1.0,
            "100 tokens ÷ 1e6 is a fraction of a cent, got {}",
            c.bounded_total_usd
        );
    }

    #[test]
    fn cost_ordering_follows_price_ordering() {
        // per_call = tokens × price / 1e6 ∝ price. The `*`→`/` mutant on line
        // 137 (tokens ÷ price) makes per_call ∝ 1/price, INVERTING which model
        // costs more. Compare two models at the same token budget: the cost
        // order must match the price order. Prices are fetched live, so the
        // test is magnitude-agnostic — it only needs the two to differ.
        let cheap_model = "anthropic/claude-sonnet-4-6";
        let dear_model = "openai/gpt-4o-mini";
        let p_cheap = output_price_per_million(cheap_model).expect("sonnet priced");
        let p_dear = output_price_per_million(dear_model).expect("gpt-4o-mini priced");
        assert!(
            (p_cheap - p_dear).abs() > f64::EPSILON,
            "the two models must have distinct output prices to order them"
        );

        let cost_of = |model: &str| {
            let yaml = format!(
                "nika: w\nmodel: {model}\ntasks:\n  t:\n    infer: {{ prompt: \"x\", max_tokens: 1000 }}\n",
            );
            ceiling_of(&yaml).bounded_total_usd
        };
        let usd_cheap = cost_of(cheap_model);
        let usd_dear = cost_of(dear_model);
        assert_eq!(
            usd_cheap > usd_dear,
            p_cheap > p_dear,
            "cost order must follow price order ({cheap_model}={p_cheap} ${usd_cheap}, {dear_model}={p_dear} ${usd_dear})"
        );
    }

    #[test]
    fn ungated_unretried_fanout_has_equal_path_costs() {
        // For an ungated, unretried task: worst = per_call × n (line 139) and
        // cheapest = per_call × n (line 141) — equal. The `*`→`/` mutant on
        // line 141 makes cheapest = per_call / n; with n = 2 that diverges from
        // worst (the degenerate n = 1 case cannot see it, which is why it
        // survived). Both ends are still priced (sonnet), so they compare.
        let c = ceiling_of(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    for_each: { items: [1, 2] }\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 1000 }\n",
        );
        assert_eq!(c.tasks[0].iterations, 2, "two-element list = 2 iterations");
        assert!(c.bounded_total_usd > 0.0, "priced worst path");
        assert!(
            (c.min_path_total_usd - c.bounded_total_usd).abs() < 1e-12,
            "ungated unretried: cheapest == worst (both per_call × 2), got {} vs {}",
            c.min_path_total_usd,
            c.bounded_total_usd
        );
    }
}

#[cfg(test)]
mod static_vars_array_len_unit {
    use super::*;
    use nika_schema::raw::RawWorkflow;
    use nika_schema::source::{Span, Spanned};
    use nika_schema::types::VarDecl;

    fn var(name: &str, value: serde_json::Value) -> (Spanned<String>, VarDecl) {
        (
            Spanned::new(name.to_owned(), Span::default()),
            VarDecl::Untyped(value),
        )
    }

    #[test]
    fn resolves_the_named_var_not_a_different_one() {
        // The `==`→`!=` mutant on the `find(|(k, _)| k.value == name)` predicate
        // would match the FIRST var whose name is NOT the target — here `other`
        // (len 1) instead of `items` (len 3). `items` is listed first so the
        // `!=` mutant skips it and resolves the wrong array → 1 not 3.
        let mut wf = RawWorkflow::new();
        wf.consts
            .push(var("items", serde_json::json!(["a", "b", "c"])));
        wf.consts.push(var("other", serde_json::json!(["x"])));
        assert_eq!(
            static_vars_array_len(&wf, "${{ const.items }}"),
            Some(3),
            "must resolve `items` (len 3), never a different entry"
        );
    }

    #[test]
    fn rejects_a_non_alphanumeric_var_name() {
        // The `||`→`&&` mutant on the guard makes the empty/bad-char test
        // require BOTH conditions; a non-empty name with a `-` then falls
        // through and (because the var exists) resolves a count instead of the
        // correct `None`. The bare-name guard must reject `bad-name` outright.
        let mut wf = RawWorkflow::new();
        wf.consts
            .push(var("bad-name", serde_json::json!(["one", "two"])));
        assert_eq!(
            static_vars_array_len(&wf, "${{ const.bad-name }}"),
            None,
            "a hyphenated name is not a bare `const.<ident>` — reject, never count"
        );
    }
}
