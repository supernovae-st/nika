// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The admission rules of knowledge release profile r2 ([`PROFILE`], format [`RELEASE_FORMAT`])
//! after collection and identity: D (the manifest), E (the inventory), F (the rows), G (lineage
//! and relations) and H (closure). Profile r1 keeps its own door; neither admits the other's
//! payload.
//!
//! r2 ships every kind of the Foundry ontology in its role: a block is the one executable
//! component, an example a solved case, a counterexample a boundary (never a component), a skill
//! a method, the other kinds knowledge with their contracts. Numbers are RFC 8785 numbers and
//! stand only where a schema types a count, a number or json.
//!
//! A refusal is exactly one code, the first rule broken in this order, with the step that broke
//! it (`F7`). The knowledge door collects the payload (A and B), judges the manifest's identity
//! (C) and then hands the exact bytes here; it keeps every byte this door admitted.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::canonical::{
    MAX_SAFE_INTEGER, StrictJsonError, jcs_json, jcs_row_digest, strict_json, strict_line,
};
use super::grammar::{blank, file_text, hex64, line, policy_token, safe_relative, spec_sha, token};

/// The profile r2 tables the door interprets.
pub mod profile;
/// F7 · the closed schemas.
mod schema;

pub use profile::{Kind, Profile, profile};

/// The admission profile this door enforces.
pub const PROFILE: &str = "nika-knowledge-release-profile/r2";
/// The manifest format this door admits.
pub const RELEASE_FORMAT: &str = "nika-knowledge-release/3";
/// The manifest: the only payload file outside its own inventory.
pub const MANIFEST_PATH: &str = "knowledge/manifest.json";
/// The relations between admitted rows.
pub const RELATIONS_PATH: &str = "knowledge/relations.jsonl";
/// The notices of every retained source.
pub const NOTICE_PATH: &str = "NOTICE.md";
/// The directories the collection layout admits, directly under the root.
pub const DIRECTORIES: [&str; 6] = [
    "LICENSES",
    "blocks",
    "counterexamples",
    "examples",
    "knowledge",
    "skills",
];
/// The most files a payload holds, the manifest included.
pub const MAX_FILES: usize = 10_000;
/// The most bytes a payload holds, the manifest included.
pub const MAX_BYTES: u64 = 67_108_864;
/// The most bytes the manifest holds.
pub const MAX_MANIFEST_BYTES: u64 = 4_194_304;
/// The most entries one directory listing holds.
pub const MAX_ENTRIES: usize = 10_001;
const MAX_LINE_BYTES: usize = 2_097_152;
const MAX_LINE_VALUES: usize = 65_536;
const MAX_MANIFEST_VALUES: usize = 65_536;
const MAX_BODY_BYTES: usize = 262_144;
const MAX_NOTICE_BYTES: usize = 65_536;
/// The most bytes of a text value.
const MAX_TEXT_BYTES: usize = 65_536;
/// The largest count (2^53 − 1).
const MAX_COUNT: u64 = MAX_SAFE_INTEGER;

/// The resource bounds as the export names them: they protect the reader, never limit an
/// intention or the language.
const BOUNDS: [(&str, u64); 10] = [
    ("body_file_bytes", MAX_BODY_BYTES as u64),
    ("bytes", MAX_BYTES),
    ("depth", super::canonical::MAX_DEPTH as u64),
    ("files", MAX_FILES as u64),
    ("line_bytes", MAX_LINE_BYTES as u64),
    ("line_values", MAX_LINE_VALUES as u64),
    ("listing_entries", MAX_ENTRIES as u64),
    ("manifest_bytes", MAX_MANIFEST_BYTES),
    ("manifest_values", MAX_MANIFEST_VALUES as u64),
    ("notice_bytes", MAX_NOTICE_BYTES as u64),
];

