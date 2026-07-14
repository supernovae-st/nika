// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika:decide` HERMETIC proof battery — zero I/O, so the lib-test
//! push gate runs it anywhere (no `NIKA_SPEC_DIR`, no spec checkout):
//! the selftest's law mutations (`decision_core_selftest.py` mirrored)
//! over the pr-triage bundle INLINED verbatim, the Belnap/abstention/
//! governance laws, the dispatcher route with receipt bytes PINNED
//! inline (the s1/s5 goldens embedded as string constants), and the
//! adversarial negative-arithmetic receipts pinned verbatim from a live
//! `decision_core.py` run.
//!
//! The disk-golden differential (reads the spec repo · HARD-FAILS when
//! absent) lives in `tests/decide_goldens.rs` — integration tests, per
//! the type-core differential discipline (lib tests never read the spec
//! repo).

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
// The snapshot builders take owned `Value`s by design (call sites hand
// `json!` literals over).
#![allow(clippy::needless_pass_by_value)]

use nika_kernel::runtime::tool_executor::ToolExecuteDyn;
use serde_json::{Value, json};

use super::{DecideError, evaluate};
use crate::test_rig::{call, dispatcher};
use nika_kernel_mock::{MockClock, MockFs, MockHttp};

/// The pr-triage golden bundle, INLINED verbatim
/// (`conformance/decision-goldens/pr-triage.bundle.json` in nika-spec) —
/// the lib battery stays hermetic; the disk copy is re-proven byte-equal
/// by the integration differential.
fn bundle() -> Value {
    json!({
        "decision_bundle_format": 1,
        "manifest": {
            "id": "pr-triage",
            "version": "1.0.0",
            "owner": "acme-org",
            "license": "Apache-2.0",
            "valid_until": "2027-01-01T00:00:00Z"
        },
        "evidence_schema": {
            "failed_required_checks": {
                "type": "integer", "required": true,
                "sources": ["ci", "audit-bot"], "integrity": "verified"
            },
            "relevant_coverage_bp": {
                "type": "integer", "required": false,
                "sources": ["coverage-bot"], "integrity": "observed"
            },
            "touches_release_workflow": {
                "type": "bool", "required": true,
                "sources": ["ci"], "integrity": "verified"
            },
            "author_tenure_days": {
                "type": "integer", "required": false, "identity": true,
                "sources": ["hr"], "integrity": "observed"
            }
        },
        "transforms": {
            "checks_0_10": { "kind": "clamp", "min": 0, "max": 10 },
            "coverage_inverse": {
                "kind": "linear", "min": 0, "max": 10000,
                "scale_bp": -10000, "offset": 10000
            },
            "release_step": {
                "kind": "bucket", "min": 0, "max": 1,
                "edges": [1], "values": [0, 10000]
            }
        },
        "rules": {
            "dimensions": {
                "change_risk": {
                    "terms": [
                        { "evidence": "failed_required_checks", "transform": "checks_0_10",
                          "weight_bp": 10000, "monotonicity": "increases" },
                        { "evidence": "touches_release_workflow", "transform": "release_step",
                          "weight_bp": 5000, "monotonicity": "increases" }
                    ]
                },
                "evidence_quality": {
                    "terms": [
                        { "evidence": "relevant_coverage_bp", "transform": "coverage_inverse",
                          "weight_bp": 3000, "monotonicity": "decreases" }
                    ]
                }
            },
            "thresholds": [
                { "dimension": "change_risk", "recommend_gte_bp": 5000 }
            ]
        },
        "governance": {
            "never_automatic": ["recommend"],
            "override": { "append_only": true, "reason_required": true },
            "appeal": "file an issue on the bundle repo",
            "human_required_triggers": ["conflict_on_required"]
        },
        "fixtures": [
            { "name": "hot", "class": "positive",
              "evidence": { "failed_required_checks": 8, "touches_release_workflow": true,
                            "relevant_coverage_bp": 2000 } },
            { "name": "cold", "class": "negative",
              "evidence": { "failed_required_checks": 0, "touches_release_workflow": false,
                            "relevant_coverage_bp": 9000 } },
            { "name": "mid", "class": "ambiguous",
              "evidence": { "failed_required_checks": 3, "touches_release_workflow": false,
                            "relevant_coverage_bp": 5000 } },
            { "name": "two-authorities", "class": "contradictory",
              "evidence": { "failed_required_checks": 2, "touches_release_workflow": false } },
            { "name": "identity-probe", "class": "adversarial",
              "evidence": { "failed_required_checks": 8, "touches_release_workflow": true,
                            "relevant_coverage_bp": 2000, "author_tenure_days": 9000 } },
            { "name": "hot-lower-checks", "class": "positive",
              "evidence": { "failed_required_checks": 5, "touches_release_workflow": true,
                            "relevant_coverage_bp": 2000 } },
            { "name": "mid-higher-coverage", "class": "ambiguous",
              "evidence": { "failed_required_checks": 3, "touches_release_workflow": false,
                            "relevant_coverage_bp": 8000 } }
        ]
    })
}

