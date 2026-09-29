// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The paid Run's cost observation journal (`.nika/inference-cost-observations.ndjson`)
//! and its writer lease — billing evidence beside the trace plane's own
//! ADR-129 lease. Descended from nika-cli-host's `run_cost` at the 15k wall
//! (2026-09-28): the lease, the strict fold and the torn-tail append are
//! compute; the question, the live account and its rows stay with the host.
//!
//! An unknown-cost Run holds the project, directory, journal and legacy lock leases from before its
//! `prepared` row until after its `settled` row; the kernel releases it however
//! the process ends. A later review that takes the lease therefore knows every
//! leased writer of an unsettled row is gone: a process killed mid-dispatch
//! runs no handler, so the restart derives that Run's UNKNOWN from the
//! `prepared` row and appends it once, never rewriting the rows it read.

use nika_fs::OwnedDir;
use nika_providers::admission;
use nix::fcntl::{Flock, FlockArg};
use std::collections::BTreeMap;
use std::io::{Read as _, Seek as _, Write as _};
use std::path::Path;

mod run;
pub use run::review::{
    AccountView, Claim, Claims, KeyAnswer, REVIEW_TTL, REVIEWS_RETAINED, Replay, ReviewRefusal,
    Reviews, review_witness,
};
pub use run::{Blocked, Cleared, JournalWitness, RunAccount, RunJournal, clear};

/// The journal's name under the project's `.nika/` directory.
pub const JOURNAL: &str = "inference-cost-observations.ndjson";

/// `<journal>.lock`, the ADR-129 lease naming, beside the journal it guards.
const LEASE: &str = "inference-cost-observations.ndjson.lock";

/// The contract a `reconciled` row names: P4's append-only resolution of one
/// Run's unknown exposure, tied to the exact bytes of that Run's latest row.
const RECONCILIATION: &str = "nika/cost-reconciliation@1";

/// The process that holds the lease, as every row it writes names it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Writer {
    /// The writer's process id.
    pub pid: u32,
    /// The host it runs on: a lease is judged on its own host only.
    pub host: String,
    /// The running kernel's boot identity, where the platform proves one
    /// (Linux): every container on that kernel shares its lock table.
    boot: Option<String>,
}

impl Writer {
    /// This process, on this host and kernel.
    #[must_use]
    pub fn this_process() -> Self {
        Self {
            pid: std::process::id(),
            host: crate::liveness::host_name(),
            boot: crate::liveness::boot_id(),
        }
    }

    /// The `{"pid","host"}` record the lease and every row carry, with the
    /// kernel's `"boot"` identity when this platform proves one.
    #[must_use]
    pub fn json(&self) -> serde_json::Value {
        let mut record = serde_json::json!({"pid": self.pid, "host": self.host});
        if let Some(boot) = &self.boot {
            record["boot"] = serde_json::Value::String(boot.clone());
        }
        record
    }

    /// Whether the lease this writer holds can judge a recorded writer: the
    /// same nonempty hostname (the historical heuristic, never a proof of one
    /// machine), or the same boot identity (the same running kernel, whose
    /// lock this writer acquired). Absent or empty identities never match.
    fn judges(&self, lease: &serde_json::Value) -> bool {
        let host = !self.host.is_empty() && lease["host"].as_str() == Some(self.host.as_str());
        let boot = (self.boot.as_deref()).is_some_and(|boot| lease["boot"].as_str() == Some(boot));
        host || boot
    }
}

/// The held project, directory, journal and legacy locks; dropping releases them (files stay).
#[derive(Debug)]
pub struct Lease {
    _project: Flock<std::fs::File>,
    _directory: Flock<std::fs::File>,
    journal: std::sync::Mutex<Flock<std::fs::File>>,
    _lock: Flock<std::fs::File>,
}

impl Lease {
    /// Read the bounded bytes of the locked journal inode, even after a rename.
    /// This is custody of a file, not authentication of its contents.
    ///
    /// # Errors
    /// The descriptor cannot be read, the journal exceeds its read bound, or
    /// an earlier operation poisoned the local cursor lock.
    pub fn read(&self) -> std::io::Result<Vec<u8>> {
        let held = self.journal_guard()?;
        let mut file = &**held;
        file.rewind()?;
        read_file(file)
    }

    /// Append one complete framed row to the locked inode, never reopening its
    /// name. Renaming the journal or `.nika` cannot redirect this observation.
    /// A torn tail is preserved and separated in the same append operation.
    ///
    /// # Errors
    /// An unsafe link count, I/O failure or poisoned cursor lock. A short write
    /// or synchronization failure is an uncertain effect, never retried here.
    pub fn append_row(&self, row: &str) -> std::io::Result<()> {
        let held = self.journal_guard()?;
        append_file(&held, row)
    }

