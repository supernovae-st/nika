// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! P4 · the supported recovery door for an unknown cost exposure. An operator
//! [`inspect`]s the journal as data, then [`submit`]s one explicit resolution of
//! one Run's exposure: an append-only `reconciled` row tied to the exact bytes
//! of that Run's latest row. Nothing is deleted or rewritten; `still_unknown`
//! keeps blocking; `billed` and `not_billed` end the exposure and authorize
//! nothing (the next unknown-cost Run still meets its own fresh review).
//!
//! Authority is the filesystem authority over this project's `.nika` plus the
//! single-writer cost lease. The one supported evidence class is the
//! operator's own attestation, recorded as unverified: Nika checks no provider
//! API, invoice or signature. The principal is the local OS account the host
//! reads, never a typed name: attributable, not authenticated. The journal is
//! not authenticated either, so a consistent row written by hand reads like one
//! this door wrote, and a copied journal carries its history with it.

use super::{
    Conflict, JOURNAL, Lease, Standing, Taken, Writer, facts, judge, lines, reference_reads,
    take_at, trace_of,
};
use nika_fs::OwnedDir;

/// The domain separator of a project binding.
const BINDING: &str = "nika/cost-project@1";

/// What a project binding is, as every document states it.
const BINDING_BASIS: &str = "host-local inspected-directory binding: the held .nika directory's device and inode under nika/cost-project@1; not an authenticated or globally unique project identity";

/// What the one supported evidence class authorizes, as every document states it.
const ATTESTATION: &str = "operator attestation: ends the exposure on the operator's word; Nika verified no billing, and a new unknown-cost Run still meets its own fresh review";

/// Why an inspection recorded an UNKNOWN row.
const DERIVED: &str =
    "the cost lease proves its writer gone: recorded unknown once, as every review records it";

/// A host-local binding of a request to the inspected `.nika` directory: the
/// sha256 of the held descriptor's device and inode under a versioned domain
/// separator. It refuses a request made against another directory (a copy, or
/// a `.nika` replaced since the inspection); a rename on the same filesystem
/// keeps it. It is not an authenticated or globally unique project identity:
/// device and inode values can recur, and a copied journal's rows still apply
/// in the copy.
///
/// # Errors
/// The held directory cannot be read.
pub(super) fn project(nika: &OwnedDir) -> std::io::Result<String> {
    use std::os::unix::fs::MetadataExt as _;
    let held = nika.as_file().metadata()?;
    let bound = format!("{BINDING}\n{}\n{}", held.dev(), held.ino());
    Ok(nika_event::source_id::sha256_hex(bound.as_bytes()))
}

/// How an operator resolves one Run's unknown exposure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Resolution {
    /// The provider billed the request(s): the exposure ends.
    Billed,
    /// The provider did not bill it: the exposure ends.
    NotBilled,
    /// Still unknown: recorded, and the Run keeps blocking.
    StillUnknown,
}

impl Resolution {
    /// The journal's word for it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Billed => "billed",
            Self::NotBilled => "not_billed",
            Self::StillUnknown => "still_unknown",
        }
    }

    fn from_journal(word: &str) -> Option<Self> {
        [Self::Billed, Self::NotBilled, Self::StillUnknown]
            .into_iter()
            .find(|resolution| resolution.as_str() == word)
    }
}

/// The evidence a resolution rests on. One class is supported: the
/// operator's own attestation, recorded with `verified: false`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Evidence {
    /// The operator attests the resolution and names where it looked
    /// (an invoice line, a dashboard entry, a support ticket): never
    /// independently verified billing.
    #[non_exhaustive]
    OperatorAttestation {
        /// The operator's reference: 1 to 512 characters, no control.
        reference: String,
    },
}

impl Evidence {
    /// An operator attestation citing `reference`.
    #[must_use]
    pub fn operator_attestation(reference: impl Into<String>) -> Self {
        Self::OperatorAttestation {
            reference: reference.into(),
        }
    }
}

/// The local OS account the host read (never a typed name): attributable,
/// not authenticated.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Principal {
    /// The account's user id.
    pub uid: u32,
    /// Its login name, when the account database names one.
    pub name: Option<String>,
}

impl Principal {
    /// The account `uid`, named `name` when known.
    #[must_use]
    pub fn new(uid: u32, name: Option<String>) -> Self {
        Self { uid, name }
    }
}

