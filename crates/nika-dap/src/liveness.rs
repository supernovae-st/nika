// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The writer's liveness — a lease beside the journal (ADR-129 · #1442).
//!
//! A journal with no terminal frame is either a run in flight or the
//! remains of a writer that died. The two used to read the same
//! (`running`, forever): the store folded frames, and a dead process
//! writes no frame. The run that writes a journal now holds
//! `<trace>.lock` — owner-only, carrying `{"pid","host"}` — under an
//! exclusive advisory lock for its lifetime; the kernel releases the lock
//! when the process ends, however it ends. A reader never guesses: it
//! asks the lock. Held → alive. Free → dead. No lease, or a lease from
//! another host → unknown, said as such.
//!
//! Run state ≠ evidence state: a dead writer proves the EVIDENCE is
//! incomplete, never that the run failed (the run's own settlement is the
//! terminal frame, and there is none).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// What the lease says about the journal's writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Liveness {
    /// The writer holds the lease: a run in flight.
    Alive {
        /// The writer's process id, as it recorded it.
        pid: u32,
    },
    /// The lease exists on this host and nobody holds it: the writer
    /// died (killed · crashed · the machine went down).
    Dead {
        /// The process id the writer recorded before it died.
        pid: u32,
    },
    /// No lease (an older engine's journal · a disabled lease) or a lease
    /// from another host: this reader cannot say.
    Unknown,
}

impl Liveness {
    /// The word (`alive` · `dead` · `unknown`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alive { .. } => "alive",
            Self::Dead { .. } => "dead",
            Self::Unknown => "unknown",
        }
    }

    /// The recorded pid, when the lease named one.
    #[must_use]
    pub const fn pid(self) -> Option<u32> {
        match self {
            Self::Alive { pid } | Self::Dead { pid } => Some(pid),
            Self::Unknown => None,
        }
    }
}

/// The lease a writer holds for its journal's lifetime — dropping it
/// releases the lock (the file stays as the record of who wrote).
#[derive(Debug)]
pub struct Lease {
    #[cfg(unix)]
    _lock: nix::fcntl::Flock<std::fs::File>,
    #[cfg(not(unix))]
    _lock: (),
}

/// `<trace>.lock` — the lease's path beside its journal.
#[must_use]
pub fn lease_path(trace: &Path) -> PathBuf {
    let mut name = trace.as_os_str().to_owned();
    name.push(".lock");
    PathBuf::from(name)
}

/// This host's name, as the lease records it (a lease is judged on the
/// host that holds it — a copied journal reads `unknown`).
#[must_use]
pub fn host_name() -> String {
    #[cfg(unix)]
    {
        nix::unistd::gethostname()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
    #[cfg(not(unix))]
    {
        String::new()
    }
}

/// Take the lease for `trace`: create `<trace>.lock` (owner-only) with
/// this process's pid and host, and hold it exclusively. Fails when the
/// lease cannot be created or is already held (two writers on one journal
/// — never legitimate).
///
/// # Errors
/// The fs error, or `WouldBlock` when another process holds the lease.
pub fn hold(trace: &Path) -> std::io::Result<Lease> {
    let path = lease_path(trace);
    // Open WITHOUT truncating: a second writer that cannot take the lease
    // must never wipe the holder's record (the lock is taken first, the
    // record written after).
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let file = opts.open(&path)?;
    #[cfg(unix)]
    {
        use nix::fcntl::{Flock, FlockArg};
        use std::io::Write as _;
        let lock = match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => lock,
            Err((_, errno)) => return Err(std::io::Error::from_raw_os_error(errno as i32)),
        };
        let record = format!(
            "{{\"pid\":{},\"host\":{}}}\n",
            std::process::id(),
            json_string(&host_name())
        );
        // The record rides the locked file: a reader that can open it sees
        // the pid; only the kernel's lock says whether the pid is alive.
        lock.set_len(0)?;
        let mut writer = &*lock;
        writer.write_all(record.as_bytes())?;
        writer.flush()?;
        Ok(Lease { _lock: lock })
    }
    #[cfg(not(unix))]
    {
        use std::io::Write as _;
        let mut file = file;
        file.set_len(0)?;
        let record = format!("{{\"pid\":{},\"host\":\"\"}}\n", std::process::id());
        file.write_all(record.as_bytes())?;
        Ok(Lease { _lock: () })
    }
}

