// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use nika_fs::OwnedDir;
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use sha2::{Digest, Sha256};

use crate::ExecutionError;

/// Current immutable snapshot format.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// Role of one byte-owned unit in an execution snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SnapshotUnitKind {
    /// The workflow admitted by the caller.
    Root,
    /// A transitively invoked workflow.
    Child,
    /// An Agent Skill document.
    Skill,
    /// An opaque import explicitly supplied by the caller.
    Import,
}

impl SnapshotUnitKind {
    fn tag(self) -> u8 {
        match self {
            Self::Root => 0,
            Self::Child => 1,
            Self::Skill => 2,
            Self::Import => 3,
        }
    }

    fn from_tag(tag: u8) -> Result<Self, ExecutionError> {
        match tag {
            0 => Ok(Self::Root),
            1 => Ok(Self::Child),
            2 => Ok(Self::Skill),
            3 => Ok(Self::Import),
            found => Err(ExecutionError::UnsupportedSnapshotFormat {
                found: u32::from(found),
                expected: SNAPSHOT_FORMAT_VERSION,
            }),
        }
    }
}

/// One logical unit and the exact bytes admitted for execution.
#[derive(Clone)]
#[non_exhaustive]
pub struct CapturedUnit {
    logical_path: String,
    kind: SnapshotUnitKind,
    bytes: Arc<[u8]>,
    digest: String,
}

impl std::fmt::Debug for CapturedUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturedUnit")
            .field("logical_path", &self.logical_path)
            .field("kind", &self.kind)
            .field("bytes_len", &self.bytes.len())
            .field("digest", &self.digest)
            .finish()
    }
}

impl CapturedUnit {
    fn new(logical_path: String, kind: SnapshotUnitKind, bytes: Vec<u8>) -> Self {
        let digest = sha256_hex(&bytes);
        Self {
            logical_path,
            kind,
            bytes: Arc::from(bytes),
            digest,
        }
    }

    /// Normalized path relative to the held project root.
    #[must_use]
    pub fn logical_path(&self) -> &str {
        &self.logical_path
    }

    /// Unit role.
    #[must_use]
    pub fn kind(&self) -> SnapshotUnitKind {
        self.kind
    }

    /// Exact admitted bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// UTF-8 view, when the unit is textual.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }

    /// SHA-256 of the exact admitted bytes.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// Resource ceilings applied before an execution can be admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SnapshotLimits {
    depth: usize,
    units: usize,
    unit_bytes: usize,
    total_bytes: usize,
}

impl SnapshotLimits {
    /// Construct explicit depth, count, per-unit, and aggregate ceilings.
    #[must_use]
    pub const fn new(
        max_depth: usize,
        max_units: usize,
        max_unit_bytes: usize,
        max_total_bytes: usize,
    ) -> Self {
        Self {
            depth: max_depth,
            units: max_units,
            unit_bytes: max_unit_bytes,
            total_bytes: max_total_bytes,
        }
    }

    /// Maximum child depth, with the root at depth zero.
    #[must_use]
    pub const fn max_depth(self) -> usize {
        self.depth
    }

    /// Maximum captured unit count.
    #[must_use]
    pub const fn max_units(self) -> usize {
        self.units
    }

    /// Maximum bytes in one unit.
    #[must_use]
    pub const fn max_unit_bytes(self) -> usize {
        self.unit_bytes
    }

    /// Maximum bytes across the snapshot.
    #[must_use]
    pub const fn max_total_bytes(self) -> usize {
        self.total_bytes
    }
}

impl Default for SnapshotLimits {
    fn default() -> Self {
        Self::new(64, 256, 1024 * 1024, 16 * 1024 * 1024)
    }
}

/// Immutable closure of every byte the admitted program may compose from.
#[derive(Clone)]
#[non_exhaustive]
pub struct ExecutionSnapshot {
    format_version: u32,
    root: String,
    units: BTreeMap<String, CapturedUnit>,
    digest: String,
}