/// One explicit resolution request, bound to an inspection's project and to
/// the exact digest of the Run's latest row.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Request {
    /// The project binding the inspection reported ([`Inspection::project`]).
    pub project: String,
    /// The Run's invocation.
    pub invocation: String,
    /// The sha256 of the Run's latest row, as the inspection named it.
    pub prior_sha256: String,
    /// The resolution.
    pub resolution: Resolution,
    /// What it rests on.
    pub evidence: Evidence,
    /// Who submits it (the host reads it from the OS).
    pub principal: Principal,
    /// When, as the host's clock reads it.
    pub observed_at: std::time::SystemTime,
}

impl Request {
    /// A resolution request for `invocation`'s latest row `prior_sha256`,
    /// inspected in `project`.
    #[must_use]
    pub fn new(
        project: String,
        invocation: String,
        prior_sha256: String,
        resolution: Resolution,
        evidence: Evidence,
        principal: Principal,
        observed_at: std::time::SystemTime,
    ) -> Self {
        Self {
            project,
            invocation,
            prior_sha256,
            resolution,
            evidence,
            principal,
            observed_at,
        }
    }
}

/// Why a request was refused. Preflight refusals preserve journal bytes;
/// [`Refusal::Io`] after an append attempt can leave a partial or complete write.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Refusal {
    /// Another live process holds the cost lease: a Run in flight or a
    /// review waiting for its answer.
    #[error(
        "another process holds this project's cost lease: a Run in flight or a review waiting · nothing was judged or written"
    )]
    Busy {
        /// The holder's recorded pid, when readable.
        pid: Option<u64>,
    },
    /// There is no cost journal in this project.
    #[error(
        "this project has no cost journal (.nika/{}): nothing to reconcile",
        JOURNAL
    )]
    NoJournal,
    /// The request names another inspected directory.
    #[error(
        "this request was inspected in another project (binding {here} here): inspect here first · nothing was written"
    )]
    OtherProject {
        /// This directory's binding.
        here: String,
    },
    /// No admitted row names that invocation.
    #[error("no Run of that invocation is on record here · nothing was written")]
    UnknownRun,
    /// The Run's unknown has not been recorded yet.
    #[error(
        "that Run's unknown is not on record yet: `nika trace cost` records it (inspect first) · nothing was written"
    )]
    InspectFirst,
    /// The Run is on record, but this door cannot resolve it.
    #[error("that Run cannot be reconciled here: {0} · nothing was written")]
    NotReconcilable(&'static str),
    /// The Run's latest row is not the one the request names.
    #[error(
        "stale request: that Run's latest row is {head}, not the one named · inspect again · nothing was written"
    )]
    Stale {
        /// The digest of the Run's latest row.
        head: String,
    },
    /// The request itself is invalid.
    #[error("invalid request: {0} · nothing was written")]
    Invalid(&'static str),
    /// The journal cannot be read, written or verified after a write. A write
    /// may have landed: inspect before retrying rather than assuming no effect.
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl Refusal {
    /// Its machine name.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Busy { .. } => "busy",
            Self::NoJournal => "no_journal",
            Self::OtherProject { .. } => "other_project",
            Self::UnknownRun => "unknown_run",
            Self::InspectFirst => "inspect_first",
            Self::NotReconcilable(_) => "not_reconcilable",
            Self::Stale { .. } => "stale",
            Self::Invalid(_) => "invalid",
            Self::Io(_) => "io",
        }
    }

    /// Whether the environment refused (a busy lease, an unreadable or
    /// unwritable journal) rather than the request itself.
    #[must_use]
    pub fn is_environment(&self) -> bool {
        matches!(self, Self::Busy { .. } | Self::Io(_))
    }

    /// The machine object: its kind, its message and the facts it names.
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        let mut refused = serde_json::json!({"kind": self.kind(), "message": self.to_string()});
        match self {
            Self::Busy { pid } => refused["holder_pid"] = serde_json::json!(pid),
            Self::OtherProject { here } => refused["binding_here"] = serde_json::json!(here),
            Self::Stale { head } => refused["head_sha256"] = serde_json::json!(head),
            _ => {}
        }
        refused
    }
}

