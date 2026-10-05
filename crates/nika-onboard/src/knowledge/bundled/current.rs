// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The source-reviewed a8 payload: only its changed files overlay the retained R3 table.
//! Unchanged bytes remain shared; admission checks the complete resulting inventory and pins.

use std::collections::BTreeMap;

pub(super) const SNAPSHOT_SHA256: &str =
    "b7f3861c55c785ba78fbf3fcfbb495ab79154b30f1bcb8483ce66018cc4659a9";
pub(super) const POLICY_SHA256: &str =
    "41ba74af8c28cffa2abbbbbf5444e510ae9ddd045c4f197f6fdf217cd978987b";

pub(super) fn files() -> BTreeMap<String, Vec<u8>> {
    let mut files: BTreeMap<_, _> = super::FILES
        .into_iter()
        .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
        .collect();
    let added = embedded_files!(
        "../../../assets/knowledge-release-a8/";
        "NOTICE.md",
        "blocks/glob-read-many.nika",
        "blocks/lookup-enrich-by-key.nika",
        "blocks/multi-csv-group-totals.nika",
        "blocks/validate-diff-convert.nika",
        "blocks/validate-quarantine-total.nika",
        "knowledge/blocks.jsonl",
        "knowledge/diagnostics.jsonl",
        "knowledge/manifest.json",
        "knowledge/patterns.jsonl",
        "knowledge/relations.jsonl",
        "knowledge/source_artifacts.jsonl",
    );
    files.extend(
        added
            .into_iter()
            .map(|(path, bytes)| (path.to_owned(), bytes.to_vec())),
    );
    files
}
