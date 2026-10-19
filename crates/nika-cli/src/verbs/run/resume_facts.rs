// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Fold the continued run's facts from its already judged prologue (spec 04/17).
//! These are value facts, not execution, cost or approval authority. Legacy traces
//! carrying none keep the composer's existing caller/default behavior.

use nika_event::{Event, EventKind};
use nika_runtime::{MissedSlots, RunFacts, ScheduledForSource};
use nika_types::resource::Value as FieldValue;
use nika_types::timestamp::Timestamp;

/// Read the first manifest, like the existing project and run-identity folds.
/// A partial or malformed recorded fact refuses instead of silently changing it.
pub(super) fn from_trace(events: &[Event]) -> Result<Option<RunFacts>, String> {
    let Some(started) = events
        .iter()
        .find(|event| event.kind == EventKind::WorkflowStarted)
    else {
        return Ok(None);
    };
    let at = field(started, "scheduled_for")?;
    let source = field(started, "scheduled_for_source")?;
    let missed = field(started, "missed")?;
    if at.is_none() && source.is_none() && missed.is_none() {
        return Ok(None);
    }
    let at = instant(
        at.ok_or_else(|| malformed("scheduled_for"))?,
        "scheduled_for",
    )?;
    let source = match source.ok_or_else(|| malformed("scheduled_for_source"))? {
        "slot" => ScheduledForSource::Slot,
        "at" => ScheduledForSource::At,
        "start" => ScheduledForSource::Start,
        _ => return Err(malformed("scheduled_for_source")),
    };
    let mut facts = RunFacts::default();
    facts.scheduled_for = Some((at, source));
    facts.missed = missed.map(silence).transpose()?;
    Ok(Some(facts))
}

fn malformed(field: &str) -> String {
    format!("recorded run facts contain a missing, duplicate or malformed `{field}`")
}

fn field<'a>(event: &'a Event, key: &str) -> Result<Option<&'a str>, String> {
    let mut values = event.fields.iter().filter(|entry| entry.key == key);
    let Some(entry) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(malformed(key));
    }
    match &entry.value {
        FieldValue::String(value) => Ok(Some(value)),
        _ => Err(malformed(key)),
    }
}

/// Use the same Jiff instant parser and checked run-clock conversion as `test --at`.
fn instant(text: &str, key: &str) -> Result<Timestamp, String> {
    let at = text
        .parse::<jiff::Timestamp>()
        .map_err(|_| malformed(key))?;
    i64::try_from(at.as_nanosecond())
        .map(Timestamp::from_unix_ns)
        .map_err(|_| malformed(key))
}

fn silence(text: &str) -> Result<MissedSlots, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| malformed("missed"))?;
    let first = value
        .get("first")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| malformed("missed.first"))?;
    let last = value
        .get("last")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| malformed("missed.last"))?;
    let count = value
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .and_then(|count| u32::try_from(count).ok())
        .ok_or_else(|| malformed("missed.count"))?;
    Ok(MissedSlots::new(
        instant(first, "missed.first")?,
        instant(last, "missed.last")?,
        count,
    ))
}

#[cfg(test)]
#[path = "resume_facts/tests.rs"]
mod tests;
