// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Schema version definitions for Nika workflows.
//!
//! This module owns the `SchemaVersion` enum which is shared across
//! the AST pipeline — used by both the analyzer (Phase 2) and the
//! analyzed AST types.
//!
//! Extracted from `analyzed/workflow.rs` so that it can be imported
//! without pulling in the full analyzed AST.

/// Validated schema version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaVersion {
    /// nika/workflow@0.1
    V01,
    /// nika/workflow@0.2
    V02,
    /// nika/workflow@0.3
    V03,
    /// nika/workflow@0.4
    V04,
    /// nika/workflow@0.5
    V05,
    /// nika/workflow@0.6
    V06,
    /// nika/workflow@0.7
    V07,
    /// nika/workflow@0.8
    V08,
    /// nika/workflow@0.9
    V09,
    /// nika/workflow@0.10
    V10,
    /// nika/workflow@0.11
    V11,
    /// nika/workflow@0.12
    V12,
}

impl SchemaVersion {
    /// Parse a schema version string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "nika/workflow@0.1" => Some(Self::V01),
            "nika/workflow@0.2" => Some(Self::V02),
            "nika/workflow@0.3" => Some(Self::V03),
            "nika/workflow@0.4" => Some(Self::V04),
            "nika/workflow@0.5" => Some(Self::V05),
            "nika/workflow@0.6" => Some(Self::V06),
            "nika/workflow@0.7" => Some(Self::V07),
            "nika/workflow@0.8" => Some(Self::V08),
            "nika/workflow@0.9" => Some(Self::V09),
            "nika/workflow@0.10" => Some(Self::V10),
            "nika/workflow@0.11" => Some(Self::V11),
            "nika/workflow@0.12" => Some(Self::V12),
            _ => None,
        }
    }

    /// Get the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::V01 => "nika/workflow@0.1",
            Self::V02 => "nika/workflow@0.2",
            Self::V03 => "nika/workflow@0.3",
            Self::V04 => "nika/workflow@0.4",
            Self::V05 => "nika/workflow@0.5",
            Self::V06 => "nika/workflow@0.6",
            Self::V07 => "nika/workflow@0.7",
            Self::V08 => "nika/workflow@0.8",
            Self::V09 => "nika/workflow@0.9",
            Self::V10 => "nika/workflow@0.10",
            Self::V11 => "nika/workflow@0.11",
            Self::V12 => "nika/workflow@0.12",
        }
    }

    /// Get all valid schema versions.
    pub fn all() -> &'static [Self] {
        &[
            Self::V01,
            Self::V02,
            Self::V03,
            Self::V04,
            Self::V05,
            Self::V06,
            Self::V07,
            Self::V08,
            Self::V09,
            Self::V10,
            Self::V11,
            Self::V12,
        ]
    }

    /// Get the latest schema version.
    pub fn latest() -> Self {
        Self::V12
    }

    /// Get the numeric version for comparison (e.g., V03 returns 3).
    pub fn version_number(&self) -> u32 {
        match self {
            Self::V01 => 1,
            Self::V02 => 2,
            Self::V03 => 3,
            Self::V04 => 4,
            Self::V05 => 5,
            Self::V06 => 6,
            Self::V07 => 7,
            Self::V08 => 8,
            Self::V09 => 9,
            Self::V10 => 10,
            Self::V11 => 11,
            Self::V12 => 12,
        }
    }

    /// Check if a version supports a minimum required version.
    pub fn supports(&self, min_version: Self) -> bool {
        self.version_number() >= min_version.version_number()
    }

    /// Get a migration hint for upgrading from this schema version.
    ///
    /// Returns None for the latest version.
    pub fn migration_hint(&self) -> Option<&'static str> {
        match self {
            Self::V01 => Some("@0.2 adds MCP servers, invoke: and agent: verbs"),
            Self::V02 => Some("@0.3 adds for_each iteration and retry config"),
            Self::V03 => Some("@0.4 adds decompose and structured output"),
            Self::V04 => Some("@0.5 adds extended thinking and output format"),
            Self::V05 => Some("@0.6 adds agents: definitions and skills:"),
            Self::V06 => Some("@0.7 adds log: config and catch: error handling"),
            Self::V07 => Some("@0.8 adds fetch: verb improvements"),
            Self::V08 => Some("@0.9 adds context: files"),
            Self::V09 => Some("@0.10 adds inputs: with defaults and artifacts:"),
            Self::V10 => Some("@0.11 adds with: bindings replacing include:"),
            Self::V11 => Some("@0.12 adds depends_on:, include:, and ?? fallback operator"),
            Self::V12 => None,
        }
    }

    /// Check if MCP servers are supported.
    pub fn supports_mcp(&self) -> bool {
        self.supports(Self::V02)
    }

    /// Check if invoke/agent verbs are supported.
    pub fn supports_invoke_agent(&self) -> bool {
        self.supports(Self::V02)
    }

    /// Check if for_each is supported.
    pub fn supports_for_each(&self) -> bool {
        self.supports(Self::V03)
    }

    /// Check if skills are supported.
    pub fn supports_skills(&self) -> bool {
        self.supports(Self::V06)
    }

    /// Check if agent definitions are supported.
    pub fn supports_agent_defs(&self) -> bool {
        self.supports(Self::V06)
    }

    /// Check if context files are supported.
    pub fn supports_context(&self) -> bool {
        self.supports(Self::V09)
    }

    /// Check if include/DAG fusion is supported.
    pub fn supports_include(&self) -> bool {
        self.supports(Self::V09)
    }

    /// Check if inputs are supported.
    pub fn supports_inputs(&self) -> bool {
        self.supports(Self::V10)
    }

    /// Check if artifacts are supported.
    pub fn supports_artifacts(&self) -> bool {
        self.supports(Self::V10)
    }

    /// Check if retry configuration is supported.
    pub fn supports_retry(&self) -> bool {
        self.supports(Self::V03)
    }

    /// Check if with: binding syntax is supported.
    pub fn supports_with(&self) -> bool {
        self.supports(Self::V12)
    }

    /// Check if include: syntax is supported.
    pub fn supports_includes(&self) -> bool {
        self.supports(Self::V12)
    }

    /// Check if depends_on: syntax is supported.
    pub fn supports_depends_on(&self) -> bool {
        self.supports(Self::V12)
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version_parse() {
        assert_eq!(
            SchemaVersion::parse("nika/workflow@0.1"),
            Some(SchemaVersion::V01)
        );
        assert_eq!(
            SchemaVersion::parse("nika/workflow@0.12"),
            Some(SchemaVersion::V12)
        );
        assert_eq!(SchemaVersion::parse("invalid"), None);
        assert_eq!(SchemaVersion::parse("nika/workflow@0.99"), None);
    }

    #[test]
    fn test_schema_version_latest() {
        assert_eq!(SchemaVersion::latest(), SchemaVersion::V12);
        assert_eq!(SchemaVersion::latest().as_str(), "nika/workflow@0.12");
    }

    #[test]
    fn test_schema_version_number() {
        assert_eq!(SchemaVersion::V01.version_number(), 1);
        assert_eq!(SchemaVersion::V05.version_number(), 5);
        assert_eq!(SchemaVersion::V12.version_number(), 12);
    }

    #[test]
    fn test_schema_version_supports() {
        // V01 only supports V01
        assert!(SchemaVersion::V01.supports(SchemaVersion::V01));
        assert!(!SchemaVersion::V01.supports(SchemaVersion::V02));

        // V12 supports all versions
        assert!(SchemaVersion::V12.supports(SchemaVersion::V01));
        assert!(SchemaVersion::V12.supports(SchemaVersion::V12));
    }

    #[test]
    fn test_schema_version_feature_gates() {
        // MCP requires v0.2+
        assert!(!SchemaVersion::V01.supports_mcp());
        assert!(SchemaVersion::V02.supports_mcp());
        assert!(SchemaVersion::V12.supports_mcp());

        // for_each requires v0.3+
        assert!(!SchemaVersion::V01.supports_for_each());
        assert!(!SchemaVersion::V02.supports_for_each());
        assert!(SchemaVersion::V03.supports_for_each());
        assert!(SchemaVersion::V12.supports_for_each());

        // skills requires v0.6+
        assert!(!SchemaVersion::V05.supports_skills());
        assert!(SchemaVersion::V06.supports_skills());
        assert!(SchemaVersion::V12.supports_skills());

        // context requires v0.9+
        assert!(!SchemaVersion::V08.supports_context());
        assert!(SchemaVersion::V09.supports_context());
        assert!(SchemaVersion::V12.supports_context());

        // inputs requires v0.10+
        assert!(!SchemaVersion::V09.supports_inputs());
        assert!(SchemaVersion::V10.supports_inputs());

        // with:/include:/depends_on: require v0.12+
        assert!(!SchemaVersion::V11.supports_with());
        assert!(SchemaVersion::V12.supports_with());
        assert!(!SchemaVersion::V11.supports_includes());
        assert!(SchemaVersion::V12.supports_includes());
        assert!(!SchemaVersion::V11.supports_depends_on());
        assert!(SchemaVersion::V12.supports_depends_on());
    }
}
