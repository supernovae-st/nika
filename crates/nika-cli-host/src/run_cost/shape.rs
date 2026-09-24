// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A finite upper bound over the existing checked DAG, never execution authority.
use nika_schema::raw::{RawAction, RawInvokeAction, RawWorkflow};
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

pub(super) fn review(
    wf: &RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    unknown_routes: usize,
) -> Result<u32, String> {
    if unknown_routes != 1 || plan.lanes.len() != 1 || !plan.is_admitted() {
        return Err("unknown-cost Run requires one exact admitted API route".into());
    }
    if !wf.secrets.is_empty() {
        return Err("unknown-cost Run cannot bind external secret inputs in this review".into());
    }
    let mut requests = 0_u32;
    for task in &wf.tasks {
        let task = &task.value;
        if task.for_each.is_some() || task.retry.is_some() || task.on_error.is_some() {
            return Err("unknown-cost Run does not support fan-out, retry or recovery".into());
        }
        match &task.action {
            RawAction::Infer(action) => {
                let model = action
                    .model
                    .as_ref()
                    .or(wf.model.as_ref())
                    .ok_or("unknown-cost Run requires a literal selected model")?;
                if model.value.contains("${{") || !plan.lanes.contains_key(&model.value) {
                    return Err(
                        "unknown-cost Run model differs from the selected static route".into(),
                    );
                }
                if action
                    .max_tokens
                    .as_ref()
                    .is_none_or(|n| n.value == 0 || n.value > 8192)
                    || action.thinking.is_some()
                    || !action.vision.is_empty()
                    || action.schema.is_some()
                {
                    return Err("unknown-cost Run requires text, max_tokens 1..8192, no thinking/vision or schema re-ask".into());
                }
                requests = requests.checked_add(1).ok_or("request bound overflow")?;
            }
            RawAction::Invoke(action) => match action.tool().map(|t| t.value.as_str()) {
                Some("nika:read" | "nika:write") => {
                    literal_path(action)?;
                }
                Some("nika:jq") => {}
                _ => {
                    return Err(
                        "unknown-cost Run supports only direct infer and local read/write/jq; no nested workflow or other tools".into(),
                    );
                }
            },
            _ => return Err("unknown-cost Run does not support exec or agent inference".into()),
        }
    }
    let report = nika_check::check(wf);
    if !report.is_clean() || report.waves.iter().flatten().count() != wf.tasks.len() {
        return Err(
            "unknown-cost Run requires a clean checked DAG; monetary choice cannot grant effects"
                .into(),
        );
    }
    if report.waves.iter().any(|wave| {
        wave.iter()
            .filter(|&&i| matches!(wf.tasks[i].value.action, RawAction::Infer(_)))
            .count()
            > 1
    }) {
        return Err(
            "unknown-cost Run requires sequential infer waves; parallel calls are unsupported"
                .into(),
        );
    }
    if requests == 0 {
        return Err("unknown-cost Run has no statically bounded direct infer".into());
    }
    Ok(requests)
}

fn literal_path(action: &RawInvokeAction) -> Result<PathBuf, String> {
    let path = action
        .args
        .as_ref()
        .and_then(|a| a.value.get("path"))
        .and_then(serde_json::Value::as_str)
        .ok_or("local file step requires a literal path")?;
    let path = Path::new(path.strip_prefix("./").unwrap_or(path));
    if path.as_os_str().is_empty()
        || path.to_string_lossy().contains("${{")
        || path
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err(
            "unknown-cost Run file paths must be literal files confined to the project".into(),
        );
    }
    Ok(path.into())
}

/// Bind pre-existing read bytes and re-observe contained output parents.
/// No input bytes or callable authority are persisted here.
/// Descriptor-rooted no-follow opens supplement (never replace) Check/permits.
pub(super) fn read_witness(root: &Path, wf: &RawWorkflow) -> Result<String, String> {
    let launch = std::env::current_dir().map_err(|e| e.to_string())?;
    if nika_runtime::project_root_fingerprint(root).ok_or("project root is unreadable")?
        != nika_runtime::project_root_fingerprint(&launch).ok_or("launch root is unreadable")?
    {
        return Err("unknown-cost Run local paths require the launch project root".into());
    }
    let directory = nika_fs::OwnedDir::open(root).map_err(|e| e.to_string())?;
    let mut files = std::collections::BTreeMap::new();
    for task in &wf.tasks {
        if let RawAction::Invoke(action) = &task.value.action
            && action.tool().is_some_and(|t| t.value == "nika:write")
        {
            let path = literal_path(action)?;
            let parts = path
                .iter()
                .map(|s| s.to_str().ok_or("non-UTF-8 path"))
                .collect::<Result<Vec<_>, _>>()?;
            let (name, parents) = parts.split_last().ok_or("empty write path")?;
            let parent = directory.open_below(parents).map_err(|e| e.to_string())?;
            match parent.open_relative(Path::new(name)) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("write target is not a contained regular file: {e}")),
            }
        }
        if let RawAction::Invoke(action) = &task.value.action
            && action.tool().is_some_and(|t| t.value == "nika:read")
        {
            let path = literal_path(action)?;
            let mut bytes = Vec::new();
            directory
                .open_relative(&path)
                .map_err(|e| e.to_string())?
                .take(1_048_577)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 1_048_576 {
                return Err("unknown-cost Run read input exceeds the 1 MiB review bound".into());
            }
            files.insert(path, nika_event::source_id::sha256_hex(&bytes));
        }
    }
    Ok(nika_event::source_id::sha256_hex(
        format!("{files:?}").as_bytes(),
    ))
}

#[cfg(test)]
mod tests;
