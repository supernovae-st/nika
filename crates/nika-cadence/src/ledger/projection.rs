// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::{
    ArmGeneration, DecisionKind, HistoryEntry, LastRecord, json_str, optional_link,
    render_execution_fields,
};

/// Render the byte-stable `last.json` projection.
#[must_use]
pub fn render_last(record: &LastRecord) -> String {
    let trace = record.trace.as_deref().map_or("null".to_owned(), json_str);
    let exit = record.exit.unwrap_or(0);
    let generation = record
        .generation
        .as_ref()
        .map_or("null".to_owned(), |value| json_str(value.as_str()));
    let execution = render_execution_fields(record.execution.as_ref());
    format!(
        "{{\"slot\":\"{}\",\"fired_at\":\"{}\",\"trace\":{trace},\"exit\":{exit},\"kind\":\"{}\",\"gen\":{generation}{execution}}}\n",
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
        execution: optional_link(&doc)?.into_option(),
    })
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
        execution: entry.execution.clone(),
    })
}
