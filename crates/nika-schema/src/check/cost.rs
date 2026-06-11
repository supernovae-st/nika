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

use crate::raw::{ForEachValue, RawAction, RawWorkflow};

/// Per-task cost envelope.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq)]
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
        };
        let model = model_override.or_else(|| default_model.clone());

        // `for_each` fan-out: a literal list is N calls (known multiplier);
        // an expression source is an unknown count → unbounded.
        let iterations = match task.value.for_each.as_ref().map(|f| &f.value) {
            None => Some(1),
            Some(ForEachValue::List(arr)) => Some(arr.as_array().map_or(1, Vec::len) as u64),
            Some(ForEachValue::Expression(_)) => None,
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
    }
}

/// Output price (USD per million tokens) for a `<provider>/<model>` string.
///
/// Resolves through the catalog's PROVIDER-SCOPED lookup
/// (`find_pricing_scoped`) — a bare cross-provider substring match
/// (`"my-gpt-4-finetune".contains("gpt-4")`) would misprice an unrelated
/// model at another provider's rate, the exact collision the catalog
/// warns about. With no `provider/` prefix we fall back to the unscoped
/// lookup (a bare model id). Returns `None` for local/unknown models —
/// sovereign zero-price models are « unpriced », never « free ».
fn output_price_per_million(model: &str) -> Option<f64> {
    let pricing = match model.split_once('/') {
        Some((provider, name)) => nika_catalog::find_pricing_scoped(provider, name),
        None => nika_catalog::find_pricing(model),
    };
    pricing.map(|p| p.output_per_million)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn ceiling_of(yaml: &str) -> CostCeiling {
        ceiling(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn bounded_infer_is_priced() {
        let c = ceiling_of(
            "\
nika: v1
workflow: priced
model: anthropic/claude-sonnet-4-6
tasks:
  - id: ask
    infer: { prompt: \"hi\", max_tokens: 1000 }
",
        );
        assert_eq!(c.tasks.len(), 1);
        assert!(!c.has_unbounded, "a token-bounded priced task is bounded");
        assert!(c.bounded_total_usd > 0.0, "sonnet has a price");
        assert_eq!(c.tasks[0].max_tokens, Some(1000));
    }

    #[test]
    fn missing_max_tokens_is_unbounded() {
        let c = ceiling_of(
            "\
nika: v1
workflow: unbounded
model: anthropic/claude-sonnet-4-6
tasks:
  - id: ask
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
nika: v1
workflow: local
model: ollama/llama3
tasks:
  - id: ask
    infer: { prompt: \"hi\", max_tokens: 1000 }
",
        );
        assert!(
            c.has_unbounded,
            "local has no cloud price → reported, not $0"
        );
        assert_eq!(c.tasks[0].unbounded_reason, Some(UnboundedReason::NoPrice));
    }

    #[test]
    fn exec_and_invoke_cost_nothing() {
        let c = ceiling_of(
            "\
nika: v1
workflow: noinfer
tasks:
  - id: sh
    exec: { command: \"true\" }
  - id: tool
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
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: t\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
        );
        let retried = ceiling_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: t\n    retry: { max_attempts: 3 }\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
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
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: a\n    exec: { command: \"true\" }\n  - id: t\n    depends_on: [a]\n    when: ${{ tasks.a.status == 'success' }}\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
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
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: t\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
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
nika: v1
workflow: agentcost
model: anthropic/claude-sonnet-4-6
tasks:
  - id: loop
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
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

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
nika: v1
workflow: w
model: ollama/my-gpt-4-finetune
tasks:
  - id: t
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
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn ceiling_of(yaml: &str) -> CostCeiling {
        ceiling(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn literal_for_each_multiplies_cost_by_count() {
        // 5-element literal list = 5× the per-call cost (was a 5× undercount).
        let single = ceiling_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: t\n    infer: { prompt: \"x\", max_tokens: 1000 }\n",
        );
        let batch = ceiling_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  - id: t\n    for_each: [1, 2, 3, 4, 5]\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 1000 }\n",
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
        // ${{ vars.items }} source → unknown iteration count → unbounded.
        let c = ceiling_of(
            "nika: v1\nworkflow: w\nmodel: anthropic/claude-sonnet-4-6\nvars: { items: \"x\" }\ntasks:\n  - id: t\n    for_each: ${{ vars.items }}\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 1000 }\n",
        );
        assert!(c.has_unbounded);
        assert_eq!(
            c.tasks[0].unbounded_reason,
            Some(UnboundedReason::UnknownIterations)
        );
    }
}
