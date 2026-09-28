// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The knowledge a session pins and the records it stamps on a compile outcome, owned beside
//! the snapshot door they read (descended from `nika-session` on 2026-09-28, C7 · D1). A pin
//! is the identity of a snapshot as it was opened: its declared version and digest, the sha256
//! of its manifest bytes and of its rows as read. A record is what one compile observed,
//! composed, presented or carried, in brief. Pure: an outcome and a value in, a record out —
//! nothing here reads the environment, calls a model, or decides a policy (the session keeps
//! its seat, its choices and its consent).

use std::path::PathBuf;

use nika_event::source_id::sha256_hex;
use serde_json::{Value, json};

use super::{KnowledgeError, PACK_BUILDER, Snapshot, pack_sha256};
use crate::compile::{AuthoringKnowledge, CompileOutcome, Strategy};

/// The identity a session pinned for its knowledge snapshot when it opened.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct KnowledgePin {
    /// The snapshot directory.
    pub dir: PathBuf,
    /// A corpus whose examples are never recalled.
    pub exclude_corpus: Option<String>,
    /// The snapshot's version, as its manifest names it.
    pub version: Option<String>,
    /// The digest the manifest declares (the exporter's, never recomputed).
    pub digest: Option<String>,
    /// The sha256 of the manifest's bytes as read (computed): the pin's integrity, with the rows.
    pub manifest_sha256: String,
    /// The digest of the row files as the door read them (computed).
    pub rows_sha256: String,
}

impl KnowledgePin {
    /// Open the snapshot at `dir` and pin its identity as read now.
    ///
    /// # Errors
    /// The directory is not a snapshot, or it is stale ([`KnowledgeError`]).
    pub fn open(dir: PathBuf, exclude_corpus: Option<String>) -> Result<Self, KnowledgeError> {
        let snapshot = Snapshot::open(&dir)?;
        Ok(Self {
            version: snapshot.version().map(str::to_owned),
            digest: snapshot.digest().map(str::to_owned),
            manifest_sha256: snapshot.manifest_sha256().to_owned(),
            rows_sha256: snapshot.rows_sha256(),
            dir,
            exclude_corpus,
        })
    }

    /// The pin's identity record (what a receipt names).
    #[must_use]
    pub fn record(&self) -> Value {
        json!({
            "version": self.version,
            "digest": self.digest,
            "digest_is": "declared by the manifest, not recomputed",
            "manifest_sha256": self.manifest_sha256,
            "rows_sha256": self.rows_sha256,
            "dir": self.dir.display().to_string(),
            "exclude_corpus": self.exclude_corpus,
        })
    }

    /// The pin as a snapshot on disk states it now.
    #[must_use]
    pub fn of(snapshot: &Snapshot) -> (Option<&str>, Option<&str>, &str, String) {
        (
            snapshot.version(),
            snapshot.digest(),
            snapshot.manifest_sha256(),
            snapshot.rows_sha256(),
        )
    }

    /// The identity in words: version · declared digest · manifest · rows (cut at twelve).
    #[must_use]
    pub fn words(
        version: Option<&str>,
        digest: Option<&str>,
        manifest: &str,
        rows: &str,
    ) -> String {
        format!(
            "{} (declared digest {} · manifest {} · rows {})",
            version.unwrap_or("unversioned"),
            short(digest.unwrap_or("none")),
            short(manifest),
            short(rows)
        )
    }
}

/// The first twelve characters of a digest.
#[must_use]
pub fn short(digest: &str) -> String {
    digest.chars().take(12).collect()
}

/// The outcome with the session's record of what it observed and presented, in brief
/// (`decision.session.observed`): each named path, its state, its kind and how many columns or
/// keys it holds (the names themselves ride the request, not the receipt).
#[must_use]
pub fn observed_in(mut out: CompileOutcome, world: Option<&Value>) -> CompileOutcome {
    let presented = out.provenance.authoring.as_ref().is_some_and(|receipt| {
        receipt
            .context
            .iter()
            .any(|call| call["call"].as_str().is_some_and(reads_knowledge))
    });
    let (Some(world), Some(record)) = (world, out.provenance.decision.as_mut()) else {
        return out;
    };
    let rows: Vec<Value> = world["observed"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            json!({
                "path": row["path"],
                "state": row["state"],
                "kind": row["kind"],
                "columns": row["columns"].as_array().map_or(0, Vec::len),
            })
        })
        .collect();
    record["session"]["observed"] =
        json!({ "attached": true, "presented": presented, "under": "project root", "rows": rows });
    out
}

