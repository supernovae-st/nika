// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The current release: the r3 payload of the run contract (profile r2, `policy-r2`), embedded
//! whole from its directory. Its inventory is not listed here: admission checks every file against
//! the manifest the trusted snapshot names, so a file added to or removed from the directory is
//! refused, never served (the build script makes cargo rescan the directory).

use std::collections::BTreeMap;

use include_dir::{Dir, DirEntry, include_dir};

static PAYLOAD: Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/assets/knowledge-release-0.123.0-r3.2-foundry-20261012");

pub(super) const SNAPSHOT_SHA256: &str =
    "8d52df01a5105fc4661851218e89ca172c5799bf3016aedbce1574ecac096075";
pub(super) const POLICY_ID: &str = "policy-r2";
pub(super) const POLICY_SHA256: &str =
    "fb070b7ac25066c86c3fe41d4253d25741565b8c494908658661b868d1ae5764";

/// Every embedded file, by its path relative to the payload root (`/`-separated).
pub(super) fn files() -> BTreeMap<String, Vec<u8>> {
    walk(&PAYLOAD)
}

/// Every file of an embedded directory, by its path relative to it (`/`-separated).
pub(super) fn walk(payload: &'static Dir<'static>) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut dirs = vec![payload];
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
