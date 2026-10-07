// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The room's filesystem side. A scratch directory of the rehearsal's own is made exclusively,
//! owner-only, under the scratch parent: it holds the room and an empty directory the candidate
//! is admitted over. The originals are read through a rooted backend whose ledger is past every
//! writing phase, so no write can reach them; each read is bounded before any allocation, and
//! never follows a symlink nor waits on a special file. The room is served by the rooted backend
//! under its own ledger, each phase sealed and drained before the next. The scratch directory is
//! removed, and its absence verified.

use std::io;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use nika_compile_cognition::rehearse::{
    CopyReceipt, Digest, FinalState, Held, LedgerFacts, Refusal,
};
use nika_fs::{EffectLedger, LedgerRefusal, OwnedDir, Phase, RoomLimits, RootedFs};
use nika_kernel::fs::{FsError, FsMetaDyn as _, FsReadDyn as _, FsWriteDyn as _};

use super::ObservedRoom;
use super::screen::Refused;

/// What one room may be written in total, the copied world and every publish of the run.
pub(super) const ROOM_BYTES: u64 = 4 * ObservedRoom::COPY_BOUND;
/// Files and directories one room may be given in total.
pub(super) const ROOM_FILES: u64 = 512;
/// How many fresh names a scratch directory is tried under before the host gives up.
const ATTEMPTS: u32 = 16;

static NEXT: AtomicU64 = AtomicU64::new(0);

/// One rehearsal's scratch directory, removed when dropped if nothing removed it before.
pub(super) struct Scratch {
    top: PathBuf,
    removed: bool,
}

