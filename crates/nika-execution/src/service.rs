// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::path::Path;

use nika_check::CheckReport;
use nika_fs::OwnedDir;
use nika_schema::ResolvedSkills;
use nika_schema::raw::RawWorkflow;
use nika_types::id::{ExecutionId, TraceId};

use crate::{ExecutionError, ExecutionSnapshot, SnapshotLimits, SnapshotUnitKind};

/// Shared admission boundary for CLI, ARM, Serve, and future interfaces.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct ExecutionService {
    limits: SnapshotLimits,
}

impl ExecutionService {
    /// Build a service with explicit immutable-world ceilings.
    #[must_use]
    pub const fn new(limits: SnapshotLimits) -> Self {
        Self { limits }
    }

    /// Capture and statically admit one descriptor-rooted workflow world.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError`] when capture, parsing, static checking, or
    /// skill resolution refuses the world.
    pub fn admit(
        &self,
        project: &OwnedDir,
        root: &Path,
    ) -> Result<AdmittedExecution, ExecutionError> {
        let snapshot = ExecutionSnapshot::capture(project, root, self.limits)?;
        Self::admit_snapshot(snapshot)
    }

    /// Admit a workflow world with explicit project-level imports.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError`] under the same fail-closed conditions as
    /// [`Self::admit`], including an unreadable or duplicate import.
    pub fn admit_with_imports<I, P>(
        &self,
        project: &OwnedDir,
        root: &Path,
        imports: I,
    ) -> Result<AdmittedExecution, ExecutionError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let snapshot =
            ExecutionSnapshot::capture_with_imports(project, root, imports, self.limits)?;
        Self::admit_snapshot(snapshot)
    }

    /// Execute through a runner that can observe only the admitted world.
    ///
    /// The runner receives no root capability and no mutable pathname. Its
    /// outcome may itself be a `Result`, preserving the caller's typed runtime
    /// verdict without widening this infrastructure boundary.
    pub fn execute<T>(
        &self,
        admitted: AdmittedExecution,
        runner: for<'a> fn(ExecutionContext<'a>) -> T,
    ) -> ExecutionVerdict<T> {
        let AdmittedExecution {
            execution_id,
            trace_id,
            snapshot,
            workflow,
            check,
            skills,
        } = admitted;
        let outcome = runner(ExecutionContext {
            execution_id,
            trace_id,
            snapshot: &snapshot,
            workflow: &workflow,
            check: &check,
            skills: &skills,
        });
        ExecutionVerdict {
            execution_id,
            trace_id,
            snapshot_digest: snapshot.digest().to_owned(),
            outcome,
        }
    }

    fn admit_snapshot(snapshot: ExecutionSnapshot) -> Result<AdmittedExecution, ExecutionError> {
        let root = snapshot.root().to_owned();
        let root_text = snapshot
            .text(&root)
            .ok_or_else(|| ExecutionError::MissingUnit {
                logical_path: root.clone(),
            })?;
        let workflow = parse(&root, root_text)?;
        let mut reader = |path: &str| {
            snapshot
                .text(path)
                .map(str::to_owned)
                .ok_or_else(|| format!("captured world has no unit `{path}`"))
        };
        let check = nika_check::check_composed(&workflow, &root, &mut reader);
        if !check.is_clean() {
            let findings = report_findings(&root, &check);
            return Err(ExecutionError::CheckFailed { findings });
        }
        let skills = validate_skills(&snapshot, &root, &workflow)?;
        validate_child_worlds(&snapshot, &root)?;
        let execution_id = ExecutionId::generate();
        Ok(AdmittedExecution {
            execution_id,
            trace_id: execution_id.into(),
            snapshot,
            workflow,
            check,
            skills,
        })
    }
}

impl Default for ExecutionService {
    fn default() -> Self {
        Self::new(SnapshotLimits::default())
    }
}

/// A checked execution world. It deliberately owns no filesystem capability.
#[derive(Debug)]
#[non_exhaustive]
pub struct AdmittedExecution {
    execution_id: ExecutionId,
    trace_id: TraceId,
    snapshot: ExecutionSnapshot,
    workflow: RawWorkflow,
    check: CheckReport,
    skills: ResolvedSkills,
}

impl AdmittedExecution {
    /// Unique admitted execution identity.
    #[must_use]
    pub const fn execution_id(&self) -> ExecutionId {
        self.execution_id
    }

    /// Root trace identity derived directly from the execution ID.
    #[must_use]
    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    /// Immutable byte world judged at admission.
    #[must_use]
    pub const fn snapshot(&self) -> &ExecutionSnapshot {
        &self.snapshot
    }

    /// Parsed root workflow produced from snapshot bytes.
    #[must_use]
    pub const fn workflow(&self) -> &RawWorkflow {
        &self.workflow
    }

    /// Static report produced from snapshot bytes.
    #[must_use]
    pub const fn check(&self) -> &CheckReport {
        &self.check
    }

    /// Root workflow's resolved skill texts from snapshot bytes.
    #[must_use]
    pub const fn skills(&self) -> &ResolvedSkills {
        &self.skills
    }
}

/// Read-only execution input handed to an injected runtime runner.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct ExecutionContext<'a> {
    execution_id: ExecutionId,
    trace_id: TraceId,
    snapshot: &'a ExecutionSnapshot,
    workflow: &'a RawWorkflow,
    check: &'a CheckReport,
    skills: &'a ResolvedSkills,
}

impl<'a> ExecutionContext<'a> {
    /// Unique admitted execution identity.
    #[must_use]
    pub const fn execution_id(self) -> ExecutionId {
        self.execution_id
    }

