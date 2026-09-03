// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resident firer's door into the ONE ledger (ADR-116: ARM's verified
//! ledger is the firing truth · wave 6 · one automation authority).
//!
//! Two firers exist for a project beat — the resident (`nika serve`) and the
//! CLI edge (`nika arm fire`). Each kept its own ledger and the same slot
//! fired twice (a paid call, twice). The resident now claims a slot through
//! THIS door: under the same per-beat lock the edge takes, into the same
//! history the edge writes and `nika arm` reads. A slot the ledger already
//! answers is refused before any claim; a lock a live process holds is
//! refused; the receipt lands in the same history when the run settles.

use std::fmt;
use std::io;
use std::path::Path;

use jiff::{SignedDuration, Timestamp, Zoned};
use nika_cadence::ledger::{Claim, Receipt};
use nika_cadence::{ArmGeneration, FencingToken, SlotId};

use crate::state::{ArmState, LockLease, LockOutcome};

/// The deadline a resident claim carries when the cadence's next slot is
/// not consulted: the edge's own fallback.
const CLAIM_DEADLINE_FALLBACK: SignedDuration = SignedDuration::from_hours(24);

/// Why the resident may not claim this slot.
#[derive(Debug)]
#[non_exhaustive]
pub enum ResidentClaimRefusal {
    /// The beat lock is held by a live process — the CLI edge is firing it.
    LockHeld {
        /// The holder.
        pid: u32,
    },
    /// The ledger already answers this slot (the other domain fired it).
    SlotAnswered {
        /// The last answered slot.
        at: Timestamp,
    },
    /// The ledger refused.
    Ledger(io::Error),
}

impl fmt::Display for ResidentClaimRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LockHeld { pid } => {
                write!(f, "the beat lock is held by a live process (pid {pid})")
            }
            Self::SlotAnswered { at } => {
                write!(f, "the ledger already answers this slot (last fired {at})")
            }
            Self::Ledger(error) => write!(f, "the ledger refused: {error}"),
        }
    }
}

impl std::error::Error for ResidentClaimRefusal {}

/// A slot the resident holds: the beat lock stays held and the claim is in
/// the history until [`ResidentClaim::settle`] lands the receipt.
pub struct ResidentClaim {
    lease: LockLease,
    claim: Claim,
    fencing: u64,
    slot: Timestamp,
}

impl fmt::Debug for ResidentClaim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResidentClaim")
            .field("slot_id", &self.claim.slot_id)
            .field("fencing", &self.fencing)
            .field("slot", &self.slot)
            .finish_non_exhaustive()
    }
}

/// Claim `slot` of the beat `label` for the resident: refused when the
/// ledger already answers it or a live process holds the beat lock; the
/// claim is recorded in the ONE history under the lock.
///
/// # Errors
///
/// [`ResidentClaimRefusal`] — a held lock, an answered slot, or the ledger.
pub fn claim_for_resident(
    project_root: &Path,
    label: &str,
    slot: &Zoned,
    slot_id: SlotId,
    generation: Option<ArmGeneration>,
    pid: u32,
    now: &Zoned,
) -> Result<ResidentClaim, ResidentClaimRefusal> {
    let state = ArmState::open(project_root).map_err(ResidentClaimRefusal::Ledger)?;
    refuse_answered(&state, label, slot)?;
    let attempt = state
        .acquire_beat_lock(label, pid, now)
        .map_err(ResidentClaimRefusal::Ledger)?;
    let lease = match attempt.outcome {
        LockOutcome::HeldAlive { pid } => return Err(ResidentClaimRefusal::LockHeld { pid }),
        LockOutcome::Acquired => attempt.lease.ok_or_else(|| {
            ResidentClaimRefusal::Ledger(io::Error::other("the beat lock returned no lease"))
        })?,
    };
    // Under the lock: the edge may have answered between the peek and the lock.
    refuse_answered(&state, label, slot)?;
    let deadline = now
        .timestamp()
        .checked_add(CLAIM_DEADLINE_FALLBACK)
        .unwrap_or_else(|_| now.timestamp());
    let mut claim = Claim::new(slot_id, deadline, now.timestamp());
    claim.generation = generation;
    let recorded =
        ArmState::record_claim_with_lease(&lease, &claim).map_err(ResidentClaimRefusal::Ledger)?;
    Ok(ResidentClaim {
        lease,
        claim,
        fencing: recorded.seq,
        slot: slot.timestamp(),
    })
}

