// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure vocabulary and fold for the durable arm ledger.
//!
//! This module knows JSON bytes and firing semantics, never paths, locks, or
//! files. The CLI adapter supplies journal texts oldest-first and owns every
//! effect. Keeping the wire judge beside the firing machine makes replay
//! available to every future firer without depending upward on `nika-cli`.

use jiff::Timestamp;

use crate::firing::{
    self, ArmGeneration, FencingToken, FiringEvent, FiringState, SkipReason, SlotId,
};

/// The versioned ledger schema tag.
pub const LEDGER_SCHEMA: &str = "nika/arm-event@1";

/// The decision vocabulary shared by projections, receipts, and output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionKind {
    /// The run exited cleanly.
    Fired,
    /// Policy consumed the slot without running it.
    Skipped,
    /// A human gate parked the run.
    Paused,
    /// The run or its file failed.
    Failed,
    /// The OS unit was torn down; history-only and slot-less.
    Disarmed,
}

impl DecisionKind {
    /// The stable wire word.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fired => "fired",
            Self::Skipped => "skipped",
            Self::Paused => "paused",
            Self::Failed => "failed",
            Self::Disarmed => "disarmed",
        }
    }

    /// Parse a projection word. `disarmed` never belongs in `last.json`.
    #[must_use]
    pub fn parse_projection(word: &str) -> Option<Self> {
        match word {
            "fired" => Some(Self::Fired),
            "skipped" => Some(Self::Skipped),
            "paused" => Some(Self::Paused),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// One decision to append to the ledger.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// The decided slot, absent only for pre-slot decisions.
    pub slot: Option<Timestamp>,
    /// The decision instant.
    pub decided_at: Timestamp,
    /// The decision kind.
    pub kind: DecisionKind,
    /// Optional skip reason.
    pub reason: Option<String>,
    /// Optional trace path.
    pub trace: Option<String>,
    /// Optional process exit code.
    pub exit: Option<u8>,
    /// Number of slots answered by one catch-up fire.
    pub slots: Option<u32>,
    /// Stable slot identity.
    pub slot_id: Option<SlotId>,
    /// Fencing token of the claim this receipt settles.
    pub fencing: Option<FencingToken>,
    /// Generation pinned by the firing.
    pub generation: Option<ArmGeneration>,
}

/// A durable pre-run claim.
#[derive(Debug, Clone)]
pub struct Claim {
    /// Stable slot identity.
    pub slot_id: SlotId,
    /// Generation pinned by this claim.
    pub generation: Option<ArmGeneration>,
    /// Instant after which an unreceipted claim becomes ambiguous.
    pub deadline: Timestamp,
    /// Claim instant.
    pub decided_at: Timestamp,
}

/// Result of one durable append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordOutcome {
    /// Appended sequence number.
    pub seq: u64,
    /// Invalid tail lines removed before the append.
    pub repaired: u64,
}

/// A claim for which no later receipt exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsettled {
    /// Claim sequence and fencing token.
    pub seq: u64,
    /// Stable slot identity.
    pub slot_id: SlotId,
    /// Crash-detector deadline.
    pub deadline: Timestamp,
    /// Claim instant.
    pub claimed_at: Timestamp,
}

/// The stable `last.json` projection.
#[derive(Debug, Clone)]
pub struct LastRecord {
    /// Last slot-bearing decision.
    pub slot: Timestamp,
    /// Decision instant.
    pub fired_at: Timestamp,
    /// Optional trace path.
    pub trace: Option<String>,
    /// Optional exit code.
    pub exit: Option<u8>,
    /// Decision kind.
    pub kind: DecisionKind,
    /// Optional pinned generation.
    pub generation: Option<ArmGeneration>,
}

struct Replay {
    last: Option<LastRecord>,
    watermark: Option<Timestamp>,
    lifecycle: Vec<FiringEvent>,
    lifecycle_beyond_last: bool,
    lifecycle_slot: Option<String>,
}

fn replay_core<'a>(journals: impl IntoIterator<Item = (&'a str, bool)>) -> Replay {
    let mut walker = Walker::new();
    for (text, versioned) in journals {
        if versioned {
            walker.fold_chain(text);
        } else {
            walker.fold_legacy(text);
        }
    }
    walker.finish()
}