/// Every refusal code of profile r2, in the contract's admission order.
pub const CODES: [&str; 38] = [
    "ADMISSION_UNTRUSTED",
    "PATH_NOT_ABSOLUTE",
    "ROOT_INVALID",
    "PAYLOAD_SYMLINK",
    "PAYLOAD_NOT_REGULAR",
    "PAYLOAD_IO",
    "PAYLOAD_TOO_LARGE",
    "MANIFEST_MISSING",
    "IDENTITY_MISMATCH",
    "MANIFEST_NOT_STRICT",
    "MANIFEST_SHAPE",
    "MANIFEST_UNSUPPORTED_FORMAT",
    "PROFILE_MISMATCH",
    "POLICY_MISMATCH",
    "PROFILE_KIND_COVERAGE",
    "PATH_UNSAFE",
    "INVENTORY_MISSING",
    "INVENTORY_EXTRA",
    "PIN_MISMATCH",
    "PROFILE_UNEXPECTED_FILE",
    "ROW_MALFORMED",
    "ROW_DUPLICATE_KEY",
    "ROW_COUNT",
    "ROW_KIND",
    "ROW_DUPLICATE_ID",
    "ROW_ORDER",
    "ROW_DIGEST",
    "ROW_UNKNOWN_FIELD",
    "ROW_MISSING_FIELD",
    "ROW_FIELD_TYPE",
    "ROW_VOCABULARY",
    "ROW_EVIDENCE",
    "ROW_SPLIT",
    "SOURCE_LICENCE",
    "ROW_FILE",
    "ROW_LINEAGE",
    "RELATION_INVALID",
    "NOTICE_INVALID",
];

/// The manifest's twelve keys.
const MANIFEST_KEYS: [&str; 12] = [
    "format",
    "profile",
    "knowledge_version",
    "target",
    "policy",
    "tool",
    "input_digest",
    "rows",
    "relations",
    "exclusions",
    "downgrades",
    "files",
];

/// One refusal: the rule broken (`F7`), its code (`ROW_FIELD_TYPE`) and what broke it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Refusal {
    /// The rule of the admission order, as the contract names it.
    pub step: &'static str,
    /// The refusal code, one of the profile's codes.
    pub code: &'static str,
    /// What broke the rule, for a person.
    pub detail: String,
}

impl Refusal {
    /// A refusal at `step` with `code`.
    #[must_use]
    pub fn new(step: &'static str, code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            step,
            code,
            detail: detail.into(),
        }
    }
}

type Checked<T> = Result<T, Refusal>;

fn refuse<T>(step: &'static str, code: &'static str, detail: impl Into<String>) -> Checked<T> {
    Err(Refusal::new(step, code, detail))
}

/// What the door admitted: the manifest, its pins, every other file's bytes, each kind's rows (by
/// kind name, in file order) and the relations.
#[derive(Debug)]
#[non_exhaustive]
pub struct Admitted {
    /// The manifest as parsed.
    pub manifest: Value,
    /// The manifest's pins: every payload file but the manifest, to its sha256.
    pub pins: BTreeMap<String, String>,
    /// Every payload file but the manifest, its exact bytes.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Each kind's rows, by kind name.
    pub rows: BTreeMap<String, Vec<Value>>,
    /// The relations, in order.
    pub relations: Vec<Value>,
}

/// The trusted policy a manifest must name: `{id, sha256}`.
#[derive(Clone, Copy, Debug)]
pub struct Policy<'a> {
    /// The policy id.
    pub id: &'a str,
    /// The policy's sha256.
    pub sha256: &'a str,
}

/// D through H on a payload whose manifest bytes are the trusted snapshot: `manifest` holds those
/// bytes, `files` every other collected file, `policy` the trusted policy, and `sha256` the
/// digest of bytes the pins name.
///
/// # Errors
/// The first rule the payload breaks, as one [`Refusal`].
pub fn admit(
    manifest: &[u8],
    files: BTreeMap<String, Vec<u8>>,
    policy: Policy<'_>,
    sha256: fn(&[u8]) -> String,
) -> Result<Admitted, Refusal> {
    let profile = profile().map_err(|why| {
        Refusal::new(
            "A1",
            "ADMISSION_UNTRUSTED",
            format!("the r2 profile: {why}"),
        )
    })?;
    let manifest = read_manifest(manifest, policy, profile)?;
    let pins: BTreeMap<String, String> = manifest["files"]
        .as_object()
        .map(|pins| {
            pins.iter()
                .map(|(path, sha)| (path.clone(), sha.as_str().unwrap_or_default().to_owned()))
                .collect()
        })
        .unwrap_or_default();
    inventory(&files, &pins, profile, sha256)?;
    let rows = rows(&manifest, &pins, &files, profile)?;
    lineage(&rows, profile)?;
    let relations = relations(&manifest, &files, &rows, profile)?;
    closure(&rows, &pins, &files, profile)?;
    Ok(Admitted {
        manifest,
        pins,
        files,
        rows,
        relations,
    })
}

