// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A seat's admitted typed rule beside the reader's own reading of the same clause (E38, a C3
//! neighbour): the reader's filter is kept as read, the stages the seat states over it are joined,
//! and stages that cannot be joined are asked, never silently dropped.

use crate::plan::Plan;
use crate::rules::Rule;
use crate::{CompileOutcome, DiagnosticKind};

/// Whether the seat's admitted rule for a clause is now the plan's (its slots are then asked).
/// A clause the reader holds no rule of takes it. The reader's own stages bind over a seat's
/// (R4 A3: a changed field, comparator, literal, direction or count is caught by the reader's
/// rule for the same words), and a seat rule with no stage of its own adds nothing. Stages a
/// seat states over the reader's plain filter replace it when both keep the same rows (same
/// clauses, junction and lines), and the plan says so; over a filter that disagrees they cannot
/// be joined, and the disagreement is recorded as work no model settles.
pub(super) fn join(plan: &mut Plan, rule: Rule, out: &mut CompileOutcome) -> bool {
    let Some(at) = plan
        .rules
        .iter()
        .position(|read| read.text() == rule.text())
    else {
        plan.rules.push(rule);
        return true;
    };
    let read = &plan.rules[at];
    if read.shaped() || !rule.shaped() {
        return false;
    }
    let (theirs, mine) = (read.to_json(), rule.to_json());
    if ["clauses", "junction", "lines"]
        .iter()
        .all(|key| theirs[*key] == mine[*key])
    {
        crate::finding(
            out,
            DiagnosticKind::Applied,
            "authoring_plan",
            format!(
                "`{}` is read as a filter; the stages the proposal states over it are kept: `{}`.",
                rule.text(),
                rule.jq()
            ),
        );
        plan.rules[at] = rule;
        return true;
    }
    plan.unknowns.push(format!(
        "The proposal reads `{}` as `{}` while the request's own reading is `{}`; the disagreement is not settled by a model.",
        rule.text(),
        rule.jq(),
        read.jq()
    ));
    false
}
