// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source tracking — spans, file IDs, and byte-offset-to-line:col conversion.

mod registry;
mod span;

pub use registry::{SourceFile, SourceRegistry};
pub use span::{ByteOffset, FileId, LineCol, Span, Spanned};