/// Rebuild the last decision and watermark from journals, oldest first.
#[must_use]
pub fn replay_projection<'a>(
    journals: impl IntoIterator<Item = (&'a str, bool)>,
) -> (Option<LastRecord>, Option<Timestamp>) {
    let replayed = replay_core(journals);
    (replayed.last, replayed.watermark)
}

/// Fold the current lifecycle from journals and apply the crash deadline.
#[must_use]
pub fn replay_state<'a>(
    journals: impl IntoIterator<Item = (&'a str, bool)>,
    now: &Timestamp,
) -> Option<(FiringState, bool, Option<String>)> {
    let replayed = replay_core(journals);
    fold_replay(&replayed, now)
}

fn fold_replay(replayed: &Replay, now: &Timestamp) -> Option<(FiringState, bool, Option<String>)> {
    if replayed.lifecycle.is_empty() {
        return None;
    }
    let mut state = firing::fold(&replayed.lifecycle);
    if matches!(state, FiringState::Claimed | FiringState::Running)
        && let Some((fencing, deadline)) = last_claim(&replayed.lifecycle)
        && *now > deadline
    {
        state = firing::transition(state, &FiringEvent::DeadlinePassed { fencing });
    }
    Some((
        state,
        replayed.lifecycle_beyond_last,
        replayed.lifecycle_slot.clone(),
    ))
}

/// Find every claim that has no matching later receipt.
#[must_use]
pub fn unsettled(text: &str) -> Vec<Unsettled> {
    let mut claims: Vec<(usize, Unsettled)> = Vec::new();
    let mut receipts: Vec<(usize, SlotId, u64)> = Vec::new();
    for (position, line) in text.lines().enumerate() {
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if doc.get("kind").and_then(serde_json::Value::as_str) == Some("claimed") {
            let (Some(seq), Some(slot_id), Some(deadline), Some(claimed_at)) = (
                doc.get("seq").and_then(serde_json::Value::as_u64),
                doc.get("slot_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(SlotId::from_wire),
                doc.get("payload")
                    .and_then(|payload| payload.get("deadline"))
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| value.parse::<Timestamp>().ok()),
                doc.get("ts")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| value.parse::<Timestamp>().ok()),
            ) else {
                continue;
            };
            claims.push((
                position,
                Unsettled {
                    seq,
                    slot_id,
                    deadline,
                    claimed_at,
                },
            ));
            continue;
        }
        let receipt = (
            doc.get("slot_id")
                .and_then(serde_json::Value::as_str)
                .and_then(SlotId::from_wire),
            doc.get("payload")
                .and_then(|payload| payload.get("fencing"))
                .and_then(serde_json::Value::as_u64),
        );
        if let (Some(slot_id), Some(fencing)) = receipt {
            receipts.push((position, slot_id, fencing));
        }
    }
    claims
        .into_iter()
        .filter(|(position, claim)| {
            !receipts.iter().any(|(later, slot_id, fencing)| {
                later > position && (slot_id, *fencing) == (&claim.slot_id, claim.seq)
            })
        })
        .map(|(_, claim)| claim)
        .collect()
}

/// Count skipped and fired decisions across journal texts.
#[must_use]
pub fn tallies<'a>(journals: impl IntoIterator<Item = &'a str>) -> (usize, usize) {
    let mut skips = 0usize;
    let mut fires = 0usize;
    for text in journals {
        for line in text.lines() {
            let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            match doc.get("kind").and_then(serde_json::Value::as_str) {
                Some("skipped") => skips += 1,
                Some("fired") => fires += 1,
                _ => {}
            }
        }
    }
    (skips, fires)
}

fn last_claim(events: &[FiringEvent]) -> Option<(FencingToken, Timestamp)> {
    events.iter().rev().find_map(|event| match event {
        FiringEvent::Claimed {
            fencing, deadline, ..
        } => Some((*fencing, *deadline)),
        _ => None,
    })
}

struct Group {
    key: Option<String>,
    events: Vec<FiringEvent>,
}

struct Walker {
    last: Option<LastRecord>,
    watermark: Option<Timestamp>,
    groups: Vec<Group>,
    current: Option<usize>,
    last_projection: Option<usize>,
}

