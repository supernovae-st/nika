// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `receipt_format: 1` — the one receipt (spec 15 §the one receipt).
//!
//! A run's receipt folds FOUR things into one shape — the check certificate
//! (05 · attempts · effects · cost bound), the trace verdict (13 Outcome +
//! chain integrity), each `assert:` judged with its level, and the
//! `nika.lock` digest the run resolved under — and `proves` the run's
//! semantic hash ([`crate::proof::semantic_hash`]). The Decision Receipt
//! ([`nika_builtin::decide`] · `decision_receipt_format: 1`) and the registry
//! certificate become INSTANCES of this shape — one voice, three surfaces.
//!
//! The receipt is domain-separated ([`HashDomain::Receipt`]) and Merkle-linked
//! to the semantic hash it proves: given a receipt you can [`verify`] it
//! proves *this* workflow and no other.
//!
//! ## Parity (spec 15 · the second evaluator)
//!
//! [`build_receipt`] mirrors `proof_core.build_receipt` — the FOLD structure
//! is byte-equal on the pre-image ([`tests::receipt_fold_matches_the_reference`]
//! feeds both evaluators a FIXED `proves` string to isolate the fold from the
//! hash algorithm, since `proves` carries the algo-dependent digest). The
//! `digest` itself differs (the reference's sha256 vs this engine's blake3) —
//! the parity is on the bytes, never across algorithms.

use serde_json::{Map, Value};

use crate::proof::{HashDomain, hash_in_domain};

/// The v1 receipt format — the `receipt_format` field value AND the pre-image
/// format version (the reference's `RECEIPT_FORMAT`).
pub const RECEIPT_FORMAT: u32 = 1;

/// The receipt's own digest field key — computed over the fold, then added
/// (so the digest covers the six folded fields, never itself · the
/// reference's `receipt["digest"] = ...` after construction).
const DIGEST_KEY: &str = "digest";

/// Fold a run's proof into `receipt_format: 1` (spec 15 · mirror of
/// `proof_core.build_receipt`).
///
/// - `proves` — the run's semantic hash hex (the identity this receipt is
///   about · [`crate::proof::SemanticHash::as_hex`]).
/// - `certificate` — the check certificate (05).
/// - `trace_verdict` — the trace-verify result (13 Outcome + chain integrity).
/// - `assertions` — each `assert:` judged with its level.
/// - `lock_digest` — the `nika.lock` digest this run resolved under.
///
/// The returned value is the six folded fields plus a `digest` (blake3 over
/// the `receipt`-domain pre-image of the six). Canonicalization sorts the
/// keys, so field construction order never leaks into the digest.
#[must_use]
pub fn build_receipt(
    proves: &str,
    certificate: Value,
    trace_verdict: Value,
    assertions: Vec<Value>,
    lock_digest: &str,
) -> Value {
    // Build the fold by owned insert (the folded parts are MOVED in — the
    // certificate/verdict/assertions are consumed, not cloned).
    let mut obj = Map::new();
    obj.insert("receipt_format".to_owned(), Value::from(RECEIPT_FORMAT));
    obj.insert("proves".to_owned(), Value::String(proves.to_owned()));
    obj.insert("certificate".to_owned(), certificate);
    obj.insert("trace_verdict".to_owned(), trace_verdict);
    obj.insert("assertions".to_owned(), Value::Array(assertions));
    obj.insert(
        "lock_digest".to_owned(),
        Value::String(lock_digest.to_owned()),
    );
    // The digest covers the six folded fields, never itself (the reference's
    // `receipt["digest"] = ...` after construction · canonical sorts keys).
    let digest = hash_in_domain(
        HashDomain::Receipt,
        RECEIPT_FORMAT,
        &Value::Object(obj.clone()),
    );
    obj.insert(DIGEST_KEY.to_owned(), Value::String(digest));
    Value::Object(obj)
}

