// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This suite's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the same carve-out class as
// arm_fire.rs / bin_smoke.rs.
#![allow(clippy::disallowed_types)]

//! `nika serve` end-to-end (W5 · LE TIREUR RÉSIDENT): a tempdir project,
//! the real binary, and the injected clock (`--now`/`--until`, D5) making
//! the loop deterministic — the harness advances the clock on each sleep,
//! so the loop never waits; the SIGTERM stop rides the REAL clock.

use std::io::Write as _;
use std::process::Command;

fn bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika"));
    // A pause must PARK, never ask (the arm_fire.rs precedent).
    cmd.stdin(std::process::Stdio::null());
    cmd
}

/// A tempdir project: the registry + the workflow shelf.
fn project(tag: &str, registry: &str, workflows: &[(&str, &str)]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nika-serve-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("workflows")).expect("workflows dir");
    let mut f = std::fs::File::create(dir.join("nika.yaml")).expect("registry file");
    f.write_all(registry.as_bytes()).expect("registry body");
    for (name, body) in workflows {
        std::fs::write(dir.join("workflows").join(name), body).expect("workflow file");
    }
    dir
}

/// The trivial beat — exits 0, no provider, no key.
const TRUE: &str =
    "nika: armed-true\npermits: { exec: true }\ntasks:\n  ok:\n    exec: { shell: \"true\" }\n";

const TOUCH: &str = "nika: dry-only\npermits: { exec: true }\ntasks:\n  effect:\n    exec: { shell: \"touch should-not-exist\" }\n";

/// Daily 03:00 UTC, skip the misses.
const DAILY_3AM: &str = concat!(
    "nika: proj\n",
    "arm:\n",
    "  - workflow: workflows/doctor.nika\n",
    "    cadence: \"TZ=UTC 0 3 * * *\"\n",
    "    plafond: 0.05\n",
    "    manqué: sauter\n",
);

/// Every minute — the loop + signal tests' cadence.
const EVERY_MINUTE: &str = concat!(
    "nika: proj\n",
    "arm:\n",
    "  - workflow: workflows/doctor.nika\n",
    "    cadence: \"TZ=UTC * * * * *\"\n",
    "    plafond: 0.05\n",
    "    manqué: sauter\n",
);

