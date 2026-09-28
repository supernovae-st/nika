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
    /// The running kernel's boot identity, where the platform proves one
    /// (Linux): every container on that kernel shares its lock table.
    boot: Option<String>,
}

impl Writer {
    /// This process, on this host and kernel.
    #[must_use]
    pub fn this_process() -> Self {
        Self {
            pid: std::process::id(),
            host: crate::liveness::host_name(),
            boot: crate::liveness::boot_id(),
        }
    }

    /// The `{"pid","host"}` record the lease and every row carry, with the
    /// kernel's `"boot"` identity when this platform proves one.
    #[must_use]
    pub fn json(&self) -> serde_json::Value {
        let mut record = serde_json::json!({"pid": self.pid, "host": self.host});
        if let Some(boot) = &self.boot {
            record["boot"] = serde_json::Value::String(boot.clone());
        }
        record
    }

    /// Whether the lease this writer holds can judge a recorded writer: the
    /// same nonempty hostname (the historical heuristic, never a proof of one
    /// machine), or the same boot identity (the same running kernel, whose
    /// lock this writer acquired). Absent or empty identities never match.
    fn judges(&self, lease: &serde_json::Value) -> bool {
        let host = !self.host.is_empty() && lease["host"].as_str() == Some(self.host.as_str());
        let boot = (self.boot.as_deref()).is_some_and(|boot| lease["boot"].as_str() == Some(boot));
        host || boot
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
    /// The file under `.nika/traces` that recorded this invocation's
    /// execution, when one does (`nika trace show` takes it).
    pub trace: Option<String>,
}

impl Blocker {
    /// One named blocker, with no trace located.
    #[must_use]
    pub fn new(invocation: String, exposure: Exposure) -> Self {
        Self {
            invocation,
            exposure,
            trace: None,
        }
    }
}

/// A row no legal transition admits: a settlement from a writer that did not
/// prepare the Run, one that comes after the Run's recorded unknown or
/// settlement, a Run with no prepared row, a second preparation, a
/// lease-less row once leases began, or a `prepared`/`settled` row whose
/// observation its account could never have written. It never settles its
/// Run; it blocks by itself until an append-only reconciliation contract
/// exists.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Conflict {
    /// The invocation the row names.
    pub invocation: String,
    /// The sha256 of the row's exact bytes.
    pub sha256: String,
    /// Why no legal transition admits it.
    pub reason: &'static str,
}

impl Conflict {
    /// One refused row.
    #[must_use]
    pub fn new(invocation: String, sha256: String, reason: &'static str) -> Self {
        Self {
            invocation,
            sha256,
            reason,
        }
    }
}

/// What the journal still holds against a new unknown-cost Run: the Runs it
/// names, the rows a killed writer cut mid-write (by the digest of their exact
/// bytes, since no identity survives the cut), and the rows it refuses.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Exposures {
    /// The blocking invocations, in invocation order.
    pub runs: Vec<Blocker>,
    /// The sha256 of every row cut mid-write.
    pub torn: Vec<String>,
    /// Every row no legal transition admits, in journal order.
    pub conflicts: Vec<Conflict>,
}

impl Exposures {
    /// The named Runs and the cut rows, with no conflicting row.
    #[must_use]
    pub fn new(runs: Vec<Blocker>, torn: Vec<String>) -> Self {
        Self {
            runs,
            torn,
            conflicts: Vec::new(),
        }
    }

    /// Nothing blocks a new unknown-cost Run.
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.runs.is_empty() && self.torn.is_empty() && self.conflicts.is_empty()
    }
}

/// [`fold_as`] for a lease holder named by its `host` alone, with no kernel
/// identity: writers are judged by hostname only, as before boot identities.
///
/// # Errors
/// As [`fold_as`].
pub fn fold(nika: &OwnedDir, host: &str, observer: &str) -> std::io::Result<Exposures> {
    let holder = Writer {
        pid: std::process::id(),
        host: host.to_owned(),
        boot: None,
    };
    fold_as(nika, &holder, observer)
}

