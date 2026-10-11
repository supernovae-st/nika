// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The knowledge door — its strict admission, the releases this build embeds, the catalogue and
//! the authoring pack — lives in the size-cap member `nika-onboard-knowledge` (the ADR-137
//! amendment), re-exported here item by item, so every `knowledge::` path stays. The pin of a
//! release in an authoring configuration stays here ([`pin`]), beside the configuration and the
//! compile round it records.

pub(crate) use nika_onboard_knowledge::bundled;
#[cfg(any(test, feature = "test-support"))]
pub use nika_onboard_knowledge::fixture;
pub use nika_onboard_knowledge::{
    ADMISSION_PROFILE, Catalogue, ComponentCatalog, KnowledgeError, PACK_BUILDER, RELEASE_FORMAT,
    RefusalCode, Snapshot, TrustedIdentity, pack_from_file, pack_sha256, redact_host_paths,
};

/// The pinned snapshot identity and the records a session stamps on an outcome (C7 · D1).
pub mod pin;
