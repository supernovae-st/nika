// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Rebind typed source references after an explicit field choice, never program text.
use super::{Clause, Operand, Rule, Shape};
impl Rule {
    /// Every input field read, excluding output names introduced by the rule.
    #[must_use]
    pub fn source_fields(&self) -> Vec<String> {
        self.fields()
    }

    /// Apply an explicit field answer to typed references. Arbitrary jq is not rewritable.
    #[must_use]
    pub fn with_source_field(&self, from: &str, to: &str) -> Option<Self> {
        if from == to {
            return Some(self.clone());
        }
        if self.program.is_some() {
            return None;
        }
        let mut rule = self.clone();
        rebind(&mut rule.clauses, &mut rule.shape, from, to);
        // Every ordered step reads source columns too (R4 F5).
        for step in &mut rule.then {
            rebind(&mut step.clauses, &mut step.shape, from, to);
        }
        Some(rule)
    }
}

/// One step's typed source references rebound from `from` to `to`, produced names aside.
fn rebind(clauses: &mut [Clause], shape: &mut Shape, from: &str, to: &str) {
    let replace = |field: &mut String| {
        if field == from {
            to.clone_into(field);
        }
    };
    for clause in clauses {
        replace(&mut clause.field);
        if let Operand::Column(field) = &mut clause.value {
            replace(field);
        }
    }
    let produced: Vec<String> = shape.produced().into_iter().map(str::to_owned).collect();
    for field in [&mut shape.join_on, &mut shape.group_by]
        .into_iter()
        .flatten()
    {
        replace(field);
    }
    for field in &mut shape.distinct_by {
        replace(field);
    }
    for aggregation in &mut shape.aggregations {
        if let Some(field) = &mut aggregation.field {
            replace(field);
        }
    }
    if let Some((field, _)) = &mut shape.sort_by
        && !produced.contains(field)
    {
        replace(field);
    }
    if produced.is_empty() {
        for field in &mut shape.columns {
            replace(field);
        }
    }
    for (field, _) in &mut shape.renames {
        if !produced.contains(field) {
            replace(field);
        }
    }
}
