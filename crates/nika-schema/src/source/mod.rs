// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source tracking — re-exported wholesale from [`nika_source`] (the
//! second member of this architectural unit · size-cap split per
//! D-2026-07-09-N1). Every consumer path is unchanged.

pub use nika_source::{ByteOffset, FileId, LineCol, SourceFile, SourceRegistry, Span, Spanned};
