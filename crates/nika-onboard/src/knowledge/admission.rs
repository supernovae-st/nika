// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The strict knowledge door: the admission of a Foundry knowledge release payload
//! ([`RELEASE_FORMAT`], profile [`ADMISSION_PROFILE`]) before any byte reaches a seat. Every
//! product door (`nika compile`, the session, serve) reads knowledge through this one admission.
//! It implements the r1 profile's shared contract (cited below as "the r1 contract §N"), the
//! contract the producer writes to. Both sides pass the same vectors (`tests/knowledge-r1`).
//!
//! There is no permissive mode and no partial load. A refusal is exactly one typed
//! [`RefusalCode`]: the first rule broken in admission order. The order is:
//! - **A.** The trust a [`TrustedIdentity`] carries, judged before any collection.
//! - **B.** The collection, bounded and anchored on held directory descriptors.
//! - **C.** The identity of the manifest's exact bytes.
//! - **D.** The manifest.
//! - **E.** The exact inventory and layout.
//! - **F.** The rows.
//! - **G.** Lineage and relations.
//! - **H.** The closure of files, licences and notices.
//!
//! The expected identity comes from the embedder, never from the payload. The payload's own
//! digests and pins are checked after it and never stand in for it.
//!
//! The bytes judged are the bytes kept: a pack presents them and never reads again.

use std::collections::BTreeMap;
use std::path::Path;

use nika_event::source_id::sha256_hex;
use serde_json::{Map, Value};

use super::canonical::strict_json;
use collect::Files;
pub(super) use collect::Stage;
use profile::{
    KINDS, LICENCES, block_path, hex64, licence_path, line, lower_hex, policy_token, safe_relative,
    spec_sha, token,
};

/// B · the collection: the memory form's bounds, the disk form's anchored walk.
mod collect;
/// The closed, typed profile this door admits: kinds, fields, types, relations, grammar.
mod profile;
/// F, G, H · the rows, the graph and the closure.
mod rows;
#[cfg(any(test, feature = "test-support"))]
pub(super) use profile::kind_files;

/// The payload format this door admits.
pub const RELEASE_FORMAT: &str = "nika-knowledge-release/2";
/// The admission profile this door enforces and a release names: the shared contract's closed
/// rules (kinds, fields and their types, evidence, lineage, relations, files, bounds).
pub const ADMISSION_PROFILE: &str = "nika-knowledge-release-profile/r1";
/// The manifest: the only payload file outside its own inventory.
pub(super) const MANIFEST_PATH: &str = "knowledge/manifest.json";
/// The relations between admitted rows.
pub(super) const RELATIONS_PATH: &str = "knowledge/relations.jsonl";
/// The notices of every retained source.
pub(super) const NOTICE_PATH: &str = "NOTICE.md";
/// The most files a payload holds, the manifest included.
pub(super) const MAX_FILES: usize = 10_000;
/// The most bytes a payload holds, the manifest included.
pub(super) const MAX_BYTES: u64 = 33_554_432;
/// The most bytes the manifest holds.
pub(super) const MAX_MANIFEST_BYTES: u64 = 4_194_304;
/// The most entries one directory listing holds (`.` and `..` aside), judged before any sort.
#[cfg_attr(not(unix), allow(dead_code))] // only the Unix walk lists directories
pub(super) const MAX_ENTRIES: usize = 10_001;
/// The most bytes one JSONL line holds, its LF aside.
pub(super) const MAX_LINE_BYTES: usize = 2_097_152;
/// The most values one JSONL line holds.
pub(super) const MAX_LINE_VALUES: usize = 8_192;
/// The most values the manifest holds.
pub(super) const MAX_MANIFEST_VALUES: usize = 32_768;
/// The largest count a manifest states (2^53 − 1).
const MAX_COUNT: u64 = 9_007_199_254_740_991;