/// Verify a receipt proves a GIVEN semantic hash and no other (spec 15 · the
/// Merkle link): `proves` matches, AND the stored `digest` recomputes over the
/// six-field body. A tampered field (or a swapped `proves`) breaks the digest.
#[must_use]
pub fn verify(receipt: &Value, expected_semantic: &str) -> bool {
    let Some(obj) = receipt.as_object() else {
        return false;
    };
    if obj.get("proves").and_then(Value::as_str) != Some(expected_semantic) {
        return false;
    }
    let Some(stored) = obj.get(DIGEST_KEY).and_then(Value::as_str) else {
        return false;
    };
    // Recompute over the body WITHOUT the digest (the fold's pre-image).
    let mut body = obj.clone();
    body.remove(DIGEST_KEY);
    let recomputed = hash_in_domain(HashDomain::Receipt, RECEIPT_FORMAT, &Value::Object(body));
    recomputed == stored
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::proof::{FORMAT_VERSION, semantic_hash};

    fn sample() -> Value {
        build_receipt(
            "blake3:fixedsem",
            json!({"attempts": 1}),
            json!({"outcome": "success"}),
            vec![json!({"assert": "no_secret_egress", "level": "StaticProof"})],
            "blake3:lock",
        )
    }

    #[test]
    fn the_receipt_proves_its_semantic_hash() {
        let sem = semantic_hash(&json!({"workflow": "w"}), FORMAT_VERSION);
        let r = build_receipt(
            sem.as_hex(),
            json!({"attempts": 1}),
            json!({"outcome": "success"}),
            vec![],
            "blake3:lock",
        );
        assert_eq!(r["proves"], json!(sem.as_hex()));
        assert_eq!(r["receipt_format"], json!(1));
    }

    #[test]
    fn the_receipt_is_self_digesting_and_verifies() {
        let r = sample();
        let digest = r["digest"].as_str().expect("a digest");
        assert_eq!(digest.len(), 64, "blake3 hex");
        assert!(
            verify(&r, "blake3:fixedsem"),
            "an untampered receipt verifies"
        );
    }

    #[test]
    fn a_tampered_receipt_or_swapped_proof_is_rejected() {
        let r = sample();
        // Wrong expected semantic → the Merkle link refuses.
        assert!(!verify(&r, "blake3:someother"));
        // Tamper a folded field → the digest no longer recomputes.
        let mut tampered = r.as_object().unwrap().clone();
        tampered.insert("lock_digest".to_owned(), json!("blake3:forged"));
        assert!(!verify(&Value::Object(tampered), "blake3:fixedsem"));
    }

    /// The FOLD-structure differential: the receipt pre-image (the six folded
    /// fields, `proves` fixed to isolate the hash algo) is byte-equal to the
    /// reference's `preimage("receipt", 1, receipt)` — the golden produced by
    /// `conformance/proof_core.py`.
    #[test]
    fn receipt_fold_matches_the_reference() {
        let r = sample();
        let mut body = r.as_object().unwrap().clone();
        body.remove(DIGEST_KEY);
        let canon = crate::proof::canonical(&Value::Object(body));
        assert_eq!(
            canon,
            r#"{"assertions":[{"assert":"no_secret_egress","level":"StaticProof"}],"certificate":{"attempts":1},"lock_digest":"blake3:lock","proves":"blake3:fixedsem","receipt_format":1,"trace_verdict":{"outcome":"success"}}"#
        );
        assert_eq!(
            format!("receipt\u{0}1\u{0}{canon}"),
            "receipt\u{0}1\u{0}{\"assertions\":[{\"assert\":\"no_secret_egress\",\"level\":\"StaticProof\"}],\"certificate\":{\"attempts\":1},\"lock_digest\":\"blake3:lock\",\"proves\":\"blake3:fixedsem\",\"receipt_format\":1,\"trace_verdict\":{\"outcome\":\"success\"}}"
        );
    }
}
