// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The effect VOCABULARY — the three classes a task reaches for, the
//! human-gate tool id, and the certificate's authority projection.
//!
//! These three outlived `policy:`. The block died with the 9-key
//! envelope (spec `d20b139`) and its judge died with it, but the words
//! it spoke are read by lanes that never depended on a declaration: the
//! lethal trifecta, the affirmative-consent law, the runtime's approval
//! tickets, the certificate. **A vocabulary is not a policy.**

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::permits::Permits;

/// The human gate: an `invoke:` of this tool (spec 10 · « the pause IS
/// the consent » · exit 4 · resume with the answer).
pub const HUMAN_GATE_TOOL: &str = "nika:prompt";

/// One `<effect-class>` (spec 10 · the closed set `exec · write · net ·
/// tools` — the effect vocabulary with `fs` split at its grain of harm:
/// `write`; reads are not gateable in v1). `lowercase` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum EffectClass {
    /// The `exec:` verb.
    Exec,
    /// A file-writing builtin (`nika:write` · `nika:edit`).
    Write,
    /// A URL-reaching builtin (`nika:fetch` · `nika:notify`).
    Net,
    /// The whole `invoke:` surface.
    Tools,
}

impl EffectClass {
    /// The wire/witness name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exec => "exec",
            Self::Write => "write",
            Self::Net => "net",
            Self::Tools => "tools",
        }
    }

    /// The COARSE effect classes ONE task carries (spec 10) — the policy
    /// projection of the builtin classification, mirroring the reference
    /// evaluator's `_task_effect_classes` exactly: `exec` = the `exec:`
    /// verb · `tools` = every `invoke:` · `net` = `nika:fetch`/`nika:notify`
    /// · `write` = `nika:write`/`nika:edit`. The fine-grained boundary
    /// table (`builtin_effect` in nika-schema) answers a DIFFERENT
    /// question (which arg carries the target); the two are pinned
    /// coherent by test, never derived from each other.
    #[must_use]
    pub fn classify(verb: &str, tool: Option<&str>) -> BTreeSet<Self> {
        let mut out = BTreeSet::new();
        if verb == "exec" {
            out.insert(Self::Exec);
        }
        if let Some(tool) = tool {
            out.insert(Self::Tools);
            if matches!(tool, "nika:fetch" | "nika:notify") {
                out.insert(Self::Net);
            }
            if matches!(tool, "nika:write" | "nika:edit") {
                out.insert(Self::Write);
            }
        }
        out
    }
}

/// The certificate's AUTHORITY projection (spec 10 §the certificate
/// names its effects) — a projection, never a judge: the check ladder
/// stays the one truth, this field exists so a certificate consumer
/// never re-derives the boundary story.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct CertEffects {
    /// Whether the file declares a `permits:` block.
    pub boundary_declared: bool,
    /// The inferred TIGHTEST boundary the body statically needs — the
    /// same object `nika check --infer-permits` prints.
    pub needed: Permits,
    /// Count of required-outside-permitted violations (0 in any clean
    /// report).
    pub escapes: usize,
}

impl CertEffects {
    /// Assemble the projection (invariant #19 — constructor on
    /// `#[non_exhaustive]`).
    #[must_use]
    pub fn new(boundary_declared: bool, needed: Permits, escapes: usize) -> Self {
        Self {
            boundary_declared,
            needed,
            escapes,
        }
    }
}
