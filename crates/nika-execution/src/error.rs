// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::fmt;

/// Refusal raised while capturing or admitting an immutable execution world.
#[derive(Debug)]
#[non_exhaustive]
pub enum ExecutionError {
    /// A captured snapshot uses a format this engine cannot readmit.
    #[non_exhaustive]
    UnsupportedSnapshotFormat {
        /// Format version carried by the snapshot.
        found: u32,
        /// Format version this engine accepts.
        expected: u32,
    },
    /// A captured unit's stored identity no longer matches its owned bytes.
    #[non_exhaustive]
    UnitDigestMismatch {
        /// Contained logical path of the changed unit.
        logical_path: String,
    },
    /// The snapshot's stored aggregate identity no longer matches its world.
    SnapshotDigestMismatch,
    /// The captured unit map no longer describes the closure rooted at `root`.
    SnapshotStructureMismatch,
    /// A logical path was absolute, escaped its root, was empty, or was not UTF-8.
    #[non_exhaustive]
    InvalidLogicalPath {
        /// The rejected path as supplied by the caller or workflow.
        path: String,
    },
    /// A descriptor-relative open or read failed.
    #[non_exhaustive]
    Io {
        /// The contained logical path that failed.
        logical_path: String,
        /// The operating-system error.
        source: std::io::Error,
    },
    /// One captured unit exceeded its byte ceiling.
    #[non_exhaustive]
    UnitSizeLimit {
        /// The contained logical path.
        logical_path: String,
        /// Configured maximum bytes per unit.
        limit: usize,
    },
    /// The captured world exceeded its aggregate byte ceiling.
    #[non_exhaustive]
    TotalSizeLimit {
        /// Configured maximum aggregate bytes.
        limit: usize,
    },
    /// The captured world exceeded its unit-count ceiling.
    #[non_exhaustive]
    UnitCountLimit {
        /// Configured maximum number of units.
        limit: usize,
    },
    /// A child graph exceeded its depth ceiling.
    #[non_exhaustive]
    DepthLimit {
        /// The child that would cross the ceiling.
        logical_path: String,
        /// Configured maximum child depth, with the root at depth zero.
        limit: usize,
    },
    /// A workflow dependency graph contains a cycle.
    #[non_exhaustive]
    DependencyCycle {
        /// Ordered logical identities ending at the repeated node.
        chain: Vec<String>,
    },
    /// Two authored references collapse onto one logical identity.
    #[non_exhaustive]
    DuplicateLogicalIdentity {
        /// The normalized identity both references selected.
        logical_path: String,
        /// First authored spelling.
        first: String,
        /// Conflicting authored spelling or unit kind.
        second: String,
    },
    /// A registry child cannot be frozen by the descriptor-rooted reader.
    #[non_exhaustive]
    RegistryDependency {
        /// The pinned or unpinned registry reference.
        reference: String,
    },
    /// Workflow or skill text was not UTF-8.
    #[non_exhaustive]
    NonUtf8 {
        /// The contained logical path.
        logical_path: String,
    },
    /// Workflow parsing failed against captured bytes.
    #[non_exhaustive]
    Parse {
        /// The contained logical path.
        logical_path: String,
        /// Parser diagnostic.
        detail: String,
    },
    /// A skill path was outside the workflow's declared read boundary.
    #[non_exhaustive]
    SkillNotAuthorized {
        /// Workflow carrying the reference.
        workflow: String,
        /// Authored skill path.
        skill: String,
    },
    /// Static checking refused the captured world.
    #[non_exhaustive]
    CheckFailed {
        /// Renderable findings from the one checker.
        findings: Vec<String>,
    },
    /// Skill parsing or resolution refused the captured world.
    #[non_exhaustive]
    SkillCheckFailed {
        /// Workflow carrying the bad skill reference.
        workflow: String,
        /// Renderable skill findings.
        findings: Vec<String>,
    },
    /// A requested unit was absent from an otherwise captured world.
    #[non_exhaustive]
    MissingUnit {
        /// Normalized logical identity.
        logical_path: String,
    },
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSnapshotFormat { found, expected } => write!(
                f,
                "unsupported execution snapshot format {found} (expected {expected})"
            ),
            Self::UnitDigestMismatch { logical_path } => write!(
                f,
                "captured unit `{logical_path}` does not match its digest"
            ),
            Self::SnapshotDigestMismatch => {
                write!(f, "captured world does not match its snapshot digest")
            }
            Self::SnapshotStructureMismatch => write!(
                f,
                "captured world does not match its rooted dependency closure"
            ),
            Self::InvalidLogicalPath { path } => write!(f, "invalid logical path `{path}`"),
            Self::Io {
                logical_path,
                source,
            } => write!(f, "cannot read captured unit `{logical_path}`: {source}"),
            Self::UnitSizeLimit {
                logical_path,
                limit,
            } => write!(f, "captured unit `{logical_path}` exceeds {limit} bytes"),
            Self::TotalSizeLimit { limit } => {
                write!(f, "captured world exceeds {limit} aggregate bytes")
            }
            Self::UnitCountLimit { limit } => {
                write!(f, "captured world exceeds {limit} units")
            }
            Self::DepthLimit {
                logical_path,
                limit,
            } => write!(f, "child `{logical_path}` exceeds dependency depth {limit}"),
            Self::DependencyCycle { chain } => {
                write!(f, "dependency cycle: {}", chain.join(" -> "))
            }
            Self::DuplicateLogicalIdentity {
                logical_path,
                first,
                second,
            } => write!(
                f,
                "duplicate logical identity `{logical_path}` from `{first}` and `{second}`"
            ),
            Self::RegistryDependency { reference } => write!(
                f,
                "registry dependency `{reference}` has no atomic owned-byte view"
            ),
            Self::NonUtf8 { logical_path } => {
                write!(f, "captured unit `{logical_path}` is not UTF-8")
            }
            Self::Parse {
                logical_path,
                detail,
            } => write!(
                f,
                "cannot parse captured workflow `{logical_path}`: {detail}"
            ),
            Self::SkillNotAuthorized { workflow, skill } => write!(
                f,
                "workflow `{workflow}` does not authorize skill read `{skill}`"
            ),
            Self::CheckFailed { findings } => {
                write!(
                    f,
                    "captured workflow failed check: {}",
                    findings.join(" | ")
                )
            }
            Self::SkillCheckFailed { workflow, findings } => write!(
                f,
                "captured skills for `{workflow}` failed check: {}",
                findings.join(" | ")
            ),
            Self::MissingUnit { logical_path } => {
                write!(f, "captured world has no unit `{logical_path}`")
            }
        }
    }
}

impl std::error::Error for ExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
