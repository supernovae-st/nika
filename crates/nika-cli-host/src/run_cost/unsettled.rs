// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The ONE reader of the cost observation journal. It folds every invocation
//! to its latest row and names each earlier Run that still blocks a new
//! unknown-cost Run. Under the held lease it also records the durable UNKNOWN
//! of an invocation whose leased writer ended without settling: a process
//! killed mid-dispatch runs no handler, so the restart derives it from the
//! `prepared` row and appends it, never rewriting the rows it read.
use nika_fs::OwnedDir;
use std::collections::BTreeMap;
use std::io::{Read as _, Seek as _};
use std::path::Path;

pub(super) const JOURNAL: &str = "inference-cost-observations.ndjson";

/// Why an earlier invocation still blocks a new unknown-cost Run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Exposure {
    /// It settled, and a request it sent ended without usable settlement.
    Uncertain,
    /// Its leased writer ended without settling: recorded as unknown.
    Unknown { pid: Option<u64> },
    /// Admitted and never settled, with no lease this host can judge (a row
    /// written before the lease existed, or on another host).
    Unjudged,
}

/// One earlier invocation that still blocks, named by its own identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Blocker {
    pub(super) invocation: String,
    pub(super) exposure: Exposure,
}

/// What the journal still holds against a new unknown-cost Run: the Runs it
/// names, and the rows a killed writer cut mid-write (by the digest of their
/// exact bytes, since no identity survives the cut).
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Exposures {
    pub(super) runs: Vec<Blocker>,
    pub(super) torn: Vec<String>,
}

impl Exposures {
    pub(super) fn is_clear(&self) -> bool {
        self.runs.is_empty() && self.torn.is_empty()
    }
}

/// Fold the journal while holding the lease: append the UNKNOWN row of every
/// leased invocation that never settled, then return every exposure.
///
/// # Errors
/// An unreadable, oversized or unrecognized journal (prior exposure is
/// unknown), or an UNKNOWN row that cannot be appended.
pub(super) fn fold(nika: &OwnedDir, host: &str, observer: &str) -> Result<Exposures, String> {
    let text = read(nika)?;
    let (latest, torn) = latest(&text)?;
    let mut exposures = Exposures {
        runs: Vec::new(),
        torn,
    };
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
                append_row(nika, &derived.to_string())
                    .map_err(|e| format!("cannot record an unsettled Run as unknown: {e}"))?;
                Exposure::Unknown {
                    pid: row["lease"]["pid"].as_u64(),
                }
            }
            _ => Exposure::Unjudged,
        };
        exposures.runs.push(Blocker {
            invocation,
            exposure,
        });
    }
    Ok(exposures)
}

/// Every exposure, named by identity, without promising a gesture this binary lacks.
pub(super) fn refusal(exposures: &Exposures) -> String {
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
pub(super) fn append_row(nika: &OwnedDir, row: &str) -> std::io::Result<()> {
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

fn read(nika: &OwnedDir) -> Result<String, String> {
    let file = match nika.open_relative(Path::new(JOURNAL)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(e.to_string()),
        Ok(file) => file,
    };
    let mut text = String::new();
    file.take(1_048_577)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > 1_048_576 {
        return Err("cost observation journal exceeds the read bound".into());
    }
    Ok(text)
}

type Latest<'a> = BTreeMap<String, (serde_json::Value, &'a str)>;

/// The latest row of each invocation with its exact line, strictly read, plus
/// the digest of every line a writer cut mid-write (not JSON at all). A JSON
/// row this engine cannot read makes prior exposure unknown.
fn latest(text: &str) -> Result<(Latest<'_>, Vec<String>), String> {
    let mut latest = BTreeMap::new();
    let mut torn = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
            torn.push(nika_event::source_id::sha256_hex(line.as_bytes()));
            continue;
        };
        let id = row["invocation"]
            .as_str()
            .ok_or("unreadable cost invocation")?
            .to_owned();
        if row["schema"] != "nika/run-cost-observation@1" {
            return Err("unrecognized cost observation".into());
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
            return Err("unreadable cost observation; prior exposure is unknown".into());
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
