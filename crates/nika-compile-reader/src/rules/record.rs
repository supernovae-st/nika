// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Re-establish closed computation invariants at the saved-plan boundary.

use super::{Kind, Operand, Rule, Shape, Term, tokenize};
use crate::plan::{Op, Step};

pub(super) fn valid(rule: &Rule) -> bool {
    if rule.text.trim().is_empty() {
        return false;
    }
    if rule.program.is_some() {
        return rule.clauses.is_empty()
            && rule.shape == Shape::default()
            && !rule.lines
            && !rule.summary;
    }
    let mut names = std::collections::BTreeSet::new();
    if !rule
        .shape
        .aggregations
        .iter()
        .map(|a| a.name.as_str())
        .chain(rule.shape.derived.iter().map(|d| d.name.as_str()))
        .all(|name| !name.is_empty() && names.insert(name))
    {
        return false;
    }
    rule.clauses.iter().all(|clause| match &clause.value {
        Operand::Number(number) => crate::rule_tokens::recorded_number(number),
        Operand::Slot(slug) => slot_slug(slug),
        _ => true,
    })
}

pub(crate) fn slot_slug(slug: &str) -> bool {
    !slug.is_empty() && slug.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

impl Rule {
    /// Anchored by its own words (the request states them, whitespace runs and typographic
    /// quotes aside, never a changed letter) and whole numeric tokens, sign included; a verified
    /// program through the compute step whose detail it is, the plan judging that step's
    /// evidence (E14 F7). This proves neither program correctness nor intent coverage.
    pub(crate) fn record_anchored(&self, intent: &str, steps: &[Step]) -> bool {
        if self.program.is_some() {
            let realized = |s: &Step| s.op == Op::Compute && s.detail.trim() == self.text.trim();
            return valid(self) && steps.iter().any(realized);
        }
        let fold = |t: &str| {
            let spaced = t.split_whitespace().collect::<Vec<_>>().join(" ");
            spaced
                .chars()
                .map(crate::text::fold_quote)
                .collect::<String>()
        };
        let tokens = tokenize(intent);
        let numeric = |number: &str| {
            tokens.iter().any(|token| match &token.kind {
                Kind::Number(value) => value == number,
                Kind::Quoted => {
                    crate::rule_tokens::number(&token.original).as_deref() == Some(number)
                }
                _ => false,
            })
        };
        let words = fold(&self.text);
        !words.is_empty()
            && fold(intent).contains(&words)
            && self.clauses.iter().all(|clause| match &clause.value {
                Operand::Number(number) => numeric(number),
                _ => true,
            })
            && self.shape.derived.iter().all(|derived| {
                [&derived.left, &derived.right]
                    .into_iter()
                    .all(|term| match term {
                        Term::Number(number) => numeric(number),
                        Term::Name(_) => true,
                    })
            })
    }
}