/// One beat's parsed `last.json` (`None` when absent).
fn last_json(dir: &std::path::Path, label: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join(".nika/arm").join(label).join("last.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// The history's raw text (`""` when absent).
fn history(dir: &std::path::Path, label: &str) -> String {
    std::fs::read_to_string(dir.join(".nika/arm").join(label).join("history.ndjson"))
        .unwrap_or_default()
}

fn succeeded_resident_job(dir: &std::path::Path, label: &str) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(dir.join(".nika/serve/jobs/state.json")).ok()?;
    let state: serde_json::Value = serde_json::from_str(&text).ok()?;
    let workflow = format!("workflows/{label}.nika");
    state["jobs"].as_array()?.iter().find_map(|job| {
        let record = job.get("record")?;
        let origin = &record["receipt"]["origin"];
        (record["status"].as_str() == Some("succeeded")
            && record["workflow"].as_str() == Some(workflow.as_str())
            && origin["kind"].as_str() == Some("schedule")
            && origin["schedule_origin"].as_str() == Some("project")
            && origin["schedule_id"].as_str() == Some(label))
        .then(|| record.clone())
    })
}

fn tree_snapshot(root: &std::path::Path) -> Vec<(std::path::PathBuf, Option<Vec<u8>>)> {
    fn walk(
        base: &std::path::Path,
        at: &std::path::Path,
        out: &mut Vec<(std::path::PathBuf, Option<Vec<u8>>)>,
    ) {
        let mut entries = std::fs::read_dir(at)
            .expect("snapshot directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("snapshot entries");
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(base)
                .expect("snapshot relative")
                .to_owned();
            if entry.file_type().expect("snapshot type").is_dir() {
                out.push((relative, None));
                walk(base, &path, out);
            } else {
                out.push((
                    relative,
                    Some(std::fs::read(&path).expect("snapshot bytes")),
                ));
            }
        }
    }

    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn serve_once_fires_what_is_due_and_exits_zero() {
    let dir = project("once", DAILY_3AM, &[("doctor.nika", TRUE)]);
    let out = bin()
        .args(["serve", "--once", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn serve");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("fired doctor · slot 2026-08-19T03:00:00Z · exit 0"),
        "{stdout}"
    );
    let last = last_json(&dir, "doctor").expect("last.json");
    assert_eq!(last["kind"], "fired");
    assert_eq!(last["slot"], "2026-08-19T03:00:00Z");
}

#[test]
fn serve_once_dry_reports_due_beat_without_state_trace_or_effect() {
    let dir = project("once-dry", DAILY_3AM, &[("doctor.nika", TOUCH)]);
    let out = bin()
        .args(["serve", "--once", "--dry", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn dry serve");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "would fire doctor · slot 2026-08-19T03:00:00Z\n"
    );
    assert!(history(&dir, "doctor").is_empty(), "zero ledger rows");
    assert!(
        !dir.join(".nika").exists(),
        "a fresh dry project stays byte-for-byte absent"
    );
    assert!(!dir.join("should-not-exist").exists(), "zero effects");
}

#[test]
fn serve_once_dry_keeps_an_existing_sidecar_byte_identical() {
    let dir = project("once-dry-existing", DAILY_3AM, &[("doctor.nika", TRUE)]);
    let seed = bin()
        .args(["arm", "fire", "doctor", "--now", "2026-08-18T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("seed real firing");
    assert_eq!(
        seed.status.code(),
        Some(0),
        "seed stderr: {}",
        String::from_utf8_lossy(&seed.stderr)
    );
    let before = tree_snapshot(&dir.join(".nika"));

    let dry = bin()
        .args(["serve", "--once", "--dry", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("dry serve over existing state");
    assert_eq!(dry.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&dry.stdout),
        "would fire doctor · slot 2026-08-19T03:00:00Z\n"
    );
    assert_eq!(
        tree_snapshot(&dir.join(".nika")),
        before,
        "dry-run does not create a lock, repair a cache, or alter one byte"
    );
}

#[test]
fn serve_loop_fires_two_beats_in_slot_order() {
    let dir = project("loop", EVERY_MINUTE, &[("doctor.nika", TRUE)]);
    let out = bin()
        .args([
            "serve",
            "--now",
            "2026-08-19T03:02:00Z",
            "--until",
            "2026-08-19T03:03:30Z",
        ])
        .current_dir(&dir)
        .output()
        .expect("spawn serve");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "one line per beat: {stdout}");
    assert!(
        lines[0].contains("fired doctor · slot 2026-08-19T03:02:00Z"),
        "the first slot first: {stdout}"
    );
    assert!(
        lines[1].contains("fired doctor · slot 2026-08-19T03:03:00Z"),
        "the second slot second: {stdout}"
    );
    // Two fires, two lines each (W5-bis): the claim, then the receipt.
    assert_eq!(history(&dir, "doctor").lines().count(), 4);
}

#[test]
fn serve_never_fires_a_cloud_beat() {
    let registry = concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika\n",
        "    cadence: \"TZ=UTC * * * * *\"\n",
        "    où: cloud\n",
        "    plafond: 0.05\n",
        "    manqué: sauter\n",
    );
    let dir = project("cloud", registry, &[("doctor.nika", TRUE)]);
    let out = bin()
        .args(["serve", "--once", "--now", "2026-08-19T03:02:00Z"])
        .current_dir(&dir)
        .output()
        .expect("spawn serve");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("fired"),
        "a cloud beat never fires: {stdout}"
    );
    assert!(
        !dir.join(".nika/arm/doctor").exists(),
        "the cloud's calendar stays the operator's — no sidecar is even opened"
    );
}

/// unix: SIGTERM while the resident idles after one settled execution — it
/// exits clean (0) and releases its server-incarnation lease.
#[cfg(unix)]
#[test]
fn serve_stops_cleanly_on_sigterm() {
    let dir = project("sigterm", EVERY_MINUTE, &[("doctor.nika", TRUE)]);
    let mut child = bin()
        .args(["serve"])
        .current_dir(&dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn serve");
    // Wait for terminal job authority, not the legacy ARM sidecar or the
    // earlier schedule claim: only succeeded + receipt proves execution.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let fired = loop {
        if let Some(job) = succeeded_resident_job(&dir, "doctor") {
            break job;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the first resident execution never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let receipt = &fired["receipt"];
    assert_eq!(receipt["job_id"], fired["id"]);
    assert_eq!(receipt["execution_id"], fired["execution_id"]);
    assert_eq!(receipt["trace_id"], fired["trace_id"]);
    assert_eq!(receipt["snapshot_digest"], fired["snapshot_digest"]);
    assert_eq!(receipt["origin"], fired["origin"]);

    let lock_path = dir.join(".nika/serve/jobs/server.lock");
    let held_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .expect("resident server lock");
    match nix::fcntl::Flock::lock(held_file, nix::fcntl::FlockArg::LockExclusiveNonblock) {
        Err((_file, nix::errno::Errno::EAGAIN)) => {}
        Err((_file, error)) => panic!("resident lease probe refused: {error}"),
        Ok(_lease) => panic!("resident server lease was not held"),
    }

    let pid = nix::unistd::Pid::from_raw(i32::try_from(child.id()).expect("pid"));
    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).expect("kill -TERM");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!("serve ignored SIGTERM");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert_eq!(status.code(), Some(0), "SIGTERM = a clean stop");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .expect("stable server lock metadata");
    let lease = nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock)
        .map_err(|(_, error)| error)
        .expect("the kernel lease is released");
    drop(lease);
}

/// A refused registry must return before the HTTP listener can become resident.
#[cfg(unix)]
#[test]
fn missing_project_refuses_before_binding_or_creating_resident_state() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().expect("isolated project");
    std::fs::create_dir(dir.path().join("workflows")).expect("workflow shelf");
    let token = dir.path().join("token");
    std::fs::write(&token, "test-only-credential-material-0123456789").expect("token");
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600))
        .expect("private token");
    // Keep this port occupied: reaching bind would produce the wrong refusal.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("port canary");
    let address = listener.local_addr().expect("local address").to_string();
    let mut child = bin()
        .args([
            "serve",
            "--bind",
            &address,
            "--workflows",
            "workflows",
            "--token-file",
            "token",
        ])
        .current_dir(dir.path())
        .env("NIKA_KEYCHAIN", "off")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn serve");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while child.try_wait().expect("wait").is_none() {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("a missing project left serve resident");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let out = child.wait_with_output().expect("collect refusal");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("nothing armed"), "{text}");
    assert!(text.contains("nika init --project-file"), "{text}");
    assert!(
        !dir.path().join(".nika").exists(),
        "refusal creates no resident state"
    );
}

// ── Native authoring on the compile door (S06): the operator's explicit seat ──

/// The one-line HTTP exchange the native tests need: status, lowercase headers, body.
#[cfg(unix)]
fn http(address: &str, request: &str) -> (u16, String, String) {
    use std::io::Read as _;
    let mut stream = std::net::TcpStream::connect(address).expect("connect serve");
    stream.write_all(request.as_bytes()).expect("request");
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).expect("response");
    let text = String::from_utf8(bytes).expect("UTF-8 response");
    let (head, body) = text.split_once("\r\n\r\n").expect("HTTP boundary");
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("status");
    (status, head.to_ascii_lowercase(), body.to_owned())
}

