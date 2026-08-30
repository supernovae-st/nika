// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI edge for the shared [`nika_arm`] firing transaction.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::MutexGuard;

use jiff::{Timestamp, Zoned};
use nika_vocab::project;

pub use nika_arm::fire::{
    CoordinatedRunSeam, ExecutionRunSeam, FireCtx, FireCtxError, FireVerdict, PreparedRun, RunSeam,
    RunShot, RunUpshot, Wait, WaitSeam, fire_beat, labels,
};

use super::args::FireArgs;
use crate::verbs::{self, VerbOutput, exit};

/// `nika arm fire <label>` — discover, validate, inject the clock and adapt
/// the shared firing transaction to the in-process CLI executor.
#[must_use]
pub fn run(fire: &FireArgs) -> VerbOutput {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let found = match project::discover(&cwd) {
        Ok(found) => found,
        Err(error) => return VerbOutput::file(format!("PROJECT ✗  {error}")),
    };
    let Some((path, _project)) = found else {
        return VerbOutput::file(
            "nothing armed — this project has no `nika.yaml`\n  \
             fix: `nika init --project-file` lays a commented starter"
                .to_owned(),
        );
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            return VerbOutput::env(format!("cannot read {}: {error}", path.display()));
        }
    };
    let registry = match nika_cadence::parse_registry(&text) {
        Ok(registry) => registry,
        Err(error) => return VerbOutput::file(format!("ARM ✗  {error}")),
    };
    let faults: Vec<String> = nika_cadence::validate(&registry)
        .map(|error| format!("  {error}"))
        .collect();
    if !faults.is_empty() {
        return VerbOutput::file(format!(
            "ARM ✗  {} in {}\n{}",
            crate::text::count(faults.len(), "refusal"),
            path.display(),
            faults.join("\n")
        ));
    }
    let labels = labels(&registry);
    let Some(index) = labels.iter().position(|label| label == &fire.label) else {
        return VerbOutput::file(format!(
            "arm fire: unknown beat `{}` — this project arms: {}",
            fire.label,
            labels.join(" · ")
        ));
    };
    let now = match parse_now(fire.now.as_deref()) {
        Ok(now) => now,
        Err(line) => return VerbOutput::file(line),
    };
    let root = path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let ctx = match FireCtx::new_with_execution(
        root.clone(),
        registry,
        index,
        now,
        std::process::id(),
        Rc::new(prod_run),
    ) {
        Ok(ctx) => ctx,
        Err(error) => return VerbOutput::file(error.to_string()),
    };
    let verdict = fire_beat(&ctx);
    let (text, code) = verdict.into_parts();
    VerbOutput { text, code }
}

fn parse_now(raw: Option<&str>) -> Result<Zoned, String> {
    match raw {
        None => Ok(Zoned::now()),
        Some(text) => text
            .parse::<Zoned>()
            .or_else(|_| {
                text.parse::<Timestamp>()
                    .map(|timestamp| timestamp.to_zoned(jiff::tz::TimeZone::UTC))
            })
            .map_err(|_| {
                format!("arm fire: --now `{text}` · RFC 3339 attendu — 2026-08-19T03:02:00Z")
            }),
    }
}

pub(crate) fn prod_run(
    execution: nika_execution::ExecutionContext<'_>,
    shot: &RunShot,
) -> RunUpshot {
    debug_assert!(!shot.workflow().is_empty());
    debug_assert_eq!(shot.generation().as_str().len(), 64);
    let Ok(_room) = enter_room(shot.project(), shot.root()) else {
        return RunUpshot::new(exit::ENV, None);
    };
    let Ok(receipt) = run_quietly(|| {
        verbs::run::run_arm_context(
            execution,
            shot.workflow(),
            shot.root().to_path_buf(),
            shot.ceiling(),
        )
    }) else {
        return RunUpshot::new(exit::ENV, None);
    };
    RunUpshot::new(
        receipt.code,
        receipt
            .trace
            .map(|path| path.to_string_lossy().into_owned()),
    )
}

struct RoomGuard {
    previous: PathBuf,
    _lease: MutexGuard<'static, ()>,
}

impl Drop for RoomGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous);
    }
}

