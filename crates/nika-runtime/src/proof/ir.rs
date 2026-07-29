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

    const BASE: &str = "nika: v1\nworkflow:\n  id: demo\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n  b:\n    with: { p: \"${{ tasks.a.output }}\" }\n    exec: { command: [\"echo\", \"${{ with.p }}\"] }\n";

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
        const AB: &str = "nika: v1\nworkflow:\n  id: demo\ntasks:\n  t:\n    with: { a: \"1\", b: \"2\" }\n    exec: { command: [\"echo\", \"${{ with.a }}${{ with.b }}\"] }\n";
        const BA: &str = "nika: v1\nworkflow:\n  id: demo\ntasks:\n  t:\n    with: { b: \"2\", a: \"1\" }\n    exec: { command: [\"echo\", \"${{ with.a }}${{ with.b }}\"] }\n";
        let ab = semantic_ir_hash(&parse(AB)).expect("projectable");
        let ba = semantic_ir_hash(&parse(BA)).expect("projectable");
        assert_eq!(ab, ba, "authored map order is not semantic identity");
    }

    #[test]
    fn task_order_does_not_change_the_root() {
        const AB: &str = "nika: v1\nworkflow:\n  id: demo\ntasks:\n  a:\n    exec: { command: [\"echo\", \"1\"] }\n  b:\n    exec: { command: [\"echo\", \"2\"] }\n";
        const BA: &str = "nika: v1\nworkflow:\n  id: demo\ntasks:\n  b:\n    exec: { command: [\"echo\", \"2\"] }\n  a:\n    exec: { command: [\"echo\", \"1\"] }\n";
        assert_eq!(
            semantic_ir_hash(&parse(AB)).expect("projectable"),
            semantic_ir_hash(&parse(BA)).expect("projectable"),
            "the root sorts the task map — authored order is not behavior"
        );
    }

    #[test]
    fn the_workflow_id_participates_in_the_root() {
        let a = semantic_ir_hash(&parse(BASE)).expect("projectable");
        let renamed = BASE.replace("id: demo", "id: other");
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
        use nika_schema::types::AssertProperty;
        use serde_json::json;

        let wf = parse(
            "nika: v1\nworkflow:\n  id: pay\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        let report = nika_check::check(&wf);
        let proves = semantic_ir_hash(&wf).expect("projectable");

        // Judge one obligation at its honest level (no_secret_egress is static).
        let property = AssertProperty::NoSecretEgress;
        let judged = vec![(property.clone(), property.level(false))];

        let receipt = build_run_receipt(
            &proves,
            &report.certificate,
            &judged,
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
        // The judged assertion rides with its honest level.
        assert_eq!(
            receipt["assertions"][0]["assert"],
            json!("no_secret_egress")
        );
        assert_eq!(receipt["assertions"][0]["level"], json!("StaticProof"));
        assert_eq!(receipt["lock_digest"], json!("blake3:lockdigest"));
        // It does NOT verify against a different workflow's identity.
        assert!(!verify(&receipt, "blake3:someotherworkflow"));
    }
}
