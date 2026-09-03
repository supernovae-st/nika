// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project file `nika.yaml` — the workflow file's sibling
//! (D-2026-08-11-N5), discovered repo-root-ward from the CWD the way
//! git finds `.git`. OPTIONAL end to end: an absent file is pure
//! defaults, zero ceremony — a run must NEVER refuse because the file
//! is missing or no repo frames the CWD. A PRESENT file is a closed
//! grammar: an unknown key anywhere is a named refusal, never a
//! silent drop (a typo'd knob that no-ops is a lie the operator
//! cannot see), and malformed YAML refuses WITH its line.
//!
//! The ladder per key, locked: the invocation flag / env var (where
//! one exists) wins · then this file · then the built-in default.
//!
//! - `ceiling:` — a DEFAULT per-run spend ceiling (USD), never a hard
//!   floor: the per-invocation `--max-cost-usd` flag ALWAYS wins.
//! - `traces.keep:` — the retention age cap (`30d`), the same
//!   semantics the three `NIKA_TRACE_*` env vars carry; each env var
//!   wins its knob when set.
//! - `registry.floor:` — a GATE, not a default: artifacts below the
//!   tier are refused. It composes by MAX with the operator's own
//!   `~/.nika/registry/policy.toml` (a project can raise its bar,
//!   never lower the operator's — the only non-downgrade read).
//! - `arm:` — parsed + shape-validated ONLY. The cadence/arm RUNTIME
//!   is another arc (`crates/nika-cadence` owns the law pass); this
//!   module validates the entry shape against the registry's THIRTEEN
//!   keys and exposes the entries so that arc can consume them later.
//!   Five are judged here (`workflow` · `cadence` · `où` · `plafond` ·
//!   `manqué`); the cadence arc's eight ride verbatim, their values
//!   judged THERE (law 8 — deux parseurs, jamais en désaccord).
//!
//! NO `seat`, NO `profile`, NO permits here — the portability test
//! (D-2026-08-10-N2) rejected them; the file carries exactly five
//! top-level keys. This module NEVER opens a network, never reads an
//! env var, and opens exactly ONE file (the discovery read — the
//! `policy.toml` class of local operator data).

mod parse;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::time::Duration;

pub use parse::parse;

/// The one file name, at the discovered root.
pub const FILE_NAME: &str = "nika.yaml";

/// `^[a-z][a-z0-9-]*$` — the kebab-case resource-name shape, the SAME
/// rule the workflow envelope applies to its own `nika:` (spec 01
/// §`nika`). One grammar for the key across both artifact classes.
#[must_use]
pub fn is_kebab_id(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The envelope line's own indentation, when the document opens on one.
fn envelope_indent(yaml: &str) -> Option<&str> {
    for line in yaml.lines() {
        let stripped = line.trim_start();
        if stripped.is_empty() || stripped.starts_with('#') {
            continue;
        }
        let indent = &line[..line.len() - stripped.len()];
        return stripped
            .strip_prefix("nika:")
            .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
            .map(|_| indent);
    }
    None
}

/// Does this document declare itself a PROJECT rather than a workflow?
///
/// The spec's discriminant (`01-envelope` §The type discriminant),
/// normative and covering every document · a `tasks:` key means WORKFLOW,
/// its absence means PROJECT. Deliberately NOT the filename: a document
/// arrives as a registry blob, an HTTP body, a stdin pipe or a chat
/// paste, and the bytes must still say what they are. Anchored to the
/// envelope's own indent, so a nested `tasks:` never qualifies; a
/// document with no `nika:` envelope is not a nika file and returns
/// false, leaving the workflow envelope's `NIKA-PARSE-002` to own it.
#[must_use]
pub fn is_project_document(yaml: &str) -> bool {
    let Some(indent) = envelope_indent(yaml) else {
        return false;
    };
    !yaml.lines().any(|line| {
        line.strip_prefix(indent)
            .is_some_and(|rest| rest.starts_with("tasks:") && !rest.starts_with("tasks::"))
    })
}

/// The closed top-level key set (D-2026-08-11-N5, verbatim) — public
/// so consumers DERIVE from it instead of retyping it (the
/// `nika_schema::parser::TOP_LEVEL_KEYS` precedent: a mirror validated
/// against itself proves nothing).
pub const TOP_LEVEL_KEYS: &[&str] = &["nika", "ceiling", "arm", "traces", "registry"];

/// The project file's JSON Schema (draft 2020-12 · English descriptions ·
/// the keys the parser reads) — what `nika spec --schema --project` prints
/// and editors validate against. Proven against the parser's closed key sets
/// and enum spellings by the tests, so the two cannot drift apart silently.
pub const PROJECT_SCHEMA_JSON: &str = include_str!("project.schema.json");

/// The starter the founding wizard offers — laid ONLY on an explicit
/// yes (never silently). The examples ride commented: absence IS the
/// defaults, and the file parses to exactly [`Project::default`], a
/// ratchet the tests pin so the example can never drift from the
/// grammar it teaches.
pub const STARTER: &str = "\
# nika.yaml — the project file (OPTIONAL · an absent key is its built-in default).
# Ladder per key: the invocation flag / env var wins · then this file · then the default.
nika: my-project

# ceiling: 0.50     # default max-cost-usd for runs of this project · --max-cost-usd always wins

# traces:
#   keep: 30d       # trace retention · the 3 NIKA_TRACE_* env vars win when set
";

/// The parsed project file. Every knob is an `Option` rung: `None`
/// means « the file does not govern this » and the consumer's ladder
/// falls to the built-in default.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Project {
    /// `nika:` — the project's NAME, kebab-case. Same grammar as a
    /// workflow's `nika:`: the key declares « this is a Nika file », the
    /// value names it. `None` only for a file that declares nothing (a
    /// comments-only or `{}` mapping governs nothing, so it owes no
    /// name); any file that carries a key carries this one.
    ///
    /// It replaced a frozen `v1` tag. The reasoning is the workflow
    /// envelope's, verbatim: a field with ONE legal value is not a
    /// version, so nothing was traded away — and the version marker the
    /// tag pretended to be now lives where it can be fetched and where
    /// an editor already reads it, the `$schema` URL.
    pub name: Option<String>,
    /// `ceiling:` — the default per-run spend ceiling (USD). The flag
    /// wins; this only fills a flag-less invocation.
    pub ceiling: Option<f64>,
    /// `traces:` — the retention rung (env vars win their knobs).
    pub traces: Option<TracesPolicy>,
    /// `registry:` — the provenance GATE (max-composed, never lowered).
    pub registry: Option<RegistryPolicy>,
    /// `arm:` — the team arming registry, validated and INERT here
    /// (the cadence arc consumes the entries; this module schedules
    /// nothing). Private field + slice accessor (FCI-014).
    arm: Vec<ArmEntry>,
}

