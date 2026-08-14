// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `receipt_format: 1` — the one receipt (spec 15 §the one receipt).
//!
//! A run's receipt folds FOUR things into one shape — the check certificate
//! (05 · attempts · effects · cost bound), the trace verdict (13 Outcome +
//! chain integrity), each `assert:` judged with its level, and the
//! `nika.lock` digest the run resolved under — and `proves` the run's
//! semantic hash ([`crate::semantic_hash`]). The Decision Receipt
//! (`nika_builtin::decide` · `decision_receipt_format: 1`) and the registry
//! certificate become INSTANCES of this shape — one voice, three surfaces.
//!
//! The receipt is domain-separated ([`HashDomain::Receipt`]) and Merkle-linked
//! to the semantic hash it proves: given a receipt you can [`verify`] it
//! proves *this* workflow and no other.
//!
//! ## Parity (spec 15 · the second evaluator)
//!
//! [`build_receipt`] mirrors `proof_core.build_receipt` — the FOLD structure
//! is byte-equal on the pre-image (`tests::receipt_fold_matches_the_reference`
//! feeds both evaluators a FIXED `proves` string to isolate the fold from the
//! hash algorithm, since `proves` carries the algo-dependent digest). The
//! `digest` itself differs (the reference's sha256 vs this engine's blake3) —
//! the parity is on the bytes, never across algorithms.

use nika_check::RunCertificate;
use serde_json::{Map, Value};

use crate::{HashDomain, SemanticHash, hash_in_domain};

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
///   about · [`crate::SemanticHash::as_hex`]).
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

/// Fold a run's receipt from the engine's OWN typed pieces (spec 15 · the one
/// receipt · the parent shape's FIRST real instance): the check certificate
/// (nika-schema [`RunCertificate`] · attempts · effects · cost bound), the
/// semantic hash it proves ([`SemanticHash`]), the trace verdict, and the
/// `nika.lock` digest the run resolved under.
///
/// The `assertions` section is EMPTY BY CONSTRUCTION since the `assert:`
/// block died with the 9-key envelope, and it renders `none` rather than
/// `0 judged` — a count of zero reads as a judgment that came back empty,
/// when in truth nothing was ever judged.
///
/// This is the honest realization the spec calls for: the parent shape plus
/// ONE real instance folding the engine's actual certificate. Fully
/// unifying the Decision Receipt (`decide.rs` `decision_receipt_format: 1`)
/// and the registry certificate INTO instances of this shape is the named
/// owed — the shape is here; the two other surfaces adopt it next.
#[must_use]
pub fn build_run_receipt(
    proves: &SemanticHash,
    certificate: &RunCertificate,
    trace_verdict: Value,
    lock_digest: &str,
) -> Value {
    // The certificate is Serialize (nika-schema) — fold it as-is.
    let cert = serde_json::to_value(certificate).unwrap_or(Value::Null);
    let assertions: Vec<Value> = Vec::new();
    build_receipt(
        proves.as_hex(),
        cert,
        trace_verdict,
        assertions,
        lock_digest,
    )
}

// ── F-P16 · the readable receipt (NEP-0014 law 3) ────────────────────
//
// Every field of the receipt schema carries its human-readable
// projection HERE — in the schema's own home, so a new field without a
// projection is REFUSED by the ratchet (`unprojected_fields` · the
// schema-level gate, exercised by this module's tests). The projection
// is NEVER the evidence: `explain_receipt` is a READING (the digest is
// not recomputed), [`verify`] is the proof — the two never share a
// trust level.

