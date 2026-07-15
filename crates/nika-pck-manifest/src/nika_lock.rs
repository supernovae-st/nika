// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika.lock` — the single lock (spec `15-proof.md` §nika.lock · F7).
//!
//! One lockfile pins EVERYTHING a run resolves, by **digest**. It unifies the
//! prior manifest lock (`crate::lockfile`) and the pin-by-default rule into
//! one file — the LOCAL boundary of the same supply chain the gateway (12)
//! and the distribution work extend.
//!
//! ```yaml
//! lock_format: 1
//! providers:  { "anthropic/claude-…": { digest: "blake3:…" } }  # models pinned by digest
//! tools:      { "nika:fetch":         { digest: "blake3:…" } }  # builtin + MCP surface
//! registry:   { "acme/flows@1.2.0":   { digest: "blake3:…" } }  # every registry: ref
//! policy:     { … }   # the resolved policy decisions (spec 10)
//! model_select: { … } # `model: { select: {require, prefer} }` materialized
//! ```
//!
//! - **Pin by default**: a run resolves ONLY what the lock pins; an unpinned
//!   dependency is a refusal ([`NIKA_LOCK_001`]). Nothing floats.
//! - Generated (`nika lock`), never hand-authored — hand-editing a digest is
//!   a lie the check catches (the lock's own hash covers it · the integrity
//!   digest is computed where blake3 lives, the `crate::proof`-style
//!   self-digest pattern; L0 here stays pure — no hasher — so [`NikaLock::validate`]
//!   pins the DIGEST-REQUIRED + PIN-BY-DEFAULT laws, byte-parity with the
//!   reference `proof_core.validate_lock`, and the source-integrity self-hash
//!   is the higher-layer's owed).
//!
//! ## The second evaluator
//!
//! [`NikaLock::validate`] mirrors `proof_core.validate_lock` (spec 15 · the
//! reference model · 22 laws): wrong `lock_format` → refuse · a digest-less
//! entry → refuse · a resolved dependency the lock does not pin → refuse. The
//! four lock laws of `proof_core_selftest.py` are pinned in `tests`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The v1 lock format marker — the reference's `LOCK_FORMAT`.
pub const LOCK_FORMAT: u32 = 1;

/// The pin-by-default refusal code (spec 15 · canon.yaml `validation_error`).
pub const NIKA_LOCK_001: &str = "NIKA-LOCK-001";

/// One pinned dependency — a content digest is MANDATORY (pin by default ·
/// nothing floats). A missing `digest` key fails deserialization; an EMPTY
/// digest is caught by [`NikaLock::validate`] (both are the reference's
/// "a digest pin is required" refusal, at the two surfaces a typed lock has).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LockPin {
    /// The pinned content digest (`blake3:…` / `sha256:…`). Installs verify
    /// content against THIS, whatever any index or mutable tag says later.
    pub digest: String,
}

impl LockPin {
    /// Pin a resolved dependency to its content digest.
    #[must_use]
    pub fn new(digest: impl Into<String>) -> Self {
        Self {
            digest: digest.into(),
        }
    }
}

/// `nika.lock` — the single lock. Each pinnable section maps a resolved ref
/// to its digest pin; `policy` and `model_select` record the run's resolved
/// decisions (spec 10 · the model-select materialization) and are carried,
/// not digest-validated (the reference validates the three pinnable sections).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NikaLock {
    /// [`LOCK_FORMAT`] — a foreign value is a refusal ([`NikaLock::validate`]).
    pub lock_format: u32,
    /// Every model pinned by content digest.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub providers: BTreeMap<String, LockPin>,
    /// Every builtin + MCP surface version pinned by digest.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools: BTreeMap<String, LockPin>,
    /// Every `registry:` ref pinned owner/name@version + digest.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub registry: BTreeMap<String, LockPin>,
    /// The resolved policy decisions (spec 10) — recorded key→value
    /// resolutions, carried for the receipt fold, not part of the
    /// pin-by-default law (the richer policy-family shape lives at spec 10).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub policy: BTreeMap<String, String>,
    /// `model: { select: {require, prefer} }` materialized — the resolved
    /// model per selection, recorded (not digest-validated here).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_select: BTreeMap<String, String>,
}

/// A `nika.lock` refusal (spec 15 · [`NIKA_LOCK_001`]). A plain data carrier,
/// NOT a `thiserror` enum — it never crosses a `NikaErrorCode` boundary (the
/// same posture as `nika-builtin`'s `decide.rs` `DecideError`, so the L0 crate
/// keeps its single allowlisted `ManifestError`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LockRefusal {
    /// Always [`NIKA_LOCK_001`] (one code · the lock namespace).
    pub code: &'static str,
    /// The human-readable law that refused.
    pub message: String,
}

