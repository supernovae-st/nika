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

/// The only journal shapes the pure replay machine accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum JournalFormat {
    /// No evidence yet.
    Empty,
    /// The strict W2 decision shape, before hash chaining.
    Legacy,
    /// A valid `nika/arm-event@1` chain.
    Versioned,
}

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

fn replay_core<'a>(journals: impl IntoIterator<Item = (&'a str, bool)>) -> Option<Replay> {
    let mut walker = Walker::new();
    for (text, versioned) in journals {
        let format = classify_journal(text)?;
        if versioned != matches!(format, JournalFormat::Versioned) {
            return None;
        }
        if versioned {
            walker.fold_chain(text);
        } else if matches!(format, JournalFormat::Legacy) {
            walker.fold_legacy(text);
        }
    }
    Some(walker.finish())
}

/// Rebuild the last decision and watermark from journals, oldest first.
#[must_use]
pub fn replay_projection<'a>(
    journals: impl IntoIterator<Item = (&'a str, bool)>,
) -> Option<(Option<LastRecord>, Option<Timestamp>)> {
    let replayed = replay_core(journals)?;
    Some((replayed.last, replayed.watermark))
}

/// Fold the current lifecycle from journals and apply the crash deadline.
#[must_use]
pub fn replay_state<'a>(
    journals: impl IntoIterator<Item = (&'a str, bool)>,
    now: &Timestamp,
) -> Option<(FiringState, bool, Option<String>)> {
    let replayed = replay_core(journals)?;
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
pub fn unsettled(text: &str) -> Option<Vec<Unsettled>> {
    let mut claims: Vec<(usize, Unsettled)> = Vec::new();
    let mut receipts: Vec<(usize, SlotId, u64)> = Vec::new();
    let versioned = match classify_journal(text)? {
        JournalFormat::Empty => return Some(Vec::new()),
        JournalFormat::Legacy => false,
        JournalFormat::Versioned => true,
    };
    let mut seq = 0u64;
    let mut prev_hash = None;
    for (position, line) in text.lines().enumerate() {
        if versioned {
            let Some(hash) = verify_line(line, seq + 1, prev_hash.as_deref()) else {
                break;
            };
            seq += 1;
            prev_hash = Some(hash);
        }
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
    Some(
        claims
            .into_iter()
            .filter(|(position, claim)| {
                !receipts.iter().any(|(later, slot_id, fencing)| {
                    later > position && (slot_id, *fencing) == (&claim.slot_id, claim.seq)
                })
            })
            .map(|(_, claim)| claim)
            .collect(),
    )
}

/// Count skipped and fired decisions across journal texts.
#[must_use]
pub fn tallies<'a>(journals: impl IntoIterator<Item = (&'a str, bool)>) -> Option<(usize, usize)> {
    let mut skips = 0usize;
    let mut fires = 0usize;
    for (text, versioned) in journals {
        let format = classify_journal(text)?;
        if versioned != matches!(format, JournalFormat::Versioned) {
            return None;
        }
        let mut seq = 0u64;
        let mut prev_hash = None;
        for line in text.lines() {
            if versioned {
                let Some(hash) = verify_line(line, seq + 1, prev_hash.as_deref()) else {
                    break;
                };
                seq += 1;
                prev_hash = Some(hash);
            }
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
    Some((skips, fires))
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

/// Classify a journal dialect without accepting ambiguous JSON evidence.
///
/// `None` means the evidence is malformed. Any ledger-envelope key commits the
/// journal to the versioned dialect; a broken or unknown version can therefore
/// never fall back to W2 replay.
#[must_use]
pub fn classify_journal(text: &str) -> Option<JournalFormat> {
    if text.is_empty() {
        return Some(JournalFormat::Empty);
    }
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() || lines.iter().any(|line| line.is_empty()) {
        return None;
    }
    let first: serde_json::Value = serde_json::from_str(lines[0]).ok()?;
    if has_ledger_marker(&first) {
        return verify_line(lines[0], 1, None).map(|_| JournalFormat::Versioned);
    }
    let docs: Vec<serde_json::Value> = lines
        .iter()
        .map(|line| serde_json::from_str(line).ok())
        .collect::<Option<_>>()?;
    docs.iter()
        .all(legacy_line_valid)
        .then_some(JournalFormat::Legacy)
}

/// Whether the complete journal is a valid versioned chain.
#[must_use]
pub fn first_line_is_versioned(text: &str) -> bool {
    classify_journal(text) == Some(JournalFormat::Versioned)
}

fn has_ledger_marker(doc: &serde_json::Value) -> bool {
    const KEYS: [&str; 8] = [
        "schema",
        "v",
        "seq",
        "ts",
        "slot_id",
        "payload",
        "prev_hash",
        "hash",
    ];
    doc.as_object()
        .is_some_and(|object| KEYS.iter().any(|key| object.contains_key(*key)))
}

fn legacy_line_valid(doc: &serde_json::Value) -> bool {
    const KEYS: [&str; 7] = [
        "slot",
        "decided_at",
        "kind",
        "reason",
        "trace",
        "exit",
        "slots",
    ];
    let Some(object) = doc.as_object() else {
        return false;
    };
    if !exact_or_subset_keys(object, &KEYS)
        || doc
            .get("slot")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| value.parse::<Timestamp>().ok())
            .is_none()
        || doc
            .get("decided_at")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| value.parse::<Timestamp>().ok())
            .is_none()
        || !doc
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|kind| matches!(kind, "fired" | "skipped" | "paused" | "failed"))
    {
        return false;
    }
    nullable_string(doc.get("reason"))
        && nullable_string(doc.get("trace"))
        && nullable_u8(doc.get("exit"))
        && nullable_u32(doc.get("slots"))
}

