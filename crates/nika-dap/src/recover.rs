// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE tolerant NDJSON trace reader — every static consumer
//! (`--resume` · `trace show` · the store scan · the forecast gather ·
//! the DAP replayer) parses through THIS fold: a torn tail keeps its
//! valid prefix, a dead FIRST line refuses. Descended from the run
//! verb's resume module (2026-07-09 · the nika-dap split) so the
//! forensics plane has one home; nika-cli re-exports it at the old
//! path (zero call-site churn).

use nika_event::Event;

/// A recovered NDJSON trace — the valid prefix + an optional truncation
/// note (a crashed run leaves a half-written last line; recovering the
/// prefix is the whole point of a flight recorder).
#[derive(Debug)]
#[non_exhaustive]
pub struct RecoveredTrace {
    /// The parsed events, in journal order.
    pub events: Vec<Event>,
    /// Present when the tail was truncated/corrupt — a human diagnostic
    /// (the caller routes it to stderr).
    pub truncated_note: Option<String>,
}

impl RecoveredTrace {
    /// Assemble a recovered trace (invariant #19: every
    /// `#[non_exhaustive]` struct constructs through `new`, never a
    /// literal — the field set stays free to grow).
    #[must_use]
    pub fn new(events: Vec<Event>, truncated_note: Option<String>) -> Self {
        Self {
            events,
            truncated_note,
        }
    }
}

/// Why a trace could not be recovered AT ALL — the valid-prefix fold
/// found nothing to keep. A torn tail is NOT this error: it rides
/// [`RecoveredTrace::truncated_note`] beside the recovered prefix.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RecoverError {
    /// The FIRST non-blank line is not a valid event — not a journal.
    #[error("{label}:{line}: bad event: {source}")]
    BadFirstLine {
        /// The caller's file label (path or display name).
        label: String,
        /// 1-based FILE line number.
        line: usize,
        /// The parse failure on that line.
        source: serde_json::Error,
    },
    /// No events at all.
    #[error("{label}: empty trace")]
    Empty {
        /// The caller's file label (path or display name).
        label: String,
    },
}

/// Parse an NDJSON trace, tolerating a truncated/corrupt TAIL. Stops at
/// the first bad line and returns the valid prefix; a bad FIRST line
/// (nothing recovered) or an empty trace is a genuinely unreadable
/// trace and errors.
///
/// # Errors
///
/// [`RecoverError`] — its `Display` names the file + line (the
/// environment class every consumer renders).
pub fn recover_events(raw: &str, label: &str) -> Result<RecoveredTrace, RecoverError> {
    let mut events = Vec::new();
    let mut truncated_note = None;
    for (lineno, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Event>(line) {
            Ok(event) => events.push(event),
            Err(e) => {
                // 0.116.0 writes a stdout settlement envelope (`run_settled`)
                // through the journal sink. It is not an `Event` (no `id`).
                // Skip it; a torn Event tail still truncates.
                if is_run_settlement(line) {
                    continue;
                }
                if events.is_empty() {
                    return Err(RecoverError::BadFirstLine {
                        label: label.to_owned(),
                        line: lineno + 1,
                        source: e,
                    });
                }
                truncated_note = Some(format!(
                    "{label}:{}: trace truncated ({e}) — recovered {} event(s)",
                    lineno + 1,
                    events.len()
                ));
                break;
            }
        }
    }
    if events.is_empty() {
        return Err(RecoverError::Empty {
            label: label.to_owned(),
        });
    }
    Ok(RecoveredTrace::new(events, truncated_note))
}

/// Load a journal file and tolerantly recover its events (the file
/// half of [`recover_events`] — the static trace readers share it).
///
/// # Errors
///
/// A reason string when the file cannot be read or the recovery
/// refuses (garbage from line one).
pub fn load_events(trace: &str) -> Result<Vec<Event>, String> {
    let raw = std::fs::read_to_string(trace) // seam-bypass-ok: L4 verb reading the journal it folds
        .map_err(|e| format!("cannot read {trace}: {e}"))?;
    let recovered = recover_events(&raw, trace).map_err(|e| e.to_string())?;
    Ok(recovered.events)
}

fn is_run_settlement(line: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return false;
    };
    value.get("kind").and_then(|k| k.as_str()) == Some("run_settled")
}

/// The first wire-code-shaped token in a failure detail (`NIKA-INFER-001`
/// · `DAG-003`) — uppercase segments joined by dashes, at least two
/// (descended from `verbs::trace::peek` 2026-07-21 · the 15k wall —
/// the autopsy's teach line asks this, never the journal's prose).
#[must_use]
pub fn first_wire_code(detail: &str) -> Option<&str> {
    detail
        .split([' ', '·', ':', '(', ')'])
        .map(str::trim)
        .find(|t| {
            t.len() >= 5
                && t.contains('-')
                && t.split('-').count() >= 2
                && t.split('-').all(|s| {
                    !s.is_empty()
                        && s.bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recover_events_keeps_the_valid_prefix_of_a_torn_trace() {
        // One valid journal line, hand-shaped on the wire dialect — the
        // test proves DESERIALIZATION, so it starts from raw JSON.
        let line = serde_json::json!({
            "id": {"uuid": "01912345-0000-7000-8000-000000000001"},
            "timestamp": 1000, "kind": "task_completed",
            "run": null, "correlation": null, "fields": []
        })
        .to_string();
        let raw = format!("{line}\n{line}\n{{\"id\":{{\"uuid\":\"torn");
        let recovered = recover_events(&raw, "t").expect("prefix recovers");
        assert_eq!(recovered.events.len(), 2);
        let note = recovered.truncated_note.expect("the tear is surfaced");
        assert!(
            note.starts_with("t:3:") && note.contains("recovered 2 event(s)"),
            "the note names the 1-based FILE line of the tear: {note}"
        );

        let err = recover_events("{not json\n", "t").expect_err("bad first line");
        assert!(
            err.to_string().starts_with("t:1: bad event:"),
            "the error names the 1-based FILE line: {err}"
        );
        assert!(recover_events("", "t").is_err(), "empty trace");
    }

    #[test]
    fn recover_events_skips_the_run_settled_envelope() {
        let event = serde_json::json!({
            "id": {"uuid": "01912345-0000-7000-8000-000000000001"},
            "timestamp": 1000, "kind": "workflow_paused",
            "run": null, "correlation": null, "fields": []
        })
        .to_string();
        let settlement = serde_json::json!({
            "kind": "run_settled",
            "status": "paused",
            "outputs": {},
            "chain": "abc"
        })
        .to_string();
        let raw = format!("{event}\n{settlement}\n{event}\n");
        let recovered = recover_events(&raw, "t").expect("settlement is not a tear");
        assert_eq!(recovered.events.len(), 2);
        assert!(recovered.truncated_note.is_none(), "{recovered:?}");
    }
}
