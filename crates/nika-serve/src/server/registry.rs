// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::path::{Component, Path};

use nika_execution::SnapshotLimits;
use nika_fs::OwnedDir;

use super::ServerError;

pub(crate) fn valid_workflow_name(value: &str) -> bool {
    let path = Path::new(value);
    value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
        && !value.contains('\\')
        && value.ends_with(".nika.yaml")
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(name) if !name.is_empty()))
}

/// The workflows under `root`, named `prefix/…` (#1369): the served
/// registry is walked from the `--workflows` directory and named from the
/// project root, so the names stay the ones a schedule declares.
pub(crate) fn list_workflows_under(
    root: &OwnedDir,
    prefix: &str,
    limits: SnapshotLimits,
) -> Result<Vec<String>, ServerError> {
    let mut workflows = Vec::new();
    collect(root, prefix, 0, limits, &mut workflows)?;
    workflows.sort();
    Ok(workflows)
}

/// The served registry's scope (#1369): the project-relative prefix of the
/// `--workflows` directory when it lies inside the resident's project root —
/// the listener exposes and schedules only what lives under it. `None` when
/// the two roots coincide, or when the served root is not inside the project
/// (it is then a project of its own, as before).
pub(crate) fn registry_scope(resident: Option<&Path>, served: &Path) -> Option<String> {
    let resident = std::fs::canonicalize(resident?).ok()?;
    let served = std::fs::canonicalize(served).ok()?;
    let inside = served.strip_prefix(&resident).ok()?;
    let mut parts = Vec::new();
    for component in inside.components() {
        parts.push(component.as_os_str().to_str()?.to_owned());
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Whether `name` (project-relative) lies under the served registry's scope.
pub(crate) fn within_scope(scope: Option<&str>, name: &str) -> bool {
    scope.is_none_or(|prefix| name.starts_with(prefix) && name[prefix.len()..].starts_with('/'))
}

pub(crate) fn workflow_exists(root: &OwnedDir, name: &str) -> bool {
    if !valid_workflow_name(name) {
        return false;
    }
    root.open_relative(Path::new(name)).is_ok()
}

fn collect(
    dir: &OwnedDir,
    prefix: &str,
    depth: usize,
    limits: SnapshotLimits,
    out: &mut Vec<String>,
) -> Result<(), ServerError> {
    if out.len() >= limits.max_units() {
        return Ok(());
    }
    let mut names = dir
        .names()
        .map_err(|error| ServerError::WorkflowRoot(error.kind()))?;
    names.sort();
    for name in names {
        if out.len() >= limits.max_units() {
            break;
        }
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if name.ends_with(".nika.yaml") {
            if dir.exists(&name).unwrap_or(false) {
                out.push(relative);
            }
            continue;
        }
        if depth >= limits.max_depth() {
            continue;
        }
        if let Ok(child) = dir.open_below(&[name.as_str()]) {
            collect(&child, &relative, depth.saturating_add(1), limits, out)?;
        }
    }
    Ok(())
}
