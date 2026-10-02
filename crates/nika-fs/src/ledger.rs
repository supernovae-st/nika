// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The effect ledger of one room: one lock over the phase, the seal, the
//! registered operations, the budget and the write evidence.
//!
//! - A phase is sealed before it is drained, under the lock, so an operation
//!   either registered before the seal (and the drain waits for it) or is
//!   refused without running and counted as late.
//! - A drain completes only once every operation any seal of the phase took
//!   has finished; a second, empty seal never completes the first one's
//!   drain, a drain of an earlier phase never drains the current one, and a
//!   drain dropped before it completes leaves the phase sealed.
//! - A panic is latched: every later drain of its phase reports it, and the
//!   room's total never forgets it, so no last drain ever reads as clean.
//! - A phase advances only after a completed drain, never back.
//! - A write belongs to the phase that authorized it: it registers in that
//!   phase, unsealed, or not at all.
//! - Nothing runs under the lock: no work, no destructor of refused work, no
//!   spawn and no wait. A refusal is decided and counted under the lock, and
//!   the refused work (with any reservation it owns) is destroyed after it.
//! - Only a claimed name in the run phase is write evidence; its usage stays
//!   committed whatever follows, and a temporary name left behind is counted.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use nika_kernel::fs::FsError;
use tokio::runtime::Handle;
use tokio::sync::oneshot;

/// The aggregate budget one room may hold: every write counts, copy-in,
/// overwrite and created directories included, and nothing is refunded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct RoomLimits {
    /// Bytes the room may be written, in total.
    pub bytes: u64,
    /// Files and created directories the room may be given, in total.
    pub files: u64,
}

impl RoomLimits {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(bytes: u64, files: u64) -> Self {
        Self { bytes, files }
    }
}

/// Where a room is in its life. A phase moves forward only, after a drain,
/// and is visited once: it is also the epoch of every write it authorizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Phase {
    /// Copy-in only: the observed inputs are written into the room.
    Preparation,
    /// The run: builtin operations only.
    Run,
    /// The host reads outputs back; every write is refused.
    ReadBack,
    /// Nothing is accepted.
    Closed,
}

/// Why the ledger refused an operation. Nothing ran when this is returned.
///
/// A verdict, not a coded error: the rooted filesystem answers it with the
/// coded [`FsError`] its caller sees, and a host maps it to its own outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LedgerRefusal {
    /// The phase was sealed before the operation registered, or the write was
    /// authorized in a phase that is no longer the current one.
    #[non_exhaustive]
    Sealed {
        /// The phase the operation belonged to.
        phase: Phase,
    },
    /// The phase accepts no write.
    #[non_exhaustive]
    ReadOnly {
        /// The current phase.
        phase: Phase,
    },
    /// The reservation exceeds what the room has left; nothing was taken.
    #[non_exhaustive]
    Budget {
        /// Bytes the room had left.
        bytes_left: u64,
        /// Files the room had left.
        files_left: u64,
    },
    /// The lifecycle was driven out of order: an advance before a completed
    /// drain, or past the last phase.
    #[non_exhaustive]
    Order {
        /// The current phase.
        phase: Phase,
    },
    /// No async runtime was current, so nothing could register.
    #[non_exhaustive]
    NoRuntime {
        /// The current phase.
        phase: Phase,
    },
}

impl fmt::Display for LedgerRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sealed { phase } => {
                write!(
                    f,
                    "the {phase:?} phase was sealed before the operation registered"
                )
            }
            Self::ReadOnly { phase } => write!(f, "the {phase:?} phase accepts no write"),
            Self::Budget {
                bytes_left,
                files_left,
            } => write!(
                f,
                "the room write budget is exhausted ({bytes_left} bytes and {files_left} files left)"
            ),
            Self::Order { phase } => {
                write!(
                    f,
                    "the room lifecycle was driven out of order in the {phase:?} phase"
                )
            }
            Self::NoRuntime { phase } => {
                write!(
                    f,
                    "no async runtime was current to register a {phase:?} operation"
                )
            }
        }
    }
}

/// One room's effect ledger.
#[derive(Debug)]
pub struct EffectLedger {
    state: Mutex<LedgerState>,
}

#[derive(Debug)]
struct LedgerState {
    phase: Phase,
    /// No registration is accepted: set by a seal, cleared by an advance.
    sealed: bool,
    /// Every operation the phase's seals took has finished.
    drained: bool,
    limits: RoomLimits,
    reserved_bytes: u64,
    reserved_files: u64,
    committed_bytes: u64,
    committed_files: u64,
    /// Completion signals of registered operations no seal has taken yet.
    registered: Vec<oneshot::Receiver<()>>,
    /// Operations a seal took whose drain has not completed.
    unjoined: usize,
    written: Vec<PathBuf>,
    late_refused: u64,
    leftovers: u64,
    /// Operations that panicked or never ran, per phase (`slot`), across all of
    /// the phase's drains: a latch, never reset.
    phase_panicked: [usize; 4],
}