// ── D · the manifest ─────────────────────────────────────────────────────────────────────────────

/// D1 to D11: strict JSON, an object, the format, the trusted profile and policy, then the closed
/// shape of every key.
fn read_manifest(bytes: &[u8], policy: Policy<'_>, profile: &Profile) -> Checked<Value> {
    let not_strict =
        |why: &str| Refusal::new("D1", "MANIFEST_NOT_STRICT", format!("the manifest {why}"));
    let text = std::str::from_utf8(bytes).map_err(|_| not_strict("is not UTF-8"))?;
    if text.starts_with('\u{feff}') {
        return Err(not_strict("starts with a byte-order mark"));
    }
    let manifest = strict_json(text, MAX_MANIFEST_VALUES)
        .map_err(|error| not_strict(&format!(": {error}")))?;
    let Some(object) = manifest.as_object() else {
        return refuse("D2", "MANIFEST_SHAPE", "the manifest is not an object");
    };
    if object.get("format").and_then(Value::as_str) != Some(RELEASE_FORMAT) {
        return refuse(
            "D3",
            "MANIFEST_UNSUPPORTED_FORMAT",
            format!("the manifest format is not {RELEASE_FORMAT}"),
        );
    }
    if object.get("profile").and_then(Value::as_str) != Some(PROFILE) {
        return refuse(
            "D4",
            "PROFILE_MISMATCH",
            "the manifest names another profile than the trusted one",
        );
    }
    if object.len() != MANIFEST_KEYS.len()
        || !MANIFEST_KEYS.iter().all(|key| object.contains_key(*key))
    {
        return refuse(
            "D5",
            "MANIFEST_SHAPE",
            "the manifest does not hold exactly its twelve keys",
        );
    }
    let named = &object["policy"];
    let shaped = closed(named, &["id", "sha256"])
        && named["id"].as_str().is_some_and(policy_token)
        && hex64(&named["sha256"]);
    if !shaped {
        return refuse("D6", "MANIFEST_SHAPE", "policy is not exactly {id, sha256}");
    }
    if named["id"] != policy.id || named["sha256"] != policy.sha256 {
        return refuse(
            "D7",
            "POLICY_MISMATCH",
            "the manifest names another policy than the trusted one",
        );
    }
    identity_fields(object)?;
    accounting(object, profile)?;
    row_files(object, profile)?;
    pins_shape(object)?;
    Ok(manifest)
}

/// An object holding exactly `keys`.
fn closed(value: &Value, keys: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
    })
}

/// A count: an integer literal of at most 2^53 − 1.
fn count(value: &Value) -> Option<u64> {
    value.as_u64().filter(|n| *n <= MAX_COUNT)
}

/// D8: the knowledge version, the target, the tool and the input digest.
fn identity_fields(manifest: &Map<String, Value>) -> Checked<()> {
    let version = manifest["knowledge_version"]
        .as_str()
        .is_some_and(|version| {
            token(version, 64, |c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || ".+-".contains(c)
            })
        });
    if !version {
        return refuse(
            "D8",
            "MANIFEST_SHAPE",
            "knowledge_version is not a short lowercase token",
        );
    }
    let target = &manifest["target"];
    let target = closed(target, &["binary", "spec_sha", "verifier_sha256"])
        && target["binary"].as_str().is_some_and(line)
        && spec_sha(&target["spec_sha"])
        && hex64(&target["verifier_sha256"]);
    if !target {
        return refuse(
            "D8",
            "MANIFEST_SHAPE",
            "target is not exactly {binary: line, spec_sha: spec, verifier_sha256: sha256}",
        );
    }
    let tool = &manifest["tool"];
    if !(closed(tool, &["id", "sha256"])
        && tool["id"].as_str().is_some_and(line)
        && hex64(&tool["sha256"]))
    {
        return refuse(
            "D8",
            "MANIFEST_SHAPE",
            "tool is not exactly {id: line, sha256: sha256}",
        );
    }
    if !hex64(&manifest["input_digest"]) {
        return refuse(
            "D8",
            "MANIFEST_SHAPE",
            "input_digest is not a lowercase sha256",
        );
    }
    Ok(())
}