/// Whether the ledger already answers `slot` for `label` — the check both
/// firers run before a claim.
///
/// # Errors
///
/// The ledger's own refusal.
pub fn slot_answered(project_root: &Path, label: &str, slot: &Zoned) -> io::Result<bool> {
    let state = ArmState::open(project_root)?;
    Ok(state
        .last_fired(label)?
        .is_some_and(|last| last.timestamp() >= slot.timestamp()))
}

fn refuse_answered(
    state: &ArmState,
    label: &str,
    slot: &Zoned,
) -> Result<(), ResidentClaimRefusal> {
    match state
        .last_fired(label)
        .map_err(ResidentClaimRefusal::Ledger)?
    {
        Some(last) if last.timestamp() >= slot.timestamp() => {
            Err(ResidentClaimRefusal::SlotAnswered {
                at: last.timestamp(),
            })
        }
        _ => Ok(()),
    }
}

impl ResidentClaim {
    /// The slot this claim holds.
    #[must_use]
    pub const fn slot(&self) -> Timestamp {
        self.slot
    }

    /// Land the receipt in the ONE history and release the beat lock.
    ///
    /// # Errors
    ///
    /// The ledger's own refusal; the lock is released either way.
    pub fn settle(self, exit: u8, trace: Option<String>, decided_at: Timestamp) -> io::Result<()> {
        let receipt = Receipt::for_claim(
            &self.claim,
            FencingToken::new(self.fencing),
            self.slot,
            decided_at,
            trace,
            exit,
            None,
        );
        ArmState::record_receipt_with_lease(&self.lease, &receipt).map(|_| ())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn zoned(raw: &str) -> Zoned {
        raw.parse().expect("zoned")
    }

    /// The resident's claim takes the beat lock; a second claim of the same
    /// slot is refused while the lock is held, and answered once the receipt
    /// lands; a later slot claims again.
    #[test]
    fn one_slot_one_firer_through_the_one_ledger() {
        let dir = tempfile::tempdir().expect("project");
        let slot = zoned("2026-09-01T09:00:00Z[UTC]");
        let now = zoned("2026-09-01T09:00:02Z[UTC]");
        let id = SlotId::derive("root.nika.yaml", "TZ=UTC 0 9 * * *", &slot);
        let held = claim_for_resident(
            dir.path(),
            "root",
            &slot,
            id.clone(),
            None,
            std::process::id(),
            &now,
        )
        .expect("the first claim");
        assert!(
            matches!(
                claim_for_resident(dir.path(), "root", &slot, id.clone(), None, 424_242, &now),
                Err(ResidentClaimRefusal::LockHeld { .. })
            ),
            "the lock is held while the resident runs"
        );
        assert!(
            !slot_answered(dir.path(), "root", &slot).expect("ledger"),
            "claimed, not yet answered"
        );
        held.settle(0, Some("t.ndjson".to_owned()), now.timestamp())
            .expect("the receipt");
        assert!(
            slot_answered(dir.path(), "root", &slot).expect("ledger"),
            "answered once the receipt lands"
        );
        assert!(
            matches!(
                claim_for_resident(
                    dir.path(),
                    "root",
                    &slot,
                    id,
                    None,
                    std::process::id(),
                    &now
                ),
                Err(ResidentClaimRefusal::SlotAnswered { .. })
            ),
            "the same slot is never fired twice"
        );
        let later = zoned("2026-09-02T09:00:00Z[UTC]");
        let later_id = SlotId::derive("root.nika.yaml", "TZ=UTC 0 9 * * *", &later);
        let next = claim_for_resident(
            dir.path(),
            "root",
            &later,
            later_id,
            None,
            std::process::id(),
            &zoned("2026-09-02T09:00:01Z[UTC]"),
        )
        .expect("the next slot claims");
        assert_eq!(next.slot(), later.timestamp());
        let last = ArmState::open(dir.path())
            .expect("state")
            .last_fired("root")
            .expect("ledger")
            .expect("fired");
        assert_eq!(
            last.timestamp(),
            slot.timestamp(),
            "`nika arm` reads the resident's fire as proven"
        );
    }
}