/// Verify one exact ledger line and return its own hash.
#[must_use]
pub fn verify_line(line: &str, expected_seq: u64, expected_prev: Option<&str>) -> Option<String> {
    const ENVELOPE_KEYS: [&str; 9] = [
        "schema",
        "v",
        "seq",
        "ts",
        "kind",
        "slot_id",
        "payload",
        "prev_hash",
        "hash",
    ];
    let doc: serde_json::Value = serde_json::from_str(line).ok()?;
    if !exact_keys(doc.as_object()?, &ENVELOPE_KEYS) {
        return None;
    }
    if doc.get("schema")?.as_str()? != LEDGER_SCHEMA || doc.get("v")?.as_u64()? != 1 {
        return None;
    }
    if doc.get("seq")?.as_u64()? != expected_seq
        || doc.get("ts")?.as_str()?.parse::<Timestamp>().is_err()
    {
        return None;
    }
    let kind = doc.get("kind")?.as_str()?;
    if !matches!(
        kind,
        "rotated" | "claimed" | "fired" | "skipped" | "paused" | "failed" | "disarmed"
    ) {
        return None;
    }
    let slot_id = match doc.get("slot_id")? {
        serde_json::Value::Null => None,
        serde_json::Value::String(value) => Some(SlotId::from_wire(value)?),
        _ => return None,
    };
    verify_payload(kind, slot_id.as_ref(), doc.get("payload")?, expected_seq)?;
    let (prev_json, linked) = match doc.get("prev_hash")? {
        serde_json::Value::Null => ("null".to_owned(), expected_prev.is_none()),
        serde_json::Value::String(value) => (
            json_str(value),
            hash_is_canonical(value) && expected_prev == Some(value.as_str()),
        ),
        _ => return None,
    };
    if !linked {
        return None;
    }
    let hash = doc.get("hash")?.as_str()?;
    if !hash_is_canonical(hash) {
        return None;
    }
    let cut = line.rfind(",\"hash\":\"")?;
    let prefix = &line[..cut];
    let suffix = format!(",\"hash\":\"{hash}\"}}");
    if line[cut..] != suffix {
        return None;
    }
    (sha256_hex(format!("{prev_json}\n{prefix}").as_bytes()) == hash).then(|| hash.to_owned())
}

