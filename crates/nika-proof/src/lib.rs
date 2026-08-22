// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The proof layer (spec `15-proof.md` · W6) — a run's canonical **semantic
//! identity**, domain-separated and Merkle-linked.
//!
//! Everything before this module made a workflow *checkable*; this one makes
//! a run *provable*: the hash of what a workflow MEANS (not how it is
//! spelled), a single lock that pins by digest (`nika_pck_manifest`), the
//! `assert:` obligations judged at an honest level (`nika_vocab::assert`),
//! and the one [`receipt`] into which the certificate, the trace verdict, the
//! assertions and the lock digest fold.
//!
//! ## The second evaluator
//!
//! The semantics belong to the SPEC, not to this engine: the stdlib-Python
//! reference (`conformance/proof_core.py` in nika-spec · 22 laws) is the
//! conformance oracle. The parity that matters is on the **pre-image** — the
//! exact bytes that get hashed — never on the digest across algorithms: the
//! reference hashes those bytes with sha256, this engine with blake3, and
//! `tests` pins the pre-image bytes BYTE-EQUAL against reference-produced
//! goldens (`tests::canonical_matches_the_reference` and siblings). A
//! differential on the canonical bytes needs no blake3 dependency on the
//! Python side — the algorithm is a pinned choice both evaluators share.
//!
//! ## The honest property (never overstated · spec 15)
//!
//! Semantically-different programs produce different canonical encodings —
//! [`canonical`] is total and injective over distinct [`Value`]s, so
//! [`preimage`] and [`semantic_hash`] separate them. Collision resistance of
//! the underlying hash (blake3) is a **cryptographic assumption**, stated as
//! such — NOT a promise this module makes. Two files that MEAN the same
//! workflow lower to the same IR (the runtime-side `ir` projection) and hence the same
//! hash; two files that mean different workflows do not.

use serde_json::Value;

pub mod receipt;
mod resume;

pub use resume::{
    KEY_VERSION, MARK, PriorSuccess, ResumeKey, ResumePlan, ResumeUnverified, definition_value,
    fields, item_stand_in, jcs_blake3_hex, reads_default_model, referenced_upstreams,
    secret_marker, skill_paths, touches_intelligence, unwind_tasks_of, workflow_targets,
};

/// The v1 pre-image format version — participates in every pre-image so a
/// value hashed under one shape can never be reused under another (spec 15 ·
/// mirror of `proof_core.RECEIPT_FORMAT`/the `format_version` argument).
pub const FORMAT_VERSION: u32 = 1;

/// The closed set of hash domains (spec 15 §domain separation · exactly the
/// reference's `DOMAINS` tuple, in order). Every hash NAMES its domain so a
/// value can never be reinterpreted across roles: a trace-domain hash never
/// collides usefully with a semantic-domain hash even over identical bytes.
///
/// The set is CLOSED at the type level — an unknown domain is unrepresentable
/// (the reference refuses it at call time; here it cannot be constructed,
/// [`HashDomain::parse`] is the boundary that mirrors the refusal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum HashDomain {
    /// The source bytes as authored (pre-lowering).
    Source,
    /// The canonical form (spec 09 · normalized types · sorted keys).
    Canonical,
    /// The desugared, versioned Semantic IR (the workflow's identity).
    Semantic,
    /// A resolved execution plan.
    Plan,
    /// A run's hash-chained trace.
    Trace,
    /// A produced artifact's content.
    Artifact,
    /// A folded proof receipt (spec 15 §receipt).
    Receipt,
}

impl HashDomain {
    /// The seven domains, in the reference's canonical order.
    pub const ALL: [Self; 7] = [
        Self::Source,
        Self::Canonical,
        Self::Semantic,
        Self::Plan,
        Self::Trace,
        Self::Artifact,
        Self::Receipt,
    ];

    /// The wire spelling — the exact token the pre-image carries (the
    /// reference's lowercase domain string).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Canonical => "canonical",
            Self::Semantic => "semantic",
            Self::Plan => "plan",
            Self::Trace => "trace",
            Self::Artifact => "artifact",
            Self::Receipt => "receipt",
        }
    }

    /// Parse a domain token, refusing anything outside the closed set — the
    /// boundary that mirrors the reference's `preimage` raising on an unknown
    /// domain (spec 15 · "the set is closed").
    ///
    /// # Errors
    ///
    /// [`UnknownDomain`] when `token` is not one of the seven.
    pub fn parse(token: &str) -> Result<Self, UnknownDomain> {
        Self::ALL
            .into_iter()
            .find(|d| d.as_str() == token)
            .ok_or_else(|| UnknownDomain(token.to_owned()))
    }
}

