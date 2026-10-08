// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Why an import or an edit was not applied. A refusal is an outcome, never
//! a verdict on the intent: the current source is kept as it was, and a
//! caller may state another edit or a whole-source revision instead.

use std::fmt;

use super::Path;
use crate::error::SchemaError;

/// Why a document edit was not applied. Every variant leaves the current
/// source unchanged; [`Refusal::Layout`] in particular means only that this
/// presentation is not edited in place, never that the language or the
/// intent refuses the change.
#[derive(Debug)]
#[non_exhaustive]
pub enum Refusal {
    /// No node at this path.
    UnknownPath {
        /// The path that addresses nothing.
        path: Path,
    },
    /// The node exists but cannot take this kind of edit (an insert into a
    /// sequence, a key that already exists, the root replaced in place).
    Shape {
        /// The node the edit addressed.
        path: Path,
        /// What the node is and what the edit needed.
        detail: String,
    },
    /// The node's presentation is not one this revision edits in place; the
    /// source was kept byte for byte and nothing was reserialized.
    Layout {
        /// The node the edit addressed.
        path: Path,
        /// Which presentation stopped the edit.
        detail: String,
    },
    /// The edited bytes are refused by the strict parser (a type, a closed
    /// enum, an unknown key, a malformed value).
    Language {
        /// The node the edit addressed (the root for an import).
        path: Path,
        /// The strict parser's own error.
        error: Box<SchemaError>,
    },
    /// The edited bytes parse, but their meaning changed beyond the stated
    /// edit (the literal projection or an untouched AST component differs).
    Drift {
        /// The node the edit addressed.
        path: Path,
        /// Where the meaning moved.
        detail: String,
    },
}

impl Refusal {
    /// The node the refused edit addressed.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::UnknownPath { path }
            | Self::Shape { path, .. }
            | Self::Layout { path, .. }
            | Self::Language { path, .. }
            | Self::Drift { path, .. } => path,
        }
    }

    /// The stable machine word of the refusal (`unknown_path` · `shape` ·
    /// `layout` · `language` · `drift`).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::UnknownPath { .. } => "unknown_path",
            Self::Shape { .. } => "shape",
            Self::Layout { .. } => "layout",
            Self::Language { .. } => "language",
            Self::Drift { .. } => "drift",
        }
    }
}

impl fmt::Display for Refusal {
    /// One sentence naming the path and the reason, for a person and for a
    /// seat's repair prompt alike; it never repeats a value of the document.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPath { path } => write!(
                f,
                "no node at `{path}`: the document has no such key or index; the source is unchanged"
            ),
            Self::Shape { path, detail } => write!(
                f,
                "`{path}` cannot take this edit: {detail}; the source is unchanged"
            ),
            Self::Layout { path, detail } => write!(
                f,
                "`{path}` is written in a presentation this revision does not edit in place ({detail}); the source is unchanged, state another edit or a whole-source revision"
            ),
            Self::Language { path, error } if path.is_root() => write!(
                f,
                "the strict parser refuses the document ({}): {error}",
                error.spec_code()
            ),
            Self::Language { path, error } => write!(
                f,
                "the edit at `{path}` yields bytes the strict parser refuses ({}): {error}; the source is unchanged",
                error.spec_code()
            ),
            Self::Drift { path, detail } => write!(
                f,
                "the edit at `{path}` would change more than it states ({detail}); the source is unchanged"
            ),
        }
    }
}