impl std::fmt::Debug for ExecutionSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionSnapshot")
            .field("format_version", &self.format_version)
            .field("root", &self.root)
            .field("unit_count", &self.units.len())
            .field("digest", &self.digest)
            .finish()
    }
}

impl ExecutionSnapshot {
    /// Capture a root workflow, its child closure, and every referenced skill.
    ///
    /// All filesystem access is descriptor-relative through `project` and
    /// completes before this function returns.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError`] on path escape, descriptor-relative I/O
    /// failure, malformed workflow, graph defect, or a resource-limit breach.
    pub fn capture(
        project: &OwnedDir,
        root: &Path,
        limits: SnapshotLimits,
    ) -> Result<Self, ExecutionError> {
        Self::capture_from(project, root, std::iter::empty::<&Path>(), limits)
    }

    /// Capture a root already acquired by an interface, plus every dependency
    /// from the held project directory.
    ///
    /// This is the stdin/adaptor admission seam: `root_bytes` become the root
    /// unit without a temporary file or a second read, while children and
    /// skills remain descriptor-relative to `project`.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError`] under the same fail-closed conditions as
    /// [`Self::capture`], including limits applied to the supplied root bytes.
    pub fn capture_root_bytes(
        project: &OwnedDir,
        root: &Path,
        root_bytes: &[u8],
        limits: SnapshotLimits,
    ) -> Result<Self, ExecutionError> {
        let root = normalize_logical(root)?;
        let source = CapturedRoot {
            project,
            logical_path: &root,
            bytes: root_bytes,
        };
        Self::capture_from(
            &source,
            Path::new(&root),
            std::iter::empty::<&Path>(),
            limits,
        )
    }

