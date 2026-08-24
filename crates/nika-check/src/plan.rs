// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The versioned, effect-free run-plan projection.

use std::path::{Path, PathBuf};

use nika_schema::raw::RawWorkflow;

use crate::CheckReport;

/// Project the checked workflow into the machine-readable dry-run contract.
///
/// When an ancestor `nika.yaml` carries `ceiling:`, the plan names that
/// spend cap and its provenance (`config.max_cost_usd` + `run_budget`).
/// `nika run --dry-run --json` prints this object; the walk is the same
/// git-style discovery `nika run` uses, started from the workflow file
/// so a preview of `prod.nika.yaml` sees the file that would govern a
/// run from that directory (#1050).
#[must_use]
pub fn payload(file: &str, wf: &RawWorkflow, report: &CheckReport) -> serde_json::Value {
    let ids: Vec<&str> = wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect();
    let waves: Vec<Vec<&str>> = report
        .waves
        .iter()
        .map(|wave| {
            wave.iter()
                .filter_map(|&index| ids.get(index).copied())
                .collect()
        })
        .collect();
    let tasks: Vec<serde_json::Value> = wf
        .tasks
        .iter()
        .map(|task| {
            serde_json::json!({
                "id": task.value.id.value,
                "verb": task.value.action.verb(),
            })
        })
        .collect();
    let mut plan = serde_json::json!({
        "plan_version": 1,
        "workflow": wf.workflow.as_ref().map(|name| name.value.as_str()),
        "file": file,
        "dry_run": true,
        "effects_executed": false,
        "waves": waves,
        "tasks": tasks,
        "cost": report.cost,
        "permits": report.permits,
        "requirements": report.requirements,
    });
    if let Some(budget) = ambient_budget(file)
        && let Some(obj) = plan.as_object_mut()
    {
        obj.insert(
            "config".to_owned(),
            serde_json::json!({ "max_cost_usd": budget.clone() }),
        );
        obj.insert("run_budget".to_owned(), budget);
    }
    plan
}

/// Presence-gated: silence when no project file sets a ceiling.
fn ambient_budget(file: &str) -> Option<serde_json::Value> {
    let (path, project) = nika_vocab::project::discover(&walk_start(file))
        .ok()
        .flatten()?;
    let usd = project.ceiling?;
    let mut v = serde_json::json!({
        "max_cost_usd": usd,
        "source": path.display().to_string(),
        "via": "project",
    });
    if let Some(line) = ceiling_line(&path)
        && let Some(o) = v.as_object_mut()
    {
        o.insert("line".to_owned(), serde_json::json!(line));
    }
    Some(v)
}

fn walk_start(file: &str) -> PathBuf {
    let path = Path::new(file);
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    match parent {
        Some(dir) if dir.is_absolute() => dir.to_path_buf(),
        Some(dir) => std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(dir),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

fn ceiling_line(path: &Path) -> Option<usize> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().enumerate().find_map(|(i, raw)| {
        let t = raw.trim_start();
        t.starts_with("ceiling:").then_some(i + 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::ParseMode;
    use nika_schema::source::FileId;

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse")
    }

    fn fixture_wf() -> RawWorkflow {
        parse(
            "nika: wf\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n",
        )
    }

    fn fresh(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nika-plan-budget-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    /// #1050 · an ancestor `nika.yaml` ceiling must ride the plan object
    /// with path:line provenance. A dry-run JSON of this file used to
    /// show only the workflow cost envelope, never the spend cap the
    /// run would honour.
    #[test]
    fn plan_names_the_ancestor_ceiling_with_provenance() {
        let root = fresh("ancestor");
        let child = root.join("sub");
        std::fs::create_dir_all(&child).expect("mkdir");
        std::fs::write(root.join("nika.yaml"), "nika: proj\nceiling: 0.01\n").expect("seed");
        let wf_path = child.join("prod.nika.yaml");
        std::fs::write(
            &wf_path,
            "nika: wf\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
        )
        .expect("wf");
        let wf = fixture_wf();
        let report = crate::check(&wf);
        let p = payload(wf_path.to_str().expect("utf8"), &wf, &report);
        assert_eq!(p["plan_version"], 1);
        assert_eq!(p["run_budget"]["max_cost_usd"], 0.01, "{p}");
        assert_eq!(p["run_budget"]["via"], "project");
        assert_eq!(p["run_budget"]["line"], 2);
        assert_eq!(p["config"]["max_cost_usd"]["max_cost_usd"], 0.01);
        assert!(
            p["run_budget"]["source"]
                .as_str()
                .is_some_and(|s| s.ends_with("nika.yaml")),
            "{p}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_project_file_without_ceiling_leaves_the_plan_silent() {
        let dir = fresh("none");
        std::fs::write(dir.join("nika.yaml"), "nika: proj\n").expect("seed");
        let wf_path = dir.join("wf.nika.yaml");
        std::fs::write(
            &wf_path,
            "nika: wf\ntasks:\n  t:\n    infer: { prompt: hi }\n",
        )
        .expect("wf");
        let wf = fixture_wf();
        let report = crate::check(&wf);
        let p = payload(wf_path.to_str().expect("utf8"), &wf, &report);
        assert!(p.get("run_budget").is_none(), "{p}");
        assert!(p.get("config").is_none(), "{p}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_nearest_project_file_wins() {
        let root = fresh("nearest");
        let child = root.join("leaf");
        std::fs::create_dir_all(&child).expect("mkdir");
        std::fs::write(root.join("nika.yaml"), "nika: root\nceiling: 9.99\n").expect("root");
        std::fs::write(child.join("nika.yaml"), "nika: leaf\nceiling: 0.25\n").expect("leaf");
        let wf_path = child.join("wf.nika.yaml");
        std::fs::write(
            &wf_path,
            "nika: wf\ntasks:\n  t:\n    infer: { prompt: hi }\n",
        )
        .expect("wf");
        let wf = fixture_wf();
        let report = crate::check(&wf);
        let p = payload(wf_path.to_str().expect("utf8"), &wf, &report);
        assert_eq!(p["run_budget"]["max_cost_usd"], 0.25, "{p}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