    /// Fold and derive UNKNOWN rows using only this journal's locked inode.
    /// `nika` is used only to locate diagnostic trace names.
    ///
    /// # Errors
    /// As [`Lease::read`] and [`Lease::append_row`], or an invalid journal.
    pub fn fold_as(
        &self,
        nika: &OwnedDir,
        holder: &Writer,
        observer: &str,
    ) -> std::io::Result<Exposures> {
        self.fold_deriving(nika, holder, observer)
            .map(|(exposures, _)| exposures)
    }

    fn fold_deriving(
        &self,
        nika: &OwnedDir,
        holder: &Writer,
        observer: &str,
    ) -> std::io::Result<(Exposures, Vec<(String, String)>)> {
        let held = self.journal_guard()?;
        let mut file = &**held;
        file.rewind()?;
        let bytes = read_file(file)?;
        derive(nika, &bytes, holder, observer, |row| append_file(file, row))
    }

    fn journal_guard(&self) -> std::io::Result<std::sync::MutexGuard<'_, Flock<std::fs::File>>> {
        self.journal
            .lock()
            .map_err(|_| invalid("cost journal cursor lock is poisoned"))
    }
}

fn append_file(mut file: &std::fs::File, row: &str) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt as _;
    let metadata = file.metadata()?;
    if metadata.nlink() != 1 {
        return Err(invalid("cost journal no longer has one owned link"));
    }
    let mut framed = String::new();
    if metadata.len() != 0 {
        file.seek(std::io::SeekFrom::End(-1))?;
        let mut last = [0_u8; 1];
        file.read_exact(&mut last)?;
        if last[0] != b'\n' {
            framed.push('\n');
        }
    }
    framed.push_str(row);
    framed.push('\n');
    // O_APPEND on the held description selects EOF atomically for this write.
    if file.write(framed.as_bytes())? != framed.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::WriteZero,
            "incomplete cost journal append",
        ));
    }
    file.sync_all()
}

/// The outcome of one attempt to take the lease.
#[derive(Debug)]
#[non_exhaustive]
pub enum Taken {
    /// This process now holds the lease.
    Held(Lease),
    /// Another live process holds it: its Run has not settled yet.
    Busy {
        /// The holder's recorded pid, when its record is readable.
        pid: Option<u64>,
    },
}

