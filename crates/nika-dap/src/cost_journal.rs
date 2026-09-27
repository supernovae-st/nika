// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The paid Run's cost observation journal (`.nika/inference-cost-observations.ndjson`)
//! and its writer lease — billing evidence beside the trace plane's own
//! ADR-129 lease. Descended from nika-cli-host's `run_cost` at the 15k wall
//! (2026-09-28): the lease, the strict fold and the torn-tail append are
//! compute; the question, the live account and its rows stay with the host.
//!
//! An unknown-cost Run holds the lease (`<journal>.lock`) from before its
//! `prepared` row until after its `settled` row; the kernel releases it however
//! the process ends. A later review that takes the lease therefore knows every
//! leased writer of an unsettled row is gone: a process killed mid-dispatch
//! runs no handler, so the restart derives that Run's UNKNOWN from the
//! `prepared` row and appends it once, never rewriting the rows it read.

use nika_fs::OwnedDir;
use nix::fcntl::{Flock, FlockArg};
use std::collections::BTreeMap;
use std::io::{Read as _, Seek as _, Write as _};
use std::path::Path;

/// The journal's name under the project's `.nika/` directory.
pub const JOURNAL: &str = "inference-cost-observations.ndjson";

/// `<journal>.lock`, the ADR-129 lease naming, beside the journal it guards.
const LEASE: &str = "inference-cost-observations.ndjson.lock";

/// The process that holds the lease, as every row it writes names it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Writer {
    /// The writer's process id.
    pub pid: u32,
    /// The host it runs on: a lease is judged on its own host only.
    pub host: String,
}

impl Writer {
    /// This process, on this host.
    #[must_use]
    pub fn this_process() -> Self {
        Self {
            pid: std::process::id(),
            host: crate::liveness::host_name(),
        }
    }

    /// The `{"pid","host"}` record the lease and every row carry.
    #[must_use]
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"pid": self.pid, "host": self.host})
    }
}

/// The held lease; dropping it releases the lock (the file stays).
#[derive(Debug)]
pub struct Lease {
    _lock: Flock<std::fs::File>,
}

/// The outcome of one attempt to take the lease.
#[derive(Debug)]
#[non_exhaustive]
pub enum Taken {
    /// This process now holds the lease.
    Held(Lease),
    /// Another live process holds it: its Run has not settled yet.
    Busy {
        /// The holder's recorded pid, when its record is readable.
        pid: Option<u64>,
    },
}

/// Take the lease without waiting and record this process in it.
///
/// # Errors
/// The lease file cannot be opened safely, locked or recorded.
pub fn take(nika: &OwnedDir, writer: &Writer) -> std::io::Result<Taken> {
    let file = nika.open_lock(LEASE)?;
    match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
        Ok(held) => {
            let record = format!("{}\n", writer.json());
            let mut file = &*held;
            file.set_len(0)?;
            file.rewind()?;
            file.write_all(record.as_bytes())?;
            file.sync_all()?;
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
        Err((_, errno)) => Err(std::io::Error::from_raw_os_error(errno as i32)),
    }
}

/// Why an earlier invocation still blocks a new unknown-cost Run.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Exposure {
    /// It settled, and a request it sent ended without usable settlement.
    Uncertain,
    /// Its leased writer ended without settling: recorded as unknown.
    Unknown {
        /// The writer the lease named, when recorded.
        pid: Option<u64>,
    },
    /// Admitted and never settled, with no lease this host can judge (a row
    /// written before the lease existed, or on another host).
    Unjudged,
}

/// One earlier invocation that still blocks, named by its own identity.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Blocker {
    /// The invocation the rows name.
    pub invocation: String,
    /// Why it blocks.
    pub exposure: Exposure,
}

impl Blocker {
    /// One named blocker.
    #[must_use]
    pub fn new(invocation: String, exposure: Exposure) -> Self {
        Self {
            invocation,
            exposure,
        }
    }
}

/// What the journal still holds against a new unknown-cost Run: the Runs it
/// names, and the rows a killed writer cut mid-write (by the digest of their
/// exact bytes, since no identity survives the cut).
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Exposures {
    /// The blocking invocations, in invocation order.
    pub runs: Vec<Blocker>,
    /// The sha256 of every row cut mid-write.
    pub torn: Vec<String>,
}

impl Exposures {
    /// The named Runs and the cut rows.
    #[must_use]
    pub fn new(runs: Vec<Blocker>, torn: Vec<String>) -> Self {
        Self { runs, torn }
    }

    /// Nothing blocks a new unknown-cost Run.
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.runs.is_empty() && self.torn.is_empty()
    }
}

/// Fold the journal while holding the lease: append the UNKNOWN row of every
/// leased invocation that never settled, then return every exposure.
///
/// # Errors
/// An unreadable, oversized or unrecognized journal (`InvalidData`: prior
/// exposure is unknown), or an UNKNOWN row that cannot be appended.
pub fn fold(nika: &OwnedDir, host: &str, observer: &str) -> std::io::Result<Exposures> {
    let text = read(nika)?;
    let (latest, torn) = latest(&text)?;
    let mut exposures = Exposures::new(Vec::new(), torn);
    for (invocation, (row, line)) in latest {
        let exposure = match row["phase"].as_str() {
            Some("settled") if row["observation"]["state"] == "Uncertain" => Exposure::Uncertain,
            Some("settled") => continue,
            Some("unknown") => Exposure::Unknown {
                pid: row["unsettled"]["writer"]["pid"].as_u64(),
            },
            // `prepared`: the lease this reader holds proves a leased writer on
            // this host is gone; anything else cannot be judged from here.
            _ if !host.is_empty() && row["lease"]["host"].as_str() == Some(host) => {
                let derived = unknown_row(&invocation, &row, line, observer);
                append_row(nika, &derived.to_string()).map_err(|e| {
                    std::io::Error::new(
                        e.kind(),
                        format!("cannot record an unsettled Run as unknown: {e}"),
                    )
                })?;
                Exposure::Unknown {
                    pid: row["lease"]["pid"].as_u64(),
                }
            }
            _ => Exposure::Unjudged,
        };
        exposures.runs.push(Blocker::new(invocation, exposure));
    }
    Ok(exposures)
}

