// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! F, G, H · the rows, the graph and the closure (the r1 contract §6, §7, §8).
//!
//! **F.** Each row file in the profile's kind order:
//! - F1, the line layer, over the whole file: UTF-8, the final LF, and each line within its
//!   bounds, strict, an object, holding no number, canonical;
//! - F2, the count;
//! - F3 to F11, each row in turn: its kind and id, uniqueness, order, digest, closed schema, pin,
//!   vocabulary, evidence, file.
//!
//! **G.** Lineage, then the relations file: its line layer, its count, and each relation.
//!
//! **H.** The block files, the licence texts and the notices.

use std::collections::{BTreeMap, BTreeSet};

use nika_event::source_id::sha256_hex;
use serde_json::Value;

use super::super::canonical::{
    StrictJsonError, canonical_json, holds_number, row_digest, strict_line,
};
use super::collect::Files;
use super::profile::{
    KINDS, KindProfile, RELATIONS, STATUSES, Target, Ty, base_name, blank, block_path, closed_row,
    free_text, id_name, licence_path, stem,
};
use super::{
    Admitted, Checked, MAX_LINE_BYTES, MAX_LINE_VALUES, NOTICE_PATH, RELATIONS_PATH, RefusalCode,
    Step, refuse,
};

/// F, G and H on the admitted manifest and the inventory it pinned.
pub(super) fn admit_rows(
    manifest: Value,
    manifest_sha256: String,
    pins: BTreeMap<String, String>,
    files: Files,
) -> Checked<Admitted> {
    let target = Target::of(&manifest);
    let mut rows = BTreeMap::new();
    let mut row_files = BTreeMap::new();
    let mut ids: BTreeMap<String, &'static KindProfile> = BTreeMap::new();
    for kind in KINDS {
        let bytes = files.get(kind.file).map_or(&[][..], Vec::as_slice);
        let parsed = lines("F1", kind.file, bytes)?;
        counted(
            "F2",
            &manifest["rows"][kind.kind]["count"],
            kind.file,
            parsed.len(),
        )?;
        let mut previous: Option<&str> = None;
        for row in &parsed {
            let id = row_identity(kind, row)?;
            if ids.insert(id.to_owned(), kind).is_some() {
                return refuse("F4", RefusalCode::DuplicateId, id);
            }
            if previous.is_some_and(|before| before >= id) {
                return refuse(
                    "F5",
                    RefusalCode::RowOrder,
                    format!("{}: {id} does not come after the row before it", kind.file),
                );
            }
            previous = Some(id);
            row_rules(kind, id, row, &target, &pins, &files)?;
        }
        row_files.insert(base_name(kind.file), sha256_hex(bytes));
        rows.insert(stem(kind.file), parsed);
    }
    lineage(&rows, &ids)?;
    let relation_bytes = files.get(RELATIONS_PATH).map_or(&[][..], Vec::as_slice);
    let relations = lines("G2", RELATIONS_PATH, relation_bytes)?;
    counted(
        "G2",
        &manifest["relations"]["count"],
        RELATIONS_PATH,
        relations.len(),
    )?;
    relations_rules(&relations, &ids)?;
    row_files.insert(base_name(RELATIONS_PATH), sha256_hex(relation_bytes));
    closure(&rows, &pins, &files)?;
    Ok(Admitted {
        manifest,
        manifest_sha256,
        pins,
        files,
        rows,
        relations,
        row_files,
    })
}

// ── F · the line layer, then each row's own rules ──────────────────────────────────────────────

