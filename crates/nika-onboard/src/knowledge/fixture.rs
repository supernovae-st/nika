// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A synthetic knowledge release the strict door admits, for tests: this crate's, and those of a
//! door that enables the `test-support` feature.
//!
//! It is the smallest release of the admission profile:
//! - one project source;
//! - a family recommending a pack that contains a pattern a checked block realizes;
//! - a diagnostic suggesting a repair principle;
//! - the notices and the licence text.
//!
//! It is written the way the producer writes a release: each row signed with its canonical digest,
//! rows sorted by id, one canonical line each, every file pinned by a closed manifest. So a test
//! breaks exactly one rule of an admitted payload. It edits the rows or the rendered files, then
//! [`Payload::seal`] pins them again, and [`Payload::identity`] is the identity a test embedder
//! trusts for the bytes it wrote itself.
//!
//! Every value is invented here. The block's check receipt is synthetic: it proves no real check
//! and is never embedded. The licence and notice texts are labelled stand-ins.

use std::collections::BTreeMap;
use std::path::Path;

use nika_event::source_id::sha256_hex;
use serde_json::{Map, Value, json};

use super::admission::{
    ADMISSION_PROFILE, EXCLUDED_KINDS, MANIFEST_PATH, RELATIONS_PATH, RELEASE_FORMAT,
    TrustedIdentity, kind_files,
};
use super::canonical::{canonical_json, row_digest};

/// The target binary every fixture row is pinned to: a label.
pub const BINARY: &str = "nika 0.122.0 (fixture)";
/// The target's specification commit.
pub const SPEC: &str = "0123456789abcdef0123456789abcdef01234567";
/// The synthetic verifier the fixture's check receipt names.
pub const VERIFIER: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
/// The fixture's policy id.
pub const POLICY_ID: &str = "policy-r";
/// The fixture's policy sha256.
pub const POLICY_SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// The fixture's knowledge version.
pub const VERSION: &str = "fixture-r1";
/// The fixture block's file.
pub const BLOCK_FILE: &str = "blocks/digest.nika";
/// The fixture block's bytes: its first line is a marker a test finds in a presented pack.
pub const BLOCK_TEXT: &str = "# FIXTURE-BLOCK-MARKER\nnika: digest\ntasks: {}\n";
/// The licence of the fixture's one source.
pub const LICENCE: &str = "AGPL-3.0-or-later";
/// The path of the text the fixture ships for [`LICENCE`].
pub const LICENCE_FILE: &str = "LICENSES/AGPL-3.0-or-later.txt";
/// An intent the fixture's family, pattern and block answer.
pub const INTENT: &str =
    "Every Monday, summarize the open tickets of ./tickets.json and send the digest";

/// A release before it is written: the rows by kind, the relations, the files beside the rows.
#[derive(Clone, Debug)]
pub struct Payload {
    /// The rows of each kind the profile ships (a row without `sha256` is signed when rendered).
    pub rows: BTreeMap<&'static str, Vec<Value>>,
    /// The relations, `{from, rel, to}`.
    pub relations: Vec<Value>,
    /// Every other file by its path under the root: the block, the notices, the licence text.
    pub files: BTreeMap<String, Vec<u8>>,
}

/// The pin every fixture row carries.
#[must_use]
pub fn pin() -> Value {
    json!({"binary": BINARY, "spec_sha": SPEC})
}

/// A row of `kind` with the fields every row carries, cited from the fixture's one source.
#[must_use]
pub fn row(kind: &str, id: &str, title: &str, status: &str, proof: &str) -> Value {
    json!({
        "id": id,
        "kind": kind,
        "title": title,
        "status": status,
        "proof_level": proof,
        "pin": pin(),
        "provenance": {"sources": ["src:fixture"]},
    })
}

/// A relation.
#[must_use]
pub fn edge(from: &str, rel: &str, to: &str) -> Value {
    json!({"from": from, "rel": rel, "to": to})
}

/// The identity a test embedder trusts for the payload `files` hold (its manifest's bytes, the
/// fixture's policy); `None` when they hold no manifest.
#[must_use]
pub fn identity_of(files: &BTreeMap<String, Vec<u8>>) -> Option<TrustedIdentity> {
    let manifest = files.get(MANIFEST_PATH)?;
    TrustedIdentity::new(&sha256_hex(manifest), POLICY_ID, POLICY_SHA256)
}

