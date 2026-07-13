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

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use nika_event::fold::{self, RunFact};
use serde::Serialize;

use crate::display::theme::{Role, Theme};
use crate::verbs::{VerbOutput, probe};

/// Most workflow files one context call audits — beyond this an agent
/// should scope by directory (the cap is REPORTED, never silent).
const MAX_WORKFLOWS: usize = 100;

/// Most recent run journals folded (newest first).
const MAX_RUNS: usize = 20;

/// Directory-walk budget (entries visited) — a context call must stay
/// instant on a monorepo and never wander into dependency trees.
const WALK_BUDGET: usize = 20_000;

/// One workflow file's audited facts — verdicts, never contents.
// Independent verdict FLAGS on a wire struct, not a state machine —
// the same flags-are-flags exemption the Theme struct carries.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize)]
struct WorkflowFact {
    /// Root-relative path (absolute paths leak usernames).
    path: String,
    /// The `workflow:` id when the file parses (`None` = parse failed).
    workflow: Option<String>,
    /// The full `is_clean` verdict (ten finding surfaces).
    clean: bool,
    /// Clean AND zero `native-first` hints (the `--native-strict` bar).
    strict_clean: bool,
    /// Total findings across the report's surfaces (0 when clean).
    findings: usize,
    tasks: usize,
    waves: usize,
    /// Task count per verb (`infer` · `exec` · `invoke` · `agent`) —
    /// the shape an agent reasons about before reading the file.
    verbs: BTreeMap<&'static str, usize>,
    /// Worst-case bounded cost in USD (the ceiling — or the FLOOR part
    /// when `cost_is_floor`).
    cost_bounded_usd: f64,
    /// `true` = at least one task is unbounded: the number above is a
    /// FLOOR, not a ceiling (unknown stays unknown · never $0).
    cost_is_floor: bool,
    /// A `permits:` boundary is declared (default-deny active).
    permits_declared: bool,
}

/// The workspace half of the aggregate (pure over collected facts).
#[derive(Debug, Serialize)]
struct Workspace {
    root: String,
    git: bool,
    workflows: Vec<WorkflowFact>,
    /// `true` when `MAX_WORKFLOWS` cut the inventory — scope by directory.
    workflows_capped: bool,
    /// How many the walk FOUND (≥ the emitted list when capped — the
    /// K8s limit/continue rule: a cap without a total reads as « all »).
    workflows_total_found: usize,
    runs: Vec<RunFact>,
    runs_capped: bool,
    runs_total_found: usize,
}

/// Cross-file rollups an agent reasons about before touching anything.
#[derive(Debug, Serialize)]
struct Rollups {
    workflows_total: usize,
    workflows_clean: usize,
    workflows_with_findings: usize,
    /// Σ bounded worst-case USD across clean-parsing files.
    cost_bounded_usd: f64,
    /// Any file whose ceiling is a floor makes the SUM a floor too.
    cost_is_floor: bool,
    permits_declared: usize,
    /// Σ recorded spend across the folded runs (only events that carried
    /// a cost — unpriced calls are COUNTED, never priced at $0).
    runs_cost_usd: f64,
    runs_unpriced_calls: u64,
}

