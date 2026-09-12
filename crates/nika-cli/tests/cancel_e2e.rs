#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
#![allow(clippy::disallowed_types)]
#![cfg(unix)]
//! The operator's signals reach the run (#1438 · the CLI twin of the
//! runtime's `cancel_gate`): the first Ctrl-C (or a SIGTERM) cancels at the
//! wave boundary · in-flight work completes and is counted, the unstarted
//! tasks settle as cancelled by the operator, the run ends with ONE
//! `workflow_cancelled` terminal, the trace seals, the exit is the cancelled
//! class (130) · a second Ctrl-C aborts mid-flight and says so.

use std::process::Command;

use nix::sys::signal::Signal;

#[path = "support/held_run.rs"]
mod held_run;
use held_run::{HeldRun, WORKFLOW};

struct Rig {
    root: std::path::PathBuf,
}

impl Rig {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("nika-cancel-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for sub in ["home", "work"] {
            std::fs::create_dir_all(root.join(sub)).expect("rig dir");
        }
        std::fs::write(root.join("work").join("wait.nika.yaml"), WORKFLOW).expect("workflow");
        Self { root }
    }

    /// The binary on a fresh machine: cleared env, a scratch HOME, the
    /// project as cwd.
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika"));
        cmd.args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.root.join("home"))
            .env("TERM", "dumb")
            .current_dir(self.root.join("work"));
        cmd
    }

    fn spawn_run(&self) -> HeldRun {
        HeldRun::spawn(
            self.command(&["run", "wait.nika.yaml", "--json", "--max-cost-usd", "0.01"]),
            &self.root.join("work"),
        )
    }
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

fn cancels_at_the_boundary(name: &str, sig: Signal) {
    let rig = Rig::new(name);
    let mut child = rig.spawn_run();
    child.signal(sig);
    child.wait_cancelling();
    let (code, stdout, stderr) = child.finish(true);
    assert_eq!(code, 130, "the cancelled class · stderr:\n{stderr}");
    assert_eq!(
        count(&stdout, "\"kind\":\"workflow_cancelled\""),
        1,
        "one cancelled terminal:\n{stdout}"
    );
    assert_eq!(
        count(&stdout, "\"kind\":\"workflow_completed\""),
        0,
        "never completed:\n{stdout}"
    );
    let b_completed = stdout
        .lines()
        .any(|l| l.contains("\"kind\":\"task_completed\"") && l.contains("\"value\":\"b\""));
    assert!(
        b_completed,
        "in-flight work completes and is counted:\n{stdout}"
    );
    let c_cancelled = stdout
        .lines()
        .any(|l| l.contains("\"kind\":\"task_cancelled\"") && l.contains("\"value\":\"c\""));
    assert!(
        c_cancelled,
        "the unstarted task settles as cancelled:\n{stdout}"
    );
    let settled = stdout
        .lines()
        .find(|l| l.contains("\"kind\":\"run_settled\""))
        .expect("the settlement frame");
    assert!(
        settled.contains("\"status\":\"cancelled\""),
        "the settlement says cancelled: {settled}"
    );
    assert!(stderr.contains("cancelling"), "the door says so:\n{stderr}");
    let ls = rig
        .command(&["trace", "ls"])
        .output()
        .expect("trace ls runs");
    let ls = String::from_utf8_lossy(&ls.stdout);
    assert!(
        ls.contains("cancelled"),
        "trace ls names the cancellation:\n{ls}"
    );
    let trace = std::fs::read_dir(rig.root.join("work").join(".nika").join("traces"))
        .expect("the traces dir")
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
        .expect("one trace on disk");
    let verify = rig
        .command(&["trace", "verify", trace.to_str().expect("utf8 path")])
        .output()
        .expect("trace verify runs");
    assert!(
        verify.status.success(),
        "the trace seals on the cancelled terminal:\n{}{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
}

#[test]
fn a_first_ctrl_c_cancels_at_the_wave_boundary_with_a_sealed_terminal() {
    cancels_at_the_boundary("int", Signal::SIGINT);
}

#[test]
fn a_sigterm_cancels_like_the_first_ctrl_c() {
    cancels_at_the_boundary("term", Signal::SIGTERM);
}

#[test]
fn a_second_ctrl_c_aborts_mid_flight_and_says_so() {
    let rig = Rig::new("abort");
    let mut child = rig.spawn_run();
    child.signal(Signal::SIGINT);
    child.wait_cancelling();
    child.signal(Signal::SIGINT);
    let (code, stdout, stderr) = child.finish(false);
    assert_eq!(code, 130, "the cancelled class · stderr:\n{stderr}");
    assert_eq!(
        count(&stdout, "\"kind\":\"workflow_cancelled\""),
        0,
        "the run was cut before its terminal:\n{stdout}"
    );
    assert!(
        stderr.contains("cancelling"),
        "the first signal spoke:\n{stderr}"
    );
    assert!(
        stderr.contains("aborted"),
        "the second signal spoke:\n{stderr}"
    );
}
