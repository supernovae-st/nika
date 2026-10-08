// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The current release: the r2 payload of the run contract (profile r2, `policy-r2`), embedded
//! whole from its directory. Its inventory is not listed here: admission checks every file against
//! the manifest the trusted snapshot names, so a file added to or removed from the directory is
//! refused, never served (the build script makes cargo rescan the directory).

use std::collections::BTreeMap;

use include_dir::{Dir, DirEntry, include_dir};

static PAYLOAD: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/assets/knowledge-release-r2");

pub(super) const SNAPSHOT_SHA256: &str =
    "6476372aa7eedf02e3b718dcd1c51769d97450eb0ae825a62b33fcf10a2471af";
pub(super) const POLICY_ID: &str = "policy-r2";
pub(super) const POLICY_SHA256: &str =
    "53ef65a30e54220dfe76472f9fd766af2ef4817d4daea6bab38dab1337381cdf";

/// Every embedded file, by its path relative to the payload root (`/`-separated).
pub(super) fn files() -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut dirs = vec![&PAYLOAD];
    while let Some(dir) = dirs.pop() {
        for entry in dir.entries() {
            match entry {
                DirEntry::Dir(inner) => dirs.push(inner),
                DirEntry::File(file) => {
                    let path: Vec<_> = (file.path().components())
                        .map(|part| part.as_os_str().to_string_lossy())
                        .collect();
                    files.insert(path.join("/"), file.contents().to_vec());
                }
            }
        }
    }
    files
}