impl Walker {
    fn new() -> Self {
        Self {
            last: None,
            watermark: None,
            groups: Vec::new(),
            current: None,
            last_projection: None,
        }
    }

    fn fold_chain(&mut self, text: &str) {
        let mut seq = 0u64;
        let mut prev: Option<String> = None;
        for line in text.lines() {
            match verify_line(line, seq + 1, prev.as_deref()) {
                Some(hash) => {
                    seq += 1;
                    prev = Some(hash);
                    if let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) {
                        self.fold_versioned(&doc);
                    }
                }
                None => break,
            }
        }
    }

    fn fold_legacy(&mut self, text: &str) {
        for line in text.lines() {
            if let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) {
                self.fold_legacy_line(&doc);
            }
        }
    }

    fn fold_versioned(&mut self, doc: &serde_json::Value) {
        let Some(kind) = doc.get("kind").and_then(serde_json::Value::as_str) else {
            return;
        };
        match kind {
            "claimed" => {
                if let Some(event) = claim_event(doc) {
                    let key = doc
                        .get("slot_id")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned);
                    self.push_event(key, event);
                }
            }
            "fired" | "skipped" | "paused" | "failed" => {
                if let Some(decided) = envelope_ts(doc) {
                    self.watermark = Some(decided);
                }
                let payload = doc.get("payload");
                let slot = payload
                    .and_then(|p| p.get("slot"))
                    .and_then(serde_json::Value::as_str)
                    .and_then(|s| s.parse::<Timestamp>().ok());
                let key = doc
                    .get("slot_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                if slot.is_none() && key.is_none() {
                    return;
                }
                let group = self.push_event(key, receipt_event(kind, payload));
                if let (Some(slot), Some(decided)) = (slot, envelope_ts(doc)) {
                    self.last = Some(LastRecord {
                        slot,
                        fired_at: decided,
                        trace: payload
                            .and_then(|p| p.get("trace"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned),
                        exit: payload
                            .and_then(|p| p.get("exit"))
                            .and_then(serde_json::Value::as_u64)
                            .and_then(|e| u8::try_from(e).ok()),
                        kind: decision_kind(kind),
                        generation: payload
                            .and_then(|p| p.get("gen"))
                            .and_then(serde_json::Value::as_str)
                            .and_then(ArmGeneration::from_wire),
                    });
                    self.last_projection = Some(group);
                }
            }
            "disarmed" => self.watermark = envelope_ts(doc).or(self.watermark),
            _ => {}
        }
    }

    fn fold_legacy_line(&mut self, doc: &serde_json::Value) {
        let Some(kind) = doc.get("kind").and_then(serde_json::Value::as_str) else {
            return;
        };
        if !matches!(kind, "fired" | "skipped" | "paused" | "failed") {
            return;
        }
        let decided = doc
            .get("decided_at")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| s.parse::<Timestamp>().ok());
        if decided.is_some() {
            self.watermark = decided;
        }
        let slot = doc
            .get("slot")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| s.parse::<Timestamp>().ok());
        let exit = doc
            .get("exit")
            .and_then(serde_json::Value::as_u64)
            .and_then(|e| u8::try_from(e).ok());
        let Some(slot) = slot else { return };
        let event = if kind == "skipped" {
            FiringEvent::Skipped {
                reason: doc
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .and_then(SkipReason::parse),
            }
        } else {
            FiringEvent::Finished {
                fencing: None,
                code: exit.unwrap_or(0),
            }
        };
        let group = self.push_event(None, event);
        if let Some(decided) = decided {
            self.last = Some(LastRecord {
                slot,
                fired_at: decided,
                trace: doc
                    .get("trace")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                exit,
                kind: decision_kind(kind),
                generation: None,
            });
            self.last_projection = Some(group);
        }
    }

    fn push_event(&mut self, key: Option<String>, event: FiringEvent) -> usize {
        let index = match key.as_deref() {
            Some(identity) => self
                .groups
                .iter()
                .position(|group| group.key.as_deref() == Some(identity))
                .unwrap_or_else(|| self.open_group(key)),
            None => self.open_group(None),
        };
        self.groups[index].events.push(event);
        self.current = Some(index);
        index
    }

    fn open_group(&mut self, key: Option<String>) -> usize {
        self.groups.push(Group {
            key,
            events: Vec::new(),
        });
        self.groups.len() - 1
    }

    fn finish(self) -> Replay {
        let (lifecycle, lifecycle_slot) = match self.current {
            Some(index) => (
                self.groups[index].events.clone(),
                self.groups[index].key.clone(),
            ),
            None => (Vec::new(), None),
        };
        Replay {
            last: self.last,
            watermark: self.watermark,
            lifecycle,
            lifecycle_beyond_last: self.current.is_some() && self.current != self.last_projection,
            lifecycle_slot,
        }
    }
}