/// The s1-dominant-risk golden receipt, embedded verbatim (the bytes
/// `decision_core.py` prints · the integration differential re-proves
/// the disk copy).
const S1_RECEIPT: &str = "{\"bundle\":{\"digest\":null,\"id\":\"pr-triage\",\"version\":\"1.0.0\"},\"conflicts\":[],\"decision_receipt_format\":1,\"determination_provenance\":[\"dimension change_risk dominates the threshold (inf 5008 >= 5000 bp) — robust, not point-estimated\",\"governance.never_automatic lists recommend — human_required\"],\"dimensions\":{\"change_risk\":{\"contributions\":[{\"contribution\":{\"hi\":8,\"lo\":8},\"evidence\":\"failed_required_checks\",\"known\":true,\"transform\":\"checks_0_10\",\"weight_bp\":10000},{\"contribution\":{\"hi\":5000,\"lo\":5000},\"evidence\":\"touches_release_workflow\",\"known\":true,\"transform\":\"release_step\",\"weight_bp\":5000}],\"interval\":{\"hi\":5008,\"lo\":5008}},\"evidence_quality\":{\"contributions\":[{\"contribution\":{\"hi\":2400,\"lo\":2400},\"evidence\":\"relevant_coverage_bp\",\"known\":true,\"transform\":\"coverage_inverse\",\"weight_bp\":3000}],\"interval\":{\"hi\":2400,\"lo\":2400}}},\"outcome\":\"human_required\",\"snapshot\":{\"digests\":[\"d1\",\"d2\",\"d3\"],\"missing\":[],\"t\":\"2026-07-14T20:00:00Z\"}}";

/// The s5-cold golden receipt, embedded verbatim (same convention).
const S5_RECEIPT: &str = "{\"bundle\":{\"digest\":null,\"id\":\"pr-triage\",\"version\":\"1.0.0\"},\"conflicts\":[],\"decision_receipt_format\":1,\"determination_provenance\":[\"no threshold admitted — defer\"],\"dimensions\":{\"change_risk\":{\"contributions\":[{\"contribution\":{\"hi\":0,\"lo\":0},\"evidence\":\"failed_required_checks\",\"known\":true,\"transform\":\"checks_0_10\",\"weight_bp\":10000},{\"contribution\":{\"hi\":0,\"lo\":0},\"evidence\":\"touches_release_workflow\",\"known\":true,\"transform\":\"release_step\",\"weight_bp\":5000}],\"interval\":{\"hi\":0,\"lo\":0}},\"evidence_quality\":{\"contributions\":[{\"contribution\":{\"hi\":300,\"lo\":300},\"evidence\":\"relevant_coverage_bp\",\"known\":true,\"transform\":\"coverage_inverse\",\"weight_bp\":3000}],\"interval\":{\"hi\":300,\"lo\":300}}},\"outcome\":\"defer\",\"snapshot\":{\"digests\":[\"d1\",\"d2\",\"d3\"],\"missing\":[],\"t\":\"2026-07-14T20:00:00Z\"}}";

/// Canonical JSON — `serde_json`'s compact form over its sorted map IS
/// the reference's `sort_keys` + `(",", ":")` + raw UTF-8 spelling.
fn canonical(v: &Value) -> String {
    serde_json::to_string(v).expect("serializes")
}

fn snap(evidence: Vec<Value>) -> Value {
    json!({ "t": "2026-07-14T20:00:00Z", "evidence": evidence })
}

fn ev(key: &str, value: Value) -> Value {
    ev_full(key, value, "ci", "verified", "d")
}