/// The line layer over a whole JSONL file, at `step` (F1 for a row file, G2 for the relations):
/// UTF-8, zero bytes for no line, else a final LF; each line (split on LF alone, so a CR stays and
/// fails) within its byte bound, then strict (§9.1: the grammar, nothing after the value, the depth
/// and the values, before a key stated twice), an object, holding no number, and the canonical text
/// of itself.
fn lines(step: Step, path: &str, bytes: &[u8]) -> Checked<Vec<Value>> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return refuse(
            step,
            RefusalCode::RowMalformed,
            format!("{path}: not UTF-8"),
        );
    };
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let Some(body) = text.strip_suffix('\n') else {
        return refuse(
            step,
            RefusalCode::RowMalformed,
            format!("{path}: the last line has no newline"),
        );
    };
    let mut values = Vec::new();
    for (at, line) in body.split('\n').enumerate() {
        let n = at + 1;
        if line.len() > MAX_LINE_BYTES {
            return refuse(
                step,
                RefusalCode::RowMalformed,
                format!("{path}:{n}: more than {MAX_LINE_BYTES} bytes"),
            );
        }
        let value = match strict_line(line, MAX_LINE_VALUES) {
            Ok(value) => value,
            Err(StrictJsonError::DuplicateKey(key)) => {
                return refuse(
                    step,
                    RefusalCode::DuplicateKey,
                    format!("{path}:{n}: `{key}` twice"),
                );
            }
            Err(error) => {
                return refuse(
                    step,
                    RefusalCode::RowMalformed,
                    format!("{path}:{n}: {error}"),
                );
            }
        };
        if !value.is_object() || holds_number(&value) {
            return refuse(
                step,
                RefusalCode::RowMalformed,
                format!("{path}:{n}: not an object without numbers"),
            );
        }
        if canonical_json(&value).as_deref() != Some(line) {
            return refuse(
                step,
                RefusalCode::RowMalformed,
                format!("{path}:{n}: not the canonical line"),
            );
        }
        values.push(value);
    }
    Ok(values)
}

/// A file's line count against the manifest's, at `step` (F2 for a row file, G2 for the
/// relations).
fn counted(step: Step, declared: &Value, path: &str, found: usize) -> Checked<()> {
    if declared.as_u64() != u64::try_from(found).ok() {
        return refuse(
            step,
            RefusalCode::CountMismatch,
            format!("{path}: {found} lines, the manifest counts {declared}"),
        );
    }
    Ok(())
}

/// F3: the file's kind, and an id `<prefix>:<name>` whose name follows the id grammar.
fn row_identity<'a>(kind: &KindProfile, row: &'a Value) -> Checked<&'a str> {
    let id = row["id"].as_str().unwrap_or_default();
    let named = id
        .strip_prefix(kind.prefix)
        .and_then(|rest| rest.strip_prefix(':'))
        .is_some_and(id_name);
    if row["kind"].as_str() != Some(kind.kind) || !named {
        return refuse(
            "F3",
            RefusalCode::RowKind,
            format!("{}: a row `{id}` of kind {}", kind.file, row["kind"]),
        );
    }
    Ok(id)
}

/// F6 to F11, in order: the digest, the closed and typed schema, the pin, the vocabulary, the
/// evidence a claim needs, the block's file.
fn row_rules(
    kind: &KindProfile,
    id: &str,
    row: &Value,
    target: &Target,
    pins: &BTreeMap<String, String>,
    files: &Files,
) -> Checked<()> {
    if row["sha256"].as_str() != row_digest(row).as_deref() {
        return refuse("F6", RefusalCode::RowDigest, id.to_owned());
    }
    let mut fields: Vec<(&str, Ty)> = vec![
        ("id", Ty::Line),
        ("kind", Ty::Line),
        ("sha256", Ty::Sha256),
        ("title", Ty::Line),
        ("status", Ty::Line),
        ("proof_level", Ty::Line),
        ("pin", Ty::Pin),
    ];
    if kind.kind != "source_artifact" {
        fields.push(("provenance", Ty::Provenance));
    }
    fields.extend_from_slice(kind.fields);
    closed_row(row, id, &fields)?;
    if row["pin"]["binary"].as_str() != Some(target.binary.as_str())
        || row["pin"]["spec_sha"].as_str() != Some(target.spec_sha.as_str())
    {
        return refuse("F8", RefusalCode::TargetPin, id.to_owned());
    }
    let status = row["status"].as_str().unwrap_or_default();
    let proof = row["proof_level"].as_str().unwrap_or_default();
    if !STATUSES.contains(&status) || !kind.proofs.contains(&proof) {
        return refuse(
            "F9",
            RefusalCode::Vocabulary,
            format!(
                "{id}: status {status:?} · proof {proof:?} for a {}",
                kind.kind
            ),
        );
    }
    evidence(kind, id, row, target)?;
    block_file(kind, id, row, pins, files)
}

