// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A [`Document`] at an exact revision: its bytes named by a digest, with the
//! revision it descends from. A change states its base: one aimed at any other
//! revision than the current one is refused as stale before anything is read,
//! so a late correction can never overwrite a newer one; the holder of the
//! current revision (a Session, a host) persists it.
//!
//! The digest is the caller's (`nika_compile::surface::sha256` names bytes by
//! the lowercase hex sha256 compiler records already store as
//! `candidate_sha256`): this pure crate hashes nothing itself.

use std::fmt;

use super::{Document, Edit, Path, Refusal, Splice};

/// How a revision names exact bytes (the same function for every revision
/// of a document).
pub type Digest = fn(&str) -> String;

/// The identity of exact bytes, as their [`Digest`] names them.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct Revision(String);

impl Revision {
    /// The revision a digest names (a stored one read back, or `digest(source)`).
    #[must_use]
    pub fn new(digest: impl Into<String>) -> Self {
        Self(digest.into())
    }

    /// The digest.
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
    digest: Digest,
}

impl DocumentRevision {
    /// Import exact bytes as a first revision, named by `digest`.
    ///
    /// # Errors
    /// The document's own refusal (the strict parser, or unreadable positions).
    pub fn import(source: impl Into<String>, digest: Digest) -> Result<Self, Refusal> {
        let document = Document::parse(source)?;
        Ok(Self {
            revision: Revision::new(digest(document.source())),
            document,
            parent: None,
            origin: Origin::Imported,
            digest,
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
            revision: Revision::new((self.digest)(document.source())),
            document,
            parent: Some(self.revision.clone()),
            origin,
            digest: self.digest,
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
mod tests {
    use serde_json::json;

    use super::{DocumentRevision, Origin, Revision, RevisionRefusal};
    use crate::document::{Edit, Path};

    const BASE: &str = "nika: window\n# the threshold\nconst:\n  window_hours: 48\ntasks:\n  t:\n    exec:\n      command: [\"echo\", \"${{ const.window_hours }}\"]\n";

    /// A test digest that names bytes by themselves: exact, collision-free.
    fn exact(source: &str) -> String {
        source.to_owned()
    }

    fn hours(value: i64) -> Edit {
        Edit::set(Path::new(["const", "window_hours"]), json!(value))
    }

    #[test]
    fn a_revision_is_the_digest_of_its_exact_bytes() {
        let first = DocumentRevision::import(BASE, exact).expect("import");
        assert_eq!(first.revision(), &Revision::new(BASE));
        assert_eq!(first.revision().as_str(), BASE);
        assert_eq!(first.parent(), None);
        assert_eq!(first.origin(), &Origin::Imported);
    }

    #[test]
    fn an_edit_names_its_base_and_links_its_parent() {
        let first = DocumentRevision::import(BASE, exact).expect("import");
        let second = first.apply(first.revision(), &[hours(72)]).expect("72");
        assert_eq!(second.parent(), Some(first.revision()));
        assert_eq!(second.source(), BASE.replace("48", "72"));
        assert_eq!(second.revision(), &Revision::new(exact(second.source())));
        let Origin::Edited { changed, splices } = second.origin() else {
            panic!("an edit records its evidence");
        };
        assert_eq!(changed, &[Path::new(["const", "window_hours"])]);
        assert_eq!(splices.len(), 1);
        let same = second
            .apply(second.revision(), &[hours(72)])
            .expect("no-op");
        assert_eq!(same.revision(), second.revision());
        assert_eq!(same.parent(), Some(first.revision()));
    }

    #[test]
    fn a_stale_edit_never_overwrites_a_newer_revision() {
        // The holder keeps the current revision; a late change aimed at the first.
        let first = DocumentRevision::import(BASE, exact).expect("import");
        let head = first.apply(first.revision(), &[hours(72)]).expect("72");
        let late = head
            .apply(first.revision(), &[hours(96)])
            .expect_err("stale");
        assert!(
            matches!(&late, RevisionRefusal::Stale { base, current }
                if base == first.revision() && current == head.revision()),
            "{late}"
        );
        assert!(late.to_string().contains("was not applied"), "{late}");
        assert!(head.source().contains("window_hours: 72"));
        let replaced = head
            .replace(first.revision(), BASE.replace("48", "1"))
            .expect_err("a stale replacement is refused too");
        assert!(matches!(replaced, RevisionRefusal::Stale { .. }));
        let next = head
            .apply(head.revision(), &[hours(96)])
            .expect("on the head");
        assert_eq!(next.parent(), Some(head.revision()));
    }

    #[test]
    fn a_whole_source_replacement_claims_no_byte_evidence() {
        let first = DocumentRevision::import(BASE, exact).expect("import");
        let replaced = first
            .replace(first.revision(), BASE.replace("# the threshold\n", ""))
            .expect("replace");
        assert_eq!(replaced.origin(), &Origin::Replaced);
        assert_eq!(replaced.parent(), Some(first.revision()));
        let refused = first
            .replace(first.revision(), "nika: [")
            .expect_err("not a document");
        assert!(matches!(refused, RevisionRefusal::Document(_)), "{refused}");
    }
}
