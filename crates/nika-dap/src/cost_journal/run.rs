// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One unknown-cost Run's custody of the journal, shared by every host that
//! reviews one (the CLI's terminal, Serve's cost-review door). Descended from
//! nika-cli-host's `run_cost` (C6, 2026-09-28) on the laws above, used and
//! never changed: [`super::take_at`], the lease's own read, fold and append,
//! and `refusal`.
//!
//! [`clear`] takes the lease from the project descriptor the host opened
//! before `.nika/` and folds what earlier Runs left through the locked journal
//! inode; [`RunJournal`] then appends that Run's rows through the SAME inode
//! (never by reopening a name: a renamed or replaced journal or `.nika/` never
//! receives a settlement, and the original keeps its rows), reading the live
//! account at every row, so its final `settled` row is the account's own last
//! word and never a stale `prepared` snapshot.

use super::{Exposures, Lease, Taken, Writer};

pub(super) mod review;
use nika_fs::OwnedDir;
use std::io::Read as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// The live account a Run's rows observe. The host owns it (the account
/// lives with the providers); the journal reads it at every row.
pub trait RunAccount: Send + Sync {
    /// The account's current observation, as the row's `observation`.
    ///
    /// # Errors
    /// An unreadable account: nothing is appended.
    fn observation(&self) -> std::io::Result<serde_json::Value>;

    /// Close live authority before the Run's final row: after it, a fresh
    /// decision is required.
    ///
    /// # Errors
    /// The account cannot close.
    fn close(&self, why: &str) -> std::io::Result<()>;
}

/// The journal's exact bytes at one moment: what a review binds after its
/// own clear, and what an admission must read again.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct JournalWitness {
    /// The journal's length in bytes (0 when it does not exist).
    pub length: u64,
    /// The sha256 of those bytes.
    pub sha256: String,
}

/// The directory identity (device, inode) the kernel reports for a held descriptor.
type Identity = (u64, u64);

/// A project whose cost lease this process holds and whose earlier Runs all
/// settled clear: the one state a new unknown-cost Run may start from. It
/// keeps the project root and its `.nika/` as the descriptors the lease was
/// taken through.
#[derive(Debug)]
pub struct Cleared {
    root: OwnedDir,
    nika: OwnedDir,
    writer: Writer,
    lease: Lease,
}

/// Why a new unknown-cost Run cannot start. The lease is not held.
#[derive(Debug)]
#[non_exhaustive]
pub enum Blocked {
    /// A live process holds the lease: a Run in flight or a review awaiting
    /// its answer.
    Busy {
        /// The holder's recorded pid, when readable.
        pid: Option<u64>,
    },
    /// Earlier Runs still block (their rows, named by [`super::refusal`]).
    Exposed(Exposures),
}

impl std::fmt::Display for Blocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy { pid } => {
                let holder = pid.map_or_else(
                    || "an unnamed process".to_owned(),
                    |pid| format!("process {pid}"),
                );
                write!(
                    f,
                    "{holder} holds this project's cost lease: an unknown-cost Run in flight or a review waiting for its answer · no second Run, no automatic retry"
                )
            }
            Self::Exposed(exposures) => f.write_str(&super::refusal(exposures)),
        }
    }
}

/// Take the project's cost lease and read what earlier Runs left, before any
/// question: `.nika/` is created below `root` when absent, and the fold may
/// append the UNKNOWN row of a Run whose writer the lease proves gone.
///
/// # Errors
/// The directory, the lease or the journal cannot be read or written safely.
pub fn clear(root: &OwnedDir, observer: &str) -> std::io::Result<Result<Cleared, Blocked>> {
    let nika = root.create_below(&[".nika"])?;
    let writer = Writer::this_process();
    let lease = match super::take_at(root, &nika, &writer)? {
        Taken::Held(lease) => lease,
        Taken::Busy { pid } => return Ok(Err(Blocked::Busy { pid })),
    };
    let exposures = lease.fold_as(&nika, &writer, observer)?;
    if !exposures.is_clear() {
        return Ok(Err(Blocked::Exposed(exposures)));
    }
    Ok(Ok(Cleared {
        root: root.try_clone()?,
        nika,
        writer,
        lease,
    }))
}

impl Cleared {
    /// The journal's exact bytes now, read through the locked journal inode.
    ///
    /// # Errors
    /// An unreadable or oversized journal.
    pub fn journal(&self) -> std::io::Result<JournalWitness> {
        let bytes = self.lease.read()?;
        Ok(JournalWitness {
            length: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            sha256: nika_event::source_id::sha256_hex(&bytes),
        })
    }

    /// Observe one project file the Run binds, through the held root (never a
    /// path reopened): a read input (`write: None`) answers the sha256 of its
    /// bytes, at most 1 MiB; a write target (`Some(creates_dirs)`) re-observes
    /// its contained parent, never creating it, and answers `None`.
    ///
    /// # Errors
    /// An unreadable or oversized input, a parent that is missing (unless the
    /// write creates it) or not a real contained directory, or a target that is
    /// not a contained regular file.
    pub fn observe_file(
        &self,
        path: &Path,
        write: Option<bool>,
    ) -> std::io::Result<Option<String>> {
        let Some(creates) = write else {
            let mut bytes = Vec::new();
            self.root
                .open_relative(path)?
                .take(1_048_577)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 1_048_576 {
                return Err(std::io::Error::other(
                    "unknown-cost Run read input exceeds the 1 MiB review bound",
                ));
            }
            return Ok(Some(nika_event::source_id::sha256_hex(&bytes)));
        };
        let parts = path
            .iter()
            .map(|s| {
                s.to_str()
                    .ok_or_else(|| std::io::Error::other("non-UTF-8 path"))
            })
            .collect::<std::io::Result<Vec<_>>>()?;
        let (name, parents) = parts
            .split_last()
            .ok_or_else(|| std::io::Error::other("empty write path"))?;
        if let Some(parent) = write_parent(&self.root, parents, creates)? {
            match parent.open_relative(Path::new(name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(std::io::Error::other(format!(
                        "write target is not a contained regular file: {e}"
                    )));
                }
                _ => {}
            }
        }
        Ok(None)
    }