/// Reasons counted at least once each, every reason one of `reasons`.
fn positive_counts(value: &Value, reasons: &[String]) -> bool {
    value.as_object().is_some_and(|counted| {
        counted
            .iter()
            .all(|(reason, n)| reasons.contains(reason) && count(n).is_some_and(|n| n > 0))
    })
}

/// D9: the exclusions (no kind excluded as a whole) and the downgrades, counted by reason.
fn accounting(manifest: &Map<String, Value>, profile: &Profile) -> Checked<()> {
    let exclusions = &manifest["exclusions"];
    let excluded = closed(exclusions, &["kinds", "rows"])
        && exclusions["kinds"].as_array().is_some_and(Vec::is_empty)
        && positive_counts(&exclusions["rows"], &profile.exclusion_reasons);
    if !excluded {
        return refuse(
            "D9",
            "MANIFEST_SHAPE",
            "exclusions is not exactly {kinds: [], rows: {reason: count >= 1}}",
        );
    }
    if !positive_counts(&manifest["downgrades"], &profile.downgrade_reasons) {
        return refuse(
            "D9",
            "MANIFEST_SHAPE",
            "downgrades is not {reason: count >= 1}",
        );
    }
    Ok(())
}

/// D10: exactly the fifteen kinds, each `{file, count}` naming its own file, then the relations.
/// Each rule is judged over every kind before the next.
fn row_files(manifest: &Map<String, Value>, profile: &Profile) -> Checked<()> {
    let Some(rows) = manifest["rows"].as_object() else {
        return refuse("D10", "MANIFEST_SHAPE", "rows is not an object");
    };
    let kinds = profile.kinds();
    if rows.len() != kinds.len() || !kinds.iter().all(|kind| rows.contains_key(kind.name())) {
        return refuse(
            "D10",
            "PROFILE_KIND_COVERAGE",
            "rows does not hold exactly the fifteen kinds",
        );
    }
    if let Some(kind) = kinds
        .iter()
        .find(|kind| !closed(&rows[kind.name()], &["file", "count"]))
    {
        return refuse(
            "D10",
            "MANIFEST_SHAPE",
            format!("rows.{} is not exactly {{file, count}}", kind.name()),
        );
    }
    if let Some(kind) = kinds
        .iter()
        .find(|kind| rows[kind.name()]["file"] != kind.file())
    {
        return refuse(
            "D10",
            "PROFILE_KIND_COVERAGE",
            format!("rows.{}.file is not {}", kind.name(), kind.file()),
        );
    }
    if let Some(kind) = kinds
        .iter()
        .find(|kind| count(&rows[kind.name()]["count"]).is_none())
    {
        return refuse(
            "D10",
            "MANIFEST_SHAPE",
            format!("rows.{}.count is not a count", kind.name()),
        );
    }
    let relations = &manifest["relations"];
    if !closed(relations, &["file", "count"]) {
        return refuse(
            "D10",
            "MANIFEST_SHAPE",
            "relations is not exactly {file, count}",
        );
    }
    if relations["file"] != RELATIONS_PATH {
        return refuse(
            "D10",
            "PROFILE_KIND_COVERAGE",
            format!("relations.file is not {RELATIONS_PATH}"),
        );
    }
    if count(&relations["count"]).is_none() {
        return refuse("D10", "MANIFEST_SHAPE", "relations.count is not a count");
    }
    Ok(())
}

/// D11: the pins, every path safe, then every pin a sha256 of a file that is not the manifest.
fn pins_shape(manifest: &Map<String, Value>) -> Checked<()> {
    let Some(pins) = manifest["files"].as_object() else {
        return refuse("D11", "MANIFEST_SHAPE", "files is not an object");
    };
    if let Some(path) = pins.keys().find(|path| !safe_relative(path)) {
        return refuse(
            "D11",
            "PATH_UNSAFE",
            format!("files pins {path:?}, which is not a safe relative path"),
        );
    }
    if let Some(path) = pins
        .iter()
        .find_map(|(path, sha)| (path == MANIFEST_PATH || !hex64(sha)).then_some(path))
    {
        return refuse(
            "D11",
            "MANIFEST_SHAPE",
            format!("files.{path} is the manifest itself or not a lowercase sha256"),
        );
    }
    Ok(())
}

// ── E · the inventory ────────────────────────────────────────────────────────────────────────────