    /// Capture the workflow closure plus caller-declared opaque imports.
    ///
    /// The current workflow grammar has no `import:` key. This explicit seam
    /// lets project-level imports join the same owned world without inventing
    /// language syntax or a second reader.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError`] under the same fail-closed conditions as
    /// [`Self::capture`], including a duplicate or unreadable import.
    pub fn capture_with_imports<I, P>(
        project: &OwnedDir,
        root: &Path,
        imports: I,
        limits: SnapshotLimits,
    ) -> Result<Self, ExecutionError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        Self::capture_from(project, root, imports, limits)
    }

    pub(crate) fn capture_from<S, I, P>(
        source: &S,
        root: &Path,
        imports: I,
        limits: SnapshotLimits,
    ) -> Result<Self, ExecutionError>
    where
        S: ByteSource,
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let root = normalize_logical(root)?;
        let mut builder = SnapshotBuilder::new(source, limits);
        builder.capture_workflow(&root, SnapshotUnitKind::Root, 0)?;
        for import in imports {
            let authored = import.as_ref().to_string_lossy().into_owned();
            let logical = normalize_logical(import.as_ref())?;
            builder.capture_import(&logical, &authored)?;
        }
        builder.validate_workflows()?;
        builder.capture_pending_skills()?;
        Ok(builder.finish(root))
    }

    /// Revalidate a previously captured world entirely from its owned bytes.
    ///
    /// No filesystem capability participates: the snapshot itself is the
    /// [`ByteSource`]. Rebuilding the rooted closure re-applies path, graph,
    /// role, UTF-8, resource, parser, checker, and skill laws before comparing
    /// every byte and identity with the supplied value.
    pub(crate) fn revalidate(&self, limits: SnapshotLimits) -> Result<(), ExecutionError> {
        if self.format_version != SNAPSHOT_FORMAT_VERSION {
            return Err(ExecutionError::UnsupportedSnapshotFormat {
                found: self.format_version,
                expected: SNAPSHOT_FORMAT_VERSION,
            });
        }
        if normalize_logical(Path::new(&self.root))? != self.root {
            return Err(ExecutionError::SnapshotStructureMismatch);
        }
        for (key, unit) in &self.units {
            if key != unit.logical_path()
                || normalize_logical(Path::new(unit.logical_path()))? != unit.logical_path()
            {
                return Err(ExecutionError::SnapshotStructureMismatch);
            }
            if sha256_hex(unit.bytes()) != unit.digest() {
                return Err(ExecutionError::UnitDigestMismatch {
                    logical_path: unit.logical_path().to_owned(),
                });
            }
        }
        if snapshot_digest(&self.root, &self.units) != self.digest {
            return Err(ExecutionError::SnapshotDigestMismatch);
        }

        let imports = self
            .units()
            .filter(|unit| unit.kind() == SnapshotUnitKind::Import)
            .map(|unit| PathBuf::from(unit.logical_path()))
            .collect::<Vec<_>>();
        let rebuilt = Self::capture_from(self, Path::new(&self.root), &imports, limits)?;
        if !same_snapshot(self, &rebuilt) {
            return Err(ExecutionError::SnapshotStructureMismatch);
        }
        Ok(())
    }

    /// Snapshot format version.
    #[must_use]
    pub const fn format_version(&self) -> u32 {
        self.format_version
    }

    /// Root workflow's normalized logical identity.
    #[must_use]
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Stable SHA-256 identity of format, root, unit roles, paths, and bytes.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Number of captured units.
    #[must_use]
    pub fn len(&self) -> usize {
        self.units.len()
    }

    /// Whether no unit was captured. A valid snapshot is never empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// Find one unit by normalized logical identity.
    #[must_use]
    pub fn unit(&self, logical_path: &str) -> Option<&CapturedUnit> {
        self.units.get(logical_path)
    }

    /// Exact bytes for one unit.
    #[must_use]
    pub fn bytes(&self, logical_path: &str) -> Option<&[u8]> {
        self.unit(logical_path).map(CapturedUnit::bytes)
    }

    /// UTF-8 text for one unit.
    #[must_use]
    pub fn text(&self, logical_path: &str) -> Option<&str> {
        self.unit(logical_path).and_then(CapturedUnit::text)
    }

    /// Deterministic logical-path order over all units.
    #[must_use]
    pub fn units(&self) -> impl ExactSizeIterator<Item = &CapturedUnit> {
        self.units.values()
    }

    /// Encode this world as UTF-8 JSON with hexadecimal unit bytes.
    ///
    /// The payload is the durable transport for a service queue: `write_atomic`
    /// accepts UTF-8, and hexadecimal keeps arbitrary unit bytes inside that
    /// contract. Decode plus [`crate::ExecutionService::readmit_snapshot`]
    /// reconstitutes the world without rereading the filesystem.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError::SnapshotStructureMismatch`] if JSON encoding
    /// fails. A well-formed in-memory snapshot always encodes.
    pub fn encode(&self) -> Result<String, ExecutionError> {
        let wire = WireSnapshot {
            format_version: self.format_version,
            root: self.root.clone(),
            digest: self.digest.clone(),
            units: self
                .units
                .values()
                .map(|unit| WireUnit {
                    path: unit.logical_path.clone(),
                    kind: unit.kind.tag(),
                    digest: unit.digest.clone(),
                    bytes_hex: encode_hex_bytes(unit.bytes()),
                })
                .collect(),
        };
        serde_json::to_string(&wire).map_err(|_| ExecutionError::SnapshotStructureMismatch)
    }

    /// Rebuild a snapshot from [`Self::encode`] output without filesystem I/O.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutionError::UnsupportedSnapshotFormat`] for an unknown
    /// version, or [`ExecutionError::SnapshotStructureMismatch`] /
    /// [`ExecutionError::UnitDigestMismatch`] / [`ExecutionError::SnapshotDigestMismatch`]
    /// when the payload is truncated, mis-typed, or tampered.
    pub fn decode(text: &str) -> Result<Self, ExecutionError> {
        let wire: WireSnapshot =
            serde_json::from_str(text).map_err(|_| ExecutionError::SnapshotStructureMismatch)?;
        if wire.format_version != SNAPSHOT_FORMAT_VERSION {
            return Err(ExecutionError::UnsupportedSnapshotFormat {
                found: wire.format_version,
                expected: SNAPSHOT_FORMAT_VERSION,
            });
        }
        let mut units = BTreeMap::new();
        for unit in wire.units {
            let kind = SnapshotUnitKind::from_tag(unit.kind)?;
            let bytes = decode_hex_bytes(&unit.bytes_hex)?;
            let captured = CapturedUnit::new(unit.path, kind, bytes);
            if captured.digest != unit.digest {
                return Err(ExecutionError::UnitDigestMismatch {
                    logical_path: captured.logical_path.clone(),
                });
            }
            if units
                .insert(captured.logical_path.clone(), captured)
                .is_some()
            {
                return Err(ExecutionError::SnapshotStructureMismatch);
            }
        }
        let snapshot = Self {
            format_version: wire.format_version,
            root: wire.root,
            units,
            digest: wire.digest,
        };
        if snapshot_digest(&snapshot.root, &snapshot.units) != snapshot.digest {
            return Err(ExecutionError::SnapshotDigestMismatch);
        }
        Ok(snapshot)
    }

    pub(crate) fn resolve_text(&self, owner: &str, authored: &str) -> Result<String, String> {
        let logical = resolve_relative(owner, authored).map_err(|e| e.to_string())?;
        self.text(&logical)
            .map(str::to_owned)
            .ok_or_else(|| format!("captured world has no unit `{logical}`"))
    }
}