/// How a Run stands in an inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunState {
    /// Settled by its writer with a sent request whose charge is unknown.
    Uncertain,
    /// Its writer let the lease go unsettled: recorded unknown.
    Unknown,
    /// Reconciled as still unknown.
    StillUnknown,
    /// Admitted and never settled, by a writer this host cannot judge.
    Unjudged,
    /// Reconciled for good.
    Reconciled(Resolution),
}

impl RunState {
    /// Its word in the inspection document.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uncertain => "uncertain",
            Self::Unknown => "unknown",
            Self::StillUnknown => "still_unknown",
            Self::Unjudged => "unjudged",
            Self::Reconciled(_) => "reconciled",
        }
    }

    /// Whether it still blocks a new unknown-cost Run.
    #[must_use]
    pub fn blocks(self) -> bool {
        !matches!(self, Self::Reconciled(_))
    }
}

/// One row of a Run's history, in journal order.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Row {
    /// Its 1-based position among the journal's rows.
    pub index: usize,
    /// Its phase.
    pub phase: String,
    /// The sha256 of its exact bytes.
    pub sha256: String,
    /// Why the fold refused it, when it did (it blocks by itself).
    pub refused: Option<&'static str>,
}

/// One Run that blocks or was reconciled, as an inspection reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunReport {
    /// The invocation its rows name.
    pub invocation: String,
    /// How it stands.
    pub state: RunState,
    /// The sha256 of its latest row: the digest a resolution must name.
    pub head_sha256: String,
    /// Why this door cannot resolve it, when it cannot.
    pub why_not: Option<&'static str>,
    /// The writer its `prepared` row's lease named.
    pub writer: Option<serde_json::Value>,
    /// The trace that recorded its execution, when one did.
    pub trace: Option<String>,
    /// What its own rows record (route · provider request ids · id window).
    pub facts: serde_json::Value,
    /// Its rows, refused ones included.
    pub history: Vec<Row>,
}

/// The journal as an operator reads it, after this inspection's own
/// derivations.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Inspection {
    /// The host-local binding a request made from this inspection names.
    pub project: String,
    /// The journal's byte length before this inspection.
    pub bytes_before: u64,
    /// Its byte length after (larger only by what this inspection derived).
    pub bytes: u64,
    /// The sha256 of its bytes after.
    pub sha256: String,
    /// Each UNKNOWN row this inspection derived and appended: the invocation
    /// and the sha256 of the appended row (the same derivation any review makes).
    pub derived: Vec<(String, String)>,
    /// Every Run that blocks or was reconciled, in invocation order.
    pub runs: Vec<RunReport>,
    /// The sha256 of every row cut mid-write: each blocks, none is reconcilable.
    pub torn: Vec<String>,
    /// Every refused row: each blocks, none is reconcilable.
    pub conflicts: Vec<Conflict>,
}

impl RunReport {
    /// The machine object for this Run.
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        let resolution = match self.state {
            RunState::Reconciled(resolution) => Some(resolution.as_str()),
            _ => None,
        };
        let history: Vec<serde_json::Value> = self
            .history
            .iter()
            .map(|row| {
                serde_json::json!({"index": row.index, "phase": row.phase,
                    "sha256": row.sha256, "refused": row.refused})
            })
            .collect();
        serde_json::json!({
            "invocation": self.invocation,
            "state": self.state.as_str(),
            "resolution": resolution,
            "blocks": self.state.blocks(),
            "head_sha256": self.head_sha256,
            "reconcilable": self.why_not.is_none(),
            "why_not": self.why_not,
            "writer": self.writer,
            "trace": self.trace,
            "facts": self.facts,
            "history": history,
        })
    }
}

impl Inspection {
    /// Whether nothing blocks a new unknown-cost Run (its own review still asks).
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.torn.is_empty()
            && self.conflicts.is_empty()
            && self.runs.iter().all(|run| !run.state.blocks())
    }

    /// The machine document (`cost_inspect_version: 1`) a host or SDK reads.
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        let derived: Vec<serde_json::Value> = self
            .derived
            .iter()
            .map(|(invocation, sha256)| {
                serde_json::json!({"invocation": invocation, "sha256": sha256, "cause": DERIVED})
            })
            .collect();
        let conflicts: Vec<serde_json::Value> = self
            .conflicts
            .iter()
            .map(|c| {
                serde_json::json!({"invocation": c.invocation, "sha256": c.sha256,
                    "reason": c.reason, "reconcilable": false})
            })
            .collect();
        serde_json::json!({
            "cost_inspect_version": 1,
            "binding": {"project": self.project, "basis": BINDING_BASIS},
            "journal": {"path": format!(".nika/{JOURNAL}"), "bytes_before": self.bytes_before,
                "bytes": self.bytes, "sha256": self.sha256},
            "derived": derived,
            "clear": self.is_clear(),
            "runs": self.runs.iter().map(RunReport::to_value).collect::<Vec<_>>(),
            "torn": self.torn,
            "conflicts": conflicts,
            "evidence_classes": {"operator_attestation": ATTESTATION},
        })
    }
}

