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
