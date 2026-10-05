// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Reuse Spec's journal receipt judge; do not confuse Display's image sample with conformance.
use std::io::{Read, Result};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const OUTPUT_CAP: u64 = 64 * 1024;
const TRACE_SCOPE: &str = "runtime/trace";

fn command(spec: &Path, home: &Path) -> Command {
    let mut command = Command::new("python3");
    command
        .arg("-B")
        .arg(spec.join("scripts/runtime-differential.py"))
        .arg(TRACE_SCOPE)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("NIKA_BIN", env!("CARGO_BIN_EXE_nika"))
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_PASSWORD", "")
        .env("PYTHONNOUSERSITE", "1")
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    command
}

fn read_capped(reader: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(OUTPUT_CAP + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > OUTPUT_CAP {
        return Err(std::io::Error::other(
            "canonical trace runner output exceeds 64 KiB",
        ));
    }
    Ok(bytes)
}

fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;
        let pid = i32::try_from(child.id()).expect("owned child pid");
        let result = killpg(Pid::from_raw(pid), Signal::SIGKILL);
        assert!(
            result.is_ok() || result == Err(nix::errno::Errno::ESRCH),
            "kill owned runner: {result:?}"
        );
    }
    #[cfg(not(unix))]
    child.kill().expect("stop owned runner");
}

fn reader(pipe: impl Read + Send + 'static) -> Receiver<Result<Vec<u8>>> {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(read_capped(pipe));
    });
    receive
}

fn receive(pipe: &Receiver<Result<Vec<u8>>>, deadline: Instant) -> Result<Vec<u8>> {
    pipe.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|error| {
            let kind = match error {
                mpsc::RecvTimeoutError::Timeout => std::io::ErrorKind::TimedOut,
                mpsc::RecvTimeoutError::Disconnected => std::io::ErrorKind::BrokenPipe,
            };
            std::io::Error::new(kind, "canonical trace reader did not finish")
        })?
}

/// The parent and both pipes share one deadline, even if a descendant retains a pipe.
fn capture(mut child: Child, deadline: Instant) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let out = reader(child.stdout.take().expect("stdout pipe"));
    let err = reader(child.stderr.take().expect("stderr pipe"));
    let result = (|| {
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "canonical trace parent did not finish",
                ));
            }
            std::thread::sleep(remaining.min(Duration::from_millis(25)));
        };
        let stdout = receive(&out, deadline)?;
        let stderr = receive(&err, deadline)?;
        Ok((status, stdout, stderr))
    })();
    if result.is_err() {
        stop(&mut child);
        // Reap the owned parent without extending the caller's deadline. Readers are never joined.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
    result
}

pub(super) fn assert_trace(spec: &Path, media: &[String]) {
    let spec = spec.canonicalize().expect("readable Spec checkout");
    assert!(
        spec.join("scripts/runtime-differential.py").is_file(),
        "Spec canonical trace runner is required"
    );
    let home = tempfile::tempdir().expect("isolated trace home");
    let deadline = Instant::now() + Duration::from_secs(240);
    let child = command(&spec, home.path())
        .spawn()
        .expect("python3 is required for Spec trace conformance");
    let (status, stdout, stderr) = capture(child, deadline)
        .expect("canonical trace runner: 240-second total deadline, each pipe at most 64 KiB");
    let stdout = String::from_utf8(stdout).expect("UTF-8 canonical verdicts");
    let stderr = String::from_utf8_lossy(&stderr);
    assert!(
        status.success() && stderr.trim().is_empty(),
        "canonical trace conformance failed ({status}): {stdout} {stderr}"
    );
    for name in media {
        assert!(
            has_agreement(&stdout, name),
            "canonical reader did not judge {name}: {stdout}"
        );
    }
}

fn has_agreement(stdout: &str, name: &str) -> bool {
    let marker = format!("AGREE     {TRACE_SCOPE}/{name}");
    stdout.lines().any(|line| line == marker)
}

#[test]
fn canonical_agreement_requires_the_full_fixture_identity_and_verdict() {
    let name = "017-image-path-report";
    assert!(has_agreement(
        "AGREE     runtime/trace/016-other\nAGREE     runtime/trace/017-image-path-report\n",
        name
    ));
    for output in [
        "AGREE     trace/017-image-path-report",
        "DIVERGE   runtime/trace/017-image-path-report",
        "ENGINE-ERROR runtime/trace/017-image-path-report",
        "AGREE     runtime/trace/018-other",
        "AGREE     runtime/trace/017-image-path-report-extra",
        "AGREE     runtime/trace/017-image-path-report trailing",
    ] {
        assert!(!has_agreement(output, name), "invalid agreement: {output}");
    }
}

#[test]
fn canonical_command_pins_binary_scope_and_output_bound() {
    let cap = usize::try_from(OUTPUT_CAP).expect("test output cap fits usize");
    let command = command(Path::new("/spec"), Path::new("/scratch"));
    let args: Vec<_> = command
        .get_args()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        [
            "-B",
            "/spec/scripts/runtime-differential.py",
            "runtime/trace"
        ]
    );
    let binary = command
        .get_envs()
        .find(|(key, _)| *key == "NIKA_BIN")
        .expect("bound binary")
        .1;
    assert_eq!(
        binary,
        Some(std::ffi::OsStr::new(env!("CARGO_BIN_EXE_nika")))
    );
    assert_eq!(
        read_capped(&vec![b'x'; cap][..]).expect("at cap").len(),
        cap
    );
    assert!(read_capped(&vec![b'x'; cap + 1][..]).is_err());
}

#[cfg(unix)]
#[test]
fn a_descendant_retaining_pipes_cannot_extend_the_deadline() {
    use std::os::unix::process::CommandExt as _;
    let mut child = Command::new("/bin/sh")
        .args(["-c", "sleep 5 & exit 0"])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("owned process group");
    let parent_deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(status) = child.try_wait().expect("shell status") {
            assert!(status.success());
            break;
        }
        if Instant::now() >= parent_deadline {
            stop(&mut child);
            panic!("fixture shell did not exit");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let started = Instant::now();
    let error = capture(child, started + Duration::from_millis(150))
        .expect_err("inherited pipes outlive the shell parent");
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn both_readers_consume_the_same_deadline_and_keep_io_failures() {
    let (send, pipe) = mpsc::channel();
    let deadline = Instant::now() + Duration::from_millis(20);
    send.send(Ok(b"first".to_vec())).expect("first output");
    assert_eq!(receive(&pipe, deadline).expect("ready"), b"first");
    assert_eq!(
        receive(&pipe, deadline)
            .expect_err("the shared deadline has elapsed")
            .kind(),
        std::io::ErrorKind::TimedOut
    );
    send.send(Err(std::io::Error::other("capped")))
        .expect("reader error");
    assert_eq!(
        receive(&pipe, deadline)
            .expect_err("the reader error is retained")
            .to_string(),
        "capped"
    );
}
