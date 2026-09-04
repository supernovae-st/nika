// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika trace session <trace> <workflow>` — the session digest.
//!
//! `nika-tui-core` derives four things every surface may show — the
//! WAVES, each step's IDLE, the wave holder others wait for, and the
//! SPEND by verb. Until this verb, nothing in the engine printed any of
//! them: the crate carried the most law and the least usage, and a law
//! nothing applies is a wish (its own `claims` module says so).
//!
//! **Why the workflow file is required and not inferred.** The journal's
//! `workflow_started` frame records the workflow's ID and its sha256 —
//! never its PATH. And waves live in the dependency graph, which only
//! the definition carries. So this verb takes both, exactly like `flow`
//! (the trace records values, the definition records the shape).
//!
//! ⭐ **THE WAVE LAW IS ASKED ONCE.** `needs` is built from
//! [`nika_check::analyzer::edges::derive_edges`] — the SAME edge set the
//! engine's own `topo_waves` orders by. Both answer `1 + max(over
//! predecessors)`: Kahn levels release a node only once EVERY predecessor
//! has been emitted, which is the longest path. So the two cannot answer
//! differently, and `waves_agree_with_the_engine` pins it on a real file
//! rather than trusting the argument.
//!
//! ⚠️ Building `needs` from the graph PROJECTION instead would have been
//! wrong, and silently: `nika_graph::project` appends
//! `derive_recovery_reads` edges on top of `derive_edges`, and
//! `topo_waves` never sees those. The extra precedence would have pushed
//! tasks into later waves and inflated the count — a number that looks
//! fine and disagrees with the engine.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use nika_tui_core::{claims, derive, ingress, model};

use crate::display::format::fmt_cost_usd;
use crate::display::theme::{Role, Theme};

use super::VerbOutput;

/// The session law's [`model::Workflow`], built from the CHECKED
/// definition.
///
/// Only what the derivations read is filled. `glyph` stays empty and
/// `touches` stays `None` on purpose: this seam has no honest source for
/// either, and inventing them would make `blast_radius` / `undeclared`
/// answer about a boundary nobody measured. An empty blast radius is a
/// claim withheld; a guessed one is a claim fabricated.
fn workflow_of(wf: &RawWorkflow, file: &str) -> model::Workflow {
    let ids: BTreeMap<String, usize> = wf
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.value.id.value.clone(), i))
        .collect();

    // The predecessors of the edge set the engine orders by — deduped
    // (parallel edges between one pair count once for precedence, the
    // same rule `topo_waves` applies with its `seen` set).
    let mut needs: Vec<BTreeSet<String>> = vec![BTreeSet::new(); wf.tasks.len()];
    for edge in nika_check::analyzer::edges::derive_edges(&wf.tasks, &ids) {
        let (Some(from), Some(slot)) = (wf.tasks.get(edge.from), needs.get_mut(edge.to)) else {
            continue;
        };
        slot.insert(from.value.id.value.clone());
    }

    let tasks = wf
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let (verb, tool) = action_facts(&t.value.action);
            let mut task = model::Task::new(
                t.value.id.value.clone(),
                verb,
                String::new(),
                needs
                    .get(i)
                    .map(|s| s.iter().cloned().collect())
                    .unwrap_or_default(),
            );
            task.origin = Some(origin_of(verb, tool.as_deref()));
            task.tool = tool;
            task
        })
        .collect();

    model::Workflow::new(
        file.to_owned(),
        String::new(),
        String::new(),
        Vec::new(),
        String::new(),
        tasks,
    )
}

/// The verb and the called tool — the two facts the derivations read off
/// an action (`cost_by_verb` reads the verb, the fan-out signature reads
/// both).
fn action_facts(action: &RawAction) -> (model::Verb, Option<String>) {
    match action {
        RawAction::Infer(_) => (model::Verb::Infer, None),
        RawAction::Exec(_) => (model::Verb::Exec, None),
        RawAction::Agent(_) => (model::Verb::Agent, None),
        RawAction::Invoke(invoke) => {
            let tool = match &invoke.target {
                RawInvokeTarget::Tool(t) => t.value.clone(),
                RawInvokeTarget::Workflow(w) => format!("workflow:{}", w.value),
            };
            (model::Verb::Invoke, Some(tool))
        }
        // The house's fail-loud wildcard (the `nika_graph::action_facts`
        // precedent): the 4 verbs are locked forever, so this arm is
        // unreachable today. Folding a future verb into one of the four
        // would make `cost_by_verb` attribute its spend to the wrong
        // name — a wrong number is worse than a loud stop.
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future verb — it must teach this digest its name before its spend can be attributed"
        )]
        other => unreachable!("unknown verb: {other:?}"),
    }
}