/// Why the strict door refused a payload: one typed cause, the first found in admission order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum RefusalCode {
    /// No trusted expected identity, or one not of the profile's shape: nothing is collected.
    Untrusted,
    /// The disk root is not an absolute path.
    PathNotAbsolute,
    /// The root is absent or not a directory.
    RootInvalid,
    /// A symbolic link: the root itself or an entry under it, dangling or not.
    Symlink,
    /// An entry that is neither a directory nor a regular file, or a name that is not UTF-8.
    NotRegular,
    /// An entry the door could not read, or one that was no longer the entry it inspected.
    Io,
    /// Past a collection bound: files, bytes, the manifest's bytes, a directory's entries.
    TooLarge,
    /// No `knowledge/manifest.json`.
    ManifestMissing,
    /// The manifest's bytes are not the ones the trusted identity names.
    IdentityMismatch,
    /// The manifest is not strict JSON within its bounds.
    ManifestNotStrict,
    /// The manifest's closed shape: a key missing or unknown, a value of the wrong type.
    ManifestShape,
    /// The manifest names another format than the one this door admits.
    UnsupportedFormat,
    /// The manifest names another profile than the trusted identity's.
    ProfileMismatch,
    /// The manifest names another policy than the trusted identity's.
    PolicyMismatch,
    /// The manifest's kinds are not the profile's: a kind missing or unknown, a file renamed.
    KindCoverage,
    /// A path that is not a safe relative path.
    UnsafePath,
    /// A file the manifest pins, or the profile requires, is absent.
    MissingFile,
    /// A file is present that the manifest does not pin.
    ExtraFile,
    /// A file whose bytes are not the ones its pin names.
    PinMismatch,
    /// A file or directory outside the profile's layout, or a block file no row names.
    UnexpectedFile,
    /// A line that is not one canonical JSON object within its bounds, or holds a number.
    RowMalformed,
    /// A line states a key twice.
    DuplicateKey,
    /// A file's line count is not the manifest's.
    CountMismatch,
    /// A row of another kind than its file's, or an id outside its kind's grammar.
    RowKind,
    /// Two rows share an id.
    DuplicateId,
    /// A row not after the one before it, by id.
    RowOrder,
    /// A row's `sha256` is not the digest of its canonical content.
    RowDigest,
    /// A field outside the kind's closed schema, at the row or nested.
    UnknownField,
    /// A field the kind's schema requires is absent, at the row or nested.
    MissingField,
    /// A field of the wrong type or shape.
    FieldType,
    /// A row not pinned to the manifest's target.
    TargetPin,
    /// A status or proof level outside the kind's closed vocabulary.
    Vocabulary,
    /// A check receipt not bound to the target's verifier and specification and to the bytes.
    Evidence,
    /// A licence text or an upstream disclosure missing, or a licence text no source names.
    Licence,
    /// A block's file: not pinned, not its pin, not its grammar, or named by two blocks.
    RowFile,
    /// A lineage that names anything but retained source rows, or a source no row cites.
    Lineage,
    /// A relation outside the profile: its shape, name, an endpoint, the order.
    Relation,
    /// The notices are not a non-blank text of their grammar and bound.
    NoticeInvalid,
}

impl RefusalCode {
    /// Every code, in the contract's admission order.
    pub const ALL: [Self; 38] = [
        Self::Untrusted,
        Self::PathNotAbsolute,
        Self::RootInvalid,
        Self::Symlink,
        Self::NotRegular,
        Self::Io,
        Self::TooLarge,
        Self::ManifestMissing,
        Self::IdentityMismatch,
        Self::ManifestNotStrict,
        Self::ManifestShape,
        Self::UnsupportedFormat,
        Self::ProfileMismatch,
        Self::PolicyMismatch,
        Self::KindCoverage,
        Self::UnsafePath,
        Self::MissingFile,
        Self::ExtraFile,
        Self::PinMismatch,
        Self::UnexpectedFile,
        Self::RowMalformed,
        Self::DuplicateKey,
        Self::CountMismatch,
        Self::RowKind,
        Self::DuplicateId,
        Self::RowOrder,
        Self::RowDigest,
        Self::UnknownField,
        Self::MissingField,
        Self::FieldType,
        Self::TargetPin,
        Self::Vocabulary,
        Self::Evidence,
        Self::Licence,
        Self::RowFile,
        Self::Lineage,
        Self::Relation,
        Self::NoticeInvalid,
    ];