/// Every exposure, named by identity, without promising a gesture this binary lacks.
#[must_use]
pub fn refusal(exposures: &Exposures) -> String {
    let runs = exposures.runs.iter().map(|b| match b.exposure {
        Exposure::Unknown { pid: Some(pid) } => format!(
            "Run {} ended without a settlement and its process {pid} is gone: billing unknown",
            b.invocation
        ),
        Exposure::Unknown { pid: None } => format!(
            "Run {} ended without a settlement: billing unknown",
            b.invocation
        ),
        Exposure::Uncertain => format!(
            "Run {} settled with a sent request whose charge is unknown",
            b.invocation
        ),
        Exposure::Unjudged => format!(
            "Run {} was admitted and never settled, and this host cannot judge its process",
            b.invocation
        ),
    });
    let torn = exposures.torn.iter().map(|sha256| {
        format!(
            "a row was cut mid-write (sha256 {sha256}): the Run that wrote it may have been billed"
        )
    });
    let named: Vec<String> = runs.chain(torn).collect();
    format!(
        "an earlier unknown-cost Run may have been billed: {} · no automatic retry: a new unknown-cost Run in this project waits until that exposure is reconciled (evidence: .nika/{JOURNAL})",
        named.join(" · ")
    )
}

/// Append one row after terminating a torn tail, so a row a killed writer left
/// half-written never fuses with the next one: its bytes stay, on their own
/// line, and the fold names them.
///
/// # Errors
/// The journal cannot be read or appended safely.
pub fn append_row(nika: &OwnedDir, row: &str) -> std::io::Result<()> {
    if torn_tail(nika)? {
        nika.append_line(JOURNAL, "")?;
    }
    nika.append_line(JOURNAL, row)
}

fn torn_tail(nika: &OwnedDir) -> std::io::Result<bool> {
    let mut file = match nika.open_relative(Path::new(JOURNAL)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        other => other?,
    };
    if file.metadata()?.len() == 0 {
        return Ok(false);
    }
    file.seek(std::io::SeekFrom::End(-1))?;
    let mut last = [0_u8; 1];
    file.read_exact(&mut last)?;
    Ok(last[0] != b'\n')
}

fn read(nika: &OwnedDir) -> std::io::Result<String> {
    let file = match nika.open_relative(Path::new(JOURNAL)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        other => other?,
    };
    let mut text = String::new();
    file.take(1_048_577).read_to_string(&mut text)?;
    if text.len() > 1_048_576 {
        return Err(invalid("cost observation journal exceeds the read bound"));
    }
    Ok(text)
}

fn invalid(reason: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, reason)
}

type Latest<'a> = BTreeMap<String, (serde_json::Value, &'a str)>;

/// The latest row of each invocation with its exact line, strictly read, plus
/// the digest of every line a writer cut mid-write (not JSON at all). A JSON
/// row this engine cannot read makes prior exposure unknown.
fn latest(text: &str) -> std::io::Result<(Latest<'_>, Vec<String>)> {
    let mut latest = BTreeMap::new();
    let mut torn = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
            torn.push(nika_event::source_id::sha256_hex(line.as_bytes()));
            continue;
        };
        let id = row["invocation"]
            .as_str()
            .ok_or_else(|| invalid("unreadable cost invocation"))?
            .to_owned();
        if row["schema"] != "nika/run-cost-observation@1" {
            return Err(invalid("unrecognized cost observation"));
        }
        let observation = &row["observation"];
        let phase_ok = match row["phase"].as_str() {
            Some("prepared" | "settled") => true,
            Some("unknown") => row["unsettled"]["prior_sha256"].is_string(),
            _ => false,
        };
        if !phase_ok
            || observation["schema"] != "nika/inference-cost-observation@1"
            || observation["known_subtotal_nano_usd"]
                .as_str()
                .and_then(|v| v.parse::<i128>().ok())
                .is_none()
            || observation["unknown_calls"].as_u64().is_none()
            || !matches!(
                observation["state"].as_str(),
                Some("Open" | "Closed" | "Uncertain")
            )
        {
            return Err(invalid(
                "unreadable cost observation; prior exposure is unknown",
            ));
        }
        latest.insert(id, (row, line));
    }
    Ok((latest, torn))
}

/// The derived row: the last observation verbatim (the account's own words),
/// why it is unknown (`interrupted`, the resident's word for lost ownership),
/// the writer the lease named, the digest of the exact row it was derived from
/// and the review that observed it — evidence a later reconciliation cites.
fn unknown_row(
    invocation: &str,
    row: &serde_json::Value,
    line: &str,
    observer: &str,
) -> serde_json::Value {
    serde_json::json!({
        "schema": "nika/run-cost-observation@1",
        "invocation": invocation,
        "phase": "unknown",
        "observation": row["observation"],
        "unsettled": {
            "cause": "interrupted",
            "writer": row["lease"],
            "prior_sha256": nika_event::source_id::sha256_hex(line.as_bytes()),
            "observed_by": observer,
        },
    })
}

#[cfg(test)]
mod tests;
