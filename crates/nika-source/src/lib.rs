// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source tracking — spans, file IDs, and byte-offset-to-line:col
//! conversion — plus the pure lexical program-source naming contract.
//! The diagnostics substrate every finding anchors to.
//!
//! Split out of `nika-schema` per the size-cap discipline
//! (D-2026-07-09-N1 · one architectural unit, two workspace members):
//! `nika-schema::source` re-exports this crate wholesale, so every
//! consumer path (`nika_schema::source::Span` · `FileId` · `Spanned`)
//! is unchanged — the schema crate remains the unit's front door.
//! Naming is owned here (zero I/O); generic `nika-fs` does not.

#![forbid(unsafe_code)]
#![warn(
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    missing_docs
)]

mod naming;
mod registry;
mod span;

pub use naming::{
    PROGRAM_GLOB, PROGRAM_GLOB_RECURSIVE, PROGRAM_SUFFIX, PROJECT_FILE_NAME,
    RETIRED_PROGRAM_SUFFIXES, RUNTIME_DIR_NAME, SourceNameKind, classify_file_name, classify_path,
    is_canonical_program_file_name, is_canonical_program_path, is_retired_program_file_name,
    path_file_name, program_stem, retired_rename_hint, typed_stem, with_program_suffix,
};
pub use registry::{SourceFile, SourceRegistry};
pub use span::{ByteOffset, FileId, LineCol, Span, Spanned};