fn verify_payload(
    kind: &str,
    slot_id: Option<&SlotId>,
    payload: &serde_json::Value,
    seq: u64,
) -> Option<()> {
    let object = payload.as_object()?;
    match kind {
        "rotated" => {
            const KEYS: [&str; 4] = ["from", "lines", "archives", "archives_sha256"];
            let from = payload.get("from")?.as_str()?;
            (slot_id.is_none()
                && exact_keys(object, &KEYS)
                && archive_ordinal(from).is_some()
                && payload
                    .get("lines")?
                    .as_u64()
                    .is_some_and(|lines| lines > 0)
                && payload
                    .get("archives")?
                    .as_u64()
                    .is_some_and(|archives| archives > 0)
                && payload
                    .get("archives_sha256")?
                    .as_str()
                    .is_some_and(hash_is_canonical))
            .then_some(())
        }
        "claimed" => {
            const KEYS: [&str; 4] = ["attempt", "deadline", "fencing", "gen"];
            (slot_id.is_some()
                && exact_keys(object, &KEYS)
                && payload.get("attempt")?.as_u64() == Some(1)
                && payload
                    .get("deadline")?
                    .as_str()?
                    .parse::<Timestamp>()
                    .is_ok()
                && payload.get("fencing")?.as_u64() == Some(seq)
                && generation_valid(payload.get("gen")))
            .then_some(())
        }
        "fired" | "skipped" | "paused" | "failed" | "disarmed" => {
            const KEYS: [&str; 7] = ["slot", "reason", "trace", "exit", "slots", "fencing", "gen"];
            let slot = payload.get("slot")?;
            let semantic_slot = slot.is_string() || slot_id.is_some();
            let shape_valid = exact_keys(object, &KEYS)
                && timestamp_or_null(slot)
                && nullable_string(payload.get("reason"))
                && nullable_string(payload.get("trace"))
                && nullable_u8(payload.get("exit"))
                && nullable_u32(payload.get("slots"))
                && nullable_u64(payload.get("fencing"))
                && generation_valid(payload.get("gen"));
            (shape_valid && (kind != "disarmed" || !semantic_slot)).then_some(())
        }
        _ => None,
    }
}

fn exact_keys(object: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> bool {
    object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
}

fn exact_or_subset_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> bool {
    object.keys().all(|key| keys.contains(&key.as_str()))
}

fn nullable_string(value: Option<&serde_json::Value>) -> bool {
    value.is_none_or(|value| value.is_null() || value.is_string())
}

fn nullable_u8(value: Option<&serde_json::Value>) -> bool {
    value.is_none_or(|value| {
        value.is_null()
            || value
                .as_u64()
                .and_then(|number| u8::try_from(number).ok())
                .is_some()
    })
}

fn nullable_u32(value: Option<&serde_json::Value>) -> bool {
    value.is_none_or(|value| {
        value.is_null()
            || value
                .as_u64()
                .and_then(|number| u32::try_from(number).ok())
                .is_some()
    })
}

fn nullable_u64(value: Option<&serde_json::Value>) -> bool {
    value.is_none_or(|value| value.is_null() || value.as_u64().is_some())
}

fn timestamp_or_null(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::String(value) => value.parse::<Timestamp>().is_ok(),
        _ => false,
    }
}

fn generation_valid(value: Option<&serde_json::Value>) -> bool {
    value.is_some_and(|value| match value {
        serde_json::Value::Null => true,
        serde_json::Value::String(value) => ArmGeneration::from_wire(value).is_some(),
        _ => false,
    })
}

fn hash_is_canonical(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
) -> Option<(String, String)> {
    let rendered = unchecked_ledger_line(seq, ts, kind, slot_id, payload, prev_hash);
    (verify_line(&rendered.0, seq, prev_hash).as_deref() == Some(rendered.1.as_str()))
        .then_some(rendered)
}

