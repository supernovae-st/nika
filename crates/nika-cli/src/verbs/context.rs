// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika welcome --deep` — the whole workspace truth in ONE call (the agent
//! aggregate · 30s-arc W4). Composition ONLY: welcome's bounded walk +
//! THE in-process check ladder (the seam MCP `nika_check` speaks) +
//! trace-journal folds + the shared `verbs::probe` engine — no new
//! analysis, one truth, one more renderer.
//!
//! The wire (`--json` · `context_version: 1` · additive-only): FACTS
//! never file contents · RELATIVE paths only (absolute paths leak
//! usernames into agent transcripts) · every array CAPPED and says so
//! (silent truncation reads as « covered everything »).

use std::fmt::Write as _;

use nika_dap::inventory::{Rollups, Workspace, collect_workflows, rollups};
use nika_event::fold::{self};

use crate::display::theme::{Role, Theme};
use crate::verbs::{VerbOutput, probe};

/// Most recent run journals folded (newest first).
const MAX_RUNS: usize = 20;

/// The `nika welcome --deep` arm. `json` emits `context_version: 1`.
#[must_use]
pub fn run(json: bool, theme: Theme) -> VerbOutput {
    let root = std::path::Path::new(".");
    let (facts, capped, found, walk_truncated) = collect_workflows(root);
    let (runs, runs_capped, runs_found) =
        fold::fold_traces(&root.join(".nika").join("traces"), MAX_RUNS);
    let probe = probe::collect(false);
    let workspace = Workspace {
        root: ".".to_owned(),
        git: root
            .canonicalize()
            .unwrap_or_else(|_| root.to_path_buf())
            .ancestors()
            .any(|a| a.join(".git").exists()),
        workflows: facts,
        workflows_capped: capped,
        workflows_total_found: found,
        walk_truncated,
        runs,
        runs_capped,
        runs_total_found: runs_found,
    };
    let rollups = rollups(&workspace.workflows, &workspace.runs);
    if json {
        return VerbOutput::ok(render_json(&workspace, &rollups, &probe));
    }
    VerbOutput::ok(render_human(&workspace, &rollups, &probe, theme))
}

/// The versioned machine aggregate (`context_version: 1` · additive-only).
fn render_json(workspace: &Workspace, rollups: &Rollups, probe: &probe::Probe) -> String {
    serde_json::json!({
        "context_version": 1,
        "identity": {
            "version": probe.version,
            "pack_version": nika_pack::pack_version(),
        },
        "workspace": workspace,
        "rollups": rollups,
        "environment": probe::environment_json(probe),
    })
    .to_string()
}