/// The session's record of the pack it attached to one call: the pinned
/// identity, the builder, the pack's digest, every reference (kind · id ·
/// bytes · sha256), whether the compiler's native door presented it to the
/// seat (its own record names the same pack digest) and, when it did, the
/// instruction digest of every call that carried it.
#[must_use]
pub fn composed_record(
    pin: &KnowledgePin,
    pack: &AuthoringKnowledge,
    out: &CompileOutcome,
) -> Value {
    let digest = pack
        .identity
        .pointer("/door/pack_sha256")
        .and_then(Value::as_str)
        .map_or_else(|| pack_sha256(pack), str::to_owned);
    let presented = out
        .provenance
        .decision
        .as_ref()
        .and_then(|d| d.pointer("/native/knowledge/identity/door/pack_sha256"))
        .and_then(Value::as_str)
        == Some(digest.as_str());
    let calls: Vec<Value> = out
        .provenance
        .authoring
        .as_ref()
        .filter(|_| presented)
        .map(|receipt| {
            receipt
                .context
                .iter()
                .filter(|call| {
                    call.get("call")
                        .and_then(Value::as_str)
                        .is_some_and(reads_knowledge)
                })
                .map(|call| json!({"call": call["call"], "instruction_sha256": call["instruction_sha256"]}))
                .collect()
        })
        .unwrap_or_default();
    let why = (!presented).then(|| match out.provenance.strategy {
        Some(strategy) if strategy != Strategy::Native => format!(
            "the request settled on the {} path; only the native door reads knowledge",
            strategy.word()
        ),
        _ => "the native door did not present the pack to the seat".to_owned(),
    });
    // What authored with the pack — the round's receipt in brief — kept with the record, so a
    // candidate an answer round replays (zero calls) still names its model, host and usage.
    let seat = out
        .provenance
        .authoring
        .as_ref()
        .filter(|_| presented)
        .map(|receipt| {
            json!({
                "model": receipt.model,
                "calls": receipt.calls,
                "input_tokens": receipt.input_tokens,
                "output_tokens": receipt.output_tokens,
                "elapsed_ms": receipt.elapsed_ms,
                "backend": receipt.backend,
            })
        });
    json!({
        "identity": pin.record(),
        "pack_builder": PACK_BUILDER,
        "pack_sha256": digest,
        "references": pack.references.iter().map(|r| json!({
            "kind": r.kind,
            "id": r.id,
            "bytes": r.text.len(),
            "sha256": sha256_hex(r.text.as_bytes()),
        })).collect::<Vec<_>>(),
        "repairs": pack.repairs.len(),
        "presented": presented,
        "why": why,
        "calls": calls,
        "seat": seat,
        "carried": false,
    })
}

/// The calls of the native door — the only ones whose instruction carries the
/// pack: the native candidate, the sketch and its fills, and their repairs.
#[must_use]
pub fn reads_knowledge(call: &str) -> bool {
    ["native", "sketch", "fill"]
        .iter()
        .any(|door| call == *door || call.starts_with(&format!("{door}-")))
}

/// The record of the round that authored a replayed candidate, carried: this
/// call presented nothing and called nobody.
#[must_use]
pub fn carried_record(record: &Value) -> Value {
    let mut carried = record.clone();
    if let Some(map) = carried.as_object_mut() {
        map.insert("carried".to_owned(), Value::Bool(true));
    }
    carried
}

/// The session's knowledge record in an outcome when the native door
/// presented the pack (what an answer round carries).
#[must_use]
pub fn presented_knowledge(out: &CompileOutcome) -> Option<Value> {
    let record = out
        .provenance
        .decision
        .as_ref()?
        .pointer("/session/authoring/knowledge")?;
    (record.get("presented") == Some(&Value::Bool(true))).then(|| record.clone())
}

/// Stamp the session's record beside the compiler's (`decision.session`), as a
/// host transport stamps its backend into the receipt: the strategy (its word) and its
/// source (`default` · `environment` · `host`), the knowledge attached (or none).
pub fn stamp(out: &mut CompileOutcome, strategy: &str, source: &str, knowledge: Option<&Value>) {
    let mut record = json!({"authoring": {
        "strategy": strategy,
        "source": source,
        "knowledge": knowledge,
    }});
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    // The decision seat's receipt, stamped by the seated call, stays beside it.
    if let Some(seat) = decision.pointer("/session/decision_seat").cloned() {
        record["decision_seat"] = seat;
    }
    if let Some(map) = decision.as_object_mut() {
        map.insert("session".to_owned(), record);
    }
    out.provenance.decision = Some(decision);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