/// The index of `phase` in the per-phase totals.
fn slot(phase: Phase) -> usize {
    match phase {
        Phase::Preparation => 0,
        Phase::Run => 1,
        Phase::ReadBack => 2,
        Phase::Closed => 3,
    }
}

impl LedgerState {
    /// The current phase when it accepts a write. A refused write counts as
    /// late: only a producer that outlived its phase writes after it.
    fn writable(&mut self) -> Result<Phase, LedgerRefusal> {
        let refusal = match self.phase {
            Phase::Preparation | Phase::Run => return Ok(self.phase),
            Phase::ReadBack => LedgerRefusal::ReadOnly {
                phase: Phase::ReadBack,
            },
            Phase::Closed => LedgerRefusal::Sealed {
                phase: Phase::Closed,
            },
        };
        self.late_refused = self.late_refused.saturating_add(1);
        Err(refusal)
    }

    /// What the room has left: its limits minus the committed and the reserved.
    fn left(&self) -> (u64, u64) {
        let bytes = self.limits.bytes.saturating_sub(self.committed_bytes);
        let files = self.limits.files.saturating_sub(self.committed_files);
        (
            bytes.saturating_sub(self.reserved_bytes),
            files.saturating_sub(self.reserved_files),
        )
    }

    fn release(&mut self, bytes: u64, files: u64) {
        self.reserved_bytes = self.reserved_bytes.saturating_sub(bytes);
        self.reserved_files = self.reserved_files.saturating_sub(files);
    }
}

