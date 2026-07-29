// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The signed envelope: [`StoreEntry`] (+ its pre-signature half,
//! [`UnsignedEntry`]) and the JCS canonical form the signature covers.
//! The envelope is VERSIONED from day one: `"v": 1` rides INSIDE the
//! signed preimage (FCI-003/011/022 — every named v2 owe changes the
//! preimage), so the signature commits to the format itself and decode
//! dispatches on `v` (an unknown version is REJECTED with the named
//! reason, never a masquerading `BadSignature`).

use nika_cap::Integrity;
use serde_json::{Value, json};

use crate::RejectReason;

/// The envelope format version this crate writes and reads. The signature
/// commits to it (the field rides the signed preimage), so a format bump
/// is a NEW envelope family, never a silent reinterpretation.
pub(crate) const ENTRY_FORMAT_VERSION: u64 = 1;

/// One store entry — the signed envelope. Minted ONLY by
/// [`crate::remember_signed`] (or decoded at recall): `sig` rides over the
/// JCS of every other field, so ANY byte flip — content, label, store,
/// timestamps — is a signature mismatch at recall.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct StoreEntry {
    /// The content (a `MemoryFrame`'s value).
    pub frame: Value,
    /// The origin label, inherited at write — INSIDE the signature.
    pub label: Integrity,
    /// The named store.
    pub store: String,
    /// The writing run.
    pub run_id: String,
    /// Write timestamp (ms).
    pub ts_ms: u64,
    /// Digest of the parent entry ids (the lineage · `""` = no parents, honest).
    pub parents_digest: String,
    /// The minisign box over the JCS of the fields above.
    pub sig: String,
}

/// An entry before signing — the caller fills the provenance (the label
/// inherited from the task's integrity · the run id · the lineage digest).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct UnsignedEntry {
    /// The content.
    pub frame: Value,
    /// The origin label (rides INSIDE the signature once signed).
    pub label: Integrity,
    /// The named store.
    pub store: String,
    /// The writing run.
    pub run_id: String,
    /// Write timestamp (ms) — the caller stamps it (the engine owns the clock seam).
    pub ts_ms: u64,
    /// Digest of the parent entry ids (`""` = no parents, honest).
    pub parents_digest: String,
}

impl UnsignedEntry {
    /// An entry with no parents (the common case — v1 lineage is caller-computed).
    #[must_use]
    pub fn new(frame: Value, label: Integrity, store: String, run_id: String, ts_ms: u64) -> Self {
        Self {
            frame,
            label,
            store,
            run_id,
            ts_ms,
            parents_digest: String::new(),
        }
    }
}

/// The label's wire form (the lattice stays `nika_cap`'s — this is only its
/// JSON projection, using [`Integrity::as_str`]'s words).
pub(crate) fn label_wire(label: &Integrity) -> Value {
    match label {
        Integrity::Trusted => json!({"kind": "trusted"}),
        Integrity::Untrusted { source } => json!({"kind": "untrusted", "source": source}),
    }
}

/// Decode the label's wire form (`None` = malformed → the entry is rejected).
pub(crate) fn label_from_wire(value: &Value) -> Option<Integrity> {
    match value.get("kind")?.as_str()? {
        "trusted" => Some(Integrity::trusted()),
        "untrusted" => Some(Integrity::untrusted(value.get("source")?.as_str()?)),
        _ => None,
    }
}

