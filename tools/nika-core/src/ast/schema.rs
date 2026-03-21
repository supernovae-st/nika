//! Schema version definitions for Nika workflows.
//!
//! Nika only supports the current schema version (@0.12).
//! Old versions are rejected with a clear upgrade message.

/// The only supported schema version.
pub const SCHEMA_VERSION: &str = "nika/workflow@0.12";

/// Validated schema version.
///
/// Single variant — Nika does not maintain backward compatibility
/// with old schema versions. Zero users = zero compat debt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaVersion {
    /// nika/workflow@0.12 — the only supported version
    V12,
}

impl SchemaVersion {
    /// Parse a schema version string. Only @0.12 is accepted.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            SCHEMA_VERSION => Some(Self::V12),
            _ => None,
        }
    }

    /// Get the string representation.
    pub fn as_str(&self) -> &'static str {
        SCHEMA_VERSION
    }

    /// Get all valid schema versions.
    pub fn all() -> &'static [Self] {
        &[Self::V12]
    }

    /// Get the latest schema version.
    pub fn latest() -> Self {
        Self::V12
    }

    /// Get the numeric version for comparison.
    pub fn version_number(&self) -> u32 {
        12
    }

    /// No migration needed — only one version exists.
    pub fn migration_hint(&self) -> Option<&'static str> {
        None
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
            SchemaVersion::parse("nika/workflow@0.12"),
            Some(SchemaVersion::V12)
        );
        assert_eq!(SchemaVersion::parse("nika/workflow@0.1"), None);
        assert_eq!(SchemaVersion::parse("nika/workflow@0.10"), None);
        assert_eq!(SchemaVersion::parse("nika/workflow@0.11"), None);
        assert_eq!(SchemaVersion::parse("invalid"), None);
    }

    #[test]
    fn test_schema_version_latest() {
        assert_eq!(SchemaVersion::latest(), SchemaVersion::V12);
        assert_eq!(SchemaVersion::latest().as_str(), "nika/workflow@0.12");
    }
}