/// This used to take a module-private `RUN_ROOM` mutex. It was one of THREE
/// private guards over one process-global resource (#1192), so it excluded
/// only other `arm fire` calls — never a `run --example` (which took nothing)
/// nor a budget test (which took its own). The lease is now the crate's, and
/// the chdir still rides `fchdir` on unix: an already-open directory fd cannot
/// be swapped by a rename between the check and the move.
fn enter_room(project: &nika_fs::OwnedDir, _root: &Path) -> std::io::Result<RoomGuard> {
    let lease = crate::cwd::hold();
    let previous = std::env::current_dir()?;
    #[cfg(unix)]
    nix::unistd::fchdir(project.as_file())?;
    #[cfg(not(unix))]
    std::env::set_current_dir(_root)?;
    Ok(RoomGuard {
        previous,
        _lease: lease,
    })
}

#[cfg(unix)]
struct StdoutGuard {
    saved: std::os::fd::OwnedFd,
}

#[cfg(unix)]
impl StdoutGuard {
    fn enter() -> std::io::Result<Self> {
        use std::io::Write as _;
        std::io::stdout().flush()?;
        let saved = nix::unistd::dup(std::io::stdout())?;
        nix::unistd::dup2_stdout(std::io::stderr())?;
        Ok(Self { saved })
    }
}

#[cfg(unix)]
impl Drop for StdoutGuard {
    fn drop(&mut self) {
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
        let _ = nix::unistd::dup2_stdout(&self.saved);
    }
}

#[cfg(unix)]
fn run_quietly<T>(f: impl FnOnce() -> T) -> std::io::Result<T> {
    let _guard = StdoutGuard::enter()?;
    Ok(f())
}

#[cfg(not(unix))]
fn run_quietly<T>(f: impl FnOnce() -> T) -> std::io::Result<T> {
    Ok(f())
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::{enter_room, parse_now};

    #[test]
    fn the_injected_clock_parses_rfc3339_and_refuses_garbage() {
        let now = parse_now(Some("2026-08-19T03:02:00Z")).expect("parses");
        assert_eq!(now.timestamp().to_string(), "2026-08-19T03:02:00Z");
        let zoned = parse_now(Some("2026-08-19T05:02:00+02:00[Europe/Paris]")).expect("parses");
        assert_eq!(zoned.timestamp().to_string(), "2026-08-19T03:02:00Z");
        assert!(parse_now(Some("demain")).is_err());
    }

    #[test]
    #[allow(clippy::disallowed_methods)] // process-global cwd needs real OS threads
    fn concurrent_run_rooms_are_serialized_and_restore_the_caller() {
        // Both of this test's bare cwd READS take the lease (#1192). They did
        // not, and that is the same defect one level up: the cwd is
        // process-global, so ASSERTING on it while another test legitimately
        // holds it reads whatever that test is doing. The lease's own
        // chdir-storm test in `crate::cwd` found this within a day of landing
        // — an unguarded read is exactly what it exists to make impossible.
        let caller = {
            let _lease = crate::cwd::hold();
            std::env::current_dir().expect("caller cwd")
        };
        let first = tempfile::tempdir().expect("first room");
        let second = tempfile::tempdir().expect("second room");
        let first_path = first.path().to_path_buf();
        let second_path = second.path().to_path_buf();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let first_thread = std::thread::spawn(move || {
            let project = nika_fs::OwnedDir::open(&first_path).expect("first project");
            let _room = enter_room(&project, &first_path).expect("first enters");
            entered_tx.send(()).expect("entered");
            release_rx.recv().expect("release");
        });
        entered_rx.recv().expect("first entered");

        let (second_tx, second_rx) = mpsc::channel();
        let second_thread = std::thread::spawn(move || {
            let project = nika_fs::OwnedDir::open(&second_path).expect("second project");
            let _room = enter_room(&project, &second_path).expect("second enters");
            second_tx
                .send(std::env::current_dir().expect("second cwd"))
                .expect("second result");
        });
        assert!(
            second_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "the second run cannot mutate cwd while the first owns it"
        );
        release_tx.send(()).expect("release first");
        first_thread.join().expect("first joins");
        assert_eq!(
            second_rx.recv().expect("second entered"),
            std::fs::canonicalize(second.path()).expect("canonical second")
        );
        second_thread.join().expect("second joins");
        // Taken briefly, not across the whole test: the threads above acquire
        // the same lease through `enter_room`, so holding it here for the
        // duration would deadlock them.
        let restored = {
            let _lease = crate::cwd::hold();
            std::env::current_dir().expect("restored cwd")
        };
        assert_eq!(restored, caller);
    }
}