/// E1 to E5: every pin present, nothing unpinned, every pin the bytes', every required file
/// pinned, every pin in the layout.
fn inventory(
    files: &BTreeMap<String, Vec<u8>>,
    pins: &BTreeMap<String, String>,
    profile: &Profile,
    sha256: fn(&[u8]) -> String,
) -> Checked<()> {
    if let Some(path) = pins.keys().find(|path| !files.contains_key(*path)) {
        return refuse(
            "E1",
            "INVENTORY_MISSING",
            format!("{path} is pinned and absent"),
        );
    }
    if let Some(path) = files.keys().find(|path| !pins.contains_key(*path)) {
        return refuse(
            "E2",
            "INVENTORY_EXTRA",
            format!("{path} is present and not pinned"),
        );
    }
    let mismatch = pins
        .iter()
        .find(|(path, pin)| files.get(*path).is_some_and(|bytes| sha256(bytes) != **pin));
    if let Some((path, _)) = mismatch {
        return refuse(
            "E3",
            "PIN_MISMATCH",
            format!("{path} does not match its pin"),
        );
    }
    if let Some(path) = profile
        .required
        .iter()
        .find(|path| !pins.contains_key(*path))
    {
        return refuse(
            "E4",
            "INVENTORY_MISSING",
            format!("{path} is required and not pinned"),
        );
    }
    if let Some(path) = pins.keys().find(|path| !profile.in_layout(path)) {
        return refuse(
            "E5",
            "PROFILE_UNEXPECTED_FILE",
            format!("{path} is outside the layout"),
        );
    }
    Ok(())
}

// ── F · the rows ─────────────────────────────────────────────────────────────────────────────────

/// F1 (G2 for the relations) over a whole JSONL file: UTF-8 ending in LF, every line strict, an
/// object, and the canonical encoding of its value (RFC 8785 numbers).
fn line_layer(data: &[u8], path: &str, step: &'static str) -> Checked<Vec<Value>> {
    let Ok(text) = std::str::from_utf8(data) else {
        return refuse(step, "ROW_MALFORMED", format!("{path} is not UTF-8"));
    };
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let Some(body) = text.strip_suffix('\n') else {
        return refuse(step, "ROW_MALFORMED", format!("{path} does not end in LF"));
    };
    let mut values = Vec::new();
    for (n, raw) in body.split('\n').enumerate() {
        let at = format!("{path}:{}", n + 1);
        if raw.len() > MAX_LINE_BYTES {
            return refuse(
                step,
                "ROW_MALFORMED",
                format!("{at} is longer than {MAX_LINE_BYTES} bytes"),
            );
        }
        let value = match strict_line(raw, MAX_LINE_VALUES) {
            Ok(value) => value,
            Err(StrictJsonError::DuplicateKey(key)) => {
                return refuse(step, "ROW_DUPLICATE_KEY", format!("{at}: `{key}` twice"));
            }
            Err(error) => return refuse(step, "ROW_MALFORMED", format!("{at}: {error}")),
        };
        if !value.is_object() {
            return refuse(step, "ROW_MALFORMED", format!("{at} is not an object"));
        }
        // A literal that is not RFC 8785's never writes back as itself.
        if jcs_json(&value) != raw {
            return refuse(
                step,
                "ROW_MALFORMED",
                format!("{at} is not the canonical encoding of its value"),
            );
        }
        values.push(value);
    }
    Ok(values)
}

/// F per kind in profile order: the line layer, the count, then F3 to F12 per row.
fn rows(
    manifest: &Value,
    pins: &BTreeMap<String, String>,
    files: &BTreeMap<String, Vec<u8>>,
    profile: &Profile,
) -> Checked<BTreeMap<String, Vec<Value>>> {
    let target = &manifest["target"];
    let mut kinds_of: BTreeSet<String> = BTreeSet::new();
    let mut admitted = BTreeMap::new();
    for kind in profile.kinds() {
        let data = files
            .get(kind.file())
            .map(Vec::as_slice)
            .unwrap_or_default();
        let values = line_layer(data, kind.file(), "F1")?;
        let counted = count(&manifest["rows"][kind.name()]["count"]);
        if u64::try_from(values.len()).ok() != counted {
            return refuse(
                "F2",
                "ROW_COUNT",
                format!(
                    "{} holds {} rows, the manifest counts them otherwise",
                    kind.file(),
                    values.len()
                ),
            );
        }
        let mut previous: Option<&str> = None;
        for (n, row) in values.iter().enumerate() {
            let at = format!("{}:{}", kind.file(), n + 1);
            let id = identity(kind, row, &at, previous, &mut kinds_of)?;
            judge(kind, id, row, target, pins, files, profile)?;
            previous = Some(id);
        }
        admitted.insert(kind.name().to_owned(), values);
    }
    Ok(admitted)
}