/// Parse one canonical W2 archive name into its chronological ordinal.
#[must_use]
pub fn archive_ordinal(name: &str) -> Option<u32> {
    if name == "history-w2.ndjson" {
        return Some(1);
    }
    let suffix = name.strip_prefix("history-w2-")?.strip_suffix(".ndjson")?;
    let ordinal = suffix.parse::<u32>().ok()?;
    ((2..u32::MAX).contains(&ordinal) && suffix == ordinal.to_string()).then_some(ordinal)
}

/// Commit an ordered archive bundle by canonical name and exact bytes.
#[must_use]
fn archive_bundle_hash<'a>(archives: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut committed = String::from("nika/arm-archives@1\n");
    for (name, text) in archives {
        committed.push_str(&json_str(name));
        committed.push('\n');
        committed.push_str(&sha256_hex(text.as_bytes()));
        committed.push('\n');
    }
    sha256_hex(committed.as_bytes())
}

/// Render the exact W7 rotation payload for an ordered, non-empty archive bundle.
#[must_use]
pub fn rotation_payload(archives: &[(&str, &str)]) -> Option<String> {
    let (from, latest) = archives.last()?;
    let lines = latest.lines().count();
    if lines == 0 || archive_ordinal(from).is_none() {
        return None;
    }
    let digest = archive_bundle_hash(archives.iter().copied());
    Some(format!(
        "{{\"from\":{},\"lines\":{lines},\"archives\":{},\"archives_sha256\":{}}}",
        json_str(from),
        archives.len(),
        json_str(&digest)
    ))
}

/// Verify the W7 genesis commitment over an ordered W2 archive bundle.
#[must_use]
pub fn archive_commitment_matches(live: &str, archives: &[(&str, &str)]) -> bool {
    if classify_journal(live) != Some(JournalFormat::Versioned) {
        return false;
    }
    let Some(first) = live
        .lines()
        .next()
        .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok())
    else {
        return false;
    };
    if first.get("kind").and_then(serde_json::Value::as_str) != Some("rotated") {
        return archives.is_empty();
    }
    let Some(payload) = first.get("payload") else {
        return false;
    };
    let actual = archive_bundle_hash(archives.iter().copied());
    let latest = archives.last();
    payload.get("archives").and_then(serde_json::Value::as_u64)
        == u64::try_from(archives.len()).ok()
        && payload.get("from").and_then(serde_json::Value::as_str) == latest.map(|(name, _)| *name)
        && payload.get("lines").and_then(serde_json::Value::as_u64)
            == latest.and_then(|(_, text)| u64::try_from(text.lines().count()).ok())
        && payload
            .get("archives_sha256")
            .and_then(serde_json::Value::as_str)
            == Some(actual.as_str())
}

/// Verify a live chain against the exact durable `head.json` bytes.
#[must_use]
pub fn chain_anchor_matches(text: &str, anchor: Option<&str>) -> bool {
    let (verified_seq, _, _) = scan_chain(text);
    let Some(anchor) = anchor else {
        return verified_seq == 0;
    };
    let Some(doc) = serde_json::from_str::<serde_json::Value>(anchor).ok() else {
        return false;
    };
    let Some(object) = doc.as_object().filter(|object| object.len() == 3) else {
        return false;
    };
    if object.get("schema").and_then(serde_json::Value::as_str) != Some("nika/arm-head@1") {
        return false;
    }
    let Some(seq) = object.get("seq").and_then(serde_json::Value::as_u64) else {
        return false;
    };
    if seq == 0 {
        return object.get("hash").is_some_and(serde_json::Value::is_null);
    }
    let Some(hash) = object.get("hash").and_then(serde_json::Value::as_str) else {
        return false;
    };
    if seq > verified_seq || !hash_is_canonical(hash) {
        return false;
    }
    usize::try_from(seq)
        .ok()
        .and_then(|line| line.checked_sub(1))
        .and_then(|line| text.lines().nth(line))
        .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .is_some_and(|line| line.get("hash").and_then(serde_json::Value::as_str) == Some(hash))
}