fn envelope_ts(doc: &serde_json::Value) -> Option<Timestamp> {
    doc.get("ts")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse::<Timestamp>().ok())
}

fn claim_event(doc: &serde_json::Value) -> Option<FiringEvent> {
    let payload = doc.get("payload")?;
    let fencing = payload.get("fencing").and_then(serde_json::Value::as_u64)?;
    let deadline = payload
        .get("deadline")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse::<Timestamp>().ok())?;
    let generation = payload
        .get("gen")
        .and_then(serde_json::Value::as_str)
        .and_then(ArmGeneration::from_wire);
    Some(FiringEvent::Claimed {
        fencing: FencingToken::new(fencing),
        generation,
        deadline,
    })
}

fn receipt_event(kind: &str, payload: Option<&serde_json::Value>) -> FiringEvent {
    if kind == "skipped" {
        return FiringEvent::Skipped {
            reason: payload
                .and_then(|p| p.get("reason"))
                .and_then(serde_json::Value::as_str)
                .and_then(SkipReason::parse),
        };
    }
    let code = payload
        .and_then(|p| p.get("exit"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|e| u8::try_from(e).ok())
        .unwrap_or(0);
    let fencing = payload
        .and_then(|p| p.get("fencing"))
        .and_then(serde_json::Value::as_u64)
        .map(FencingToken::new);
    FiringEvent::Finished { fencing, code }
}

fn decision_kind(word: &str) -> DecisionKind {
    match word {
        "fired" => DecisionKind::Fired,
        "skipped" => DecisionKind::Skipped,
        "paused" => DecisionKind::Paused,
        _ => DecisionKind::Failed,
    }
}

/// Whether the first line declares the versioned ledger schema.
#[must_use]
pub fn first_line_is_versioned(text: &str) -> bool {
    text.lines()
        .next()
        .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .and_then(|doc| {
            doc.get("schema")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .is_some_and(|schema| schema == LEDGER_SCHEMA)
}

/// Verify one exact ledger line and return its own hash.
#[must_use]
pub fn verify_line(line: &str, expected_seq: u64, expected_prev: Option<&str>) -> Option<String> {
    let doc: serde_json::Value = serde_json::from_str(line).ok()?;
    if doc.get("schema")?.as_str()? != LEDGER_SCHEMA || doc.get("v")?.as_u64()? != 1 {
        return None;
    }
    if doc.get("seq")?.as_u64()? != expected_seq
        || doc.get("ts")?.as_str()?.parse::<Timestamp>().is_err()
    {
        return None;
    }
    doc.get("kind")?.as_str()?;
    doc.get("payload")?.as_object()?;
    if !matches!(
        doc.get("slot_id")?,
        serde_json::Value::Null | serde_json::Value::String(_)
    ) {
        return None;
    }
    let (prev_json, linked) = match doc.get("prev_hash")? {
        serde_json::Value::Null => ("null".to_owned(), expected_prev.is_none()),
        serde_json::Value::String(value) => {
            (json_str(value), expected_prev == Some(value.as_str()))
        }
        _ => return None,
    };
    if !linked {
        return None;
    }
    let hash = doc.get("hash")?.as_str()?;
    let cut = line.rfind(",\"hash\":\"")?;
    let prefix = &line[..cut];
    (sha256_hex(format!("{prev_json}\n{prefix}").as_bytes()) == hash).then(|| hash.to_owned())
}

/// Verify a journal until its first invalid line.
///
/// Returns `(last sequence, last hash, valid prefix line count)`.
#[must_use]
pub fn scan_chain(text: &str) -> (u64, Option<String>, usize) {
    let mut seq = 0u64;
    let mut prev_hash = None;
    let mut valid_lines = 0usize;
    for line in text.lines() {
        match verify_line(line, seq + 1, prev_hash.as_deref()) {
            Some(hash) => {
                seq += 1;
                prev_hash = Some(hash);
                valid_lines += 1;
            }
            None => break,
        }
    }
    (seq, prev_hash, valid_lines)
}

/// Render a canonical ledger line and return the line plus its hash.
#[must_use]
pub fn ledger_line(
    seq: u64,
    ts: Timestamp,
    kind: &str,
    slot_id: Option<&str>,
    payload: &str,
    prev_hash: Option<&str>,
) -> (String, String) {
    let kind_json = json_str(kind);
    let slot_json = slot_id.map_or("null".to_owned(), json_str);
    let prev_json = prev_hash.map_or("null".to_owned(), json_str);
    let prefix = format!(
        "{{\"schema\":\"{LEDGER_SCHEMA}\",\"v\":1,\"seq\":{seq},\"ts\":\"{ts}\",\"kind\":{kind_json},\"slot_id\":{slot_json},\"payload\":{payload},\"prev_hash\":{prev_json}"
    );
    let hash = sha256_hex(format!("{prev_json}\n{prefix}").as_bytes());
    (format!("{prefix},\"hash\":\"{hash}\"}}"), hash)
}

/// Render a JSON string literal without a fallible serializer edge.
#[must_use]
pub fn json_str(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('"');
    for ch in raw.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            value if value.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", u32::from(value));
            }
            value => out.push(value),
        }
    }
    out.push('"');
    out
}

