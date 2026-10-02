// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The knowledge release this build embeds: the issued bytes of one qualified Foundry payload,
//! compiled in from the product's one copy (`assets/knowledge-release/`) and admitted through the
//! strict memory door ([`Snapshot::from_files`]) against the identity its owner issued — constants
//! of this build, never read from the payload's own declarations. Nothing is read from disk. A
//! qualified payload, whose owner's record states `official_release: false`: nothing here calls it
//! an official release. It is the knowledge where nothing names any
//! ([`crate::compile_config::KnowledgeChoice::Default`]).
//!
//! Available and admitted is not composed: this payload (policy-R) holds three blocks, the
//! diagnostics and their source artifacts, and no family, pattern, relation, repair principle or
//! example. The door reaches a block only through a pattern that realizes it, so every pack it
//! composes from this release presents no reference and no repair principle: its identity and its
//! selection record are stated, and no byte of it reaches a seat.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{KnowledgeError, RefusalCode, Snapshot, TrustedIdentity};

/// The name the embedded release is admitted under: the root a refusal names and the `dir` of the
/// door's identity record. A label, never a path on disk.
pub(crate) const LABEL: &str = "embedded:nika-knowledge-release";

/// The issued `SNAPSHOT_SHA256`: the sha256 of the payload manifest's bytes.
pub(crate) const SNAPSHOT_SHA256: &str =
    "5bcd108a78e9fbb6e27827b34d8090b74f6285a125cdcef812b33dd51738e692";

/// The policy the payload was qualified under.
pub(crate) const POLICY_ID: &str = "policy-r";

/// The sha256 of that policy.
pub(crate) const POLICY_SHA256: &str =
    "5b567a1557ba430fe57fe9a80934de29b2b09805d4cf26b4868e3395c8795465";

/// The payload's files, each path under its root with its issued bytes: a finite table.
pub(crate) const FILES: [(&str, &[u8]); 14] = [
    (
        "LICENSES/AGPL-3.0-or-later.txt",
        include_bytes!("../../assets/knowledge-release/LICENSES/AGPL-3.0-or-later.txt"),
    ),
    (
        "NOTICE.md",
        include_bytes!("../../assets/knowledge-release/NOTICE.md"),
    ),
    (
        "blocks/run-deterministic.nika",
        include_bytes!("../../assets/knowledge-release/blocks/run-deterministic.nika"),
    ),
    (
        "blocks/typed-inputs-outputs.nika",
        include_bytes!("../../assets/knowledge-release/blocks/typed-inputs-outputs.nika"),
    ),
    (
        "blocks/when-skipped-fallback.nika",
        include_bytes!("../../assets/knowledge-release/blocks/when-skipped-fallback.nika"),
    ),
    (
        "knowledge/blocks.jsonl",
        include_bytes!("../../assets/knowledge-release/knowledge/blocks.jsonl"),
    ),
    (
        "knowledge/diagnostics.jsonl",
        include_bytes!("../../assets/knowledge-release/knowledge/diagnostics.jsonl"),
    ),
    (
        "knowledge/families.jsonl",
        include_bytes!("../../assets/knowledge-release/knowledge/families.jsonl"),
    ),
    (
        "knowledge/manifest.json",
        include_bytes!("../../assets/knowledge-release/knowledge/manifest.json"),
    ),
    (
        "knowledge/pattern_packs.jsonl",
        include_bytes!("../../assets/knowledge-release/knowledge/pattern_packs.jsonl"),
    ),
    (
        "knowledge/patterns.jsonl",
        include_bytes!("../../assets/knowledge-release/knowledge/patterns.jsonl"),
    ),
    (
        "knowledge/relations.jsonl",
        include_bytes!("../../assets/knowledge-release/knowledge/relations.jsonl"),
    ),
    (
        "knowledge/repair_principles.jsonl",
        include_bytes!("../../assets/knowledge-release/knowledge/repair_principles.jsonl"),
    ),
    (
        "knowledge/source_artifacts.jsonl",
        include_bytes!("../../assets/knowledge-release/knowledge/source_artifacts.jsonl"),
    ),
];

/// The identity this build trusts for its embedded release: the issued constants above.
///
/// # Errors
/// [`KnowledgeError::Unavailable`] ([`RefusalCode::Untrusted`]) when the constants are not of an
/// identity's shape: typed and said, never a panic.
pub(crate) fn identity() -> Result<TrustedIdentity, KnowledgeError> {
    TrustedIdentity::new(SNAPSHOT_SHA256, POLICY_ID, POLICY_SHA256).ok_or_else(|| {
        KnowledgeError::Unavailable {
            root: PathBuf::from(LABEL),
            code: RefusalCode::Untrusted,
            detail: "the embedded release's issued identity is not of an identity's shape"
                .to_owned(),
        }
    })
}

/// The embedded release admitted now through the strict memory door, against `identity` (this
/// build's own, or the one a pin holds): no byte is read from disk, and without an identity the
/// door refuses it as it refuses any source.
///
/// # Errors
/// The strict door's refusal ([`KnowledgeError::Unavailable`]), typed: never another source.
pub(crate) fn admit(identity: Option<&TrustedIdentity>) -> Result<Snapshot, KnowledgeError> {
    let files: BTreeMap<String, Vec<u8>> = FILES
        .into_iter()
        .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
        .collect();
    Snapshot::from_files(LABEL, files, identity)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