/// F3 to F5: the row's kind and id, unique, after the previous row's.
fn identity<'r>(
    kind: &Kind,
    row: &'r Value,
    at: &str,
    previous: Option<&str>,
    seen: &mut BTreeSet<String>,
) -> Checked<&'r str> {
    let id = row["id"].as_str().filter(|id| {
        id.strip_prefix(kind.prefix())
            .and_then(|rest| rest.strip_prefix(':'))
            .is_some_and(super::grammar::id_name)
    });
    let Some(id) = id.filter(|_| row["kind"] == kind.name()) else {
        return refuse(
            "F3",
            "ROW_KIND",
            format!(
                "{at} is not a {} row with a well-formed {}: id",
                kind.name(),
                kind.prefix()
            ),
        );
    };
    if !seen.insert(id.to_owned()) {
        return refuse("F4", "ROW_DUPLICATE_ID", format!("{id} appears twice"));
    }
    if previous.is_some_and(|previous| id <= previous) {
        return refuse(
            "F5",
            "ROW_ORDER",
            format!("{at}: {id} does not follow the previous id"),
        );
    }
    Ok(id)
}

/// F6 to F12 for one row: digest, schema, vocabulary, evidence, split, body, disclosure.
fn judge(
    kind: &Kind,
    id: &str,
    row: &Value,
    target: &Value,
    pins: &BTreeMap<String, String>,
    files: &BTreeMap<String, Vec<u8>>,
    profile: &Profile,
) -> Checked<()> {
    if row["sha256"].as_str() != jcs_row_digest(row).as_deref() {
        return refuse(
            "F6",
            "ROW_DIGEST",
            format!("{id}: sha256 is not the row's digest"),
        );
    }
    if let Some((code, detail)) = schema::fault(row, kind.row(), profile) {
        return refuse("F7", code, format!("{id}: {detail}"));
    }
    let proof = row["proof_level"].as_str().unwrap_or_default();
    if !kind.proof_levels().iter().any(|level| level == proof) {
        return refuse(
            "F8",
            "ROW_VOCABULARY",
            format!(
                "{id}: proof level {proof:?} is not one a {} carries",
                kind.name()
            ),
        );
    }
    evidence(kind, id, row, target, proof)?;
    let disclosed = |key: &str, values: &[String]| {
        row[key]
            .as_str()
            .is_some_and(|v| values.iter().any(|w| w == v))
    };
    if matches!(kind.name(), "example" | "counterexample")
        && !(disclosed("split", &profile.splits) && disclosed("exposure", &profile.exposures))
    {
        return refuse(
            "F10",
            "ROW_SPLIT",
            format!(
                "{id}: split {} or exposure {} is not one a release carries",
                row["split"], row["exposure"]
            ),
        );
    }
    if kind.body_directory().is_some() {
        body(kind, id, row, pins, files, profile)?;
    }
    let undisclosed = row["ownership"] != "project" && row["upstream"].as_str().is_some_and(blank);
    if kind.name() == "source_artifact" && undisclosed {
        return refuse(
            "F12",
            "SOURCE_LICENCE",
            format!("{id}: material that is not the project's own discloses no upstream"),
        );
    }
    Ok(())
}

/// F9: CHECKED exactly when a receipt binds the target verifier and specification to the row's
/// bytes with a verdict the kind admits; any other proof level carries no receipt.
fn evidence(kind: &Kind, id: &str, row: &Value, target: &Value, proof: &str) -> Checked<()> {
    let receipt = row
        .get("check_receipt")
        .filter(|receipt| !receipt.is_null());
    if proof == "CHECKED" {
        let bound = receipt
            .filter(|receipt| receipt.is_object())
            .is_some_and(|receipt| {
                receipt.get("verifier_sha256") == target.get("verifier_sha256")
                    && receipt.get("spec_sha") == target.get("spec_sha")
                    && receipt.get("sha256") == row.get("file_sha256")
                    && receipt["verdict"]
                        .as_str()
                        .is_some_and(|verdict| kind.verdicts().iter().any(|v| v == verdict))
            });
        if !bound {
            return refuse(
                "F9",
                "ROW_EVIDENCE",
                format!(
                    "{id}: the check receipt is not bound to the target verifier, the spec and the file bytes with a verdict this kind admits"
                ),
            );
        }
    } else if receipt.is_some() {
        return refuse(
            "F9",
            "ROW_EVIDENCE",
            format!("{id}: a check receipt on a row whose proof level is not CHECKED"),
        );
    }
    Ok(())
}

