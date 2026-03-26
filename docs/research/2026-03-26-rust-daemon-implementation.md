# Research: Rust Daemon Implementation Patterns (2025/2026)

> Research for `nika-daemon` crate. Covers daemonization, IPC, lifecycle, platform integration, security, and prior art.

**Date:** 2026-03-26
**Relevance:** Phase 1 of `docs/plans/2026-03-26-nika-daemon-architecture.md`

---

## 1. Daemonization: Fork vs Modern Alternatives

### Verdict: Skip double-fork. Use self-exec pattern.

The classic Unix double-fork (`fork -> setsid -> fork -> exit parent`) is **actively harmful** in async Rust. Tokio's runtime uses thread-local globals, thread pools, and epoll/kqueue file descriptors that do not survive `fork()`. The child process inherits a corrupted runtime state.

**Three options ranked:**

| Approach | Async-safe? | Complexity | Recommendation |
|----------|------------|------------|----------------|
| Self-exec (`Command::new(current_exe()).arg("--daemon").spawn()`) | Yes | Low | **Do this** |
| Single fork + setsid (before tokio starts) | Fragile | Medium | Avoid |
| Double fork | No | High | Never in async Rust |

### Self-exec pattern (recommended)

The process re-launches itself with a `daemon run --foreground` hidden subcommand. The parent spawns the child, writes the PID file, and exits. The child starts the tokio runtime fresh with no inherited state.

```rust
use std::process::Command;

pub fn daemonize(sock_dir: &Path) -> Result<u32, NikaError> {
    let exe = std::env::current_exe()
        .map_err(|e| NikaError::daemon(160, format!("cannot find self: {e}")))?;

    let child = Command::new(&exe)
        .args(["daemon", "run", "--foreground"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .current_dir("/")
        .spawn()
        .map_err(|e| NikaError::daemon(160, format!("spawn failed: {e}")))?;

    let pid = child.id();
    write_pid_file(sock_dir, pid)?;
    Ok(pid)
}
```

**Why this is what turbo does:** Turborepo's daemon uses exactly this pattern. The `turbo` CLI spawns a separate daemon process. The client auto-starts the daemon if it cannot connect to the gRPC socket. There is no `fork()` anywhere in their Rust code.

### Gotchas

- **macOS: `current_exe()` returns the real path**, not a symlink. If installed via Homebrew, the path may change on upgrade. Use `which nika` or resolve the canonical path.
- **Linux: `/proc/self/exe` is reliable** but may be deleted if the binary was replaced during an upgrade. The daemon should handle `ENOENT` on re-exec gracefully.
- **Never fork after tokio starts.** Not even "just once." The child inherits kqueue/epoll FDs, timer wheels, and I/O driver state that will cause silent corruption.

---

## 2. Unix Socket IPC with Tokio

### Verdict: Do it. Length-prefixed JSON over `tokio::net::UnixStream`.

### Wire format

```
[4-byte big-endian u32 length][JSON payload]
```

Maximum message size: **4 MB** (generous for JSON-RPC-style messages, prevents OOM).

### Server skeleton

```rust
use tokio::net::UnixListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_MSG_SIZE: usize = 4 * 1024 * 1024; // 4 MB

pub async fn serve(listener: UnixListener, shutdown: tokio::sync::watch::Receiver<bool>) {
    loop {
        tokio::select! {
            Ok((stream, _addr)) = listener.accept() => {
                tokio::spawn(handle_connection(stream));
            }
            _ = shutdown.changed() => break,
        }
    }
}

async fn handle_connection(mut stream: tokio::net::UnixStream) -> Result<(), Error> {
    loop {
        // Read length prefix
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).await.is_err() {
            return Ok(()); // Client disconnected
        }
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > MAX_MSG_SIZE {
            return Err(Error::MessageTooLarge(len));
        }

        // Read payload
        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await?;

        // Deserialize, dispatch, serialize response
        let request: DaemonRequest = serde_json::from_slice(&buf)?;
        let response = dispatch(request).await;
        let response_bytes = serde_json::to_vec(&response)?;

        // Write response
        let resp_len = (response_bytes.len() as u32).to_be_bytes();
        stream.write_all(&resp_len).await?;
        stream.write_all(&response_bytes).await?;
        stream.flush().await?;
    }
}
```

