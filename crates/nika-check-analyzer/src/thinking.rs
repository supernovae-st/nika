// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `infer.thinking` judgments — the cross-field and cross-fact laws
//! the parser cannot hold.
//!
//! The parser validates each field's TYPE (`thinking.enabled` bool ·
//! `budget_tokens` u32 · `max_tokens` u32) and stops; the dispatch copies
//! `budget_tokens` verbatim into the provider call — nothing downstream
//! re-judges either law. Both were false greens: ✔ check, then the
//! refusal (or the silently dropped budget) at run.
//!
//! - **the budget lives UNDER the cap** — with `budget_tokens` and
//!   `max_tokens` both declared, the budget must be smaller: the
//!   reasoning share lives inside `max_tokens`, so a budget ≥ the cap
//!   leaves no room for the visible answer and the provider refuses the
//!   call at run.
//! - **the seat can reason** — `thinking: { enabled: true }` on a model
//!   the catalog KNOWS cannot reason is a dead declaration (the wire
//!   400s or drops the budget). Refused only when the catalog positively
//!   knows the exact seat — a provider row or an EXACT pricing pattern
//!   (the two lanes the run itself trusts · the `catalog_warning`
//!   precedent). A fuzzy pricing match never counts: it exists to absorb
//!   dated variants for a cost estimate, not to license a refusal. A
//!   model the catalog has never heard of keeps the defaults
//!   (`reasoning = false` = « no evidence it reasons », NOT « it
//!   cannot »), so without that gate every newly shipped model would
//!   refuse a legal `thinking:`.
//!
//! A templated seat (`${{ }}`) is judged THROUGH its declared default —
//! the same [`crate::static_literal_of`] law the MODELS rung applies;
//! one with no literal default defers to the run, making no claim.
//!
//! The lane's home, two descents in one day (2026-08-25): written beside
//! `nika-cli`'s models rung, which the 15k crate wall pushed to
//! `nika-check` — already at the wall itself — and on to this substrate
//! (the ADR-115 direction), which also owns the [`static_literal_of`](crate::static_literal_of)
//! resolver the via-default arm reads. The judge stays pure; the CLI
//! folds the rows into its `ModelsAudit` beside the resolver's findings.

use nika_schema::raw::{RawAction, RawInferAction, RawWorkflow};

/// One thinking-law refusal: the task carrying the dead or
/// self-defeating declaration, the seat it names, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThinkingFinding {
    /// The task carrying the `thinking:` block.
    pub task: String,
    /// The seat the judgment read (task `model:` or the envelope
    /// default · the declared default of a templated seat · `-` when the
    /// budget-vs-cap law fired with no seat declared).
    pub model: String,
    /// Why — the law violated and the repair.
    pub why: String,
}

impl ThinkingFinding {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(task: String, model: String, why: String) -> Self {
        Self { task, model, why }
    }
}