pub(crate) trait ByteSource {
    fn read(&self, logical_path: &str, limit: usize) -> Result<Vec<u8>, ExecutionError>;
}

struct CapturedRoot<'a> {
    project: &'a OwnedDir,
    logical_path: &'a str,
    bytes: &'a [u8],
}

impl ByteSource for CapturedRoot<'_> {
    fn read(&self, logical_path: &str, limit: usize) -> Result<Vec<u8>, ExecutionError> {
        if logical_path == self.logical_path {
            return Ok(self.bytes.to_vec());
        }
        ByteSource::read(self.project, logical_path, limit)
    }
}

impl ByteSource for OwnedDir {
    fn read(&self, logical_path: &str, limit: usize) -> Result<Vec<u8>, ExecutionError> {
        let mut file = self
            .open_relative(Path::new(logical_path))
            .map_err(|source| ExecutionError::Io {
                logical_path: logical_path.to_owned(),
                source,
            })?;
        let take = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
        let mut bytes = Vec::new();
        file.by_ref()
            .take(take)
            .read_to_end(&mut bytes)
            .map_err(|source| ExecutionError::Io {
                logical_path: logical_path.to_owned(),
                source,
            })?;
        Ok(bytes)
    }
}

impl ByteSource for ExecutionSnapshot {
    fn read(&self, logical_path: &str, limit: usize) -> Result<Vec<u8>, ExecutionError> {
        let bytes = self
            .bytes(logical_path)
            .ok_or_else(|| ExecutionError::MissingUnit {
                logical_path: logical_path.to_owned(),
            })?;
        if bytes.len() > limit {
            return Err(ExecutionError::UnitSizeLimit {
                logical_path: logical_path.to_owned(),
                limit,
            });
        }
        Ok(bytes.to_vec())
    }
}

struct SnapshotBuilder<'a, S> {
    source: &'a S,
    limits: SnapshotLimits,
    units: BTreeMap<String, CapturedUnit>,
    authored: BTreeMap<(String, String), String>,
    pending_skills: Vec<(String, String)>,
    visiting: Vec<String>,
    total_bytes: usize,
}

impl<'a, S: ByteSource> SnapshotBuilder<'a, S> {
    fn new(source: &'a S, limits: SnapshotLimits) -> Self {
        Self {
            source,
            limits,
            units: BTreeMap::new(),
            authored: BTreeMap::new(),
            pending_skills: Vec::new(),
            visiting: Vec::new(),
            total_bytes: 0,
        }
    }