### Client skeleton

```rust
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct DaemonClient {
    stream: UnixStream,
}

impl DaemonClient {
    pub async fn connect(sock_path: &Path) -> Result<Self, Error> {
        let stream = UnixStream::connect(sock_path).await?;
        Ok(Self { stream })
    }

    pub async fn send(&mut self, request: &DaemonRequest) -> Result<DaemonResponse, Error> {
        let bytes = serde_json::to_vec(request)?;
        let len = (bytes.len() as u32).to_be_bytes();

        self.stream.write_all(&len).await?;
        self.stream.write_all(&bytes).await?;
        self.stream.flush().await?;

        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;

        let mut buf = vec![0u8; resp_len];
        self.stream.read_exact(&mut buf).await?;

        Ok(serde_json::from_slice(&buf)?)
    }
}
```

### Gotchas

- **Backpressure:** `write_all()` + `flush()` naturally applies backpressure via tokio's write buffer. No manual flow control needed for request-response patterns.
- **Connection per request vs persistent:** Use one connection per CLI invocation (simpler, no connection pooling needed). The daemon spawns a task per connection.
- **Socket cleanup:** Always `std::fs::remove_file(sock_path)` before `UnixListener::bind()`. Check for stale socket first (try connect, if fails, remove).
- **Partial reads:** `read_exact()` handles this correctly. Never use `read()` for framed protocols.
- **No abstract sockets for nika:** Abstract sockets are Linux-only. Nika must support macOS. Use filesystem sockets at `~/.nika/daemon/nika.sock`.

---

## 3. nix Crate 0.29

### Verdict: Use it for signal handling only. Skip fork.

Since we use the self-exec pattern, we only need `nix` for:
- `kill(pid, None)` -- check if a process is alive (PID file liveness)
- Signal constants (but tokio handles signal listening natively)

### API (unchanged from 0.28)

```rust
// ForkResult (NOT NEEDED for self-exec, documented for reference)
pub enum ForkResult {
    Parent { child: Pid },
    Child,
}

// Process liveness check
use nix::sys::signal::kill;
use nix::unistd::Pid;

pub fn is_process_alive(pid: u32) -> bool {
    kill(Pid::from_raw(pid as i32), None).is_ok()
}
```

### Breaking changes 0.28 -> 0.29

- `SigAction` is now `#[repr(transparent)]` (non-breaking for normal use).
- `SignalFd` supports shared references (`&SignalFd`).
- MSRV raised to Rust 1.69.
- `RawFd` -> `AsFd` migration in some I/O functions (not fork/signal).
- **No breaking changes to `fork()`, `setsid()`, or signal APIs.**

### Recommendation

Keep `nix = { version = "0.29", features = ["signal", "process"] }` in dependencies. Use it for `kill()` liveness checks. Use `tokio::signal` for actual signal handling in the async daemon.

---

## 4. macOS launchd Plist Generation

### Verdict: Do it. Essential for macOS auto-start.

### Complete plist template

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>studio.supernovae.nika.daemon</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/nika</string>
        <string>daemon</string>
        <string>run</string>
        <string>--foreground</string>
    </array>

    <key>RunAtLoad</key>
    <false/>

    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>

    <key>StandardOutPath</key>
    <string>/tmp/nika-daemon.out.log</string>

    <key>StandardErrorPath</key>
    <string>/tmp/nika-daemon.err.log</string>

    <key>ProcessType</key>
    <string>Background</string>

    <key>Nice</key>
    <integer>5</integer>

    <key>SoftResourceLimits</key>
    <dict>
        <key>NumberOfFiles</key>
        <integer>4096</integer>
    </dict>