/// F11: the body file is a pinned file of the kind's directory, its pin the row's `file_sha256`,
/// its text admitted.
fn body(
    kind: &Kind,
    id: &str,
    row: &Value,
    pins: &BTreeMap<String, String>,
    files: &BTreeMap<String, Vec<u8>>,
    profile: &Profile,
) -> Checked<()> {
    let path = row["file"].as_str().unwrap_or_default();
    let problem = if profile.body_kind(path).map(Kind::name) != Some(kind.name()) {
        Some(format!("{path} is not a {} body path", kind.name()))
    } else if !pins.contains_key(path) {
        Some(format!("{path} is not pinned"))
    } else if row["file_sha256"].as_str() != pins.get(path).map(String::as_str) {
        Some(format!("file_sha256 is not the pin of {path}"))
    } else if !files
        .get(path)
        .is_some_and(|bytes| file_text(bytes, MAX_BODY_BYTES))
    {
        Some(format!(
            "{path} is not 1 to {MAX_BODY_BYTES} bytes of admitted, non-blank text"
        ))
    } else {
        None
    };
    match problem {
        Some(problem) => refuse("F11", "ROW_FILE", format!("{id}: {problem}")),
        None => Ok(()),
    }
}

// ── G · lineage and relations ────────────────────────────────────────────────────────────────────

/// G1: every row but a source cites only source rows of this payload, and every source is cited.
fn lineage(rows: &BTreeMap<String, Vec<Value>>, profile: &Profile) -> Checked<()> {
    let sources: BTreeSet<&str> = rows
        .get("source_artifact")
        .into_iter()
        .flatten()
        .filter_map(|row| row["id"].as_str())
        .collect();
    let mut cited = BTreeSet::new();
    for kind in profile
        .kinds()
        .iter()
        .filter(|kind| kind.name() != "source_artifact")
    {
        for row in rows.get(kind.name()).into_iter().flatten() {
            for source in row["provenance"]["sources"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let source = source.as_str().unwrap_or_default();
                if !sources.contains(source) {
                    return refuse(
                        "G1",
                        "ROW_LINEAGE",
                        format!(
                            "{}: cites {source}, which is not a source row of this payload",
                            row["id"]
                        ),
                    );
                }
                cited.insert(source);
            }
        }
    }
    if let Some(source) = sources.difference(&cited).next() {
        return refuse("G1", "ROW_LINEAGE", format!("{source}: no row cites it"));
    }
    Ok(())
}