impl Receipt {
    /// The machine document (`cost_reconcile_version: 1`) a host or SDK reads.
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "cost_reconcile_version": 1,
            "event": self.event,
            "event_sha256": self.event_sha256,
            "prior_sha256": self.event["reconciliation"]["prior_sha256"],
            "journal": {"path": format!(".nika/{JOURNAL}"),
                "bytes_before": self.before.0, "sha256_before": self.before.1,
                "bytes_after": self.after.0, "sha256_after": self.after.1,
                "prefix_preserved": self.prefix_preserved},
            "still_blocks": self.still_blocks,
            "clear": self.clear,
            "authority": ATTESTATION,
        })
    }
}

/// The resolution appended, and what it proves about the journal.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Receipt {
    /// The appended event, exactly as written.
    pub event: serde_json::Value,
    /// The sha256 of the appended line.
    pub event_sha256: String,
    /// The journal's byte length and sha256 before the append.
    pub before: (u64, String),
    /// The journal's byte length and sha256 after it.
    pub after: (u64, String),
    /// Whether the bytes before are a prefix of the bytes after.
    pub prefix_preserved: bool,
    /// Whether that Run still blocks (a `still_unknown` resolution).
    pub still_blocks: bool,
    /// Whether nothing blocks a new unknown-cost Run now.
    pub clear: bool,
}

fn held(project: &OwnedDir, nika: &OwnedDir, writer: &Writer) -> Result<Lease, Refusal> {
    match take_at(project, nika, writer)? {
        Taken::Held(lease) => Ok(lease),
        Taken::Busy { pid } => Err(Refusal::Busy { pid }),
    }
}

/// The project's `.nika`, held by descriptor (never created: a refusal writes
/// nothing, not even a directory).
fn journal_dir(root: &std::path::Path) -> Result<(OwnedDir, OwnedDir), Refusal> {
    let project = OwnedDir::open(root)?;
    let nika = project.open_below(&[".nika"]).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Refusal::NoJournal,
        _ => Refusal::Io(e),
    })?;
    Ok((project, nika))
}

fn digest_of(bytes: &[u8]) -> String {
    nika_event::source_id::sha256_hex(bytes)
}

fn length(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len()).unwrap_or(u64::MAX)
}

/// Inspect the project whose root `root` holds, under the cost lease: the same
/// fold a review runs, which records a Run as unknown once when the lease
/// proves its writer gone (reported in [`Inspection::derived`]).
///
/// # Errors
/// A busy lease, no journal, or an unreadable journal.
pub fn inspect(
    root: &std::path::Path,
    writer: &Writer,
    observer: &str,
) -> Result<Inspection, Refusal> {
    let (project_dir, nika) = journal_dir(root)?;
    let lease = held(&project_dir, &nika, writer)?;
    let before = lease.read()?;
    let (_, derived) = lease.fold_deriving(&nika, writer, observer)?;
    let bytes = lease.read()?;
    let (standings, torn, conflicts) = judge(&bytes)?;
    let histories = histories(&bytes, &conflicts);
    let runs = standings
        .iter()
        .filter_map(|(invocation, standing)| {
            report(
                &nika,
                writer,
                invocation,
                standing,
                histories.get(invocation),
            )
        })
        .collect();
    Ok(Inspection {
        project: project(&nika)?,
        bytes_before: length(&before),
        bytes: length(&bytes),
        sha256: digest_of(&bytes),
        derived,
        runs,
        torn,
        conflicts,
    })
}