</dict>
</plist>
```

### Installation from Rust

```rust
pub fn install_launchd(bin_path: &Path) -> Result<(), NikaError> {
    let label = "studio.supernovae.nika.daemon";
    let plist_dir = dirs::home_dir()
        .ok_or_else(|| NikaError::daemon(160, "no home dir"))?
        .join("Library/LaunchAgents");

    std::fs::create_dir_all(&plist_dir)?;
    let plist_path = plist_dir.join(format!("{label}.plist"));

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>daemon</string>
        <string>run</string>
        <string>--foreground</string>
    </array>
    <key>RunAtLoad</key>
    <false/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>StandardOutPath</key>
    <string>/tmp/nika-daemon.out.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/nika-daemon.err.log</string>
    <key>ProcessType</key>
    <string>Background</string>
</dict>
</plist>"#,
        label = label,
        bin = bin_path.display(),
    );

    std::fs::write(&plist_path, plist_content)?;
    Ok(())
}
```

### Key decisions

| Field | Value | Why |
|-------|-------|-----|
| `RunAtLoad` | `false` | Daemon is opt-in, not auto-start. User runs `nika daemon install` then `nika daemon start`. |
| `KeepAlive.SuccessfulExit` | `false` | Restart if daemon crashes. Normal `nika daemon stop` sends SIGTERM which exits 0, so launchd will NOT restart. |
| `KeepAlive.Crashed` | `true` | Restart on abnormal exit (SIGSEGV, etc). |
| `ProcessType` | `Background` | Lower scheduling priority, appropriate for a helper daemon. |

### Loading commands

```bash
# Modern macOS (10.10+) -- use bootstrap/bootout, not load/unload
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/studio.supernovae.nika.daemon.plist
launchctl bootout gui/$(id -u)/studio.supernovae.nika.daemon

# Legacy fallback
launchctl load ~/Library/LaunchAgents/studio.supernovae.nika.daemon.plist
launchctl unload ~/Library/LaunchAgents/studio.supernovae.nika.daemon.plist
```

### Gotchas

- **Label must match filename** (minus `.plist` extension).
- **Permissions:** The plist file should be owned by the user, mode 644. NOT root-owned for LaunchAgents.
- **Binary path must be absolute.** Resolve `which nika` at install time, not at plist-generation time.
- **Log rotation:** launchd does NOT rotate logs. Use `/tmp/` or implement rotation. Consider `~/.nika/daemon/` instead for persistence.
- **ProgramArguments is an array**, not a string. Each argument is a separate `<string>` element.
- **KeepAlive dict vs boolean:** Use the dict form for fine-grained control. A plain `<true/>` restarts unconditionally (annoying during development).

---

## 5. systemd User Service Files

### Verdict: Do it. Essential for Linux auto-start.

### Complete unit file

```ini
[Unit]
Description=Nika Workflow Engine Daemon
Documentation=https://github.com/supernovae-st/nika
After=default.target

[Service]
Type=simple
ExecStart=/usr/local/bin/nika daemon run --foreground
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=nika_daemon=info

[Install]
WantedBy=default.target
```

### Installation from Rust

```rust
pub fn install_systemd(bin_path: &Path) -> Result<(), NikaError> {
    let unit_dir = dirs::config_dir()
        .ok_or_else(|| NikaError::daemon(160, "no config dir"))?
        .join("systemd/user");

    std::fs::create_dir_all(&unit_dir)?;
    let unit_path = unit_dir.join("nika-daemon.service");

    let unit_content = format!(
        r#"[Unit]
Description=Nika Workflow Engine Daemon
After=default.target

[Service]
Type=simple
ExecStart={bin} daemon run --foreground
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#,
        bin = bin_path.display(),
    );

    std::fs::write(&unit_path, unit_content)?;
    Ok(())
}
```

### Socket activation

**Skip for Phase 1.** Socket activation (`Type=notify` + `.socket` unit) is powerful but adds complexity:

- Requires the `systemd` crate for `sd_listen_fds()` to inherit the socket FD.
- Adds a second unit file (`.socket`).
- Only useful if daemon startup is slow and you want on-demand activation.
- Nika daemon starts fast (no DB migration, no network setup), so `Type=simple` is sufficient.

**Defer to Phase 4** if users request on-demand activation.

### sd-notify

**Skip for Phase 1.** `Type=notify` with `sd_notify(READY=1)` is useful for daemons with slow init (DB connections, network probes). Nika daemon binds a socket and is ready in milliseconds. `Type=simple` is correct.

**Defer** unless we add SQLite migration or provider health probes to startup.

### Commands

```bash
# Install and enable
systemctl --user daemon-reload
systemctl --user enable nika-daemon.service
systemctl --user start nika-daemon.service

