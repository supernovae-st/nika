// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conformance cert (ADR-094 D3) — a claim the installer re-derives.

use serde::{Deserialize, Serialize};

use crate::hash::Blake3Hash;
use crate::manifest::validate_schema;
use crate::{ManifestError, PCK_SCHEMA};

/// The cert's headline verdict. Closed + `#[non_exhaustive]` — future
/// grades (e.g. a partial/waived class) are additive variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum CertVerdict {
    /// The static oracle passed clean.
    Pass,
    /// The static oracle found violations (the cert still ships — an
    /// HONEST fail is publishable; the installer sees it before landing).
    Fail,
}

/// The `nika check` static proof, carried WITH the artifact: what it
/// touches (effects), what it declares (permits), what it reads
/// (secrets), what it may spend — **as opaque claim strings**. `nika
/// verify` re-runs the real oracle locally (SkillFortify-class
/// install-time re-derivation); this type never validates semantics —
/// typed vocabularies live above L0 and must not drag the parser here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Cert {
    /// [`PCK_SCHEMA`] — parses on mismatch, `validate()` reports.
    pub schema: String,
    /// The manifest this cert certifies (its blake3 — D1 identity).
    pub manifest_hash: Blake3Hash,
    /// The certifying engine version (`nika --version` form).
    pub engine: String,
    /// The headline verdict.
    pub verdict: CertVerdict,
    /// Effect claims (e.g. `fs.write:./out/*` · `net.http:api.example.com`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<String>,
    /// The declared permits boundary, flattened to claim strings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permits: Vec<String>,
    /// Secret names the workflow reads (enumeration, never values).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets_read: Vec<String>,
    /// The static cost floor, exact-string USD (micro-USD grain — never a
    /// float; absent = the oracle priced nothing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_floor_usd: Option<String>,
}

impl Cert {
    /// Construct with the required fields; claims start empty (INV#19).
    #[must_use]
    pub fn new(manifest_hash: Blake3Hash, engine: impl Into<String>, verdict: CertVerdict) -> Self {
        Self {
            schema: PCK_SCHEMA.to_owned(),
            manifest_hash,
            engine: engine.into(),
            verdict,
            effects: Vec::new(),
            permits: Vec::new(),
            secrets_read: Vec::new(),
            cost_floor_usd: None,
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
    fn verdict_wire_forms_are_kebab_and_locked() {
        assert_eq!(
            serde_json::to_string(&CertVerdict::Pass).unwrap(),
            "\"pass\""
        );
        assert_eq!(
            serde_json::to_string(&CertVerdict::Fail).unwrap(),
            "\"fail\""
        );
        let v: CertVerdict = serde_json::from_str("\"pass\"").unwrap();
        assert_eq!(v, CertVerdict::Pass);
        // an unknown verdict is a hard error — the verdict is a trust
        // surface, not a router (contrast ArtifactKind's totality)
        assert!(serde_json::from_str::<CertVerdict>("\"maybe\"").is_err());
    }

    #[test]
    fn cert_round_trips_with_claims_and_omits_empty_ones() {
        let mut c = Cert::new(b3(), "nika 0.97.0", CertVerdict::Pass);
        let lean = serde_json::to_string(&c).unwrap();
        assert!(
            !lean.contains("effects"),
            "empty claim vecs stay off the wire"
        );
        c.effects = vec!["net.http:api.example.com".into()];
        c.cost_floor_usd = Some("0.000062".into());
        let back: Cert = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
        back.validate().unwrap();
    }

    #[test]
    fn validate_refuses_a_foreign_schema() {
        let mut c = Cert::new(b3(), "nika 0.97.0", CertVerdict::Pass);
        c.schema = "nika/pck@9".to_owned();
        assert_eq!(
            c.validate().unwrap_err(),
            ManifestError::SchemaUnsupported {
                got: "nika/pck@9".into()
            }
        );
    }

    #[test]
    fn cost_floor_is_an_exact_string_never_a_float() {
        let mut c = Cert::new(b3(), "nika 0.97.0", CertVerdict::Pass);
        c.cost_floor_usd = Some("0.000001".into());
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"0.000001\""), "string on the wire: {json}");
    }
}
