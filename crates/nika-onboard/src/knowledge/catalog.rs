// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The admitted release as a catalogue (`nika_compile_seats::foundry`): its checked blocks
//! resolved as executable components by id and the version or digest a reference pins, their
//! admitted bytes re-verified against the pin; its entries enumerated for whole-catalog reach,
//! each rendered in full exactly as a pack presents it. Under r1 the entries are its blocks and
//! patterns. Under r2 they are every row of every kind, each in its role: a counterexample is a
//! boundary to read, never a component to resolve. The sources are no choice: each entry carries
//! the sources it derives from.

use nika_compile_seats::foundry::component::pinned;
use nika_compile_seats::foundry::{Component, ComponentCatalog, ComponentRef, Release, Unresolved};
use serde_json::Value;

use super::{Composition, Snapshot};
use crate::compile::KnowledgeReference;

/// The row files a profile r1 catalogue enumerates: the kinds a pack presents from a release.
const ENTRIES: [&str; 2] = ["blocks", "patterns"];

/// The release as the catalogue of one request that holds out an evaluation corpus: no example
/// or counterexample of that corpus is listed, referenced or resolved, so the holdout stays out
/// of the whole-catalog reach as it stays out of the pack ([`Snapshot::pack`]). An evaluation
/// boundary, never a restriction on what a person creates.
#[derive(Clone, Copy, Debug)]
pub struct Catalogue<'a> {
    snapshot: &'a Snapshot,
    holdout: Option<&'a str>,
}

impl Snapshot {
    /// The release as the catalogue of a request whose pack holds out `exclude_corpus`.
    #[must_use]
    pub const fn catalogue<'a>(&'a self, exclude_corpus: Option<&'a str>) -> Catalogue<'a> {
        Catalogue {
            snapshot: self,
            holdout: exclude_corpus,
        }
    }
}

impl Catalogue<'_> {
    /// Whether the row `id` names belongs to the held-out corpus.
    fn held_out(&self, id: &str) -> bool {
        let corpus = |row: &Value| row["corpus"].as_str().map(str::to_owned);
        self.holdout.is_some_and(|holdout| {
            (self.snapshot.row(id)).is_some_and(|row| corpus(row).as_deref() == Some(holdout))
        })
    }
}

impl ComponentCatalog for Catalogue<'_> {
    fn release(&self) -> Release {
        self.snapshot.release()
    }

    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
        if self.held_out(&reference.id) {
            return Err(Unresolved::Unknown(reference.id.clone()));
        }
        self.snapshot.resolve(reference)
    }

    fn entries(&self) -> Vec<Value> {
        (self.snapshot.entries().into_iter())
            .filter(|row| !self.held_out(row["id"].as_str().unwrap_or_default()))
            .collect()
    }

    fn reference(&self, id: &str) -> Option<KnowledgeReference> {
        (!self.held_out(id))
            .then(|| self.snapshot.reference(id))
            .flatten()
    }
}

impl ComponentCatalog for Snapshot {
    fn release(&self) -> Release {
        let version = self.version().unwrap_or_default();
        Release::new(version, &self.manifest_sha256, self.admission)
    }

    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
        reference.block_name()?;
        let release = self.release();
        pinned(reference, &release)?;
        let unknown = || Unresolved::Unknown(reference.id.clone());
        let row = self.row(&reference.id).ok_or_else(unknown)?;
        let file = row.get("file").and_then(Value::as_str).unwrap_or_default();
        Component::from_row(release, row, self.files.get(file).map(Vec::as_slice))
    }

    fn entries(&self) -> Vec<Value> {
        if self.whole_ontology() {
            return (Self::kinds().iter())
                .filter(|kind| kind.role() != "provenance")
                .flat_map(|kind| self.rows(kind.stem()).iter().cloned())
                .collect();
        }
        (ENTRIES.iter())
            .flat_map(|kind| self.rows(kind).iter().cloned())
            .collect()
    }

    fn reference(&self, id: &str) -> Option<KnowledgeReference> {
        if self.whole_ontology() {
            let kind = self.row(id)?["kind"].as_str()?;
            let text =
                (kind != "source_artifact").then(|| self.entry(id, &mut Composition::new("")));
            return Some(KnowledgeReference {
                kind: kind.to_owned(),
                id: id.to_owned(),
                text: text?.ok()?,
            });
        }
        let kind = id.split(':').next()?;
        let text = match kind {
            "block" => self.block_text(id, &mut Composition::new("")),
            "pattern" => self.pattern_text(id),
            _ => return None,
        };
        Some(KnowledgeReference {
            kind: kind.to_owned(),
            id: id.to_owned(),
            text: text.ok()?,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod r2_tests;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