    fn capture_workflow(
        &mut self,
        logical_path: &str,
        kind: SnapshotUnitKind,
        depth: usize,
    ) -> Result<(), ExecutionError> {
        if depth > self.limits.depth {
            return Err(ExecutionError::DepthLimit {
                logical_path: logical_path.to_owned(),
                limit: self.limits.depth,
            });
        }
        if self.visiting.iter().any(|item| item == logical_path) {
            let mut chain = self.visiting.clone();
            chain.push(logical_path.to_owned());
            return Err(ExecutionError::DependencyCycle { chain });
        }
        if let Some(unit) = self.units.get(logical_path) {
            return Self::ensure_kind(unit, kind, logical_path);
        }
        let bytes = self.read_bounded(logical_path)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| ExecutionError::NonUtf8 {
            logical_path: logical_path.to_owned(),
        })?;
        let workflow = parse_workflow(logical_path, text)?;
        self.insert(logical_path, kind, bytes)?;
        self.visiting.push(logical_path.to_owned());
        let result = self.capture_dependencies(logical_path, &workflow, depth);
        self.visiting.pop();
        result
    }

    fn capture_dependencies(
        &mut self,
        owner: &str,
        workflow: &RawWorkflow,
        depth: usize,
    ) -> Result<(), ExecutionError> {
        for task in &workflow.tasks {
            if let RawAction::Invoke(action) = &task.value.action
                && let RawInvokeTarget::Workflow(target) = &action.target
            {
                self.capture_child(owner, &target.value, depth + 1)?;
            }
        }
        for (_, skill) in nika_schema::skill_refs(workflow) {
            let allowed = workflow
                .permits
                .as_ref()
                .is_some_and(|p| p.value.allows_path(&skill.value, false));
            if !allowed {
                return Err(ExecutionError::SkillNotAuthorized {
                    workflow: owner.to_owned(),
                    skill: skill.value.clone(),
                });
            }
            self.pending_skills
                .push((owner.to_owned(), skill.value.clone()));
        }
        Ok(())
    }

    fn capture_child(
        &mut self,
        owner: &str,
        authored: &str,
        depth: usize,
    ) -> Result<(), ExecutionError> {
        if authored.starts_with("registry:") {
            return Err(ExecutionError::RegistryDependency {
                reference: authored.to_owned(),
            });
        }
        let logical = resolve_relative(owner, authored)?;
        self.record_authored(owner, &logical, authored)?;
        self.capture_workflow(&logical, SnapshotUnitKind::Child, depth)
    }

    fn capture_leaf(
        &mut self,
        owner: &str,
        authored: &str,
        kind: SnapshotUnitKind,
    ) -> Result<(), ExecutionError> {
        let logical = resolve_relative(owner, authored)?;
        self.record_authored(owner, &logical, authored)?;
        if let Some(unit) = self.units.get(&logical) {
            return Self::ensure_kind(unit, kind, &logical);
        }
        let bytes = self.read_bounded(&logical)?;
        if kind == SnapshotUnitKind::Skill {
            std::str::from_utf8(&bytes).map_err(|_| ExecutionError::NonUtf8 {
                logical_path: logical.clone(),
            })?;
        }
        self.insert(&logical, kind, bytes)
    }

    fn capture_import(&mut self, logical: &str, authored: &str) -> Result<(), ExecutionError> {
        self.record_authored("<imports>", logical, authored)?;
        if let Some(unit) = self.units.get(logical) {
            return Err(ExecutionError::DuplicateLogicalIdentity {
                logical_path: logical.to_owned(),
                first: format!("{:?}", unit.kind),
                second: authored.to_owned(),
            });
        }
        let bytes = self.read_bounded(logical)?;
        self.insert(logical, SnapshotUnitKind::Import, bytes)
    }

    fn validate_workflows(&self) -> Result<(), ExecutionError> {
        let workflows = self.workflow_texts()?;
        for (logical_path, text) in workflows {
            let workflow = parse_workflow(&logical_path, &text)?;
            let mut reader = |path: &str| {
                self.units
                    .get(path)
                    .and_then(CapturedUnit::text)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("captured world has no workflow `{path}`"))
            };
            let report = nika_check::check_composed(&workflow, &logical_path, &mut reader);
            if !report.is_clean() {
                return Err(ExecutionError::CheckFailed {
                    findings: report_findings(&logical_path, &report),
                });
            }
        }
        Ok(())
    }

    fn workflow_texts(&self) -> Result<Vec<(String, String)>, ExecutionError> {
        self.units
            .values()
            .filter(|unit| {
                matches!(
                    unit.kind(),
                    SnapshotUnitKind::Root | SnapshotUnitKind::Child
                )
            })
            .map(|unit| {
                let text = unit.text().ok_or_else(|| ExecutionError::NonUtf8 {
                    logical_path: unit.logical_path().to_owned(),
                })?;
                Ok((unit.logical_path().to_owned(), text.to_owned()))
            })
            .collect()
    }

    fn capture_pending_skills(&mut self) -> Result<(), ExecutionError> {
        let pending = std::mem::take(&mut self.pending_skills);
        for (owner, authored) in pending {
            self.capture_leaf(&owner, &authored, SnapshotUnitKind::Skill)?;
        }
        Ok(())
    }

    fn read_bounded(&self, logical_path: &str) -> Result<Vec<u8>, ExecutionError> {
        let bytes = self.source.read(logical_path, self.limits.unit_bytes)?;
        if bytes.len() > self.limits.unit_bytes {
            return Err(ExecutionError::UnitSizeLimit {
                logical_path: logical_path.to_owned(),
                limit: self.limits.unit_bytes,
            });
        }
        Ok(bytes)
    }

    fn insert(
        &mut self,
        logical_path: &str,
        kind: SnapshotUnitKind,
        bytes: Vec<u8>,
    ) -> Result<(), ExecutionError> {
        if self.units.len() >= self.limits.units {
            return Err(ExecutionError::UnitCountLimit {
                limit: self.limits.units,
            });
        }
        let total =
            self.total_bytes
                .checked_add(bytes.len())
                .ok_or(ExecutionError::TotalSizeLimit {
                    limit: self.limits.total_bytes,
                })?;
        if total > self.limits.total_bytes {
            return Err(ExecutionError::TotalSizeLimit {
                limit: self.limits.total_bytes,
            });
        }
        self.total_bytes = total;
        self.units.insert(
            logical_path.to_owned(),
            CapturedUnit::new(logical_path.to_owned(), kind, bytes),
        );
        Ok(())
    }

    fn record_authored(
        &mut self,
        owner: &str,
        logical: &str,
        authored: &str,
    ) -> Result<(), ExecutionError> {
        let key = (owner.to_owned(), logical.to_owned());
        if let Some(first) = self.authored.get(&key) {
            if first != authored {
                return Err(ExecutionError::DuplicateLogicalIdentity {
                    logical_path: logical.to_owned(),
                    first: first.clone(),
                    second: authored.to_owned(),
                });
            }
            return Ok(());
        }
        self.authored.insert(key, authored.to_owned());
        Ok(())
    }

    fn ensure_kind(
        unit: &CapturedUnit,
        kind: SnapshotUnitKind,
        logical: &str,
    ) -> Result<(), ExecutionError> {
        if unit.kind == kind {
            return Ok(());
        }
        Err(ExecutionError::DuplicateLogicalIdentity {
            logical_path: logical.to_owned(),
            first: format!("{:?}", unit.kind),
            second: format!("{kind:?}"),
        })
    }

    fn finish(self, root: String) -> ExecutionSnapshot {
        let digest = snapshot_digest(&root, &self.units);
        ExecutionSnapshot {
            format_version: SNAPSHOT_FORMAT_VERSION,
            root,
            units: self.units,
            digest,
        }
    }
}