fn ev_full(key: &str, value: Value, source: &str, integrity: &str, digest: &str) -> Value {
    json!({
        "key": key, "value": value, "source": source,
        "observed_at": "2026-07-14T20:00:00Z", "digest": digest,
        "confidentiality": "internal", "integrity": integrity,
        "quality": { "freshness": "fresh", "completeness": "complete",
                     "independence_group": source },
    })
}

/// The s1-dominant-risk snapshot rebuilt inline (same fields the receipt
/// reads: keys · values · sources · integrity · digests · t).
fn s1_snapshot() -> Value {
    snap(vec![
        ev_full("failed_required_checks", json!(8), "ci", "verified", "d1"),
        ev_full(
            "touches_release_workflow",
            json!(true),
            "ci",
            "verified",
            "d2",
        ),
        ev_full(
            "relevant_coverage_bp",
            json!(2000),
            "coverage-bot",
            "observed",
            "d3",
        ),
    ])
}

// ─── the embedded golden pins (byte-parity without I/O) ─────────────────

#[test]
fn s1_receipt_is_byte_equal_with_the_embedded_golden() {
    let receipt = evaluate(&bundle(), &s1_snapshot()).expect("evaluates");
    assert_eq!(canonical(&receipt), S1_RECEIPT);
}

#[test]
fn determinism_two_runs_byte_equal() {
    let bundle = bundle();
    let snapshot = s1_snapshot();
    let a = canonical(&evaluate(&bundle, &snapshot).expect("evaluates"));
    let b = canonical(&evaluate(&bundle, &snapshot).expect("evaluates"));
    assert_eq!(a, b);
}

// ─── bundle laws (NIKA-DECIDE-001 · the selftest mutations) ─────────────

fn refuses_bundle(mutate: impl FnOnce(&mut Value)) -> String {
    let mut b = bundle();
    mutate(&mut b);
    match evaluate(&b, &snap(vec![])) {
        Err(DecideError::Bundle(msg)) => msg,
        other => panic!("mutated bundle must refuse as DECIDE-001, got {other:?}"),
    }
}

#[test]
fn fixed_point_float_weight_refused() {
    let msg = refuses_bundle(|b| {
        b["rules"]["dimensions"]["change_risk"]["terms"][0]["weight_bp"] = json!(0.5);
    });
    assert!(msg.contains("fixed-point law"), "{msg}");
}

#[test]
fn undeclared_evidence_key_in_rules_refused() {
    let msg = refuses_bundle(|b| {
        b["rules"]["dimensions"]["change_risk"]["terms"][0]["evidence"] = json!("ghost_key");
    });
    assert!(msg.contains("undeclared key"), "{msg}");
}

#[test]
fn identity_key_in_technical_dimension_refused() {
    let msg = refuses_bundle(|b| {
        let terms = b["rules"]["dimensions"]["change_risk"]["terms"]
            .as_array_mut()
            .expect("terms");
        terms.push(json!({
            "evidence": "author_tenure_days", "transform": "checks_0_10",
            "weight_bp": 100, "monotonicity": "none",
        }));
    });
    assert!(msg.contains("identity"), "{msg}");
}

#[test]
fn contradictory_fixture_is_mandatory() {
    let msg = refuses_bundle(|b| {
        let fixtures = b["fixtures"].as_array().expect("fixtures").clone();
        b["fixtures"] = Value::Array(
            fixtures
                .into_iter()
                .filter(|f| f["class"] != "contradictory")
                .collect(),
        );
    });
    assert!(msg.contains("CONTRADICTORY"), "{msg}");
}

#[test]
fn monotonicity_is_checked_on_the_bundles_own_fixtures() {
    // Flip the declared direction — the hot/hot-lower-checks fixture pair
    // (8→5 checks, all else equal) now violates it.
    let msg = refuses_bundle(|b| {
        b["rules"]["dimensions"]["change_risk"]["terms"][0]["monotonicity"] = json!("decreases");
    });
    assert!(msg.contains("monotonicity"), "{msg}");
}

#[test]
fn bucket_edges_must_be_sorted() {
    // A well-shaped bucket (values = edges + 1) with unsorted edges.
    let msg = refuses_bundle(|b| {
        b["transforms"]["release_step"]["edges"] = json!([2, 1]);
        b["transforms"]["release_step"]["values"] = json!([0, 1, 2]);
    });
    assert!(msg.contains("sorted"), "{msg}");
    // …and the selftest's exact mutation (edges grow, values do not)
    // refuses on the shape law.
    let msg = refuses_bundle(|b| {
        b["transforms"]["release_step"]["edges"] = json!([1, 0]);
    });
    assert!(msg.contains("edges[n] + values[n+1]"), "{msg}");
}

