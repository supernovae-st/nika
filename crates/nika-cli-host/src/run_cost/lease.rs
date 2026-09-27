// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The paid Run's writer lease beside its cost observation journal — the
//! ADR-129 lease applied to billing evidence. An unknown-cost Run holds it
//! from before its `prepared` row until after its `settled` row; the kernel
//! releases it however the process ends. A later review that takes the
//! lease therefore knows every leased writer of an unsettled row is gone:
//! a restart can record that Run's UNKNOWN instead of guessing.
use nika_fs::OwnedDir;
use nix::fcntl::{Flock, FlockArg};
use std::io::{Read as _, Seek as _, Write as _};

/// `<journal>.lock`, the ADR-129 lease naming, beside the journal it guards.
const LEASE: &str = "inference-cost-observations.ndjson.lock";

/// The process that holds the lease, as every row it writes names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Writer {
    pub(super) pid: u32,
    pub(super) host: String,
}

impl Writer {
    pub(super) fn this_process() -> Self {
        Self {
            pid: std::process::id(),
            host: nika_dap::liveness::host_name(),
        }
    }

    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::json!({"pid": self.pid, "host": self.host})
    }
}

/// The held lease; dropping it releases the lock (the file stays).
#[derive(Debug)]
pub(super) struct Lease {
    _lock: Flock<std::fs::File>,
}

/// The outcome of one attempt to take the lease.
#[derive(Debug)]
pub(super) enum Taken {
    Held(Lease),
    /// Another live process holds it: its Run has not settled yet.
    Busy {
        pid: Option<u64>,
    },
}

/// Take the lease without waiting and record this process in it.
///
/// # Errors
/// The lease file cannot be opened safely, locked or recorded.
pub(super) fn take(nika: &OwnedDir, writer: &Writer) -> Result<Taken, String> {
    let file = nika.open_lock(LEASE).map_err(|e| e.to_string())?;
    match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
        Ok(held) => {
            let record = format!("{}\n", writer.json());
            let mut file = &*held;
            file.set_len(0)
                .and_then(|()| file.rewind())
                .and_then(|()| file.write_all(record.as_bytes()))
                .and_then(|()| file.sync_all())
                .map_err(|e| format!("cost lease is not recordable: {e}"))?;
            Ok(Taken::Held(Lease { _lock: held }))
        }
        Err((file, nix::errno::Errno::EWOULDBLOCK)) => {
            // The holder's record is diagnostic: an unreadable one names no pid.
            let mut text = String::new();
            let pid = (&file)
                .take(4096)
                .read_to_string(&mut text)
                .ok()
                .and_then(|_| serde_json::from_str::<serde_json::Value>(&text).ok())
                .and_then(|record| record["pid"].as_u64());
            Ok(Taken::Busy { pid })
        }
        Err((_, errno)) => Err(format!("cost lease is not lockable: {errno}")),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    /// A held lease refuses a second holder and names it; a dropped lease
    /// is taken again (the kernel's release is the whole liveness proof).
    #[test]
    fn a_held_lease_is_busy_and_a_released_one_is_taken_again() {
        let root = tempfile::tempdir().unwrap();
        let nika = OwnedDir::open(root.path())
            .unwrap()
            .create_below(&[".nika"])
            .unwrap();
        let writer = Writer::this_process();
        let Taken::Held(lease) = take(&nika, &writer).unwrap() else {
            panic!("a free lease is taken");
        };
        match take(&nika, &writer).unwrap() {
            Taken::Busy { pid } => assert_eq!(pid, Some(u64::from(writer.pid))),
            Taken::Held(_) => panic!("a held lease is never shared"),
        }
        drop(lease);
        assert!(matches!(take(&nika, &writer).unwrap(), Taken::Held(_)));
        let record = std::fs::read_to_string(root.path().join(".nika").join(LEASE)).unwrap();
        let record: serde_json::Value = serde_json::from_str(&record).unwrap();
        assert_eq!(record, writer.json());
    }
}