/// The JCS canonical bytes — the engine's ONE canonicalization voice,
/// byte-identical to `nika_proof::canonical`: `serde_json` object keys are
/// BTreeMap-sorted (this workspace never enables `preserve_order`), compact,
/// raw UTF-8. Serializing a `Value` cannot fail (string keys · no NaN
/// inhabitants) — the `""` fallback is unreachable, same as the proof layer's.
pub(crate) fn jcs(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

impl StoreEntry {
    /// The unsigned half with an empty `sig` (the signer's input).
    pub(crate) fn from_unsigned(u: UnsignedEntry) -> Self {
        Self {
            frame: u.frame,
            label: u.label,
            store: u.store,
            run_id: u.run_id,
            ts_ms: u.ts_ms,
            parents_digest: u.parents_digest,
            sig: String::new(),
        }
    }

    /// The signed preimage value: every field EXCEPT `sig` (F-P8 · the
    /// label rides inside, so a relabel IS a signature mismatch). The
    /// format version `"v"` rides the preimage too — the signature
    /// commits to the envelope's format, not just its content.
    #[must_use]
    pub fn preimage_value(&self) -> Value {
        json!({
            "frame": self.frame,
            "label": label_wire(&self.label),
            "parents_digest": self.parents_digest,
            "run_id": self.run_id,
            "store": self.store,
            "ts_ms": self.ts_ms,
            "v": ENTRY_FORMAT_VERSION,
        })
    }

    /// The JCS preimage bytes the signature covers.
    #[must_use]
    pub fn preimage(&self) -> String {
        jcs(&self.preimage_value())
    }

    /// The on-disk value (the preimage + the `sig` field).
    #[must_use]
    pub fn file_value(&self) -> Value {
        let mut value = self.preimage_value();
        if let Value::Object(ref mut map) = value {
            map.insert("sig".to_owned(), Value::String(self.sig.clone()));
        }
        value
    }

    /// Decode an entry from its on-disk value, dispatching on the format
    /// version FIRST: a `"v"` this version does not know rejects
    /// [`RejectReason::UnsupportedVersion`] (the format is named, never a
    /// masquerading signature failure); any other decode gap is
    /// [`RejectReason::Malformed`]. An ABSENT `v` decodes as v1 (a
    /// pre-marker hand-rolled envelope — the signature commits to `v`, so
    /// honestly-signed bytes always carry it; a missing one can only ride
    /// an unsigned envelope, and the `Unsigned` naming stays honest). A
    /// MISSING `sig` decodes as the empty signature — the `Unsigned`
    /// class (the 0 %-admission floor), not `Malformed`: the envelope is
    /// otherwise complete, it just has no provenance.
    pub(crate) fn from_file_value(value: &Value) -> Result<Self, RejectReason> {
        match value.get("v").and_then(Value::as_u64) {
            None | Some(ENTRY_FORMAT_VERSION) => {}
            Some(_) => return Err(RejectReason::UnsupportedVersion),
        }
        let malformed = || RejectReason::Malformed;
        Ok(Self {
            frame: value.get("frame").ok_or_else(malformed)?.clone(),
            label: label_from_wire(value.get("label").ok_or_else(malformed)?)
                .ok_or_else(malformed)?,
            store: value
                .get("store")
                .and_then(Value::as_str)
                .ok_or_else(malformed)?
                .to_owned(),
            run_id: value
                .get("run_id")
                .and_then(Value::as_str)
                .ok_or_else(malformed)?
                .to_owned(),
            ts_ms: value
                .get("ts_ms")
                .and_then(Value::as_u64)
                .ok_or_else(malformed)?,
            parents_digest: value
                .get("parents_digest")
                .and_then(Value::as_str)
                .ok_or_else(malformed)?
                .to_owned(),
            sig: match value.get("sig") {
                None => String::new(),
                Some(v) => v.as_str().ok_or_else(malformed)?.to_owned(),
            },
        })
    }

    /// The entry's content digest: blake3 over the JCS preimage (the signed
    /// bytes) — names the entry in its file name and in the seal fold's
    /// admitted set.
    #[must_use]
    pub fn digest(&self) -> String {
        blake3::hash(self.preimage().as_bytes())
            .to_hex()
            .to_string()
    }

    /// The deterministic `MemoryId` (the first 16 digest bytes as a UUID) —
    /// the kernel trait surface's stable identity for forget-by-id.
    #[must_use]
    pub fn memory_id(&self) -> nika_kernel::memory::MemoryId {
        let hash = blake3::hash(self.preimage().as_bytes());
        let mut raw = [0u8; 16];
        raw.copy_from_slice(&hash.as_bytes()[..16]);
        nika_kernel::memory::MemoryId::new(uuid::Uuid::from_bytes(raw))
    }
}
