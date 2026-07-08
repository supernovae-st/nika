// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The shared artifact's manifest (`nika/pck@1` · FCI-003).

use serde::{Deserialize, Serialize};

use crate::hash::{Blake3Hash, Sha256Hash};
use crate::kind::ArtifactKind;
use crate::{ManifestError, PCK_SCHEMA};

/// One content file: repo-relative path + its sha256 (per-file integrity —
/// the `nika-pack` embedded-pack precedent, same row shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FileEntry {
    /// Artifact-relative path (forward slashes on the wire).
    pub path: String,
    /// The file's sha256.
    pub sha256: Sha256Hash,
}

impl FileEntry {
    /// Construct a row (INV#19).
    #[must_use]
    pub fn new(path: impl Into<String>, sha256: Sha256Hash) -> Self {
        Self {
            path: path.into(),
            sha256,
        }
    }
}

/// The manifest an author publishes with an artifact. Signed DETACHED
/// (minisign · ADR-094 D3) — deliberately NO signature field in the data:
/// the signature covers these bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Manifest {
    /// [`PCK_SCHEMA`] — parses on mismatch, `validate()` reports (FCI-003).
    pub schema: String,
    /// Human name (display only — NEVER identity · D1: the hash is).
    pub name: String,
    /// The artifact's own version string (publisher-defined semantics).
    pub version: String,
    /// The D4 class this artifact routes as.
    pub kind: ArtifactKind,
    /// One-line description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Author attribution (display; trust is the detached signature).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// SPDX license id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Per-file integrity rows.
    pub files: Vec<FileEntry>,
    /// The artifact's identity: its blake3 content hash (D1/D2 — computed
    /// by `nika-blob`, carried here).
    pub content_hash: Blake3Hash,
}

impl Manifest {
    /// Construct with the required fields; optional attribution starts
    /// empty (INV#19 — literal construction is sealed by `#[non_exhaustive]`).
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        kind: ArtifactKind,
        files: Vec<FileEntry>,
        content_hash: Blake3Hash,
    ) -> Self {
        Self {
            schema: PCK_SCHEMA.to_owned(),
            name: name.into(),
            version: version.into(),
            kind,
            description: None,
            author: None,
            license: None,
            files,
            content_hash,
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

/// The one shared schema check (manifest · cert · lockfile).
pub(crate) fn validate_schema(schema: &str) -> Result<(), ManifestError> {
    if schema == PCK_SCHEMA {
        Ok(())
    } else {
        Err(ManifestError::SchemaUnsupported {
            got: schema.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b3() -> Blake3Hash {
        Blake3Hash::new("a3f5b8c9d0e1f2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2b").unwrap()
    }
    fn s2() -> Sha256Hash {
        Sha256Hash::new("00f5b8c9d0e1f2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f8091a2b").unwrap()
    }

    fn sample() -> Manifest {
        Manifest::new(
            "pr-risk-review",
            "1.0.0",
            ArtifactKind::Workflow,
            vec![FileEntry::new("pr-risk-review.nika.yaml", s2())],
            b3(),
        )
    }

    #[test]
    fn new_stamps_the_schema_and_validates_clean() {
        let m = sample();
        assert_eq!(m.schema, "nika/pck@1");
        m.validate().unwrap();
    }

    #[test]
    fn a_future_schema_parses_then_validate_reports() {
        let mut m = sample();
        m.schema = "nika/pck@2".to_owned();
        let json = serde_json::to_string(&m).unwrap();
        let back: Manifest = serde_json::from_str(&json).unwrap(); // parses fine
        assert_eq!(
            back.validate().unwrap_err(),
            ManifestError::SchemaUnsupported {
                got: "nika/pck@2".into()
            }
        );
    }

    #[test]
    fn toml_round_trip_the_documented_wire_format() {
        // FCI-003: `nika/pck@1` is a TOML format — prove the shape holds.
        let m = sample();
        let toml = toml_convert::to_string(&m).unwrap();
        assert!(toml.contains("schema = \"nika/pck@1\""), "{toml}");
        assert!(toml.contains("kind = \"workflow\""), "{toml}");
        let back: Manifest = toml_convert::from_str(&toml).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn json_round_trip_and_optional_fields_omitted() {
        let m = sample();
        let json = serde_json::to_string(&m).unwrap();
        assert!(
            !json.contains("description"),
            "absent options stay off the wire"
        );
        let back: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }
}