    /// The code's stable word, shared with the producer.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Untrusted => "ADMISSION_UNTRUSTED",
            Self::PathNotAbsolute => "PATH_NOT_ABSOLUTE",
            Self::RootInvalid => "ROOT_INVALID",
            Self::Symlink => "PAYLOAD_SYMLINK",
            Self::NotRegular => "PAYLOAD_NOT_REGULAR",
            Self::Io => "PAYLOAD_IO",
            Self::TooLarge => "PAYLOAD_TOO_LARGE",
            Self::ManifestMissing => "MANIFEST_MISSING",
            Self::IdentityMismatch => "IDENTITY_MISMATCH",
            Self::ManifestNotStrict => "MANIFEST_NOT_STRICT",
            Self::ManifestShape => "MANIFEST_SHAPE",
            Self::UnsupportedFormat => "MANIFEST_UNSUPPORTED_FORMAT",
            Self::ProfileMismatch => "PROFILE_MISMATCH",
            Self::PolicyMismatch => "POLICY_MISMATCH",
            Self::KindCoverage => "PROFILE_KIND_COVERAGE",
            Self::UnsafePath => "PATH_UNSAFE",
            Self::MissingFile => "INVENTORY_MISSING",
            Self::ExtraFile => "INVENTORY_EXTRA",
            Self::PinMismatch => "PIN_MISMATCH",
            Self::UnexpectedFile => "PROFILE_UNEXPECTED_FILE",
            Self::RowMalformed => "ROW_MALFORMED",
            Self::DuplicateKey => "ROW_DUPLICATE_KEY",
            Self::CountMismatch => "ROW_COUNT",
            Self::RowKind => "ROW_KIND",
            Self::DuplicateId => "ROW_DUPLICATE_ID",
            Self::RowOrder => "ROW_ORDER",
            Self::RowDigest => "ROW_DIGEST",
            Self::UnknownField => "ROW_UNKNOWN_FIELD",
            Self::MissingField => "ROW_MISSING_FIELD",
            Self::FieldType => "ROW_FIELD_TYPE",
            Self::TargetPin => "ROW_TARGET_PIN",
            Self::Vocabulary => "ROW_VOCABULARY",
            Self::Evidence => "ROW_EVIDENCE",
            Self::Licence => "SOURCE_LICENCE",
            Self::RowFile => "ROW_FILE",
            Self::Lineage => "ROW_LINEAGE",
            Self::Relation => "RELATION_INVALID",
            Self::NoticeInvalid => "NOTICE_INVALID",
        }
    }
}

impl std::fmt::Display for RefusalCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The identity a trusted embedder expects of a release, under this reader's profile
/// ([`ADMISSION_PROFILE`]): the release's `SNAPSHOT_SHA256` (the sha256 of its manifest's exact
/// bytes) and its policy `{id, sha256}`.
///
/// It comes from the embedder's own trusted record: the release pin of the build that vendors
/// the bytes, or a qualified release record a host holds. It never comes from the payload, its
/// directory, a file beside it, its own digests or a binary label.
///
/// It recognizes the bytes of one release and grants nothing else: no read, network, exec or
/// write, and no consent to what a block's text suggests. The door refuses a source without one
/// ([`RefusalCode::Untrusted`]) before it collects anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedIdentity {
    snapshot_sha256: String,
    policy_id: String,
    policy_sha256: String,
}

impl TrustedIdentity {
    /// An expected identity under this reader's profile. `None` when a part is not of its shape
    /// (a sha256 is 64 lowercase hex, a policy id `[a-z0-9][a-z0-9-]{0,63}`): the door then has
    /// no identity, and refuses the source as it refuses one named without any.
    #[must_use]
    pub fn new(snapshot_sha256: &str, policy_id: &str, policy_sha256: &str) -> Option<Self> {
        let shaped = lower_hex(snapshot_sha256, 64)
            && policy_token(policy_id)
            && lower_hex(policy_sha256, 64);
        shaped.then(|| Self {
            snapshot_sha256: snapshot_sha256.to_owned(),
            policy_id: policy_id.to_owned(),
            policy_sha256: policy_sha256.to_owned(),
        })
    }

