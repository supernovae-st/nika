// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A complete `.nika` document at an exact revision: the parser's
//! [`Document`] (bytes, AST, literal projection, node positions) named by
//! the sha256 of its bytes, with the revision it descends from. A change
//! states its base: one aimed at any other revision than the current one is
//! refused as stale before anything is read, so a late correction can never
//! overwrite a newer one; the holder of the current revision (a Session, a
//! host) persists it. Pure: no store, no clock, no authority.

use std::fmt;

use sha2::{Digest, Sha256};

pub use nika_schema::document::{
    Applied, Document, Edit, Node, NodeKind, Path, Refusal, Splice, Style, literal_projection,
};

/// The identity of exact bytes: their lowercase hex sha256 (the value the
/// compiler's records already store as `candidate_sha256`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(String);

impl Revision {
    /// The revision of `source`.
    #[must_use]
    pub fn of(source: &str) -> Self {
        Self(format!("{:x}", Sha256::digest(source.as_bytes())))
    }

    /// A stored revision read back: 64 lowercase hex digits, nothing else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        (text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
            .then(|| Self(text.to_owned()))
    }

    /// The hex digest.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How a revision came to be. Only an edit carries byte evidence: a whole
/// replacement claims nothing about bytes it did not keep.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Origin {
    /// Bytes imported as they were.
    Imported,
    /// Targeted edits of the parent: the nodes they addressed and the
    /// splices that made them, every other byte the parent's.
    Edited {
        /// The nodes the edits addressed, in order.
        changed: Vec<Path>,
        /// The splices, in order, each in the text it was applied to.
        splices: Vec<Splice>,
    },
    /// A whole-source replacement of the parent.
    Replaced,
}

/// Why a revision was not made; the current revision is unchanged.
#[derive(Debug)]
#[non_exhaustive]
pub enum RevisionRefusal {
    /// The change was aimed at another revision than the current one.
    Stale {
        /// The revision the change named.
        base: Revision,
        /// The revision it would have replaced.
        current: Revision,
    },
    /// The document refused the edit or the new bytes.
    Document(Refusal),
}

impl fmt::Display for RevisionRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale { base, current } => write!(
                f,
                "the change targets revision {base}, but the current revision is {current}: it was not applied; read the current document and state it again"
            ),
            Self::Document(refusal) => refusal.fmt(f),
        }
    }
}

/// A [`Document`] at its exact revision, with its parent and origin.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DocumentRevision {
    document: Document,
    revision: Revision,
    parent: Option<Revision>,
    origin: Origin,
}

impl DocumentRevision {
    /// Import exact bytes as a first revision.
    ///
    /// # Errors
    /// The document's own refusal (the strict parser, or unreadable positions).
    pub fn import(source: impl Into<String>) -> Result<Self, Refusal> {
        let document = Document::parse(source)?;
        Ok(Self {
            revision: Revision::of(document.source()),
            document,
            parent: None,
            origin: Origin::Imported,
        })
    }

    /// The document itself.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The exact bytes.
    #[must_use]
    pub fn source(&self) -> &str {
        self.document.source()
    }

    /// This revision.
    #[must_use]
    pub fn revision(&self) -> &Revision {
        &self.revision
    }

    /// The revision it descends from; `None` for an import.
    #[must_use]
    pub fn parent(&self) -> Option<&Revision> {
        self.parent.as_ref()
    }

    /// How it was made.
    #[must_use]
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    fn current(&self, base: &Revision) -> Result<(), RevisionRefusal> {
        if base == &self.revision {
            Ok(())
        } else {
            Err(RevisionRefusal::Stale {
                base: base.clone(),
                current: self.revision.clone(),
            })
        }
    }

    fn child(&self, document: Document, origin: Origin) -> Self {
        if document.source() == self.source() {
            return self.clone();
        }
        Self {
            revision: Revision::of(document.source()),
            document,
            parent: Some(self.revision.clone()),
            origin,
        }
    }

    /// Apply targeted `edits` to this revision, named as `base`. Edits that
    /// leave the bytes as they are return this same revision.
    ///
    /// # Errors
    /// [`RevisionRefusal::Stale`] when `base` is not this revision, checked
    /// first; otherwise the document's refusal of an edit.
    pub fn apply(&self, base: &Revision, edits: &[Edit]) -> Result<Self, RevisionRefusal> {
        self.current(base)?;
        let applied = self
            .document
            .apply(edits)
            .map_err(RevisionRefusal::Document)?;
        let origin = Origin::Edited {
            changed: applied.changed().to_vec(),
            splices: applied.splices().to_vec(),
        };
        Ok(self.child(applied.into_document(), origin))
    }

    /// Replace the whole source of this revision, named as `base`: for a
    /// change no targeted edit states. The new bytes are imported whole;
    /// nothing is claimed about the bytes of the old ones.
    ///
    /// # Errors
    /// [`RevisionRefusal::Stale`] when `base` is not this revision; otherwise
    /// the document's refusal of the new bytes.
    pub fn replace(
        &self,
        base: &Revision,
        source: impl Into<String>,
    ) -> Result<Self, RevisionRefusal> {
        self.current(base)?;
        let document = Document::parse(source).map_err(RevisionRefusal::Document)?;
        Ok(self.child(document, Origin::Replaced))
    }
}

#[cfg(test)]
mod tests;
