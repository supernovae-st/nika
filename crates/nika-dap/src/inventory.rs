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
///
/// Entries are visited in a STABLE order (sorted per directory), so the
/// filesystem's iteration order never leaks into a render. The return is
/// the honesty flag (P0-4 · audit UX 2026-07-30): `true` = TRUNCATED —
/// the budget died with work unvisited, an entry errored, or a directory
/// could not be read. A truncated walk's empty `out` is UNKNOWN, never
/// « zero workflows »; only `false` certifies the list is complete. The
/// depth cap is a design boundary, not truncation: reaching it is `false`.
pub fn collect_workflow_paths(
    root: &Path,
    dir: &Path,
    depth: u8,
    budget: &mut usize,
    out: &mut Vec<PathBuf>,
) -> bool {
    if depth == 0 {
        return false;
    }
    if *budget == 0 {
        return true; // called with work to do and nothing left to spend
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return true; // an unreadable dir may hide workflows — never « zero »
    };
    let mut sorted = Vec::new();
    let mut truncated = false;
    for entry in entries {
        match entry {
            Ok(entry) => sorted.push(entry),
            Err(_) => truncated = true, // an entry we could not stat is unknown
        }
    }
    sorted.sort_by_key(std::fs::DirEntry::file_name);
    for entry in sorted {
        if *budget == 0 {
            return true; // died with entries still unvisited
        }
        *budget -= 1;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            truncated |= collect_workflow_paths(root, &path, depth - 1, budget, out);
        } else if name.ends_with(".nika.yaml") || name.ends_with(".nika.yml") {
            out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
    truncated
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
// Independent report FLAGS on a wire struct, not a state machine — the
// same flags-are-flags exemption WorkflowFact above carries.
#[allow(clippy::struct_excessive_bools)]
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
    /// `true` when the WALK itself gave up (budget died · unreadable
    /// directory) — the counts above are a lower bound, never « zero »
    /// (P0-4 · audit UX 2026-07-30). Reported exactly like
    /// [`Workspace::workflows_capped`], never silent.
    pub walk_truncated: bool,
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
/// `*.nika.yaml` through the in-process check ladder. Returns the facts,
/// the `MAX_WORKFLOWS` cap flag, the total found, and the walk's own
/// truncation flag (P0-4: a budget-killed or unreadable tree is reported
/// exactly like the cap, never read as « zero »).
#[must_use]
pub fn collect_workflows(root: &Path) -> (Vec<WorkflowFact>, bool, usize, bool) {
    let mut paths = Vec::new();
    let mut budget = WALK_BUDGET;
    let walk_truncated = collect_workflow_paths(root, root, 6, &mut budget, &mut paths);
    paths.sort();
    let found = paths.len();
    let capped = found > MAX_WORKFLOWS;
    paths.truncate(MAX_WORKFLOWS);
    (
        paths.iter().map(|p| audit(root, p)).collect(),
        capped,
        found,
        walk_truncated,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// P0-4 (audit UX 2026-07-30): an injected tiny budget dies before the
    /// walk ever reaches the workflow's directory — the return MUST carry
    /// `truncated: true`, so no caller can read the short list as « zero ».
    /// The noise sorts BEFORE `z/` by the walk's own stable order, so the
    /// budget provably dies upstream of the workflow.
    #[test]
    fn an_exhausted_budget_reports_truncated_never_a_silent_short_list() {
        let dir = tempfile::tempdir().expect("scratch");
        for i in 0..8 {
            std::fs::write(dir.path().join(format!("noise-{i}.txt")), "x").expect("write");
        }
        std::fs::create_dir(dir.path().join("z")).expect("mkdir");
        std::fs::write(dir.path().join("z/flow.nika.yaml"), "x").expect("write");

        let mut out = Vec::new();
        let mut budget = 3; // dies inside the noise — z/ is never entered
        let truncated = collect_workflow_paths(dir.path(), dir.path(), 4, &mut budget, &mut out);
        assert!(truncated, "a budget that died mid-walk IS truncation");
        assert!(out.is_empty(), "the workflow was never reached: {out:?}");

        // The SAME tree with a living budget: complete, the file seen.
        let mut out = Vec::new();
        let mut budget = WALK_BUDGET;
        let truncated = collect_workflow_paths(dir.path(), dir.path(), 4, &mut budget, &mut out);
        assert!(!truncated, "a full walk is complete");
        assert_eq!(out, vec![PathBuf::from("z/flow.nika.yaml")]);
    }

    /// A directory the walk cannot READ may hide workflows — silence here
    /// is the exact « an FS error renders as zero » lie the finding names.
    #[test]
    fn an_unreadable_dir_reports_truncated_never_zero() {
        let dir = tempfile::tempdir().expect("scratch");
        let ghost = dir.path().join("does-not-exist");
        let mut out = Vec::new();
        let mut budget = WALK_BUDGET;
        let truncated = collect_workflow_paths(dir.path(), &ghost, 4, &mut budget, &mut out);
        assert!(truncated, "an unreadable dir is unknown, never zero");
        assert!(out.is_empty());
    }

    /// FS order is not an input: the SAME file set created in two
    /// different orders walks to the SAME sequence (the mirror family
    /// prints what the walk returns — the stable sort lives here, once).
    #[test]
    fn the_walk_order_is_fs_invariant() {
        let names = [
            "b.nika.yaml",
            "a.nika.yaml",
            "sub/c.nika.yaml",
            "sub/a.nika.yaml",
        ];
        let layout = |order: &[usize]| {
            let dir = tempfile::tempdir().expect("scratch");
            std::fs::create_dir(dir.path().join("sub")).expect("mkdir");
            for &i in order {
                std::fs::write(dir.path().join(names[i]), "x").expect("write");
            }
            dir
        };
        let walk = |dir: &Path| {
            let mut out = Vec::new();
            let mut budget = WALK_BUDGET;
            let truncated = collect_workflow_paths(dir, dir, 4, &mut budget, &mut out);
            assert!(!truncated);
            out
        };
        let one = layout(&[0, 1, 2, 3]);
        let two = layout(&[3, 2, 1, 0]);
        let a = walk(one.path());
        let b = walk(two.path());
        assert_eq!(a, b, "creation order must not leak into the render");
        let mut sorted = a.clone();
        sorted.sort();
        assert_eq!(a, sorted, "the walk emits the canonical sorted order");
    }
}