#[test]
fn missing_region_and_bad_manifest_refused() {
    let msg = refuses_bundle(|b| {
        b.as_object_mut().expect("bundle").remove("governance");
    });
    assert!(msg.contains("bundle.governance"), "{msg}");
    let msg = refuses_bundle(|b| {
        b["manifest"]["id"] = json!("");
    });
    assert!(msg.contains("manifest.id"), "{msg}");
}

#[test]
fn never_automatic_outside_the_outcome_enum_refused() {
    let msg = refuses_bundle(|b| {
        b["governance"]["never_automatic"] = json!(["approve"]);
    });
    assert!(msg.contains("not an outcome"), "{msg}");
}

// ─── snapshot laws (NIKA-DECIDE-002 · the selftest mutations) ───────────

fn refuses_snapshot(snapshot: &Value) -> String {
    match evaluate(&bundle(), snapshot) {
        Err(DecideError::Snapshot(msg)) => msg,
        other => panic!("snapshot must refuse as DECIDE-002, got {other:?}"),
    }
}

#[test]
fn type_misfit_refused() {
    let msg = refuses_snapshot(&snap(vec![
        ev("failed_required_checks", json!("eight")),
        ev("touches_release_workflow", json!(false)),
    ]));
    assert!(msg.contains("does not fit"), "{msg}");
}

#[test]
fn unauthorized_source_refused() {
    let msg = refuses_snapshot(&snap(vec![
        ev_full(
            "failed_required_checks",
            json!(1),
            "random-blog",
            "verified",
            "d",
        ),
        ev("touches_release_workflow", json!(false)),
    ]));
    assert!(msg.contains("not authorized"), "{msg}");
}

#[test]
fn integrity_below_the_declared_floor_refused() {
    let msg = refuses_snapshot(&snap(vec![
        ev_full("failed_required_checks", json!(1), "ci", "observed", "d"),
        ev("touches_release_workflow", json!(false)),
    ]));
    assert!(msg.contains("below the declared floor"), "{msg}");
}

#[test]
fn undeclared_snapshot_key_refused() {
    let msg = refuses_snapshot(&snap(vec![
        ev("failed_required_checks", json!(1)),
        ev("touches_release_workflow", json!(false)),
        ev("ghost", json!(1)),
    ]));
    assert!(msg.contains("not a declared evidence key"), "{msg}");
}

#[test]
fn integrity_outside_the_lattice_refused() {
    let msg = refuses_snapshot(&snap(vec![
        ev_full("failed_required_checks", json!(1), "ci", "cosmic", "d"),
        ev("touches_release_workflow", json!(false)),
    ]));
    assert!(msg.contains("not in the lattice"), "{msg}");
}

// ─── Belnap · abstention · governance (the selftest laws) ───────────────

#[test]
fn authoritative_conflict_on_required_forces_human_required_with_witness() {
    let receipt = evaluate(
        &bundle(),
        &snap(vec![
            ev_full(
                "failed_required_checks",
                json!(2),
                "ci",
                "authoritative",
                "d",
            ),
            ev_full(
                "failed_required_checks",
                json!(7),
                "audit-bot",
                "authoritative",
                "d9",
            ),
            ev("touches_release_workflow", json!(false)),
        ]),
    )
    .expect("evaluates");
    assert_eq!(receipt["outcome"], "human_required");
    let witness = receipt["conflicts"][0]["witness"]
        .as_array()
        .expect("witness");
    assert_eq!(witness.len(), 2, "the witness carries both sources");
    // Witness rides source-sorted (the reference's `sorted(key=str)`).
    assert_eq!(witness[0]["source"], "audit-bot");
    assert_eq!(witness[1]["source"], "ci");
}

#[test]
fn missing_required_defers_and_never_zero_fills() {
    let receipt = evaluate(
        &bundle(),
        &snap(vec![ev("touches_release_workflow", json!(true))]),
    )
    .expect("evaluates");
    assert_eq!(receipt["outcome"], "defer");
    assert_eq!(receipt["snapshot"]["missing"][0], "failed_required_checks");
    // The determination line carries Python's list-repr spelling.
    let det = receipt["determination_provenance"][0]
        .as_str()
        .expect("det");
    assert!(det.contains("['failed_required_checks']"), "{det}");
    // Unknown ≠ 0: the unknown term contributes an INTERVAL.
    let interval = &receipt["dimensions"]["change_risk"]["interval"];
    assert_ne!(interval["lo"], interval["hi"]);
}