/// F10: a block's check receipt bound to the target's verifier and specification and to its
/// file's bytes, with the positive verdict; a source the project does not own discloses its
/// upstream.
fn evidence(kind: &KindProfile, id: &str, row: &Value, target: &Target) -> Checked<()> {
    match kind.kind {
        "block" => {
            let receipt = &row["check_receipt"];
            let bound = receipt["verifier_sha256"].as_str()
                == Some(target.verifier_sha256.as_str())
                && receipt["spec_sha"].as_str() == Some(target.spec_sha.as_str())
                && receipt["sha256"] == row["file_sha256"]
                && receipt["verdict"] == "CURRENT_CHECKED";
            if !bound {
                return refuse(
                    "F10",
                    RefusalCode::Evidence,
                    format!(
                        "{id}: no CURRENT_CHECKED receipt bound to the target's verifier and specification and to the file's bytes"
                    ),
                );
            }
            Ok(())
        }
        "source_artifact" => {
            let owned = row["ownership"] == "project";
            if !owned && row["upstream"].as_str().is_none_or(blank) {
                return refuse(
                    "F10",
                    RefusalCode::Licence,
                    format!("{id}: a source the project does not own discloses its upstream"),
                );
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// F11: a block's file pinned, `file_sha256` its pin, its bytes a non-blank text of the free
/// text grammar, at most 65536 bytes.
fn block_file(
    kind: &KindProfile,
    id: &str,
    row: &Value,
    pins: &BTreeMap<String, String>,
    files: &Files,
) -> Checked<()> {
    if kind.kind != "block" {
        return Ok(());
    }
    let file = row["file"].as_str().unwrap_or_default();
    let Some(pinned) = pins.get(file) else {
        return refuse(
            "F11",
            RefusalCode::RowFile,
            format!("{id}: {file} is not in the inventory"),
        );
    };
    if row["file_sha256"].as_str() != Some(pinned.as_str()) {
        return refuse(
            "F11",
            RefusalCode::RowFile,
            format!("{id}: file_sha256 is not {file}'s pin"),
        );
    }
    if !files.get(file).is_some_and(|bytes| free_text(bytes)) {
        return refuse(
            "F11",
            RefusalCode::RowFile,
            format!("{id}: {file} is not a text of the profile's grammar and bound"),
        );
    }
    Ok(())
}

// ── G · lineage and relations ──────────────────────────────────────────────────────────────────

/// G1: every row but a source cites retained source rows, each once; every source is cited.
fn lineage(
    rows: &BTreeMap<String, Vec<Value>>,
    ids: &BTreeMap<String, &'static KindProfile>,
) -> Checked<()> {
    let mut cited = BTreeSet::new();
    for row in rows.values().flatten() {
        if row["kind"] == "source_artifact" {
            continue;
        }
        let id = row["id"].as_str().unwrap_or_default();
        let sources = row["provenance"]["sources"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let mut distinct = BTreeSet::new();
        for source in sources {
            let name = source.as_str().unwrap_or_default();
            let is_source = ids
                .get(name)
                .is_some_and(|kind| kind.kind == "source_artifact");
            if !is_source || !distinct.insert(name) {
                return refuse(
                    "G1",
                    RefusalCode::Lineage,
                    format!("{id}: its lineage names {source}, not a distinct retained source"),
                );
            }
            cited.insert(name);
        }
    }
    let uncited = ids
        .iter()
        .find(|(id, kind)| kind.kind == "source_artifact" && !cited.contains(id.as_str()));
    if let Some((id, _)) = uncited {
        return refuse(
            "G1",
            RefusalCode::Lineage,
            format!("{id}: a source no retained row cites"),
        );
    }
    Ok(())
}

/// G2: each relation exactly `{from, rel, to}` of strings, a relation the profile names, both
/// endpoints admitted rows of its domain and range, strictly after the one before.
fn relations_rules(
    relations: &[Value],
    ids: &BTreeMap<String, &'static KindProfile>,
) -> Checked<()> {
    let mut previous: Option<(&str, &str, &str)> = None;
    for (at, edge) in relations.iter().enumerate() {
        let n = format!("{RELATIONS_PATH}:{}", at + 1);
        let shaped = edge.as_object().is_some_and(|fields| {
            fields.len() == 3
                && ["from", "rel", "to"]
                    .iter()
                    .all(|key| fields.get(*key).is_some_and(Value::is_string))
        });
        if !shaped {
            return refuse(
                "G2",
                RefusalCode::Relation,
                format!("{n}: not exactly {{from, rel, to}}"),
            );
        }
        let (from, rel, to) = (
            edge["from"].as_str().unwrap_or_default(),
            edge["rel"].as_str().unwrap_or_default(),
            edge["to"].as_str().unwrap_or_default(),
        );
        let Some((_, domain, range)) = RELATIONS.iter().find(|(name, _, _)| *name == rel) else {
            return refuse(
                "G2",
                RefusalCode::Relation,
                format!("{n}: {rel} is no relation of this profile"),
            );
        };
        for (end, allowed) in [(from, domain), (to, range)] {
            if ids.get(end).is_none_or(|kind| kind.kind != *allowed) {
                return refuse(
                    "G2",
                    RefusalCode::Relation,
                    format!("{n}: {rel} does not reach a {allowed} at {end:?}"),
                );
            }
        }
        let key = (from, rel, to);
        if previous.is_some_and(|before| before >= key) {
            return refuse(
                "G2",
                RefusalCode::Relation,
                format!("{n}: out of order, or stated twice"),
            );
        }
        previous = Some(key);
    }
    Ok(())
}

// ── H · the closure of block files, licence texts and notices ──────────────────────────────────

/// H1 to H5: every block file named by exactly one block row; every source's licence text
/// pinned and every licence text named by a source; the notices a non-blank text of their
/// grammar and bound.
fn closure(
    rows: &BTreeMap<String, Vec<Value>>,
    pins: &BTreeMap<String, String>,
    files: &Files,
) -> Checked<()> {
    let mut named = BTreeSet::new();
    for block in rows.get("blocks").map_or(&[][..], Vec::as_slice) {
        let file = block["file"].as_str().unwrap_or_default();
        if !named.insert(file) {
            return refuse(
                "H1",
                RefusalCode::RowFile,
                format!("{file} is named by two blocks"),
            );
        }
    }
    if let Some(path) = pins
        .keys()
        .find(|path| block_path(path) && !named.contains(path.as_str()))
    {
        return refuse(
            "H2",
            RefusalCode::UnexpectedFile,
            format!("{path}: no block names it"),
        );
    }
    let mut licences = BTreeSet::new();
    for source in rows.get("source_artifacts").map_or(&[][..], Vec::as_slice) {
        let licence = source["licence"].as_str().unwrap_or_default();
        if !pins.contains_key(&format!("LICENSES/{licence}.txt")) {
            return refuse(
                "H3",
                RefusalCode::Licence,
                format!("{}: no LICENSES/{licence}.txt", source["id"]),
            );
        }
        licences.insert(licence);
    }
    if let Some(path) = pins
        .keys()
        .find(|path| licence_path(path).is_some_and(|licence| !licences.contains(licence)))
    {
        return refuse(
            "H4",
            RefusalCode::Licence,
            format!("{path}: no retained source names this licence"),
        );
    }
    if !files.get(NOTICE_PATH).is_some_and(|bytes| free_text(bytes)) {
        return refuse(
            "H5",
            RefusalCode::NoticeInvalid,
            format!("{NOTICE_PATH} is not a non-blank text of the profile's grammar and bound"),
        );
    }
    Ok(())
}