    /// The identity a release record states (the r1 contract §2): exactly
    /// `{profile, snapshot_sha256, policy: {id, sha256}}`, its profile this reader's. `None`
    /// for anything else.
    #[must_use]
    pub fn from_json(value: &Value) -> Option<Self> {
        let record = value.as_object().filter(|record| record.len() == 3)?;
        let policy = record
            .get("policy")?
            .as_object()
            .filter(|policy| policy.len() == 2)?;
        if record.get("profile")?.as_str()? != ADMISSION_PROFILE {
            return None;
        }
        Self::new(
            record.get("snapshot_sha256")?.as_str()?,
            policy.get("id")?.as_str()?,
            policy.get("sha256")?.as_str()?,
        )
    }

    /// The profile this identity is under: this reader's.
    #[must_use]
    pub const fn profile(&self) -> &'static str {
        ADMISSION_PROFILE
    }

    /// The expected `SNAPSHOT_SHA256`.
    #[must_use]
    pub fn snapshot_sha256(&self) -> &str {
        &self.snapshot_sha256
    }

    /// The expected policy's id.
    #[must_use]
    pub fn policy_id(&self) -> &str {
        &self.policy_id
    }

    /// The expected policy's sha256.
    #[must_use]
    pub fn policy_sha256(&self) -> &str {
        &self.policy_sha256
    }
}

/// What admission keeps of a payload: its manifest and pins, every file's admitted bytes but the
/// manifest's, the rows by row-file stem, the relations, and each row file's sha256 by name.
#[derive(Debug)]
pub(super) struct Admitted {
    pub(super) manifest: Value,
    pub(super) manifest_sha256: String,
    pub(super) pins: BTreeMap<String, String>,
    pub(super) files: BTreeMap<String, Vec<u8>>,
    pub(super) rows: BTreeMap<String, Vec<Value>>,
    pub(super) relations: Vec<Value>,
    pub(super) row_files: BTreeMap<String, String>,
}

/// The rule of the admission order a refusal breaks, as the r1 contract §10 names it (`B5`,
/// `F7`): two rules may give one code, and the step tells them apart.
pub(super) type Step = &'static str;

/// One refusal, before a door binds it to the source it named: the rule broken, the code, the
/// detail.
#[derive(Debug)]
pub(super) struct Refusal(pub(super) Step, pub(super) RefusalCode, pub(super) String);

pub(super) type Checked<T> = Result<T, Refusal>;

fn refuse<T>(step: Step, code: RefusalCode, detail: impl Into<String>) -> Checked<T> {
    Err(Refusal(step, code, detail.into()))
}

/// The disk form: admit the payload at the root `root` (A, then B through H). Nothing of the
/// root is touched unless a trusted identity is named. `probe` is a test barrier between two
/// steps of the walk; a door passes one that does nothing.
pub(super) fn admit(
    root: &Path,
    identity: Option<&TrustedIdentity>,
    probe: &mut dyn FnMut(Stage, &str),
) -> Checked<Admitted> {
    let identity = trusted(identity)?;
    let files = collect::read_root(root, probe)?;
    admit_collected(files, identity)
}

/// The memory form: admit the files an embedder holds, by their relative paths (A, then the
/// paths and bounds of B, then C through H).
pub(super) fn admit_memory(
    files: BTreeMap<String, Vec<u8>>,
    identity: Option<&TrustedIdentity>,
) -> Checked<Admitted> {
    let identity = trusted(identity)?;
    collect::check_memory(&files)?;
    admit_collected(files, identity)
}

/// A1: a trusted identity is named.
fn trusted(identity: Option<&TrustedIdentity>) -> Checked<&TrustedIdentity> {
    identity.ok_or_else(|| {
        Refusal(
            "A1",
            RefusalCode::Untrusted,
            "no trusted expected identity: the embedder names one from its own release record, and a named source carries none today".to_owned(),
        )
    })
}