/// The `nika welcome --deep` arm. `json` emits `context_version: 1`.
#[must_use]
pub fn run(json: bool, theme: Theme) -> VerbOutput {
    let root = Path::new(".");
    let (facts, capped, found) = collect_workflows(root);
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

/// Walk the root (bounded · dot/dep dirs skipped) and audit every
/// `*.nika.yaml` through the in-process check ladder.
fn collect_workflows(root: &Path) -> (Vec<WorkflowFact>, bool, usize) {
    let mut paths = Vec::new();
    let mut budget = WALK_BUDGET;
    probe::collect_workflow_paths(root, root, 6, &mut budget, &mut paths);
    paths.sort();
    let found = paths.len();
    let capped = found > MAX_WORKFLOWS;
    paths.truncate(MAX_WORKFLOWS);
    (
        paths.iter().map(|p| audit(root, p)).collect(),
        capped,
        found,
    )
}

/// Audit one file — parse + the check ladder, folded to verdict facts.
/// A file that does not read or parse is an inventory row with
/// `clean: false` (an agent must SEE it, not silently miss it).
fn audit(root: &Path, rel: &Path) -> WorkflowFact {
    let path = rel.display().to_string();
    let unread = |findings: usize| WorkflowFact {
        path: path.clone(),
        workflow: None,
        clean: false,
        strict_clean: false,
        findings,
        tasks: 0,
        waves: 0,
        verbs: BTreeMap::default(),
        cost_bounded_usd: 0.0,
        cost_is_floor: false,
        permits_declared: false,
    };
    let Ok(yaml) = std::fs::read_to_string(root.join(rel)) else {
        return unread(1);
    };
    let Ok(wf) = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    ) else {
        return unread(1);
    };
    let report = nika_schema::check(&wf);
    let mut verbs: BTreeMap<&'static str, usize> = BTreeMap::default();
    for task in &wf.tasks {
        *verbs.entry(task.value.action.verb()).or_insert(0) += 1;
    }
    let findings = report.conformance.len()
        + report.secret_leaks.len()
        + report.secret_egresses.len()
        + report.capability_escapes.len()
        + report.schema_findings.len()
        + report.schema_lints.len()
        + report.unknown_tools.len()
        + report.unknown_args.len()
        + report.missing_args.len()
        + report.gate_findings.len();
    let native_first_hints = report
        .hints
        .iter()
        .filter(|h| h.kind == "native-first")
        .count();
    WorkflowFact {
        path,
        workflow: wf.workflow.as_ref().map(|w| w.value.clone()),
        clean: report.is_clean(),
        strict_clean: report.is_clean() && native_first_hints == 0,
        findings,
        tasks: wf.tasks.len(),
        waves: report.waves.len(),
        verbs,
        cost_bounded_usd: report.cost.bounded_total_usd,
        cost_is_floor: report.cost.has_unbounded,
        permits_declared: wf.permits.is_some(),
    }
}

fn rollups(facts: &[WorkflowFact], runs: &[RunFact]) -> Rollups {
    Rollups {
        workflows_total: facts.len(),
        workflows_clean: facts.iter().filter(|f| f.clean).count(),
        workflows_with_findings: facts.iter().filter(|f| !f.clean).count(),
        cost_bounded_usd: facts.iter().map(|f| f.cost_bounded_usd).sum(),
        cost_is_floor: facts.iter().any(|f| f.cost_is_floor),
        permits_declared: facts.iter().filter(|f| f.permits_declared).count(),
        runs_cost_usd: runs.iter().filter_map(|r| r.cost_usd).sum(),
        runs_unpriced_calls: runs.iter().filter_map(|r| r.unpriced_calls).sum(),
    }
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
        let _ = writeln!(s, "no workflows here yet — nika new scaffolds one");
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
            format!("≥ ${:.4}", f.cost_bounded_usd)
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
    let _ = writeln!(s);
    let floor = if rollups.cost_is_floor { "≥" } else { "≤" };
    let _ = writeln!(
        s,
        "{} {} clean / {} total · {floor} ${:.4} · {} recorded",
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
    use crate::verbs::exit;

    fn plain() -> Theme {
        Theme::new(false, false, false)
    }

    /// A scratch workspace with two workflows (one clean, one with a
    /// finding) and one recorded journal.
    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nika-context-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
        ));
        std::fs::create_dir_all(dir.join("flows")).expect("mkdir");
        std::fs::create_dir_all(dir.join("node_modules")).expect("mkdir");
        std::fs::create_dir_all(dir.join(".nika/traces")).expect("mkdir");
        std::fs::write(
            dir.join("good.nika.yaml"),
            "nika: v1\nworkflow: good\nmodel: mock/echo\ntasks:\n  - id: a\n    infer: { prompt: \"x\", max_tokens: 10 }\n",
        )
        .expect("write");
        // `when:` as a bare string = a conformance finding.
        std::fs::write(
            dir.join("flows/bad.nika.yaml"),
            "nika: v1\nworkflow: bad\ntasks:\n  - id: a\n    exec: { command: [\"echo\", \"x\"] }\n  - id: b\n    depends_on: [a]\n    when: maybe\n    exec: { command: [\"echo\", \"y\"] }\n",
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
        let (facts, capped, found) = collect_workflows(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert!(!capped);
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
        let (facts, capped, found) = collect_workflows(&dir);
        let (runs, runs_capped, runs_found) =
            fold::fold_traces(&dir.join(".nika/traces"), MAX_RUNS);
        std::fs::remove_dir_all(&dir).ok();
        let workspace = Workspace {
            root: ".".to_owned(),
            git: false,
            workflows: facts,
            workflows_capped: capped,
            workflows_total_found: found,
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