    /// Whether `root` is still the directory this lease was taken in: the
    /// path opened now, and its `.nika/`, are the very objects held. A
    /// directory replaced at the same path (moved away, copied back) is not.
    ///
    /// # Errors
    /// The held descriptors cannot report their identity.
    pub fn same_place(&self, root: &Path) -> std::io::Result<bool> {
        let held = (identity(&self.root)?, identity(&self.nika)?);
        let Ok(now) = OwnedDir::open(root) else {
            return Ok(false);
        };
        let Ok(nika) = now.open_below(&[".nika"]) else {
            return Ok(false);
        };
        Ok((identity(&now)?, identity(&nika)?) == held)
    }
}

/// The write target's contained parent, walked one component at a time and
/// never created here: an existing component must be a real directory (a
/// symlink or a file refuses); a missing one is accepted only when the write
/// itself declares `create_dirs: true`, and then nothing below it exists yet to
/// re-observe (`None`). The review runs this again after the answer, so a
/// component that changes kind meanwhile refuses then.
fn write_parent(
    directory: &OwnedDir,
    parents: &[&str],
    creates: bool,
) -> std::io::Result<Option<OwnedDir>> {
    let mut current = directory.try_clone()?;
    for (depth, component) in parents.iter().enumerate() {
        let named = || parents[..=depth].join("/");
        match current.open_below(&[component]) {
            Ok(next) => current = next,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && creates => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(std::io::Error::other(format!(
                    "write target parent `{}` does not exist and the write does not create it (`create_dirs: true` would)",
                    named()
                )));
            }
            Err(e) => {
                return Err(std::io::Error::other(format!(
                    "write target parent `{}` is not a contained directory: {e}",
                    named()
                )));
            }
        }
    }
    Ok(Some(current))
}

fn identity(dir: &OwnedDir) -> std::io::Result<Identity> {
    use std::os::unix::fs::MetadataExt as _;
    let meta = dir.as_file().metadata()?;
    Ok((meta.dev(), meta.ino()))
}

/// The Run's side of the journal: it holds the lease from before its
/// `prepared` row until after its `settled` one, and every row names its
/// writer. A Run that ends without [`Self::settle`] (an early return, an
/// unwinding panic) settles, best effort, what its account observed when
/// this drops. When that settlement cannot be written (the account cannot
/// close, the append fails) or the process dies, `prepared` stays behind, and
/// the released lease lets the next review record that Run as unknown.
pub struct RunJournal {
    writer: Writer,
    invocation: String,
    account: Box<dyn RunAccount>,
    /// Set by the first settlement attempt, whatever its outcome.
    attempted: AtomicBool,
    settled: AtomicBool,
    lease: Lease,
}

impl std::fmt::Debug for RunJournal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunJournal")
            .field("invocation", &self.invocation)
            .field("writer", &self.writer)
            .field("attempted", &self.attempted.load(Ordering::SeqCst))
            .field("settled", &self.settled.load(Ordering::SeqCst))
            .finish_non_exhaustive()
    }
}

impl RunJournal {
    /// The journal of one confirmed Run, over the lease its review cleared.
    #[must_use]
    pub fn new(cleared: Cleared, invocation: String, account: Box<dyn RunAccount>) -> Self {
        Self {
            writer: cleared.writer,
            invocation,
            account,
            attempted: AtomicBool::new(false),
            settled: AtomicBool::new(false),
            lease: cleared.lease,
        }
    }

    /// Append one row with the account's current observation, through the
    /// locked journal inode. No callable authority is serialized.
    ///
    /// # Errors
    /// An unreadable account or an unwritable journal (a journal with another
    /// link refuses; a short write or failed sync is uncertain, never retried).
    pub fn observe(&self, phase: &str) -> std::io::Result<()> {
        let observation = self.account.observation()?;
        let row = serde_json::json!({"schema": "nika/run-cost-observation@1",
            "invocation": self.invocation, "phase": phase, "observation": observation,
            "lease": self.writer.json()});
        self.lease.append_row(&row.to_string())
    }

    /// Close the account, then append the final `settled` row with what it
    /// observed, including uncertainty. Only the first call attempts it: a
    /// failed or uncertain closure or append (a short write, a failed sync)
    /// never earns a second one, so a later call succeeds only once that
    /// attempt succeeded and otherwise reports it as pending or unverified.
    ///
    /// # Errors
    /// Account closure or journal failure, or an earlier attempt not proven
    /// to have settled: the host must report possible billing.
    pub fn settle(&self) -> std::io::Result<()> {
        if self.attempted.swap(true, Ordering::SeqCst) {
            if self.settled.load(Ordering::SeqCst) {
                return Ok(());
            }
            return Err(std::io::Error::other(
                "cost settlement was already attempted; its journal outcome is pending or unverified",
            ));
        }
        self.account.close("Run ended; fresh decision required")?;
        self.observe("settled")?;
        self.settled.store(true, Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for RunJournal {
    fn drop(&mut self) {
        if !self.attempted.load(Ordering::SeqCst) {
            // Best effort while the lease is still held, and only when no
            // settlement was attempted: a failed one is never repeated. What it
            // left (the `prepared` row, a torn tail) the next review records as
            // unknown.
            let _ = self.settle();
        }
    }
}

#[cfg(test)]
mod tests;