/// Render the byte-stable `last.json` projection.
#[must_use]
pub fn render_last(record: &LastRecord) -> String {
    let trace = record.trace.as_deref().map_or("null".to_owned(), json_str);
    let exit = record.exit.unwrap_or(0);
    let generation = record
        .generation
        .as_ref()
        .map_or("null".to_owned(), |value| json_str(value.as_str()));
    format!(
        "{{\"slot\":\"{}\",\"fired_at\":\"{}\",\"trace\":{trace},\"exit\":{exit},\"kind\":\"{}\",\"gen\":{generation}}}\n",
        record.slot,
        record.fired_at,
        record.kind.as_str()
    )
}

/// Parse the byte-stable `last.json` projection.
#[must_use]
pub fn parse_last(text: &str) -> Option<LastRecord> {
    let doc: serde_json::Value = serde_json::from_str(text).ok()?;
    Some(LastRecord {
        slot: doc.get("slot")?.as_str()?.parse().ok()?,
        fired_at: doc.get("fired_at")?.as_str()?.parse().ok()?,
        trace: doc
            .get("trace")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        exit: doc
            .get("exit")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u8::try_from(value).ok()),
        kind: DecisionKind::parse_projection(doc.get("kind")?.as_str()?)?,
        generation: doc
            .get("gen")
            .and_then(serde_json::Value::as_str)
            .and_then(ArmGeneration::from_wire),
    })
}

