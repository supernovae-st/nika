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

/// One sanctioned destination for a secret — an entry of a secret's
/// `egress:` list (spec `01-envelope.md` §secrets · declassification).
///
/// The DECENTRALIZED-LABEL-MODEL principle (Myers & Liskov · *A
/// Decentralized Model for Information Flow Control* · SOSP 1997) ·
/// declassification is the OWNER's act, co-located with the data — so an
/// egress clause lives on the secret it sanctions, never on the sink.
/// A rule names the SPECIFIC sink (`to:`) and, for network sinks, the
/// destination host — a literal `host:` OR `host_from_self: true` when
/// the secret value IS the URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EgressRule {
    /// The sink this rule sanctions — a tool id (`nika:fetch` ·
    /// `nika:notify` · `mcp:<server>/<tool>`), the literal `exec`, or the
    /// provider-egress sinks `infer` / `agent` (BUG#3 · a secret sent in a
    /// prompt). SPECIFIC by design · a clearance for `nika:fetch` never
    /// authorizes `exec`, and `infer` never authorizes `agent` (no
    /// cross-sink laundering).
    pub to: String,
    /// A static-literal destination host (network sinks) — the egress is
    /// sanctioned only when the sink's destination arg is exactly this
    /// host. `None` when the rule carries no host clause (the sink has no
    /// addressable host · e.g. an `exec` egress) OR when `host_from_self`
    /// is set.
    pub host: Option<String>,
    /// The secret value IS the destination URL (the host is unknown
    /// statically · a webhook secret). Sanctions only when the sink's
    /// destination arg is exactly `${{ secrets.<this> }}` AND no other
    /// secret co-occurs in the same effect (the non-occlusion guard).
    pub host_from_self: bool,
}

impl EgressRule {
    /// A sink-only egress rule (no host clause) — `{ to }`.
    #[must_use]
    pub fn to(to: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            host: None,
            host_from_self: false,
        }
    }

    /// A network egress rule bound to a literal `host:`.
    #[must_use]
    pub fn to_host(to: impl Into<String>, host: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            host: Some(host.into()),
            host_from_self: false,
        }
    }

    /// A network egress rule where the secret value IS the URL
    /// (`host_from_self: true`).
    #[must_use]
    pub fn to_self_host(to: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            host: None,
            host_from_self: true,
        }
    }
}

/// A secret reference — `{ source, key }` + an optional `egress:` policy.
///
/// The engine masks every resolved secret value in logs, traces, and
/// journal events. An inline literal (`api_key: "sk-..."`) is a parse
/// error — the parser enforces that, not this type.
///
/// **Egress (declassification).** `egress` is the secret's sanctioned-
/// destination list. **Empty (the default) = NO egress** — a secret
/// reaching any `exec`/`invoke` effect OR an `infer`/`agent` PROMPT is a
/// blocking leak. A non-empty list sanctions exactly the named sinks (see
/// [`EgressRule`] · `to: "infer"` / `to: "agent"` for the provider-egress
/// sink · BUG#3). Only the infer/agent OUTPUT keeps the carve-out (the model
/// response never taints downstream · ADR-092).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SecretRef {
    /// Which store to resolve against (default `vault`).
    pub source: SecretSource,
    /// The store-specific key (vault path · env var name · file path).
    pub key: String,
    /// The sanctioned-egress list (default-deny · empty = no egress).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub egress: Vec<EgressRule>,
}

impl SecretRef {
    /// Create a secret reference with no sanctioned egress (default-deny).
    #[must_use]
    pub fn new(source: SecretSource, key: impl Into<String>) -> Self {
        Self {
            source,
            key: key.into(),
            egress: Vec::new(),
        }
    }

    /// Set the sanctioned-egress list (builder form).
    #[must_use]
    pub fn with_egress(mut self, egress: Vec<EgressRule>) -> Self {
        self.egress = egress;
        self
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

    #[test]
    fn new_secret_has_no_egress_by_default() {
        // default-deny: a freshly-declared secret sanctions nothing.
        let s = SecretRef::new(SecretSource::Vault, "k");
        assert!(s.egress.is_empty());
    }

    #[test]
    fn egress_rule_constructors_set_the_three_shapes() {
        let sink_only = EgressRule::to("exec");
        assert_eq!(sink_only.to, "exec");
        assert_eq!(sink_only.host, None);
        assert!(!sink_only.host_from_self);

        let literal = EgressRule::to_host("nika:fetch", "api.stripe.com");
        assert_eq!(literal.to, "nika:fetch");
        assert_eq!(literal.host.as_deref(), Some("api.stripe.com"));
        assert!(!literal.host_from_self);

        let self_host = EgressRule::to_self_host("nika:notify");
        assert_eq!(self_host.to, "nika:notify");
        assert_eq!(self_host.host, None);
        assert!(self_host.host_from_self);
    }

    #[test]
    fn with_egress_attaches_the_list() {
        let s = SecretRef::new(SecretSource::Env, "WEBHOOK")
            .with_egress(vec![EgressRule::to_self_host("nika:notify")]);
        assert_eq!(s.egress.len(), 1);
        assert!(s.egress[0].host_from_self);
    }

    #[test]
    fn empty_egress_is_omitted_from_serialization() {
        // backward-compat: a secret without egress serializes exactly as
        // before (no `egress` key) — existing snapshots/JSON stay stable.
        let s = SecretRef::new(SecretSource::Vault, "k");
        let json = serde_json::to_string(&s).expect("serialize");
        assert!(!json.contains("egress"), "empty egress omitted: {json}");
    }

    #[test]
    fn egress_round_trips_through_serde() {
        let s = SecretRef::new(SecretSource::Env, "WEBHOOK").with_egress(vec![
            EgressRule::to_self_host("nika:notify"),
            EgressRule::to_host("nika:fetch", "api.stripe.com"),
        ]);
        let json = serde_json::to_string(&s).expect("serialize");
        let back: SecretRef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, back);
    }
}
