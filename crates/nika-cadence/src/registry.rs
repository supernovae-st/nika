// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The registry types — the parsed shape of `nika.yaml`.
//!
//! Discipline (the forward-compat invariants): `nika:` carries the
//! project's kebab-case NAME, the same grammar the workflow envelope
//! gives its own `nika:` · public-field structs are `#[non_exhaustive]`
//! (FCI-016) · no `HashMap` anywhere — the workspace lint
//! `iter_over_hash_type = "deny"` guards a future signature's
//! determinism, and this crate simply holds no maps · no `Vec` in
//! public returns (FCI-014) — accessors hand out slices or iterators.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};

use crate::cron::CronSpec;

/// The one project file (`nika.yaml`) — round 1 carries `ceiling` +
/// `arm:`. The project's other rungs (`traces:` · `registry:`) are
/// admitted opaque and judged by the project reader.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ArmRegistry {
    /// The project's NAME, kebab-case — the same grammar the workflow
    /// envelope gives its own `nika:`. It replaced a frozen `v1` tag: a
    /// field with one legal value is not a version, and the version now
    /// rides the `$schema` URL where an editor already reads it.
    pub nika: String,
    /// The project-wide run ceiling (USD) — a default every beat's
    /// `plafond` may exceed EXPLICITLY: the hierarchy stays visible.
    #[serde(default)]
    pub ceiling: Option<f64>,
    /// The armed beats (`arm:`).
    #[serde(default, rename = "arm")]
    beats: Vec<Beat>,
    /// Another rung of the SAME file — `traces:` belongs to the project
    /// reader (`nika_vocab::project` · `traces.keep`, the retention rung
    /// `nika-dap` consumes). Admitted OPAQUE so the closed grammar accepts
    /// the file it shares with that reader; judged THERE, never here.
    /// (2026-08-18: refused as « round 2 » it told operators to remove a
    /// key that ships and is consumed — the project starter's own
    /// `traces:` line made `nika arm` refuse. The leading underscore is
    /// the read: this field exists to be admitted, not consulted.)
    #[serde(default, rename = "traces")]
    _traces: Option<serde::de::IgnoredAny>,
    /// Another rung of the SAME file — `registry:` (`registry.floor`, the
    /// provenance GATE the registry client max-composes). Admitted opaque
    /// · judged by the project reader.
    #[serde(default, rename = "registry")]
    _registry: Option<serde::de::IgnoredAny>,
}

impl ArmRegistry {
    /// `^[a-z][a-z0-9-]*$` — the name shape, shared with the workflow
    /// envelope and with `nika_vocab::project`.
    #[must_use]
    pub fn is_kebab_id(s: &str) -> bool {
        s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }

    /// A retired schema tag (`^v[0-9]+$`). It refuses in a PROJECT file
    /// and nowhere else: a pre-nuke workflow carried `workflow:` beside
    /// its `nika: v1` and refuses on that key, while a project file had
    /// no companion — the same bytes would quietly stop meaning « schema
    /// v1 » and start meaning « a project named v1 ». Only the WHOLE
    /// marker: `vault` · `v2ray` · `v1-migration` stay ordinary names.
    #[must_use]
    pub fn is_retired_tag(s: &str) -> bool {
        s.strip_prefix('v')
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
    }

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
    /// The workflow to fire — a repo-relative `*.nika` path. Its
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
    /// Per-beat inputs (#1370) — the `--var KEY=VALUE` pairs every fire
    /// binds to the workflow's declared `inputs:`, so a tenant-
    /// parameterized workflow is armed once per tenant instead of
    /// rendered once per tenant. One SCALAR per key (string · number ·
    /// bool), carried as its `--var` text: the workflow's declared type
    /// drives the coercion at admission, exactly as a run's `--var`
    /// does (a typed array input takes its JSON text in quotes). Key-
    /// sorted (a `BTreeMap`): the generation hashes it deterministically.
    /// Absent reads as EMPTY — the beat it always was. The key SHAPE is
    /// judged by the validator; the membership (declared or not) is the
    /// fire edge's, which alone opens the workflow.
    #[serde(default, deserialize_with = "scalar_inputs")]
    pub inputs: BTreeMap<String, String>,
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

    /// The inputs in the `--var` shape (`KEY=VALUE`), key-sorted — the
    /// run edge hands them to the same door a run's `--var` enters, so
    /// unknown keys, declared types and `required:` are judged there,
    /// once, by the workflow's own declaration.
    pub fn input_vars(&self) -> impl Iterator<Item = String> + '_ {
        self.inputs
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
    }
}

/// `inputs:` — a mapping of SCALARS, each carried as its `--var` text.
/// A list, a map or a null refuses at parse, naming the key and the
/// spelling a typed collection input accepts (its JSON text, quoted).
fn scalar_inputs<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error> {
    struct Inputs;

    impl<'de> Visitor<'de> for Inputs {
        type Value = BTreeMap<String, String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("inputs: a mapping of scalar values (string · number · bool)")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut out = BTreeMap::new();
            while let Some(key) = access.next_key::<String>()? {
                let value = access
                    .next_value::<ScalarText>()
                    .map_err(|e| de::Error::custom(format!("inputs.{key}: {e}")))?;
                if out.insert(key.clone(), value.0).is_some() {
                    // Never last-wins: two values for one `--var KEY` is a
                    // guess about which tenant fires.
                    return Err(de::Error::custom(format!(
                        "inputs.{key}: written twice — one value per key"
                    )));
                }
            }
            Ok(out)
        }
    }

    deserializer.deserialize_map(Inputs)
}

/// One scalar as the text `--var` would carry: strings verbatim, numbers
/// and booleans in their shortest spelling.
struct ScalarText(String);

impl<'de> Deserialize<'de> for ScalarText {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Scalar;

        impl Visitor<'_> for Scalar {
            type Value = ScalarText;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(
                    "one scalar (string · number · bool) — a list or map input is written as its JSON text in quotes ('[\"a\",\"b\"]')",
                )
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(ScalarText(v.to_owned()))
            }

            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(ScalarText(v.to_string()))
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(ScalarText(v.to_string()))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(ScalarText(v.to_string()))
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Ok(ScalarText(v.to_string()))
            }
        }

        deserializer.deserialize_any(Scalar)
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