/// Render the decision payload inside a versioned ledger envelope.
#[must_use]
pub fn decision_payload(entry: &HistoryEntry) -> String {
    let slot = entry
        .slot
        .map_or("null".to_owned(), |value| format!("\"{value}\""));
    let reason = entry.reason.as_deref().map_or("null".to_owned(), json_str);
    let trace = entry.trace.as_deref().map_or("null".to_owned(), json_str);
    let exit = entry
        .exit
        .map_or("null".to_owned(), |value| value.to_string());
    let slots = entry
        .slots
        .map_or("null".to_owned(), |value| value.to_string());
    let fencing = entry
        .fencing
        .map_or("null".to_owned(), |value| value.get().to_string());
    let generation = entry
        .generation
        .as_ref()
        .map_or("null".to_owned(), |value| json_str(value.as_str()));
    format!(
        "{{\"slot\":{slot},\"reason\":{reason},\"trace\":{trace},\"exit\":{exit},\"slots\":{slots},\"fencing\":{fencing},\"gen\":{generation}}}"
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn ts(value: &str) -> Timestamp {
        value.parse().expect("timestamp")
    }

    fn line(
        seq: u64,
        kind: &str,
        slot: Option<&str>,
        payload: &str,
        prev: Option<&str>,
    ) -> (String, String) {
        line_at(seq, "2026-08-19T03:02:00Z", kind, slot, payload, prev)
    }

    fn line_at(
        seq: u64,
        at: &str,
        kind: &str,
        slot: Option<&str>,
        payload: &str,
        prev: Option<&str>,
    ) -> (String, String) {
        ledger_line(seq, ts(at), kind, slot, payload, prev)
    }

    #[test]
    fn canonical_lines_verify_and_one_changed_byte_refuses() {
        let (first, hash) = line(
            1,
            "fired",
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            r#"{"slot":"2026-08-19T03:00:00Z","exit":0}"#,
            None,
        );
        assert_eq!(verify_line(&first, 1, None), Some(hash));
        assert!(verify_line(&first.replace("exit\":0", "exit\":1"), 1, None).is_none());
        assert_eq!(scan_chain(&format!("{first}\nbroken\n")).2, 1);
    }

    #[test]
    fn projection_vocabulary_is_exact() {
        for (word, expected) in [
            ("fired", DecisionKind::Fired),
            ("skipped", DecisionKind::Skipped),
            ("paused", DecisionKind::Paused),
            ("failed", DecisionKind::Failed),
        ] {
            assert_eq!(DecisionKind::parse_projection(word), Some(expected));
            assert_eq!(expected.as_str(), word);
        }
        assert_eq!(DecisionKind::parse_projection("disarmed"), None);
        assert_eq!(DecisionKind::parse_projection("unknown"), None);
    }

    #[test]
    fn schema_and_line_guards_refuse_each_independent_mismatch() {
        let (valid, hash) = line(1, "fired", None, r#"{"slot":null}"#, None);
        assert!(first_line_is_versioned(&valid));
        assert!(!first_line_is_versioned(""));
        assert!(!first_line_is_versioned(r#"{"schema":"nika/arm-event@2"}"#));
        assert_eq!(verify_line(&valid, 1, None), Some(hash));
        assert!(verify_line(&valid.replace(LEDGER_SCHEMA, "nika/arm-event@2"), 1, None).is_none());
        assert!(verify_line(&valid.replace("\"v\":1", "\"v\":2"), 1, None).is_none());
        assert!(verify_line(&valid, 2, None).is_none());
        assert!(
            verify_line(
                &valid.replace("2026-08-19T03:02:00Z", "not-a-time"),
                1,
                None
            )
            .is_none()
        );

        let wrong_schema_prefix = concat!(
            r#"{"schema":"nika/arm-event@2","v":1,"seq":1,"#,
            r#""ts":"2026-08-19T03:02:00Z","kind":"fired","slot_id":null,"#,
            r#""payload":{"slot":null},"prev_hash":null"#
        );
        let wrong_schema_hash = sha256_hex(format!("null\n{wrong_schema_prefix}").as_bytes());
        let wrong_schema = format!(r#"{wrong_schema_prefix},"hash":"{wrong_schema_hash}"}}"#);
        assert!(verify_line(&wrong_schema, 1, None).is_none());
    }

    #[test]
    fn scan_chain_reports_the_exact_prefix_identity() {
        let (first, first_hash) = line(1, "fired", None, r#"{"slot":null}"#, None);
        let (second, second_hash) = line(2, "skipped", None, r#"{"slot":null}"#, Some(&first_hash));
        let text = format!("{first}\n{second}\nbroken\n");
        assert_eq!(scan_chain(&text), (2, Some(second_hash), 2));
    }

    #[test]
    fn json_and_decision_payload_are_canonical() {
        assert_eq!(json_str("plain"), r#""plain""#);
        assert_eq!(json_str("a\"b\\c\n"), "\"a\\\"b\\\\c\\u000a\"");

        let entry = HistoryEntry {
            slot: Some(ts("2026-08-19T03:00:00Z")),
            decided_at: ts("2026-08-19T03:02:00Z"),
            kind: DecisionKind::Fired,
            reason: Some("quoted \"reason\"".to_owned()),
            trace: Some("trace\\path".to_owned()),
            exit: Some(7),
            slots: Some(2),
            slot_id: None,
            fencing: Some(FencingToken::new(9)),
            generation: ArmGeneration::from_wire(&"a".repeat(64)),
        };
        assert_eq!(
            decision_payload(&entry),
            r#"{"slot":"2026-08-19T03:00:00Z","reason":"quoted \"reason\"","trace":"trace\\path","exit":7,"slots":2,"fencing":9,"gen":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#
        );
    }

    #[test]
    fn versioned_claim_and_receipt_replay_one_lifecycle() {
        let slot = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let (claim, hash) = line(
            1,
            "claimed",
            Some(slot),
            r#"{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
            None,
        );
        let (receipt, _) = line(
            2,
            "fired",
            Some(slot),
            r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":0,"fencing":1,"gen":null}"#,
            Some(&hash),
        );
        let text = format!("{claim}\n{receipt}\n");
        let replayed = replay_core([(&*text, true)]);
        assert_eq!(
            replayed.last.as_ref().expect("last").kind,
            DecisionKind::Fired
        );
        assert_eq!(
            fold_replay(&replayed, &ts("2026-08-21T03:00:00Z"))
                .expect("fold")
                .0,
            FiringState::Succeeded
        );
        assert!(unsettled(&text).is_empty());
    }

    #[test]
    fn public_replay_returns_projection_watermark_and_fold_context() {
        let slot = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let (claim, hash) = line_at(
            1,
            "2026-08-19T03:01:00Z",
            "claimed",
            Some(slot),
            r#"{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
            None,
        );
        let (receipt, _) = line_at(
            2,
            "2026-08-19T03:02:00Z",
            "fired",
            Some(slot),
            r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":0,"fencing":1,"gen":null}"#,
            Some(&hash),
        );
        let text = format!("{claim}\n{receipt}\n");
        let (last, watermark) = replay_projection([(&*text, true)]);
        assert_eq!(last.expect("last").kind, DecisionKind::Fired);
        assert_eq!(watermark, Some(ts("2026-08-19T03:02:00Z")));
        let (state, beyond_last, lifecycle_slot) =
            replay_state([(&*text, true)], &ts("2026-08-19T03:03:00Z")).expect("state");
        assert_eq!(state, FiringState::Succeeded);
        assert!(!beyond_last);
        assert_eq!(lifecycle_slot.as_deref(), Some(slot));
    }

    #[test]
    fn replay_keeps_interleaved_slot_groups_separate() {
        let slot_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let slot_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let (claim_a, hash_a) = line(
            1,
            "claimed",
            Some(slot_a),
            r#"{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
            None,
        );
        let (claim_b, hash_b) = line(
            2,
            "claimed",
            Some(slot_b),
            r#"{"deadline":"2026-08-20T03:00:00Z","fencing":2,"gen":null}"#,
            Some(&hash_a),
        );
        let (receipt_b, _) = line(
            3,
            "fired",
            Some(slot_b),
            r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":0,"fencing":2,"gen":null}"#,
            Some(&hash_b),
        );
        let text = format!("{claim_a}\n{claim_b}\n{receipt_b}\n");
        let (state, beyond_last, lifecycle_slot) =
            replay_state([(&*text, true)], &ts("2026-08-19T03:03:00Z")).expect("state");
        assert_eq!(state, FiringState::Succeeded);
        assert!(!beyond_last);
        assert_eq!(lifecycle_slot.as_deref(), Some(slot_b));
    }

    #[test]
    fn a_new_claim_after_the_projection_is_reported_as_beyond_last() {
        let slot_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let slot_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let (receipt, hash) = line(
            1,
            "fired",
            Some(slot_a),
            r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":0,"fencing":null,"gen":null}"#,
            None,
        );
        let (claim, _) = line(
            2,
            "claimed",
            Some(slot_b),
            r#"{"deadline":"2026-08-20T03:00:00Z","fencing":2,"gen":null}"#,
            Some(&hash),
        );
        let text = format!("{receipt}\n{claim}\n");
        let (state, beyond_last, lifecycle_slot) =
            replay_state([(&*text, true)], &ts("2026-08-19T03:03:00Z")).expect("state");
        assert_eq!(state, FiringState::Claimed);
        assert!(beyond_last);
        assert_eq!(lifecycle_slot.as_deref(), Some(slot_b));
    }

    #[test]
    fn orphan_claim_crosses_only_the_open_deadline_boundary() {
        let slot = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let (claim, _) = line(
            1,
            "claimed",
            Some(slot),
            r#"{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
            None,
        );
        let replayed = replay_core([(&*claim, true)]);
        assert_eq!(unsettled(&claim).len(), 1);
        assert_eq!(
            fold_replay(&replayed, &ts("2026-08-20T03:00:00Z"))
                .expect("fold")
                .0,
            FiringState::Claimed
        );
        assert_eq!(
            fold_replay(&replayed, &ts("2026-08-20T03:00:00.000000001Z"))
                .expect("fold")
                .0,
            FiringState::Ambiguous
        );
    }

    #[test]
    fn unsettled_requires_a_later_receipt_with_both_identities() {
        let slot_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let slot_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let claim = format!(
            r#"{{"seq":1,"ts":"2026-08-19T03:00:00Z","kind":"claimed","slot_id":"{slot_a}","payload":{{"deadline":"2026-08-20T03:00:00Z","fencing":1}}}}"#
        );
        let earlier_receipt =
            format!(r#"{{"kind":"fired","slot_id":"{slot_a}","payload":{{"fencing":1}}}}"#);
        let wrong_slot =
            format!(r#"{{"kind":"fired","slot_id":"{slot_b}","payload":{{"fencing":1}}}}"#);
        let wrong_fence =
            format!(r#"{{"kind":"fired","slot_id":"{slot_a}","payload":{{"fencing":2}}}}"#);
        assert_eq!(unsettled(&format!("{earlier_receipt}\n{claim}\n")).len(), 1);
        assert_eq!(unsettled(&format!("{claim}\n{wrong_slot}\n")).len(), 1);
        assert_eq!(unsettled(&format!("{claim}\n{wrong_fence}\n")).len(), 1);
        assert!(unsettled(&format!("{claim}\n{earlier_receipt}\n")).is_empty());
    }

    #[test]
    fn tallies_count_each_decision_across_journals() {
        let first = concat!(
            "{\"kind\":\"skipped\"}\n",
            "{\"kind\":\"fired\"}\n",
            "{\"kind\":\"ignored\"}\n"
        );
        let second = "{\"kind\":\"fired\"}\nnot-json\n";
        assert_eq!(tallies([first, second]), (1, 2));
    }

    #[test]
    fn slotless_disarm_advances_only_the_watermark() {
        let (disarmed, _) = line_at(1, "2026-08-19T04:00:00Z", "disarmed", None, r"{}", None);
        let (last, watermark) = replay_projection([(&*disarmed, true)]);
        assert!(last.is_none());
        assert_eq!(watermark, Some(ts("2026-08-19T04:00:00Z")));
        assert!(envelope_ts(&serde_json::json!({"ts": "not-a-time"})).is_none());
        assert!(envelope_ts(&serde_json::json!({})).is_none());
    }

    #[test]
    fn versioned_direct_receipt_without_slot_id_still_projects() {
        let (receipt, _) = line_at(
            1,
            "2026-08-19T03:02:00Z",
            "paused",
            None,
            r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":4,"fencing":null,"gen":null}"#,
            None,
        );
        let (last, watermark) = replay_projection([(&*receipt, true)]);
        let last = last.expect("projection");
        assert_eq!(last.kind, DecisionKind::Paused);
        assert_eq!(watermark, Some(ts("2026-08-19T03:02:00Z")));
        let (state, _, lifecycle_slot) =
            replay_state([(&*receipt, true)], &ts("2026-08-19T03:03:00Z")).expect("state");
        assert_eq!(state, FiringState::Cancelled);
        assert!(lifecycle_slot.is_none());
    }

    #[test]
    fn legacy_replay_and_projection_round_trip_stay_byte_stable() {
        let legacy = r#"{"slot":"2026-08-19T03:00:00Z","decided_at":"2026-08-19T03:02:00Z","kind":"skipped","reason":"overlap","exit":0}"#;
        let replayed = replay_core([(legacy, false)]);
        let last = replayed.last.expect("last");
        let rendered = render_last(&last);
        let parsed = parse_last(&rendered).expect("projection");
        assert_eq!(parsed.kind, DecisionKind::Skipped);
        assert_eq!(parsed.slot, last.slot);
        assert_eq!(tallies([legacy]), (1, 0));
        assert_eq!(
            replay_state([(legacy, false)], &ts("2026-08-19T03:03:00Z"))
                .expect("state")
                .0,
            FiringState::Skipped
        );
    }
}