/// An unknown hash domain — the closed-set refusal (spec 15). A plain data
/// carrier, NOT a `thiserror` enum: it never crosses a `NikaErrorCode`
/// boundary (the domain set is a construction-time programming invariant, the
/// same posture as `decide.rs`'s `DecideError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownDomain(pub String);

impl std::fmt::Display for UnknownDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown hash domain `{}` — the set is closed {:?}",
            self.0,
            HashDomain::ALL.map(HashDomain::as_str)
        )
    }
}

/// The JCS-shaped canonical byte string (sorted keys · no spaces · raw
/// UTF-8) — the ONE encoding both evaluators hash over.
///
/// This is byte-identical to the reference's
/// `json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)`:
/// `serde_json::to_string` emits a `Value`'s object keys in `BTreeMap` order
/// (this workspace does not enable `preserve_order`, so map order IS sorted
/// order · verified by `tests::canonical_is_sorted_keys`), compact, with
/// non-ASCII passed through raw. Serializing a `Value` cannot fail (string
/// keys · no NaN inhabitants) — the `""` fallback is unreachable.
///
/// **Idempotent**: `canonical(parse(canonical(x))) == canonical(x)` (spec 15
/// · property-pinned in `tests::canonical_is_idempotent`).
#[must_use]
pub fn canonical(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// The domain-separated pre-image bytes: `domain ‖ 0x00 ‖ format_version ‖
/// 0x00 ‖ JCS(ir)` (spec 15 · mirror of `proof_core.preimage`).
///
/// The NUL separators make the concatenation unambiguous — a domain can never
/// bleed into the version or the payload, and two payloads under different
/// domains never share a pre-image over identical `ir` bytes. Returns a
/// [`String`]: the canonical form is valid UTF-8 and NUL is a valid scalar,
/// so the whole pre-image is well-formed UTF-8 (hashed as
/// [`str::as_bytes`]).
#[must_use]
pub fn preimage(domain: HashDomain, format_version: u32, ir: &Value) -> String {
    format!(
        "{}\u{0}{format_version}\u{0}{}",
        domain.as_str(),
        canonical(ir)
    )
}

/// A blake3 hash over a domain pre-image, as lowercase hex — the engine-side
/// digest (the reference hashes the SAME pre-image with sha256; the parity is
/// on [`preimage`], never on this digest across algorithms).
#[must_use]
pub fn hash_in_domain(domain: HashDomain, format_version: u32, ir: &Value) -> String {
    blake3::hash(preimage(domain, format_version, ir).as_bytes())
        .to_hex()
        .to_string()
}

/// A workflow's semantic identity — `H(semantic ‖ format_version ‖ JCS(ir))`
/// under blake3 (spec 15 §the semantic hash · G13). The `ir` is the
/// desugared, versioned Semantic IR (the runtime-side `ir` projection); this is the key
/// cache and resume re-key on (a result is reused iff the semantic identity
/// matches · 14 §law 10).
#[must_use]
pub fn semantic_hash(ir: &Value, format_version: u32) -> SemanticHash {
    SemanticHash(hash_in_domain(HashDomain::Semantic, format_version, ir))
}

/// A semantic identity as lowercase blake3 hex — a typed wrapper so a
/// semantic hash is never confused with a raw content digest or a trace
/// chain-head at a call site (the `ChildRunSummary::def_hash` vs semantic
/// distinction · spec 14 law 10).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SemanticHash(String);

impl SemanticHash {
    /// The lowercase hex (64 chars · blake3-256).
    #[must_use]
    pub fn as_hex(&self) -> &str {
        &self.0
    }

    /// Consume into the owned hex string (receipt `proves` field).
    #[must_use]
    pub fn into_hex(self) -> String {
        self.0
    }
}

impl std::fmt::Display for SemanticHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    // The reference's shared fixtures (`proof_core_selftest.py`) — two
    // workflows that MEAN different things (one command byte apart).
    fn ir_a() -> Value {
        json!({"workflow": "w", "tasks": {"a": {"verb": "exec", "cmd": ["echo", "x"]}}})
    }
    fn ir_b() -> Value {
        json!({"workflow": "w", "tasks": {"a": {"verb": "exec", "cmd": ["echo", "y"]}}})
    }

    // ── canonical + hash (reference laws 1-5) ───────────────────────────

    #[test]
    fn canonical_is_sorted_keys_no_spaces() {
        // Reference: canonical({"b":1,"a":2}) == '{"a":2,"b":1}'.
        assert_eq!(canonical(&json!({"b": 1, "a": 2})), r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn canonical_is_idempotent() {
        // Reference law: canonical(parse(canonical(x))) == canonical(x).
        let once = canonical(&ir_a());
        let reparsed: Value = serde_json::from_str(&once).unwrap();
        assert_eq!(canonical(&reparsed), once);
    }

    #[test]
    fn distinct_irs_distinct_canonical_bytes() {
        assert_ne!(canonical(&ir_a()), canonical(&ir_b()));
    }

    #[test]
    fn distinct_irs_distinct_semantic_hash() {
        assert_ne!(
            semantic_hash(&ir_a(), FORMAT_VERSION),
            semantic_hash(&ir_b(), FORMAT_VERSION)
        );
    }

    #[test]
    fn same_ir_same_hash_deterministic() {
        assert_eq!(
            semantic_hash(&ir_a(), FORMAT_VERSION),
            semantic_hash(&ir_a(), FORMAT_VERSION)
        );
        assert_eq!(semantic_hash(&ir_a(), FORMAT_VERSION).as_hex().len(), 64);
    }

    // ── domain separation (reference laws 6-8) ──────────────────────────

    #[test]
    fn domain_separation_same_bytes_different_domain() {
        assert_ne!(
            preimage(HashDomain::Semantic, 1, &ir_a()),
            preimage(HashDomain::Trace, 1, &ir_a())
        );
    }

    #[test]
    fn an_unknown_domain_is_refused() {
        assert_eq!(HashDomain::parse("semantic"), Ok(HashDomain::Semantic));
        assert!(HashDomain::parse("made-up").is_err());
        // The seven parse round-trip through their wire spelling.
        for d in HashDomain::ALL {
            assert_eq!(HashDomain::parse(d.as_str()), Ok(d));
        }
    }

    #[test]
    fn format_version_participates_in_the_preimage() {
        assert_ne!(
            preimage(HashDomain::Semantic, 1, &ir_a()),
            preimage(HashDomain::Semantic, 2, &ir_a())
        );
        assert_ne!(
            hash_in_domain(HashDomain::Semantic, 1, &ir_a()),
            hash_in_domain(HashDomain::Semantic, 2, &ir_a())
        );
    }

    // ── the DIFFERENTIAL: pre-image bytes == the reference's goldens ─────
    //
    // These constants were produced BY the second evaluator
    // (`conformance/proof_core.py` · `preimage(...)` / `canonical(...)`),
    // pinned here as bytes. `\u{0}` is the NUL separator the reference
    // writes as `\x00`. If the engine's canonicalization ever drifts from
    // the reference's `json.dumps(sort_keys, separators, ensure_ascii=False)`
    // these break — the parity is on the bytes that get hashed, algorithm-
    // independent (spec 15 · the reference validates the PRE-IMAGE).

    #[test]
    fn canonical_matches_the_reference() {
        assert_eq!(
            canonical(&ir_a()),
            r#"{"tasks":{"a":{"cmd":["echo","x"],"verb":"exec"}},"workflow":"w"}"#
        );
        // Raw UTF-8 (ensure_ascii=False) — non-ASCII passes through unescaped.
        assert_eq!(
            canonical(&json!({"k": "héllo·wörld"})),
            "{\"k\":\"héllo·wörld\"}"
        );
    }

    #[test]
    fn preimage_matches_the_reference_goldens() {
        let a = ir_a();
        assert_eq!(
            preimage(HashDomain::Semantic, 1, &a),
            "semantic\u{0}1\u{0}{\"tasks\":{\"a\":{\"cmd\":[\"echo\",\"x\"],\"verb\":\"exec\"}},\"workflow\":\"w\"}"
        );
        assert_eq!(
            preimage(HashDomain::Trace, 1, &a),
            "trace\u{0}1\u{0}{\"tasks\":{\"a\":{\"cmd\":[\"echo\",\"x\"],\"verb\":\"exec\"}},\"workflow\":\"w\"}"
        );
        assert_eq!(
            preimage(HashDomain::Semantic, 2, &a),
            "semantic\u{0}2\u{0}{\"tasks\":{\"a\":{\"cmd\":[\"echo\",\"x\"],\"verb\":\"exec\"}},\"workflow\":\"w\"}"
        );
    }

    #[test]
    fn the_seven_domains_are_the_reference_set_in_order() {
        assert_eq!(
            HashDomain::ALL.map(HashDomain::as_str),
            [
                "source",
                "canonical",
                "semantic",
                "plan",
                "trace",
                "artifact",
                "receipt"
            ]
        );
    }
}