/// Each invocation's rows in journal order (torn lines have no invocation).
fn histories(
    bytes: &[u8],
    conflicts: &[Conflict],
) -> std::collections::BTreeMap<String, Vec<(Row, serde_json::Value)>> {
    let mut out: std::collections::BTreeMap<String, Vec<(Row, serde_json::Value)>> =
        std::collections::BTreeMap::new();
    for (index, line) in lines(bytes).enumerate() {
        let Some(Ok(row)) = std::str::from_utf8(line)
            .ok()
            .map(serde_json::from_str::<serde_json::Value>)
        else {
            continue;
        };
        let Some(invocation) = row["invocation"].as_str() else {
            continue;
        };
        let sha256 = digest_of(line);
        let refused = conflicts
            .iter()
            .find(|c| c.invocation == invocation && c.sha256 == sha256)
            .map(|c| c.reason);
        let entry = Row {
            index: index + 1,
            phase: row["phase"].as_str().unwrap_or_default().to_owned(),
            sha256,
            refused,
        };
        out.entry(invocation.to_owned())
            .or_default()
            .push((entry, row));
    }
    out
}

/// One Run's report, or `None` for a clean settlement.
fn report(
    nika: &OwnedDir,
    writer: &Writer,
    invocation: &str,
    standing: &Standing<'_>,
    history: Option<&Vec<(Row, serde_json::Value)>>,
) -> Option<RunReport> {
    let (state, head, why_not) = match standing {
        Standing::Settled(_, false) => return None,
        Standing::Settled(head, true) => (RunState::Uncertain, *head, None),
        Standing::Unknown(_, head) => (RunState::Unknown, *head, None),
        Standing::Held(head) => (RunState::StillUnknown, *head, None),
        Standing::Reconciled(head) => {
            let row: serde_json::Value = serde_json::from_slice(head).ok()?;
            let word = row["reconciliation"]["resolution"].as_str()?;
            let resolution = Resolution::from_journal(word)?;
            let done = Some("it is already reconciled");
            (RunState::Reconciled(resolution), *head, done)
        }
        Standing::Prepared(row, line) => {
            let why = if writer.judges(&row["lease"]) {
                "its unknown is not on record yet"
            } else {
                "its writer cannot be judged from this host (a row written before the cost lease, or on another host)"
            };
            (RunState::Unjudged, *line, Some(why))
        }
    };
    let rows = history.map(Vec::as_slice).unwrap_or_default();
    let prepared = rows
        .iter()
        .find(|(row, _)| row.phase == "prepared" && row.refused.is_none());
    let head_row = serde_json::from_slice::<serde_json::Value>(head).unwrap_or_default();
    Some(RunReport {
        invocation: invocation.to_owned(),
        state,
        head_sha256: digest_of(head),
        why_not,
        writer: prepared
            .map(|(_, row)| row["lease"].clone())
            .filter(|lease| !lease.is_null()),
        trace: trace_of(nika, invocation),
        facts: facts(&head_row),
        history: rows.iter().map(|(row, _)| row.clone()).collect(),
    })
}

