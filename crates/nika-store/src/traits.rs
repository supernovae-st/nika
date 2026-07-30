// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The kernel memory-trait impls (ADR-078 opt-in seal): the future L2
//! orchestrator composes [`SignedMemoryStore`] without knowing the
//! signature exists. The trait surface carries no integrity label and no
//! timestamp, so the store is constructed with the label every trait-path
//! write stamps (per-task inheritance is the [`crate::remember_signed`]
//! free-function path's job) and an injected clock. Trait recall returns
//! ADMITTED entries only — rejections are never hits; they stay named in
//! [`crate::recall_verified`] and counted in the seal fold.
//!
//! The query facets are named, never silently half-applied: **v1 is
//! single-tenant** — a non-default [`RecallQuery::tenant`] scope recalls
//! NOTHING, never everything (ADR-031's mandatory keyspace scope, failed
//! closed); **`min_score`** binds UNIFORMLY against v1's constant 1.0
//! score (substring recall carries no similarity — a threshold above 1.0,
//! or a NaN, admits nothing; the placeholder never fails open);
//! **`observed_after`/`observed_before`** bind when the frame carries the
//! stamp — a stamp-less entry under a WINDOWED query is SKIPPED (the
//! window cannot bind what it cannot read), never silently windowed-in;
//! **`limit`** keeps the NEWEST hits (recall ranks ts-descending — the
//! constant score leaves recency as the honest "ranked" the kernel
//! contract names). The projection's skip classes — a non-string
//! `content`, an unknown level word — are named on the recall impl, and
//! the reserved frame fields (`cipher` · `provenance` · `retention` ·
//! `redactions`) REFUSE the write (fail-closed — the signed envelope
//! carries no slot for them, so writing would drop them INSIDE a
//! signature).

use std::path::PathBuf;

use nika_cap::Integrity;
use nika_error::id::TenantId;
use nika_kernel::memory::{
    MemoryError, MemoryForget, MemoryFrame, MemoryHit, MemoryId, MemoryLevel, MemoryRecall,
    MemoryRemember, RecallQuery,
};
use nika_kernel::sealed::Sealed;
use serde_json::Value;

/// A signed, verified store behind the kernel memory traits (F-P8 · ADR-078).
// No `Debug` on purpose: the minisign SECRET key rides the struct — a
// derived Debug would print key material (or lie about it), and neither
// is worth the convenience (the `NullMemoryStore` precedent carries the
// attribute; the secret custody is why this one stops there).
#[non_exhaustive]
pub struct SignedMemoryStore {
    dir: PathBuf,
    store: String,
    run_id: String,
    label: Integrity,
    key: minisign::SecretKey,
    pubkey: minisign::PublicKey,
    now_ms: fn() -> u64,
}

impl SignedMemoryStore {
    /// Construct over the store dir (`.nika/memory/<store>/`), the injected
    /// keypair (custody stays with the L4 caller), the label trait-path
    /// writes stamp, and the clock. The dir need not exist yet (the first
    /// write creates it) — but its basename MUST be the store name (the
    /// layout's invariant): a mismatch means the caller confused dir and
    /// store, and writes would land under a name the fold walks as
    /// another store's. The pin also subsumes the [`crate::store_dir`]
    /// name validation (a separator or `..` can never equal a basename).
    ///
    /// # Errors
    ///
    /// [`crate::StoreError::DirLayout`] when the dir's basename is not the
    /// store name.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dir: PathBuf,
        store: String,
        run_id: String,
        label: Integrity,
        key: minisign::SecretKey,
        pubkey: minisign::PublicKey,
        now_ms: fn() -> u64,
    ) -> Result<Self, crate::StoreError> {
        if dir.file_name() != Some(std::ffi::OsStr::new(&store)) {
            return Err(crate::StoreError::DirLayout {
                reason: format!(
                    "store dir {} does not end in the store name {store:?}",
                    dir.display()
                ),
            });
        }
        Ok(Self {
            dir,
            store,
            run_id,
            label,
            key,
            pubkey,
            now_ms,
        })
    }
}

// Opt-in sealed pattern per ADR-078 (the NullMemoryStore precedent).
impl Sealed for SignedMemoryStore {}

fn storage(e: &crate::StoreError) -> MemoryError {
    MemoryError::Storage {
        reason: e.to_string(),
    }
}

