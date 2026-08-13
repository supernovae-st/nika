// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Semantic IR projection — Merkle by task (spec 15 §the semantic hash).
//!
//! A workflow's identity is the hash of its desugared, versioned Semantic IR,
//! built **Merkle by task**: each task's semantic subtree hashes
//! independently (the leaf), and the workflow hash commits to the leaf hashes
//! (the root). A proof of the whole contains a proof of each part — and a
//! composed child (spec 14) folds in as a subtree under its invoking task
//! (the `ChildRunSummary` semantic anchor · law 10).
//!
//! ## What this projection IS today (honest scope · spec 15 · G13)
//!
//! The per-task leaf reuses the ADR-099 `definition_value` — the span-free,
//! behavior-bearing definition as WRITTEN (verb body · `with:` · `output:` ·
//! `retry:`/`on_error:`/`on_finally:` · `when:` · `for_each:` · the
//! scheduling knobs). That projection already canonicalizes authored map
//! order away (spec 15 · two files that MEAN the same task lower to the same
//! leaf · proven by the resume suite's `with_declaration_order_is_canonicalized_away`).
//!
//! ## What this projection does NOT yet do (owed · named, not simulated)
//!
//! The FULL spec-15 lowering — normative-default expansion, unit
//! normalization (durations → ns · sizes → bytes), Unicode NFC, reference
//! resolution — is the owed deepening. Two files that differ ONLY in an
//! un-expanded default or an un-normalized unit are semantically equal but
//! would hash differently here; that gap is `specified`, not `proven`. The
//! canonicalization DISCIPLINE (JCS sorted keys · blake3 · domain-separated)
//! is exact; the IR's completeness is the deepening the distribution window
//! extends.

use std::collections::BTreeMap;

use nika_schema::raw::{RawTask, RawWorkflow};
use serde_json::{Value, json};

use crate::proof::{FORMAT_VERSION, SemanticHash, semantic_hash};
use crate::resume::definition_value;

/// A workflow's Merkle proof by task (spec 15 §Merkle by task): the leaf
/// hashes and the root that commits to them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkflowProof {
    /// The workflow's semantic identity — the hash of the IR that commits to
    /// every task leaf hash (the Merkle root · spec 15 · G13).
    pub workflow: SemanticHash,
    /// Each task's semantic subtree hash, by task id (the leaves). A
    /// composed child's own [`WorkflowProof::workflow`] is the subtree that
    /// folds in under its invoking task.
    pub tasks: BTreeMap<String, SemanticHash>,
}

/// One task's semantic subtree hash — `H_semantic` over the leaf payload
/// `{task, verb, definition}` (the ADR-099 definition, generalized to the
/// proof domain). `None` when the task carries a `#[non_exhaustive]` form the
/// projection does not know (honest degradation — no false identity, ever).
#[must_use]
pub fn task_semantic_hash(task: &RawTask) -> Option<SemanticHash> {
    let leaf = task_leaf(task)?;
    Some(semantic_hash(&leaf, FORMAT_VERSION))
}

/// The per-task leaf IR — the behavior-bearing definition wrapped with its id
/// and verb, so two distinct tasks never share a leaf even with identical
/// bodies (the resume `def_hash` payload shape, in the proof domain).
fn task_leaf(task: &RawTask) -> Option<Value> {
    Some(json!({
        "task": task.id.value,
        "verb": task.action.verb(),
        "definition": definition_value(task)?,
    }))
}

/// The Merkle proof: hash each task independently, then a root that COMMITS
/// to the leaf hashes (spec 15 · "the workflow hash commits to the task
/// hashes"). `None` when any task is not projectable — the whole workflow is
/// then not semantically hashable (honest, never a partial identity).
#[must_use]
pub fn merkle_by_task(wf: &RawWorkflow) -> Option<WorkflowProof> {
    let mut tasks: BTreeMap<String, SemanticHash> = BTreeMap::new();
    for spanned in &wf.tasks {
        let task = &spanned.value;
        let hash = task_semantic_hash(task)?;
        tasks.insert(task.id.value.clone(), hash);
    }
    let root_ir = root_ir(wf, &tasks);
    Some(WorkflowProof {
        workflow: semantic_hash(&root_ir, FORMAT_VERSION),
        tasks,
    })
}

/// The root IR the workflow hash is taken over — the workflow id plus the map
/// of task id → leaf hash hex (the Merkle commitment). Canonicalization sorts
/// the task map, so authored task order never leaks into the identity.
fn root_ir(wf: &RawWorkflow, tasks: &BTreeMap<String, SemanticHash>) -> Value {
    let leaves: serde_json::Map<String, Value> = tasks
        .iter()
        .map(|(id, h)| (id.clone(), Value::String(h.as_hex().to_owned())))
        .collect();
    json!({
        "workflow": wf.workflow.as_ref().map(|w| w.value.clone()),
        "tasks": leaves,
    })
}