    /// Root trace identity.
    #[must_use]
    pub const fn trace_id(self) -> TraceId {
        self.trace_id
    }

    /// Immutable byte world.
    #[must_use]
    pub const fn snapshot(self) -> &'a ExecutionSnapshot {
        self.snapshot
    }

    /// Parsed root workflow.
    #[must_use]
    pub const fn workflow(self) -> &'a RawWorkflow {
        self.workflow
    }

    /// Static check report.
    #[must_use]
    pub const fn check(self) -> &'a CheckReport {
        self.check
    }

    /// Root workflow's resolved skills.
    #[must_use]
    pub const fn skills(self) -> &'a ResolvedSkills {
        self.skills
    }
}

/// Typed result envelope returned by the shared execution boundary.
#[derive(Debug)]
#[non_exhaustive]
pub struct ExecutionVerdict<T> {
    execution_id: ExecutionId,
    trace_id: TraceId,
    snapshot_digest: String,
    outcome: T,
}

impl<T> ExecutionVerdict<T> {
    /// Unique admitted execution identity.
    #[must_use]
    pub const fn execution_id(&self) -> ExecutionId {
        self.execution_id
    }

    /// Root trace identity returned without any directory scan.
    #[must_use]
    pub const fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    /// Identity of the exact byte world the runner consumed.
    #[must_use]
    pub fn snapshot_digest(&self) -> &str {
        &self.snapshot_digest
    }

    /// Borrow the injected runtime's typed outcome.
    #[must_use]
    pub const fn outcome(&self) -> &T {
        &self.outcome
    }

    /// Consume the envelope and return the injected runtime's outcome.
    #[must_use]
    pub fn into_outcome(self) -> T {
        self.outcome
    }
}

fn parse(logical_path: &str, text: &str) -> Result<RawWorkflow, ExecutionError> {
    nika_schema::parse(
        text,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .map_err(|error| ExecutionError::Parse {
        logical_path: logical_path.to_owned(),
        detail: error.to_string(),
    })
}

fn validate_skills(
    snapshot: &ExecutionSnapshot,
    logical_path: &str,
    workflow: &RawWorkflow,
) -> Result<ResolvedSkills, ExecutionError> {
    let mut reader = |authored: &str| snapshot.resolve_text(logical_path, authored);
    let resolved = nika_schema::resolve_skills(workflow, &mut reader);
    if resolved.findings.is_empty() {
        return Ok(resolved);
    }
    Err(ExecutionError::SkillCheckFailed {
        workflow: logical_path.to_owned(),
        findings: resolved
            .findings
            .iter()
            .map(nika_schema::SkillFinding::row)
            .collect(),
    })
}

fn validate_child_worlds(snapshot: &ExecutionSnapshot, root: &str) -> Result<(), ExecutionError> {
    for unit in snapshot.units() {
        if unit.logical_path() == root || unit.kind() != SnapshotUnitKind::Child {
            continue;
        }
        let text = unit.text().ok_or_else(|| ExecutionError::NonUtf8 {
            logical_path: unit.logical_path().to_owned(),
        })?;
        let workflow = parse(unit.logical_path(), text)?;
        let mut reader = |path: &str| {
            snapshot
                .text(path)
                .map(str::to_owned)
                .ok_or_else(|| format!("captured world has no unit `{path}`"))
        };
        let report = nika_check::check_composed(&workflow, unit.logical_path(), &mut reader);
        if !report.is_clean() {
            return Err(ExecutionError::CheckFailed {
                findings: report_findings(unit.logical_path(), &report),
            });
        }
        validate_skills(snapshot, unit.logical_path(), &workflow)?;
    }
    Ok(())
}

fn report_findings(logical_path: &str, report: &CheckReport) -> Vec<String> {
    report
        .findings
        .iter()
        .map(|finding| format!("{logical_path}: {}", finding.message))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;
    use crate::snapshot::ByteSource;

    struct CountingSource {
        reads: Cell<usize>,
        files: BTreeMap<String, Vec<u8>>,
    }

    impl ByteSource for CountingSource {
        fn read(&self, logical_path: &str, _limit: usize) -> Result<Vec<u8>, ExecutionError> {
            self.reads.set(self.reads.get() + 1);
            self.files
                .get(logical_path)
                .cloned()
                .ok_or_else(|| ExecutionError::MissingUnit {
                    logical_path: logical_path.to_owned(),
                })
        }
    }

    #[test]
    fn admission_and_execution_read_zero_sources_after_capture() {
        let workflow = b"nika: root\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \".\" }\n";
        let source = CountingSource {
            reads: Cell::new(0),
            files: BTreeMap::from([("root.nika.yaml".to_owned(), workflow.to_vec())]),
        };
        let snapshot = ExecutionSnapshot::capture_from(
            &source,
            Path::new("root.nika.yaml"),
            std::iter::empty::<&Path>(),
            SnapshotLimits::default(),
        )
        .expect("capture");
        let reads_at_boundary = source.reads.get();
        let admitted = ExecutionService::admit_snapshot(snapshot).expect("admit captured world");
        assert_eq!(source.reads.get(), reads_at_boundary, "check never reopens");
        let verdict = ExecutionService::default().execute(admitted, read_root);
        assert_eq!(source.reads.get(), reads_at_boundary, "run never reopens");
        assert_eq!(
            verdict.outcome().as_deref(),
            Some(std::str::from_utf8(workflow).expect("utf8"))
        );
    }

    fn read_root(cx: ExecutionContext<'_>) -> Option<String> {
        cx.snapshot().text("root.nika.yaml").map(str::to_owned)
    }
}