/// The frame's stored value (only what recall can return rides).
fn frame_value(frame: &MemoryFrame) -> Value {
    serde_json::json!({
        "content": frame.content,
        "level": level_str(frame.level),
        "tags": frame.tags,
        "metadata": frame.metadata,
        "source": frame.source,
        "observed_at": frame.observed_at,
    })
}

/// The `MemoryLevel` wire word (mirrors its serde `snake_case` — the frame
/// value stays readable without the nika-types serde feature). The
/// asymmetry, named: the WRITE side maps a future level to `"working"` —
/// the signed envelope must stamp SOME word and the future tier's real
/// name is unknowable here — while the READ side SKIPS an unknown word
/// (`level_from_str` returns `None`, never a silent downgrade); the
/// round-trip holds exactly for the tiers this version knows.
fn level_str(level: MemoryLevel) -> &'static str {
    match level {
        MemoryLevel::Episodic => "episodic",
        MemoryLevel::Semantic => "semantic",
        MemoryLevel::Procedural => "procedural",
        MemoryLevel::Reflective => "reflective",
        MemoryLevel::Conceptual => "conceptual",
        _ => "working", // a future level reads as working (never a guess at its name)
    }
}

/// Decode the level word (`None` = a tier this version does not know —
/// the caller SKIPS the hit, never a silent downgrade to `Working`).
fn level_from_str(word: &str) -> Option<MemoryLevel> {
    Some(match word {
        "working" => MemoryLevel::Working,
        "episodic" => MemoryLevel::Episodic,
        "semantic" => MemoryLevel::Semantic,
        "procedural" => MemoryLevel::Procedural,
        "reflective" => MemoryLevel::Reflective,
        "conceptual" => MemoryLevel::Conceptual,
        _ => return None,
    })
}

impl MemoryRemember for SignedMemoryStore {
    async fn remember(&self, frame: MemoryFrame) -> Result<MemoryId, MemoryError> {
        // Fail CLOSED on the reserved fields (cipher · provenance ·
        // retention · redactions): the v1 envelope carries no slot for
        // them, so writing anyway would DROP them inside a SIGNED
        // envelope — a silent loss the signature would then attest.
        // (Storage is the write path's error class; this refusal is
        // deterministic — a retry of the same frame refuses again.)
        if frame.cipher.is_some()
            || frame.provenance.is_some()
            || frame.retention.is_some()
            || frame.redactions.is_some()
        {
            return Err(MemoryError::Storage {
                reason: "reserved frame fields (cipher · provenance · retention · redactions) \
                     have no slot in the v1 signed envelope — the write is refused, \
                     never silently dropped inside a signature"
                    .to_owned(),
            });
        }
        let entry = crate::UnsignedEntry::new(
            frame_value(&frame),
            self.label.clone(),
            self.store.clone(),
            self.run_id.clone(),
            (self.now_ms)(),
        );
        let signed =
            crate::remember_signed(&self.dir, entry, &self.key).map_err(|e| storage(&e))?;
        Ok(signed.memory_id())
    }
}