/// Who wrote the code a step runs — read off the tool namespace, which
/// is the only provenance signal a static definition carries.
fn origin_of(verb: model::Verb, tool: Option<&str>) -> model::Origin {
    match verb {
        model::Verb::Exec => model::Origin::Shell,
        model::Verb::Infer | model::Verb::Agent => model::Origin::Model,
        model::Verb::Invoke => match tool {
            Some(t) if t.starts_with("mcp:") => model::Origin::Mcp,
            Some(t) if t.starts_with("registry:") => model::Origin::Registry,
            _ => model::Origin::Builtin,
        },
    }
}

/// `nika trace session <trace> <workflow>` — waves · the wave holder ·
/// the spend by verb · the wall, each gated by the predicate that says
/// whether it may be claimed at all.
#[must_use]
pub fn session(trace: &str, workflow: &str, theme: Theme) -> VerbOutput {
    let bytes = match std::fs::read_to_string(trace) {
        Ok(bytes) => bytes,
        Err(e) => return VerbOutput::env(format!("cannot read {trace}: {e}")),
    };
    let run = match ingress::run_from_journal(&bytes) {
        Ok(run) => run,
        Err(e) => return VerbOutput::file(format!("TRACE ✗  {e}")),
    };
    let (wf, _report) = match crate::load_checked(workflow) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    let model_wf = workflow_of(&wf, workflow);
    VerbOutput::ok(render_session(&model_wf, &run, theme))
}