impl Project {
    /// The armed beats, in file order — validated, never scheduled.
    #[must_use]
    pub fn arm(&self) -> &[ArmEntry] {
        &self.arm
    }
}

/// The `traces:` block — one word today, the retention age cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct TracesPolicy {
    /// `keep:` — a trace older than this exits even under the keep-N
    /// window (the `NIKA_TRACE_MAX_AGE_DAYS` semantics, file-side).
    pub keep: Duration,
}

/// The `registry:` block — the provenance admission floor (a GATE).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RegistryPolicy {
    /// `floor:` — refuse artifacts below this provenance tier.
    pub floor: ProvenanceFloor,
}

/// The NEP-0016 provenance ladder, mirrored. The CANONICAL enum is
/// `nika_registry_client::ProvenanceTier` — an L2 type this L0 crate
/// cannot name; the closed spelling set is the contract between the
/// two, and the consuming seam maps `as_str()` back onto the tier
/// (a drift there fails loud at the seam, pinned by a round-trip
/// test on BOTH sides). Declaration order IS the total order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ProvenanceFloor {
    /// The v0.1 digest floor: bytes consistent with the pinned
    /// `integrity.sha256`, origin unproven.
    Unprovenanced,
    /// Minisign verified under the publisher's TOFU-anchored key.
    Provenanced,
    /// RESERVED — a registry staged-window statement (evidence format
    /// lands with the registry arc); accepted in the file so the gate
    /// can demand it the day evidence exists.
    StageClear,
    /// RESERVED — a complete in-toto publish layout.
    Verified,
}

impl ProvenanceFloor {
    /// Every tier, lowest first — the round-trip ratchet's input.
    pub const ALL: [Self; 4] = [
        Self::Unprovenanced,
        Self::Provenanced,
        Self::StageClear,
        Self::Verified,
    ];

    /// The canonical lowercase spelling — the exact strings the
    /// ladder is closed over (shared with `ProvenanceTier`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unprovenanced => "unprovenanced",
            Self::Provenanced => "provenanced",
            Self::StageClear => "stage-clear",
            Self::Verified => "verified",
        }
    }

    /// Parse a tier string — `None` on anything outside the closed
    /// set (an unknown tier REFUSES wherever it appears).
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.as_str() == raw)
    }
}

