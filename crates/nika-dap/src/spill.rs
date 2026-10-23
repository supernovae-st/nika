// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Values too large for one journal line, kept beside the journal (NIK-131).
//!
//! A journal line is bounded ([`crate::chain::MAX_LINE_BYTES`]): the verifier
//! refuses a longer one before any parse. A task whose output was larger (a
//! fetched 454,763-byte page made a 1,180,930-byte `task_completed`: its
//! `output` and its `outcome` each carry the value) stopped the journal at
//! that line, so the trace ended there and the run could not be resumed.
//!
//! The writer now moves the largest string field values of such a line, one
//! at a time, to `<journal>.values/<sha256 hex>` (the value's exact UTF-8
//! bytes, owner-only like the journal) until the line fits. Each moved field
//! keeps the digest, `sha256:<hex>`, as its value, and the line's
//! [`SPILLED_FIELDS`] field names the fields it moved. The chain hashes the
//! line as written, so it commits to each digest, and a value is read back
//! only when its bytes hash to its digest: [`rehydrate`] restores the event
//! as it was emitted; a missing or altered value stays a digest, which a
//! reader treats as absent (a resume runs that task again, never on wrong
//! bytes). A line that stays over the bound with every value moved is
//! refused as before. Nothing is dropped.

use std::collections::BTreeSet;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use nika_event::Event;
use nika_event::source_id::sha256_hex;
use nika_types::resource::Value as FieldValue;
use serde_json::Value;

/// The field naming, as a JSON array of keys, the fields of a line whose
/// values the journal keeps beside it.
pub const SPILLED_FIELDS: &str = "spilled_fields";

/// The prefix of a kept value's reference: `sha256:<64 lowercase hex>`.
const DIGEST_PREFIX: &str = "sha256:";

/// A value shorter than this stays on its line: moving it saves too little.
const MIN_SPILL_BYTES: usize = 1024;

/// The most bytes one kept value is read back with (the whole-journal bound).
const MAX_VALUE_BYTES: u64 = crate::bounded::MAX_JOURNAL_BYTES as u64;

/// `<journal>.values` — the directory beside a journal holding its kept values.
#[must_use]
pub fn values_dir(journal: &Path) -> PathBuf {
    let mut name = journal.as_os_str().to_owned();
    name.push(".values");
    PathBuf::from(name)
}

/// The bytes of the values `journal` keeps beside it (0 when it keeps none).
pub(crate) fn kept_bytes(journal: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(values_dir(journal)) else {
        return 0;
    };
    (entries.flatten())
        .filter_map(|entry| entry.metadata().ok())
        .filter(std::fs::Metadata::is_file)
        .map(|meta| meta.len())
        .fold(0, u64::saturating_add)
}

/// Remove the values `journal` keeps beside it, with the journal they belong
/// to (retention): fail-open, like the journal's own removal.
pub(crate) fn remove_values(journal: &Path) {
    let _ = std::fs::remove_dir_all(values_dir(journal));
}

/// Move the largest string value of an event-shaped `record` (a `fields`
/// array of `{key, value}`) beside `journal`, its field keeping the digest.
/// `Ok(false)` when no value is left worth moving.
///
/// # Errors
/// The value could not be written beside the journal.
pub(crate) fn spill_largest(record: &mut Value, journal: &Path) -> std::io::Result<bool> {
    let Some(fields) = record.get_mut("fields").and_then(Value::as_array_mut) else {
        return Ok(false);
    };
    let largest = (fields.iter().enumerate())
        .filter(|(_, kv)| kv.get("key").and_then(Value::as_str) != Some(SPILLED_FIELDS))
        .filter_map(|(at, kv)| Some((kv.get("value")?.as_str()?.len(), at)))
        .filter(|(len, _)| *len >= MIN_SPILL_BYTES)
        .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let Some((_, at)) = largest else {
        return Ok(false);
    };
    let key = fields[at]["key"].as_str().unwrap_or_default().to_owned();
    let text = fields[at]["value"].as_str().unwrap_or_default();
    let digest = sha256_hex(text.as_bytes());
    store(&values_dir(journal), &digest, text.as_bytes())?;
    fields[at]["value"] = Value::String(format!("{DIGEST_PREFIX}{digest}"));
    let mut keys: Vec<String> = (fields.iter())
        .find(|kv| kv.get("key").and_then(Value::as_str) == Some(SPILLED_FIELDS))
        .and_then(|kv| serde_json::from_str(kv.get("value")?.as_str()?).ok())
        .unwrap_or_default();
    keys.push(key);
    fields.retain(|kv| kv.get("key").and_then(Value::as_str) != Some(SPILLED_FIELDS));
    let listed = serde_json::to_string(&keys).map_err(std::io::Error::from)?;
    fields.push(serde_json::json!({ "key": SPILLED_FIELDS, "value": listed }));
    Ok(true)
}