impl EffectLedger {
    /// A fresh ledger in [`Phase::Preparation`], unsealed, nothing reserved.
    #[must_use]
    pub fn new(limits: RoomLimits) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(LedgerState {
                phase: Phase::Preparation,
                sealed: false,
                drained: false,
                limits,
                reserved_bytes: 0,
                reserved_files: 0,
                committed_bytes: 0,
                committed_files: 0,
                registered: Vec::new(),
                unjoined: 0,
                written: Vec::new(),
                late_refused: 0,
                leftovers: 0,
                phase_panicked: [0; 4],
            }),
        })
    }

    /// The one lock. It is held only for bookkeeping: never across work, a
    /// destructor of refused work, a spawn or a wait.
    fn state(&self) -> MutexGuard<'_, LedgerState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The current phase.
    #[must_use]
    pub fn phase(&self) -> Phase {
        self.state().phase
    }

    /// Run `work` on the blocking pool as a registered operation of the current
    /// phase. The seal check and the registration happen under one lock: the
    /// operation either registers before the seal, and every drain waits for
    /// it, or is refused without running. The receiver yields `work`'s result;
    /// dropping it never detaches the operation from the drain.
    ///
    /// # Errors
    /// [`LedgerRefusal::Sealed`] when the phase is sealed (a closed room always
    /// is): `work` is destroyed without running, after the lock is released,
    /// and the refusal is counted as late. [`LedgerRefusal::NoRuntime`] outside
    /// a tokio runtime.
    pub fn run_blocking<T, F>(&self, work: F) -> Result<oneshot::Receiver<T>, LedgerRefusal>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        self.register(None, work)
    }

    /// [`Self::run_blocking`] for a write when `writer` names the phase that
    /// authorized it: the registration is also refused, and counted as late,
    /// once that phase is no longer the current one.
    pub(crate) fn register<T, F>(
        &self,
        writer: Option<Phase>,
        work: F,
    ) -> Result<oneshot::Receiver<T>, LedgerRefusal>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let (done, finished) = oneshot::channel();
        let runtime = match self.admit(writer, finished) {
            Ok(runtime) => runtime,
            Err(refusal) => {
                // Decided and counted under the lock, destroyed after it: a
                // reservation `work` owns takes the lock again when it drops.
                drop(work);
                return Err(refusal);
            }
        };
        let (sender, receiver) = oneshot::channel();
        let completion = Completion(done);
        // Spawned after the lock is released: a runtime shutting down drops
        // `work` (and its reservation) inside this call.
        drop(runtime.spawn_blocking(move || {
            let _ = sender.send(work());
            completion.finish();
        }));
        Ok(receiver)
    }

    /// Under the lock: refuse and count a registration after the seal or
    /// outside its epoch, or keep its completion signal for the next seal.
    fn admit(
        &self,
        writer: Option<Phase>,
        finished: oneshot::Receiver<()>,
    ) -> Result<Handle, LedgerRefusal> {
        let mut state = self.state();
        let phase = state.phase;
        if state.sealed || writer.is_some_and(|authorized| authorized != phase) {
            state.late_refused = state.late_refused.saturating_add(1);
            return Err(LedgerRefusal::Sealed {
                phase: writer.unwrap_or(phase),
            });
        }
        let runtime = Handle::try_current().map_err(|_| LedgerRefusal::NoRuntime { phase })?;
        state.registered.push(finished);
        Ok(runtime)
    }

    /// Seal the current phase and take every registered operation, atomically.
    /// From this instant every registration is refused and counted as late.
    /// The returned [`Drain`] waits for what was taken.
    pub fn seal(self: &Arc<Self>) -> Drain {
        let mut state = self.state();
        state.sealed = true;
        let operations = std::mem::take(&mut state.registered);
        state.unjoined = state.unjoined.saturating_add(operations.len());
        let phase = state.phase;
        drop(state);
        Drain {
            ledger: Arc::clone(self),
            phase,
            operations,
        }
    }

    /// [`Self::seal`], then [`Drain::join`]: the one exit every outcome takes.
    pub async fn seal_and_drain(self: &Arc<Self>) -> Drained {
        self.seal().join().await
    }

    /// Move to the next phase. Allowed only after a completed drain of the
    /// current one; a phase never re-opens, and the closed room stays sealed.
    ///
    /// # Errors
    /// [`LedgerRefusal::Order`] before a completed drain, or once closed.
    pub fn advance(&self) -> Result<Phase, LedgerRefusal> {
        let mut state = self.state();
        let next = match state.phase {
            Phase::Preparation => Phase::Run,
            Phase::Run => Phase::ReadBack,
            Phase::ReadBack => Phase::Closed,
            Phase::Closed => {
                return Err(LedgerRefusal::Order {
                    phase: Phase::Closed,
                });
            }
        };
        if !state.drained {
            return Err(LedgerRefusal::Order { phase: state.phase });
        }
        state.phase = next;
        state.sealed = next == Phase::Closed;
        state.drained = false;
        Ok(next)
    }

    /// Reserve `bytes` and `files` of the room's aggregate budget, atomically,
    /// before a write starts. The reservation belongs to the current phase;
    /// what it does not commit returns when it drops. Returning a reservation
    /// restores the quota only: it never claims a failed attempt cost nothing.
    ///
    /// # Errors
    /// [`LedgerRefusal::Budget`] when the room has less left, nothing taken;
    /// [`LedgerRefusal::ReadOnly`] in read-back and [`LedgerRefusal::Sealed`]
    /// once closed, each counted as late.
    pub fn reserve(self: &Arc<Self>, bytes: u64, files: u64) -> Result<Reservation, LedgerRefusal> {
        let mut state = self.state();
        let phase = state.writable()?;
        let (bytes_left, files_left) = state.left();
        if bytes > bytes_left || files > files_left {
            return Err(LedgerRefusal::Budget {
                bytes_left,
                files_left,
            });
        }
        state.reserved_bytes = state.reserved_bytes.saturating_add(bytes);
        state.reserved_files = state.reserved_files.saturating_add(files);
        Ok(Reservation {
            ledger: Arc::clone(self),
            bytes,
            files,
            phase,
        })
    }

    /// The current phase when it accepts a write, for a write that takes no
    /// budget before it starts (a directory chain or a removal).
    pub(crate) fn writable(&self) -> Result<Phase, LedgerRefusal> {
        self.state().writable()
    }

    /// Record `path` as write evidence when the name was claimed in the run
    /// phase. Called inside the registered operation, so a drain covers it.
    pub(crate) fn published(&self, path: &Path) {
        let mut state = self.state();
        if state.phase == Phase::Run {
            state.written.push(path.to_path_buf());
        }
    }

    /// Settle one publication once both of its steps are known: `claimed` is
    /// the claim of the name (a rename or a link) and `cleaned` whether no
    /// temporary name is left. A claimed name is evidence (in the run phase)
    /// and its usage stays committed whatever came after. An unclaimed write
    /// returns its budget only when cleaned. A temporary name left behind is
    /// committed usage and counted, so the room never reads as clean.
    ///
    /// # Errors
    /// The claim's own error, unchanged.
    pub(crate) fn settle(
        &self,
        reservation: Reservation,
        path: &Path,
        used: u64,
        claimed: Result<(), FsError>,
        cleaned: bool,
    ) -> Result<(), FsError> {
        if !cleaned {
            let mut state = self.state();
            state.leftovers = state.leftovers.saturating_add(1);
        }
        match claimed {
            Ok(()) => {
                self.published(path);
                reservation.commit(used, 1);
                Ok(())
            }
            Err(error) => {
                if !cleaned {
                    reservation.commit(used, 1);
                }
                Err(error)
            }
        }
    }

    /// The room-relative paths the run published, in publication order. A
    /// snapshot copied under the lock, which is released before this returns:
    /// iterating never holds the ledger.
    pub fn written(&self) -> impl Iterator<Item = PathBuf> + use<> {
        self.state().written.clone().into_iter()
    }

    /// Operations refused because they arrived after their phase: after a
    /// seal, outside the epoch that authorized them, or a write after the run.
    #[must_use]
    pub fn late_refusals(&self) -> u64 {
        self.state().late_refused
    }

    /// Temporary names a failed cleanup left in the room. Any one means the
    /// room's result cannot be reported clean.
    #[must_use]
    pub fn leftovers(&self) -> u64 {
        self.state().leftovers
    }

    /// Operations that panicked or never ran, over the room's whole life and
    /// every drain of every phase. A clean result needs zero here, whatever
    /// any single [`Drained`] reported.
    #[must_use]
    pub fn panicked(&self) -> u64 {
        let total = self
            .state()
            .phase_panicked
            .iter()
            .fold(0_usize, |sum, phase| sum.saturating_add(*phase));
        u64::try_from(total).unwrap_or(u64::MAX)
    }

    /// The room's byte bound, which also caps every read.
    pub(crate) fn byte_bound(&self) -> u64 {
        self.state().limits.bytes
    }

    /// A drain of `phase` waited for `operations`, `panicked` of which did not
    /// finish cleanly. Its panics are latched into the phase's total, which
    /// this answers. Only a drain of the current phase can drain it, once no
    /// seal's operations remain: a drain of an earlier phase never does.
    fn joined(&self, phase: Phase, operations: usize, panicked: usize) -> usize {
        let mut state = self.state();
        state.unjoined = state.unjoined.saturating_sub(operations);
        let total = &mut state.phase_panicked[slot(phase)];
        *total = total.saturating_add(panicked);
        let latched = *total;
        if phase == state.phase && state.sealed && state.unjoined == 0 {
            state.drained = true;
        }
        latched
    }
}

