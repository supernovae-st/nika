// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The knowledge a session pins and the records it stamps on a compile outcome, owned beside
//! the snapshot door they read (descended from `nika-session` on 2026-09-28, C7 · D1). A pin
//! is the identity of a snapshot as it was opened: its declared version and digest, the sha256
//! of its manifest bytes and of its rows as read. A record is what one compile observed,
//! composed, presented or carried, in brief. Pure: an outcome and a value in, a record out —
//! nothing here reads the environment, calls a model, or decides a policy (the session keeps
//! its seat, its choices and its consent).

use std::fmt::Write as _;
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

    /// Whether `snapshot`, as read now, is no longer the one pinned — its manifest's own bytes,
    /// its rows, its declared version or digest: both identities in words (pinned, found), or
    /// `None` while it is the same.
    #[must_use]
    pub fn moved(&self, snapshot: &Snapshot) -> Option<(String, String)> {
        let (version, digest, manifest, rows) = Self::of(snapshot);
        let same = version == self.version.as_deref()
            && digest == self.digest.as_deref()
            && manifest == self.manifest_sha256
            && rows == self.rows_sha256;
        (!same).then(|| {
            let pinned = Self::words(
                self.version.as_deref(),
                self.digest.as_deref(),
                &self.manifest_sha256,
                &self.rows_sha256,
            );
            (pinned, Self::words(version, digest, manifest, &rows))
        })
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

    /// The identity in the words a status line says: the declared version, the declared digest,
    /// and the digests of the manifest and the rows as read (cut at twelve).
    #[must_use]
    pub fn status_words(&self) -> String {
        format!(
            "knowledge {} · declared digest {} · manifest {} · rows {}",
            self.version.as_deref().unwrap_or("unversioned"),
            short(self.digest.as_deref().unwrap_or("none")),
            short(&self.manifest_sha256),
            short(&self.rows_sha256)
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
/// keys it holds (the names themselves ride the request, not the receipt), and the identity of
/// the whole observation it attached (`world_sha256`): the rows are a summary for display, never
/// that identity.
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
    record["session"]["observed"] = json!({ "attached": true, "presented": presented,
        "under": "project root", "rows": rows, "world_sha256": world_sha256(world) });
    out
}

/// The identity of an observation as a host attached it: the sha256 of its bytes as held.
pub(crate) fn world_sha256(world: &Value) -> String {
    sha256_hex(world.to_string().as_bytes())
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

/// The decision seat's receipt beside the compiler's own record of the same questions
/// (`decision.session.decision_seat`): what the seat was asked, sent, answered or refused. The
/// session's later [`stamp`] keeps it.
pub fn stamp_seat(out: &mut CompileOutcome, receipt: Value) {
    let record = out.provenance.decision.get_or_insert_with(|| json!({}));
    if let Some(record) = record.as_object_mut()
        && let Some(session) = record
            .entry("session")
            .or_insert_with(|| json!({}))
            .as_object_mut()
    {
        session.insert("decision_seat".to_owned(), receipt);
    }
}

/// The knowledge lines of the session's record (`decision.session.authoring`), as `/details`
/// shows them (descended from `nika-session`'s `runtime/details.rs`, C11 · B19): the strategy
/// the policy carried, then the pack — presented to the seat (with the calls that carried it),
/// attached but never read (and why), or carried from the round that authored a replayed
/// candidate. Each line is appended to `text`, starting on a new line.
pub fn knowledge_lines(record: &Value, text: &mut String) {
    // A digest the record states, first twelve characters; any other value reads "none".
    let digest = |value: &Value| short(value.as_str().unwrap_or("none"));
    let _ = write!(
        text,
        "\n  authoring strategy: {} ({})",
        record["strategy"].as_str().unwrap_or("unknown"),
        record["source"].as_str().unwrap_or("unknown")
    );
    let knowledge = &record["knowledge"];
    if knowledge.is_null() {
        text.push_str("\n  knowledge: none attached");
        return;
    }
    let identity = &knowledge["identity"];
    let references = knowledge["references"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let bytes: u64 = references.iter().filter_map(|r| r["bytes"].as_u64()).sum();
    // The pack's and the calls' digests are printed whole: they are what an
    // auditor compares to the bytes a seat received.
    // The digest is the manifest's own claim; the manifest and rows digests are computed.
    let _ = write!(
        text,
        "\n  knowledge: {} · declared digest {} · manifest {} · rows {} · {} reference{} · {bytes} B · {}\n  pack sha256 {}",
        identity["version"].as_str().unwrap_or("unversioned"),
        digest(&identity["digest"]),
        digest(&identity["manifest_sha256"]),
        digest(&identity["rows_sha256"]),
        references.len(),
        if references.len() == 1 { "" } else { "s" },
        knowledge["pack_builder"]
            .as_str()
            .unwrap_or("unknown builder"),
        knowledge["pack_sha256"].as_str().unwrap_or("none")
    );
    for reference in references {
        let _ = write!(
            text,
            "\n    {} {} · {} B · sha256 {}",
            reference["kind"].as_str().unwrap_or("?"),
            reference["id"].as_str().unwrap_or("?"),
            reference["bytes"].as_u64().unwrap_or(0),
            digest(&reference["sha256"])
        );
    }
    let carried = knowledge["carried"].as_bool().unwrap_or(false);
    if knowledge["presented"].as_bool().unwrap_or(false) {
        let calls = knowledge["calls"].as_array().map_or(&[][..], Vec::as_slice);
        let _ = write!(
            text,
            "\n  presented to the seat in {} call{}{}",
            calls.len(),
            if calls.len() == 1 { "" } else { "s" },
            if carried {
                " of the round that authored this candidate (this answer round replayed it · zero calls)"
            } else {
                ""
            }
        );
        for call in calls {
            let _ = write!(
                text,
                "\n    {} · instruction sha256 {}",
                call["call"].as_str().unwrap_or("?"),
                call["instruction_sha256"].as_str().unwrap_or("none")
            );
        }
        seat_line(&knowledge["seat"], text);
    } else {
        let _ = write!(
            text,
            "\n  not presented: {}",
            knowledge["why"]
                .as_str()
                .unwrap_or("the native door did not read it")
        );
    }
}

/// What authored with the pack, in brief (kept with the knowledge record, so a replayed candidate
/// still names it): the model, where its calls went, how many, the usage the provider reported.
fn seat_line(seat: &Value, text: &mut String) {
    if !seat.is_object() {
        return;
    }
    if seat["backend"]["kind"] == "harness_infer" {
        let _ = write!(
            text,
            "\n    by subscription {} · {} call(s) · responding identities in backend receipt · cost unknown",
            seat["backend"]["adapter"].as_str().unwrap_or("unknown"),
            seat["calls"]
        );
        return;
    }
    let usage = match (
        seat["input_tokens"].as_u64(),
        seat["output_tokens"].as_u64(),
    ) {
        (Some(i), Some(o)) => format!("{i} in / {o} out tokens"),
        _ => "usage not reported by the provider".to_owned(),
    };
    let calls = seat["calls"].as_u64().unwrap_or(0);
    let _ = write!(
        text,
        "\n    by {} · host {} · {calls} call{} in that round · {usage} · {} ms",
        seat["model"].as_str().unwrap_or("unknown model"),
        seat["backend"]["host"].as_str().unwrap_or("unknown"),
        if calls == 1 { "" } else { "s" },
        seat["elapsed_ms"].as_u64().unwrap_or(0)
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
