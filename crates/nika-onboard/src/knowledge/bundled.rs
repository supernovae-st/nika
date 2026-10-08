// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Issued knowledge releases embedded by this build. New authoring uses the current release, the
//! r2 run contract; retained a8 and R3 pins reopen their exact bytes. Selection by snapshot does
//! not admit a payload: every version still passes `Snapshot::from_files` against the caller's
//! complete trusted identity.

// One typed table for each finite embedded r1 release; paths remain literal and compiled in. The
// current r2 release is embedded whole from its directory (see `current`).
macro_rules! embedded_files {
    ($root:literal; $($path:literal),* $(,)?) => {
        [$(($path, include_bytes!(concat!($root, $path)) as &[u8])),*]
    };
}

mod a8;
mod current;

#[cfg(test)]
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{ADMISSION_PROFILE, KnowledgeError, RefusalCode, Snapshot, TrustedIdentity};

/// The name the embedded release is admitted under: the root a refusal names and the `dir` of the
/// door's identity record. A label, never a path on disk.
pub(crate) const LABEL: &str = "embedded:nika-knowledge-release";

/// The retained R3 snapshot identity, kept for earlier authoring pins.
pub(crate) const R3_SNAPSHOT_SHA256: &str =
    "b787fc53d6858db43d55958daaf02539fadcad4feeacc17b63c5aefcb92cc32b";

/// The policy family of the retained r1 payloads (R3 and a8), as their pins name it.
#[cfg(test)]
pub(crate) const POLICY_ID: &str = "policy-r";

/// R3's exact files. The current release explicitly replaces changed bytes over this table.
pub(crate) const FILES: [(&str, &[u8]); 14] = embedded_files!(
    "../../assets/knowledge-release/";
    "LICENSES/AGPL-3.0-or-later.txt",
    "NOTICE.md",
    "blocks/run-deterministic.nika",
    "blocks/typed-inputs-outputs.nika",
    "blocks/when-skipped-fallback.nika",
    "knowledge/blocks.jsonl",
    "knowledge/diagnostics.jsonl",
    "knowledge/families.jsonl",
    "knowledge/manifest.json",
    "knowledge/pattern_packs.jsonl",
    "knowledge/patterns.jsonl",
    "knowledge/relations.jsonl",
    "knowledge/repair_principles.jsonl",
    "knowledge/source_artifacts.jsonl",
);

/// The identity this build issues for new authoring. Earlier pins keep their own identity.
///
/// # Errors
/// [`KnowledgeError::Unavailable`] ([`RefusalCode::Untrusted`]) when the constants are not of an
/// identity's shape: typed and said, never a panic.
pub(crate) fn identity() -> Result<TrustedIdentity, KnowledgeError> {
    TrustedIdentity::r2(
        current::SNAPSHOT_SHA256,
        current::POLICY_ID,
        current::POLICY_SHA256,
    )
    .ok_or_else(|| KnowledgeError::Unavailable {
        root: PathBuf::from(LABEL),
        code: RefusalCode::Untrusted,
        detail: "the embedded release's issued identity is not of an identity's shape".to_owned(),
    })
}

/// Admit the exact issued bytes requested by a retained pin, or the current bytes for a new
/// identity. An identity naming a release this build does not embed is refused by the strict
/// memory door of its own profile (an r1 one against the latest r1 bytes, an r2 one against the
/// current), so the refusal names the identity it does not match; an incomplete one refuses too.
///
/// # Errors
/// The strict door's refusal ([`KnowledgeError::Unavailable`]), typed: never another source.
pub(crate) fn admit(identity: Option<&TrustedIdentity>) -> Result<Snapshot, KnowledgeError> {
    let files = match identity.map(|id| (id.snapshot_sha256(), id.profile())) {
        Some((R3_SNAPSHOT_SHA256, _)) => FILES
            .into_iter()
            .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
            .collect(),
        Some((a8::SNAPSHOT_SHA256, _) | (_, ADMISSION_PROFILE)) => a8::files(),
        _ => current::files(),
    };
    Snapshot::from_files(LABEL, files, identity)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