/// Submit one resolution, under the cost lease. Everything is judged before
/// the one write: the project binding, the Run's standing and latest row, the
/// evidence, and the event itself through the same fold every review runs. A
/// Run whose unknown is not on record yet is refused (inspect first): this
/// door derives nothing. Every preflight refusal leaves the journal's bytes untouched.
///
/// # Errors
/// A [`Refusal`]; an [`Refusal::Io`] after the append means its effect is
/// uncertain: it could not be read back or verified as the Run's current event.
pub fn submit(
    root: &std::path::Path,
    writer: &Writer,
    request: &Request,
) -> Result<Receipt, Refusal> {
    let (project_dir, nika) = journal_dir(root)?;
    let lease = held(&project_dir, &nika, writer)?;
    let here = project(&nika)?;
    if request.project != here {
        return Err(Refusal::OtherProject { here });
    }
    let before = lease.read()?;
    let (standings, _, _) = judge(&before)?;
    let head = match standings.get(&request.invocation) {
        None => return Err(Refusal::UnknownRun),
        Some(Standing::Settled(head, true) | Standing::Unknown(_, head) | Standing::Held(head)) => {
            *head
        }
        Some(Standing::Prepared(row, _)) if writer.judges(&row["lease"]) => {
            return Err(Refusal::InspectFirst);
        }
        Some(Standing::Prepared(..)) => {
            return Err(Refusal::NotReconcilable(
                "its writer cannot be judged from this host",
            ));
        }
        Some(Standing::Settled(_, false)) => {
            return Err(Refusal::NotReconcilable("it settled without uncertainty"));
        }
        Some(Standing::Reconciled(_)) => {
            return Err(Refusal::NotReconcilable("it is already reconciled"));
        }
    };
    let prior = digest_of(head);
    if request.prior_sha256 != prior {
        return Err(Refusal::Stale { head: prior });
    }
    let Evidence::OperatorAttestation { reference } = &request.evidence;
    if !reference_reads(reference) {
        return Err(Refusal::Invalid(
            "the evidence reference must be 1 to 512 characters without control characters",
        ));
    }
    let event = event(request, head, &prior, &here, writer)?;
    let line = event.to_string();
    // The fold must admit the event exactly as it will be written.
    let mut after = before.clone();
    if after.last().is_some_and(|last| *last != b'\n') {
        after.push(b'\n');
    }
    after.extend_from_slice(line.as_bytes());
    after.push(b'\n');
    let (judged, _, refused) = judge(&after)?;
    let moved = matches!(judged.get(&request.invocation),
        Some(Standing::Held(row) | Standing::Reconciled(row)) if *row == line.as_bytes());
    if !moved
        || refused
            .iter()
            .any(|c| c.sha256 == digest_of(line.as_bytes()))
    {
        return Err(Refusal::Invalid("the fold does not admit this resolution"));
    }
    lease.append_row(&line)?;
    let written = lease.read()?;
    let (standings, torn, conflicts) = judge(&written)?;
    verify_landed(
        &before,
        &written,
        &request.invocation,
        &line,
        &standings,
        &conflicts,
    )?;
    let still_blocks = matches!(standings.get(&request.invocation), Some(Standing::Held(_)));
    let clear = torn.is_empty()
        && conflicts.is_empty()
        && standings
            .values()
            .all(|s| matches!(s, Standing::Settled(_, false) | Standing::Reconciled(_)));
    Ok(Receipt {
        event,
        event_sha256: digest_of(line.as_bytes()),
        before: (length(&before), digest_of(&before)),
        after: (length(&written), digest_of(&written)),
        prefix_preserved: written.starts_with(&before),
        still_blocks,
        clear,
    })
}

/// A successful append must have advanced this Run to this exact event. An
/// I/O failure here is explicitly after-write uncertainty, not a preflight refusal.
fn verify_landed(
    before: &[u8],
    written: &[u8],
    invocation: &str,
    line: &str,
    standings: &std::collections::BTreeMap<String, Standing<'_>>,
    conflicts: &[Conflict],
) -> Result<(), Refusal> {
    let moved = matches!(standings.get(invocation),
        Some(Standing::Held(row) | Standing::Reconciled(row)) if *row == line.as_bytes());
    if !written.starts_with(before)
        || !moved
        || conflicts
            .iter()
            .any(|c| c.sha256 == digest_of(line.as_bytes()))
    {
        return Err(Refusal::Io(super::invalid(
            "reconciliation append was attempted but its resulting standing is unverified; inspect before any further action",
        )));
    }
    Ok(())
}

/// The `reconciled` row: the Run's own facts copied, never typed.
fn event(
    request: &Request,
    head: &[u8],
    prior: &str,
    project: &str,
    writer: &Writer,
) -> Result<serde_json::Value, Refusal> {
    let head_row = serde_json::from_slice::<serde_json::Value>(head)
        .map_err(|_| Refusal::Invalid("the Run's latest row does not read"))?;
    let recorded = facts(&head_row);
    let Evidence::OperatorAttestation { reference } = &request.evidence;
    let observed_at = jiff::Timestamp::try_from(request.observed_at)
        .map_err(|_| Refusal::Invalid("the observation time is out of range"))?;
    Ok(serde_json::json!({
        "schema": "nika/run-cost-observation@1",
        "invocation": request.invocation,
        "phase": "reconciled",
        "reconciliation": {
            "schema": super::RECONCILIATION,
            "prior_sha256": prior,
            "resolution": request.resolution.as_str(),
            "evidence": {"class": "operator_attestation", "verified": false,
                "reference": reference},
            "route": recorded["route"],
            "provider_request_ids": recorded["provider_request_ids"],
            "window": recorded["window"],
            "principal": {"kind": "local_account", "uid": request.principal.uid,
                "name": request.principal.name},
            "project": {"binding": project, "basis": "host-local inspected-directory binding"},
            "observed_at": observed_at.to_string(),
        },
        "lease": writer.json(),
    }))
}
