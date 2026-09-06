// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The COST rung of the check card — the priced arm, the composed-only
//! arm, the empty arm and the task rows (split from `check_render.rs` at
//! the 1,500-line wall · wave 3.b; the bodies moved verbatim).

use std::fmt::Write as _;

use super::*;

/// The composition arm of [`cost()`] (spec 14 · the 2026-07-29 finding): no
/// OWN inference task, but resolvable children are priced — printing
/// `$0.00` here told the operator a free story about a bill the child
/// explains at `≤$X`. The totals already carry the children; the uncapped
/// count names the composed half only (no own task exists to count).
fn cost_composed_only(out: &mut String, report: &CheckReport, t: Theme) {
    let calls = crate::vocab::count(report.cost.composed.len(), "composed child call");
    if report.cost.has_unbounded {
        let _ = writeln!(
            out,
            " {} {}     {}",
            t.paint(Role::Warn, if t.ascii { "! " } else { "⚠ " }),
            t.paint(Role::Strong, "COST"),
            t.paint(
                Role::Warn,
                &format!(
                    "bounded portion ${} · no total ceiling · {} · {}",
                    crate::vocab::usd(report.cost.bounded_total_usd),
                    calls,
                    unbounded_census(report)
                )
            )
        );
    } else {
        let money = format!(
            "${} – ${} worst-case output ceiling · {} · own inference $0.00",
            crate::vocab::usd(report.cost.min_path_total_usd),
            crate::vocab::usd(report.cost.bounded_total_usd),
            calls
        );
        let _ = writeln!(
            out,
            " {} {}     {}",
            mark(t, true),
            t.paint(Role::Strong, "COST"),
            money
        );
    }
}

/// The COST arm for a workflow with NO own inference task — the `$0.00`
/// that used to claim the whole bill from a lane that prices
/// `infer:`/`agent:` and nothing else; the composed-children arm joins
/// here (extracted under the fn-length law).
fn cost_empty_arm(out: &mut String, report: &CheckReport, t: Theme) {
    if !report.cost.composed.is_empty() {
        return cost_composed_only(out, report, t);
    }
    let _ = writeln!(
        out,
        " {} {}     {}",
        mark(t, true),
        t.paint(Role::Strong, "COST"),
        // An `exec:` runs an arbitrary program, and the programs authors
        // reach for first are billed LLM CLIs; an `mcp:` call is a third
        // party's meter. Measured 2026-07-29: a lone
        // `exec: ["claude", "-p", "write a novel"]` printed
        // `✔ COST no inference tasks · $0.00` and
        // `✔ audited · est ≤$0.0000`.
        t.paint(
            Role::Dim,
            "no infer/agent tasks · $0.00 · exec + mcp spend unpriced"
        )
    );
}

/// The per-task rows of [`cost()`] — a priced row at its cap, or the
/// UNBOUNDED row with its named reason (extracted under the fn-length law).
fn cost_task_rows(out: &mut String, report: &CheckReport, t: Theme) {
    let le = crate::vocab::at_most(t.ascii);
    for c in &report.cost.tasks {
        let model = c.model.as_deref().unwrap_or("?");
        match (&c.usd, &c.unbounded_reason) {
            (Some(worst), _) => {
                let _ = writeln!(
                    out,
                    "   {}  {}  {le}{} tk  ${}",
                    c.task,
                    t.paint(Role::Dim, model),
                    c.max_tokens.unwrap_or(0),
                    crate::vocab::usd(*worst),
                );
            }
            (None, reason) => {
                let why = match reason {
                    Some(UnboundedReason::NoTokenLimit) => "no max_tokens declared",
                    Some(UnboundedReason::NoPrice) => "no catalog price (local/unknown model)",
                    Some(UnboundedReason::UnknownIterations) => {
                        "for_each over an expression (unknown count)"
                    }
                    _ => "unbounded",
                };
                let _ = writeln!(
                    out,
                    "   {}  {}  {} {}",
                    c.task,
                    t.paint(Role::Dim, model),
                    t.paint(Role::Warn, "UNBOUNDED"),
                    t.paint(Role::Dim, &format!("— {why}")),
                );
            }
        }
    }
}

