// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reader's own refusals of a HOT admission ([`Reading::hot_rejections`],
//! [`Reading::complete`]): every clause consumed is not evidence of understanding. Beside
//! `lexicon.rs` at the 1,500-line file cap; `hot` adds the laws over the reader's cue table
//! and plan.
use super::Reading;
use crate::objects;
use crate::plan::{EffectPolicy, Op};

impl Reading {
    /// Why this reading may NOT be admitted as HOT under the strict contract: every clause
    /// consumed is not evidence of understanding. A step is explicit when its object is a
    /// typed literal or a short noun phrase without coordinated residue; an effect when its
    /// target is short or literal; and nothing ambiguous, unresolved or unknown remains.
    #[must_use]
    pub fn hot_rejections(&self) -> Vec<String> {
        let mut why = Vec::new();
        if !self.unresolved.is_empty() {
            why.push(format!("{} unresolved clause(s)", self.unresolved.len()));
        }
        if !self.ambiguous.is_empty() {
            why.push(format!("{} ambiguous clause(s)", self.ambiguous.len()));
        }
        if !self.plan.unknowns.is_empty() {
            why.push("unknown requested work".to_owned());
        }
        if self.plan.steps.is_empty() && self.plan.effects.is_empty() {
            why.push("nothing recognized".to_owned());
        }
        for step in &self.plan.steps {
            let categorical = step.op == Op::Classify && !step.categories.is_empty();
            // A rule the closed grammar parsed is a typed literal, explicit by construction;
            // the plan joins a promoted rule to an existing computation with ` ; `, so each
            // part is judged on its own.
            let ruled = step.op == Op::Compute
                && step.detail.split(" ; ").all(|part| {
                    objects::explicit_object(part)
                        || self.plan.rules.iter().any(|r| r.text() == part.trim())
                });
            // An extract's object is the list of the fields to pull out: a list of short
            // noun phrases is explicit, whatever its length.
            let listed = step.op == Op::Extract && objects::explicit_field_list(&step.detail);
            if !categorical && !ruled && !listed && !objects::explicit_object(&step.detail) {
                why.push(format!(
                    "`{}` object is not explicit: {}",
                    step.op.word(),
                    step.detail.trim()
                ));
            }
        }
        for effect in &self.plan.effects {
            if matches!(
                effect.policy,
                EffectPolicy::Automatic | EffectPolicy::HumanFirst
            ) && !objects::explicit_object(&effect.target)
            {
                why.push(format!(
                    "`{}` target is not explicit: {}",
                    effect.verb.word(),
                    effect.target.trim()
                ));
            }
        }
        if !self.soft_constraints.is_empty() {
            why.push(format!(
                "{} prose clause(s) the reader cannot parse",
                self.soft_constraints.len()
            ));
        }
        // A constraint needs an operation to carry it; reads and writes carry nothing.
        if !self.plan.constraints.is_empty()
            && !self.plan.steps.iter().any(|s| s.op.carries_constraints())
        {
            why.push(format!(
                "{} constraint(s) with no operation to carry them",
                self.plan.constraints.len()
            ));
        }
        // Accounting: a clause the reader saw must be the evidence of something it produced.
        for clause in &self.seen {
            // An element read from the prefix of a clause accounts for the clause: the
            // rest of the clause was its policy (`publish it to ./x.md only after my approval`).
            let within = |evidence: &str| {
                !evidence.trim().is_empty()
                    && (evidence.contains(clause.as_str()) || clause.contains(evidence))
            };
            let accounted = self.plan.steps.iter().any(|s| within(&s.evidence))
                || self.plan.effects.iter().any(|e| within(&e.evidence))
                || self.plan.obligations.iter().any(|o| within(&o.evidence))
                || self.plan.rules.iter().any(|r| within(r.text()))
                || self.policy_clauses.iter().any(|c| within(c))
                || self
                    .plan
                    .constraints
                    .iter()
                    .any(|c| c == clause || c.contains(clause.as_str()))
                || self.unresolved.contains(clause)
                || self.ambiguous.iter().any(|a| a.clause == *clause)
                || self
                    .plan
                    .trigger
                    .as_deref()
                    .is_some_and(|t| clause.to_lowercase().starts_with(t));
            if !accounted {
                why.push(format!("unaccounted clause: {clause}"));
            }
        }
        why
    }

    /// HOT is possible only when every clause was consumed and something was asked.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.unresolved.is_empty()
            && self.ambiguous.is_empty()
            && (!self.plan.steps.is_empty() || !self.plan.effects.is_empty())
    }
}