#[test]
fn unknown_optional_straddle_is_incomparable_defer() {
    let receipt = evaluate(
        &bundle(),
        &snap(vec![
            ev("failed_required_checks", json!(4)),
            ev("touches_release_workflow", json!(false)),
        ]),
    )
    .expect("evaluates");
    assert_eq!(receipt["outcome"], "defer");
    let quality = &receipt["dimensions"]["evidence_quality"]["interval"];
    assert_ne!(quality["lo"], quality["hi"]);
}

#[test]
fn never_automatic_forces_human_required() {
    let receipt = evaluate(
        &bundle(),
        &snap(vec![
            ev("failed_required_checks", json!(9)),
            ev("touches_release_workflow", json!(true)),
            ev_full(
                "relevant_coverage_bp",
                json!(1000),
                "coverage-bot",
                "observed",
                "d3",
            ),
        ]),
    )
    .expect("evaluates");
    assert_eq!(receipt["outcome"], "human_required");
    let det = receipt["determination_provenance"]
        .as_array()
        .expect("det lines");
    assert!(
        det.iter()
            .any(|d| d.as_str().is_some_and(|s| s.contains("never_automatic"))),
        "{det:?}"
    );
}

#[test]
fn a_float_fits_the_integer_type_but_scores_unknown() {
    // The reference's subtlety pinned: `3.0` FITS `integer` (spec 09 ·
    // `float.is_integer()`), yet `isinstance(v, int)` is False — the
    // term scores as an INTERVAL, never a point.
    let receipt = evaluate(
        &bundle(),
        &snap(vec![
            ev("failed_required_checks", json!(0)),
            ev("touches_release_workflow", json!(false)),
            ev_full(
                "relevant_coverage_bp",
                json!(2000.0),
                "coverage-bot",
                "observed",
                "d3",
            ),
        ]),
    )
    .expect("a fitting float passes DECIDE-002");
    let contribution = &receipt["dimensions"]["evidence_quality"]["contributions"][0];
    assert_eq!(contribution["known"], false, "{contribution}");
    assert_ne!(
        contribution["contribution"]["lo"],
        contribution["contribution"]["hi"]
    );
}

#[test]
fn python_floor_division_is_mirrored_on_negatives() {
    // `(1 * -3333) // 10000` is -1 in Python (floor), 0 under Rust
    // truncation — the `div_euclid` mirror is load-bearing.
    assert_eq!(super::mul_bp(1, -3333).expect("in range"), -1);
    assert_eq!(super::mul_bp(-3, 5000).expect("in range"), -2);
    assert_eq!(super::mul_bp(3, 5000).expect("in range"), 1);
}

#[test]
fn duplicate_snapshot_keys_score_last_wins() {
    // The reference's dict comprehension keeps the LAST claim for
    // scoring (golden s4 pins it end-to-end in the integration
    // differential; this pins it in isolation with sub-authoritative
    // duplicates — no conflict, still last-wins).
    let receipt = evaluate(
        &bundle(),
        &snap(vec![
            ev("failed_required_checks", json!(2)),
            ev_full(
                "failed_required_checks",
                json!(7),
                "audit-bot",
                "verified",
                "d9",
            ),
            ev("touches_release_workflow", json!(false)),
        ]),
    )
    .expect("evaluates");
    assert!(
        receipt["conflicts"]
            .as_array()
            .expect("conflicts")
            .is_empty()
    );
    let contribution = &receipt["dimensions"]["change_risk"]["contributions"][0];
    assert_eq!(contribution["contribution"]["lo"], 7);
}

// ─── the dispatcher route (arg plane + wire) ────────────────────────────

fn rig() -> crate::test_rig::TestDispatcher {
    dispatcher(MockFs::new(), MockHttp::new(), MockClock::new())
}

async fn dispatch(args: Value) -> nika_kernel::runtime::tool_executor::ToolResult {
    rig()
        .execute(call("nika:decide", args))
        .await
        .expect("dispatches")
}