/// Write `bytes` as `<dir>/<digest>`, once: a value already kept there is the
/// same value. Owner-only (0700 · 0600) on unix, as the journal is: a kept
/// value is task output. The bytes land under a temporary name first, so the
/// digest's name only ever holds a whole value.
fn store(dir: &Path, digest: &str, bytes: &[u8]) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder.create(dir)?;
    let path = dir.join(digest);
    if path.is_file() {
        return Ok(());
    }
    let staged = dir.join(format!(".{digest}.{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(&staged)?;
    file.write_all(bytes)?;
    file.sync_data()?;
    std::fs::rename(&staged, &path)
}

/// The digest a kept value's reference names, when `text` is one.
fn reference(text: &str) -> Option<&str> {
    let digest = text.strip_prefix(DIGEST_PREFIX)?;
    let hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
    (digest.len() == 64 && digest.bytes().all(hex)).then_some(digest)
}

/// The value kept as `<dir>/<digest>`, when its bytes hash to the digest.
fn read_value(dir: &Path, digest: &str) -> Option<String> {
    let file = std::fs::File::open(dir.join(digest)).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_VALUE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_VALUE_BYTES || sha256_hex(&bytes) != digest {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// What [`rehydrate`] restored.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Rehydrated {
    /// Values read back from beside the journal.
    pub restored: usize,
    /// One `<kind>.<field> sha256:<16 hex>` per value missing or altered
    /// beside the journal; its field keeps the digest.
    pub unresolved: Vec<String>,
}

/// Restore the values `events`, read from `journal`, keep beside it: each
/// field [`SPILLED_FIELDS`] names gets back its value, admitted only when its
/// bytes hash to the digest the line carries. An event whose values all came
/// back loses its [`SPILLED_FIELDS`] field, so it reads as it was emitted.
#[must_use]
pub fn rehydrate(events: &mut [Event], journal: &Path) -> Rehydrated {
    let dir = values_dir(journal);
    let mut report = Rehydrated::default();
    for event in events.iter_mut() {
        let Some(keys) = event.str_field(SPILLED_FIELDS).and_then(listed_keys) else {
            continue;
        };
        let kind = serde_json::to_value(event.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        let mut whole = true;
        for kv in event.fields.iter_mut().filter(|kv| keys.contains(&kv.key)) {
            let FieldValue::String(text) = &kv.value else {
                continue;
            };
            let value = reference(text).and_then(|digest| read_value(&dir, digest));
            if let Some(value) = value {
                kv.value = FieldValue::String(value);
                report.restored += 1;
            } else {
                whole = false;
                let shown: String = text.chars().take(DIGEST_PREFIX.len() + 16).collect();
                report.unresolved.push(format!("{kind}.{} {shown}", kv.key));
            }
        }
        if whole {
            event.fields.retain(|kv| kv.key != SPILLED_FIELDS);
        }
    }
    report
}

/// The keys a [`SPILLED_FIELDS`] value lists.
fn listed_keys(text: &str) -> Option<BTreeSet<String>> {
    serde_json::from_str::<Vec<String>>(text)
        .ok()
        .map(|keys| keys.into_iter().collect())
}

/// Every value a raw journal keeps beside `journal` that is missing there or
/// whose bytes no longer hash to the digest its line carries, as
/// [`Rehydrated::unresolved`] names them. Empty when every value is whole.
#[must_use]
pub fn unresolved_values(raw: &str, journal: &Path) -> Vec<String> {
    let mut events: Vec<Event> = (raw.lines())
        .filter(|line| line.contains(SPILLED_FIELDS))
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    rehydrate(&mut events, journal).unresolved
}

#[cfg(test)]
#[path = "spill_tests.rs"]
mod tests;