# Check status
systemctl --user status nika-daemon.service

# View logs
journalctl --user -u nika-daemon.service -f

# Uninstall
systemctl --user stop nika-daemon.service
systemctl --user disable nika-daemon.service
rm ~/.config/systemd/user/nika-daemon.service
systemctl --user daemon-reload
```

### Gotchas

- **`systemctl --user` requires `loginctl enable-linger $USER`** for the daemon to survive logout. Without linger, user services stop when the last session ends.
- **`XDG_RUNTIME_DIR`** must be set. systemd sets it to `/run/user/$UID` for user services.
- **Socket path:** Use `$XDG_RUNTIME_DIR/nika/nika.sock` on Linux (standard), fall back to `~/.nika/daemon/nika.sock` if not available.

---

## 6. Windows Named Pipes

### Verdict: Defer. Unix-only for Phase 1-4.

Windows Named Pipes are the correct IPC mechanism on Windows (`tokio::net::windows::named_pipe`), but:

- Nika has **zero Windows users** and zero backward compat obligations.
- Named Pipes require `#[cfg(windows)]` guards throughout the IPC layer.
- The API is different enough to need a full abstraction layer.
- Testing requires Windows CI.

**The right approach:**

1. Design the `DaemonClient`/`DaemonServer` trait around an abstract transport.
2. Implement `UnixTransport` for Phase 1.
3. Add `NamedPipeTransport` later if Windows demand materializes.

```rust
// Future-proof trait design
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&mut self, msg: &[u8]) -> Result<(), Error>;
    async fn recv(&mut self) -> Result<Vec<u8>, Error>;
}
```

### tokio Named Pipe API (for reference)

```rust
// Server
use tokio::net::windows::named_pipe::ServerOptions;

let server = ServerOptions::new()
    .first_pipe_instance(true)
    .create(r"\\.\pipe\nika-daemon")?;
server.connect().await?;  // Wait for client

// Client
use tokio::net::windows::named_pipe::ClientOptions;
let client = ClientOptions::new().open(r"\\.\pipe\nika-daemon")?;
```

Named Pipes use `AsyncReadExt`/`AsyncWriteExt` just like Unix streams, so the length-prefixed JSON layer would be identical.

---

## 7. Health Check Patterns

### Verdict: Do it. Socket probe + PID check.

For a local CLI daemon (not an HTTP service), health checks are:

### Three-tier check

```
Level 1: PID alive?     (kill -0 $PID)           -- fast, no socket needed
Level 2: Socket responds? (connect + Ping/Pong)   -- confirms daemon is functional
Level 3: Services healthy? (Status request)        -- confirms all subsystems work
```

### Implementation

```rust
pub enum HealthStatus {
    /// Daemon is not running (no PID file or stale PID)
    Dead,
    /// Daemon process exists but socket is unresponsive
    Zombie,
    /// Daemon responds to Ping but services report issues
    Degraded { issues: Vec<String> },
    /// All systems operational
    Healthy { uptime: Duration, services: Vec<ServiceStatus> },
}

pub async fn check_health(sock_dir: &Path) -> HealthStatus {
    // Level 1: PID check
    let pid_path = sock_dir.join("nika.pid");
    let pid = match read_pid_file(&pid_path) {
        Ok(pid) => pid,
        Err(_) => return HealthStatus::Dead,
    };

    if !is_process_alive(pid) {
        // Stale PID file -- clean up
        let _ = std::fs::remove_file(&pid_path);
        let _ = std::fs::remove_file(sock_dir.join("nika.sock"));
        return HealthStatus::Dead;
    }

    // Level 2: Socket probe
    let sock_path = sock_dir.join("nika.sock");
    let mut client = match DaemonClient::connect(&sock_path).await {
        Ok(c) => c,
        Err(_) => return HealthStatus::Zombie,
    };

    // Level 3: Full status
    match client.send(&DaemonRequest::Status).await {
        Ok(DaemonResponse::StatusInfo { uptime_secs, services, .. }) => {
            let issues: Vec<String> = services.iter()
                .filter(|s| !s.healthy)
                .map(|s| format!("{}: {}", s.name, s.message))
                .collect();
            if issues.is_empty() {
                HealthStatus::Healthy {
                    uptime: Duration::from_secs(uptime_secs),
                    services,
                }
            } else {
                HealthStatus::Degraded { issues }
            }
        }
        _ => HealthStatus::Zombie,
    }
}
```