/// The fixed dotted-path projections (top-level receipt fields · the
/// certificate's own fields · the engine-known `trace_verdict` keys ·
/// the assertion rows). The four `Bound` axes share their sub-field
/// sentences via [`projection_of`].
const FIXED_PROJECTIONS: &[(&str, &str)] = &[
    ("receipt_format", "the receipt schema version (spec 15)"),
    (
        "proves",
        "the workflow identity this receipt attests (the semantic hash)",
    ),
    (
        "certificate",
        "the check-time resource certificate folded into this receipt",
    ),
    (
        "certificate.task_attempts",
        "upper bound on task-body executions (attempts × fan-out)",
    ),
    (
        "certificate.llm_calls",
        "upper bound on LLM calls (infer + agent turns)",
    ),
    (
        "certificate.effect_calls",
        "upper bound on effect calls (exec + invoke dispatches)",
    ),
    (
        "certificate.usd_micros",
        "the parametric spend bound in micro-USD (absent when unpriceable)",
    ),
    (
        "certificate.span_attempts",
        "the longest sequential dependency chain, in attempts (the span)",
    ),
    (
        "certificate.derivation",
        "the per-task witness rows the audit re-checks",
    ),
    (
        "certificate.derivation.task",
        "the task this witness row derives",
    ),
    (
        "certificate.derivation.deps",
        "the task's dependencies (the DAG edges the span folds over)",
    ),
    (
        "certificate.derivation.attempts",
        "the retry: max_attempts (1 when absent)",
    ),
    (
        "certificate.derivation.fanout",
        "the fan-out shape (a known multiplier · a runtime collection)",
    ),
    (
        "certificate.derivation.fanout.known",
        "the known multiplier of a literal/plain fan-out",
    ),
    (
        "certificate.derivation.main_llm",
        "LLM calls per body run (main action)",
    ),
    (
        "certificate.derivation.main_effect",
        "effect calls per body run (main action)",
    ),
    (
        "certificate.derivation.main_spend_micros",
        "spend per body run in micro-USD (null = unpriceable)",
    ),
    (
        "certificate.derivation.finally_llm",
        "LLM calls per iteration from on_finally cleanups",
    ),
    (
        "certificate.derivation.finally_effect",
        "effect calls per iteration from on_finally cleanups",
    ),
    (
        "certificate.derivation.finally_spend_micros",
        "on_finally spend per iteration (null = unpriceable)",
    ),
    (
        "certificate.effects",
        "the authority projection (spec 10 · re-derived at audit, never trusted)",
    ),
    (
        "certificate.effects.boundary_declared",
        "whether the workflow declares a permits: boundary",
    ),
    (
        "certificate.effects.needed",
        "the tightest boundary the body statically needs (the --infer-permits derivation)",
    ),
    (
        "certificate.effects.escapes",
        "statically-detected effects outside the declared boundary",
    ),
    (
        "trace_verdict",
        "the trace-verify result folded into this receipt",
    ),
    ("trace_verdict.outcome", "the run's terminal outcome"),
    (
        "trace_verdict.chain",
        "the journal chain status the verdict names",
    ),
    ("trace_verdict.events", "the journaled event count"),
    ("trace_verdict.head", "the chain head the verdict names"),
    (
        "trace_verdict.sealed",
        "whether the journal is sealed (attributable)",
    ),
    (
        "assertions",
        "the assert: obligations, each judged at its honest level",
    ),
    (
        "assertions.assert",
        "the asserted property (the closed spec-15 vocabulary)",
    ),
    (
        "assertions.level",
        "the level the evidence supports (StaticProof · …)",
    ),
    (
        "lock_digest",
        "the nika.lock digest the run resolved under (unrecorded when the journal never carried it)",
    ),
    (
        "digest",
        "the self-binding digest — verify recomputes it over the folded fields",
    ),
];

/// The four `Bound` axes of the certificate — their sub-fields share
/// the degree-1-polynomial sentences (the axis row itself projects from
/// [`FIXED_PROJECTIONS`]).
const BOUND_AXES: &[&str] = &["task_attempts", "llm_calls", "effect_calls", "usd_micros"];

/// The shared sub-field projections under one `Bound` axis
/// (`certificate.<axis>.<suffix>`).
const BOUND_SUFFIX_PROJECTIONS: &[(&str, &str)] = &[
    ("constant", "the constant part of a degree-1 bound"),
    (
        "terms",
        "the parametric terms (coeff × a task's for_each size)",
    ),
    (
        "terms.task",
        "the task whose for_each collection size parameterizes the term",
    ),
    ("terms.coeff", "the multiplier on that size"),
];

/// Paths whose projection covers the WHOLE subtree — the dynamic-key
/// objects (projecting each runtime-named key would be a guess; the
/// container sentence says what the map IS).
const LEAF_PATHS: &[&str] = &["certificate.effects.needed"];

/// The human-readable projection of one receipt field, by dotted path
/// (`certificate.task_attempts.constant` · `assertions.assert` · …) —
/// `None` for a field the schema does not know (the ratchet's refuse).
#[must_use]
pub fn projection_of(path: &str) -> Option<&'static str> {
    if let Some(hit) = FIXED_PROJECTIONS.iter().find(|(key, _)| *key == path) {
        return Some(hit.1);
    }
    if let Some(rest) = path.strip_prefix("certificate.")
        && let Some((axis, suffix)) = rest.split_once('.')
        && BOUND_AXES.contains(&axis)
    {
        return BOUND_SUFFIX_PROJECTIONS
            .iter()
            .find(|(key, _)| *key == suffix)
            .map(|(_, sentence)| *sentence);
    }
    None
}

/// The ratchet walk (F-P16 law 3): every dotted key path of `receipt`
/// that carries NO projection — a new field without one is REFUSED here
/// (the schema-level gate: this list must be empty for a lawful
/// receipt). Leaf paths (`LEAF_PATHS`) stop the descent — their
/// projection covers the whole dynamic subtree.
#[must_use]
pub fn unprojected_fields(receipt: &Value) -> Vec<String> {
    let mut missing = Vec::new();
    walk_fields(receipt, "", &mut missing);
    missing
}

