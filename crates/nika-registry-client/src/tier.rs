// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The NEP-0016 provenance ladder + the operator admission floor.
//!
//! The resolver already proves consistency (the pinned digest) and, for
//! signed entries, origin (minisign under a TOFU-anchored key) — this
//! module gives those proofs their VOCABULARY: a closed tier ladder
//! (`unprovenanced < provenanced < stage-clear < verified`), totally
//! ordered, admitted by EVIDENCE observed at fetch, never by a registry
//! claim. Two laws ride it:
//!
//! - **Evidence admits, never claims.** v1 admits exactly two tiers:
//!   an unsigned entry resolves `unprovenanced` (the v0.1 digest floor);
//!   an entry whose minisign verifies under the TOFU record resolves
//!   `provenanced`. `stage-clear`/`verified` are RESERVED — their
//!   evidence formats land with the registry arc (F-P25/F-P24) — so v1
//!   admits neither, and a cache record naming one is treated as
//!   tampered (the NIKA-REG-004 class: it claims what no fetch could
//!   have proven).
//! - **The floor is operator data.** `~/.nika/registry/policy.toml`
//!   (`{ version = 1, floor = "<tier>" }`) — an ABSENT file means
//!   `floor = "unprovenanced"` (today's behavior, stated honestly), an
//!   unknown `version`/`floor`/key refuses CLOSED (a typo'd floor must
//!   never silently no-op). No floor lives in source: the default is
//!   the semantics of absence.

use std::path::Path;

use crate::RegistryError;

/// The policy file name, beside the cache (`~/.nika/registry/`).
const POLICY_FILE: &str = "policy.toml";
/// The closed set of policy top-level keys — an unknown key is almost
/// certainly a typo'd floor, and a silently-ignored floor is a lie.
const POLICY_KEYS: [&str; 2] = ["version", "floor"];
/// The one policy version this engine speaks.
const POLICY_VERSION: i64 = 1;

/// A provenance tier (NEP-0016 law 1) — the set is CLOSED the way the
/// verb set is closed: growth is a spec amendment, never a parse. The
/// declaration order IS the total order (`Unprovenanced < Provenanced <
/// StageClear < Verified`) the floor comparison rides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProvenanceTier {
    /// The v0.1 digest floor: bytes provably consistent with the pinned
    /// `integrity.sha256`, origin unproven.
    Unprovenanced,
    /// The artifact's minisign verified under the publisher's
    /// TOFU-anchored key: origin proven to a key, key continuity proven
    /// by the TOFU record.
    Provenanced,
    /// RESERVED — a registry staged-window statement (the evidence
    /// format lands with F-P25); v1 admits it nowhere.
    StageClear,
    /// RESERVED — a complete in-toto publish layout (the evidence
    /// format lands with F-P24); v1 admits it nowhere.
    Verified,
}

impl ProvenanceTier {
    /// The canonical lowercase spelling (policy file · cache record ·
    /// receipts) — the exact strings the ladder is closed over.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unprovenanced => "unprovenanced",
            Self::Provenanced => "provenanced",
            Self::StageClear => "stage-clear",
            Self::Verified => "verified",
        }
    }

    /// Parse a tier string — `None` on anything outside the closed set
    /// (an unknown tier REFUSES wherever it appears: law 1).
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "unprovenanced" => Some(Self::Unprovenanced),
            "provenanced" => Some(Self::Provenanced),
            "stage-clear" => Some(Self::StageClear),
            "verified" => Some(Self::Verified),
            _ => None,
        }
    }

    /// Can v1 evidence admit this tier? The fetch lane observes exactly
    /// two evidence classes (digest · minisign+TOFU), so anything above
    /// `Provenanced` on a v1 record is inflation — the tampered class.
    pub(crate) fn admissible_by_v1_evidence(self) -> bool {
        self <= Self::Provenanced
    }

    /// The legacy boolean this tier denotes (`signed: true` anchored a
    /// verified minisign = `provenanced`) — the backwards-compat read
    /// for records written before the tier field existed (NEP-0016 §
    /// Backwards Compatibility: no migration, no invalidation).
    pub(crate) fn denotes_signed(self) -> bool {
        self >= Self::Provenanced
    }
}

impl std::fmt::Display for ProvenanceTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The operator admission policy (law 3): just the floor today. Loaded
/// once per resolve; an absent file is the honest default.
pub(crate) struct Policy {
    pub floor: ProvenanceTier,
}

impl Policy {
    /// Read `~/.nika/registry/policy.toml`. Absent → the
    /// `unprovenanced` floor. Present → it must be exactly
    /// `{ version = 1, floor = "<tier>" }`: an unknown version, an
    /// unknown floor, an unknown key, or a missing field all refuse
    /// CLOSED with the fix taught (the operator's intent is unknown —
    /// guessing it would be the lie).
    pub fn load(cache_root: &Path) -> Result<Self, RegistryError> {
        let path = cache_root.join(POLICY_FILE);
        let read = std::fs::read_to_string(&path); // seam-bypass-ok: local operator policy · #512 follow-up
        let raw = match read {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    floor: ProvenanceTier::Unprovenanced,
                });
            }
            Err(e) => {
                return Err(RegistryError::env(format!(
                    "cannot read the registry policy {}: {e}",
                    path.display()
                )));
            }
        };
        Self::parse(&raw, &path)
    }

    /// Parse + vet the policy text — the closed-set law applied to the
    /// operator's own file (a typo must refuse, never no-op).
    fn parse(raw: &str, path: &Path) -> Result<Self, RegistryError> {
        let bad = |why: String| {
            RegistryError::env(format!(
                "the registry policy {} {why}\n  the file is operator data — fix it, or delete it for the default (`floor = \"unprovenanced\"`)",
                path.display()
            ))
        };
        let doc: toml_edit::DocumentMut = raw
            .parse()
            .map_err(|e| bad(format!("does not parse as TOML: {e}")))?;
        for (key, _) in doc.iter() {
            if !POLICY_KEYS.contains(&key) {
                return Err(bad(format!(
                    "carries unknown key `{key}` — v1 speaks exactly {{ version, floor }}"
                )));
            }
        }
        match doc.get("version").and_then(toml_edit::Item::as_integer) {
            Some(POLICY_VERSION) => {}
            Some(n) => {
                return Err(bad(format!(
                    "sets `version = {n}` — this engine speaks policy version 1"
                )));
            }
            None => return Err(bad("needs `version = 1` (an integer)".to_owned())),
        }
        let floor_raw = match doc.get("floor").and_then(toml_edit::Item::as_str) {
            Some(raw) => raw,
            None => {
                return Err(bad(
                    "needs `floor = \"<tier>\"` — one of unprovenanced · provenanced · stage-clear · verified".to_owned(),
                ));
            }
        };
        let Some(floor) = ProvenanceTier::parse(floor_raw) else {
            return Err(bad(format!(
                "sets `floor = \"{floor_raw}\"` — unknown tier; the closed ladder is unprovenanced < provenanced < stage-clear < verified"
            )));
        };
        Ok(Self { floor })
    }
}