/// Judge every infer task's `thinking:` block against the two laws the
/// module doc names. Deterministic and catalog-bound: the same workflow
/// on the same binary yields the same findings.
#[must_use]
pub fn thinking_findings(wf: &RawWorkflow) -> Vec<ThinkingFinding> {
    let mut findings = Vec::new();
    for task in &wf.tasks {
        let RawAction::Infer(action) = &task.value.action else {
            continue;
        };
        let Some(thinking) = &action.thinking else {
            continue;
        };
        if !thinking.value.enabled {
            continue;
        }
        let id = task.value.id.value.as_str();
        // The budget-vs-cap compare is seat-independent: both fields are
        // literal u32s by the time the parse admits them.
        if let (Some(budget), Some(max)) =
            (thinking.value.budget_tokens, action.max_tokens.as_ref())
            && budget >= max.value
        {
            findings.push(ThinkingFinding::new(
                id.to_owned(),
                seat_label(wf, action),
                format!(
                    "`thinking.budget_tokens` ({budget}) on `{id}` must stay UNDER \
                     `max_tokens` ({}) — the reasoning share lives inside the cap, so a \
                     budget ≥ the cap leaves no room for the answer and the provider \
                     refuses the call at run",
                    max.value
                ),
            ));
        }
        // The reasoning-seat law needs a statically known seat.
        let Some(seat) = action.model.as_ref().or(wf.model.as_ref()) else {
            continue;
        };
        let judged = if seat.value.contains("${{") {
            let Some(literal) = crate::static_literal_of(wf, &seat.value)
                .and_then(|v| v.as_str().map(str::to_owned))
            else {
                continue; // no literal default — the run judges
            };
            literal
        } else {
            seat.value.clone()
        };
        let Some((provider, name)) = judged.split_once('/') else {
            continue; // no provider prefix — the resolver's refusal owns that class
        };
        if catalog_knows(provider, name, &judged)
            && !nika_catalog::model_capabilities(provider, name).reasoning
        {
            findings.push(ThinkingFinding::new(
                id.to_owned(),
                judged.clone(),
                format!(
                    "`thinking: {{ enabled: true }}` on `{id}` seats `{judged}` — the \
                     catalog knows this model cannot reason, so the declaration is dead \
                     at the wire (the budget is dropped or the call refused); remove \
                     `thinking:` or seat a reasoning-capable model"
                ),
            ));
        }
    }
    findings
}

/// The catalog POSITIVELY knows this exact seat: a provider row (the
/// binary's own teaching — nicknames + wire ids) or an EXACT pricing
/// pattern. See the module doc for why fuzzy matches never count.
fn catalog_knows(provider: &str, name: &str, judged: &str) -> bool {
    let row_knows = nika_catalog::find_provider(provider)
        .is_some_and(|row| row.models.iter().any(|m| m.id == name || m.model == name));
    row_knows
        || nika_catalog::all_pricing().iter().any(|p| {
            p.provider.eq_ignore_ascii_case(provider)
                && (p.model_pattern == name || p.model_pattern == judged)
        })
}