/// C through H, on the collected files (every path under the root, the manifest's included).
fn admit_collected(mut files: Files, identity: &TrustedIdentity) -> Checked<Admitted> {
    let Some(manifest_bytes) = files.remove(MANIFEST_PATH) else {
        return refuse("C1", RefusalCode::ManifestMissing, MANIFEST_PATH);
    };
    let manifest_sha256 = sha256_hex(&manifest_bytes);
    if manifest_sha256 != identity.snapshot_sha256() {
        return refuse(
            "C2",
            RefusalCode::IdentityMismatch,
            format!(
                "the manifest reads sha256 {manifest_sha256}, the trusted identity names {}",
                identity.snapshot_sha256()
            ),
        );
    }
    let (manifest, pins) = manifest(&manifest_bytes, identity)?;
    inventory(&files, &pins)?;
    rows::admit_rows(manifest, manifest_sha256, pins, files)
}

// ── D · the manifest: strict, the format, the trusted profile and policy, closed shape ──────────

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

/// The kinds of the producer's ontology a release does not ship, in this order: the drop stated.
pub(super) const EXCLUDED_KINDS: [&str; 8] = [
    "callable",
    "capability_interface",
    "construct",
    "counterexample",
    "example",
    "intent_facet",
    "skeleton",
    "skill",
];

/// The reasons a manifest may count for rows the policy left out (closed).
const EXCLUSION_REASONS: [&str; 9] = [
    "STATUS_EXCLUDED",
    "CHECK_NOT_CURRENT_ON_TARGET",
    "CHECK_UNPROVEN",
    "OUTCOME_UNPROVEN",
    "CASCADE_MANDATORY_REFERENCE",
    "CASCADE_EMPTY_AFTER_PRUNE",
    "SOURCE_UNCITED",
    "SOURCE_EXCLUDED",
    "SOURCE_EXCLUDED_CASCADE",
];

/// The reasons a manifest may count for rows whose claim the policy lowered (closed).
const DOWNGRADE_REASONS: [&str; 2] = ["OUTCOME_UNPROVEN", "CHECK_UNPROVEN"];

/// D1 to D11: the manifest as strict JSON, its format, the trusted profile and policy, then
/// its closed shape, every nested value typed, the kind coverage and the pins.
fn manifest(
    bytes: &[u8],
    identity: &TrustedIdentity,
) -> Checked<(Value, BTreeMap<String, String>)> {
    let not_strict = |words: String| Refusal("D1", RefusalCode::ManifestNotStrict, words);
    let text = std::str::from_utf8(bytes).map_err(|_| not_strict("not UTF-8".to_owned()))?;
    let value =
        strict_json(text, MAX_MANIFEST_VALUES).map_err(|error| not_strict(error.to_string()))?;
    let Some(object) = value.as_object() else {
        return refuse(
            "D2",
            RefusalCode::ManifestShape,
            "the manifest is not an object",
        );
    };
    if object.get("format").and_then(Value::as_str) != Some(RELEASE_FORMAT) {
        return refuse(
            "D3",
            RefusalCode::UnsupportedFormat,
            format!("this door admits {RELEASE_FORMAT}"),
        );
    }
    if object.get("profile").and_then(Value::as_str) != Some(identity.profile()) {
        return refuse(
            "D4",
            RefusalCode::ProfileMismatch,
            format!("the trusted identity names {}", identity.profile()),
        );
    }
    closed_keys("D5", object, &MANIFEST_KEYS, "the manifest")?;
    closed_object("D6", &value["policy"], &["id", "sha256"], "policy")?;
    let policy = &value["policy"];
    if !(policy["id"].as_str().is_some_and(policy_token) && hex64(&policy["sha256"])) {
        return refuse(
            "D6",
            RefusalCode::ManifestShape,
            "policy is not {id, sha256}",
        );
    }
    if policy["id"] != identity.policy_id() || policy["sha256"] != identity.policy_sha256() {
        return refuse(
            "D7",
            RefusalCode::PolicyMismatch,
            format!(
                "the manifest names policy {} {}, the trusted identity {} {}",
                policy["id"],
                policy["sha256"],
                identity.policy_id(),
                identity.policy_sha256()
            ),
        );
    }
    typed_heads(&value)?;
    exclusions(&value["exclusions"], &value["downgrades"])?;
    coverage(&value)?;
    let pins = pins(&value["files"])?;
    Ok((value, pins))
}