### Stale PID detection

```rust
fn is_process_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    // kill with signal 0: checks existence without sending a signal
    kill(Pid::from_raw(pid as i32), None).is_ok()
}
```

### Stale socket detection

Before binding, try to connect. If connection fails with `ConnectionRefused`, the socket is stale:

```rust
async fn clean_stale_socket(sock_path: &Path) -> Result<(), Error> {
    if sock_path.exists() {
        match tokio::net::UnixStream::connect(sock_path).await {
            Ok(_) => {
                // Another daemon is running
                return Err(Error::AlreadyRunning);
            }
            Err(_) => {
                // Stale socket, remove it
                std::fs::remove_file(sock_path)?;
            }
        }
    }
    Ok(())
}
```

### Integration with `nika doctor`

```
$ nika doctor
  ...
  Daemon ............ running (PID 12345, uptime 2h 15m)
    Secrets ........ healthy
    Cache .......... healthy (142 entries, 87% hit rate)
    Jobs ........... healthy (0 running, 12 completed)
  ...
```

---

## 8. Daemon Security

### Socket permissions

```rust
use std::os::unix::fs::PermissionsExt;

fn bind_socket(sock_path: &Path) -> Result<UnixListener, Error> {
    clean_stale_socket(sock_path).await?;
    let listener = UnixListener::bind(sock_path)?;

    // Set socket to owner-only (0o600)
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(sock_path, perms)?;

    Ok(listener)
}
```

### PID file with flock

The safe pattern: open, lock, write, hold lock until exit.

```rust
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;

pub struct PidFile {
    file: File,
    path: PathBuf,
}

impl PidFile {
    pub fn acquire(path: &Path) -> Result<Self, NikaError> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| NikaError::daemon(160, format!("pid file open: {e}")))?;

        // Try exclusive non-blocking lock
        let fd = file.as_raw_fd();
        let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::WouldBlock {
                return Err(NikaError::daemon(160, "daemon already running (PID file locked)"));
            }
            return Err(NikaError::daemon(160, format!("flock failed: {err}")));
        }

        // Write PID
        let mut file = file;
        write!(file, "{}", std::process::id())
            .map_err(|e| NikaError::daemon(160, format!("pid write: {e}")))?;
        file.sync_all()
            .map_err(|e| NikaError::daemon(160, format!("pid sync: {e}")))?;

        Ok(Self { file, path: path.to_owned() })
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        // Lock is automatically released when file is dropped
    }
}
```

### Graceful shutdown with tokio

```rust
use tokio::signal::unix::{signal, SignalKind};

pub async fn run_daemon(listener: UnixListener, pid_file: PidFile) {
    let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("SIGINT handler");
    let mut sighup = signal(SignalKind::hangup()).expect("SIGHUP handler");

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let server = tokio::spawn(serve(listener, shutdown_rx));

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::info!("SIGTERM received, shutting down");
        }
        _ = sigint.recv() => {
            tracing::info!("SIGINT received, shutting down");
        }
        _ = sighup.recv() => {
            tracing::info!("SIGHUP received, reloading config");
            // TODO: reload config, don't shutdown
            // For now, treat as shutdown
        }
    }

    // Signal all tasks to stop
    let _ = shutdown_tx.send(true);

    // Wait for server to drain (with timeout)
    let drain_timeout = tokio::time::timeout(
        Duration::from_secs(5),
        server,
    ).await;

    match drain_timeout {
        Ok(Ok(())) => tracing::info!("clean shutdown"),
        Ok(Err(e)) => tracing::warn!("server error during shutdown: {e}"),
        Err(_) => tracing::warn!("shutdown timed out after 5s, forcing exit"),
    }

    // PidFile::drop runs here, removing the PID file and releasing the flock
    drop(pid_file);
}
```