/// Take the lease under the directory's current parent and record this writer.
///
/// This compatibility entry cannot know a project's original identity after
/// the supplied directory moves. Project-facing hosts must use [`take_at`]
/// with the project descriptor held before they open `.nika`.
///
/// # Errors
/// The directory or journal cannot be locked, a child has multiple links or changed
/// identity, or the legacy lease record cannot be written safely.
pub fn take(nika: &OwnedDir, writer: &Writer) -> std::io::Result<Taken> {
    use nix::fcntl::{OFlag, openat};
    use nix::sys::stat::Mode;
    let project = openat(
        nika.as_file(),
        "..",
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    take_under(std::fs::File::from(project), None, nika, writer)
}

/// Take a cost lease in the original held project, without following `.nika/..`.
///
/// The caller opens `project` before opening its `.nika`. Moving that child
/// afterward cannot change which project's live writer this acquisition excludes.
///
/// # Errors
/// The child no longer names this project's `.nika`, or any lease cannot be
/// acquired safely. A busy original project is returned before child writes.
pub fn take_at(project: &OwnedDir, nika: &OwnedDir, writer: &Writer) -> std::io::Result<Taken> {
    use nix::fcntl::{OFlag, openat};
    use nix::sys::stat::Mode;
    // A fresh open description, not dup: flock ownership must stay independent.
    let fresh = openat(
        project.as_file(),
        ".",
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    take_under(std::fs::File::from(fresh), Some(project), nika, writer)
}

fn take_under(
    project: std::fs::File,
    original: Option<&OwnedDir>,
    nika: &OwnedDir,
    writer: &Writer,
) -> std::io::Result<Taken> {
    use nix::fcntl::{OFlag, openat};
    use nix::sys::stat::Mode;
    let Some(project) = lock_file(project)? else {
        return Ok(Taken::Busy {
            pid: lease_pid(nika),
        });
    };
    if let Some(original) = original {
        use std::os::unix::fs::MetadataExt as _;
        let named = original.open_below(&[".nika"])?.as_file().metadata()?;
        let held = nika.as_file().metadata()?;
        if named.dev() != held.dev() || named.ino() != held.ino() {
            return Err(invalid(
                "cost journal directory changed from its original project",
            ));
        }
    }
    // A fresh open description is essential: dup/try_clone shares flock ownership.
    // The directory lock survives unlinking either of the child file names.
    let directory = openat(
        nika.as_file(),
        ".",
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    let Some(directory) = lock_file(std::fs::File::from(directory))? else {
        return Ok(Taken::Busy {
            pid: lease_pid(nika),
        });
    };
    // Keep the historical lock for older writers, then guard the actual journal
    // inode as well. Shared hard links are ambiguous project custody: refuse.
    let Some(held) = lock_file(nika.open_lock(LEASE)?)? else {
        return Ok(Taken::Busy {
            pid: lease_pid(nika),
        });
    };
    same_named_file(nika, &held, LEASE)?;
    let Some(journal) = lock_file(nika.open_lock(JOURNAL)?)? else {
        return Ok(Taken::Busy {
            pid: lease_pid(nika),
        });
    };
    same_named_file(nika, &journal, JOURNAL)?;
    same_named_file(nika, &held, LEASE)?;
    let flags = nix::fcntl::fcntl(&*journal, nix::fcntl::FcntlArg::F_GETFL)
        .map_err(std::io::Error::from)?;
    nix::fcntl::fcntl(
        &*journal,
        nix::fcntl::FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_APPEND),
    )
    .map_err(std::io::Error::from)?;
    let record = format!("{}\n", writer.json());
    let mut file = &*held;
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(record.as_bytes())?;
    file.sync_all()?;
    Ok(Taken::Held(Lease {
        _project: project,
        _directory: directory,
        journal: std::sync::Mutex::new(journal),
        _lock: held,
    }))
}

fn lock_file(file: std::fs::File) -> std::io::Result<Option<Flock<std::fs::File>>> {
    match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
        Ok(held) => Ok(Some(held)),
        Err((_, nix::errno::Errno::EWOULDBLOCK)) => Ok(None),
        Err((_, errno)) => Err(std::io::Error::from_raw_os_error(errno as i32)),
    }
}

fn same_named_file(nika: &OwnedDir, held: &std::fs::File, name: &str) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt as _;
    let actual = held.metadata()?;
    let named = nika.open_relative(Path::new(name))?.metadata()?;
    if actual.nlink() != 1
        || named.nlink() != 1
        || actual.dev() != named.dev()
        || actual.ino() != named.ino()
    {
        return Err(invalid(
            "cost journal custody changed or has multiple hard links",
        ));
    }
    Ok(())
}

fn lease_pid(nika: &OwnedDir) -> Option<u64> {
    // A diagnostic only: a replaced or unreadable record proves no identity.
    let mut text = String::new();
    nika.open_relative(Path::new(LEASE))
        .ok()?
        .take(4096)
        .read_to_string(&mut text)
        .ok()?;
    serde_json::from_str::<serde_json::Value>(&text).ok()?["pid"].as_u64()
}

/// Why an earlier invocation still blocks a new unknown-cost Run.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Exposure {
    /// It settled, and a request it sent ended without usable settlement.
    Uncertain,
    /// Its leased writer ended without settling: recorded as unknown.
    Unknown {
        /// The writer the lease named, when recorded.
        pid: Option<u64>,
    },
    /// Admitted and never settled, with no lease this host can judge (a row
    /// written before the lease existed, or on another host).
    Unjudged,
    /// An operator reconciled it as still unknown: its billing stays unknown.
    StillUnknown,
}

/// One earlier invocation that still blocks, named by its own identity.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Blocker {
    /// The invocation the rows name.
    pub invocation: String,
    /// Why it blocks.
    pub exposure: Exposure,
    /// The file under `.nika/traces` that recorded this invocation's
    /// execution, when one does (`nika trace show` takes it).
    pub trace: Option<String>,
}

impl Blocker {
    /// One named blocker, with no trace located.
    #[must_use]
    pub fn new(invocation: String, exposure: Exposure) -> Self {
        Self {
            invocation,
            exposure,
            trace: None,
        }
    }
}

/// A row no legal transition admits: a settlement from a writer that did not
/// prepare the Run, one that comes after the Run's recorded unknown or
/// settlement, a Run with no prepared row, a second preparation, a
/// lease-less row once leases began, or a `prepared`/`settled` row whose
/// observation its account could never have written. It never settles its
/// Run; it blocks by itself until an append-only reconciliation contract
/// exists.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Conflict {
    /// The invocation the row names.
    pub invocation: String,
    /// The sha256 of the row's exact bytes.
    pub sha256: String,
    /// Why no legal transition admits it.
    pub reason: &'static str,
}

