// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Fresh descriptor-rooted observations; static shape analysis lives in L3.
use nika_dap::cost_journal::Cleared;
use nika_schema::raw::RawWorkflow;
use std::path::Path;

/// Bind pre-existing read bytes and re-observe contained output parents through
/// the review's held project root (`Cleared::observe_file`, over the files L3
/// finds statically). No input bytes or callable authority are persisted here.
/// Descriptor-rooted no-follow opens supplement (never replace) Check/permits.
pub(super) fn read_witness(
    cleared: &Cleared,
    root: &Path,
    wf: &RawWorkflow,
    launch: &Path,
) -> Result<String, String> {
    if nika_runtime::project_root_fingerprint(root).ok_or("project root is unreadable")?
        != nika_runtime::project_root_fingerprint(launch).ok_or("launch root is unreadable")?
    {
        return Err("unknown-cost Run local paths require the launch project root".into());
    }
    let mut files = std::collections::BTreeMap::new();
    for bound in nika_service_execution::run_cost::bound_files(wf) {
        let bound = bound.map_err(|e| e.to_string())?;
        let observed = cleared.observe_file(&bound.path, bound.write);
        if let Some(sha256) = observed.map_err(|e| e.to_string())? {
            files.insert(bound.path, sha256);
        }
    }
    Ok(nika_event::source_id::sha256_hex(
        format!("{files:?}").as_bytes(),
    ))
}