### Signal handling edge cases

- **SIGHUP:** Traditionally means "reload config." Phase 1: treat as shutdown. Phase 2: implement config reload.
- **Double SIGTERM:** Second signal during drain should force-exit. Add a counter or use `select!` with a second signal arm.
- **SIGPIPE:** Ignored by default in Rust. No action needed.
- **Child process cleanup:** Jobs spawned by the daemon should be killed on shutdown. Use process groups (`setsid` for child) and `SIGTERM` the group.

---

## 9. keyring Crate v3

### Verdict: Do it. Use `apple-native` feature. Call from `spawn_blocking`.

### Current API (v3.6.x)

```rust
use keyring::Entry;

// Create entry (service name, username)
let entry = Entry::new("nika", "anthropic-api-key")?;

// Store
entry.set_password("sk-ant-...")?;

// Retrieve
let secret = entry.get_password()?;  // Returns String

// Delete
entry.delete_credential()?;

// Binary secrets (v3+)
entry.set_secret(vec![0u8; 32])?;
let bytes = entry.get_secret()?;  // Returns Vec<u8>
```

### Async wrapper for daemon use

```rust
pub async fn get_secret(provider: &str) -> Result<Option<String>, NikaError> {
    let provider = provider.to_owned();
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new("nika", &provider)
            .map_err(|e| NikaError::daemon(161, format!("keyring init: {e}")))?;
        match entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(NikaError::daemon(161, format!("keyring get: {e}"))),
        }
    })
    .await
    .map_err(|e| NikaError::daemon(161, format!("spawn_blocking: {e}")))?
}
```

### Cargo.toml

```toml
keyring = { version = "3", features = ["apple-native", "windows-native"] }
```

### Gotchas

- **macOS Keychain popups:** In a daemon process (no UI), keychain access does NOT trigger popups if the login keychain is unlocked (which it is when the user is logged in). First access after a fresh boot MAY prompt. The daemon should handle this gracefully (return error, let CLI fallback to env vars).
- **Thread safety:** `Entry` is not `Send`/`Sync`. Always create entries inside `spawn_blocking`, never share across threads.
- **MSRV:** Requires Rust 1.75+.
- **Fallback:** If `apple-native` feature is missing, keyring falls back to a mock store. Always verify the feature flag is enabled.
- **Test safety:** Use `keyring::mock` or env vars in tests. NEVER hit the real keychain in CI. This is already documented in the project memory (`feedback_no_keychain_popup.md`).

---

## 10. Prior Art: How Existing Rust Daemons Work

### Turborepo (`turbo daemon`)

| Aspect | Implementation |
|--------|---------------|
| **IPC** | gRPC (tonic) over Unix domain sockets |
| **Daemonization** | Self-exec pattern (CLI spawns daemon process) |
| **Auto-start** | Client tries to connect; on failure, spawns daemon |
| **Protocol** | Protobuf-defined messages |
| **Socket path** | `~/.turbo/daemon/` or `$XDG_RUNTIME_DIR/turbo/` |
| **Lifecycle** | PID file + socket liveness check |
| **Fallback** | `--no-daemon` flag for standalone mode |

**Key insight:** turbo uses gRPC which is heavier than needed for nika. Length-prefixed JSON is simpler, has zero codegen, and is debuggable with `socat`.

### bacon (Rust code checker)

- **No daemon mode.** Runs as a foreground TUI process.
- Uses `notify` crate for file watching (inotify/kqueue).
- Terminal lifecycle (Ctrl+C to stop).
- **Lesson:** Not all watchers need a daemon. `nika watch` in Phase 3 could be either daemon-integrated or standalone.