/// D8: `knowledge_version`, `target`, `tool` and `input_digest`, each of its type.
fn typed_heads(manifest: &Value) -> Checked<()> {
    let version = manifest["knowledge_version"].as_str().unwrap_or_default();
    closed_object("D8", &manifest["tool"], &["id", "sha256"], "tool")?;
    closed_object(
        "D8",
        &manifest["target"],
        &["binary", "spec_sha", "verifier_sha256"],
        "target",
    )?;
    let (tool, target) = (&manifest["tool"], &manifest["target"]);
    let shaped = token(version, 64, |c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || ".+-".contains(c)
    }) && tool["id"].as_str().is_some_and(line)
        && hex64(&tool["sha256"])
        && target["binary"].as_str().is_some_and(line)
        && spec_sha(&target["spec_sha"])
        && hex64(&target["verifier_sha256"])
        && hex64(&manifest["input_digest"]);
    if !shaped {
        return refuse(
            "D8",
            RefusalCode::ManifestShape,
            "knowledge_version, target, tool or input_digest is not of its type",
        );
    }
    Ok(())
}

/// `value` as an object holding exactly `keys` (each a manifest key), judged at `step`.
fn closed_object<'a>(
    step: Step,
    value: &'a Value,
    keys: &[&str],
    what: &str,
) -> Checked<&'a Map<String, Value>> {
    let Some(object) = value.as_object() else {
        return refuse(
            step,
            RefusalCode::ManifestShape,
            format!("{what} is not an object"),
        );
    };
    closed_keys(step, object, keys, what)?;
    Ok(object)
}

/// An object's keys are exactly `keys`, judged at `step`.
fn closed_keys(step: Step, object: &Map<String, Value>, keys: &[&str], what: &str) -> Checked<()> {
    let unknown = object.keys().find(|key| !keys.contains(&key.as_str()));
    let missing = keys.iter().find(|key| !object.contains_key(**key));
    match (unknown, missing) {
        (None, None) => Ok(()),
        (Some(key), _) => refuse(
            step,
            RefusalCode::ManifestShape,
            format!("{what}: {key} is unknown"),
        ),
        (None, Some(key)) => refuse(
            step,
            RefusalCode::ManifestShape,
            format!("{what}: no {key}"),
        ),
    }
}

/// A count: a non-negative integer of at most 2^53 − 1, at least `least`, never a boolean,
/// a fraction or an exponent.
fn count(value: &Value, least: u64) -> bool {
    value
        .as_u64()
        .is_some_and(|n| (least..=MAX_COUNT).contains(&n))
}

/// D9: the exclusion record `{kinds: EXCLUDED_KINDS, rows: {reason: count ≥ 1}}` and the
/// downgrades `{reason: count ≥ 1}`, every reason from its closed list.
fn exclusions(value: &Value, downgrades: &Value) -> Checked<()> {
    closed_object("D9", value, &["kinds", "rows"], "exclusions")?;
    let kinds_stated = value["kinds"].as_array().is_some_and(|kinds| {
        kinds.len() == EXCLUDED_KINDS.len()
            && kinds
                .iter()
                .zip(EXCLUDED_KINDS)
                .all(|(kind, want)| kind == want)
    });
    let counted = |reasons: &Value, closed: &[&str]| {
        reasons.as_object().is_some_and(|reasons| {
            reasons
                .iter()
                .all(|(reason, n)| closed.contains(&reason.as_str()) && count(n, 1))
        })
    };
    if !kinds_stated
        || !counted(&value["rows"], &EXCLUSION_REASONS)
        || !counted(downgrades, &DOWNGRADE_REASONS)
    {
        return refuse(
            "D9",
            RefusalCode::ManifestShape,
            "exclusions or downgrades are not the profile's closed record",
        );
    }
    Ok(())
}

