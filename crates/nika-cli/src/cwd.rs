// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The process's ONE current-directory lease (#1192).
//!
//! `std::env::set_current_dir` is **process-global**. `cargo test --lib` runs
//! a crate's tests as parallel threads in ONE process, so a test that chdirs
//! and a test that reads the cwd are racing whether or not either of them
//! knows it.
//!
//! This crate used to have THREE independent guards and one site with none:
//!
//! | site | guard |
//! |---|---|
//! | `verbs::check::budget` tests | a module-private `CWD_LOCK` |
//! | `verbs::arm::fire::enter_room` | a module-private `RUN_ROOM` |
//! | `verbs::run::example` | **nothing** |
//!
//! Three private mutexes for one global resource do not compose: a budget
//! test could hold its own lock for its whole body and still have the ground
//! moved under it by `arm fire` (which took a DIFFERENT mutex) or by
//! `run --example` (which took none). The `CWD_LOCK` doc comment was accurate
//! and insufficient in the same breath — *"one lock for every test **in this
//! module**"* — because the hazard is process-wide and the lock was not.
//!
//! Measured 2026-08-24 across four CI runs of effectively identical trees:
//! `access-harness tests` failed one run in four, always the same three
//! `verbs::check::budget::tests` — the ones whose assertions derive from the
//! current directory — with the backtrace on `obj.get("run_budget")`, i.e.
//! the ancestor walk found no `nika.yaml` because the cwd was no longer the
//! temp dir the test had just entered. Two of those four runs carried
//! byte-identical source.
//!
//! An intermittent red teaches everyone to press the button again, and that
//! habit is how a REAL red gets waved through. So there is one lease, and
//! every site that moves the process takes it.
//!
//! ## Take it ONCE — the lease is not reentrant
//!
//! [`hold`] and [`enter`] both lock the same `std::sync::Mutex`, which is not
//! reentrant: a caller that already holds the lease and calls either again
//! **deadlocks itself**. This is not hypothetical — one budget test enters two
//! directories in a row, and expressing that as two [`enter`] calls would hang
//! it forever.
//!
//! So the contract is: take the lease ONCE for the whole span in which the cwd
//! must be yours, and move as many times as you like inside it. [`enter`] for
//! a single move that should be undone on the way out; [`hold`] when you will
//! do the moving yourself (several hops, or `fchdir`).
//!
//! ## Poisoning
//!
//! The mutex guards no data — only exclusion — and every holder restores the
//! previous directory from `Drop`, which runs during a panic unwind. So a
//! poisoned lease means "some test panicked", never "the cwd is inconsistent",
//! and recovering from it is correct. Refusing instead would let one panicking
//! test wedge every later chdir in the process.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// The one lease. Private on purpose: callers take it through [`hold`] or
/// [`enter`], never by naming a second mutex.
static CWD: Mutex<()> = Mutex::new(());

/// Take the lease WITHOUT moving the process.
///
/// For a caller that changes directory by some route this module cannot
/// perform for it — `fchdir(2)` on an already-open directory fd, which is
/// what `arm fire` uses because it is immune to a rename between the check
/// and the move. Such a caller still owes everyone else exclusion.
pub(crate) fn hold() -> MutexGuard<'static, ()> {
    // See §Poisoning: exclusion has no invariant a panic can break.
    CWD.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The lease plus the promise to put the process back where it was found.
pub(crate) struct Lease {
    previous: Option<PathBuf>,
    _guard: MutexGuard<'static, ()>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            // Best-effort: the original directory can have been removed under
            // us (a temp room its owner already cleaned). Failing to return is
            // not worth panicking a Drop over — the lease is released either
            // way, which is what the next holder needs.
            let _ = std::env::set_current_dir(previous);
        }
    }
}

/// Take the lease and move the process into `dir` until the [`Lease`] drops.
///
/// # Errors
///
/// Whatever `set_current_dir` raises for `dir`. The lease is dropped on the
/// error path, so a failed entry never strands it.
pub(crate) fn enter(dir: &Path) -> std::io::Result<Lease> {
    let guard = hold();
    // Read the previous dir INSIDE the lease: outside it, another thread's
    // chdir could land between the read and the move, and we would faithfully
    // restore a directory that was never ours.
    let previous = std::env::current_dir().ok();
    std::env::set_current_dir(dir)?;
    Ok(Lease {
        previous,
        _guard: guard,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the three private mutexes could not give: a reader that
    /// holds the lease sees a STABLE cwd even while another thread is doing
    /// nothing but chdir, as fast as it can.
    ///
    /// This is the deliberate race #1192 asks for, run in-process the way
    /// `cargo test --lib` runs everything. Against the old shape — a private
    /// lock on one side and a bare `set_current_dir` on the other — the
    /// reader's assertion fails; the churner here stands in for
    /// `run --example`, which took no guard at all.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "the cwd is a PROCESS global, so only a real OS thread can race it — \
                  a tokio task on the same worker proves nothing about this hazard"
    )]
    fn a_reader_holding_the_lease_sees_a_stable_cwd_under_a_chdir_storm() {
        let room = std::env::temp_dir().join(format!("nika-cwd-lease-{}", std::process::id()));
        let elsewhere = std::env::temp_dir().join(format!("nika-cwd-storm-{}", std::process::id()));
        std::fs::create_dir_all(&room).expect("mkdir room");
        std::fs::create_dir_all(&elsewhere).expect("mkdir elsewhere");

        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let churn_stop = std::sync::Arc::clone(&stop);
        let churn_target = elsewhere.clone();
        let churner = std::thread::spawn(move || {
            while !churn_stop.load(std::sync::atomic::Ordering::Relaxed) {
                // Takes the lease, exactly as every chdir site now must.
                if let Ok(lease) = enter(&churn_target) {
                    drop(lease);
                }
            }
        });

        // canonicalize: macOS hands back /private/var for /var, and the
        // comparison must be about the DIRECTORY, not its spelling.
        let want = room.canonicalize().expect("canonicalize room");
        for _ in 0..200 {
            let lease = enter(&room).expect("enter room");
            let seen = std::env::current_dir()
                .expect("cwd")
                .canonicalize()
                .expect("canonicalize cwd");
            assert_eq!(
                seen, want,
                "the cwd moved under a holder of the lease — the lease is not exclusive"
            );
            drop(lease);
        }

        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        churner.join().expect("churner");
        let _ = std::fs::remove_dir_all(&room);
        let _ = std::fs::remove_dir_all(&elsewhere);
    }

    /// The lease returns the process to where it found it, so a test that
    /// borrows the cwd cannot leave it borrowed for the next one.
    #[test]
    fn the_lease_restores_the_previous_directory() {
        let room = std::env::temp_dir().join(format!("nika-cwd-restore-{}", std::process::id()));
        std::fs::create_dir_all(&room).expect("mkdir");
        let before = {
            let _hold = hold();
            std::env::current_dir().expect("cwd")
        };
        {
            let _lease = enter(&room).expect("enter");
        }
        let after = {
            let _hold = hold();
            std::env::current_dir().expect("cwd")
        };
        assert_eq!(before, after, "the lease did not put the process back");
        let _ = std::fs::remove_dir_all(&room);
    }
}
