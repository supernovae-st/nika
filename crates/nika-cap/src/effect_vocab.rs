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
    /// · `write` = `nika:write`/`nika:edit`/`nika:remove_file`. The fine-grained boundary
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
            if matches!(tool, "nika:write" | "nika:edit" | "nika:remove_file") {
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn as_str_spells_every_class_on_the_wire() {
        assert_eq!(EffectClass::Exec.as_str(), "exec");
        assert_eq!(EffectClass::Write.as_str(), "write");
        assert_eq!(EffectClass::Net.as_str(), "net");
        assert_eq!(EffectClass::Tools.as_str(), "tools");
    }

    #[test]
    fn as_str_and_serde_spell_that_name_the_same_way() {
        for class in [
            EffectClass::Exec,
            EffectClass::Write,
            EffectClass::Net,
            EffectClass::Tools,
        ] {
            let wire = serde_json::to_string(&class).expect("an effect class serializes");
            assert_eq!(wire, format!("\"{}\"", class.as_str()));
            let back: EffectClass =
                serde_json::from_str(&wire).expect("its own wire name reads back");
            assert_eq!(back, class);
        }
    }

    #[test]
    fn classify_projects_through_as_str_in_a_stable_order() {
        let names = |verb: &str, tool: Option<&str>| {
            EffectClass::classify(verb, tool)
                .into_iter()
                .map(EffectClass::as_str)
                .collect::<Vec<_>>()
        };
        assert_eq!(names("exec", None), vec!["exec"]);
        assert_eq!(names("invoke", Some("nika:write")), vec!["write", "tools"]);
        assert_eq!(
            names("invoke", Some("nika:remove_file")),
            vec!["write", "tools"],
            "a removal is the coarse write class, never a read"
        );
        assert_eq!(names("invoke", Some("nika:fetch")), vec!["net", "tools"]);
        assert_eq!(names("invoke", Some("nika:read")), vec!["tools"]);
        assert_eq!(names("infer", None), Vec::<&str>::new());
    }
}