impl Conflict {
    /// One refused row.
    #[must_use]
    pub fn new(invocation: String, sha256: String, reason: &'static str) -> Self {
        Self {
            invocation,
            sha256,
            reason,
        }
    }
}

/// What the journal still holds against a new unknown-cost Run: the Runs it
/// names, the rows a killed writer cut mid-write (by the digest of their exact
/// bytes, since no identity survives the cut), and the rows it refuses.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Exposures {
    /// The blocking invocations, in invocation order.
    pub runs: Vec<Blocker>,
    /// The sha256 of every row cut mid-write.
    pub torn: Vec<String>,
    /// Every row no legal transition admits, in journal order.
    pub conflicts: Vec<Conflict>,
}

impl Exposures {
    /// The named Runs and the cut rows, with no conflicting row.
    #[must_use]
    pub fn new(runs: Vec<Blocker>, torn: Vec<String>) -> Self {
        Self {
            runs,
            torn,
            conflicts: Vec::new(),
        }
    }

    /// Nothing blocks a new unknown-cost Run.
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.runs.is_empty() && self.torn.is_empty() && self.conflicts.is_empty()
    }
}

/// [`fold_as`] for a lease holder named by its `host` alone, with no kernel
/// identity: writers are judged by hostname only, as before boot identities.
///
/// # Errors
/// As [`fold_as`].
pub fn fold(nika: &OwnedDir, host: &str, observer: &str) -> std::io::Result<Exposures> {
    let holder = Writer {
        pid: std::process::id(),
        host: host.to_owned(),
        boot: None,
    };
    fold_as(nika, &holder, observer)
}

/// Fold the journal while `holder` holds the lease: append the UNKNOWN row of
/// every leased invocation that never settled and whose writer the holder can
/// judge ([`Writer`]'s host or kernel), then return every exposure.
///
/// # Errors
/// An unreadable, oversized or unrecognized journal (`InvalidData`: prior
/// exposure is unknown), or an UNKNOWN row that cannot be appended.
pub fn fold_as(nika: &OwnedDir, holder: &Writer, observer: &str) -> std::io::Result<Exposures> {
    fold_deriving(nika, holder, observer).map(|(exposures, _)| exposures)
}

/// [`fold_as`], also naming each UNKNOWN row it derived and appended: the
/// invocation and the sha256 of the appended line (an inspection says so).
fn fold_deriving(
    nika: &OwnedDir,
    holder: &Writer,
    observer: &str,
) -> std::io::Result<(Exposures, Vec<(String, String)>)> {
    let bytes = read(nika)?;
    derive(nika, &bytes, holder, observer, |row| append_row(nika, row))
}

fn derive(
    nika: &OwnedDir,
    bytes: &[u8],
    holder: &Writer,
    observer: &str,
    mut append: impl FnMut(&str) -> std::io::Result<()>,
) -> std::io::Result<(Exposures, Vec<(String, String)>)> {
    let (standings, torn, conflicts) = judge(bytes)?;
    let mut exposures = Exposures::new(Vec::new(), torn);
    exposures.conflicts = conflicts;
    let mut derived_rows = Vec::new();
    for (invocation, standing) in standings {
        let exposure = match standing {
            Standing::Settled(_, true) => Exposure::Uncertain,
            Standing::Settled(_, false) | Standing::Reconciled(_) => continue,
            Standing::Unknown(pid, _) => Exposure::Unknown { pid },
            Standing::Held(_) => Exposure::StillUnknown,
            // `prepared`: the lease the holder acquired proves a leased writer
            // on its host or kernel is gone; anything else cannot be judged.
            Standing::Prepared(row, line) if holder.judges(&row["lease"]) => {
                let derived = unknown_row(&invocation, &row, line, observer).to_string();
                append(&derived).map_err(|e| {
                    std::io::Error::new(
                        e.kind(),
                        format!("cannot record an unsettled Run as unknown: {e}"),
                    )
                })?;
                let digest = nika_event::source_id::sha256_hex(derived.as_bytes());
                derived_rows.push((invocation.clone(), digest));
                Exposure::Unknown {
                    pid: row["lease"]["pid"].as_u64(),
                }
            }
            _ => Exposure::Unjudged,
        };
        let trace = trace_of(nika, &invocation);
        let mut blocker = Blocker::new(invocation, exposure);
        blocker.trace = trace;
        exposures.runs.push(blocker);
    }
    Ok((exposures, derived_rows))
}