impl std::fmt::Display for ProvenanceFloor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One armed beat (`arm:` entry) — parsed + shape-validated, INERT
/// until the cadence arc consumes it. `plafond` and `manque` are
/// REQUIRED in the file (the pay law · the run-missed law — a default
/// would either spend what nobody asked for or lose a deliverable in
/// silence); the parser refuses their absence by name.
///
/// The cadence arc's eight further keys ride VERBATIM
/// (`Option<String>` — only `actif` is judged, a bool: the same
/// non-coercion shape law as `ceiling:`). Their VALUES are never
/// judged here: the grammar lives in `nika-cadence` (law 8 — deux
/// parseurs, jamais en désaccord), so a `chevauchement: nimporte`
/// passes this gate and is refused by the cadence one.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ArmEntry {
    /// `workflow:` — a repo-relative `*.nika.yaml` path (its EXISTENCE
    /// is judged at the consuming edge, never here — zero I/O law).
    pub workflow: String,
    /// `cadence:` — the expression verbatim (`"dimanche 18h07"` · a
    /// cron form). The expression LAW belongs to the cadence arc; the
    /// shape law here is « a non-empty string ».
    pub cadence: String,
    /// `où:` — the deployment locus (`None` = `local`, the safe
    /// default, at consumption time).
    pub ou: Option<ArmLocus>,
    /// `plafond:` — the per-tick ceiling (USD), REQUIRED, positive.
    pub plafond: f64,
    /// `manqué:` — what « missed » means, REQUIRED, no default.
    pub manque: MissPolicy,
    /// `chevauchement:` — the overlap policy, verbatim (the closed
    /// enum is the cadence arc's law).
    pub chevauchement: Option<String>,
    /// `après_saut:` — the after-skip policy, verbatim (same law).
    pub apres_saut: Option<String>,
    /// `actif:` — the declared INTENTION (`false` = suspended). The
    /// one judged shape among the eight: a bool, unquoted.
    pub actif: Option<bool>,
    /// `raison:` — why the beat is suspended, verbatim (REQUIRED when
    /// `actif: false` — the cadence arc's law, not this gate's).
    pub raison: Option<String>,
    /// `jusqu_au:` — the suspension expiry, verbatim (the ISO-date law
    /// is the cadence arc's).
    pub jusqu_au: Option<String>,
    /// `tolérance:` — the (m,k)-firm form, verbatim (the `m/k` law is
    /// the cadence arc's).
    pub tolerance: Option<String>,
    /// `décalage:` — the jitter form, verbatim (only `hash` exists —
    /// the cadence arc's law).
    pub decalage: Option<String>,
    /// `par:` — DECLARES the human (N3 — proves NOTHING: the machine's
    /// key is what authorizes).
    pub par: Option<String>,
}

/// `où:` — the deployment locus (`local | cloud`). Mirrors the
/// cadence arc's `Locus` over the same closed spellings (this crate
/// cannot name the L2 type; the consuming arc maps them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArmLocus {
    /// This machine.
    Local,
    /// The paid cloud.
    Cloud,
}

impl ArmLocus {
    /// The canonical spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Cloud => "cloud",
        }
    }

    /// Parse a locus — `None` outside the closed set.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "local" => Some(Self::Local),
            "cloud" => Some(Self::Cloud),
            _ => None,
        }
    }
}

/// `manqué:` — the missed-run policy. Mirrors the cadence arc's
/// `MissPolicy` over the same closed spellings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MissPolicy {
    /// Fire every missed slot, oldest first.
    Rattraper,
    /// Fire ONE catch-up for the whole silence.
    RattraperUneFois,
    /// Never catch up — a skip is an EVENT, never an execution.
    Sauter,
}

impl MissPolicy {
    /// The canonical spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rattraper => "rattraper",
            Self::RattraperUneFois => "rattraper-une-fois",
            Self::Sauter => "sauter",
        }
    }

    /// Parse a policy — `None` outside the closed set.
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "rattraper" => Some(Self::Rattraper),
            "rattraper-une-fois" => Some(Self::RattraperUneFois),
            "sauter" => Some(Self::Sauter),
            _ => None,
        }
    }
}

/// The refusal class — one variant per grammar law. This grammar owes
/// no `NIKA-*` range (the `CadenceError` precedent): every fault is
/// rendered as a taught fix at the consuming boundary, and the slugs
/// are this grammar's OWN names — they resolve nowhere else by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProjectErrorKind {
    /// The YAML itself does not parse, or the top level is not a mapping.
    Grammar,
    /// The file exists but cannot be read (permissions · a directory).
    Unreadable,
    /// `nika:` is absent, or carries something that is not a
    /// kebab-case name.
    Identity,
    /// A key outside the closed set — top-level, `traces:`,
    /// `registry:`, or an `arm:` entry. Never a silent drop.
    UnknownKey,
    /// A known key carries a value outside its shape law.
    BadValue,
}

