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
        self.readmit_snapshot(snapshot)
    }

    /// Admit root bytes already captured by an interface, with transitive
    /// dependencies resolved from the held project directory.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError`] under the same fail-closed conditions as
    /// [`Self::admit`].
    pub fn admit_root_bytes(
        &self,
        project: &OwnedDir,
        root: &Path,
        root_bytes: &[u8],
    ) -> Result<AdmittedExecution, ExecutionError> {
        let snapshot =
            ExecutionSnapshot::capture_root_bytes(project, root, root_bytes, self.limits)?;
        self.readmit_snapshot(snapshot)
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
        self.readmit_snapshot(snapshot)
    }

    /// Execute through the original function-pointer runner seam.
    ///
    /// This convenience surface supplies only [`ExecutionContext`] to the
    /// runner. It is not a process sandbox: the runner remains trusted code
    /// and can use ambient capabilities it obtains elsewhere.
    pub fn execute<T>(
        &self,
        admitted: AdmittedExecution,
        runner: for<'a> fn(ExecutionContext<'a>) -> T,
    ) -> ExecutionVerdict<T> {
        let session = self.begin(admitted);
        let outcome = runner(session.context());
        session.complete(outcome)
    }

    /// Begin one execution session over an admitted immutable world.
    ///
    /// Interface adapters keep their request state outside this boundary,
    /// borrow the capability-free [`ExecutionContext`] through
    /// [`ExecutionSession::context`], then return their typed result through
    /// [`ExecutionSession::complete`]. The split makes the service's custody
    /// claim precise: it supplies no filesystem capability or mutable
    /// workflow pathname. It does not claim to sandbox trusted in-process
    /// adapter code from ambient operating-system APIs.
    #[must_use]
    pub fn begin(&self, admitted: AdmittedExecution) -> ExecutionSession {
        let AdmittedExecution {
            execution_id,
            trace_id,
            snapshot,
            workflow,
            check,
            skills,
        } = admitted;
        ExecutionSession {
            execution_id,
            trace_id,
            snapshot,
            workflow,
            check,
            skills,
        }
    }

    /// Readmit a captured snapshot after revalidating its complete owned world.
    ///
    /// This is the service/queue boundary: callers may retain or transport an
    /// [`ExecutionSnapshot`], but a new execution identity is minted only
    /// after the current engine re-applies format, digest, rooted-closure,
    /// parser, checker, and skill validation to those exact bytes. The method
    /// performs no filesystem reads.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError`] when any stored identity, resource ceiling,
    /// dependency role, workflow, check finding, or skill no longer validates.
    pub fn readmit_snapshot(
        &self,
        snapshot: ExecutionSnapshot,
    ) -> Result<AdmittedExecution, ExecutionError> {
        snapshot.revalidate(self.limits)?;
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
#[non_exhaustive]
pub struct AdmittedExecution {
    execution_id: ExecutionId,
    trace_id: TraceId,
    snapshot: ExecutionSnapshot,
    workflow: RawWorkflow,
    check: CheckReport,
    skills: ResolvedSkills,
}

impl std::fmt::Debug for AdmittedExecution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdmittedExecution")
            .field("execution_id", &self.execution_id)
            .field("trace_id", &self.trace_id)
            .field("snapshot_digest", &self.snapshot.digest())
            .finish_non_exhaustive()
    }
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
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct ExecutionContext<'a> {
    execution_id: ExecutionId,
    trace_id: TraceId,
    snapshot: &'a ExecutionSnapshot,
    workflow: &'a RawWorkflow,
    check: &'a CheckReport,
    skills: &'a ResolvedSkills,
}

#[allow(clippy::elidable_lifetime_names)] // Preserve the locked public trait shape.
impl<'a> std::fmt::Debug for ExecutionContext<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionContext")
            .field("execution_id", &self.execution_id)
            .field("trace_id", &self.trace_id)
            .field("snapshot_digest", &self.snapshot.digest())
            .finish_non_exhaustive()
    }
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

/// Owned execution session held between admission and a typed verdict.
///
/// The session owns the immutable definition world. Interface-specific
/// request state and effect capabilities deliberately remain outside it.
#[non_exhaustive]
pub struct ExecutionSession {
    execution_id: ExecutionId,
    trace_id: TraceId,
    snapshot: ExecutionSnapshot,
    workflow: RawWorkflow,
    check: CheckReport,
    skills: ResolvedSkills,
}

impl std::fmt::Debug for ExecutionSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionSession")
            .field("execution_id", &self.execution_id)
            .field("trace_id", &self.trace_id)
            .field("snapshot_digest", &self.snapshot.digest())
            .finish_non_exhaustive()
    }
}

impl ExecutionSession {
    /// Borrow the complete admitted definition world without a filesystem
    /// capability, root path, or reader callback.
    #[must_use]
    pub const fn context(&self) -> ExecutionContext<'_> {
        ExecutionContext {
            execution_id: self.execution_id,
            trace_id: self.trace_id,
            snapshot: &self.snapshot,
            workflow: &self.workflow,
            check: &self.check,
            skills: &self.skills,
        }
    }

    /// Consume the session and bind one typed adapter result to its exact
    /// execution, trace, and snapshot identities.
    #[must_use]
    pub fn complete<T>(self, outcome: T) -> ExecutionVerdict<T> {
        ExecutionVerdict {
            execution_id: self.execution_id,
            trace_id: self.trace_id,
            snapshot_digest: self.snapshot.digest().to_owned(),
            outcome,
        }
    }
}