/// The human map — one row per workflow, the rollup line, the hand-off.
fn render_human(
    workspace: &Workspace,
    rollups: &Rollups,
    probe: &probe::Probe,
    theme: Theme,
) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{} {} — workspace context",
        theme.logo(),
        theme.paint(Role::Strong, &format!("nika {}", probe.version)),
    );
    let _ = writeln!(s);
    if workspace.workflows.is_empty() {
        // P0-4: an empty list behind a TRUNCATED walk is unknown, never
        // « nothing here » — the zero claim is the complete walk's alone.
        if workspace.walk_truncated {
            let _ = writeln!(
                s,
                "scan partial — the walk gave up before covering the tree"
            );
        } else {
            let _ = writeln!(s, "no workflows here yet — nika new scaffolds one");
        }
        return s;
    }
    let width = workspace
        .workflows
        .iter()
        .map(|f| f.path.chars().count())
        .max()
        .unwrap_or(0);
    for f in &workspace.workflows {
        let verdict = if f.clean {
            theme.paint(Role::Good, "clean")
        } else {
            theme.paint(Role::Bad, &crate::text::count(f.findings, "finding"))
        };
        let cost = if f.cost_is_floor {
            // The priced PORTION with uncapped spend on top — never a
            // bound (`≥` here claimed one the number cannot make, the
            // 2026-07-29 FLOOR finding · one voice with check/explain).
            format!("~${:.4}+", f.cost_bounded_usd)
        } else {
            format!("≤ ${:.4}", f.cost_bounded_usd)
        };
        let _ = writeln!(
            s,
            "  {:<width$}  {verdict} · {} · {} · {cost}{}",
            f.path,
            crate::text::count(f.tasks, "task"),
            crate::text::count(f.waves, "wave"),
            if f.permits_declared {
                " · permits ✓".to_owned()
            } else {
                theme.paint(Role::Dim, " · permits —")
            },
        );
    }
    if workspace.workflows_capped {
        let _ = writeln!(
            s,
            "  {}",
            theme.paint(Role::Dim, "… inventory capped — scope by directory")
        );
    }
    if workspace.walk_truncated {
        // The WALK's own honesty flag (P0-4) — reported exactly like the
        // MAX_WORKFLOWS cap above, never silent.
        let _ = writeln!(
            s,
            "  {}",
            theme.paint(Role::Dim, "… scan partial — scope by directory")
        );
    }
    let _ = writeln!(s);
    let (mark, plus) = if rollups.cost_is_floor {
        ("~", "+") // priced portion + uncapped — never a bound (the finding)
    } else {
        ("≤", "")
    };
    let _ = writeln!(
        s,
        "{} {} clean / {} total · {mark} ${:.4}{plus} · {} recorded",
        theme.paint(Role::Strong, "rollup"),
        rollups.workflows_clean,
        rollups.workflows_total,
        rollups.cost_bounded_usd,
        crate::text::count(workspace.runs.len(), "run"),
    );
    let _ = writeln!(
        s,
        "{}",
        theme.paint(Role::Dim, "machine twin: nika welcome --deep --json"),
    );
    s
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use std::collections::BTreeMap;

    use nika_dap::inventory::WorkflowFact;
    use nika_event::fold::RunFact;

    use crate::verbs::exit;

    fn plain() -> Theme {
        Theme::new(false, false, false)
    }

    /// A scratch workspace with two workflows (one clean, one with a
    /// finding) and one recorded journal.
    fn scratch() -> PathBuf {
        // Uniqueness: pid + an atomic discriminator — NOT the wall
        // clock. `subsec_nanos` has ~1µs real granularity on macOS, so
        // two parallel tests could land in the same tick and SHARE the
        // dir: one test's cleanup then wiped the other's fixtures
        // mid-audit (the gate caught it 2026-07-21 — the audit's load
        // failed and the fact read `findings: 1, tasks: 0`).
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "nika-context-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(dir.join("flows")).expect("mkdir");
        std::fs::create_dir_all(dir.join("node_modules")).expect("mkdir");
        std::fs::create_dir_all(dir.join(".nika/traces")).expect("mkdir");
        std::fs::write(
            dir.join("good.nika.yaml"),
            "nika: good\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n",
        )
        .expect("write");
        // `when:` as a bare string = a conformance finding.
        std::fs::write(
            dir.join("flows/bad.nika.yaml"),
            "nika: bad\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n  b:\n    after:\n      a: success\n    when: maybe\n    exec: { command: [\"echo\", \"y\"] }\n",
        )
        .expect("write");
        // Hidden from the walk: dependency tree.
        std::fs::write(dir.join("node_modules/dep.nika.yaml"), "x").expect("write");
        // One journal: a started head + a completed tail with cost fields.
        std::fs::write(
            dir.join(".nika/traces/2026-07-08T20-00-00Z-abcd.ndjson"),
            concat!(
                r#"{"kind":"workflow_started","fields":[{"key":"workflow","value":"good"}]}"#,
                "\n",
                r#"{"kind":"workflow_completed","fields":[{"key":"workflow","value":"good"},{"key":"total_cost_usd","value":0.0123},{"key":"unpriced_calls","value":1}]}"#,
                "\n",
            ),
        )
        .expect("write");
        dir
    }

    #[test]
    fn the_inventory_audits_and_the_paths_stay_relative() {
        let dir = scratch();
        let (facts, capped, found, walk_truncated) = collect_workflows(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert!(!capped);
        assert!(!walk_truncated, "a small scratch tree walks complete");
        assert_eq!(found, 2);
        assert_eq!(facts.len(), 2, "{facts:?}");
        // Relative, sorted, dependency trees skipped.
        assert_eq!(facts[0].path, "flows/bad.nika.yaml");
        assert_eq!(facts[1].path, "good.nika.yaml");
        assert!(!facts[0].path.starts_with('/'), "no absolute paths");
        let bad = &facts[0];
        assert!(!bad.clean);
        assert!(bad.findings >= 1, "{bad:?}");
        let good = &facts[1];
        assert!(good.clean, "{good:?}");
        assert!(good.strict_clean);
        assert_eq!(good.tasks, 1);
        assert_eq!(good.workflow.as_deref(), Some("good"));
        assert!(!good.permits_declared);
    }

    #[test]
    fn journals_fold_from_head_and_tail_only() {
        let dir = scratch();
        let (runs, capped, found) = fold::fold_traces(&dir.join(".nika/traces"), MAX_RUNS);
        std::fs::remove_dir_all(&dir).ok();
        assert!(!capped);
        assert_eq!(found, 1);
        assert_eq!(runs.len(), 1);
        let run = &runs[0];
        assert_eq!(run.workflow.as_deref(), Some("good"));
        assert_eq!(run.verdict.as_deref(), Some("workflow_completed"));
        assert_eq!(run.cost_usd, Some(0.0123));
        assert_eq!(run.unpriced_calls, Some(1));
        assert!(run.trace.starts_with(".nika/traces/"), "{}", run.trace);
    }

    #[test]
    fn a_truncated_tail_folds_to_an_honest_unknown() {
        let dir = std::env::temp_dir().join(format!("nika-context-trunc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let journal = dir.join("crashed.ndjson");
        std::fs::write(
            &journal,
            concat!(
                r#"{"kind":"workflow_started","fields":[{"key":"workflow","value":"w"}]}"#,
                "\n",
                r#"{"kind":"task_started","fi"#, // the crash signature
            ),
        )
        .expect("write");
        let fact = fold::fold_one(&journal);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(fact.workflow.as_deref(), Some("w"));
        assert_eq!(fact.verdict, None, "a cut tail is unknown, never a claim");
        assert_eq!(fact.cost_usd, None);
    }

    #[test]
    fn rollups_sum_and_the_floor_is_contagious() {
        let mk = |clean: bool, cost: f64, floor: bool| WorkflowFact {
            path: "x".into(),
            workflow: None,
            clean,
            strict_clean: clean,
            findings: usize::from(!clean),
            tasks: 1,
            waves: 1,
            verbs: BTreeMap::default(),
            cost_bounded_usd: cost,
            cost_is_floor: floor,
            permits_declared: clean,
        };
        let runs = [RunFact {
            trace: "t".into(),
            workflow: None,
            verdict: Some("workflow_completed".into()),
            cost_usd: Some(0.02),
            unpriced_calls: Some(3),
        }];
        let r = rollups(&[mk(true, 0.10, false), mk(false, 0.05, true)], &runs);
        assert_eq!(r.workflows_total, 2);
        assert_eq!(r.workflows_clean, 1);
        assert_eq!(r.workflows_with_findings, 1);
        assert!((r.cost_bounded_usd - 0.15).abs() < 1e-9);
        assert!(r.cost_is_floor, "one floor makes the sum a floor");
        assert_eq!(r.permits_declared, 1);
        assert!((r.runs_cost_usd - 0.02).abs() < 1e-9);
        assert_eq!(r.runs_unpriced_calls, 3);
    }

    #[test]
    fn the_verb_emits_a_versioned_value_free_wire() {
        // Run in a scratch cwd via the pure pieces (the verb reads `.`;
        // the wire shape is what we pin here).
        let dir = scratch();
        let (facts, capped, found, walk_truncated) = collect_workflows(&dir);
        let (runs, runs_capped, runs_found) =
            fold::fold_traces(&dir.join(".nika/traces"), MAX_RUNS);
        std::fs::remove_dir_all(&dir).ok();
        let workspace = Workspace {
            root: ".".to_owned(),
            git: false,
            workflows: facts,
            workflows_capped: capped,
            workflows_total_found: found,
            walk_truncated,
            runs,
            runs_capped,
            runs_total_found: runs_found,
        };
        let roll = rollups(&workspace.workflows, &workspace.runs);
        let probe = probe::collect(false);
        let raw = render_json(&workspace, &roll, &probe);
        let v: serde_json::Value = serde_json::from_str(&raw).expect("parses");
        assert_eq!(v["context_version"], 1);
        assert_eq!(
            v["workspace"]["workflows"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(v["rollups"]["workflows_clean"], 1);
        assert_eq!(
            v["workspace"]["walk_truncated"], false,
            "the wire reports the walk's own honesty flag (P0-4)"
        );
        assert!(v["identity"]["version"].is_string());
        assert!(v["identity"]["pack_version"].is_string());
        assert!(
            !raw.contains("API_KEY") && !raw.contains("key_present"),
            "counts, never per-key facts: {raw}"
        );
        // The human render exits 0 and hands over to the twin.
        let out = run(false, plain());
        assert_eq!(out.code, exit::OK);
    }
}