/// The completion signal of one registered operation: sent once its work
/// returned, dropped unsent when the work panicked or never ran.
struct Completion(oneshot::Sender<()>);

impl Completion {
    fn finish(self) {
        let _ = self.0.send(());
    }
}

/// Budget held for one write, in the phase that authorized it. Dropping it
/// returns what was not committed.
#[derive(Debug)]
pub struct Reservation {
    ledger: Arc<EffectLedger>,
    bytes: u64,
    files: u64,
    phase: Phase,
}

impl Reservation {
    /// Keep `bytes` and `files`, at most what was reserved, as the room's
    /// usage; the rest returns at once.
    pub fn commit(mut self, bytes: u64, files: u64) {
        let mut state = self.ledger.state();
        state.release(self.bytes, self.files);
        state.committed_bytes = state.committed_bytes.saturating_add(bytes.min(self.bytes));
        state.committed_files = state.committed_files.saturating_add(files.min(self.files));
        drop(state);
        self.bytes = 0;
        self.files = 0;
    }

    /// The phase (the epoch) that authorized this write.
    pub(crate) fn phase(&self) -> Phase {
        self.phase
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if self.bytes != 0 || self.files != 0 {
            self.ledger.state().release(self.bytes, self.files);
        }
    }
}

/// The operations one seal took, still to be waited for.
#[derive(Debug)]
#[must_use = "a seal without its join proves nothing stopped"]
pub struct Drain {
    ledger: Arc<EffectLedger>,
    phase: Phase,
    operations: Vec<oneshot::Receiver<()>>,
}

impl Drain {
    /// Wait for every operation the seal took, whatever its outcome, then mark
    /// the phase drained once no seal's operations remain (only a drain of the
    /// current phase can). A join dropped before it completes never marks the
    /// phase drained: the phase then stays sealed for good and cannot advance.
    pub async fn join(self) -> Drained {
        let Self {
            ledger,
            phase,
            operations,
        } = self;
        let joined = operations.len();
        let mut panicked = 0;
        for finished in operations {
            if finished.await.is_err() {
                panicked += 1;
            }
        }
        let panicked = ledger.joined(phase, joined, panicked);
        Drained {
            phase,
            joined,
            panicked,
        }
    }
}

/// What one drain waited for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Drained {
    /// The phase that was drained.
    pub phase: Phase,
    /// Operations this drain waited for.
    pub joined: usize,
    /// Operations of this phase, across all of its drains so far, that
    /// panicked or never ran (a runtime shutting down): a latch, so a later
    /// drain never reports the phase cleaner than an earlier one did. The
    /// room's total is [`EffectLedger::panicked`].
    pub panicked: usize,
}

impl Drained {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(phase: Phase, joined: usize, panicked: usize) -> Self {
        Self {
            phase,
            joined,
            panicked,
        }
    }
}