/// The recursive half of [`unprojected_fields`] — array elements extend
/// the container's path (an assertions row's `assert` projects as
/// `assertions.assert`).
fn walk_fields(value: &Value, prefix: &str, missing: &mut Vec<String>) {
    if LEAF_PATHS
        .iter()
        .any(|leaf| prefix == *leaf || prefix.starts_with(&format!("{leaf}.")))
    {
        return;
    }
    if let Value::Object(map) = value {
        for (key, child) in map {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            if projection_of(&path).is_none() {
                missing.push(path.clone());
            }
            walk_fields(child, &path, missing);
        }
    } else if let Value::Array(items) = value {
        for item in items {
            walk_fields(item, prefix, missing);
        }
    }
}

/// Render a receipt's readable projection as STABLE text (F-P16 ·
/// NEP-0014 law 3) — one row per field, the schema's own sentence
/// beside the value, in the schema's fixed order. A READING, never a
/// proof: the digest is not recomputed here (`verify` owns the proof),
/// and an unprojected field is named, never silently dropped.
#[must_use]
pub fn explain_receipt(receipt: &Value) -> String {
    use std::fmt::Write as _;
    let mut out = String::from(
        "receipt · the one run receipt (receipt_format 1)\n  \
         a READING, never a proof — the digest is NOT recomputed here · `verify` is the proof\n",
    );
    let Value::Object(map) = receipt else {
        out.push_str("\nnot a receipt — expected a JSON object");
        return out;
    };
    for (index, key) in ["proves", "digest", "lock_digest"].iter().enumerate() {
        if let Some(value) = map.get(*key) {
            let lead = if index == 0 { "\n" } else { "" };
            let _ = writeln!(
                out,
                "{lead}{key:<12} {} — {}",
                scalar_text(value),
                projection_of(key).unwrap_or("·")
            );
        }
    }
    if let Some(certificate) = map.get("certificate") {
        explain_certificate(&mut out, certificate);
    }
    if let Some(verdict) = map.get("trace_verdict").and_then(Value::as_object) {
        let _ = writeln!(
            out,
            "\ntrace_verdict — {}",
            projection_of("trace_verdict").unwrap_or("·")
        );
        for key in ["outcome", "chain", "events", "head", "sealed"] {
            if let Some(value) = verdict.get(key) {
                let path = format!("trace_verdict.{key}");
                let _ = writeln!(
                    out,
                    "  {key:<10} {} — {}",
                    scalar_text(value),
                    projection_of(&path).unwrap_or("·")
                );
            }
        }
    }
    if let Some(assertions) = map.get("assertions").and_then(Value::as_array) {
        // « none » rather than « 0 judged »: since the `assert:` key died
        // (2026-08-13) nothing can mint an obligation, and a count of zero
        // reads like a judgment that came back empty. It never ran.
        let _ = if assertions.is_empty() {
            writeln!(
                out,
                "\nassertions  none — no obligation was claimed (the `assert:` key died 2026-08-13)"
            )
        } else {
            writeln!(
                out,
                "\nassertions  {} judged — {}",
                assertions.len(),
                projection_of("assertions").unwrap_or("·")
            )
        };
        for entry in assertions {
            let assert = entry["assert"].as_str().unwrap_or("?");
            let level = entry["level"].as_str().unwrap_or("?");
            let _ = writeln!(out, "  {assert}  {level}");
        }
    }
    let missing = unprojected_fields(receipt);
    if !missing.is_empty() {
        let _ = writeln!(
            out,
            "\nunprojected fields (the schema refuses them): {}",
            missing.join(" · ")
        );
    }
    out
}

/// The certificate section of [`explain_receipt`] — the four `Bound`
/// axes rendered as `≤ constant + coeff·|task|`, the span, the witness
/// row count, and the authority summary.
fn explain_certificate(out: &mut String, certificate: &Value) {
    use std::fmt::Write as _;
    let _ = writeln!(
        out,
        "\ncertificate — {}",
        projection_of("certificate").unwrap_or("·")
    );
    for axis in BOUND_AXES {
        let path = format!("certificate.{axis}");
        let _ = writeln!(
            out,
            "  {axis:<14} {} — {}",
            bound_text(certificate.get(*axis)),
            projection_of(&path).unwrap_or("·")
        );
    }
    let span = certificate["span_attempts"].as_u64().unwrap_or(0);
    let _ = writeln!(
        out,
        "  span_attempts  {span} — {}",
        projection_of("certificate.span_attempts").unwrap_or("·")
    );
    let rows = certificate["derivation"].as_array().map_or(0, Vec::len);
    let _ = writeln!(
        out,
        "  derivation     {rows} witness rows — {}",
        projection_of("certificate.derivation").unwrap_or("·")
    );
    let effects = &certificate["effects"];
    let declared = effects["boundary_declared"].as_bool().unwrap_or(false);
    let escapes = effects["escapes"].as_u64().unwrap_or(0);
    let _ = writeln!(
        out,
        "  effects        boundary {} · {escapes} escapes — {}",
        if declared { "declared" } else { "undeclared" },
        projection_of("certificate.effects").unwrap_or("·")
    );
}