fn parse_workflow(logical_path: &str, text: &str) -> Result<RawWorkflow, ExecutionError> {
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

fn resolve_relative(owner: &str, authored: &str) -> Result<String, ExecutionError> {
    let target = Path::new(authored);
    if target.is_absolute() {
        return Err(ExecutionError::InvalidLogicalPath {
            path: authored.to_owned(),
        });
    }
    let mut joined = PathBuf::new();
    if let Some(parent) = Path::new(owner).parent() {
        joined.push(parent);
    }
    joined.push(target);
    normalize_logical(&joined)
}

fn normalize_logical(path: &Path) -> Result<String, ExecutionError> {
    let authored = path.to_string_lossy().into_owned();
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| ExecutionError::InvalidLogicalPath {
                        path: authored.clone(),
                    })?;
                parts.push(part.to_owned());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    return Err(ExecutionError::InvalidLogicalPath { path: authored });
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ExecutionError::InvalidLogicalPath { path: authored });
            }
        }
    }
    if parts.is_empty() {
        return Err(ExecutionError::InvalidLogicalPath { path: authored });
    }
    Ok(parts.join("/"))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct WireSnapshot {
    format_version: u32,
    root: String,
    digest: String,
    units: Vec<WireUnit>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct WireUnit {
    path: String,
    kind: u8,
    digest: String,
    bytes_hex: String,
}

fn encode_hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn decode_hex_bytes(hex: &str) -> Result<Vec<u8>, ExecutionError> {
    if !hex.len().is_multiple_of(2)
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ExecutionError::SnapshotStructureMismatch);
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let chars = hex.as_bytes();
    for chunk in chars.chunks_exact(2) {
        let text =
            std::str::from_utf8(chunk).map_err(|_| ExecutionError::SnapshotStructureMismatch)?;
        let byte =
            u8::from_str_radix(text, 16).map_err(|_| ExecutionError::SnapshotStructureMismatch)?;
        bytes.push(byte);
    }
    Ok(bytes)
}

