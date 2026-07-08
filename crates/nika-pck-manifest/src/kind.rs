// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The D4 artifact taxonomy — closed reserved-core + the vendor escape.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The artifact classes the pck layer routes on (ADR-094 D4 · ratified
/// D-2026-07-08-N2 · mirrored by FCI-004's reserved-core list).
///
/// **TOTAL deserialization**: a wire string outside the reserved core lands
/// in [`ArtifactKind::CustomTool`] verbatim — `x-<vendor>:*` declarations
/// AND future core classes both parse today (the taxonomy is a ROUTER, not
/// a validator; a 12th class is an additive variant, never a break).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ArtifactKind {
    /// A `.nika.yaml` workflow — the atomic unit.
    Workflow,
    /// A content pack (spec snapshot · templates · examples).
    Pack,
    /// A markdown skill.
    Skill,
    /// An agent preset (verb + tools + budgets).
    AgentPreset,
    /// A template variant (a parameterized workflow family member).
    TemplateVariant,
    /// A hash-pinned pointer to model weights (GGUF/safetensors upstream).
    ModelPointer,
    /// An MCP server alias/config row.
    McpConfig,
    /// A conformance fixture (the paired check⇄run corpus class).
    ConformanceFixture,
    /// A provider profile (endpoint + auth shape + capability facts).
    ProviderProfile,
    /// A policy preset (permits/budget bundles).
    PolicyPreset,
    /// A benchmark suite.
    BenchSuite,
    /// The open escape: `x-<vendor>:*` custom-tool declarations and any
    /// not-yet-known class — carried verbatim, round-trips byte-identical.
    CustomTool(CustomKind),
}

/// The opaque payload of [`ArtifactKind::CustomTool`] — constructible ONLY
/// through [`ArtifactKind::from_wire`], which routes reserved-core wire
/// forms to their named variants FIRST. A `CustomTool` carrying a core
/// string (`CustomTool("workflow")` shadowing [`ArtifactKind::Workflow`])
/// is therefore **unrepresentable**, and type→wire→type stays identity on
/// a vocabulary frozen forever (Gate-11 P1 · injectivity by construction).
///
/// Deliberate INV#19 exemption: NO public constructor — `from_wire` IS the
/// constructor; a public `new` would reopen the shadowing hole.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct CustomKind(String);

impl CustomKind {
    /// The wire string, verbatim.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for CustomKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl ArtifactKind {
    /// The kebab-case wire form (the shape Claude plugins / MCP registry /
    /// ARA all ship · the string `CustomTool` carries verbatim).
    #[must_use]
    pub fn as_wire(&self) -> &str {
        match self {
            ArtifactKind::Workflow => "workflow",
            ArtifactKind::Pack => "pack",
            ArtifactKind::Skill => "skill",
            ArtifactKind::AgentPreset => "agent-preset",
            ArtifactKind::TemplateVariant => "template-variant",
            ArtifactKind::ModelPointer => "model-pointer",
            ArtifactKind::McpConfig => "mcp-config",
            ArtifactKind::ConformanceFixture => "conformance-fixture",
            ArtifactKind::ProviderProfile => "provider-profile",
            ArtifactKind::PolicyPreset => "policy-preset",
            ArtifactKind::BenchSuite => "bench-suite",
            ArtifactKind::CustomTool(k) => k.as_str(),
        }
    }

    /// Parse the wire form — TOTAL: never fails, unknown → `CustomTool`.
    #[must_use]
    pub fn from_wire(s: &str) -> Self {
        match s {
            "workflow" => ArtifactKind::Workflow,
            "pack" => ArtifactKind::Pack,
            "skill" => ArtifactKind::Skill,
            "agent-preset" => ArtifactKind::AgentPreset,
            "template-variant" => ArtifactKind::TemplateVariant,
            "model-pointer" => ArtifactKind::ModelPointer,
            "mcp-config" => ArtifactKind::McpConfig,
            "conformance-fixture" => ArtifactKind::ConformanceFixture,
            "provider-profile" => ArtifactKind::ProviderProfile,
            "policy-preset" => ArtifactKind::PolicyPreset,
            "bench-suite" => ArtifactKind::BenchSuite,
            other => ArtifactKind::CustomTool(CustomKind(other.to_owned())),
        }
    }
}