/// The whole-workflow semantic identity (the Merkle root) — the convenience
/// accessor when only the root hash is wanted (cache/resume re-key · spec 15).
/// `None` on an unprojectable task (honest degradation).
#[must_use]
pub fn semantic_ir_hash(wf: &RawWorkflow) -> Option<SemanticHash> {
    merkle_by_task(wf).map(|proof| proof.workflow)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    const BASE: &str = "nika: demo\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n  b:\n    with: { p: \"${{ tasks.a.output }}\" }\n    exec: { command: [\"echo\", \"${{ with.p }}\"] }\n";

    #[test]
    fn the_root_commits_to_every_task_leaf() {
        let wf = parse(BASE);
        let proof = merkle_by_task(&wf).expect("projectable");
        assert_eq!(proof.tasks.len(), 2, "one leaf per task");
        assert!(proof.tasks.contains_key("a") && proof.tasks.contains_key("b"));
        for h in proof.tasks.values() {
            assert_eq!(h.as_hex().len(), 64, "blake3 hex leaf");
        }
        assert_eq!(proof.workflow.as_hex().len(), 64, "blake3 hex root");
    }

    #[test]
    fn a_changed_task_moves_its_leaf_and_the_root() {
        let a = merkle_by_task(&parse(BASE)).expect("projectable");
        let edited = BASE.replace("echo\", \"x", "echo\", \"z");
        let b = merkle_by_task(&parse(&edited)).expect("projectable");
        assert_ne!(a.tasks["a"], b.tasks["a"], "the edited task's leaf moves");
        assert_eq!(
            a.tasks["b"], b.tasks["b"],
            "the untouched task's leaf holds"
        );
        assert_ne!(a.workflow, b.workflow, "the root commits to the moved leaf");
    }

    #[test]
    fn semantically_equal_respellings_share_the_identity() {
        // Two files that MEAN the same workflow (authored `with:` order is not
        // behavior · spec 15) lower to the SAME semantic hash — semantic, not
        // textual. This is the property the cache/resume re-key rides.
        const AB: &str = "nika: demo\ntasks:\n  t:\n    with: { a: \"1\", b: \"2\" }\n    exec: { command: [\"echo\", \"${{ with.a }}${{ with.b }}\"] }\n";
        const BA: &str = "nika: demo\ntasks:\n  t:\n    with: { b: \"2\", a: \"1\" }\n    exec: { command: [\"echo\", \"${{ with.a }}${{ with.b }}\"] }\n";
        let ab = semantic_ir_hash(&parse(AB)).expect("projectable");
        let ba = semantic_ir_hash(&parse(BA)).expect("projectable");
        assert_eq!(ab, ba, "authored map order is not semantic identity");
    }

    #[test]
    fn task_order_does_not_change_the_root() {
        const AB: &str = "nika: demo\ntasks:\n  a:\n    exec: { command: [\"echo\", \"1\"] }\n  b:\n    exec: { command: [\"echo\", \"2\"] }\n";
        const BA: &str = "nika: demo\ntasks:\n  b:\n    exec: { command: [\"echo\", \"2\"] }\n  a:\n    exec: { command: [\"echo\", \"1\"] }\n";
        assert_eq!(
            semantic_ir_hash(&parse(AB)).expect("projectable"),
            semantic_ir_hash(&parse(BA)).expect("projectable"),
            "the root sorts the task map — authored order is not behavior"
        );
    }

    #[test]
    fn the_workflow_id_participates_in_the_root() {
        let a = semantic_ir_hash(&parse(BASE)).expect("projectable");
        let renamed = BASE.replace("nika: demo", "nika: other");
        let b = semantic_ir_hash(&parse(&renamed)).expect("projectable");
        assert_ne!(a, b, "the workflow id is part of the identity");
    }

    /// The ONE real instance: a receipt folded from the engine's OWN typed
    /// pieces — a real `RunCertificate` (from an actually-checked workflow),
    /// the workflow's real semantic hash (the Merkle root), and an `assert:`
    /// obligation judged at its honest level. The receipt proves THIS
    /// workflow's identity and verifies. (Moved here with the `ir`
    /// projection when the receipt/hash primitives split to `nika-proof` —
    /// the projection stayed with the runtime's resume family.)
    #[test]
    fn a_run_receipt_folds_the_engine_certificate_and_verifies() {
        use nika_proof::receipt::{build_run_receipt, verify};
        use serde_json::json;

        let wf = parse("nika: pay\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n");
        let report = nika_check::check(&wf);
        let proves = semantic_ir_hash(&wf).expect("projectable");

        let receipt = build_run_receipt(
            &proves,
            &report.certificate,
            json!({ "outcome": "success" }),
            "blake3:lockdigest",
        );

        // The receipt proves THIS workflow's semantic hash and verifies.
        assert!(
            verify(&receipt, proves.as_hex()),
            "the run receipt verifies"
        );
        // The engine's real certificate is folded in (attempts · effects · bound).
        assert!(
            receipt["certificate"].is_object(),
            "the RunCertificate is folded, not a placeholder"
        );
        // No obligation can be minted since `assert:` died — the array
        // is present (readers index it) and empty (nothing was claimed).
        assert_eq!(receipt["assertions"], json!([]));
        assert_eq!(receipt["lock_digest"], json!("blake3:lockdigest"));
        // It does NOT verify against a different workflow's identity.
        assert!(!verify(&receipt, "blake3:someotherworkflow"));
    }

    // ── F-P16 · the readable receipt (NEP-0014 law 3) ─────────────────
    // (these live with the `ir` tests: the rich fixture rides
    //  `semantic_ir_hash`, which stayed here with the resume family)

    /// The rich fixture's YAML — one const so the receipt, the proves
    /// and the golden can never drift apart.
    const RICH_YAML: &str = "nika: pay\nmodel: anthropic/claude-sonnet-4-6\npermits:\n  exec: [\"ls\", \"echo\"]\n  tools: [\"nika:log\"]\ntasks:\n  src:\n    exec: { command: [\"ls\"] }\n  fan:\n    with: { items: \"${{ tasks.src.output.files }}\" }\n    for_each: { items: \"${{ with.items }}\" }\n    retry: { max_attempts: 2 }\n    infer: { prompt: \"x ${{ item }}\", max_tokens: 200 }\n  fan_cleanup:\n    after: { fan: unwind }\n    invoke: { tool: \"nika:log\", args: { message: \"done\" } }\n";

    /// A receipt whose certificate exercises EVERY field family: a
    /// `for_each` expression (parametric terms) · a retry · a declared
    /// boundary (the effects projection) · an `on_finally` cleanup.
    fn rich_receipt() -> Value {
        use nika_proof::receipt::build_run_receipt;
        use serde_json::json;

        let wf = parse(RICH_YAML);
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "the fixture is clean: {report:?}");
        let proves = semantic_ir_hash(&wf).expect("projectable");
        build_run_receipt(
            &proves,
            &report.certificate,
            json!({
                "outcome": "completed",
                "chain": "intact",
                "events": 7,
                "head": "ab12",
                "sealed": true,
            }),
            "blake3:lockdigest",
        )
    }

    /// THE law: every field of a real receipt carries its projection —
    /// the walk over every dotted key path (the certificate's bounds ·
    /// the witness rows · the effects · the verdict · the assertions)
    /// finds ZERO unprojected fields.
    #[test]
    fn every_field_of_a_real_receipt_carries_a_projection() {
        use nika_proof::receipt::{projection_of, unprojected_fields};

        let receipt = rich_receipt();
        let missing = unprojected_fields(&receipt);
        assert!(
            missing.is_empty(),
            "fields without a projection (the schema refuses them): {missing:?}"
        );
        // …and the walk is not vacuous: the receipt HAS the families.
        for family in [
            "certificate.task_attempts.terms.task",
            "certificate.derivation.finally_effect",
            "certificate.effects.boundary_declared",
            "trace_verdict.sealed",
            "assertions.level",
            "digest",
        ] {
            assert!(
                projection_of(family).is_some(),
                "{family} projects: {}",
                projection_of(family).unwrap_or("∅")
            );
        }
    }

    /// The refuse: a field WITHOUT a projection is caught by the walk —
    /// this is what keeps a future schema edit from shipping a field no
    /// one can read (the ratchet turns red, the schema is refused).
    #[test]
    fn a_field_without_a_projection_is_refused() {
        use nika_proof::receipt::unprojected_fields;
        use serde_json::json;

        let mut receipt = rich_receipt();
        receipt
            .as_object_mut()
            .unwrap()
            .insert("mystery".to_owned(), json!(1));
        receipt["certificate"]
            .as_object_mut()
            .unwrap()
            .insert("unbounded".to_owned(), json!(true));
        let missing = unprojected_fields(&receipt);
        assert_eq!(
            missing,
            vec!["certificate.unbounded".to_owned(), "mystery".to_owned()],
            "both unprojected fields are named"
        );
    }

    /// The dynamic subtree posture: `effects.needed` projects its
    /// CONTAINER sentence and the walk never descends into the
    /// runtime-named keys (projecting each would be a guess).
    #[test]
    fn the_dynamic_needed_map_is_a_leaf() {
        use nika_proof::receipt::{projection_of, unprojected_fields};
        use serde_json::json;

        assert!(projection_of("certificate.effects.needed").is_some());
        let receipt = json!({
            "certificate": { "effects": { "needed": { "exec": ["git"], "fs": { "read": ["./x"] } } } },
        });
        assert!(
            unprojected_fields(&receipt).is_empty(),
            "the leaf covers the whole subtree"
        );
    }

    /// The stable render — golden: the same receipt always renders the
    /// same text (the 3-OS law: no paths, no colours, no clock), and the
    /// header says what explain IS (a reading · never the proof).
    #[test]
    fn explain_renders_the_stable_projection() {
        use nika_proof::receipt::{build_receipt, explain_receipt};
        use serde_json::json;

        let receipt = build_receipt(
            "blake3:fixedsem",
            json!({"attempts": 1}),
            json!({"outcome": "success"}),
            vec![json!({"assert": "no_secret_egress", "level": "StaticProof"})],
            "blake3:lock",
        );
        let text = explain_receipt(&receipt);
        // The reading-not-proof header, always first.
        assert!(
            text.starts_with(
                "receipt · the one run receipt (receipt_format 1)\n  a READING, never a proof"
            ),
            "{text}"
        );
        // The folded fields render with their projections.
        for row in [
            "proves       blake3:fixedsem — the workflow identity this receipt attests (the semantic hash)",
            "lock_digest  blake3:lock — the nika.lock digest the run resolved under",
            "digest       ",
            "trace_verdict — the trace-verify result folded into this receipt",
            "  outcome    success — the run's terminal outcome",
            "assertions  1 judged — the assert: obligations, each judged at its honest level",
            "  no_secret_egress  StaticProof",
        ] {
            assert!(text.contains(row), "missing row `{row}` in:\n{text}");
        }
        // A foreign certificate shape (no Bound axes) renders without a
        // panic — the reading stays total.
        assert!(
            text.contains("certificate — the check-time resource certificate"),
            "{text}"
        );
        // explain ≠ evidence: the digest rides as DATA (no verify call —
        // a doctored receipt renders the same way, and that is the law).
        let mut doctored = receipt.as_object().unwrap().clone();
        doctored.insert("lock_digest".to_owned(), json!("blake3:forged"));
        let doctored_text = explain_receipt(&Value::Object(doctored));
        assert!(
            doctored_text.contains("blake3:forged"),
            "a reading, not a verdict"
        );
    }

    /// The golden pin of the FULL render over the rich receipt — one
    /// byte-level contract for the 3-OS stability claim.
    #[test]
    fn the_explain_render_is_byte_stable() {
        use nika_proof::receipt::explain_receipt;

        let text = explain_receipt(&rich_receipt());
        let expected = "\
receipt · the one run receipt (receipt_format 1)
  a READING, never a proof — the digest is NOT recomputed here · `verify` is the proof

proves       PROVES — the workflow identity this receipt attests (the semantic hash)
digest       DIGEST — the self-binding digest — verify recomputes it over the folded fields
lock_digest  blake3:lockdigest — the nika.lock digest the run resolved under (unrecorded when the journal never carried it)

certificate — the check-time resource certificate folded into this receipt
  task_attempts  ≤ 2 + 2·|fan| — upper bound on task-body executions (attempts × fan-out)
  llm_calls      ≤ 0 + 2·|fan| — upper bound on LLM calls (infer + agent turns)
  effect_calls   ≤ 2 — upper bound on effect calls (exec + invoke dispatches)
  usd_micros     ≤ 0 + 6000·|fan| — the parametric spend bound in micro-USD (absent when unpriceable)
  span_attempts  4 — the longest sequential dependency chain, in attempts (the span)
  derivation     3 witness rows — the per-task witness rows the audit re-checks
  effects        boundary declared · 0 escapes — the authority projection (spec 10 · re-derived at audit, never trusted)

trace_verdict — the trace-verify result folded into this receipt
  outcome    completed — the run's terminal outcome
  chain      intact — the journal chain status the verdict names
  events     7 — the journaled event count
  head       ab12 — the chain head the verdict names
  sealed     true — whether the journal is sealed (attributable)

assertions  none — no obligation was claimed (the `assert:` key died 2026-08-13)
";
        let normalized = text
            .replace(&rich_proves(), "PROVES")
            .replace(&rich_digest(), "DIGEST");
        assert_eq!(normalized, expected, "the stable render drifted:\n{text}");
    }

    /// The rich fixture's semantic hash (the golden normalizes it out).
    fn rich_proves() -> String {
        let wf = parse(RICH_YAML);
        semantic_ir_hash(&wf)
            .expect("projectable")
            .as_hex()
            .to_owned()
    }

    /// The rich fixture's receipt digest (normalized out of the golden —
    /// it is deterministic; the pin lives on the STRUCTURE).
    fn rich_digest() -> String {
        rich_receipt()["digest"]
            .as_str()
            .expect("a digest")
            .to_owned()
    }
}