/// The fixture's one source: a project source, its licence shipped, no lineage of its own.
fn source_row() -> Value {
    let mut source = row(
        "source_artifact",
        "src:fixture",
        "Synthetic fixture source",
        "PROMOTED",
        "NONE",
    );
    if let Some(fields) = source.as_object_mut() {
        fields.remove("provenance");
    }
    source["licence"] = json!(LICENCE);
    source["ownership"] = json!("project");
    source["upstream"] = json!("");
    source
}

/// The fixture's checked block: its file's bytes bound by the pin and the synthetic receipt.
fn block_row() -> Value {
    let file_sha = sha256_hex(BLOCK_TEXT.as_bytes());
    let mut block = row(
        "block",
        "block:digest",
        "Digest block",
        "EXPERIMENTAL",
        "CHECKED",
    );
    block["purpose"] = json!("read the tickets, summarize them, notify");
    block["file"] = json!(BLOCK_FILE);
    block["file_sha256"] = json!(file_sha);
    block["holes"] = json!([
        {"name": "const.source_path", "owner": "human", "note": "the file the request names"}
    ]);
    block["effects"] = json!(["fs.read"]);
    block["authority"] = json!(["permits.fs"]);
    block["interfaces"] = json!([]);
    block["callables"] = json!(["nika:read"]);
    block["known_failure_modes"] = json!(["an empty file summarizes to nothing"]);
    block["check_receipt"] = json!({
        "verifier_sha256": VERIFIER, "spec_sha": SPEC, "sha256": file_sha,
        "verdict": "CURRENT_CHECKED",
    });
    block
}

impl Payload {
    /// The smallest release of the profile.
    #[must_use]
    pub fn minimal() -> Self {
        let mut family = row(
            "family",
            "family:scheduled-digest",
            "Scheduled digest",
            "EXPERIMENTAL",
            "NONE",
        );
        family["need"] = json!(
            "Every cadence, gather the open tickets from a source, summarize them and send a digest"
        );
        family["facets"] = json!({"cadence": ["weekly"], "delivery": ["message"]});
        let pack = row(
            "pattern_pack",
            "pack:digest",
            "Digest pack",
            "EXPERIMENTAL",
            "NONE",
        );
        let mut pattern = row(
            "pattern",
            "pattern:summarize",
            "Summarize the tickets",
            "EXPERIMENTAL",
            "NONE",
        );
        pattern["purpose"] = json!("One infer over the gathered tickets.");
        pattern["notes"] = json!("state max_tokens");
        let diagnostic = row(
            "diagnostic",
            "diagnostic:NIKA-PARSE-022",
            "tasks is a list",
            "PROMOTED",
            "NONE",
        );
        let mut repair = row(
            "repair_principle",
            "repair:TASKS_AS_LIST",
            "tasks is a map",
            "EXPERIMENTAL",
            "NONE",
        );
        repair["strategy"] = json!("rewrite the list as a map keyed by id");
        let mut rows: BTreeMap<&'static str, Vec<Value>> =
            kind_files().map(|(kind, _)| (kind, Vec::new())).collect();
        for (kind, value) in [
            ("source_artifact", source_row()),
            ("family", family),
            ("pattern_pack", pack),
            ("pattern", pattern),
            ("block", block_row()),
            ("diagnostic", diagnostic),
            ("repair_principle", repair),
        ] {
            rows.entry(kind).or_default().push(value);
        }
        let relations = vec![
            edge("family:scheduled-digest", "RECOMMENDS", "pack:digest"),
            edge("pack:digest", "CONTAINS", "pattern:summarize"),
            edge("block:digest", "REALIZES", "pattern:summarize"),
            edge(
                "diagnostic:NIKA-PARSE-022",
                "SUGGESTS_REPAIR",
                "repair:TASKS_AS_LIST",
            ),
        ];
        let notice =
            "# Notices\n\nA synthetic fixture: every row and file is invented for tests.\n";
        let licence = "A stand-in for the licence text of a synthetic fixture.\n";
        let files = BTreeMap::from([
            (BLOCK_FILE.to_owned(), BLOCK_TEXT.as_bytes().to_vec()),
            ("NOTICE.md".to_owned(), notice.as_bytes().to_vec()),
            (LICENCE_FILE.to_owned(), licence.as_bytes().to_vec()),
        ]);
        Self {
            rows,
            relations,
            files,
        }
    }

    /// The rows of `kind`, to edit before rendering.
    pub fn kind(&mut self, kind: &'static str) -> &mut Vec<Value> {
        self.rows.entry(kind).or_default()
    }

    /// The row whose id is `id`, whatever its kind, to edit before rendering.
    pub fn row(&mut self, id: &str) -> Option<&mut Value> {
        self.rows.values_mut().flatten().find(|row| row["id"] == id)
    }