impl Serialize for ArtifactKind {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_wire())
    }
}

impl<'de> Deserialize<'de> for ArtifactKind {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        Ok(ArtifactKind::from_wire(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every reserved-core variant, paired with its LOCKED wire form —
    /// renaming a shipped variant is the ONE breaking change D4 forbids,
    /// so the full table is pinned value-exactly.
    const CORE: &[(ArtifactKind, &str)] = &[
        (ArtifactKind::Workflow, "workflow"),
        (ArtifactKind::Pack, "pack"),
        (ArtifactKind::Skill, "skill"),
        (ArtifactKind::AgentPreset, "agent-preset"),
        (ArtifactKind::TemplateVariant, "template-variant"),
        (ArtifactKind::ModelPointer, "model-pointer"),
        (ArtifactKind::McpConfig, "mcp-config"),
        (ArtifactKind::ConformanceFixture, "conformance-fixture"),
        (ArtifactKind::ProviderProfile, "provider-profile"),
        (ArtifactKind::PolicyPreset, "policy-preset"),
        (ArtifactKind::BenchSuite, "bench-suite"),
    ];

    #[test]
    fn the_eleven_reserved_core_wire_forms_are_locked() {
        assert_eq!(CORE.len(), 11, "the D4 reserved core is 11 named classes");
        for (kind, wire) in CORE {
            assert_eq!(kind.as_wire(), *wire);
            assert_eq!(&ArtifactKind::from_wire(wire), kind);
            // serde agrees with the hand pair
            let json = serde_json::to_string(kind).unwrap();
            assert_eq!(json, format!("\"{wire}\""));
            let back: ArtifactKind = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, kind);
        }
    }

    #[test]
    fn unknown_wire_strings_land_in_custom_tool_verbatim() {
        // the vendor escape…
        let k = ArtifactKind::from_wire("x-acme:deploy-tool");
        assert!(
            matches!(&k, ArtifactKind::CustomTool(c) if c.as_str() == "x-acme:deploy-tool"),
            "{k:?}"
        );
        assert_eq!(k.as_wire(), "x-acme:deploy-tool");
        // …and a FUTURE core class parses today (forward-compat by construction)
        let f: ArtifactKind = serde_json::from_str("\"cognitive-machinery\"").unwrap();
        assert_eq!(f, ArtifactKind::from_wire("cognitive-machinery"));
        // round-trips byte-identical
        assert_eq!(
            serde_json::to_string(&f).unwrap(),
            "\"cognitive-machinery\""
        );
    }

    #[test]
    fn deserialization_is_total_never_an_error() {
        for s in ["", " ", "WORKFLOW", "Workflow", "workflow ", "🦋"] {
            let k: ArtifactKind = serde_json::from_str(&format!("{s:?}")).unwrap();
            // case/space variants are NOT the core forms — they stay custom,
            // verbatim (the wire is exact, not fuzzy)
            assert!(
                matches!(&k, ArtifactKind::CustomTool(c) if c.as_str() == s),
                "{k:?}"
            );
        }
    }

    #[test]
    fn custom_tool_cannot_shadow_a_core_form_injectivity_by_construction() {
        // Gate-11 P1: `CustomTool("workflow")` used to be constructible and
        // serialized byte-identical to `Workflow` (type→wire→type broke).
        // `CustomKind` has no public constructor — the ONLY path is
        // `from_wire`, which routes core forms to their named variants:
        for (kind, wire) in CORE {
            assert_eq!(&ArtifactKind::from_wire(wire), kind);
            assert!(
                !matches!(ArtifactKind::from_wire(wire), ArtifactKind::CustomTool(_)),
                "core form `{wire}` must never ride the escape"
            );
        }
        // and Display on the payload prints the wire verbatim
        match ArtifactKind::from_wire("x-acme:deploy") {
            ArtifactKind::CustomTool(c) => assert_eq!(c.to_string(), "x-acme:deploy"),
            other => panic!("expected CustomTool, got {other:?}"),
        }
    }
}
