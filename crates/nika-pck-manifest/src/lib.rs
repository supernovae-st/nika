// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-pck-manifest` — the pck sharing layer's data contract as pure types.
//!
//! ADR-094 (accepted 2026-07-08 · D-2026-07-08-N2) locks a sharing
//! architecture where **identity is decentralized** (an artifact IS its
//! blake3 hash · a ref is a git URL + path + version), **discovery is a
//! losable git index**, and **trust lives in the artifact** (a conformance
//! cert the installer re-derives locally). This crate is row 1 of the D6
//! crate mapping: the manifest · cert · lockfile · ref · hash types every
//! future pck crate (registry L1 · git L1 · orchestrator L2 · CLI L4)
//! deserializes against — landed FIRST so the wire shapes freeze under
//! `#[non_exhaustive]` + INV#19 before any I/O code exists.
//!
//! ## Fences (what this crate is NOT)
//!
//! Zero I/O · zero crypto (hashes are VALIDATED HEX, computed by
//! `nika-blob`; minisign verification is the suite's job) · zero ref-string
//! grammar (`PackageRef` is struct-only — a canonical syntax is an ADR-094
//! follow-up) · zero semantic claim validation (cert claims are opaque
//! strings; `nika verify` re-runs the real oracle).
//!
//! Layer **L0** — pure, zero I/O, zero async, zero `nika-*` deps.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod cert;
mod hash;
mod kind;
mod lockfile;
mod manifest;
pub mod nika_lock;
mod refs;

pub use cert::{Cert, CertVerdict};
pub use hash::{Blake3Hash, Sha256Hash};
pub use kind::{ArtifactKind, CustomKind};
pub use lockfile::{LockEntry, Lockfile};
pub use manifest::{FileEntry, Manifest};
pub use nika_lock::{LOCK_FORMAT, LockPin, LockRefusal, NIKA_LOCK_001, NikaLock};
pub use refs::PackageRef;

/// The one schema marker every pck document carries (FCI-003 · a
/// `nika/pck@2` still PARSES — `validate()` reports it, dispatch stays
/// version-shaped).
pub const PCK_SCHEMA: &str = "nika/pck@1";

/// This crate's failure modes — a local typed enum, NO `NIKA-` codes at L0
/// (the NIKA-200..299 pck range activates in the I/O-bearing suite crates ·
/// same posture as `nika-cap`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ManifestError {
    /// A hash field is not 64 hex characters.
    #[error("invalid {what}: `{got}` is not a 64-hex-char digest")]
    HashInvalid {
        /// Which digest kind was being parsed (`sha256` · `blake3`).
        what: &'static str,
        /// The offending input (verbatim).
        got: String,
    },
    /// The document's `schema:` is not [`PCK_SCHEMA`].
    #[error("unsupported pck schema `{got}` (this engine speaks `nika/pck@1`)")]
    SchemaUnsupported {
        /// The schema string the document carried.
        got: String,
    },
}
