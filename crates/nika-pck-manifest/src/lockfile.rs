// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Hash pins that survive any index dying (ADR-094 D1 · constraint 4).

use serde::{Deserialize, Serialize};

use crate::hash::Blake3Hash;
use crate::manifest::validate_schema;
use crate::refs::PackageRef;
use crate::{ManifestError, PCK_SCHEMA};

/// One pinned artifact: where it came from + what it MUST hash to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LockEntry {
    /// The ref that was resolved (provenance — display + re-resolve).
    #[serde(rename = "ref")]
    pub reference: PackageRef,
    /// The pinned identity: installs verify content against THIS, whatever
    /// any index or remote says later (mutable tags cannot bite).
    pub content_hash: Blake3Hash,
}

impl LockEntry {
    /// Pin a resolved ref to its content hash (INV#19).
    #[must_use]
    pub fn new(reference: PackageRef, content_hash: Blake3Hash) -> Self {
        Self {
            reference,
            content_hash,
        }
    }
}

/// The lockfile: the project's installed set as pins. Entry order is the
/// PRODUCER's (the L2 orchestrator sorts by ref for deterministic diffs —
/// a data-shape concern above L0, documented here so no consumer assumes
/// this type sorts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Lockfile {
    /// [`PCK_SCHEMA`] — parses on mismatch, `validate()` reports.
    pub schema: String,
    /// The pins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<LockEntry>,
}

impl Lockfile {
    /// An empty lockfile at the current schema (INV#19).
    #[must_use]
    pub fn new(entries: Vec<LockEntry>) -> Self {
        Self {
            schema: PCK_SCHEMA.to_owned(),
            entries,
        }
    }

    /// Structural validation: the schema marker is one this crate speaks.
    ///
    /// # Errors
    /// [`ManifestError::SchemaUnsupported`] when `schema` ≠ [`PCK_SCHEMA`].
    pub fn validate(&self) -> Result<(), ManifestError> {
        validate_schema(&self.schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b3() -> Blake3Hash {
        Blake3Hash::new("a3f5b8c9d0e1f2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2b").unwrap()
    }

    #[test]
    fn wire_key_is_ref_and_round_trips_toml_and_json() {
        let lf = Lockfile::new(vec![LockEntry::new(
            PackageRef::new("https://github.com/acme/flows", None, "v1"),
            b3(),
        )]);
        let json = serde_json::to_string(&lf).unwrap();
        assert!(json.contains("\"ref\":{"), "the wire key is `ref`: {json}");
        let back: Lockfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, lf);
        let toml = toml_convert::to_string(&lf).unwrap();
        let back: Lockfile = toml_convert::from_str(&toml).unwrap();
        assert_eq!(back, lf);
        back.validate().unwrap();
    }

    #[test]
    fn validate_refuses_a_foreign_schema() {
        let mut lf = Lockfile::new(Vec::new());
        lf.schema = "nika/lock@1".to_owned();
        assert_eq!(
            lf.validate().unwrap_err(),
            ManifestError::SchemaUnsupported {
                got: "nika/lock@1".into()
            }
        );
    }

    #[test]
    fn empty_lockfile_is_lean_and_valid() {
        let lf = Lockfile::new(Vec::new());
        assert_eq!(
            serde_json::to_string(&lf).unwrap(),
            r#"{"schema":"nika/pck@1"}"#
        );
        let back: Lockfile = serde_json::from_str(r#"{"schema":"nika/pck@1"}"#).unwrap();
        assert_eq!(back.entries.len(), 0, "absent entries deserialize as empty");
        back.validate().unwrap();
    }
}