/// The seat label a thinking finding prints — the task's `model:` or the
/// envelope default, `-` when neither reaches the task.
fn seat_label(wf: &RawWorkflow, action: &RawInferAction) -> String {
    action
        .model
        .as_ref()
        .or(wf.model.as_ref())
        .map_or_else(|| "-".to_owned(), |s| s.value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn findings_of(yaml: &str) -> Vec<ThinkingFinding> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        thinking_findings(&wf)
    }

    fn infer_wf(model: &str, max_tokens: &str, thinking: &str) -> String {
        format!(
            "nika: w\ntasks:\n  t:\n    infer:\n      prompt: hi\n      model: {model}\n      \
             max_tokens: {max_tokens}\n      thinking: {thinking}\n"
        )
    }

    /// The budget-vs-cap law: the parse typed both fields as u32 and
    /// stopped there; the dispatch copies the budget verbatim, so a
    /// budget ≥ the cap audited green and the provider refused the call
    /// at run. The finding fires at check, before a token is spent.
    #[test]
    fn a_budget_at_or_over_the_cap_is_a_finding() {
        for (budget, max) in [("100", "100"), ("101", "100")] {
            let f = findings_of(&infer_wf(
                "mock/echo",
                max,
                &format!("{{ enabled: true, budget_tokens: {budget} }}"),
            ));
            assert_eq!(f.len(), 1, "budget {budget} vs cap {max}: {f:?}");
            assert_eq!(f[0].task, "t", "{f:?}");
            assert!(
                f[0].why.contains("budget_tokens") && f[0].why.contains("max_tokens"),
                "the finding names both fields: {}",
                f[0].why
            );
        }

        // Control pair: a budget UNDER the cap makes no finding (a
        // compare that always fires is the same defect, mirrored).
        let ok = findings_of(&infer_wf(
            "mock/echo",
            "100",
            "{ enabled: true, budget_tokens: 50 }",
        ));
        assert!(ok.is_empty(), "the legal twin stays clean: {ok:?}");
    }

    /// The compare needs BOTH literals: a budget without a declared cap
    /// (the provider default governs) and a cap without a budget (the
    /// provider's own thinking bound governs) make no claim.
    #[test]
    fn the_compare_needs_both_sides() {
        let no_cap = findings_of(
            "nika: w\ntasks:\n  t:\n    infer:\n      prompt: hi\n      model: mock/echo\n      \
             thinking: { enabled: true, budget_tokens: 5000 }\n",
        );
        assert!(no_cap.is_empty(), "no declared cap, no compare: {no_cap:?}");
        let no_budget = findings_of(&infer_wf("mock/echo", "100", "{ enabled: true }"));
        assert!(
            no_budget.is_empty(),
            "no declared budget, no compare: {no_budget:?}"
        );
        let disabled = findings_of(&infer_wf(
            "mock/echo",
            "100",
            "{ enabled: false, budget_tokens: 100 }",
        ));
        assert!(
            disabled.is_empty(),
            "enabled: false declares no thinking: {disabled:?}"
        );
    }

    /// The reasoning-seat law: `enabled: true` on a model the catalog
    /// KNOWS cannot reason (gpt-4o-mini is a priced catalog row with
    /// `reasoning = false`) is a dead declaration — refused at check.
    #[test]
    fn thinking_on_a_known_non_reasoning_seat_is_a_finding() {
        let f = findings_of(&infer_wf(
            "\"openai/gpt-4o-mini\"",
            "500",
            "{ enabled: true, budget_tokens: 100 }",
        ));
        assert_eq!(f.len(), 1, "a dead declaration is a finding: {f:?}");
        assert_eq!(f[0].model, "openai/gpt-4o-mini", "{f:?}");
        assert!(f[0].why.contains("reason"), "{}", f[0].why);

        // Controls — every neighbor the refusal must NOT eat:
        // a reasoning seat (o3) · a seat the catalog does not carry
        // (the catalog_warning lane's advisory, never this refusal) · a
        // templated seat with no literal default (the run judges it).
        for yaml in [
            infer_wf(
                "\"openai/o3\"",
                "500",
                "{ enabled: true, budget_tokens: 100 }",
            ),
            infer_wf(
                "\"mock/echo\"",
                "500",
                "{ enabled: true, budget_tokens: 100 }",
            ),
            "nika: w\ninputs:\n  seat: { type: string, required: true }\ntasks:\n  t:\n    \
             infer:\n      prompt: hi\n      model: \"${{ inputs.seat }}\"\n      \
             max_tokens: 500\n      thinking: { enabled: true, budget_tokens: 100 }\n"
                .to_owned(),
        ] {
            let f = findings_of(&yaml);
            assert!(f.is_empty(), "no refusal here: {f:?}\n{yaml}");
        }
    }

    /// A templated seat is judged THROUGH its declared default — the
    /// via-default law applied to the reasoning fact: a
    /// `${{ const.seat }}` whose default cannot reason is the same dead
    /// declaration, named as the default that was judged.
    #[test]
    fn a_templated_seat_is_judged_through_its_declared_default() {
        let f = findings_of(
            "nika: w\nconst:\n  seat: \"openai/gpt-4o-mini\"\ntasks:\n  t:\n    infer:\n      \
             prompt: hi\n      model: \"${{ const.seat }}\"\n      max_tokens: 500\n      \
             thinking: { enabled: true, budget_tokens: 100 }\n",
        );
        assert_eq!(f.len(), 1, "the declared default is judged: {f:?}");
        assert_eq!(f[0].model, "openai/gpt-4o-mini", "{f:?}");
    }

    /// `agent:` carries no `thinking:` field and `exec`/`invoke` no seat
    /// law — the judgment is infer-scoped by construction.
    #[test]
    fn non_infer_tasks_make_no_thinking_claim() {
        let f = findings_of("nika: w\ntasks:\n  t:\n    exec: { command: [\"echo\", \"ok\"] }\n");
        assert!(f.is_empty(), "{f:?}");
    }
}