/// Fold the journal while `holder` holds the lease: append the UNKNOWN row of
/// every leased invocation that never settled and whose writer the holder can
/// judge ([`Writer`]'s host or kernel), then return every exposure.
///
/// # Errors
/// An unreadable, oversized or unrecognized journal (`InvalidData`: prior
/// exposure is unknown), or an UNKNOWN row that cannot be appended.
pub fn fold_as(nika: &OwnedDir, holder: &Writer, observer: &str) -> std::io::Result<Exposures> {
    let bytes = read(nika)?;
    let (standings, torn, conflicts) = judge(&bytes)?;
    let mut exposures = Exposures::new(Vec::new(), torn);
    exposures.conflicts = conflicts;
    for (invocation, standing) in standings {
        let exposure = match standing {
            Standing::Settled(_, true) => Exposure::Uncertain,
            Standing::Settled(_, false) => continue,
            Standing::Unknown(pid, _) => Exposure::Unknown { pid },
            // `prepared`: the lease the holder acquired proves a leased writer
            // on its host or kernel is gone; anything else cannot be judged.
            Standing::Prepared(row, line) if holder.judges(&row["lease"]) => {
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
        let trace = trace_of(nika, &invocation);
        let mut blocker = Blocker::new(invocation, exposure);
        blocker.trace = trace;
        exposures.runs.push(blocker);
    }
    Ok(exposures)
}

/// The trace file that recorded an `exe-<uuid>` invocation: a store name of
/// that trace id whose first frame names the same execution. Diagnostic only
/// (any failure is `None`): the refusal hands it to `nika trace show`.
fn trace_of(nika: &OwnedDir, invocation: &str) -> Option<String> {
    let id = uuid::Uuid::parse_str(invocation.strip_prefix("exe-")?).ok()?;
    let traces = nika.open_below(&["traces"]).ok()?;
    let mut names = traces.names().ok()?;
    names.sort();
    names.into_iter().find(|name| {
        let mut first = Vec::new();
        crate::journal::may_name_trace(name, id)
            && traces
                .open_relative(Path::new(name))
                .and_then(|file| file.take(65_536).read_to_end(&mut first))
                .is_ok()
            && first.split(|b| *b == b'\n').next().is_some_and(|line| {
                serde_json::from_slice::<serde_json::Value>(line).is_ok_and(|frame| {
                    frame["execution"]["uuid"].as_str() == Some(&id.hyphenated().to_string())
                })
            })
    })
}

/// Every exposure, named by identity, without promising a gesture this binary lacks.
#[must_use]
pub fn refusal(exposures: &Exposures) -> String {
    // Each Run by its journal identity (escaped: journal text is never a
    // terminal control) and, when one recorded it, its trace. A writer is
    // only said to have let the lease go: the lease proves no more, and the
    // pid may be this very process (an earlier Run whose settlement failed).
    let runs = exposures.runs.iter().map(|b| {
        let run = match &b.trace {
            Some(trace) => format!(
                "Run {} (trace .nika/traces/{})",
                crate::escape_tty(&b.invocation),
                crate::escape_tty(trace)
            ),
            None => format!("Run {}", crate::escape_tty(&b.invocation)),
        };
        match b.exposure {
            Exposure::Unknown { pid: Some(pid) } => format!(
                "{run} ended without a settlement; its writer, process {pid}, no longer holds the cost lease: billing unknown"
            ),
            Exposure::Unknown { pid: None } => format!(
                "{run} ended without a settlement and its writer no longer holds the cost lease: billing unknown"
            ),
            Exposure::Uncertain => {
                format!("{run} settled with a sent request whose charge is unknown")
            }
            Exposure::Unjudged => format!(
                "{run} was admitted and never settled, and this host cannot judge its writer"
            ),
        }
    });
    let torn = exposures.torn.iter().map(|sha256| {
        format!(
            "a row was cut mid-write (sha256 {sha256}): the Run that wrote it may have been billed"
        )
    });
    // The invocation a refused row names is journal text: never a terminal control.
    let conflicts = exposures.conflicts.iter().map(|c| {
        format!(
            "a row for Run {} (sha256 {}) {}: refused as a conflict, the exposure before it stands",
            crate::escape_tty(&c.invocation),
            c.sha256,
            c.reason
        )
    });
    let named: Vec<String> = runs.chain(torn).chain(conflicts).collect();
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

/// The journal's bytes, never decoded whole: a writer cut inside a multi-byte
/// character leaves one torn line, not an unreadable journal.
fn read(nika: &OwnedDir) -> std::io::Result<Vec<u8>> {
    let file = match nika.open_relative(Path::new(JOURNAL)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        other => other?,
    };
    let mut bytes = Vec::new();
    file.take(1_048_577).read_to_end(&mut bytes)?;
    if bytes.len() > 1_048_576 {
        return Err(invalid("cost observation journal exceeds the read bound"));
    }
    Ok(bytes)
}

fn invalid(reason: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, reason)
}

/// Each non-blank line's exact bytes (a `\r` before the newline excluded, as
/// `str::lines` reads it), in journal order.
fn lines(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes
        .split(|b| *b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
}

/// One Run's standing, advanced only by a legal next row.
enum Standing<'a> {
    /// Admitted: the row and its exact bytes.
    Prepared(serde_json::Value, &'a [u8]),
    /// Settled by the writer that prepared it: the row's exact bytes, and
    /// whether a sent request's charge stayed unknown.
    Settled(&'a [u8], bool),
    /// Recorded unknown: the pid the lease named, and the row's exact bytes.
    Unknown(Option<u64>, &'a [u8]),
}

type Judged<'a> = (BTreeMap<String, Standing<'a>>, Vec<String>, Vec<Conflict>);

/// Every Run's standing under the legal transitions: `prepared`, then either
/// `settled` by the same lease writer or the `unknown` derived from that exact
/// row, and nothing after either but a byte-identical repeat. A row no
/// transition admits, or whose observation contradicts its own account, is a
/// conflict and changes no standing. Lines that are not UTF-8 or not JSON are
/// torn, named by digest; a JSON row this engine cannot read makes prior
/// exposure unknown.
fn judge(bytes: &[u8]) -> std::io::Result<Judged<'_>> {
    let (mut standings, mut torn, mut conflicts) = (BTreeMap::new(), Vec::new(), Vec::new());
    // A lease-less `prepared`/`settled` row is legacy evidence only while no
    // leased row came before it; after that no engine writes one.
    let mut leased = false;
    for line in lines(bytes) {
        let parsed = std::str::from_utf8(line).ok().map(serde_json::from_str);
        let Some(Ok::<serde_json::Value, _>(row)) = parsed else {
            torn.push(nika_event::source_id::sha256_hex(line));
            continue;
        };
        let id = strict(&row)?;
        let lease_less = row["phase"] != "unknown" && row["lease"].is_null();
        let next = if lease_less && leased {
            Err("carries no cost lease after leased rows began")
        } else {
            leased |= !row["lease"].is_null();
            // The transition law speaks first; a row it admits must also be
            // an observation its account could have written.
            let written = consistent(&row);
            advance(standings.get(&id), row, line).and_then(|next| written.map(|()| next))
        };
        match next {
            Ok(standing) => drop(standings.insert(id, standing)),
            Err(why) => conflicts.push(Conflict::new(
                id,
                nika_event::source_id::sha256_hex(line),
                why,
            )),
        }
    }
    Ok((standings, torn, conflicts))
}

/// The row's invocation, once its schema, phase and observation read strictly.
fn strict(row: &serde_json::Value) -> std::io::Result<String> {
    let id = row["invocation"]
        .as_str()
        .ok_or_else(|| invalid("unreadable cost invocation"))?
        .to_owned();
    if row["schema"] != "nika/run-cost-observation@1" {
        return Err(invalid("unrecognized cost observation"));
    }
    // A derived unknown carries the account's last words as `prior_observation`
    // (an earlier engine wrote them as `observation`): never a current state.
    let (observation, phase_ok) = match row["phase"].as_str() {
        Some("prepared" | "settled") => (&row["observation"], true),
        Some("unknown") => (
            row.get("prior_observation").unwrap_or(&row["observation"]),
            row["unsettled"]["prior_sha256"].is_string(),
        ),
        _ => (&row["observation"], false),
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
    Ok(id)
}

/// Whether a `prepared` or `settled` row's observation is one its account could
/// have written, or why not (nika-providers `InferenceReceipt::observation`, the
/// same serializer since the journal's first writer). The account counts as
/// unknown exactly the sent attempts it could not price, adds only nonnegative
/// estimates of sent attempts to its known subtotal, is untouched when the host
/// writes `prepared` (right after the review confirms the choice) and closed
/// before it writes `settled`. A derived `unknown` row carries prior history: it
/// is judged through the exact `prepared` row it names.
fn consistent(row: &serde_json::Value) -> Result<(), &'static str> {
    let prepared = match row["phase"].as_str() {
        Some("prepared") => true,
        Some("settled") => false,
        _ => return Ok(()),
    };
    let observation = &row["observation"];
    let (Some(priced), Some(unpriced)) = (
        observation["attempts"].as_array(),
        observation["unknown_attempts"].as_array(),
    ) else {
        return Err("records its account without both attempt lists");
    };
    let (mut unknown_calls, mut known) = (0_u64, 0_i128);
    for attempt in priced.iter().chain(unpriced) {
        match (attempt["sent"].as_bool(), &attempt["estimated_nano_usd"]) {
            (Some(true), serde_json::Value::Null) => unknown_calls += 1,
            (Some(false), serde_json::Value::Null) => {}
            (Some(true), serde_json::Value::String(nano)) => {
                let estimate = nano.parse::<i128>().ok().filter(|value| *value >= 0);
                known = estimate
                    .and_then(|value| known.checked_add(value))
                    .ok_or("records an attempt its account cannot write")?;
            }
            _ => return Err("records an attempt its account cannot write"),
        }
    }
    let subtotal = observation["known_subtotal_nano_usd"]
        .as_str()
        .and_then(|nano| nano.parse::<i128>().ok());
    match subtotal {
        Some(nano) if nano < 0 => return Err("reports a negative known subtotal"),
        Some(nano) if nano == known => {}
        _ => return Err("reports a known subtotal its attempts do not add up to"),
    }
    if observation["unknown_calls"].as_u64() != Some(unknown_calls) {
        return Err("counts unknown calls its sent attempts do not record");
    }
    let open = observation["state"] == "Open";
    if prepared && !(open && priced.is_empty() && unpriced.is_empty()) {
        return Err("prepares the Run with an account that already moved");
    }
    if !prepared && open {
        return Err("settles the Run with its account still open");
    }
    Ok(())
}

/// The Run's next standing, or why no legal transition admits the row.
fn advance<'a>(
    standing: Option<&Standing<'a>>,
    row: serde_json::Value,
    line: &'a [u8],
) -> Result<Standing<'a>, &'static str> {
    let phase = row["phase"].as_str().unwrap_or_default();
    match (standing, phase) {
        (None, "prepared") => Ok(Standing::Prepared(row, line)),
        (None, _) => Err("names a Run with no prepared row before it"),
        (Some(Standing::Prepared(prepared, _)), "settled") if row["lease"] == prepared["lease"] => {
            Ok(Standing::Settled(
                line,
                row["observation"]["state"] == "Uncertain",
            ))
        }
        (Some(Standing::Prepared(..)), "settled") => {
            Err("settles a Run from a writer that did not prepare it")
        }
        (Some(Standing::Prepared(prepared, prior)), "unknown")
            if row["unsettled"]["writer"] == prepared["lease"]
                && row["unsettled"]["prior_sha256"]
                    == nika_event::source_id::sha256_hex(prior).as_str() =>
        {
            Ok(Standing::Unknown(prepared["lease"]["pid"].as_u64(), line))
        }
        (Some(Standing::Prepared(..)), "unknown") => {
            Err("records unknown from a row other than the Run's prepared one")
        }
        (Some(Standing::Prepared(..)), _) => Err("prepares the Run a second time"),
        (Some(&Standing::Settled(settled, uncertain)), _) if settled == line => {
            Ok(Standing::Settled(settled, uncertain))
        }
        (Some(&Standing::Unknown(pid, recorded)), _) if recorded == line => {
            Ok(Standing::Unknown(pid, recorded))
        }
        (Some(Standing::Settled(..)), _) => Err("follows the Run's settlement"),
        (Some(Standing::Unknown(..)), _) => Err("follows the Run's recorded unknown"),
    }
}

/// The derived row: the last observation verbatim as `prior_observation` (the
/// account's own words when the Run was prepared: its `Open` state and zero
/// counters are history, never the Run's current state), why it is unknown
/// (`interrupted`, the resident's word for lost ownership), the writer the
/// lease named, the digest of the exact row it was derived from and the
/// review that observed it — evidence a later reconciliation cites.
fn unknown_row(
    invocation: &str,
    row: &serde_json::Value,
    line: &[u8],
    observer: &str,
) -> serde_json::Value {
    serde_json::json!({
        "schema": "nika/run-cost-observation@1",
        "invocation": invocation,
        "phase": "unknown",
        "prior_observation": row["observation"],
        "unsettled": {
            "cause": "interrupted",
            "writer": row["lease"],
            "prior_sha256": nika_event::source_id::sha256_hex(line),
            "observed_by": observer,
        },
    })
}

#[cfg(test)]
mod tests;