/// Ask the lease: is the writer of `trace` alive on this host?
#[must_use]
pub fn probe(trace: &Path) -> Liveness {
    let path = lease_path(trace);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Liveness::Unknown;
    };
    let (pid, host) = parse_record(&text);
    let Some(pid) = pid else {
        return Liveness::Unknown;
    };
    if host.as_deref() != Some(host_name().as_str()) {
        return Liveness::Unknown;
    }
    #[cfg(unix)]
    {
        use nix::fcntl::{Flock, FlockArg};
        let Ok(file) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        else {
            return Liveness::Unknown;
        };
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            // Nobody holds it: the kernel released the writer's lock when
            // the writer died. Our probe lock drops here.
            Ok(_held) => Liveness::Dead { pid },
            // EWOULDBLOCK (EAGAIN on every platform this crate builds for):
            // the writer holds it.
            Err((_, nix::errno::Errno::EWOULDBLOCK)) => Liveness::Alive { pid },
            Err(_) => Liveness::Unknown,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Liveness::Unknown
    }
}

/// Remove the lease beside a journal that is being removed (best effort:
/// a missing lease is not an error).
pub fn remove_lease(trace: &Path) {
    let _ = std::fs::remove_file(lease_path(trace));
}

/// The lease record's two fields, read without a JSON dependency (the
/// record is this module's own, two scalars).
fn parse_record(text: &str) -> (Option<u32>, Option<String>) {
    let pid = text
        .split("\"pid\":")
        .nth(1)
        .and_then(|rest| rest.split([',', '}']).next())
        .and_then(|s| s.trim().parse::<u32>().ok());
    let host = text.split("\"host\":\"").nth(1).map(|rest| {
        // The scalar ends at the first unescaped quote; `\\"` and `\\\\`
        // are the two escapes the writer emits.
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(next) = chars.next() {
                        out.push(next);
                    }
                }
                '"' => break,
                c => out.push(c),
            }
        }
        out
    });
    (pid, host)
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-liveness-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir.join("run.ndjson")
    }

    /// A held lease reads alive; a dropped lease reads dead; no lease
    /// reads unknown; a lease from another host reads unknown.
    #[cfg(unix)]
    #[test]
    fn the_lease_tells_alive_from_dead_and_never_guesses() {
        let trace = scratch("ladder");
        assert_eq!(probe(&trace), Liveness::Unknown, "no lease → unknown");
        let lease = hold(&trace).expect("the lease is taken");
        assert_eq!(
            probe(&trace),
            Liveness::Alive {
                pid: std::process::id()
            }
        );
        assert!(
            hold(&trace).is_err(),
            "a second writer never takes a held lease"
        );
        drop(lease);
        assert_eq!(
            probe(&trace),
            Liveness::Dead {
                pid: std::process::id()
            },
            "released → the writer is gone"
        );
        std::fs::write(
            lease_path(&trace),
            "{\"pid\":7,\"host\":\"another-host\"}\n",
        )
        .expect("a foreign lease");
        assert_eq!(probe(&trace), Liveness::Unknown, "another host → unknown");
        remove_lease(&trace);
        assert_eq!(probe(&trace), Liveness::Unknown);
    }

    #[test]
    fn the_record_roundtrips_its_two_scalars() {
        let text = format!("{{\"pid\":42,\"host\":{}}}\n", json_string("a\"b"));
        assert_eq!(parse_record(&text), (Some(42), Some("a\"b".to_owned())));
        assert_eq!(parse_record("garbage"), (None, None));
        assert_eq!(Liveness::Dead { pid: 1 }.pid(), Some(1));
        assert_eq!(Liveness::Unknown.as_str(), "unknown");
    }
}
