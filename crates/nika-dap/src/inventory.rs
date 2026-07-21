// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workspace inventory — which `*.nika.yaml` files exist and how
//! each one AUDITS (the parse + check-ladder fold to verdict facts).
//! Descended from `nika-cli`'s `verbs::context` (2026-07-21 · the 15k
//! wall — the drift family's sibling: drift audits one file's
//! declarations against its body, this audits the workspace's file
//! SET). The CLI keeps the probe fold, the workspace assembly, and
//! both renders (human · `--json`); the walk, the per-file audit, and
//! the cross-file rollups are pure forensics and live here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nika_event::fold::RunFact;
use serde::Serialize;

/// Directories the mirror-family workspace walks never enter —
/// dependency/build trees dwarf the workspace and these surfaces have
/// a latency budget, not a completeness one (welcome · context).
pub const SKIP_DIRS: [&str; 8] = [
    ".git",
    "node_modules",
    "target",
    ".venv",
    "venv",
    "dist",
    "build",
    "vendor",
];

/// Directory-walk budget (entries visited) — a context call must stay
/// instant on a monorepo and never wander into dependency trees.
pub const WALK_BUDGET: usize = 20_000;

/// Most workflows the inventory emits (the cap is REPORTED, never
/// silent — the K8s limit/continue rule).
pub const MAX_WORKFLOWS: usize = 100;

/// Bounded workspace walk: collect root-relative `*.nika.yaml` paths
/// (depth- and budget-capped · dot/dep dirs skipped). The ONE walk the
/// mirror family shares — welcome counts it, context audits it.
pub fn collect_workflow_paths(
    root: &Path,
    dir: &Path,
    depth: u8,
    budget: &mut usize,
    out: &mut Vec<PathBuf>,
) {
    if depth == 0 || *budget == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect_workflow_paths(root, &path, depth - 1, budget, out);
        } else if name.ends_with(".nika.yaml") || name.ends_with(".nika.yml") {
            out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
}

/// One workflow file's audited facts — verdicts, never contents.
// Independent verdict FLAGS on a wire struct, not a state machine —
// the same flags-are-flags exemption the Theme struct carries.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowFact {
    /// Root-relative path (absolute paths leak usernames).
    pub path: String,
    /// The `workflow:` id when the file parses (`None` = parse failed).
    pub workflow: Option<String>,
    /// The full `is_clean` verdict (ten finding surfaces).
    pub clean: bool,
    /// Clean AND zero `native-first` hints (the `--native-strict` bar).
    pub strict_clean: bool,
    /// Total findings across the report's surfaces (0 when clean).
    pub findings: usize,
    /// Task count.
    pub tasks: usize,
    /// Wave count.
    pub waves: usize,
    /// Task count per verb (`infer` · `exec` · `invoke` · `agent`) —
    /// the shape an agent reasons about before reading the file.
    pub verbs: BTreeMap<&'static str, usize>,
    /// Worst-case bounded cost in USD (the ceiling — or the FLOOR part
    /// when `cost_is_floor`).
    pub cost_bounded_usd: f64,
    /// `true` = at least one task is unbounded: the number above is a
    /// FLOOR, not a ceiling (unknown stays unknown · never $0).
    pub cost_is_floor: bool,
    /// A `permits:` boundary is declared (default-deny active).
    pub permits_declared: bool,
}

/// The workspace half of the aggregate (pure over collected facts).
#[derive(Debug, Serialize)]
pub struct Workspace {
    /// The walked root (always relative).
    pub root: String,
    /// A `.git` ancestor exists.
    pub git: bool,
    /// The audited workflows (capped at [`MAX_WORKFLOWS`]).
    pub workflows: Vec<WorkflowFact>,
    /// `true` when `MAX_WORKFLOWS` cut the inventory — scope by directory.
    pub workflows_capped: bool,
    /// How many the walk FOUND (≥ the emitted list when capped).
    pub workflows_total_found: usize,
    /// Most recent run journals folded (newest first).
    pub runs: Vec<RunFact>,
    /// `true` when the run fold was capped.
    pub runs_capped: bool,
    /// How many runs the fold found.
    pub runs_total_found: usize,
}

/// Cross-file rollups an agent reasons about before touching anything.
#[derive(Debug, Serialize)]
pub struct Rollups {
    /// Workflows in the emitted list.
    pub workflows_total: usize,
    /// Clean across the ten finding surfaces.
    pub workflows_clean: usize,
    /// With at least one finding.
    pub workflows_with_findings: usize,
    /// Σ bounded worst-case USD across clean-parsing files.
    pub cost_bounded_usd: f64,
    /// Any file whose ceiling is a floor makes the SUM a floor too.
    pub cost_is_floor: bool,
    /// Files declaring a `permits:` boundary.
    pub permits_declared: usize,
    /// Σ recorded spend across the folded runs (only events that carried
    /// a cost — unpriced calls are COUNTED, never priced at $0).
    pub runs_cost_usd: f64,
    /// Σ unpriced calls across the folded runs.
    pub runs_unpriced_calls: u64,
}

/// Walk the root (bounded · dot/dep dirs skipped) and audit every
/// `*.nika.yaml` through the in-process check ladder.
#[must_use]
pub fn collect_workflows(root: &Path) -> (Vec<WorkflowFact>, bool, usize) {
    let mut paths = Vec::new();
    let mut budget = WALK_BUDGET;
    collect_workflow_paths(root, root, 6, &mut budget, &mut paths);
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
    let report = nika_check::check(&wf);
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

/// The environment fragment both machine mirrors emit (welcome ·
/// context): client wiring booleans · local provider ids · cloud key
/// COUNTS. Names and counts by construction — no value exists to leak.
#[must_use]
pub fn rollups(facts: &[WorkflowFact], runs: &[RunFact]) -> Rollups {
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
