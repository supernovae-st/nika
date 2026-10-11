// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The historical snapshot layout (`manifest.json` · one JSONL per kind beside it · the files its
//! rows name under an ancestor `foundry/`), read WITHOUT admission for the retrieval baseline
//! tests only: they prove the selection, ranking and rendering over the fixtures they were written
//! against, which no release contract admits. No product door reads this layout; the strict door
//! ([`Snapshot::open`]) refuses it, and the identity this loader states says it was not admitted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nika_event::source_id::sha256_hex;
use serde_json::Value;

use super::Snapshot;

/// What a legacy fixture's identity says of its verification.
pub(super) const NOT_ADMITTED: &str = "legacy test fixture: not admitted";

impl Snapshot {
    /// A historical fixture snapshot at `dir`, loaded as the retired door loaded it (tests only).
    pub(crate) fn legacy_fixture(dir: &Path) -> Self {
        let text = std::fs::read_to_string(dir.join("manifest.json")).expect("a fixture manifest");
        let manifest: Value = serde_json::from_str(&text).expect("a fixture manifest in JSON");
        let pins = manifest
            .get("files")
            .and_then(Value::as_object)
            .map(|files| {
                files
                    .iter()
                    .filter_map(|(path, sha)| Some((path.clone(), sha.as_str()?.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("a fixture directory")
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .collect();
        names.sort();
        let (mut rows, mut relations, mut row_files) =
            (BTreeMap::new(), Vec::new(), BTreeMap::new());
        for name in names {
            let Some(kind) = name.strip_suffix(".jsonl") else {
                continue;
            };
            let bytes = std::fs::read(dir.join(&name)).expect("a fixture row file");
            let parsed: Vec<Value> = String::from_utf8_lossy(&bytes)
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| serde_json::from_str(line).expect("a fixture row"))
                .collect();
            if kind == "relations" {
                relations = parsed;
            } else {
                rows.insert(kind.to_owned(), parsed);
            }
            row_files.insert(name, sha256_hex(&bytes));
        }
        let files = files_root(dir)
            .map(|root| read_tree(&root, &root))
            .unwrap_or_default();
        Self {
            dir: dir.to_path_buf(),
            manifest,
            rows,
            relations,
            pins,
            files,
            row_files,
            manifest_sha256: sha256_hex(text.as_bytes()),
            admission: NOT_ADMITTED,
        }
    }
}

/// The retired door's files root: the first ancestor of the snapshot directory holding a
/// `foundry/` directory with `blocks/` or `examples/` inside it.
fn files_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().skip(1).take(6).find_map(|ancestor| {
        let candidate = ancestor.join("foundry");
        (candidate.join("blocks").is_dir() || candidate.join("examples").is_dir())
            .then_some(candidate)
    })
}

/// Every file under `dir` by its `/`-separated path under `root`.
fn read_tree(root: &Path, dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(dir)
        .expect("a fixture directory")
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_dir() {
            files.extend(read_tree(root, &path));
        } else if let Ok(relative) = path.strip_prefix(root) {
            let key: Vec<String> = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            files.insert(key.join("/"), std::fs::read(&path).expect("a fixture file"));
        }
    }
    files
}
