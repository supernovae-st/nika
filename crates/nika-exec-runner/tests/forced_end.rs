// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(any(target_os = "linux", target_os = "macos"))]

//! The process-wide forced end through the production `TokioShell`. It ends every group
//! this process owns and refuses every later spawn, so it lives alone in this test binary: one
//! test, no other spawn sharing its process.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nika_exec_runner::{TokioShell, terminate_owned_groups};
use nika_kernel::process::ShellRunDyn as _;
use nika_kernel::{ShellCommand, ShellError};
use rustix::io::Errno;
use rustix::process::{Pid, test_kill_process};

/// A leaf that names its pid, then waits long past the test before its second marker.
const LEAF: &str = "echo $$ > started\nsleep 30\necho late > late\n";

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn started(dir: &Path) -> Pid {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let written = std::fs::read_to_string(dir.join("started")).unwrap_or_default();
        if let Some(pid) = written.trim().parse().ok().and_then(Pid::from_raw) {
            return pid;
        }
        assert!(Instant::now() < deadline, "the leaf never started");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn a_forced_end_ends_the_exec_still_running_and_starts_nothing_after() {
    let dir = Dir(std::env::temp_dir().join(format!("nika-forced-end-{}", std::process::id())));
    std::fs::create_dir_all(&dir.0).expect("leaf dir");
    std::fs::write(dir.0.join("leaf.sh"), LEAF).expect("leaf script");
    let mut command = ShellCommand::new("/bin/sh").arg("leaf.sh");
    command.cwd = Some(dir.0.clone());
    let shell = TokioShell::new();
    let settled = Arc::new(AtomicBool::new(false));
    let marker = Arc::clone(&settled);
    let run = tokio::spawn(async move {
        let outcome = shell.run(command).await;
        marker.store(true, Ordering::SeqCst);
        outcome
    });
    let leaf = started(&dir.0).await;

    // Blocking by design: off the executor's thread, as the CLI's signal listener calls it.
    let (ended, ran_on) = tokio::task::spawn_blocking(move || {
        let mut report = None;
        terminate_owned_groups(Duration::from_secs(2), |ended| {
            // The run cannot go on past its group's end before `then` returns: the CLI exits here.
            std::thread::sleep(Duration::from_millis(300));
            report = Some((ended.clone(), settled.load(Ordering::SeqCst)));
        });
        report.expect("then is handed the report")
    })
    .await
    .expect("the forced end returns");
    assert!(!ran_on, "the run went on before then returned");
    assert_eq!((ended.ended(), ended.spared()), (1, 0), "{ended}");
    assert!(ended.complete(), "{ended}");
    assert!(
        matches!(test_kill_process(leaf), Err(Errno::SRCH)),
        "the leaf is gone, reaped by the forced end"
    );
    let outcome = run.await.expect("the run task ends");
    assert!(
        outcome.is_err(),
        "its run never reports success: {outcome:?}"
    );
    assert!(
        !dir.0.join("late").exists(),
        "the ended leaf never wrote its second marker"
    );

    // Nothing starts once a forced end began.
    let after = TokioShell::new()
        .run(ShellCommand::new("/bin/echo").arg("never"))
        .await;
    assert!(
        matches!(&after, Err(ShellError::Other { reason }) if reason.contains("ending")),
        "{after:?}"
    );
}