impl LockRefusal {
    fn new(message: impl Into<String>) -> Self {
        Self {
            code: NIKA_LOCK_001,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for LockRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl NikaLock {
    /// An empty lock at the current format (INV-019 · `#[non_exhaustive]`
    /// structs ship a constructor).
    #[must_use]
    pub fn new() -> Self {
        Self {
            lock_format: LOCK_FORMAT,
            providers: BTreeMap::new(),
            tools: BTreeMap::new(),
            registry: BTreeMap::new(),
            policy: BTreeMap::new(),
            model_select: BTreeMap::new(),
        }
    }

    /// The set of refs this lock pins (providers ∪ tools ∪ registry) — the
    /// pinnable sections the reference builds its `pinned` set from.
    #[must_use]
    pub fn pinned(&self) -> BTreeSet<&str> {
        self.providers
            .keys()
            .chain(self.tools.keys())
            .chain(self.registry.keys())
            .map(String::as_str)
            .collect()
    }

    /// Validate the lock against the set of dependency refs a run actually
    /// `resolved` (spec 15 · mirror of `proof_core.validate_lock`):
    ///
    /// 1. `lock_format` must be [`LOCK_FORMAT`] (a foreign version is refused);
    /// 2. every pinnable entry MUST carry a non-empty digest (pin by default);
    /// 3. every resolved dependency MUST be pinned — an unpinned resolution is
    ///    a refusal ([`NIKA_LOCK_001`]), never a warning. Nothing floats.
    ///
    /// # Errors
    ///
    /// [`LockRefusal`] ([`NIKA_LOCK_001`]) on any of the three laws.
    pub fn validate(&self, resolved: &BTreeSet<String>) -> Result<(), LockRefusal> {
        if self.lock_format != LOCK_FORMAT {
            return Err(LockRefusal::new(format!(
                "nika.lock · lock_format must be {LOCK_FORMAT} (got {})",
                self.lock_format
            )));
        }
        for (section, block) in [
            ("providers", &self.providers),
            ("tools", &self.tools),
            ("registry", &self.registry),
        ] {
            for (reference, pin) in block {
                if pin.digest.is_empty() {
                    return Err(LockRefusal::new(format!(
                        "nika.lock.{section}.{reference} · a digest pin is required \
                         (pin by default · nothing floats · {NIKA_LOCK_001})"
                    )));
                }
            }
        }
        let pinned = self.pinned();
        let unpinned: Vec<&str> = resolved
            .iter()
            .map(String::as_str)
            .filter(|r| !pinned.contains(r))
            .collect();
        if !unpinned.is_empty() {
            return Err(LockRefusal::new(format!(
                "nika.lock · resolved dependencies not pinned: {unpinned:?} \
                 (pin by default · {NIKA_LOCK_001})"
            )));
        }
        Ok(())
    }
}

impl Default for NikaLock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn resolved(refs: &[&str]) -> BTreeSet<String> {
        refs.iter().map(|s| (*s).to_owned()).collect()
    }

    fn good_lock() -> NikaLock {
        let mut lock = NikaLock::new();
        lock.providers
            .insert("anthropic/claude".to_owned(), LockPin::new("blake3:aa"));
        lock.tools
            .insert("nika:fetch".to_owned(), LockPin::new("blake3:bb"));
        lock
    }

    // ── the four reference lock laws (proof_core_selftest.py) ────────────

    #[test]
    fn a_fully_pinned_lock_validates() {
        good_lock()
            .validate(&resolved(&["anthropic/claude", "nika:fetch"]))
            .expect("a fully-pinned lock validates");
    }

    #[test]
    fn an_unpinned_resolved_dependency_is_refused() {
        let err = good_lock()
            .validate(&resolved(&["anthropic/claude", "nika:fetch", "mcp:x/y"]))
            .expect_err("an unpinned resolved dependency is refused");
        assert_eq!(err.code, NIKA_LOCK_001);
        assert!(err.message.contains("mcp:x/y"), "the culprit is named");
    }

    #[test]
    fn a_digest_less_lock_entry_is_refused_at_validate() {
        // An empty digest is the reference's "a digest pin is required".
        let mut lock = NikaLock::new();
        lock.providers.insert("p".to_owned(), LockPin::new(""));
        let err = lock
            .validate(&resolved(&["p"]))
            .expect_err("an empty digest is refused");
        assert_eq!(err.code, NIKA_LOCK_001);
    }

    #[test]
    fn a_digest_less_lock_entry_is_refused_at_parse() {
        // A MISSING digest key is the same law at the deserialize boundary —
        // `{"providers":{"p":{}}}` cannot round-trip into a typed pin.
        let json = r#"{"lock_format":1,"providers":{"p":{}}}"#;
        assert!(
            serde_json::from_str::<NikaLock>(json).is_err(),
            "a digest-less entry does not deserialize (pin by default)"
        );
    }

    #[test]
    fn a_wrong_lock_format_is_refused() {
        let mut lock = NikaLock::new();
        lock.lock_format = 9;
        let err = lock
            .validate(&resolved(&[]))
            .expect_err("a wrong lock_format is refused");
        assert_eq!(err.code, NIKA_LOCK_001);
        assert!(err.message.contains("lock_format"));
    }

    // ── generation shape · round-trips · nothing floats ─────────────────

    #[test]
    fn the_lock_round_trips_json() {
        let lock = good_lock();
        let json = serde_json::to_string(&lock).unwrap();
        let back: NikaLock = serde_json::from_str(&json).unwrap();
        assert_eq!(back, lock);
    }

    #[test]
    fn an_empty_lock_with_no_resolutions_validates() {
        // The degenerate case: nothing resolved, nothing to pin.
        NikaLock::new()
            .validate(&resolved(&[]))
            .expect("an empty lock over an empty resolution set is valid");
    }

    #[test]
    fn pinned_unions_the_three_pinnable_sections() {
        let mut lock = good_lock();
        lock.registry
            .insert("acme/flows@1".to_owned(), LockPin::new("blake3:cc"));
        // policy / model_select are recorded, never pinned.
        lock.policy.insert("net".to_owned(), "deny".to_owned());
        let pinned = lock.pinned();
        assert!(pinned.contains("anthropic/claude"));
        assert!(pinned.contains("nika:fetch"));
        assert!(pinned.contains("acme/flows@1"));
        assert!(!pinned.contains("net"), "policy is recorded, not pinned");
    }
}
