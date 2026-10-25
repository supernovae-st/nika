// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The journal a settled job's artifacts come from. It is located by the job's receipt under
//! the backend's journal directory, read ONCE within the verifier's bound, judged over those
//! very bytes by the one verifier `nika trace verify` runs, then parsed from the same bytes:
//! no second read can slip other content between the verdict and the events. Only a journal
//! whose chain is intact, whose seal verifies against this machine's run keys and whose head
//! is the receipt's `chain_head` yields events (blocking · the fs).

use std::path::{Path, PathBuf};

use nika_event::Event;
use nika_trace::trace_verify::{JOURNAL_BOUND, VerifyOptions, verify_captured};
use serde_json::Value;

use crate::JobReceipt;

/// The tiers a sealed, intact journal attains (the CLI's words).
const SEALED_TIERS: [&str; 3] = ["sealed", "anchored", "replayed"];

/// Why a settled job's journal yields no artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refusal {
    /// No journal to read: the receipt names no chain head, or no journal by its identity exists.
    Unavailable,
    /// A journal exists but is not the sealed, intact one the receipt names.
    Unverified,
}

/// A journal the receipt's head names, judged sealed: its attained tier and its events.
#[derive(Debug)]
pub(super) struct Held {
    /// `sealed` · `anchored` · `replayed`.
    pub(super) verdict: &'static str,
    /// The journal's events, in order, spilled values read back.
    pub(super) events: Vec<Event>,
}

/// Locate, read, judge and parse the journal `receipt` names under `dir`; `key` is a run
/// public key weighed before the custody defaults (`None` in production: the resident's own
/// custody, as `nika trace verify` resolves it).
pub(super) fn hold(
    dir: &Path,
    receipt: &JobReceipt,
    key: Option<PathBuf>,
) -> Result<Held, Refusal> {
    let head = receipt.chain_head().ok_or(Refusal::Unavailable)?;
    let path = nika_dap::store::locate_trace(dir, receipt.execution_id(), receipt.trace_id())
        .ok_or(Refusal::Unavailable)?;
    let bound = u64::try_from(JOURNAL_BOUND).unwrap_or(u64::MAX);
    // Over the bound, not UTF-8 or a link in place of the file: not a journal this resident
    // wrote whole, so nothing in it is recorded.
    let raw = match nika_fs::read_owned(&path, bound) {
        Ok(Some(raw)) => raw,
        Ok(None) => return Err(Refusal::Unavailable),
        Err(_) => return Err(Refusal::Unverified),
    };
    let label = path.to_string_lossy();
    let opts = VerifyOptions {
        key,
        json: true,
        sealed: true,
        ..VerifyOptions::default()
    };
    let judged = verify_captured(&label, &raw, &opts);
    let verdict = sealed_tier(judged.code, &judged.text, head).ok_or(Refusal::Unverified)?;
    let mut recovered =
        nika_dap::recover::recover_events(&raw, &label).map_err(|_| Refusal::Unverified)?;
    if recovered.truncated_note.is_some() {
        return Err(Refusal::Unverified);
    }
    let kept = nika_dap::spill::rehydrate(&mut recovered.events, &path);
    if !kept.unresolved.is_empty() {
        return Err(Refusal::Unverified);
    }
    Ok(Held {
        verdict,
        events: recovered.events,
    })
}

/// The tier the verifier's `--json` document attains when it holds every leg: exit 0, an
/// intact chain whose head is `head`, a seal that verifies. The headline word alone is not
/// enough (a forged seal still reads `ok`, an anchor gap still reads `sealed`), so each leg
/// is read.
pub(super) fn sealed_tier(code: u8, document: &str, head: &str) -> Option<&'static str> {
    let document: Value = serde_json::from_str(document.trim_end()).ok()?;
    let holds = code == 0
        && document["exit"] == 0
        && document["chain"]["headline"] == "intact"
        && document["seal"]["tier"] == "sealed"
        && document["chain"]["head"].as_str() == Some(head);
    if !holds {
        return None;
    }
    SEALED_TIERS
        .into_iter()
        .find(|tier| document["tier"] == *tier)
}
