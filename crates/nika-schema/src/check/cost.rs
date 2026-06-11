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

use crate::raw::{RawAction, RawWorkflow};

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
    /// Worst-case USD for this task · `None` = unbounded tokens OR the
    /// model has no catalog price (both reported, never silently zero).
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

        let (usd, unbounded_reason) = match max_tokens {
            None => (None, Some(UnboundedReason::NoTokenLimit)),
            Some(tokens) => match model.as_deref().and_then(output_price_per_million) {
                Some(price) => {
                    #[allow(clippy::cast_precision_loss)]
                    let cost = (tokens as f64) * price / 1_000_000.0;
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
/// The model segment is matched against `nika-catalog` pricing rows by
/// exact `model_pattern`, then by substring (the catalog's documented
/// match order). Returns `None` for local/unknown providers (zero-price
/// sovereign models are correctly « unpriced », not « free »).
fn output_price_per_million(model: &str) -> Option<f64> {
    let name = model.split_once('/').map_or(model, |(_, n)| n);
    let rows = nika_catalog::all_pricing();
    rows.iter()
        .find(|p| p.model_pattern == name)
        .or_else(|| rows.iter().find(|p| name.contains(p.model_pattern)))
        .map(|p| p.output_per_million)
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