/// G2: the relations' line layer and count, then each edge exactly `{from, rel, to, attrs}`, a
/// relation of the profile between admitted rows of its domain and range, closed attributes, in
/// strictly increasing order.
fn relations(
    manifest: &Value,
    files: &BTreeMap<String, Vec<u8>>,
    rows: &BTreeMap<String, Vec<Value>>,
    profile: &Profile,
) -> Checked<Vec<Value>> {
    let data = files
        .get(RELATIONS_PATH)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let edges = line_layer(data, RELATIONS_PATH, "G2")?;
    if u64::try_from(edges.len()).ok() != count(&manifest["relations"]["count"]) {
        return refuse(
            "G2",
            "ROW_COUNT",
            format!(
                "{RELATIONS_PATH} holds {} relations, the manifest counts them otherwise",
                edges.len()
            ),
        );
    }
    let kind_of: BTreeMap<&str, &str> = rows
        .iter()
        .flat_map(|(kind, rows)| {
            rows.iter()
                .filter_map(move |row| Some((row["id"].as_str()?, kind.as_str())))
        })
        .collect();
    let mut previous: Option<(String, String, String, String)> = None;
    for (n, edge) in edges.iter().enumerate() {
        let at = format!("{RELATIONS_PATH}:{}", n + 1);
        let invalid = |why: String| refuse("G2", "RELATION_INVALID", format!("{at}{why}"));
        let ends = ["from", "rel", "to"].map(|key| edge[key].as_str());
        let [Some(from), Some(rel), Some(to)] = ends else {
            return invalid(" is not exactly {from, rel, to, attrs}".to_owned());
        };
        if !closed(edge, &["attrs", "from", "rel", "to"]) {
            return invalid(" is not exactly {from, rel, to, attrs}".to_owned());
        }
        let Some((domain, range)) = profile.relations.get(rel) else {
            return invalid(format!(": {rel:?} is not a relation of profile r2"));
        };
        let within = |id: &str, kinds: &[String]| {
            kind_of
                .get(id)
                .is_some_and(|kind| kinds.iter().any(|k| k == kind))
        };
        if !(within(from, domain) && within(to, range)) {
            return invalid(format!(
                ": {rel} goes from an admitted {} to an admitted {}",
                domain.join("/"),
                range.join("/")
            ));
        }
        let attrs = &edge["attrs"];
        let fault = if attrs.is_object() {
            schema::fault(attrs, &profile.attrs, profile).map(|(_, detail)| detail)
        } else {
            Some("attrs is not an object".to_owned())
        };
        if let Some(detail) = fault {
            return invalid(format!(": attrs: {detail}"));
        }
        let key = (
            from.to_owned(),
            rel.to_owned(),
            to.to_owned(),
            jcs_json(attrs),
        );
        if previous.as_ref().is_some_and(|previous| key <= *previous) {
            return invalid(" does not follow the previous relation".to_owned());
        }
        previous = Some(key);
    }
    Ok(edges)
}

// ── H · closure ──────────────────────────────────────────────────────────────────────────────────

/// H1 to H5: each body named once and every pinned body named, each source's licence text pinned
/// and every pinned licence text named, the notices admitted.
fn closure(
    rows: &BTreeMap<String, Vec<Value>>,
    pins: &BTreeMap<String, String>,
    files: &BTreeMap<String, Vec<u8>>,
    profile: &Profile,
) -> Checked<()> {
    let mut named: BTreeMap<&str, &str> = BTreeMap::new();
    for kind in profile
        .kinds()
        .iter()
        .filter(|kind| kind.body_directory().is_some())
    {
        for row in rows.get(kind.name()).into_iter().flatten() {
            let (file, id) = (
                row["file"].as_str().unwrap_or_default(),
                row["id"].as_str().unwrap_or_default(),
            );
            if let Some(first) = named.insert(file, id) {
                return refuse(
                    "H1",
                    "ROW_FILE",
                    format!("{file} is named by {first} and {id}"),
                );
            }
        }
    }
    if let Some(path) = pins
        .keys()
        .find(|path| profile.body_kind(path).is_some() && !named.contains_key(path.as_str()))
    {
        return refuse(
            "H2",
            "PROFILE_UNEXPECTED_FILE",
            format!("{path} is pinned and no row names it"),
        );
    }
    let sources = rows
        .get("source_artifact")
        .map(Vec::as_slice)
        .unwrap_or_default();
    let texts: BTreeSet<String> = sources
        .iter()
        .map(|row| {
            format!(
                "LICENSES/{}.txt",
                row["licence"].as_str().unwrap_or_default()
            )
        })
        .collect();
    for row in sources {
        let text = format!(
            "LICENSES/{}.txt",
            row["licence"].as_str().unwrap_or_default()
        );
        if !pins.contains_key(&text) {
            return refuse(
                "H3",
                "SOURCE_LICENCE",
                format!("{}: the text of {text} is not pinned", row["id"]),
            );
        }
    }
    if let Some(path) = pins
        .keys()
        .find(|path| path.starts_with("LICENSES/") && !texts.contains(*path))
    {
        return refuse(
            "H4",
            "SOURCE_LICENCE",
            format!("{path} is pinned and no source names its licence"),
        );
    }
    if !files
        .get(NOTICE_PATH)
        .is_some_and(|bytes| file_text(bytes, MAX_NOTICE_BYTES))
    {
        return refuse(
            "H5",
            "NOTICE_INVALID",
            format!(
                "{NOTICE_PATH} is not 1 to {MAX_NOTICE_BYTES} bytes of admitted, non-blank text"
            ),
        );
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
