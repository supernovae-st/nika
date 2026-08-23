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

pub(crate) fn list_workflows(
    root: &OwnedDir,
    limits: SnapshotLimits,
) -> Result<Vec<String>, ServerError> {
    let mut workflows = Vec::new();
    collect(root, "", 0, limits, &mut workflows)?;
    workflows.sort();
    Ok(workflows)
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