#[cfg(unix)]
fn compile_post(address: &str, token: &str, body: &str) -> (u16, String, String) {
    http(
        address,
        &format!(
            "POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    )
}

/// A seat on the OpenAI-compatible wire answering every request with `text`; the bodies kept.
/// A seat on the OpenAI-compatible wire answering every request with `text` — the first one
/// held until released when `park_first` — keeping every body and `Authorization` header.
#[cfg(unix)]
struct Seat {
    port: u16,
    bodies: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    authorizations: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    entered: std::sync::mpsc::Receiver<()>,
    release: std::sync::mpsc::Sender<()>,
}

#[cfg(unix)]
fn read_seat_request(stream: &mut std::net::TcpStream) -> Option<(serde_json::Value, String)> {
    use std::io::Read as _;
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    let head_end = loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let header = |wanted: &str| {
        head.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case(wanted)
                .then(|| value.trim().to_owned())
        })
    };
    let length: usize = header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let authorization = header("authorization").unwrap_or_default();
    while buffer.len() < head_end + length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
        }
    }
    let body = serde_json::from_slice(buffer.get(head_end..head_end + length)?).ok()?;
    Some((body, authorization))
}

#[cfg(unix)]
fn seat(text: String, park_first: bool) -> Seat {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("seat");
    let port = listener.local_addr().expect("seat address").port();
    let bodies = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let authorizations = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (arrived, entered) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (seen, presented) = (
        std::sync::Arc::clone(&bodies),
        std::sync::Arc::clone(&authorizations),
    );
    // The loopback seat's accept loop: a test harness thread, never production.
    #[allow(clippy::disallowed_methods)]
    std::thread::spawn(move || {
        for (index, stream) in listener.incoming().enumerate() {
            let Ok(mut stream) = stream else { continue };
            let Some((body, authorization)) = read_seat_request(&mut stream) else {
                continue;
            };
            seen.lock().expect("bodies").push(body);
            presented.lock().expect("headers").push(authorization);
            if park_first && index == 0 {
                let _ = arrived.send(());
                let _ = released.recv();
            }
            let reply = serde_json::json!({
                "id": "chatcmpl-s06", "object": "chat.completion",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 900, "completion_tokens": 100, "total_tokens": 1000},
            })
            .to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    Seat {
        port,
        bodies,
        authorizations,
        entered,
        release,
    }
}

const TOKEN_VALUE: &str = "test-only-credential-material-0123456789";

/// The owner-only Bearer file every native test serves with.
#[cfg(unix)]
fn secure_token(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let token = dir.join("token");
    std::fs::write(&token, TOKEN_VALUE).expect("token");
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).expect("mode");
}

#[cfg(unix)]
fn free_address() -> String {
    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("free port");
    format!("127.0.0.1:{}", probe.local_addr().expect("address").port())
}

/// `nika serve` seating a native authoring model (`seat` flags) in a clean environment (`env`).
#[cfg(unix)]
fn native_serve(
    dir: &std::path::Path,
    address: &str,
    seat: &[&str],
    env: &[(&str, &str)],
) -> std::process::Child {
    let home = dir.join("home");
    std::fs::create_dir_all(&home).expect("home");
    let mut command = bin();
    command
        .args([
            "serve",
            "--bind",
            address,
            "--workflows",
            "workflows",
            "--token-file",
            "token",
        ])
        .args(seat)
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("NIKA_KEYCHAIN", "off")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for (name, value) in env {
        command.env(name, value);
    }
    command.spawn().expect("spawn serve")
}

/// The public health body, once the listener answers it.
#[cfg(unix)]
fn healthy(address: &str, child: &mut std::process::Child) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if std::net::TcpStream::connect(address).is_ok() {
            let (status, _, body) = http(
                address,
                "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            );
            if status == 200 {
                return body;
            }
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!("serve never became healthy");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// SIGTERM, then the exit code of a clean stop.
#[cfg(unix)]
fn terminate(child: &mut std::process::Child) -> Option<i32> {
    let pid = nix::unistd::Pid::from_raw(i32::try_from(child.id()).expect("pid"));
    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).expect("kill -TERM");
    let stop = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait().expect("wait") {
            return status.code();
        }
        if std::time::Instant::now() >= stop {
            let _ = child.kill();
            panic!("serve ignored SIGTERM");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

const NATIVE_INTENT: &str = "Read ./a.md and do something clever with it, then write ./b.md";

/// A candidate for [`NATIVE_INTENT`] whose run model is the placeholder the compiler asks for.
fn native_candidate() -> String {
    "nika: clever-rewrite\nmodel: mock/echo\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"./a.md\"]\n    write: [\"./b.md\"]\ntasks:\n  read_source:\n    invoke:\n      tool: \"nika:read\"\n      args: { path: \"./a.md\" }\n  transform:\n    with: { text: \"${{ tasks.read_source.output }}\" }\n    infer:\n      max_tokens: 600\n      prompt: \"Rewrite this text in a clever way, inventing nothing: ${{ with.text }}\"\n  write_result:\n    with: { content: \"${{ tasks.transform.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./b.md\", content: \"${{ with.content }}\" }\n".to_owned()
}

#[test]
fn serve_help_names_the_native_authoring_seat_and_it_needs_the_listener() {
    let help = bin().args(["serve", "--help"]).output().expect("help");
    let text = String::from_utf8_lossy(&help.stdout);
    for flag in [
        "--authoring-model",
        "--authoring-max-tokens",
        "--authoring-timeout",
        "--authoring-deadline",
        "--authoring-repairs",
        "--knowledge",
        "--knowledge-exclude",
    ] {
        assert!(text.contains(flag), "{flag}: {text}");
    }
    let refused = bin()
        .args(["serve", "--authoring-model", "vllm/s06-seat"])
        .env("NIKA_KEYCHAIN", "off")
        .output()
        .expect("usage refusal");
    assert_eq!(refused.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--bind"));
}

/// An operator's seat the server cannot honor refuses before the listener binds.
#[cfg(unix)]
#[test]
fn an_invalid_native_seat_refuses_before_binding() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = project("native-refused", DAILY_3AM, &[("doctor.nika", TRUE)]);
    let token = dir.join("token");
    std::fs::write(&token, "test-only-credential-material-0123456789").expect("token");
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).expect("mode");
    // Keep the port occupied: reaching bind would produce another refusal.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("port canary");
    let address = listener.local_addr().expect("address").to_string();
    for (flags, why) in [
        (["--authoring-model", "claude-code/default"], "harness"),
        (["--authoring-repairs", "9"], "repair rounds must be 0..=5"),
    ] {
        let mut args = vec![
            "serve",
            "--bind",
            address.as_str(),
            "--workflows",
            "workflows",
            "--token-file",
            "token",
        ];
        if flags[0] != "--authoring-model" {
            args.extend(["--authoring-model", "vllm/s06-seat"]);
        }
        args.extend(flags);
        let out = bin()
            .args(&args)
            .current_dir(&dir)
            .env("NIKA_KEYCHAIN", "off")
            .output()
            .expect("serve refusal");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert!(text.contains("native authoring refused"), "{text}");
        assert!(text.contains(why), "{text}");
        assert!(
            !text.contains("listener failed"),
            "never reached bind: {text}"
        );
    }
    drop(listener);
    let _ = std::fs::remove_dir_all(dir);
}

/// The real binary, a real Bearer, a controlled seat: the operator's flag seats it, a caller
/// opts in, the kept round replays with zero calls, and nothing is run or written.
#[cfg(unix)]
#[test]
fn serve_authors_natively_over_http_only_for_an_explicit_caller() {
    let answer = serde_json::json!({
        "candidate": native_candidate(), "questions": [], "gaps": [], "notes": "s06",
    })
    .to_string();
    let seat = seat(answer, false);
    let bodies = std::sync::Arc::clone(&seat.bodies);
    let dir = project("native-http", DAILY_3AM, &[("doctor.nika", TRUE)]);
    secure_token(&dir);
    let token_value = TOKEN_VALUE;
    let address = free_address();
    let base = format!("127.0.0.1:{}", seat.port);
    let mut child = native_serve(
        &dir,
        &address,
        &[
            "--authoring-model",
            "vllm/s06-seat",
            "--authoring-repairs",
            "0",
            "--authoring-max-tokens",
            "2048",
        ],
        &[("NIKA_VLLM_BASE_URL", base.as_str())],
    );
    let health = healthy(&address, &mut child);
    assert!(health.contains("compileNativeV2"), "{health}");
    assert!(
        !health.contains("s06-seat"),
        "the seat is never public: {health}"
    );

    // An old request never reaches the seat.
    let (status, _, body) = compile_post(
        &address,
        token_value,
        r#"{"compile_version":1,"mode":"create","intent":"hello"}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(bodies.lock().expect("bodies").len(), 0);

    // An explicit caller: one call under the operator's model and bound.
    let fresh = serde_json::json!({
        "compile_version": 2, "mode": "create", "cognition": "explicitProvider",
        "intent": NATIVE_INTENT,
    })
    .to_string();
    let (status, head, body) = compile_post(&address, token_value, &fresh);
    assert_eq!(status, 200, "{body}");
    let document: serde_json::Value = serde_json::from_str(&body).expect("document");
    assert_eq!(document["compile_version"], 2, "{document:#}");
    assert_eq!(
        document["provenance"]["authoring"]["model"],
        "vllm/s06-seat"
    );
    assert_eq!(document["provenance"]["authoring"]["calls"], 1);
    let token_line = head
        .lines()
        .find_map(|l| l.strip_prefix("nika-compile-replay:"))
        .expect("a kept round")
        .trim()
        .to_owned();
    {
        let received = bodies.lock().expect("bodies");
        assert_eq!(received.len(), 1);
        assert_eq!(received[0]["model"], "s06-seat");
        assert_eq!(received[0]["max_tokens"].as_u64(), Some(2048));
    }
    // The answer round: the kept plan, zero calls.
    let replay = serde_json::json!({
        "compile_version": 2, "mode": "create", "cognition": "deterministicOnly",
        "replay_token": token_line, "intent": NATIVE_INTENT,
        "answers": {"model": "mistral/mistral-small-latest"},
    })
    .to_string();
    let (status, _, body) = compile_post(&address, token_value, &replay);
    assert_eq!(status, 200, "{body}");
    let replayed: serde_json::Value = serde_json::from_str(&body).expect("replayed");
    assert_eq!(replayed["status"], "ready", "{replayed:#}");
    assert_eq!(bodies.lock().expect("bodies").len(), 1, "zero calls");
    assert!(!dir.join("b.md").exists() && !dir.join(".nika/traces").exists());
    assert!(
        !dir.join("clever-rewrite.nika").exists(),
        "nothing materialized"
    );

    let code = terminate(&mut child);
    assert_eq!(code, Some(0));
    let _ = std::fs::remove_dir_all(dir);
}

/// The key the seat's provider resolves through the environment's own precedence
/// (`NIKA_MISTRAL_API_KEY` over `MISTRAL_API_KEY`) is the key it sends and the key withheld.
#[cfg(unix)]
#[test]
fn the_key_the_environment_resolves_for_the_seat_is_withheld() {
    const PREFERRED: &str = "synthetic-S19-preferred-key-424242";
    const CONVENTIONAL: &str = "synthetic-S19-conventional-key-777777";
    let echoing = native_candidate().replace(
        "inventing nothing",
        &format!("inventing nothing, signed {PREFERRED}"),
    );
    let answer = serde_json::json!({
        "candidate": echoing, "questions": [], "gaps": [], "notes": "s19",
    })
    .to_string();
    let seat = seat(answer, false);
    let dir = project("native-precedence", DAILY_3AM, &[("doctor.nika", TRUE)]);
    secure_token(&dir);
    let address = free_address();
    let base = format!("http://127.0.0.1:{}/v1/chat/completions", seat.port);
    let mut child = native_serve(
        &dir,
        &address,
        &[
            "--authoring-model",
            "mistral/s19-seat",
            "--authoring-repairs",
            "0",
        ],
        &[
            ("NIKA_MISTRAL_BASE_URL", base.as_str()),
            ("NIKA_MISTRAL_API_KEY", PREFERRED),
            ("MISTRAL_API_KEY", CONVENTIONAL),
        ],
    );
    let _health = healthy(&address, &mut child);
    let fresh = serde_json::json!({
        "compile_version": 2, "mode": "create", "cognition": "explicitProvider",
        "intent": NATIVE_INTENT,
    })
    .to_string();
    let (status, head, body) = compile_post(&address, TOKEN_VALUE, &fresh);
    assert_eq!(status, 500, "{body}");
    assert!(body.contains("compile_disclosure_refused"), "{body}");
    assert!(!body.contains(PREFERRED), "{body}");
    assert!(!head.contains("nika-compile-replay"), "nothing kept");
    assert_eq!(
        *seat.authorizations.lock().expect("headers"),
        vec![format!("Bearer {PREFERRED}")],
        "the environment's own precedence chose the key sent"
    );
    assert_eq!(terminate(&mut child), Some(0));
    let _ = std::fs::remove_dir_all(dir);
}

/// SIGTERM while a native round waits on its provider: the server stops the round and exits
/// cleanly, and the released provider answer buys no repair call.
#[cfg(unix)]
#[test]
fn serve_stops_a_pending_native_round_on_sigterm_without_a_repair() {
    let broken = serde_json::json!({
        "candidate": "nika: broken\ntasks: {}\n", "questions": [], "gaps": [], "notes": "s19",
    })
    .to_string();
    let seat = seat(broken, true);
    let dir = project("native-sigterm", DAILY_3AM, &[("doctor.nika", TRUE)]);
    secure_token(&dir);
    let address = free_address();
    let base = format!("127.0.0.1:{}", seat.port);
    let mut child = native_serve(
        &dir,
        &address,
        &[
            "--authoring-model",
            "vllm/s06-seat",
            "--authoring-repairs",
            "1",
        ],
        &[("NIKA_VLLM_BASE_URL", base.as_str())],
    );
    let _health = healthy(&address, &mut child);
    let fresh = serde_json::json!({
        "compile_version": 2, "mode": "create", "cognition": "explicitProvider",
        "intent": NATIVE_INTENT,
    })
    .to_string();
    let mut caller = std::net::TcpStream::connect(&address).expect("connect serve");
    write!(
        caller,
        "POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nAuthorization: Bearer {TOKEN_VALUE}\r\nContent-Length: {}\r\n\r\n{fresh}",
        fresh.len()
    )
    .expect("request");
    seat.entered
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("the seat received the round");
    let stopping = std::time::Instant::now();
    assert_eq!(
        terminate(&mut child),
        Some(0),
        "a clean stop with a round pending"
    );
    assert!(stopping.elapsed() < std::time::Duration::from_secs(15));
    let _ = seat.release.send(());
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(
        seat.bodies.lock().expect("bodies").len(),
        1,
        "no repair call after SIGTERM"
    );
    drop(caller);
    let _ = std::fs::remove_dir_all(dir);
}

/// A resident job cannot obtain a fresh Run cost review, so an admitted API route whose price
/// needs one (an OpenAI-compatible override on plain HTTP, which `nika run` refuses before
/// dispatch) refuses before the worker starts: the job settles failed with `admission_refused`,
/// the provider sees no request and the local task ordered before the inference leaves no marker.
#[cfg(unix)]
#[test]
fn an_unreviewed_api_route_refuses_before_any_job_effect() {
    const WORKFLOW: &str = concat!(
        "nika: unreviewed\n",
        "model: openai/gpt-4.1-mini\n",
        "permits:\n",
        "  fs: { write: [\"marker.txt\"] }\n",
        "  tools: [\"nika:write\"]\n",
        "tasks:\n",
        "  mark:\n",
        "    invoke:\n",
        "      tool: \"nika:write\"\n",
        "      args: { path: \"marker.txt\", content: \"effect\" }\n",
        "  ask:\n",
        "    after: { mark: success }\n",
        "    infer: { prompt: \"Reply with one word.\", max_tokens: 32 }\n",
    );
    /// A caught regression panics before the clean stop: this test's own server is then
    /// killed and reaped on unwind, and the temp project stays behind as evidence.
    struct OwnedServe(Option<std::process::Child>);
    impl Drop for OwnedServe {
        fn drop(&mut self) {
            if let Some(mut child) = self.0.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
    let seat = seat("never requested".to_owned(), false);
    let dir = project(
        "unreviewed-api-route",
        "nika: unreviewed-api-route\n",
        &[("unreviewed.nika", WORKFLOW)],
    );
    secure_token(&dir);
    let address = free_address();
    let base = format!("http://127.0.0.1:{}/v1/chat/completions", seat.port);
    let mut serve = OwnedServe(Some(native_serve(
        &dir,
        &address,
        &[],
        &[
            ("NIKA_OPENAI_BASE_URL", base.as_str()),
            ("NIKA_OPENAI_API_KEY", "synthetic-unreviewed-route-key"),
        ],
    )));
    let _health = healthy(&address, serve.0.as_mut().expect("owned serve"));
    let job = r#"{"workflow":"workflows/unreviewed.nika"}"#;
    let (status, _, admitted) = http(
        &address,
        &format!(
            "POST /v1/jobs HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN_VALUE}\r\nIdempotency-Key: unreviewed-route-1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{job}",
            job.len()
        ),
    );
    assert_eq!(status, 202, "{admitted}");
    let admitted: serde_json::Value = serde_json::from_str(&admitted).expect("admission body");
    let id = admitted["id"].as_str().expect("job id").to_owned();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let settled = loop {
        let (status, _, body) = http(
            &address,
            &format!(
                "GET /v1/jobs/{id} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN_VALUE}\r\nConnection: close\r\n\r\n"
            ),
        );
        assert_eq!(status, 200, "{body}");
        let record: serde_json::Value = serde_json::from_str(&body).expect("job body");
        if record["status"] != "queued" && record["status"] != "running" {
            break record;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the job never settled: {record}"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert_eq!(settled["status"], "failed", "{settled}");
    assert_eq!(settled["error"]["code"], "admission_refused", "{settled}");
    assert!(
        settled["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("price unknown")),
        "{settled}"
    );
    assert!(
        seat.bodies.lock().expect("bodies").is_empty(),
        "the provider must receive no request"
    );
    assert!(
        !dir.join("marker.txt").exists(),
        "no task may run before the refusal"
    );
    let mut child = serve.0.take().expect("owned serve");
    assert_eq!(terminate(&mut child), Some(0));
    let _ = std::fs::remove_dir_all(dir);
}
