// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Real subprocess regressions: fixtures terminate themselves even on failure.

use crate::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

struct Fixture(PathBuf);

impl Fixture {
    fn new(parent_exits: bool) -> Result<Self, String> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nika-process-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let ending = if parent_exits { "exit 0" } else { "wait" };
        std::fs::write(
            root.join("worker.sh"),
            format!(
                "(sleep 1; printf late > late) &\nprintf '%s' \"$$\" > parent\nprintf ready > ready\n{ending}\n"
            ),
        )
        .map_err(|error| error.to_string())?;
        Ok(Self(root))
    }

    fn command(&self) -> ShellCommand {
        let mut command = ShellCommand::new("/bin/sh").arg("worker.sh");
        command.cwd = Some(self.0.clone());
        command
    }

    async fn ready(&self) -> Result<(), String> {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !self.0.join("ready").is_file() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .map_err(|error| format!("fixture must start before cancellation: {error}"))
    }

    async fn assert_no_late_effect(&self) {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(
            !self.0.join("late").exists(),
            "descendant wrote after cancellation"
        );
    }

    async fn leader_exited(&self) -> Result<(), String> {
        use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
        let id: i32 = std::fs::read_to_string(self.0.join("parent"))
            .map_err(|error| error.to_string())?
            .parse::<i32>()
            .map_err(|error| error.to_string())?;
        let pid = Pid::from_raw(id).ok_or("fixture must record a usable PID")?;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let status = waitid(
                    WaitId::Pid(pid),
                    WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
                )
                .map_err(|error| format!("executor must retain its waitable leader: {error}"))?;
                if status.is_some() {
                    return Ok::<(), String>(());
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .map_err(|error| format!("leader must exit before cancellation: {error}"))?
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn drop_after_ready(parent_exits: bool) -> Result<(), String> {
    let fixture = Fixture::new(parent_exits)?;
    let shell = TokioShell::new();
    let mut run = Box::pin(shell.run(fixture.command()));
    tokio::select! {
        ready = fixture.ready() => { ready?; },
        result = &mut run => panic!("run settled before the fixture was ready: {result:?}"),
    }
    if parent_exits {
        tokio::select! {
            exited = fixture.leader_exited() => { exited?; },
            result = &mut run => panic!("collection finished before descendant: {result:?}"),
        }
        std::future::poll_fn(|cx| {
            assert!(run.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        fixture.leader_exited().await?;
    }
    drop(run);
    fixture.assert_no_late_effect().await;
    assert!(
        shell
            .registry
            .lock()
            .map_err(|error| error.to_string())?
            .is_empty(),
        "drop must remove cancellation registration"
    );
    Ok(())
}

#[tokio::test]
async fn dropped_run_stops_its_descendants() -> Result<(), String> {
    drop_after_ready(false).await
}

#[tokio::test]
async fn dropped_run_keeps_ownership_after_leader_exit() -> Result<(), String> {
    drop_after_ready(true).await
}

#[tokio::test]
async fn command_timeout_stops_its_descendants() -> Result<(), String> {
    let fixture = Fixture::new(false)?;
    let shell = TokioShell::new();
    let mut command = fixture.command();
    command.timeout = Some(Duration::from_millis(300));
    assert!(matches!(
        shell.run(command).await,
        Err(ShellError::Timeout { .. })
    ));
    assert!(fixture.0.join("ready").is_file());
    fixture.assert_no_late_effect().await;
    assert!(
        shell
            .registry
            .lock()
            .map_err(|error| error.to_string())?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn unwind_stops_descendants_and_releases_registration() -> Result<(), String> {
    let fixture = Fixture::new(false)?;
    let shell = TokioShell::new();
    let mut run = Box::pin(shell.run(fixture.command()));
    tokio::select! {
        ready = fixture.ready() => { ready?; },
        result = &mut run => panic!("run returned early: {result:?}"),
    }
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _owned = run;
        std::panic::resume_unwind(Box::new("controlled caller unwind"));
    }));
    assert!(panic.is_err());
    fixture.assert_no_late_effect().await;
    assert!(
        shell
            .registry
            .lock()
            .map_err(|error| error.to_string())?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn natural_exit_still_collects_descendant_output() -> Result<(), String> {
    let fixture = Fixture::new(true)?;
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        TokioShell::new().run(fixture.command()),
    )
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;
    assert_eq!(result.status, 0);
    assert_eq!(
        std::fs::read_to_string(fixture.0.join("late")).map_err(|error| error.to_string())?,
        "late"
    );
    Ok(())
}

#[test]
fn cleanup_does_not_remove_a_reused_pid_registration() -> Result<(), String> {
    let shell = TokioShell::new();
    let first = shell.register(4242);
    let resources = Resources {
        registry: Arc::clone(&shell.registry),
        pid: Some(4242),
        notify: Some(first),
        scratch: None,
    };
    let replacement = shell.register(4242);
    drop(resources);
    let registry = shell.registry.lock().map_err(|error| error.to_string())?;
    assert!(Arc::ptr_eq(
        registry
            .get(&4242)
            .ok_or("replacement registration missing")?,
        &replacement
    ));
    Ok(())
}

#[tokio::test]
async fn explicit_cancel_stops_only_its_own_group() -> Result<(), String> {
    let target = Fixture::new(false)?;
    let witness = Fixture::new(false)?;
    let shell = TokioShell::new();
    let mut target_run = Box::pin(shell.run(target.command()));
    let mut witness_run = Box::pin(shell.run(witness.command()));
    tokio::select! {
        ready = async { target.ready().await?; witness.ready().await } => { ready?; },
        result = &mut target_run => panic!("target returned early: {result:?}"),
        result = &mut witness_run => panic!("witness returned early: {result:?}"),
    }
    // Identify the target through its own shell's pid, not registry ordering.
    let pid =
        std::fs::read_to_string(target.0.join("parent")).map_err(|error| error.to_string())?;
    shell
        .cancel(pid.trim())
        .await
        .map_err(|error| error.to_string())?;
    assert!(matches!(
        target_run.await,
        Err(ShellError::Cancelled { .. })
    ));
    let result = tokio::time::timeout(Duration::from_secs(5), witness_run)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    assert_eq!(result.status, 0);
    assert_eq!(
        std::fs::read_to_string(witness.0.join("late")).map_err(|error| error.to_string())?,
        "late"
    );
    target.assert_no_late_effect().await;
    assert!(
        shell
            .registry
            .lock()
            .map_err(|error| error.to_string())?
            .is_empty()
    );
    Ok(())
}