/// The trait projection of one ADMITTED entry — `None` = the hit is
/// SKIPPED, the class named where it happens (the trait surface carries
/// no skip-count channel, so the naming lives in these comments): a
/// non-string `content` (an honestly-signed non-text frame is not
/// recallable as text — never a silent empty-content hit, never a
/// vanishing entry) · an unknown level word (fail-closed tier — skipped,
/// never downgraded to `Working`; the entry stays ADMITTED in
/// `recall_verified`, only this projection declines it) · a stamp-less
/// entry under an observed-window query (the window cannot bind what it
/// cannot read).
fn project_hit(entry: &crate::StoreEntry, query: &RecallQuery, needle: &str) -> Option<MemoryHit> {
    let content = entry
        .frame
        .get("content")
        .and_then(Value::as_str)?
        .to_owned();
    if !needle.is_empty() && !content.to_lowercase().contains(needle) {
        return None;
    }
    let level = match entry.frame.get("level").and_then(Value::as_str) {
        // A v1 writer always stamps the level; a missing word stays the
        // v1 default (named, not silent).
        None => MemoryLevel::Working,
        Some(word) => level_from_str(word)?, // fail-closed tier (the fn doc)
    };
    if query.levels.as_ref().is_some_and(|ls| !ls.contains(&level)) {
        return None;
    }
    // min_score binds UNIFORMLY against v1's constant score (every hit
    // scores 1.0): a threshold above 1.0 admits nothing, and a NaN
    // threshold admits nothing either (every NaN comparison is false) —
    // the placeholder never fails open.
    if query.min_score.is_some_and(|m| m > 1.0 || m.is_nan()) {
        return None;
    }
    // The observed window binds when the frame carries the stamp (the
    // kernel's ns-canonical `observed_at`), STRICTLY inside: a stamp-less
    // entry under a windowed query is skipped (the fn doc names the
    // class); an unwindowed query never asks.
    if query.observed_after.is_some() || query.observed_before.is_some() {
        let observed = entry.frame.get("observed_at").and_then(Value::as_u64)?;
        if query.observed_after.is_some_and(|after| observed <= after) {
            return None;
        }
        if query
            .observed_before
            .is_some_and(|before| observed >= before)
        {
            return None;
        }
    }
    let tags: Vec<String> = entry
        .frame
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|t| t.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if query
        .tags
        .as_ref()
        .is_some_and(|want| !want.iter().all(|t| tags.contains(t)))
    {
        return None;
    }
    let mut hit = MemoryHit::new(entry.memory_id(), content, level, 1.0);
    hit.tags = tags;
    hit.metadata = entry
        .frame
        .get("metadata")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    hit.source = entry
        .frame
        .get("source")
        .and_then(Value::as_str)
        .map(str::to_owned);
    hit.observed_at = entry.frame.get("observed_at").and_then(Value::as_u64);
    Some(hit)
}

impl MemoryRecall for SignedMemoryStore {
    /// v1 recall is substring over ADMITTED entries, ranked MOST-RECENT
    /// FIRST (ts descending — the constant score carries no similarity
    /// signal, so recency is the honest "ranked" the kernel contract
    /// names): `limit` then keeps the NEWEST hits, never the oldest.
    /// `tags` is conjunctive-ALL (every wanted tag must ride the frame —
    /// an EMPTY wanted set admits everything, the vacuous truth), while
    /// `levels: Some([])` filters EVERYTHING (`contains` on an empty set
    /// is false) — two different empty-set postures, both named. The skip
    /// classes are named on the `project_hit` projection (non-string
    /// content · unknown level word · stamp-less under a window).
    async fn recall(&self, query: RecallQuery) -> Result<Vec<MemoryHit>, MemoryError> {
        // ADR-031 fail-CLOSED: v1 is single-tenant — a non-default scope
        // recalls NOTHING, never everything (an unscoped sweep would leak
        // the default keyspace into a tenant that asked for its own).
        if query.tenant != TenantId::default_tenant() {
            return Ok(Vec::new());
        }
        let entries = crate::recall_verified(&self.dir, &self.store, &self.pubkey)
            .map_err(|e| storage(&e))?;
        let needle = query.text.to_lowercase();
        let mut hits = Vec::new();
        for ev in entries {
            let (crate::RecallVerdict::Admitted, Some(entry)) = (ev.verdict, ev.entry) else {
                continue;
            };
            if let Some(hit) = project_hit(&entry, &query, &needle) {
                hits.push((entry.ts_ms, hit));
            }
        }
        // Most-recent first (the impl doc names why recency is the rank);
        // the stable sort keeps the walk's deterministic path order
        // inside one timestamp.
        hits.sort_by(|a, b| b.0.cmp(&a.0));
        let mut hits: Vec<MemoryHit> = hits.into_iter().map(|(_, hit)| hit).collect();
        if let Some(limit) = query.limit {
            hits.truncate(limit);
        }
        Ok(hits)
    }
}

impl MemoryForget for SignedMemoryStore {
    async fn forget(&self, id: MemoryId) -> Result<(), MemoryError> {
        let files = crate::dir::walk(&self.dir).map_err(|e| storage(&e))?;
        for file in files {
            let Ok(entry) = &file.decoded else {
                continue;
            };
            if entry.memory_id() == id {
                return crate::dir::remove(&file.path).map_err(|e| storage(&e));
            }
        }
        Ok(()) // idempotent — a missing id is a no-op (the trait's contract)
    }
}