/// The trace file that recorded an `exe-<uuid>` invocation: a store name of
/// that trace id whose first frame names the same execution. Diagnostic only
/// (any failure is `None`): the refusal hands it to `nika trace show`.
fn trace_of(nika: &OwnedDir, invocation: &str) -> Option<String> {
    let id = uuid::Uuid::parse_str(invocation.strip_prefix("exe-")?).ok()?;
    let traces = nika.open_below(&["traces"]).ok()?;
    let mut names = traces.names().ok()?;
    names.sort();
    names.into_iter().find(|name| {
        let mut first = Vec::new();
        crate::journal::may_name_trace(name, id)
            && traces
                .open_relative(Path::new(name))
                .and_then(|file| file.take(65_536).read_to_end(&mut first))
                .is_ok()
            && first.split(|b| *b == b'\n').next().is_some_and(|line| {
                serde_json::from_slice::<serde_json::Value>(line).is_ok_and(|frame| {
                    frame["execution"]["uuid"].as_str() == Some(&id.hyphenated().to_string())
                })
            })
    })
}

/// Every exposure, named by identity, and the one gesture this binary has:
/// `nika trace cost` inspects them (P4 · its `reconcile` appends a resolution).
#[must_use]
pub fn refusal(exposures: &Exposures) -> String {
    // Each Run by its journal identity (escaped: journal text is never a
    // terminal control) and, when one recorded it, its trace. A writer is
    // only said to have let the lease go: the lease proves no more, and the
    // pid may be this very process (an earlier Run whose settlement failed).
    let runs = exposures.runs.iter().map(|b| {
        let run = match &b.trace {
            Some(trace) => format!(
                "Run {} (trace .nika/traces/{})",
                crate::escape_tty(&b.invocation),
                crate::escape_tty(trace)
            ),
            None => format!("Run {}", crate::escape_tty(&b.invocation)),
        };
        match b.exposure {
            Exposure::Unknown { pid: Some(pid) } => format!(
                "{run} ended without a settlement; its writer, process {pid}, no longer holds the cost lease: billing unknown"
            ),
            Exposure::Unknown { pid: None } => format!(
                "{run} ended without a settlement and its writer no longer holds the cost lease: billing unknown"
            ),
            Exposure::Uncertain => {
                format!("{run} settled with a sent request whose charge is unknown")
            }
            Exposure::Unjudged => format!(
                "{run} was admitted and never settled, and this host cannot judge its writer"
            ),
            Exposure::StillUnknown => {
                format!("{run} was reconciled as still unknown: billing unknown")
            }
        }
    });
    let torn = exposures.torn.iter().map(|sha256| {
        format!(
            "a row was cut mid-write (sha256 {sha256}): the Run that wrote it may have been billed"
        )
    });
    // The invocation a refused row names is journal text: never a terminal control.
    let conflicts = exposures.conflicts.iter().map(|c| {
        format!(
            "a row for Run {} (sha256 {}) {}: refused as a conflict, the exposure before it stands",
            crate::escape_tty(&c.invocation),
            c.sha256,
            c.reason
        )
    });
    let named: Vec<String> = runs.chain(torn).chain(conflicts).collect();
    format!(
        "an earlier unknown-cost Run may have been billed: {} · no automatic retry: a new unknown-cost Run in this project waits until that exposure is reconciled (evidence: .nika/{JOURNAL} · inspect it: `nika trace cost`)",
        named.join(" · ")
    )
}

/// Append one row after terminating a torn tail, so a row a killed writer left
/// half-written never fuses with the next one: its bytes stay, on their own
/// line, and the fold names them.
///
/// # Errors
/// The journal cannot be read or appended safely.
pub fn append_row(nika: &OwnedDir, row: &str) -> std::io::Result<()> {
    if torn_tail(nika)? {
        nika.append_line(JOURNAL, "")?;
    }
    nika.append_line(JOURNAL, row)
}

fn torn_tail(nika: &OwnedDir) -> std::io::Result<bool> {
    let mut file = match nika.open_relative(Path::new(JOURNAL)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        other => other?,
    };
    if file.metadata()?.len() == 0 {
        return Ok(false);
    }
    file.seek(std::io::SeekFrom::End(-1))?;
    let mut last = [0_u8; 1];
    file.read_exact(&mut last)?;
    Ok(last[0] != b'\n')
}

/// The journal's bytes, never decoded whole: a writer cut inside a multi-byte
/// character leaves one torn line, not an unreadable journal.
fn read(nika: &OwnedDir) -> std::io::Result<Vec<u8>> {
    let file = match nika.open_relative(Path::new(JOURNAL)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        other => other?,
    };
    read_file(file)
}