#[tokio::test]
async fn inline_bundle_through_the_dispatcher_yields_the_pinned_receipt() {
    let result = rig()
        .execute(call(
            "nika:decide",
            json!({ "bundle": bundle(), "evidence": s1_snapshot() }),
        ))
        .await
        .expect("dispatches");
    assert!(!result.is_error, "{}", result.content);
    // The wire content IS the canonical receipt (compact · sorted keys).
    assert_eq!(result.content, S1_RECEIPT);
    let structured = result.structured.expect("receipt is structured");
    assert_eq!(canonical(&structured), S1_RECEIPT);
}

#[tokio::test]
async fn bundle_path_through_the_dispatcher_reads_and_evaluates() {
    // The path form: the inline bundle written INTO the mock filesystem
    // (no spec checkout) — the s5-cold evidence yields the pinned receipt.
    let bundle_text = canonical(&bundle());
    let fs = MockFs::new().with_file("./decisions/pr-triage.bundle.json", bundle_text.as_str());
    let evidence = snap(vec![
        ev_full("failed_required_checks", json!(0), "ci", "verified", "d1"),
        ev_full(
            "touches_release_workflow",
            json!(false),
            "ci",
            "verified",
            "d2",
        ),
        ev_full(
            "relevant_coverage_bp",
            json!(9000),
            "coverage-bot",
            "observed",
            "d3",
        ),
    ]);
    let result = dispatcher(fs, MockHttp::new(), MockClock::new())
        .execute(call(
            "nika:decide",
            json!({
                "bundle": "./decisions/pr-triage.bundle.json",
                "evidence": evidence,
            }),
        ))
        .await
        .expect("dispatches");
    assert!(!result.is_error, "{}", result.content);
    assert_eq!(result.content, S5_RECEIPT);
}

#[tokio::test]
async fn arg_shape_violations_are_the_builtin_arg_plane() {
    for args in [
        json!({}),
        json!({ "bundle": 42, "evidence": {} }),
        json!({ "bundle": {} }),
        json!({ "bundle": {}, "evidence": "not-an-object" }),
    ] {
        let result = dispatch(args.clone()).await;
        assert!(result.is_error, "{args}");
        assert!(
            result.content.starts_with("NIKA-BUILTIN-DECIDE-001"),
            "{args} → {}",
            result.content
        );
    }
}

#[tokio::test]
async fn unreadable_and_malformed_bundle_paths_fail_on_their_planes() {
    // A missing file is the ARG plane (the path could not be served)…
    let missing = dispatch(json!({ "bundle": "./nowhere.json", "evidence": {} })).await;
    assert!(missing.is_error);
    assert!(
        missing.content.starts_with("NIKA-BUILTIN-DECIDE-001"),
        "{}",
        missing.content
    );
    // …while a file that reads but is not JSON is a malformed BUNDLE
    // (NIKA-DECIDE-001 — the bundle-law plane).
    let fs = MockFs::new().with_file("./broken.json", "{ not json");
    let result = dispatcher(fs, MockHttp::new(), MockClock::new())
        .execute(call(
            "nika:decide",
            json!({ "bundle": "./broken.json", "evidence": {} }),
        ))
        .await
        .expect("dispatches");
    assert!(result.is_error);
    assert!(
        result.content.starts_with("NIKA-DECIDE-001"),
        "{}",
        result.content
    );
}

#[tokio::test]
async fn kernel_refusals_carry_the_spec_codes_on_the_wire() {
    // DECIDE-002 through the full dispatcher path (empty snapshot is
    // fine — an undeclared key is not).
    let result = dispatch(json!({
        "bundle": bundle(),
        "evidence": { "t": "2026-07-14T20:00:00Z",
                       "evidence": [ { "key": "ghost", "value": 1 } ] },
    }))
    .await;
    assert!(result.is_error);
    assert!(
        result.content.starts_with("NIKA-DECIDE-002"),
        "{}",
        result.content
    );
}

