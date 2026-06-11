// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Secret references — the envelope `secrets:` block.
//!
//! Per spec `01-envelope.md` §secrets · « A secret is always a **reference
//! to a store** — never an inline literal. » The `source` field is a closed
//! enum, and the entry shape is **discriminated by it** (spec 01 ·
//! `vault`/`env` require `key:` · `file` requires `path:`) ·
//!
//! | `source` | YAML field | the reference means |
//! |---|---|---|
//! | `vault` (default) | `key:` | path in the local `nika-vault` |
//! | `env` | `key:` | name of an OS environment variable |
//! | `file` | `path:` | path to a file holding the value |
//!
//! The model stores the reference uniformly in [`SecretRef::key`] — the
//! parser owns the field-name discrimination (a `file` entry written
//! with `key:`, or a `vault`/`env` entry written with `path:`, is a
//! parse error).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Which store a secret reference resolves against.
///
/// Closed enum at v0.1 (spec `01-envelope.md` §secrets) — `vault` is the
/// sovereign default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum SecretSource {
    /// Path in the local `nika-vault` (the sovereign default).
    #[default]
    Vault,
    /// Name of an OS environment variable (12-factor / CI secrets) —
    /// still masked, unlike the plain `env:` block.
    Env,
    /// Path to a file holding the value (Docker / k8s mounted secrets).
    File,
}

impl SecretSource {
    /// Parse the YAML `source:` scalar. Returns `None` for anything
    /// outside the closed v0.1 enum.
    #[must_use]
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "vault" => Some(Self::Vault),
            "env" => Some(Self::Env),
            "file" => Some(Self::File),
            _ => None,
        }
    }
}

impl fmt::Display for SecretSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vault => write!(f, "vault"),
            Self::Env => write!(f, "env"),
            Self::File => write!(f, "file"),
        }
    }
}

/// A secret reference — `{ source, key }`.
///
/// The engine masks every resolved secret value in logs, traces, and
/// journal events. An inline literal (`api_key: "sk-..."`) is a parse
/// error — the parser enforces that, not this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SecretRef {
    /// Which store to resolve against (default `vault`).
    pub source: SecretSource,
    /// The store-specific key (vault path · env var name · file path).
    pub key: String,
}

impl SecretRef {
    /// Create a secret reference.
    #[must_use]
    pub fn new(source: SecretSource, key: impl Into<String>) -> Self {
        Self {
            source,
            key: key.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_source_is_vault() {
        assert_eq!(SecretSource::default(), SecretSource::Vault);
    }

    #[test]
    fn from_str_closed_enum() {
        assert_eq!(
            SecretSource::from_str_opt("vault"),
            Some(SecretSource::Vault)
        );
        assert_eq!(SecretSource::from_str_opt("env"), Some(SecretSource::Env));
        assert_eq!(SecretSource::from_str_opt("file"), Some(SecretSource::File));
        assert_eq!(SecretSource::from_str_opt("aws"), None);
        assert_eq!(SecretSource::from_str_opt("Vault"), None); // case-sensitive
    }

    #[test]
    fn new_carries_fields() {
        let s = SecretRef::new(SecretSource::Env, "GITHUB_TOKEN");
        assert_eq!(s.source, SecretSource::Env);
        assert_eq!(s.key, "GITHUB_TOKEN");
    }

    #[test]
    fn display() {
        assert_eq!(SecretSource::Vault.to_string(), "vault");
        assert_eq!(SecretSource::Env.to_string(), "env");
        assert_eq!(SecretSource::File.to_string(), "file");
    }
}