    /// Every payload file but the manifest, as the producer writes them: each row signed unless
    /// it carries a `sha256` already, rows sorted by id, one canonical line each; the relations
    /// sorted; a kind without a row is an empty file.
    #[must_use]
    pub fn render(&self) -> BTreeMap<String, Vec<u8>> {
        let mut files = self.files.clone();
        for (kind, file) in kind_files() {
            let mut rows = self.rows.get(kind).cloned().unwrap_or_default();
            for row in &mut rows {
                if row.get("sha256").is_none() {
                    row["sha256"] = row_digest(row).map_or(Value::Null, Value::String);
                }
            }
            rows.sort_by_key(|row| text(&row["id"]));
            files.insert(file.to_owned(), lines(&rows));
        }
        let mut relations = self.relations.clone();
        relations.sort_by_key(|e| (text(&e["from"]), text(&e["rel"]), text(&e["to"])));
        files.insert(RELATIONS_PATH.to_owned(), lines(&relations));
        files
    }

    /// The closed manifest that pins `files` (every payload file but the manifest), each row
    /// file's count its number of lines.
    #[must_use]
    pub fn manifest(files: &BTreeMap<String, Vec<u8>>) -> Value {
        let count = |path: &str| {
            // n newlines split the bytes into n + 1 pieces, and no bytes into one.
            files
                .get(path)
                .map_or(0, |bytes| bytes.split(|b| *b == b'\n').count() - 1)
        };
        let rows: Map<String, Value> = kind_files()
            .map(|(kind, file)| (kind.to_owned(), json!({"file": file, "count": count(file)})))
            .collect();
        let pins: Map<String, Value> = files
            .iter()
            .map(|(path, bytes)| (path.clone(), json!(sha256_hex(bytes))))
            .collect();
        json!({
            "format": RELEASE_FORMAT,
            "profile": ADMISSION_PROFILE,
            "knowledge_version": VERSION,
            "target": {"binary": BINARY, "spec_sha": SPEC, "verifier_sha256": VERIFIER},
            "policy": {"id": POLICY_ID, "sha256": POLICY_SHA256},
            "tool": {"id": "fixture-producer", "sha256": "b".repeat(64)},
            "input_digest": "c".repeat(64),
            "rows": rows,
            "relations": {"file": RELATIONS_PATH, "count": count(RELATIONS_PATH)},
            "exclusions": {"kinds": EXCLUDED_KINDS, "rows": {}},
            "downgrades": {},
            "files": pins,
        })
    }

    /// `files` with the manifest that pins them: what a payload root holds.
    #[must_use]
    pub fn seal(mut files: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
        let manifest = Self::manifest(&files);
        files.insert(MANIFEST_PATH.to_owned(), manifest_bytes(&manifest));
        files
    }

    /// Every file of this release, the manifest included.
    #[must_use]
    pub fn files(&self) -> BTreeMap<String, Vec<u8>> {
        Self::seal(self.render())
    }

    /// The identity a test embedder trusts for this release as [`Self::files`] renders it.
    #[must_use]
    pub fn identity(&self) -> Option<TrustedIdentity> {
        identity_of(&self.files())
    }

    /// Write this release under `root` (created).
    ///
    /// # Errors
    /// A directory or a file that cannot be written.
    pub fn write(&self, root: &Path) -> std::io::Result<()> {
        write_files(root, &self.files())
    }
}

/// The manifest's path under a release root.
#[must_use]
pub fn manifest_path() -> &'static str {
    MANIFEST_PATH
}

/// The manifest's bytes as the fixture writes them (pretty, a final newline): the identity is
/// over these bytes, whatever their whitespace.
#[must_use]
pub fn manifest_bytes(manifest: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(manifest).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

/// Write `files` under `root`, creating directories.
///
/// # Errors
/// A directory or a file that cannot be written.
pub fn write_files(root: &Path, files: &BTreeMap<String, Vec<u8>>) -> std::io::Result<()> {
    for (path, bytes) in files {
        let at = root.join(path);
        if let Some(parent) = at.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&at, bytes)?;
    }
    Ok(())
}

/// A value's text, for ordering (a value that is no text orders first).
fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

/// One line per value: its canonical text where it has one, else its plain JSON (which the door
/// refuses as not canonical — a test that wants that writes it so).
fn lines(values: &[Value]) -> Vec<u8> {
    values
        .iter()
        .map(|value| canonical_json(value).unwrap_or_else(|| value.to_string()) + "\n")
        .collect::<String>()
        .into_bytes()
}