### cargo-watch

- **No daemon mode.** Foreground process with shell backgrounding (`&`).
- File watching via `notify`.
- Now in "live support" status (deprecated in favor of bacon).
- **Lesson:** Simple tools stay simple. Daemon is justified for nika because of secrets + jobs + cache.

### rust-analyzer

| Aspect | Implementation |
|--------|---------------|
| **IPC** | LSP protocol over stdio (not sockets) |
| **Lifecycle** | Started by editor, dies when editor closes |
| **No daemon** | Intentionally NOT a daemon -- one instance per editor window |

**Lesson:** LSP over stdio is simpler than sockets for editor integration. Nika LSP (`nika-lsp`) should stay as stdio. The daemon is separate.

### watchman (Facebook)

| Aspect | Implementation |
|--------|---------------|
| **Language** | C++ (with Rust bindings) |
| **IPC** | Unix domain sockets, BSER (Binary SERialization) protocol |
| **Daemonization** | Classic double-fork |
| **Auto-start** | Client spawns watchman if socket missing |
| **Socket path** | `/var/run/watchman/$USER-state/` |

**Lesson:** BSER is faster than JSON but opaque. For nika, JSON is fine -- messages are small and human-debuggable.

---

## Summary: Decision Matrix

| Topic | Decision | Phase |
|-------|----------|-------|
| Daemonization | Self-exec (`Command::new(current_exe())`) | Phase 1 |
| IPC | Length-prefixed JSON over Unix socket | Phase 1 |
| Signal handling | `tokio::signal` (SIGTERM, SIGINT, SIGHUP) | Phase 1 |
| PID file | flock-based, auto-cleanup on drop | Phase 1 |
| Socket permissions | `0o600` (owner-only) | Phase 1 |
| Health checks | 3-tier (PID, socket, services) | Phase 1 |
| macOS launchd | Generate plist, `launchctl bootstrap` | Phase 1 |
| Linux systemd | Generate unit file, `systemctl --user` | Phase 1 |
| Secrets (keyring) | v3 + apple-native + `spawn_blocking` | Phase 1 |
| nix crate | 0.29, `kill()` only (no fork) | Phase 1 |
| Socket activation | Skip | Defer (Phase 4+) |
| sd-notify | Skip | Defer (Phase 4+) |
| Windows Named Pipes | Skip | Defer (post-launch) |
| Abstract sockets | Skip (macOS compat) | Never |
| gRPC/protobuf | Skip (overkill) | Never |
| Double fork | Skip (async-unsafe) | Never |

---

## Recommended Dependency Additions

```toml
# nika-daemon/Cargo.toml (Phase 1 only)
[dependencies]
tokio = { workspace = true, features = ["net", "signal", "process", "io-util", "time"] }
serde = { workspace = true }
serde_json = { workspace = true }
keyring = { version = "3", features = ["apple-native"] }
nix = { version = "0.29", features = ["signal", "process"] }
tracing = { workspace = true }
dirs = { workspace = true }

# Phase 2 additions
rusqlite = { version = "0.32", features = ["bundled"] }
cron = "0.13"
chrono = { workspace = true }

# Phase 3 additions
notify = "7"
notify-debouncer-full = "0.4"
blake3 = { workspace = true }
dashmap = { workspace = true }
```

---

## Sources

- Perplexity searches (2026-03-26): Rust daemon patterns, tokio Unix sockets, nix 0.29, launchd, systemd, Windows Named Pipes, keyring v3, turbo/bacon/watchman architecture
- Turborepo source: github.com/vercel/turborepo (daemon implementation uses gRPC over Unix sockets, self-exec pattern)
- nix crate changelog: 0.28 -> 0.29 has no breaking changes for fork/signal APIs
- keyring crate: docs.rs/keyring/3 (apple-native backend, spawn_blocking pattern)
- Existing nika plan: `docs/plans/2026-03-26-nika-daemon-architecture.md`