/// Typed result envelope returned by the shared execution boundary.
#[non_exhaustive]
pub struct ExecutionVerdict<T> {
    execution_id: ExecutionId,
    trace_id: TraceId,
    snapshot_digest: String,
    outcome: T,
}

impl<T: std::fmt::Debug> std::fmt::Debug for ExecutionVerdict<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionVerdict")
            .field("execution_id", &self.execution_id)
            .field("trace_id", &self.trace_id)
            .field("snapshot_digest", &self.snapshot_digest)
            .finish_non_exhaustive()
    }
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
        detail: error.diagnostic().to_string(),
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
        .map(|finding| match finding.code.as_deref() {
            Some(code) => format!("{code} {logical_path}: {}", finding.message),
            None => format!("{logical_path}: {}", finding.message),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;
    use crate::snapshot::ByteSource;

    struct CountingSource {
        reads: RefCell<Vec<String>>,
        files: BTreeMap<String, Vec<u8>>,
    }

    impl ByteSource for CountingSource {
        fn read(&self, logical_path: &str, _limit: usize) -> Result<Vec<u8>, ExecutionError> {
            self.reads.borrow_mut().push(logical_path.to_owned());
            self.files
                .get(logical_path)
                .cloned()
                .ok_or_else(|| ExecutionError::MissingUnit {
                    logical_path: logical_path.to_owned(),
                })
        }
    }

    #[test]
    fn digest_check_skills_and_run_read_zero_sources_after_capture() {
        let workflow = b"nika: root\nmodel: mock/echo\npermits:\n  fs:\n    read: [\"skills/review/SKILL.md\"]\ntasks:\n  review:\n    agent: { prompt: \"review\", skills: [\"skills/review/SKILL.md\"] }\n";
        let skill = b"---\nname: review\ndescription: Review code.\n---\nOriginal.\n";
        let import = b"policy-v1";
        let source = CountingSource {
            reads: RefCell::new(Vec::new()),
            files: BTreeMap::from([
                ("root.nika.yaml".to_owned(), workflow.to_vec()),
                ("imports/policy.bin".to_owned(), import.to_vec()),
                ("skills/review/SKILL.md".to_owned(), skill.to_vec()),
            ]),
        };
        let snapshot = ExecutionSnapshot::capture_from(
            &source,
            Path::new("root.nika.yaml"),
            [Path::new("imports/policy.bin")],
            SnapshotLimits::default(),
        )
        .expect("capture");
        let reads_at_boundary = source.reads.borrow().clone();
        assert_eq!(
            reads_at_boundary,
            [
                "root.nika.yaml",
                "imports/policy.bin",
                "skills/review/SKILL.md"
            ]
        );
        let digest = snapshot.digest().to_owned();
        assert_eq!(source.reads.borrow().as_slice(), reads_at_boundary);
        let admitted = ExecutionService::default()
            .readmit_snapshot(snapshot)
            .expect("admit captured world");
        assert_eq!(
            source.reads.borrow().as_slice(),
            reads_at_boundary,
            "parse, composed check, and skill resolution never reopen"
        );
        let verdict = ExecutionService::default().execute(admitted, read_owned_world);
        assert_eq!(
            source.reads.borrow().as_slice(),
            reads_at_boundary,
            "run never reopens"
        );
        assert_eq!(verdict.snapshot_digest(), digest);
        assert_eq!(
            verdict.outcome(),
            &[
                std::str::from_utf8(workflow)
                    .expect("workflow utf8")
                    .to_owned(),
                std::str::from_utf8(skill).expect("skill utf8").to_owned(),
                String::from_utf8(import.to_vec()).expect("import utf8"),
            ]
        );
    }

    fn read_owned_world(cx: ExecutionContext<'_>) -> Vec<String> {
        [
            "root.nika.yaml",
            "skills/review/SKILL.md",
            "imports/policy.bin",
        ]
        .into_iter()
        .filter_map(|path| cx.snapshot().text(path).map(str::to_owned))
        .collect()
    }

    #[test]
    fn execution_session_keeps_adapter_request_outside_the_context() {
        let workflow = b"nika: root\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \".\" }\n";
        let source = CountingSource {
            reads: RefCell::new(Vec::new()),
            files: BTreeMap::from([("root.nika.yaml".to_owned(), workflow.to_vec())]),
        };
        let snapshot = ExecutionSnapshot::capture_from(
            &source,
            Path::new("root.nika.yaml"),
            std::iter::empty::<&Path>(),
            SnapshotLimits::default(),
        )
        .expect("capture");
        let admitted = ExecutionService::default()
            .readmit_snapshot(snapshot)
            .expect("admit captured world");
        let execution_id = admitted.execution_id();
        let trace_id = admitted.trace_id();
        let digest = admitted.snapshot().digest().to_owned();
        let adapter_request = String::from("model=mock/echo;max_cost_usd=0");
        let session = ExecutionService::default().begin(admitted);
        let context = session.context();
        assert_eq!(context.execution_id(), execution_id);
        assert_eq!(context.trace_id(), trace_id);
        assert_eq!(context.snapshot().digest(), digest);
        let verdict = session.complete(adapter_request);

        assert_eq!(verdict.execution_id(), execution_id);
        assert_eq!(verdict.trace_id(), trace_id);
        assert_eq!(verdict.snapshot_digest(), digest);
        assert_eq!(verdict.outcome(), "model=mock/echo;max_cost_usd=0");
    }
}