#[test]
fn adversarial_negative_arithmetic_is_byte_equal_with_the_live_reference() {
    // Pinned from `decision_core.py` run 2026-07-14 (the two receipts below
    // are the reference's VERBATIM canonical output): negative scale_bp,
    // non-exact floor division on negatives (`(-3*-3333)//10000` then
    // `-12345//10000 = -2`, where truncation would say -1), a negative
    // WEIGHT over a bucket interval, and a bool riding a bucket.
    let bundle = json!({
        "decision_bundle_format": 1,
        "manifest": {"id": "adversarial", "version": "0.1.0", "owner": "t", "license": "Apache-2.0"},
        "evidence_schema": {
            "x": {"type": "integer", "required": true, "integrity": "observed"},
            "flag": {"type": "bool", "required": false, "integrity": "observed"}
        },
        "transforms": {
            "neg": {"kind": "linear", "min": -7, "max": 13, "scale_bp": -3333, "offset": -1},
            "steps": {"kind": "bucket", "min": -5, "max": 5, "edges": [-2, 0, 3], "values": [7, -9, 4, -1]}
        },
        "rules": {
            "dimensions": {
                "d1": {"terms": [
                    {"evidence": "x", "transform": "neg", "weight_bp": 12345, "monotonicity": "none"},
                    {"evidence": "flag", "transform": "steps", "weight_bp": -5000, "monotonicity": "none"}
                ]}
            },
            "thresholds": [{"dimension": "d1", "recommend_gte_bp": -3}]
        },
        "governance": {"never_automatic": []},
        "fixtures": [
            {"name": "c", "class": "contradictory", "evidence": {"x": 1}},
            {"name": "p", "class": "positive", "evidence": {"x": 3, "flag": true}}
        ]
    });
    let snap1 = json!({"t": "2026-07-14T21:00:00Z", "evidence": [
        {"key": "x", "value": -3, "source": "s1", "integrity": "observed", "digest": "za"},
    ]});
    let want1 = "{\"bundle\":{\"digest\":null,\"id\":\"adversarial\",\"version\":\"0.1.0\"},\"conflicts\":[],\"decision_receipt_format\":1,\"determination_provenance\":[\"dimension d1 straddles the threshold ([-6, -2] vs -3 bp) — incomparable with the available evidence, never a false order\"],\"dimensions\":{\"d1\":{\"contributions\":[{\"contribution\":{\"hi\":-2,\"lo\":-2},\"evidence\":\"x\",\"known\":true,\"transform\":\"neg\",\"weight_bp\":12345},{\"contribution\":{\"hi\":0,\"lo\":-4},\"evidence\":\"flag\",\"known\":false,\"transform\":\"steps\",\"weight_bp\":-5000}],\"interval\":{\"hi\":-2,\"lo\":-6}}},\"outcome\":\"defer\",\"snapshot\":{\"digests\":[\"za\"],\"missing\":[],\"t\":\"2026-07-14T21:00:00Z\"}}";
    assert_eq!(
        canonical(&evaluate(&bundle, &snap1).expect("evaluates")),
        want1
    );
    let snap2 = json!({"t": "2026-07-14T21:00:00Z", "evidence": [
        {"key": "x", "value": 5, "source": "s1", "integrity": "verified", "digest": "zb"},
        {"key": "flag", "value": true, "source": "s2", "integrity": "observed", "digest": "zc"},
    ]});
    let want2 = "{\"bundle\":{\"digest\":null,\"id\":\"adversarial\",\"version\":\"0.1.0\"},\"conflicts\":[],\"decision_receipt_format\":1,\"determination_provenance\":[\"no threshold admitted — defer\"],\"dimensions\":{\"d1\":{\"contributions\":[{\"contribution\":{\"hi\":-4,\"lo\":-4},\"evidence\":\"x\",\"known\":true,\"transform\":\"neg\",\"weight_bp\":12345},{\"contribution\":{\"hi\":-2,\"lo\":-2},\"evidence\":\"flag\",\"known\":true,\"transform\":\"steps\",\"weight_bp\":-5000}],\"interval\":{\"hi\":-6,\"lo\":-6}}},\"outcome\":\"defer\",\"snapshot\":{\"digests\":[\"zb\",\"zc\"],\"missing\":[],\"t\":\"2026-07-14T21:00:00Z\"}}";
    assert_eq!(
        canonical(&evaluate(&bundle, &snap2).expect("evaluates")),
        want2
    );
}

#[test]
fn py_repr_matches_cpython_for_the_realistic_key_domain() {
    assert_eq!(super::py_repr_list(&["a_key".to_owned()]), "['a_key']");
    assert_eq!(
        super::py_repr_list(&["a".to_owned(), "b".to_owned()]),
        "['a', 'b']"
    );
    assert_eq!(super::py_repr_str("it's"), r#""it's""#);
    assert_eq!(super::py_repr_str(r#"both'and""#), r#"'both\'and"'"#);
    assert_eq!(super::py_repr_str("back\\slash"), r"'back\\slash'");
}