pub(super) fn cost(out: &mut String, report: &CheckReport, t: Theme, layers: &VerdictLayers) {
    if report.cost.tasks.is_empty() {
        return cost_empty_arm(out, report, t);
    }
    // OUTPUT ceiling, and the word is load-bearing. `cost::ceiling` prices
    // `max_tokens`, which the spec defines as "Max OUTPUT tokens"
    // (02-verbs §infer), against `output_price_per_million`. The prompt is
    // not underweighted in that sum — it is absent, and
    // `input_per_million` has no reader anywhere in `nika-check`.
    //
    // The gap is not academic. Measured 2026-07-28 on the most common shape
    // a person writes first — fetch a document, summarise it: a 3.2 MB body
    // interpolated into one prompt is ~818k input tokens, $2.4563 at that
    // model's published input rate, against a line reading $0.0075. 328x,
    // under a green mark, with the input price sitting four lines above the
    // output price the sum reads in the same catalog block.
    //
    // Pricing it properly needs a static bound on interpolated content,
    // which is real work (the shape is the one `for_each` already uses:
    // literal is known, expression is unbounded). Until that lands the
    // verdict NARROWS instead of overreaching — a claim that covers what it
    // computes is always available, and is the only honest thing to print
    // while the other half has no bound.
    //
    // Unbounded cost is a WARNING posture (is_clean ignores it): the
    // report stays honest about the floor without failing the file.
    // The unbounded arm used to print `$min – $bounded` under the word
    // FLOOR — and `audited_line` documents why both halves are false
    // (`min_path_total_usd` bounds nothing from below · measured 126× the
    // other way). Same decision as there: claim neither bound, show the
    // only true number (the priced portion), and name the uncapped tasks.
    let (cost_mark, money, bound) = if report.cost.has_unbounded {
        (
            t.paint(Role::Warn, if t.ascii { "! " } else { "⚠ " }),
            format!(
                "bounded portion ${}",
                crate::vocab::usd(report.cost.bounded_total_usd)
            ),
            t.paint(
                Role::Warn,
                &format!("no total ceiling · {}", unbounded_census(report)),
            ),
        )
    } else {
        (
            mark(t, true),
            format!(
                "${} – ${}",
                crate::vocab::usd(report.cost.min_path_total_usd),
                crate::vocab::usd(report.cost.bounded_total_usd)
            ),
            // W3-F4 · a seat-served model is billed to its subscription: the
            // dollar figure is its API counterfactual, said in words.
            if layers.seat_served.is_empty() {
                "worst-case output ceiling".to_owned()
            } else {
                format!(
                    "worst-case output ceiling · seat-served (unmetered): {} — the dollar figure is the API counterfactual",
                    layers.seat_served.join(", ")
                )
            },
        )
    };
    // The price table's date rides WITH the number. A ceiling is a
    // promise, and a promise computed against prices that have since
    // moved is a promise about the past — the `--json` lane has carried
    // `pricing.snapshot.as_of` since the models rung shipped, but the
    // human lane never showed it, so the one reader who cannot query the
    // payload was the one who could not tell.
    //
    // This is not hypothetical drift: vendor intro pricing expires on
    // announced dates, so a workflow audited in one month can bill more
    // in the next with the file unchanged.
    let snap = nika_catalog::pricing_snapshot();
    let _ = writeln!(
        out,
        " {cost_mark} {}     {} {bound} {}",
        t.paint(Role::Strong, "COST"),
        t.paint(Role::Strong, &money),
        // `exec` + `mcp` join `prompts` in the unpriced list for the same
        // reason the empty-cost branch above names them: this ceiling
        // sums `infer:`/`agent:` output tokens, so a workflow that mixes
        // an `infer:` with an `exec:` gets a ceiling with the exec's whole
        // bill missing — and the ✔ does not say so.
        t.paint(
            Role::Dim,
            &format!("· prompts, exec + mcp unpriced · prices {}", snap.as_of)
        ),
    );
    cost_task_rows(out, report, t);
}