/// The one-line form of a `Bound` value (`≤ 5 + 2·|fan|` ·
/// `unpriceable` for the absent spend axis).
fn bound_text(bound: Option<&Value>) -> String {
    use std::fmt::Write as _;
    let Some(bound) = bound else {
        return "unpriceable".to_owned();
    };
    if bound.is_null() {
        return "unpriceable".to_owned();
    }
    let constant = bound["constant"].as_u64().unwrap_or(0);
    let mut text = format!("≤ {constant}");
    if let Some(terms) = bound["terms"].as_array() {
        for term in terms {
            let task = term["task"].as_str().unwrap_or("?");
            let coeff = term["coeff"].as_u64().unwrap_or(0);
            let _ = write!(text, " + {coeff}·|{task}|");
        }
    }
    text
}

/// The stable scalar render (strings ESCAPED · numbers/bools plain ·
/// null named). A receipt is an untrusted artifact (F-P16 · NEP-0012
/// law 2): a string cloned raw into the explain output would reach the
/// TTY with its control bytes live (the OSC52 / terminal-escape
/// class), so the string arm routes through the escape. The READING
/// posture is unaffected — `verify` owns the proof and never reads
/// this render.
fn scalar_text(value: &Value) -> String {
    match value {
        Value::String(s) => escape_tty(s),
        Value::Null => "null".to_owned(),
        other => other.to_string(),
    }
}

/// Strip every control character (C0 · C1 · DEL — ESC dies, so the
/// OSC52 clipboard class dies) from an artifact-originated string
/// before it reaches a terminal. The local twin of
/// `nika_dap::escape_tty`, duplicated deliberately: that one-voice
/// helper lives at L4, and this L0 crate cannot depend on an interface
/// crate — the semantics are identical and both are pinned by test.
fn escape_tty(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{FORMAT_VERSION, semantic_hash};

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
        let canon = crate::canonical(&Value::Object(body));
        assert_eq!(
            canon,
            r#"{"assertions":[{"assert":"no_secret_egress","level":"StaticProof"}],"certificate":{"attempts":1},"lock_digest":"blake3:lock","proves":"blake3:fixedsem","receipt_format":1,"trace_verdict":{"outcome":"success"}}"#
        );
        assert_eq!(
            format!("receipt\u{0}1\u{0}{canon}"),
            "receipt\u{0}1\u{0}{\"assertions\":[{\"assert\":\"no_secret_egress\",\"level\":\"StaticProof\"}],\"certificate\":{\"attempts\":1},\"lock_digest\":\"blake3:lock\",\"proves\":\"blake3:fixedsem\",\"receipt_format\":1,\"trace_verdict\":{\"outcome\":\"success\"}}"
        );
    }

    // The ONE-real-instance receipt test (a run receipt folding the
    // engine's OWN certificate + the workflow's semantic IR hash) lives
    // with the `ir` projection — nika-runtime's `proof::ir` tests, since
    // the projection stayed with the runtime's resume family.

    /// F-P16 · NEP-0012 law 2: a receipt is an UNTRUSTED artifact — its
    /// strings reach the explain TTY escaped (ESC dies, so the OSC52
    /// clipboard class dies), never live.
    #[test]
    fn explain_escapes_control_chars_in_artifact_strings() {
        let mut r = sample();
        r["trace_verdict"]["outcome"] = json!("success\u{1b}]52;;evil\u{7}\nforged-line");
        let out = explain_receipt(&r);
        assert!(
            !out.contains('\u{1b}'),
            "no live ESC reaches the TTY: {out:?}"
        );
        assert!(!out.contains('\u{7}'), "no live BEL either: {out:?}");
        // The escape STRIPS (it does not mangle): the visible text
        // survives on one line, control bytes gone.
        assert!(
            out.contains("success]52;;evilforged-line"),
            "the text survives, stripped: {out:?}"
        );
        // And the escape helper's own edges (the nika_dap twin's pin).
        assert_eq!(escape_tty("plain-key_1234"), "plain-key_1234");
        assert_eq!(escape_tty("\u{7f}\u{9b}"), "");
    }
}