fn read_file(file: impl std::io::Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(1_048_577).read_to_end(&mut bytes)?;
    if bytes.len() > 1_048_576 {
        return Err(invalid("cost observation journal exceeds the read bound"));
    }
    Ok(bytes)
}

fn invalid(reason: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, reason)
}

/// Each non-blank line's exact bytes (a `\r` before the newline excluded, as
/// `str::lines` reads it), in journal order.
fn lines(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes
        .split(|b| *b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
}

/// One Run's standing, advanced only by a legal next row.
enum Standing<'a> {
    /// Admitted: the row and its exact bytes.
    Prepared(serde_json::Value, &'a [u8]),
    /// Settled by the writer that prepared it: the row's exact bytes, and
    /// whether a sent request's charge stayed unknown.
    Settled(&'a [u8], bool),
    /// Recorded unknown: the pid the lease named, and the row's exact bytes.
    Unknown(Option<u64>, &'a [u8]),
    /// Reconciled as still unknown: that reconciliation's exact bytes, now the
    /// Run's latest row (the one a later reconciliation must name). Blocks.
    Held(&'a [u8]),
    /// Reconciled as billed or not billed: the row's exact bytes. Clear.
    Reconciled(&'a [u8]),
}

type Judged<'a> = (BTreeMap<String, Standing<'a>>, Vec<String>, Vec<Conflict>);

/// Every Run's standing under the legal transitions: `prepared`, then either
/// `settled` by the same lease writer or the `unknown` derived from that exact
/// row, and nothing after either but a byte-identical repeat. A row no
/// transition admits, or whose observation contradicts its own account, is a
/// conflict and changes no standing. Lines that are not UTF-8 or not JSON are
/// torn, named by digest; a JSON row this engine cannot read makes prior
/// exposure unknown.
fn judge(bytes: &[u8]) -> std::io::Result<Judged<'_>> {
    let (mut standings, mut torn, mut conflicts) = (BTreeMap::new(), Vec::new(), Vec::new());
    // A lease-less `prepared`/`settled` row is legacy evidence only while no
    // leased row came before it; after that no engine writes one.
    let mut leased = false;
    for line in lines(bytes) {
        let parsed = std::str::from_utf8(line).ok().map(serde_json::from_str);
        let Some(Ok::<serde_json::Value, _>(row)) = parsed else {
            torn.push(nika_event::source_id::sha256_hex(line));
            continue;
        };
        let id = strict(&row)?;
        let lease_less = row["phase"] != "unknown" && row["lease"].is_null();
        let next = if lease_less && leased {
            Err("carries no cost lease after leased rows began")
        } else {
            leased |= !row["lease"].is_null();
            // The transition law speaks first; a row it admits must also be
            // an observation its account could have written.
            let written = consistent(&row);
            advance(standings.get(&id), row, line).and_then(|next| written.map(|()| next))
        };
        match next {
            Ok(standing) => drop(standings.insert(id, standing)),
            Err(why) => conflicts.push(Conflict::new(
                id,
                nika_event::source_id::sha256_hex(line),
                why,
            )),
        }
    }
    Ok((standings, torn, conflicts))
}

/// The row's invocation, once its schema, phase and observation read strictly.
fn strict(row: &serde_json::Value) -> std::io::Result<String> {
    let id = row["invocation"]
        .as_str()
        .ok_or_else(|| invalid("unreadable cost invocation"))?
        .to_owned();
    if row["schema"] != "nika/run-cost-observation@1" {
        return Err(invalid("unrecognized cost observation"));
    }
    // A derived unknown carries the account's last words as `prior_observation`
    // (an earlier engine wrote them as `observation`): never a current state.
    let (observation, phase_ok) = match row["phase"].as_str() {
        Some("prepared" | "settled") => (&row["observation"], true),
        Some("unknown") => (
            row.get("prior_observation").unwrap_or(&row["observation"]),
            row["unsettled"]["prior_sha256"].is_string(),
        ),
        // A reconciliation carries no observation: it reads once it names its
        // contract and the row it resolves. What it claims is judged in
        // `advance` (an engine before this phase finds it unreadable and
        // refuses: it never mistakes a reconciliation for a clear journal).
        Some("reconciled") => {
            let reconciliation = &row["reconciliation"];
            return (reconciliation["schema"] == RECONCILIATION
                && reconciliation["prior_sha256"].is_string())
            .then_some(id)
            .ok_or_else(|| invalid("unreadable cost reconciliation; prior exposure is unknown"));
        }
        _ => (&row["observation"], false),
    };
    if !phase_ok || !admission::observation_readable(observation) {
        return Err(invalid(
            "unreadable cost observation; prior exposure is unknown",
        ));
    }
    Ok(id)
}

/// Whether a `prepared` or `settled` row's observation is one its account could
/// have written, or why not (nika-providers `admission::observation_consistent`,
/// beside `InferenceReceipt::observation`, the same serializer since the
/// journal's first writer), and whether it fits its phase: the account is
/// untouched when the host writes `prepared` (right after the review confirms
/// the choice) and closed before it writes `settled`. A derived `unknown` row
/// carries prior history: it is judged through the exact `prepared` row it names.
fn consistent(row: &serde_json::Value) -> Result<(), &'static str> {
    let prepared = match row["phase"].as_str() {
        Some("prepared") => true,
        Some("settled") => false,
        _ => return Ok(()),
    };
    let (state, moved) = admission::observation_consistent(&row["observation"])?;
    let open = state == admission::AdmissionState::Open;
    if prepared && (moved || !open) {
        return Err("prepares the Run with an account that already moved");
    }
    if !prepared && open {
        return Err("settles the Run with its account still open");
    }
    Ok(())
}

/// The Run's next standing, or why no legal transition admits the row.
fn advance<'a>(
    standing: Option<&Standing<'a>>,
    row: serde_json::Value,
    line: &'a [u8],
) -> Result<Standing<'a>, &'static str> {
    let phase = row["phase"].as_str().unwrap_or_default();
    match (standing, phase) {
        (None, "prepared") => Ok(Standing::Prepared(row, line)),
        (None, _) => Err("names a Run with no prepared row before it"),
        (Some(Standing::Prepared(prepared, _)), "settled") if row["lease"] == prepared["lease"] => {
            Ok(Standing::Settled(
                line,
                row["observation"]["state"] == "Uncertain",
            ))
        }
        (Some(Standing::Prepared(..)), "settled") => {
            Err("settles a Run from a writer that did not prepare it")
        }
        (Some(Standing::Prepared(prepared, prior)), "unknown")
            if row["unsettled"]["writer"] == prepared["lease"]
                && row["unsettled"]["prior_sha256"]
                    == nika_event::source_id::sha256_hex(prior).as_str() =>
        {
            Ok(Standing::Unknown(prepared["lease"]["pid"].as_u64(), line))
        }
        (Some(Standing::Prepared(..)), "unknown") => {
            Err("records unknown from a row other than the Run's prepared one")
        }
        (Some(Standing::Prepared(..)), "reconciled") => {
            Err("reconciles a Run that has not settled or been recorded unknown")
        }
        (Some(Standing::Prepared(..)), _) => Err("prepares the Run a second time"),
        // A byte-identical repeat of the Run's latest row (a retried append).
        (Some(&Standing::Settled(settled, uncertain)), _) if settled == line => {
            Ok(Standing::Settled(settled, uncertain))
        }
        (Some(&Standing::Unknown(pid, recorded)), _) if recorded == line => {
            Ok(Standing::Unknown(pid, recorded))
        }
        (Some(&Standing::Held(held)), _) if held == line => Ok(Standing::Held(held)),
        (Some(&Standing::Reconciled(done)), _) if done == line => Ok(Standing::Reconciled(done)),
        (
            Some(
                &(Standing::Settled(head, true)
                | Standing::Unknown(_, head)
                | Standing::Held(head)),
            ),
            "reconciled",
        ) => reconciled(head, &row, line),
        (Some(Standing::Settled(..)), "reconciled") => {
            Err("reconciles a Run that settled without uncertainty")
        }
        (Some(Standing::Settled(..)), _) => Err("follows the Run's settlement"),
        (Some(Standing::Unknown(..)), _) => Err("follows the Run's recorded unknown"),
        (Some(Standing::Held(..) | Standing::Reconciled(..)), _) => {
            Err("follows the Run's reconciliation")
        }
    }
}

/// A `reconciled` row's next standing (P4), or why it cannot resolve the Run.
/// It names the Run's latest row by the digest of its exact bytes, carries the
/// cost lease its writer held, the one supported evidence class (the operator's
/// own attestation, never verified provider billing), a local principal and an
/// observation time, and copies the facts the Run's own rows record
/// ([`facts`]). `still_unknown` keeps the Run blocking with this row as its
/// latest; `billed` and `not_billed` end its exposure (they authorize nothing:
/// the next Run still faces its own fresh review).
fn reconciled<'a>(
    head: &'a [u8],
    row: &serde_json::Value,
    line: &'a [u8],
) -> Result<Standing<'a>, &'static str> {
    let claim = &row["reconciliation"];
    if claim["prior_sha256"] != nika_event::source_id::sha256_hex(head).as_str() {
        return Err("reconciles a row other than the Run's latest");
    }
    if !(row["lease"]["pid"].is_u64() && row["lease"]["host"].is_string()) {
        return Err("reconciles without holding the cost lease");
    }
    let evidence = &claim["evidence"];
    if evidence["class"] != "operator_attestation"
        || evidence["verified"] != false
        || !evidence["reference"].as_str().is_some_and(reference_reads)
    {
        return Err("carries evidence this engine does not support");
    }
    let principal = &claim["principal"];
    if principal["kind"] != "local_account"
        || principal["uid"]
            .as_u64()
            .is_none_or(|uid| u32::try_from(uid).is_err())
        || !(principal["name"].is_null() || principal["name"].is_string())
    {
        return Err("names no local principal");
    }
    if claim["observed_at"]
        .as_str()
        .is_none_or(|at| at.parse::<jiff::Timestamp>().is_err())
    {
        return Err("records no readable observation time");
    }
    let recorded = serde_json::from_slice::<serde_json::Value>(head)
        .map_or(serde_json::Value::Null, |head| facts(&head));
    if ["route", "provider_request_ids", "window"]
        .iter()
        .any(|fact| claim[*fact] != recorded[*fact])
    {
        return Err("names a route or request its Run does not record");
    }
    match claim["resolution"].as_str() {
        Some("billed" | "not_billed") => Ok(Standing::Reconciled(line)),
        Some("still_unknown") => Ok(Standing::Held(line)),
        _ => Err("names a resolution this engine does not know"),
    }
}

