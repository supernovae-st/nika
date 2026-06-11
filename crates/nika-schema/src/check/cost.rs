// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cost ceiling — the worst-case spend, computed statically.
//!
//! For each `infer:`/`agent:` task with a declared output bound
//! (`max_tokens` · `max_tokens_total`), the ceiling is
//! `tokens × output_price_per_million / 1e6`, priced from
//! `nika-catalog`. A task with NO declared token bound is **unbounded** —
//! the most likely « why did this cost $200 » surprise — and is reported
//! as such (the foot-gun is named, not hidden).

use crate::raw::{ForEachValue, RawAction, RawWorkflow};

/// Per-task worst-case cost.
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
    /// Worst-case USD for this task (already × `iterations`) · `None` =
    /// unbounded tokens, unknown iterations, OR the model has no catalog
    /// price (all reported, never silently zero).
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

/// The whole-workflow cost ceiling.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct CostCeiling {
    /// Per-task breakdown (only `infer:`/`agent:` tasks).
    pub tasks: Vec<TaskCost>,
    /// Σ of the bounded task costs in USD.
    pub bounded_total_usd: f64,
    /// `true` when at least one inference task is unbounded — the total is
    /// a FLOOR, not a ceiling, and the report says so.
    pub has_unbounded: bool,
}

/// Compute the cost ceiling for a workflow.
#[must_use]
pub(super) fn ceiling(wf: &RawWorkflow) -> CostCeiling {
    let default_model = wf.model.as_ref().map(|m| m.value.clone());
    let mut tasks = Vec::new();
    let mut bounded_total_usd = 0.0;
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

        let (usd, unbounded_reason) = match (max_tokens, iterations) {
            (None, _) => (None, Some(UnboundedReason::NoTokenLimit)),
            (Some(_), None) => (None, Some(UnboundedReason::UnknownIterations)),
            (Some(tokens), Some(n)) => match model.as_deref().and_then(output_price_per_million) {
                Some(price) => {
                    #[allow(clippy::cast_precision_loss)]
                    let cost = (tokens as f64) * (n as f64) * price / 1_000_000.0;
                    bounded_total_usd += cost;
                    (Some(cost), None)
                }
                None => (None, Some(UnboundedReason::NoPrice)),
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
            usd,
            unbounded_reason,
        });
    }

    CostCeiling {
        tasks,
        bounded_total_usd,
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