impl Scratch {
    /// A fresh scratch directory under `parent`, made exclusively and owner-only, with the room
    /// and the admission directory in it, both held open.
    pub(super) fn create(parent: &Path) -> io::Result<(Self, OwnedDir, OwnedDir)> {
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        let mut attempt = 0;
        let scratch = loop {
            let name = format!(
                "nika-rehearsal-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let top = parent.join(name);
            match builder.create(&top) {
                Ok(()) => {
                    break Self {
                        top,
                        removed: false,
                    };
                }
                Err(error)
                    if error.kind() == io::ErrorKind::AlreadyExists && attempt < ATTEMPTS =>
                {
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        };
        builder.create(scratch.top.join("room"))?;
        builder.create(scratch.top.join("admit"))?;
        let room = OwnedDir::open(&scratch.top.join("room"))?;
        let admit = OwnedDir::open(&scratch.top.join("admit"))?;
        Ok((scratch, room, admit))
    }

    /// Where the room is, for display.
    pub(super) fn room(&self) -> PathBuf {
        self.top.join("room")
    }

    /// Remove the whole scratch directory, then verify that nothing is left at its name.
    pub(super) fn remove(mut self) -> bool {
        self.removed = true;
        let _ = std::fs::remove_dir_all(&self.top);
        matches!(std::fs::symlink_metadata(&self.top), Err(error) if error.kind() == io::ErrorKind::NotFound)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.removed {
            let _ = std::fs::remove_dir_all(&self.top);
        }
    }
}

/// The originals of `project`, read through a rooted backend whose ledger is past every writing
/// phase (no write can reach them), each read capped at the copy bound.
pub(super) async fn originals(project: &Path) -> Result<RootedFs, Refused> {
    let unreadable = |why: String| {
        Refused::new(
            Refusal::CopyIn,
            format!("the observed project cannot be read: {why}"),
        )
    };
    let held = OwnedDir::open(project).map_err(|error| unreadable(error.to_string()))?;
    let ledger = EffectLedger::new(RoomLimits::new(ObservedRoom::COPY_BOUND, 0));
    for _ in 0..2 {
        next_phase(&ledger)
            .await
            .map_err(|refusal| unreadable(refusal.to_string()))?;
    }
    Ok(RootedFs::new(held, ledger))
}

/// Copy every observed input into `room`, whole and bounded in total, never through a symlink
/// nor from a special file, each copy read back from the room: a receipt per input, in order.
pub(super) async fn copy_in(
    originals: &RootedFs,
    room: &RootedFs,
    (inputs, write_only): (&[(String, String)], &[String]),
) -> Result<Vec<CopyReceipt>, Refused> {
    let mut receipts = Vec::with_capacity(inputs.len());
    let mut total = 0_u64;
    for (named, at) in inputs {
        let path = Path::new(at);
        let refused =
            |why: &str| Refused::new(Refusal::CopyIn, format!("the observed input {named} {why}"));
        let found = match originals.metadata(path).await {
            Err(FsError::NotFound { .. }) if write_only.contains(at) => continue,
            found => found.map_err(|error| refused(&unreadable(&error)))?,
        };
        if !found.is_file {
            return Err(refused("is not a regular file"));
        }
        let bytes = originals
            .read(path)
            .await
            .map_err(|error| refused(&unreadable(&error)))?;
        total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if total > ObservedRoom::COPY_BOUND {
            return Err(Refused::new(
                Refusal::CopyIn,
                format!(
                    "the observed inputs pass the copy bound of {} bytes: a cut world is never \
                     rehearsed",
                    ObservedRoom::COPY_BOUND
                ),
            ));
        }
        room.write(path, &bytes)
            .await
            .map_err(|error| refused(&format!("could not be copied into the room: {error}")))?;
        let copied = room
            .read(path)
            .await
            .map_err(|error| refused(&format!("could not be read back in the room: {error}")))?;
        if copied != bytes {
            return Err(refused("differs in the room from its original"));
        }
        let held = Held::of(&copied, ObservedRoom::PREVIEW_BOUND);
        receipts.push(CopyReceipt::new(
            named.clone(),
            Digest::of(&bytes),
            Some(Digest::of(&copied)),
            held,
        ));
    }
    Ok(receipts)
}

/// Why an original could not be read, in the words of what it is.
fn unreadable(error: &FsError) -> String {
    match error {
        FsError::SymlinkRefused { .. } => {
            "goes through a symlink, which the room never follows".to_owned()
        }
        FsError::NotFound { .. } => "is absent".to_owned(),
        FsError::InvalidData { .. } => {
            "is not a regular file read whole within the copy bound".to_owned()
        }
        other => format!("could not be read: {other}"),
    }
}

/// What the room holds at `at`, read through the room in its read-back phase.
pub(super) async fn final_state(room: &RootedFs, at: &str) -> FinalState {
    let path = Path::new(at);
    match room.metadata(path).await {
        Err(FsError::NotFound { .. }) => FinalState::Absent,
        Err(_) => FinalState::Unreadable,
        Ok(found) if found.is_dir => FinalState::Directory,
        Ok(found) if !found.is_file => FinalState::Unreadable,
        Ok(_) => match room.read(path).await {
            Ok(bytes) => FinalState::File {
                digest: Digest::of(&bytes),
                held: Held::of(&bytes, ObservedRoom::PREVIEW_BOUND),
            },
            Err(_) => FinalState::Unreadable,
        },
    }
}

/// Seal the ledger's current phase, join every operation it took, then move to the next one.
pub(super) async fn next_phase(ledger: &Arc<EffectLedger>) -> Result<Phase, LedgerRefusal> {
    let _drained = ledger.seal_and_drain().await;
    ledger.advance()
}

/// The ledger's own facts: what the run published, what arrived late, what was left or
/// panicked, and whether every phase drained.
pub(super) fn facts(ledger: &EffectLedger, drained: bool) -> LedgerFacts {
    let written = ledger
        .written()
        .map(|path| path.display().to_string())
        .collect();
    let mut facts = LedgerFacts::clean(written);
    facts.late_refused = ledger.late_refusals();
    facts.leftovers = ledger.leftovers();
    facts.panicked = ledger.panicked();
    facts.drained = drained;
    facts
}
