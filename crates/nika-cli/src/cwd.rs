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

    /// The property the three private mutexes could not give: while one holder
    /// owns the lease, a second cannot move the process — it BLOCKS, and the
    /// first keeps the directory it entered.
    ///
    /// This is the deliberate race #1192 asks for, in the shape `arm fire`
    /// already uses for the same question
    /// (`concurrent_run_rooms_are_serialized_and_restore_the_caller`): let the
    /// contender attempt the move and assert it does not complete.
    ///
    /// It was first written as a chdir STORM — a thread doing nothing but
    /// enter/leave for the length of the test — and that was wrong, in a way
    /// worth leaving written down. The storm proved the same property, but it
    /// held the process cwd somewhere unexpected for seconds at a time, so
    /// EVERY concurrent test whose behaviour depends on the working directory
    /// became flaky. It broke two within a day, one of them only indirectly
    /// (a workspace walk that starts at the cwd). A test for a process-global
    /// must borrow that global for as little time as it can: the hazard it
    /// studies is the hazard it inflicts.
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "the cwd is a PROCESS global, so only a real OS thread can race it — \
                  a tokio task on the same worker proves nothing about this hazard"
    )]
    fn a_second_holder_cannot_move_the_process_while_the_first_owns_the_lease() {
        use std::sync::mpsc;
        use std::time::Duration;

        let room = tempfile::tempdir().expect("room");
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        let contender_target = elsewhere.path().to_path_buf();
        // canonicalize: macOS hands back /private/var for /var, and the
        // comparison is about the DIRECTORY, not its spelling.
        let want = room.path().canonicalize().expect("canonical room");

        let held = enter(room.path()).expect("first enters");

        let (tx, rx) = mpsc::channel();
        let contender = std::thread::spawn(move || {
            let lease = enter(&contender_target).expect("second enters");
            tx.send(()).expect("second signalled");
            drop(lease);
        });

        assert!(
            rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "a second holder moved the process while the first owned the lease"
        );
        let seen = std::env::current_dir()
            .expect("cwd")
            .canonicalize()
            .expect("canonical cwd");
        assert_eq!(seen, want, "the cwd moved under the holder of the lease");

        drop(held);
        rx.recv()
            .expect("the contender proceeds once the lease is free");
        contender.join().expect("contender joins");
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