/// D10: exactly the profile's kinds, each at its row file with a count, and the relations at
/// theirs; a skill is named apart.
fn coverage(manifest: &Value) -> Checked<()> {
    let Some(rows) = manifest["rows"].as_object() else {
        return refuse("D10", RefusalCode::ManifestShape, "rows is not an object");
    };
    if rows.contains_key("skill") {
        return refuse(
            "D10",
            RefusalCode::KindCoverage,
            "rows.skill: a release ships no skill",
        );
    }
    let unknown = rows
        .keys()
        .find(|kind| !KINDS.iter().any(|shipped| shipped.kind == kind.as_str()));
    let missing = KINDS.iter().find(|kind| !rows.contains_key(kind.kind));
    if let Some(kind) = unknown
        .map(String::as_str)
        .or(missing.map(|kind| kind.kind))
    {
        return refuse("D10", RefusalCode::KindCoverage, format!("rows: {kind}"));
    }
    let entries = KINDS
        .iter()
        .map(|kind| (&manifest["rows"][kind.kind], kind.file, kind.kind))
        .chain([(&manifest["relations"], RELATIONS_PATH, "relations")]);
    for (entry, file, what) in entries {
        closed_object("D10", entry, &["file", "count"], what)?;
        if entry["file"].as_str() != Some(file) {
            return refuse(
                "D10",
                RefusalCode::KindCoverage,
                format!("{what}.file is not {file}"),
            );
        }
        if !count(&entry["count"], 0) {
            return refuse(
                "D10",
                RefusalCode::ManifestShape,
                format!("{what}.count is not a count"),
            );
        }
    }
    Ok(())
}

/// D11: the pins, every path safe and relative, every value a sha256, never the manifest.
fn pins(files: &Value) -> Checked<BTreeMap<String, String>> {
    let Some(files) = files.as_object() else {
        return refuse("D11", RefusalCode::ManifestShape, "files is not an object");
    };
    let mut pins = BTreeMap::new();
    for (path, sha) in files {
        if !safe_relative(path) {
            return refuse("D11", RefusalCode::UnsafePath, path.clone());
        }
        let Some(sha) = sha.as_str().filter(|sha| lower_hex(sha, 64)) else {
            return refuse("D11", RefusalCode::ManifestShape, format!("files.{path}"));
        };
        if path == MANIFEST_PATH {
            return refuse(
                "D11",
                RefusalCode::ManifestShape,
                "files pins the manifest itself",
            );
        }
        pins.insert(path.clone(), sha.to_owned());
    }
    Ok(pins)
}

// ── E · the exact inventory and the closed file layout ─────────────────────────────────────────

/// E1 to E5: every pinned file present, no file unpinned, every file its pin, the required
/// files pinned, every pinned file a row file, the relations, the notices, a block's file or an
/// admitted licence text.
fn inventory(files: &Files, pins: &BTreeMap<String, String>) -> Checked<()> {
    if let Some(path) = pins.keys().find(|path| !files.contains_key(*path)) {
        return refuse("E1", RefusalCode::MissingFile, path.clone());
    }
    if let Some(path) = files.keys().find(|path| !pins.contains_key(*path)) {
        return refuse("E2", RefusalCode::ExtraFile, path.clone());
    }
    for (path, bytes) in files {
        let found = sha256_hex(bytes);
        if pins.get(path) != Some(&found) {
            return refuse(
                "E3",
                RefusalCode::PinMismatch,
                format!("{path} reads sha256 {found}"),
            );
        }
    }
    let required = KINDS
        .iter()
        .map(|kind| kind.file)
        .chain([RELATIONS_PATH, NOTICE_PATH]);
    for path in required {
        if !pins.contains_key(path) {
            return refuse("E4", RefusalCode::MissingFile, path);
        }
    }
    for path in pins.keys() {
        let fixed = KINDS.iter().any(|kind| kind.file == path)
            || path == RELATIONS_PATH
            || path == NOTICE_PATH;
        if !fixed && !block_path(path) && licence_path(path).is_none() {
            return refuse(
                "E5",
                RefusalCode::UnexpectedFile,
                format!("{path}: outside the profile's layout (licences: {LICENCES:?})"),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod constructed;
#[cfg(test)]
mod real_payload;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod vectors;
