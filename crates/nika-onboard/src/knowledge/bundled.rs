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
//! This payload (policy-R) holds three patterns and their realizing blocks, diagnostics and source
//! artifacts. Lexical recall selects patterns for the intent, then follows REALIZES to their blocks.
//! An intent sharing no recalled words receives no references. There is no family, repair principle
//! or example. Admission and presentation do not establish useful generation or runtime behavior.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{KnowledgeError, RefusalCode, Snapshot, TrustedIdentity};

/// The name the embedded release is admitted under: the root a refusal names and the `dir` of the
/// door's identity record. A label, never a path on disk.
pub(crate) const LABEL: &str = "embedded:nika-knowledge-release";

/// The issued `SNAPSHOT_SHA256`: the sha256 of the payload manifest's bytes.
pub(crate) const SNAPSHOT_SHA256: &str =
    "effc8d45b88a62c08cd4569abaadb8863823baaa0d52a313b925e9e1faf51b11";

/// The policy the payload was qualified under.
pub(crate) const POLICY_ID: &str = "policy-r";

/// The sha256 of that policy.
pub(crate) const POLICY_SHA256: &str =
    "d0471eeb904416dd411fde12244918771a5578ae1526f1e0a775b5d421087f36";

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