fn snapshot_digest(root: &str, units: &BTreeMap<String, CapturedUnit>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"nika-execution-snapshot\0");
    hasher.update(SNAPSHOT_FORMAT_VERSION.to_be_bytes());
    hash_field(&mut hasher, root.as_bytes());
    for unit in units.values() {
        hasher.update([unit.kind.tag()]);
        hash_field(&mut hasher, unit.logical_path.as_bytes());
        hash_field(&mut hasher, unit.bytes());
    }
    sha256_finish(hasher)
}

fn same_snapshot(left: &ExecutionSnapshot, right: &ExecutionSnapshot) -> bool {
    left.format_version == right.format_version
        && left.root == right.root
        && left.digest == right.digest
        && left.units.len() == right.units.len()
        && left.units.iter().all(|(logical, unit)| {
            right.units.get(logical).is_some_and(|other| {
                unit.logical_path == other.logical_path
                    && unit.kind == other.kind
                    && unit.bytes() == other.bytes()
                    && unit.digest == other.digest
            })
        })
}

fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(bytes);
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    sha256_finish(hasher)
}

fn sha256_finish(hasher: Sha256) -> String {
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

pub(crate) fn report_findings(logical_path: &str, report: &nika_check::CheckReport) -> Vec<String> {
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
    use super::*;

    fn valid_snapshot() -> ExecutionSnapshot {
        let root = "root.nika.yaml".to_owned();
        let unit = CapturedUnit::new(
            root.clone(),
            SnapshotUnitKind::Root,
            b"nika: root\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \".\" }\n"
                .to_vec(),
        );
        let units = BTreeMap::from([(root.clone(), unit)]);
        let digest = snapshot_digest(&root, &units);
        ExecutionSnapshot {
            format_version: SNAPSHOT_FORMAT_VERSION,
            root,
            units,
            digest,
        }
    }

    #[test]
    fn public_readmission_revalidates_owned_bytes_without_a_reader() {
        let snapshot = valid_snapshot();
        let digest = snapshot.digest().to_owned();
        let admitted = crate::ExecutionService::default()
            .readmit_snapshot(snapshot)
            .expect("owned snapshot readmits");

        assert_eq!(admitted.snapshot().digest(), digest);
        assert_eq!(admitted.snapshot().root(), "root.nika.yaml");
    }

    #[test]
    fn readmission_refuses_stale_unit_and_aggregate_identities() {
        let mut stale_unit = valid_snapshot();
        stale_unit
            .units
            .get_mut("root.nika.yaml")
            .expect("root")
            .digest = "0".repeat(64);
        assert!(matches!(
            crate::ExecutionService::default().readmit_snapshot(stale_unit),
            Err(ExecutionError::UnitDigestMismatch { .. })
        ));

        let mut stale_world = valid_snapshot();
        stale_world.digest = "f".repeat(64);
        assert!(matches!(
            crate::ExecutionService::default().readmit_snapshot(stale_world),
            Err(ExecutionError::SnapshotDigestMismatch)
        ));
    }

    #[test]
    fn readmission_refuses_an_owned_but_unreachable_unit() {
        let mut snapshot = valid_snapshot();
        let orphan = CapturedUnit::new(
            "orphan.nika.yaml".to_owned(),
            SnapshotUnitKind::Child,
            b"nika: orphan\npermits: {}\ntasks: {}\n".to_vec(),
        );
        snapshot
            .units
            .insert(orphan.logical_path().to_owned(), orphan);
        snapshot.digest = snapshot_digest(&snapshot.root, &snapshot.units);

        assert!(matches!(
            crate::ExecutionService::default().readmit_snapshot(snapshot),
            Err(ExecutionError::SnapshotStructureMismatch)
        ));
    }

    #[test]
    fn readmission_refuses_an_unknown_snapshot_format() {
        let mut snapshot = valid_snapshot();
        snapshot.format_version = SNAPSHOT_FORMAT_VERSION + 1;

        assert!(matches!(
            crate::ExecutionService::default().readmit_snapshot(snapshot),
            Err(ExecutionError::UnsupportedSnapshotFormat { .. })
        ));
    }

    #[test]
    fn encode_round_trip_preserves_owned_bytes_and_refuses_tampering() {
        let snapshot = valid_snapshot();
        let encoded = snapshot.encode().expect("encode");
        let decoded = ExecutionSnapshot::decode(&encoded).expect("decode");
        assert!(same_snapshot(&snapshot, &decoded));
        crate::ExecutionService::default()
            .readmit_snapshot(decoded)
            .expect("readmit encoded world");

        let tampered = encoded.replace(&snapshot.digest, &"a".repeat(64));
        assert!(matches!(
            ExecutionSnapshot::decode(&tampered),
            Err(ExecutionError::SnapshotDigestMismatch)
        ));
        assert!(ExecutionSnapshot::decode("{").is_err());
    }

    proptest::proptest! {
        #[test]
        fn snapshot_digest_is_independent_of_map_insertion_order(
            left in proptest::collection::vec(proptest::num::u8::ANY, 0..256),
            right in proptest::collection::vec(proptest::num::u8::ANY, 0..256),
        ) {
            let a = CapturedUnit::new("imports/a".to_owned(), SnapshotUnitKind::Import, left);
            let b = CapturedUnit::new("imports/b".to_owned(), SnapshotUnitKind::Import, right);
            let first = BTreeMap::from([
                (a.logical_path().to_owned(), a.clone()),
                (b.logical_path().to_owned(), b.clone()),
            ]);
            let second = BTreeMap::from([
                (b.logical_path().to_owned(), b),
                (a.logical_path().to_owned(), a),
            ]);
            proptest::prop_assert_eq!(
                snapshot_digest("root.nika.yaml", &first),
                snapshot_digest("root.nika.yaml", &second),
            );
        }

        #[test]
        fn dot_segments_do_not_change_logical_identity(
            segments in proptest::collection::vec("[a-z][a-z0-9]{0,7}", 1..8),
        ) {
            let plain = segments.join("/");
            let dotted = format!("./{}", segments.join("/./"));
            proptest::prop_assert_eq!(
                normalize_logical(Path::new(&plain)).expect("plain path"),
                normalize_logical(Path::new(&dotted)).expect("dotted path"),
            );
        }
    }
}