/// An operator's evidence reference: 1 to 512 characters, no control character
/// (journal text is rendered to terminals).
fn reference_reads(reference: &str) -> bool {
    (1..=512).contains(&reference.chars().count()) && !reference.chars().any(char::is_control)
}

/// What the Run's own rows record about the request that may have been
/// billed, as a reconciliation must copy it (never typed by an operator): the
/// route the unknown-cost choice named, every provider request id its attempts
/// saw, and the window its execution ids bound. The window's basis is the
/// `UUIDv7` time of the invocation (`not_before`) and of the review that derived
/// its unknown (`not_after`): id times, never a measured request timestamp. A
/// reconciliation row's own copies are the facts of a Run it holds.
fn facts(head: &serde_json::Value) -> serde_json::Value {
    if head["phase"] == "reconciled" {
        let claim = &head["reconciliation"];
        return serde_json::json!({"route": claim["route"],
            "provider_request_ids": claim["provider_request_ids"], "window": claim["window"]});
    }
    let observation = match head["phase"].as_str() {
        Some("unknown") => head
            .get("prior_observation")
            .unwrap_or(&head["observation"]),
        _ => &head["observation"],
    };
    let route = admission::observation_route(observation);
    let ids = admission::observation_request_ids(observation);
    serde_json::json!({"route": route, "provider_request_ids": ids, "window": {
        "basis": "uuidv7-execution-ids",
        "not_before": uuid_time(head["invocation"].as_str()),
        "not_after": uuid_time(head["unsettled"]["observed_by"].as_str()),
    }})
}

