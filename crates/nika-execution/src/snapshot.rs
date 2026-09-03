// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::borrow::Cow;
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

// This is a transport invariant rather than an operating-system PATH_MAX.
// It bounds every normalized identity before the snapshot owns that metadata.
const MAX_LOGICAL_PATH_BYTES: usize = 4 * 1024;
const SHA256_HEX_BYTES: usize = 64;
const MAX_JSON_STRING_EXPANSION: usize = 6;
const WIRE_SNAPSHOT_OVERHEAD_BYTES: usize =
    r#"{"format_version":1,"root":"","digest":"","units":[]}"#.len();
const WIRE_UNIT_OVERHEAD_BYTES: usize = r#"{"path":"","kind":0,"digest":"","bytes_hex":""}"#.len();

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
    /// Construct explicit depth, count, per-unit, and aggregate decoded-body ceilings.
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

    /// Maximum decoded unit-body bytes across the snapshot.
    #[must_use]
    pub const fn max_total_bytes(self) -> usize {
        self.total_bytes
    }

    // Maximum canonical JSON transport size for a world under these decoded
    // limits. Saturation keeps caller-supplied `usize::MAX` ceilings monotonic
    // instead of wrapping them into a small, fail-open transport allowance.
    fn max_encoded_bytes(self) -> usize {
        let escaped_path = MAX_LOGICAL_PATH_BYTES.saturating_mul(MAX_JSON_STRING_EXPANSION);
        let unit_metadata = WIRE_UNIT_OVERHEAD_BYTES
            .saturating_add(escaped_path)
            .saturating_add(SHA256_HEX_BYTES);

        WIRE_SNAPSHOT_OVERHEAD_BYTES
            .saturating_add(escaped_path)
            .saturating_add(SHA256_HEX_BYTES)
            .saturating_add(self.total_bytes.saturating_mul(2))
            .saturating_add(self.units.saturating_mul(unit_metadata))
            .saturating_add(self.units.saturating_sub(1))
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
        builder.capture_mcp_registry_if_named()?;
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
    /// The default execution ceilings are applied before hexadecimal unit
    /// bodies are decoded. Use [`Self::decode_with_limits`] when an adapter
    /// owns stricter ceilings.
    ///
    /// # Errors
    ///
    /// Returns a typed size or metadata limit error before allocating an
    /// oversized world, [`ExecutionError::UnsupportedSnapshotFormat`] for an
    /// unknown version, or [`ExecutionError::SnapshotStructureMismatch`] /
    /// [`ExecutionError::UnitDigestMismatch`] / [`ExecutionError::SnapshotDigestMismatch`]
    /// when the payload is truncated, mis-typed, or tampered.
    pub fn decode(text: &str) -> Result<Self, ExecutionError> {
        Self::decode_with_limits(text, SnapshotLimits::default())
    }

    /// Rebuild an encoded snapshot under explicit resource ceilings.
    ///
    /// `max_total_bytes` continues to mean the sum of decoded unit bodies (16
    /// MiB by default). The accepted UTF-8 JSON transport is separately bounded
    /// before Serde allocation by a deterministic ceiling large enough for the
    /// canonical [`Self::encode`] representation: hexadecimal bodies can take
    /// twice their decoded size, and JSON structure and bounded metadata add
    /// further bytes. The decoded total and encoded transport sizes are related
    /// bounds, not equal measurements.
    ///
    /// Count, metadata, per-unit, and aggregate decoded-byte limits are checked
    /// before unit bodies are allocated. Readmission subsequently re-applies
    /// the rooted dependency-depth bound and all semantic checks to owned bytes.
    ///
    /// # Errors
    ///
    /// Returns the typed limit variants of [`ExecutionError`] for an
    /// oversized encoded world, [`ExecutionError::UnsupportedSnapshotFormat`]
    /// for an unknown version, or a typed structure/digest mismatch when the
    /// payload is malformed or tampered.
    pub fn decode_with_limits(text: &str, limits: SnapshotLimits) -> Result<Self, ExecutionError> {
        let encoded_limit = limits.max_encoded_bytes();
        if text.len() > encoded_limit {
            return Err(ExecutionError::EncodedSnapshotSizeLimit {
                limit: encoded_limit,
            });
        }

        let wire: BorrowedWireSnapshot<'_> =
            serde_json::from_str(text).map_err(|_| ExecutionError::SnapshotStructureMismatch)?;
        if wire.format_version != SNAPSHOT_FORMAT_VERSION {
            return Err(ExecutionError::UnsupportedSnapshotFormat {
                found: wire.format_version,
                expected: SNAPSHOT_FORMAT_VERSION,
            });
        }
        if wire.units.len() > limits.units {
            return Err(ExecutionError::UnitCountLimit {
                limit: limits.units,
            });
        }
        preflight_wire_metadata_and_bodies(&wire, limits)?;

        let mut units = BTreeMap::new();
        for unit in wire.units {
            let kind = SnapshotUnitKind::from_tag(unit.kind)?;
            let bytes = decode_preflighted_hex_bytes(&unit.bytes_hex)?;
            let captured = CapturedUnit::new(unit.path.into_owned(), kind, bytes);
            if captured.digest != unit.digest.as_ref() {
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
            root: wire.root.into_owned(),
            units,
            digest: wire.digest.into_owned(),
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

    /// The MCP registry and its pins ride the captured world whenever a
    /// captured workflow names an `mcp:` tool: the admission check inside
    /// the snapshot reads the servers the project configured through the
    /// same reader, so a configured lane is reachable at run time and an
    /// unconfigured one is refused by name (#1374). Decided from the texts
    /// already captured (the root is read once, never twice); an absent
    /// file is not packed — the check then says so, honestly.
    fn capture_mcp_registry_if_named(&mut self) -> Result<(), ExecutionError> {
        let names_mcp = self
            .workflow_texts()?
            .iter()
            .any(|(_, text)| text.contains("mcp:"));
        if !names_mcp {
            return Ok(());
        }
        for logical in MCP_REGISTRY_UNITS {
            if self.units.contains_key(*logical) {
                continue;
            }
            let Ok(bytes) = self.read_bounded(logical) else {
                continue;
            };
            self.record_authored("<mcp>", logical, logical)?;
            self.insert(logical, SnapshotUnitKind::Import, bytes)?;
        }
        Ok(())
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

/// The project's MCP registry and the pins the operator approved: packed
/// as imports when a captured workflow names an `mcp:` tool.
const MCP_REGISTRY_UNITS: &[&str] = &[".nika/mcp_servers.json", ".nika/mcp_pins.json"];

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
    let normalized = parts.join("/");
    enforce_metadata_limit("logical path", normalized.len(), MAX_LOGICAL_PATH_BYTES)?;
    Ok(normalized)
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

#[derive(serde::Deserialize)]
struct BorrowedWireSnapshot<'a> {
    format_version: u32,
    #[serde(borrow)]
    root: Cow<'a, str>,
    #[serde(borrow)]
    digest: Cow<'a, str>,
    #[serde(borrow)]
    units: Vec<BorrowedWireUnit<'a>>,
}

#[derive(serde::Deserialize)]
struct BorrowedWireUnit<'a> {
    #[serde(borrow)]
    path: Cow<'a, str>,
    kind: u8,
    #[serde(borrow)]
    digest: Cow<'a, str>,
    #[serde(borrow)]
    bytes_hex: Cow<'a, str>,
}

fn encode_hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn decode_preflighted_hex_bytes(hex: &str) -> Result<Vec<u8>, ExecutionError> {
    let byte_len = hex.len() / 2;
    let mut bytes = Vec::with_capacity(byte_len);
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

fn preflight_wire_metadata_and_bodies(
    wire: &BorrowedWireSnapshot<'_>,
    limits: SnapshotLimits,
) -> Result<(), ExecutionError> {
    enforce_metadata_limit("root", wire.root.len(), MAX_LOGICAL_PATH_BYTES)?;
    preflight_digest("snapshot digest", &wire.digest)?;

    let mut total_bytes = 0usize;
    for unit in &wire.units {
        enforce_metadata_limit("unit path", unit.path.len(), MAX_LOGICAL_PATH_BYTES)?;
        preflight_digest("unit digest", &unit.digest)?;
        let byte_len = decoded_hex_len(&unit.bytes_hex)?;
        if byte_len > limits.unit_bytes {
            return Err(ExecutionError::UnitSizeLimit {
                logical_path: unit.path.to_string(),
                limit: limits.unit_bytes,
            });
        }
        total_bytes = total_bytes
            .checked_add(byte_len)
            .ok_or(ExecutionError::TotalSizeLimit {
                limit: limits.total_bytes,
            })?;
        if total_bytes > limits.total_bytes {
            return Err(ExecutionError::TotalSizeLimit {
                limit: limits.total_bytes,
            });
        }
    }
    Ok(())
}

fn enforce_metadata_limit(
    field: &'static str,
    len: usize,
    limit: usize,
) -> Result<(), ExecutionError> {
    if len > limit {
        return Err(ExecutionError::SnapshotMetadataLimit { field, limit });
    }
    Ok(())
}

fn preflight_digest(field: &'static str, digest: &str) -> Result<(), ExecutionError> {
    enforce_metadata_limit(field, digest.len(), SHA256_HEX_BYTES)?;
    if digest.len() != SHA256_HEX_BYTES
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ExecutionError::SnapshotStructureMismatch);
    }
    Ok(())
}

fn encoded_hex_byte_len(hex: &str) -> Result<usize, ExecutionError> {
    if !hex.len().is_multiple_of(2) {
        return Err(ExecutionError::SnapshotStructureMismatch);
    }
    Ok(hex.len() / 2)
}

fn decoded_hex_len(hex: &str) -> Result<usize, ExecutionError> {
    let byte_len = encoded_hex_byte_len(hex)?;
    if !hex
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(ExecutionError::SnapshotStructureMismatch);
    }
    Ok(byte_len)
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

    /// A captured world that names an `mcp:` tool carries the project's MCP
    /// registry and pins as units — the admission check inside it sees the
    /// configured servers; a world without `mcp:` carries none (#1374).
    #[test]
    fn the_mcp_registry_rides_the_captured_world_when_a_workflow_names_a_server() {
        let dir = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(dir.path().join(".nika")).expect(".nika");
        std::fs::write(
            dir.path().join(".nika/mcp_servers.json"),
            b"{\"mcp_servers_format\":1,\"servers\":{\"sandbox\":{\"command\":[\"true\"]}}}",
        )
        .expect("registry");
        std::fs::write(
            dir.path().join(".nika/mcp_pins.json"),
            b"{\"sandbox\":{\"echo\":\"0000\"}}",
        )
        .expect("pins");
        std::fs::write(
            dir.path().join("mcp.nika.yaml"),
            b"nika: mcp\npermits:\n  tools: [\"mcp:sandbox/echo\"]\ntasks:\n  call:\n    invoke:\n      tool: \"mcp:sandbox/echo\"\n      args: { text: hi }\n",
        )
        .expect("workflow");
        std::fs::write(
            dir.path().join("plain.nika.yaml"),
            b"nika: plain\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 4 }\n",
        )
        .expect("workflow");
        let project = nika_fs::OwnedDir::open(dir.path()).expect("open");
        let limits = SnapshotLimits::default();
        let with_mcp = ExecutionSnapshot::capture(&project, Path::new("mcp.nika.yaml"), limits)
            .expect("captured");
        assert!(
            with_mcp
                .units()
                .any(|u| u.logical_path() == ".nika/mcp_servers.json"),
            "the registry rides the world: {:?}",
            with_mcp
                .units()
                .map(|u| u.logical_path().to_owned())
                .collect::<Vec<_>>()
        );
        assert!(
            with_mcp
                .units()
                .any(|u| u.logical_path() == ".nika/mcp_pins.json")
        );
        with_mcp
            .revalidate(limits)
            .expect("readmits whole with the registry");
        let plain = ExecutionSnapshot::capture(&project, Path::new("plain.nika.yaml"), limits)
            .expect("captured");
        assert!(
            !plain
                .units()
                .any(|u| u.logical_path().starts_with(".nika/")),
            "no mcp: no registry"
        );
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

    #[test]
    fn encoded_snapshot_round_trip_preserves_json_escaped_paths() {
        let root = "quoted\"root.nika.yaml".to_owned();
        let unit = CapturedUnit::new(root.clone(), SnapshotUnitKind::Root, Vec::new());
        let units = BTreeMap::from([(root.clone(), unit)]);
        let snapshot = ExecutionSnapshot {
            format_version: SNAPSHOT_FORMAT_VERSION,
            digest: snapshot_digest(&root, &units),
            root,
            units,
        };

        let encoded = snapshot.encode().expect("encode escaped path");
        let decoded = ExecutionSnapshot::decode(&encoded).expect("decode escaped path");
        assert!(same_snapshot(&snapshot, &decoded));
    }

    #[test]
    fn encoded_envelope_is_bounded_before_json_deserialization() {
        let limits = SnapshotLimits::new(0, 0, 0, 0);
        let encoded_limit = limits.max_encoded_bytes();
        let oversized = " ".repeat(encoded_limit + 1);

        assert_eq!(
            SnapshotLimits::default().max_total_bytes(),
            16 * 1024 * 1024
        );
        assert!(SnapshotLimits::default().max_encoded_bytes() > 16 * 1024 * 1024);

        assert!(matches!(
            ExecutionSnapshot::decode_with_limits(&oversized, limits),
            Err(ExecutionError::EncodedSnapshotSizeLimit { limit })
                if limit == encoded_limit
        ));
    }

    #[test]
    fn root_and_unit_path_metadata_are_independently_bounded() {
        let oversized_root = serde_json::json!({
            "format_version": SNAPSHOT_FORMAT_VERSION,
            "root": "r".repeat(MAX_LOGICAL_PATH_BYTES + 1),
            "digest": "0".repeat(SHA256_HEX_BYTES),
            "units": [],
        })
        .to_string();
        assert!(matches!(
            ExecutionSnapshot::decode(&oversized_root),
            Err(ExecutionError::SnapshotMetadataLimit { field: "root", limit })
                if limit == MAX_LOGICAL_PATH_BYTES
        ));

        let oversized_path = serde_json::json!({
            "format_version": SNAPSHOT_FORMAT_VERSION,
            "root": "root.nika.yaml",
            "digest": "0".repeat(SHA256_HEX_BYTES),
            "units": [{
                "path": "p".repeat(MAX_LOGICAL_PATH_BYTES + 1),
                "kind": 0,
                "digest": "0".repeat(SHA256_HEX_BYTES),
                "bytes_hex": "",
            }],
        })
        .to_string();
        assert!(matches!(
            ExecutionSnapshot::decode(&oversized_path),
            Err(ExecutionError::SnapshotMetadataLimit { field: "unit path", limit })
                if limit == MAX_LOGICAL_PATH_BYTES
        ));
    }

    #[test]
    fn excessive_unit_count_and_hex_fail_with_typed_limits() {
        let unit = serde_json::json!({
            "path": "root.nika.yaml",
            "kind": 0,
            "digest": sha256_hex(&[]),
            "bytes_hex": "",
        });
        let excessive_count = serde_json::json!({
            "format_version": SNAPSHOT_FORMAT_VERSION,
            "root": "root.nika.yaml",
            "digest": "0".repeat(SHA256_HEX_BYTES),
            "units": [unit.clone(), unit],
        })
        .to_string();
        let count_limits = SnapshotLimits::new(0, 1, usize::MAX, usize::MAX);
        assert!(matches!(
            ExecutionSnapshot::decode_with_limits(&excessive_count, count_limits),
            Err(ExecutionError::UnitCountLimit { limit: 1 })
        ));

        let excessive_hex = serde_json::json!({
            "format_version": SNAPSHOT_FORMAT_VERSION,
            "root": "root.nika.yaml",
            "digest": "0".repeat(SHA256_HEX_BYTES),
            "units": [{
                "path": "root.nika.yaml",
                "kind": 0,
                "digest": sha256_hex(&[0, 0]),
                "bytes_hex": "0000",
            }],
        })
        .to_string();
        let body_limits = SnapshotLimits::new(0, 1, 1, usize::MAX);
        assert!(matches!(
            ExecutionSnapshot::decode_with_limits(&excessive_hex, body_limits),
            Err(ExecutionError::UnitSizeLimit { limit: 1, .. })
        ));
    }

    #[test]
    fn malformed_hex_and_digest_metadata_fail_without_truncation() {
        let malformed_hex = serde_json::json!({
            "format_version": SNAPSHOT_FORMAT_VERSION,
            "root": "root.nika.yaml",
            "digest": "0".repeat(SHA256_HEX_BYTES),
            "units": [{
                "path": "root.nika.yaml",
                "kind": 0,
                "digest": sha256_hex(&[0]),
                "bytes_hex": "0G",
            }],
        })
        .to_string();
        assert!(matches!(
            ExecutionSnapshot::decode(&malformed_hex),
            Err(ExecutionError::SnapshotStructureMismatch)
        ));

        let tampered_unit_digest = serde_json::json!({
            "format_version": SNAPSHOT_FORMAT_VERSION,
            "root": "root.nika.yaml",
            "digest": "0".repeat(SHA256_HEX_BYTES),
            "units": [{
                "path": "root.nika.yaml",
                "kind": 0,
                "digest": "f".repeat(SHA256_HEX_BYTES),
                "bytes_hex": "",
            }],
        })
        .to_string();
        assert!(matches!(
            ExecutionSnapshot::decode(&tampered_unit_digest),
            Err(ExecutionError::UnitDigestMismatch { .. })
        ));

        let oversized_digest = serde_json::json!({
            "format_version": SNAPSHOT_FORMAT_VERSION,
            "root": "root.nika.yaml",
            "digest": "f".repeat(SHA256_HEX_BYTES + 1),
            "units": [],
        })
        .to_string();
        assert!(matches!(
            ExecutionSnapshot::decode(&oversized_digest),
            Err(ExecutionError::SnapshotMetadataLimit {
                field: "snapshot digest",
                limit: SHA256_HEX_BYTES,
            })
        ));
    }

    #[test]
    fn encoded_limit_arithmetic_saturates_instead_of_wrapping() {
        let limits = SnapshotLimits::new(usize::MAX, usize::MAX, usize::MAX, usize::MAX);
        assert_eq!(limits.max_encoded_bytes(), usize::MAX);

        let snapshot = valid_snapshot();
        let encoded = snapshot.encode().expect("encode");
        let decoded = ExecutionSnapshot::decode_with_limits(&encoded, limits)
            .expect("saturated limits remain permissive");
        assert!(same_snapshot(&snapshot, &decoded));
    }

    #[test]
    fn encoded_snapshot_limits_fail_with_typed_errors_before_readmission() {
        let snapshot = valid_snapshot();
        let encoded = snapshot.encode().expect("encode");

        let count = SnapshotLimits::new(64, 0, usize::MAX, usize::MAX);
        assert!(matches!(
            ExecutionSnapshot::decode_with_limits(&encoded, count),
            Err(ExecutionError::UnitCountLimit { limit: 0 })
        ));

        let unit = SnapshotLimits::new(64, 1, 1, usize::MAX);
        assert!(matches!(
            ExecutionSnapshot::decode_with_limits(&encoded, unit),
            Err(ExecutionError::UnitSizeLimit { limit: 1, .. })
        ));

        let total = SnapshotLimits::new(64, 1, usize::MAX, 1);
        assert!(matches!(
            ExecutionSnapshot::decode_with_limits(&encoded, total),
            Err(ExecutionError::TotalSizeLimit { limit: 1 })
        ));
    }

    #[test]
    fn aggregate_limit_preflights_every_unit_before_decoding_any_body() {
        let mut snapshot = valid_snapshot();
        let root_bytes = snapshot
            .units
            .get("root.nika.yaml")
            .map(|unit| unit.bytes.len())
            .unwrap_or_default();
        snapshot.units.insert(
            "z.import".to_owned(),
            CapturedUnit::new("z.import".to_owned(), SnapshotUnitKind::Import, vec![7]),
        );
        snapshot.digest = snapshot_digest(&snapshot.root, &snapshot.units);

        let encoded = snapshot.encode().expect("encode");
        let mut wire: serde_json::Value = serde_json::from_str(&encoded).expect("wire json");
        wire["units"][0]["digest"] = serde_json::Value::String("0".repeat(64));
        let tampered = serde_json::to_string(&wire).expect("tampered wire");
        let limits = SnapshotLimits::new(64, 2, usize::MAX, root_bytes);

        assert!(matches!(
            ExecutionSnapshot::decode_with_limits(&tampered, limits),
            Err(ExecutionError::TotalSizeLimit { limit }) if limit == root_bytes
        ));
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
