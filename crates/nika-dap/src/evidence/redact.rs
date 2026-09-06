// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The redacted projection — the evidence pack's DEFAULT class (T9).
//! The SLSA/in-toto posture applied to the run journal: an attestation
//! carries the payload's HASH, never the payload. The bundle an
//! auditor takes offline proves the run's INTEGRITY (chain · head ·
//! seal — all computed over the ORIGINAL bytes the operator keeps);
//! the CONTENT (what the model answered · what a tool returned · what
//! a file read) is a separate, operator-side disclosure. Two
//! different asks — the pack says so, never conflates them.
//!
//! The walk is per line, three rules:
//!
//! - a line WITHOUT a payload field rides BYTE-VERBATIM — a
//!   zero-payload journal projects to its exact bytes, and its chain
//!   still re-walks (the untouched-line law);
//! - a line carrying one is re-serialized with each payload VALUE
//!   replaced by its placeholder: sha256 over the field's own bytes +
//!   the reason in `unavailable` (the pack's honest-null pattern,
//!   turned on content). Structural fields — kind · ids · timestamps ·
//!   hashes · chain links · durations · counts · error classes — are
//!   never touched, so the auditor keeps the run's full SHAPE and the
//!   seal line stays verbatim;
//! - a line that does not parse (the torn tail of a crash mid-write)
//!   becomes ONE marker object carrying its sha256 — a half-written
//!   payload never crosses either.
//!
//! The `chain` fields stay the ORIGINAL ones by design: they attest
//! the linkage of the bytes the operator holds. They do NOT re-walk
//! over the projection (the placeholders change the hashed bytes —
//! that is the construction, not a tamper). VERIFY.md says so plainly,
//! and `trace.projection_sha256` gives the auditor the one offline
//! check the projection itself supports.

use std::borrow::Cow;

use serde_json::{Value, json};

use nika_event::source_id::sha256_hex;

/// The payload keys — the journal fields whose VALUE can carry run
/// content. Grounded in the runtime's own secrets seam
/// (`nika-runtime/src/secret.rs`'s `PAYLOAD_FIELDS`: the terminal
/// frame's `outcome`/`output` and the failure `detail`), plus the
/// display contract's streamed `delta` (`infer_chunk`) and the pause
/// frame's shown content (`message` · `choices` — the prompt's
/// templates resolve task output; only secrets are marker-masked).
/// Two more since the V9 sensitivity pass (T9-F04): a fan-out
/// terminal's `items` (every item's `message` · the same failure text
/// `detail` carries, one row per item) and the success frame's
/// `warning` (a diagnostic that can quote the model's blank answer or
/// a path). Sorted: the manifest's `redaction.fields` lists them
/// verbatim.
pub(crate) const PAYLOAD_KEYS: [&str; 8] = [
    "choices", "delta", "detail", "items", "message", "outcome", "output", "warning",
];

/// The marker key naming an unparseable tail line in the projection —
/// NOT `kind`: the line is not an event, it never parsed.
const TAIL_MARKER: &str = "unparseable_tail";

/// The tail marker's `unavailable` reason.
const TAIL_REASON: &str = "the torn final line (a crash mid-write) never parses — its bytes stay with the operator's own journal (VERIFY.md §3)";

/// One redaction pass's result: the projection bytes + how many
/// placeholders were minted (payload fields + an unparseable tail).
pub(crate) struct Redaction {
    /// `journal.ndjson`'s content on the redacted class.
    pub(crate) projection: String,
    /// Payload fields replaced + the tail marker, when minted.
    pub(crate) placeholders: usize,
}

/// The `unavailable` reason one placeholder carries — the pack's
/// honest-null pattern class, per content kind.
fn reason_for(key: &str) -> &'static str {
    match key {
        "output" | "outcome" => {
            "redacted — the task payload (model output · tool result · file content) stays with the operator's own journal; this pack proves integrity, not content (VERIFY.md §3)"
        }
        "detail" => {
            "redacted — the failure detail can embed payload text; it stays with the operator's own journal (VERIFY.md §3)"
        }
        "delta" => {
            "redacted — a streamed model chunk stays with the operator's own journal (VERIFY.md §3)"
        }
        "message" | "choices" => {
            "redacted — the shown prompt can embed resolved task output; it stays with the operator's own journal (VERIFY.md §3)"
        }
        "items" => {
            "redacted — a fan-out's per-item terminals carry each item's failure text; they stay with the operator's own journal (VERIFY.md §3)"
        }
        "warning" => {
            "redacted — a non-fatal diagnostic can quote model output or a path; it stays with the operator's own journal (VERIFY.md §3)"
        }
        _ => "redacted — this payload field stays with the operator's own journal (VERIFY.md §3)",
    }
}

/// sha256 over the field's OWN bytes: the string verbatim · the
/// compact JSON for a non-string (payload fields are strings by
/// construction — the fallback keeps the walk total). The disclosure
/// check hashes the same bytes (`VERIFY.md` §3 shows the command).
fn field_digest(value: &Value) -> String {
    match value {
        Value::String(s) => sha256_hex(s.as_bytes()),
        other => sha256_hex(other.to_string().as_bytes()),
    }
}

/// Redact one line: byte-verbatim when it carries no payload field,
/// re-serialized with placeholders when it does. The trailing `\r` of
/// a CRLF survivor rides back where it was.
fn redact_line(line: &str) -> (Cow<'_, str>, usize) {
    if line.trim().is_empty() {
        return (Cow::Borrowed(line), 0);
    }
    let (body, cr) = line.strip_suffix('\r').map_or((line, ""), |b| (b, "\r"));
    let Ok(mut v) = serde_json::from_str::<Value>(body) else {
        let marker = json!({
            TAIL_MARKER: true,
            "sha256": sha256_hex(body.as_bytes()),
            "unavailable": TAIL_REASON,
        });
        return (Cow::Owned(format!("{marker}{cr}")), 1);
    };
    let Some(fields) = v.get_mut("fields").and_then(Value::as_array_mut) else {
        return (Cow::Borrowed(line), 0); // not an event shape — nothing to cut
    };
    let mut n = 0;
    for entry in fields {
        let Some(key) = entry.get("key").and_then(Value::as_str) else {
            continue;
        };
        if !PAYLOAD_KEYS.contains(&key) {
            continue;
        }
        let placeholder = json!({
            "sha256": field_digest(entry.get("value").unwrap_or(&Value::Null)),
            "unavailable": reason_for(key),
        });
        if let Some(obj) = entry.as_object_mut() {
            obj.insert("value".to_owned(), placeholder);
            n += 1;
        }
    }
    if n == 0 {
        (Cow::Borrowed(line), 0)
    } else {
        (Cow::Owned(format!("{v}{cr}")), n)
    }
}

/// Project the whole journal: same line count, same order, the
/// newline shape preserved byte for byte (a torn tail has no trailing
/// newline; the projection adds none either).
pub(crate) fn redact_journal(raw: &str) -> Redaction {
    let mut projection = String::with_capacity(raw.len());
    let mut placeholders = 0;
    let mut rest = raw;
    loop {
        if let Some(idx) = rest.find('\n') {
            let (line, n) = redact_line(&rest[..idx]);
            projection.push_str(&line);
            projection.push('\n');
            placeholders += n;
            rest = &rest[idx + 1..];
        } else {
            if !rest.is_empty() {
                let (line, n) = redact_line(rest);
                projection.push_str(&line);
                placeholders += n;
            }
            break;
        }
    }
    Redaction {
        projection,
        placeholders,
    }
}
