// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `declassify:` receipt evidence (F-O1 PR-3 · NEP-0004 law 5 · the
//! only door through the permit re-gate) — split out of `task.rs` under
//! the ADR-023 1,500-LOC ceiling: one row per declared door, the digest
//! read from the live scopes at dispatch time.

use std::collections::BTreeMap;

use nika_schema::raw::RawTask;
use serde_json::Value;

use crate::record::TaskRecord;

/// One `declassify:` entry's receipt evidence (NEP-0004 law 5 · the only
/// door through the permit re-gate): the raised binding, the author's
/// justification, and the digest of the value the door admitted (when
/// the binding resolves at dispatch — an unresolvable binding records
/// the door with `value_digest` absent, never a guess).
pub(crate) struct DeclassifyEvidence {
    /// The `from:` binding, verbatim (`inputs.p` · `tasks.dl.output`).
    pub from: String,
    /// The `because:` justification, verbatim.
    pub because: String,
    /// blake3 hex over the JCS of the binding's resolved value.
    pub value_digest: Option<String>,
}

/// Compute the receipt evidence for a task that is about to RUN: one row
/// per `declassify:` entry, the digest read from the live scopes
/// (`inputs.` / `config.` / a settled `tasks.<id>.output` — anything
/// else records the door digest-less).
pub(crate) fn declassify_evidence(
    task: &RawTask,
    inputs: &BTreeMap<String, Value>,
    records: &BTreeMap<String, TaskRecord>,
) -> Vec<DeclassifyEvidence> {
    task.taint_lifts()
        .filter(|entry| entry.from.is_some())
        .map(|entry| {
            // parser-guaranteed on `law: taint` (rule 5), filtered above
            let from = entry
                .from
                .as_ref()
                .map_or_else(String::new, |f| f.value.clone());
            let value = binding_value(&from, inputs, records);
            DeclassifyEvidence {
                from,
                because: entry.because.value.clone(),
                value_digest: value.and_then(crate::resume::jcs_blake3_hex),
            }
        })
        .collect()
}

/// The live value of a `declassify.from` binding (`inputs.X` ·
/// `tasks.<id>.output`) — `None` when the binding names
/// anything else (the door is still recorded, digest absent).
fn binding_value<'a>(
    from: &str,
    inputs: &'a BTreeMap<String, Value>,
    records: &'a BTreeMap<String, TaskRecord>,
) -> Option<&'a Value> {
    if let Some(name) = from.strip_prefix("inputs.") {
        return inputs.get(name);
    }
    if let Some(rest) = from.strip_prefix("tasks.") {
        let id = rest.strip_suffix(".output")?;
        return records.get(id).map(|rec| &rec.output);
    }
    None
}
