// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source tracking — spans, file IDs, and byte-offset-to-line:col
//! conversion. The diagnostics substrate every finding anchors to.
//!
//! Split out of `nika-schema` per the size-cap discipline
//! (D-2026-07-09-N1 · one architectural unit, two workspace members):
//! `nika-schema::source` re-exports this crate wholesale, so every
//! consumer path (`nika_schema::source::Span` · `FileId` · `Spanned`)
//! is unchanged — the schema crate remains the unit's front door.

#![forbid(unsafe_code)]
#![warn(
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    missing_docs
)]

mod registry;
mod span;

pub use registry::{SourceFile, SourceRegistry};
pub use span::{ByteOffset, FileId, LineCol, Span, Spanned};
