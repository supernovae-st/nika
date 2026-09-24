// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Fresh descriptor-rooted observations; static shape analysis lives in L3.
use nika_check::analyzer::static_args::ConstStrings;
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_service_execution::run_cost::project_file_path;
use std::io::Read as _;
use std::path::Path;

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
    let consts = ConstStrings::of(wf);
    let mut files = std::collections::BTreeMap::new();
    for task in &wf.tasks {
        if let RawAction::Invoke(action) = &task.value.action
            && action.tool().is_some_and(|t| t.value == "nika:write")
        {
            let path = project_file_path(&consts, action)?;
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
            let path = project_file_path(&consts, action)?;
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