/// The digest itself — a pure function of the two truths, so the test
/// reads it without a filesystem.
fn render_session(wf: &model::Workflow, run: &model::Run, theme: Theme) -> String {
    let mut out = String::new();
    let waves = derive::waves(wf);

    // The receipt, gated: `chain intact` may only ride a run carrying
    // POSITIVE evidence of having happened. When it cannot, the digest
    // still prints — the numbers come from the journal actually read —
    // but it says so instead of implying a receipt it does not have.
    if claims::may_claim_chain_intact(run) {
        let _ = writeln!(out, "  {} {}", theme.paint(Role::Dim, "trace"), run.trace);
    } else {
        let _ = writeln!(
            out,
            "  {}",
            theme.paint(
                Role::Warn,
                "this journal carries no trace id — the numbers below are real, the receipt is not"
            )
        );
    }

    let widest = waves.groups().iter().map(Vec::len).max().unwrap_or(0);
    let _ = writeln!(
        out,
        "  {} · widest {}",
        crate::text::count(waves.len(), "wave"),
        crate::text::count(widest, "task"),
    );

    // ⭐ The bottleneck, gated TWICE — once by the run's health, once by
    // the claim. `has_failed` first, because the crate's own rule is that
    // no bottleneck gets painted over a failure: on a broken run the
    // holder is an artefact of what died, not of what held.
    let neck = derive::bottleneck(&waves, run);
    if derive::has_failed(run) {
        let _ = writeln!(
            out,
            "  {}",
            theme.paint(
                Role::Warn,
                "the run broke — no wave holder is named over a failure"
            )
        );
    } else if claims::may_claim_bottleneck(neck.as_ref()) {
        // The `is_some_and` above is what makes this unwrap-free read
        // total: the claim IS the presence test.
        if let Some(neck) = neck.as_ref() {
            let _ = writeln!(
                out,
                "  bottleneck {} — {:.1}s of waiting across {}",
                neck.id,
                neck.idle_total,
                crate::text::count(neck.blocked, "blocked step"),
            );
        }
    } else {
        let _ = writeln!(
            out,
            "  {}",
            theme.paint(
                Role::Dim,
                "no wave holder — nothing waited long enough on one step",
            )
        );
    }

    // The spend, by verb. A verb that cost nothing is not printed: an
    // absent verb on screen is noise (the crate's `verbs_used` rule,
    // applied to money).
    // A run nobody metered (a rehearsal · a local path) says so — never
    // « $0.00 spent » (ADR-128 · unknown cost is not zero).
    let metered = run.steps.iter().any(|s| s.cost.is_some());
    let mut spend = if metered {
        format!("  {} spent", fmt_cost_usd(derive::total_cost(run)))
    } else {
        "  no spend metered (unmetered · a rehearsal or a local path)".to_owned()
    };
    for (verb, amount) in derive::cost_by_verb(wf, run) {
        if amount > 0.0 {
            let _ = write!(spend, " · {verb} {}", fmt_cost_usd(amount));
        }
    }
    let _ = writeln!(out, "{spend}");

    let _ = writeln!(
        out,
        "  {}",
        theme.paint(
            Role::Dim,
            &format!(
                "{:.1}s wall · derived from the journal × the checked definition",
                derive::total_time(run)
            )
        )
    );
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const PLAIN: Theme = Theme::new(false, true, false);

    /// A step counts as having WAITED past this many seconds — the
    /// crate's own threshold (`derive::bottleneck`), spelled here so the
    /// tie fixture below sits provably UNDER it rather than near it.
    const BLOCKED_FLOOR_S: f64 = 0.05;

    /// Three waves — `a` alone, then `b` ∥ `b2`, then `c`. Wave 1 holds
    /// two tasks on purpose: `bottleneck` skips any wave of fewer than
    /// two, so a chain could never exercise the holder law.
    ///
    /// Every edge here is IMPLICIT (`${{ tasks.X.output }}`), never an
    /// `after:` — which is exactly what a converter reading the declared
    /// `needs:` would have missed.
    /// `concat!` of COMPLETE per-line literals, never a backslash
    /// continuation. `check-fn-length.sh` strips string literals before
    /// counting braces, but it strips them LINE BY LINE: a continued
    /// literal loses its opening quote after the first line, so the
    /// `{ command: … }` below read as real code braces and the ratchet
    /// measured this fixture at 173 lines instead of 10.
    const YAML_DIAMOND: &str = concat!(
        "nika: t\n",
        "permits: { exec: [\"echo\"] }\n",
        "tasks:\n",
        "  a:\n    exec: { command: [\"echo\", \"a\"] }\n",
        "  b:\n    with: { x: \"${{ tasks.a.output }}\" }\n",
        "    exec: { command: [\"echo\", \"b\"] }\n",
        "  b2:\n    with: { x: \"${{ tasks.a.output }}\" }\n",
        "    exec: { command: [\"echo\", \"b2\"] }\n",
        "  c:\n    with: { y: \"${{ tasks.b.output }}\" }\n",
        "    exec: { command: [\"echo\", \"c\"] }\n",
    );

    fn checked(yaml: &str) -> (RawWorkflow, nika_check::CheckReport) {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::source::FileId::new(0),
            nika_schema::parser::ParseMode::Strict,
        )
        .expect("parse");
        let report = nika_check::check(&wf);
        (wf, report)
    }

    /// ⭐ THE PARITY PIN. `derive::waves` (1 + max over `needs`) and the
    /// engine's `report.waves` (Kahn levels over the derived edges) are
    /// two implementations of one law. This test is the reason the
    /// converter reads `derive_edges` and not the graph projection.
    #[test]
    fn waves_agree_with_the_engine() {
        let (wf, report) = checked(YAML_DIAMOND);
        assert!(!report.waves.is_empty(), "the fixture must be checkable");
        let model_wf = workflow_of(&wf, "t.nika.yaml");
        let ours = derive::waves(&model_wf);

        assert_eq!(
            ours.len(),
            report.waves.len(),
            "wave COUNT diverged: {} vs the engine's {}",
            ours.len(),
            report.waves.len()
        );
        for (index, wave) in report.waves.iter().enumerate() {
            for &task in wave {
                let id = &wf.tasks[task].value.id.value;
                assert_eq!(
                    ours.of(id),
                    index,
                    "`{id}` sits in wave {} for us, {index} for the engine",
                    ours.of(id)
                );
            }
        }
    }

    /// The converter carries the dependency edges — not just `after:`,
    /// the IMPLICIT `${{ tasks.X }}` ones too (the whole reason it reads
    /// the derived set rather than the declared `needs:`).
    #[test]
    fn implicit_data_edges_become_needs() {
        let (wf, _) = checked(YAML_DIAMOND);
        let model_wf = workflow_of(&wf, "t.nika.yaml");
        let by_id: BTreeMap<&str, &model::Task> =
            model_wf.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
        assert_eq!(by_id["a"].needs, Vec::<String>::new(), "a is a root");
        assert_eq!(by_id["b"].needs, vec!["a".to_owned()]);
        assert_eq!(by_id["c"].needs, vec!["b".to_owned()]);
    }

    fn step(id: &str, start: f64, dur: f64, cost: Option<f64>) -> model::Step {
        let mut s = model::Step::new(id.to_owned(), start, dur);
        s.cost = cost;
        s
    }

    fn run_of(steps: Vec<model::Step>) -> model::Run {
        model::Run::new(
            "019f8148-0287-75b0-bb03-00dcf087000a".to_owned(),
            String::new(),
            String::new(),
            steps,
        )
    }

    /// The four numbers land, and the holder is NAMED — b2 holds wave 1
    /// while b waits on it.
    #[test]
    fn the_digest_names_the_holder_and_the_spend() {
        let (wf, _) = checked(YAML_DIAMOND);
        let model_wf = workflow_of(&wf, "t.nika.yaml");
        let run = run_of(vec![
            step("a", 0.0, 1.0, Some(0.10)),
            step("b", 1.0, 1.0, Some(0.02)),
            step("b2", 1.0, 5.0, Some(0.00)),
            step("c", 6.0, 1.0, Some(0.30)),
        ]);
        let text = render_session(&model_wf, &run, PLAIN);

        assert!(text.contains("3 waves"), "wave count: {text}");
        assert!(
            text.contains("bottleneck b2"),
            "the holder is named: {text}"
        );
        assert!(text.contains("1 blocked step"), "the blocked count: {text}");
        assert!(text.contains("$0.42 spent"), "the total: {text}");
        assert!(text.contains("exec $0.42"), "the spend by verb: {text}");
        assert!(text.contains("7.0s wall"), "the wall: {text}");
    }

    /// ADR-128 · a run nobody metered never reads « $0.00 spent »: the
    /// digest says no spend was metered.
    #[test]
    fn an_unmetered_run_says_so_never_a_zero() {
        let (wf, _) = checked(YAML_DIAMOND);
        let model_wf = workflow_of(&wf, "t.nika.yaml");
        let run = run_of(vec![
            step("a", 0.0, 1.0, None),
            step("b", 1.0, 1.0, None),
            step("b2", 1.0, 5.0, None),
            step("c", 6.0, 1.0, None),
        ]);
        let text = render_session(&model_wf, &run, PLAIN);
        assert!(text.contains("no spend metered"), "{text}");
        assert!(
            !text.contains("$0.00 spent"),
            "a zero nobody metered: {text}"
        );
    }

    /// ⭐ A bottleneck that costs nothing ISN'T one. Two near-equal steps:
    /// one finishes "last" and must NOT be crowned.
    #[test]
    fn a_holder_nobody_waited_for_is_not_named() {
        let (wf, _) = checked(YAML_DIAMOND);
        let model_wf = workflow_of(&wf, "t.nika.yaml");
        let run = run_of(vec![
            step("a", 0.0, 1.0, None),
            step("b", 1.0, 1.0, None),
            step("b2", 1.0, 1.0 + BLOCKED_FLOOR_S / 2.0, None),
            step("c", 3.0, 1.0, None),
        ]);
        let text = render_session(&model_wf, &run, PLAIN);
        assert!(
            text.contains("no wave holder"),
            "a tie must not crown anyone: {text}"
        );
    }

    /// No holder is painted over a failure.
    #[test]
    fn a_broken_run_names_no_holder() {
        let (wf, _) = checked(YAML_DIAMOND);
        let model_wf = workflow_of(&wf, "t.nika.yaml");
        let mut steps = vec![
            step("a", 0.0, 1.0, None),
            step("b", 1.0, 1.0, None),
            step("b2", 1.0, 5.0, None),
        ];
        steps[1].failed = Some(model::Failure::new(
            "NIKA-1201".to_owned(),
            "exec refused".to_owned(),
        ));
        let text = render_session(&model_wf, &run_of(steps), PLAIN);
        assert!(text.contains("the run broke"), "{text}");
        assert!(
            !text.contains("bottleneck"),
            "no holder over a failure: {text}"
        );
    }

    /// The receipt is a CLAIM, and it is withheld when the journal
    /// carries no trace id.
    #[test]
    fn a_journal_without_a_trace_id_withholds_the_receipt() {
        let (wf, _) = checked(YAML_DIAMOND);
        let model_wf = workflow_of(&wf, "t.nika.yaml");
        let run = model::Run::new(
            String::new(),
            String::new(),
            String::new(),
            vec![step("a", 0.0, 1.0, None)],
        );
        let text = render_session(&model_wf, &run, PLAIN);
        assert!(text.contains("the receipt is not"), "{text}");
    }

    /// The journal fold is the crate's, and its refusal rides through:
    /// half a journal is no journal.
    #[test]
    fn a_torn_journal_refuses_with_its_line() {
        let err = ingress::run_from_journal("{\"kind\":\"workflow_started\"}\nnot json\n")
            .expect_err("a torn journal refuses");
        assert!(format!("{err}").contains("line 2"), "{err}");
    }
}