/// The RFC 3339 time a `UUIDv7` `exe-` execution id carries, when it is one.
fn uuid_time(id: Option<&str>) -> Option<String> {
    let uuid = uuid::Uuid::parse_str(id?.strip_prefix("exe-")?).ok()?;
    if uuid.get_version_num() != 7 {
        return None;
    }
    let (secs, nanos) = uuid.get_timestamp()?.to_unix();
    let time = jiff::Timestamp::new(i64::try_from(secs).ok()?, i32::try_from(nanos).ok()?);
    time.ok().map(|time| time.to_string())
}

/// The derived row: the last observation verbatim as `prior_observation` (the
/// account's own words when the Run was prepared: its `Open` state and zero
/// counters are history, never the Run's current state), why it is unknown
/// (`interrupted`, the resident's word for lost ownership), the writer the
/// lease named, the digest of the exact row it was derived from and the
/// review that observed it — evidence a later reconciliation cites.
fn unknown_row(
    invocation: &str,
    row: &serde_json::Value,
    line: &[u8],
    observer: &str,
) -> serde_json::Value {
    serde_json::json!({
        "schema": "nika/run-cost-observation@1",
        "invocation": invocation,
        "phase": "unknown",
        "prior_observation": row["observation"],
        "unsettled": {
            "cause": "interrupted",
            "writer": row["lease"],
            "prior_sha256": nika_event::source_id::sha256_hex(line),
            "observed_by": observer,
        },
    })
}

pub mod reconcile;

#[cfg(test)]
mod reconcile_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod custody_tests;
