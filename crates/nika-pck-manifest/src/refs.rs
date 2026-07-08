// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The decentralized package ref (ADR-094 D1) — struct-only, no grammar.

use serde::{Deserialize, Serialize};

/// Go-module-style ref: the hosting platform IS the namespace — no global
/// name table to squat. Struct-only by design: a canonical string
/// syntax (display/parse) is an ADR-094 follow-up decision the L1 registry
/// needs; freezing a grammar here would outrun the ratified shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PackageRef {
    /// The git URL that owns the artifact (`https://…` · `git@…`).
    pub url: String,
    /// Repo-relative path to the artifact dir (None = repo root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The version selector (a tag · publisher-defined semantics).
    pub version: String,
}

impl PackageRef {
    /// Construct from the resolved parts (INV#19).
    #[must_use]
    pub fn new(url: impl Into<String>, path: Option<String>, version: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            path,
            version: version.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trips_and_omits_absent_path() {
        let r = PackageRef::new("https://github.com/acme/flows", None, "v1.2.0");
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(
            json, r#"{"url":"https://github.com/acme/flows","version":"v1.2.0"}"#,
            "no `path` key when absent — lean TOML/JSON rows"
        );
        let back: PackageRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);

        let with = PackageRef::new("https://sr.ht/x/y", Some("packs/a".into()), "2026.07");
        let back: PackageRef =
            serde_json::from_str(&serde_json::to_string(&with).unwrap()).unwrap();
        assert_eq!(back.path.as_deref(), Some("packs/a"));
    }

    #[test]
    fn unknown_fields_are_ignored_additive_forever() {
        let json = r#"{"url":"u","version":"v","future_field":123}"#;
        let r: PackageRef = serde_json::from_str(json).unwrap();
        assert_eq!(r.url, "u");
    }
}