/// Render canonical durable chain-head bytes when sequence and hash agree.
#[must_use]
pub fn render_chain_anchor(seq: u64, hash: Option<&str>) -> Option<String> {
    if (seq == 0 && hash.is_some()) || (seq > 0 && !hash.is_some_and(hash_is_canonical)) {
        return None;
    }
    let hash = hash.map_or("null".to_owned(), json_str);
    Some(format!(
        "{{\"schema\":\"nika/arm-head@1\",\"seq\":{seq},\"hash\":{hash}}}\n"
    ))
}

/// Verify one immutable filesystem snapshot before any projection consumes it.
#[must_use]
pub fn journal_snapshot_matches(anchor: Option<&str>, journals: &[(&str, &str, bool)]) -> bool {
    if journals.iter().filter(|(_, _, live)| *live).count() > 1 {
        return false;
    }
    let live = journals
        .iter()
        .find(|(_, _, live)| *live)
        .map_or("", |(_, text, _)| *text);
    if !chain_anchor_matches(live, anchor) {
        return false;
    }
    let format = classify_journal(live);
    let archives: Vec<(&str, &str)> = journals
        .iter()
        .filter(|(_, _, live)| !*live)
        .map(|(name, text, _)| (*name, *text))
        .collect();
    if format == Some(JournalFormat::Versioned) {
        if !archive_commitment_matches(live, &archives) {
            return false;
        }
    } else if !archives.is_empty() {
        return false;
    }
    journals.iter().all(|(_, text, live)| {
        classify_journal(text)
            .is_some_and(|format| *live || matches!(format, JournalFormat::Legacy))
    })
}

/// Render a durable W2 migration intent after validating its archive name.
#[must_use]
pub fn render_migration_intent(
    archive: &str,
    lines: usize,
    rotated_at: &Timestamp,
) -> Option<String> {
    archive_ordinal(archive)?;
    Some(format!(
        "{{\"archive\":{},\"lines\":{lines},\"rotated_at\":\"{rotated_at}\"}}\n",
        json_str(archive)
    ))
}

/// Parse the exact migration-intent shape.
#[must_use]
pub fn parse_migration_intent(text: &str) -> Option<(String, usize, Timestamp)> {
    const KEYS: [&str; 3] = ["archive", "lines", "rotated_at"];
    let doc: serde_json::Value = serde_json::from_str(text).ok()?;
    let object = doc.as_object()?;
    if !exact_keys(object, &KEYS) {
        return None;
    }
    let archive = doc.get("archive")?.as_str()?.to_owned();
    archive_ordinal(&archive)?;
    Some((
        archive,
        usize::try_from(doc.get("lines")?.as_u64()?).ok()?,
        doc.get("rotated_at")?.as_str()?.parse().ok()?,
    ))
}

fn unchecked_ledger_line(
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

/// Build the slot-bearing `last.json` projection from one decision.
#[must_use]
pub fn last_projection(entry: &HistoryEntry) -> Option<LastRecord> {
    Some(LastRecord {
        slot: entry.slot?,
        fired_at: entry.decided_at,
        trace: entry.trace.clone(),
        exit: entry.exit,
        kind: entry.kind,
        generation: entry.generation.clone(),
    })
}

/// Render the canonical claim payload whose sequence is its fencing token.
#[must_use]
pub fn claim_payload(claim: &Claim, seq: u64) -> Option<String> {
    if seq == 0 {
        return None;
    }
    let generation = claim
        .generation
        .as_ref()
        .map_or("null".to_owned(), |value| json_str(value.as_str()));
    Some(format!(
        "{{\"attempt\":1,\"deadline\":\"{}\",\"fencing\":{seq},\"gen\":{generation}}}",
        claim.deadline
    ))
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
#[path = "ledger/tests.rs"]
mod tests;