impl ProjectErrorKind {
    /// The stable refusal slug — this grammar's own wire name, NOT a
    /// `NIKA-*` registry code (none is owed).
    #[must_use]
    pub const fn spec_code(self) -> &'static str {
        match self {
            Self::Grammar => "project.grammar",
            Self::Unreadable => "project.unreadable",
            Self::Identity => "project.identity",
            Self::UnknownKey => "project.unknown-key",
            Self::BadValue => "project.bad-value",
        }
    }
}

/// One named refusal: the law's reason + the fix form + the line (when
/// the text carries one) + the path (attached by [`discover`] — the
/// pure [`parse()`] knows no path). Every consumer maps it to its own
/// law: the budget/registry gates refuse CLOSED, the retention seam
/// notes it and fails OPEN (a broken GC never blocks a run).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProjectError {
    kind: ProjectErrorKind,
    detail: String,
    remedy: String,
    line: Option<usize>,
    path: Option<PathBuf>,
}

impl ProjectError {
    /// A refusal over the text (parse-side — no path known yet).
    #[must_use]
    pub(crate) fn at(
        kind: ProjectErrorKind,
        detail: impl Into<String>,
        remedy: impl Into<String>,
        line: Option<usize>,
    ) -> Self {
        Self {
            kind,
            detail: detail.into(),
            remedy: remedy.into(),
            line,
            path: None,
        }
    }

    /// A refusal over the file itself (discovery read failed).
    #[must_use]
    pub(crate) fn io(path: &Path, err: &std::io::Error) -> Self {
        Self {
            kind: ProjectErrorKind::Unreadable,
            detail: format!("cannot read the project file: {err}"),
            remedy: "fix the permissions, or remove the file — an absent file is pure defaults"
                .to_owned(),
            line: None,
            path: Some(path.to_path_buf()),
        }
    }

    /// Attach the discovered path (parse knows no path — the file
    /// edge does; the error speaks both).
    #[must_use]
    pub(crate) fn in_path(mut self, path: &Path) -> Self {
        self.path = Some(path.to_path_buf());
        self
    }

    /// The refusal class.
    #[must_use]
    pub const fn kind(&self) -> ProjectErrorKind {
        self.kind
    }

    /// What is refused, and why (the law, one sentence).
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// The fix form — every refusal teaches its remedy.
    #[must_use]
    pub fn remedy(&self) -> &str {
        &self.remedy
    }

    /// The 1-based line the refusal rides, when the text carried one.
    #[must_use]
    pub const fn line(&self) -> Option<usize> {
        self.line
    }

    /// The file carrying this refusal, when discovered from disk.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

impl std::fmt::Display for ProjectError {
    /// ONE line, complete and greppable: `nika.yaml:7 ·
    /// project.unknown-key · unknown key `celing` — the closed set is
    /// … — the fix form`. A note lane (doctor · the fail-open
    /// retention hook) must be able to speak it whole.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{}", path.display())?;
            if let Some(line) = self.line {
                write!(f, ":{line}")?;
            }
            write!(f, " · ")?;
        }
        write!(
            f,
            "{} · {} — {}",
            self.kind.spec_code(),
            self.detail,
            self.remedy
        )
    }
}

impl std::error::Error for ProjectError {}

/// Discover + parse the project file, walking `start` and its
/// ancestors repo-root-ward (the `.git` model): the FIRST `nika.yaml`
/// found governs. `Ok(None)` = absent everywhere up the walk — pure
/// defaults, zero ceremony, NEVER a refusal. A file that exists but
/// will not read or parse is a named refusal WITH its line.
///
/// # Errors
/// [`ProjectError`] — unreadable file (the path names it), malformed
/// YAML (the line names it), or a closed-grammar fault (unknown key ·
/// frozen tag · bad value).
pub fn discover(start: &Path) -> Result<Option<(PathBuf, Project)>, ProjectError> {
    for dir in start.ancestors() {
        let candidate = dir.join(FILE_NAME);
        // seam-bypass-ok: local operator config, read-only — the
        // registry `policy.toml` class (#512 follow-up).
        match std::fs::read_to_string(&candidate) {
            Ok(text) => {
                let project = parse(&text).map_err(|e| e.in_path(&candidate))?;
                return Ok(Some((candidate, project)));
            }
            // Absent here → keep walking (the arm's fall-through IS
            // the walk; an explicit `continue` trips the lint).
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(ProjectError::io(&candidate, &e)),
        }
    }
    Ok(None)
}

/// The CWD door — discovery walks up from the invocation directory,
/// git-style. Every run-time consumer (the budget gate · the
/// retention ladder · the registry floor) enters here so the walk
/// law lives exactly once.
///
/// # Errors
/// As [`discover`]; a CWD that will not resolve reads as unreadable.
pub fn discover_from_cwd() -> Result<Option<(PathBuf, Project)>, ProjectError> {
    let cwd = std::env::current_dir().map_err(|e| ProjectError::io(Path::new("."), &e))?;
    discover(&cwd)
}
