// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The registry types — the parsed shape of `nika.yaml`.
//!
//! Discipline (the forward-compat invariants): the wire tag is `nika:
//! v1`, frozen (FCI-003) · public-field structs are `#[non_exhaustive]`
//! (FCI-016) · no `HashMap` anywhere — the workspace lint
//! `iter_over_hash_type = "deny"` guards a future signature's
//! determinism, and this crate simply holds no maps · no `Vec` in
//! public returns (FCI-014) — accessors hand out slices or iterators.

use serde::Deserialize;

use crate::cron::CronSpec;

/// The one project file (`nika.yaml`) — round 1 carries `ceiling` +
/// `arm:` only; `traces:`/`registry:` wait for a measured lack and are
/// refused by name at validation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ArmRegistry {
    /// The schema tag — frozen at `v1` (the same freeze law as the
    /// workflow envelope: touching it is the project's heaviest move).
    pub nika: String,
    /// The project-wide run ceiling (USD) — a default every beat's
    /// `plafond` may exceed EXPLICITLY: the hierarchy stays visible.
    #[serde(default)]
    pub ceiling: Option<f64>,
    /// The armed beats (`arm:`).
    #[serde(default, rename = "arm")]
    beats: Vec<Beat>,
    /// Round-2 key — presence-detected, refused by name at validation.
    #[serde(default)]
    pub(crate) traces: Option<serde::de::IgnoredAny>,
    /// Round-2 key — presence-detected, refused by name at validation.
    #[serde(default)]
    pub(crate) registry: Option<serde::de::IgnoredAny>,
}

impl ArmRegistry {
    /// The wire tag this parser accepts — frozen forever (FCI-003).
    pub const SCHEMA: &'static str = "v1";

    /// The armed beats, in file order.
    pub fn beats(&self) -> impl Iterator<Item = &Beat> {
        self.beats.iter()
    }

    /// How many beats are armed.
    #[must_use]
    pub fn beat_count(&self) -> usize {
        self.beats.len()
    }
}

/// One armed beat — every law-required field rides as an `Option` so
/// the VALIDATOR (not serde) refuses its absence with the law's reason
/// and its fix, instead of a bare "missing field".
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Beat {
    /// The workflow to fire — a repo-relative `*.nika.yaml` path. Its
    /// EXISTENCE is judged at the L4 edge (this crate never touches a
    /// filesystem); the path's SHAPE is judged here.
    pub workflow: String,
    /// The cadence expression — both forms, the zone inside.
    pub cadence: String,
    /// Deployment locus — `local` (default) | `cloud`.
    #[serde(default, rename = "où")]
    pub ou: Option<Locus>,
    /// Per-tick ceiling (USD) — REQUIRED, no default (the pay law).
    #[serde(default)]
    pub plafond: Option<f64>,
    /// Missed-run policy — REQUIRED, no default (the run-missed law).
    #[serde(default, rename = "manqué")]
    pub manque: Option<MissPolicy>,
    /// Overlap policy — the safe `sauter` applies when absent (law ⑥).
    #[serde(default)]
    pub chevauchement: Option<Overlap>,
    /// After an overlap skip — `prochain-créneau` applies when absent,
    /// so a slow beat never becomes a tight loop (the
    /// `DeferReactivation` scar, closed by the default).
    #[serde(default, rename = "après_saut")]
    pub apres_saut: Option<AfterSkip>,
    /// Declared INTENTION (`actif: false` = suspended) — runtime state
    /// never lives in this file (it changes by itself, and what changes
    /// by itself is never written in what a human re-reads).
    #[serde(default)]
    pub actif: Option<bool>,
    /// Why the beat is suspended — required when `actif: false`.
    #[serde(default)]
    pub raison: Option<String>,
    /// Suspension expiry (ISO date) — required when `actif: false`.
    #[serde(default, rename = "jusqu_au")]
    pub jusqu_au: Option<String>,
    /// (m,k)-firm skip tolerance (`3/4`) — beyond it a skip is an
    /// OUTAGE, not a skip.
    #[serde(default, rename = "tolérance")]
    pub tolerance: Option<String>,
    /// Deterministic jitter derived from the path — `hash` only.
    #[serde(default, rename = "décalage")]
    pub decalage: Option<String>,
    /// Declares the human (N3 — proves NOTHING: the machine's key is
    /// what authorizes; a merge arms nothing).
    #[serde(default)]
    pub par: Option<String>,
    /// Refused in round 1: signature verification is ②'s (`serve`) —
    /// we claim nothing we cannot prove.
    #[serde(default)]
    pub(crate) signature: Option<serde::de::IgnoredAny>,
    /// Refused in round 1: the per-window quota waits a measured lack.
    #[serde(default)]
    pub(crate) budget: Option<serde::de::IgnoredAny>,
}

impl Beat {
    /// The effective locus (`où:` absent ⇒ `local` — the safe default).
    #[must_use]
    pub fn locus(&self) -> Locus {
        self.ou.unwrap_or(Locus::Local)
    }

    /// The effective overlap policy (absent ⇒ `sauter` — law ⑥).
    #[must_use]
    pub fn overlap(&self) -> Overlap {
        self.chevauchement.unwrap_or(Overlap::Sauter)
    }

    /// The effective after-skip policy (absent ⇒ `prochain-créneau` —
    /// the tight loop is impossible by default).
    #[must_use]
    pub fn after_skip(&self) -> AfterSkip {
        self.apres_saut.unwrap_or(AfterSkip::ProchainCreneau)
    }

    /// The declared intention (absent ⇒ armed).
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.actif.unwrap_or(true)
    }
}

/// Deployment locus — `où:` (`local | cloud`; moving is a one-word
/// diff · "le cloud exécute, le calendrier demeure à toi").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum Locus {
    /// This machine (launchd today, `serve` at ②).
    #[serde(rename = "local")]
    Local,
    /// The paid cloud (③).
    #[serde(rename = "cloud")]
    Cloud,
}

/// The missed-run policy — `manqué:` (OBLIGATORY, no default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum MissPolicy {
    /// Fire every missed slot, oldest first.
    #[serde(rename = "rattraper")]
    Rattraper,
    /// Fire ONE catch-up for the whole silence.
    #[serde(rename = "rattraper-une-fois")]
    RattraperUneFois,
    /// Never catch up — a skip is an EVENT, never an execution.
    #[serde(rename = "sauter")]
    Sauter,
}

/// The overlap policy — `chevauchement:` (closed enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum Overlap {
    /// Skip the new tick while one runs (the default — the safe value).
    #[serde(rename = "sauter")]
    Sauter,
    /// Queue the new tick.
    #[serde(rename = "file")]
    File,
    /// Replace the running tick.
    #[serde(rename = "remplacer")]
    Remplacer,
}

/// What happens after an overlap skip — `après_saut:`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum AfterSkip {
    /// Wait for the next scheduled slot (the default).
    #[serde(rename = "prochain-créneau")]
    ProchainCreneau,
    /// Re-fire as soon as the running tick completes.
    #[serde(rename = "à-complétion")]
    ACompletion,
}

/// A beat's cadence, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Cadence {
    /// `on-webhook` — event beats share this registry (§5, q5).
    Webhook,
    /// A 5-field cron over an explicit IANA zone — the only scheduled
    /// form this grammar accepts (the zone is IN the expression).
    Cron {
        /// The IANA zone name (`Europe/Paris`), resolved against the
        /// EMBEDDED tzdb at computation time — never the host's.
        tz: String,
        /// The five parsed fields.
        spec: CronSpec,
    },
}
