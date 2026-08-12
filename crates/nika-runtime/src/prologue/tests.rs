// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `workflow_started` prologue tests — the evidence-pack fields
//! (A5): the workflow's semantic hash · the declared boundary as JSON ·
//! the composer-selected sandbox backend, journaled so the pack reads
//! the run's identity + boundary + confinement from the journal's OWN
//! bytes. Split from `tests.rs` (that file rides the 1,500-LOC ceiling).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use super::emit_prologue;
use crate::*;

/// Run the prologue over one yaml fixture and return the started
/// event's string fields as (key, value) pairs.
fn started_fields(yaml: &str, sandbox: Option<&str>) -> Vec<(String, String)> {
    started_fields_with_origins(yaml, sandbox, &BTreeMap::new())
}

/// [`started_fields`] with the F-P13 input origins the composer would
/// inject (empty = the no-claim posture).
fn started_fields_with_origins(
    yaml: &str,
    sandbox: Option<&str>,
    origins: &BTreeMap<String, InputOrigin>,
) -> Vec<(String, String)> {
    started_fields_full(yaml, sandbox, None, false, origins, None, None, None, None)
}

/// The full knob set (origins · the F-P21 declared compat · the ADR-099
/// trust-amendment opt-out · the F-P18 operator budget · the issue-772
/// model override · the #889 policy posture + witnessed waiver).
#[allow(clippy::too_many_arguments)] // the prologue's own knob set (its precedent)
fn started_fields_full(
    yaml: &str,
    sandbox: Option<&str>,
    sandbox_policy: Option<&str>,
    sandbox_waived: bool,
    origins: &BTreeMap<String, InputOrigin>,
    resume_compat: Option<&str>,
    resume_unverified: Option<&crate::resume::ResumeUnverified>,
    max_cost_usd: Option<f64>,
    model_override: Option<&str>,
) -> Vec<(String, String)> {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let book = crate::approval::ApprovalBook::new();
    emit_prologue(
        &wf,
        "pay",
        None,
        None,
        sandbox,
        sandbox_policy,
        sandbox_waived,
        origins,
        resume_compat,
        resume_unverified,
        max_cost_usd,
        model_override,
        Vec::new(),
        None,
        &book,
        &mut stamper,
        &mut sink,
    );
    let started = &sink.events()[0];
    started
        .fields
        .iter()
        .filter_map(|f| match &f.value {
            nika_types::resource::Value::String(s) => Some((f.key.clone(), s.clone())),
            _ => None,
        })
        .collect()
}

fn get<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// The A5 evidence fields ride the prologue: the semantic hash (the
/// proof layer's OWN projection — byte-equal to the seal's), the
/// declared boundary as spec-wire JSON, and the sandbox backend name.
#[test]
fn prologue_journals_the_evidence_fields() {
    let yaml = "nika: pay\npermits:\n  fs: { read: [\"./in/**\"], write: [\"./out/**\"] }\n  exec: [\"echo\"]\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let fields = started_fields(yaml, Some("seatbelt"));
    let sem = crate::proof::ir::semantic_ir_hash(&wf).expect("projectable");
    assert_eq!(get(&fields, "semantic_hash"), Some(sem.as_hex()));
    assert_eq!(get(&fields, "sandbox"), Some("seatbelt"));
    let permits: serde_json::Value =
        serde_json::from_str(get(&fields, "permits_json").expect("the boundary is journaled"))
            .expect("permits_json is valid JSON");
    assert_eq!(permits["fs"]["write"], serde_json::json!(["./out/**"]));
    assert_eq!(permits["exec"], serde_json::json!(["echo"]));
}

/// Absent facts stay absent (never an invented claim): no `permits:`
/// block → no `permits_json`; an unnamed backend → no `sandbox`. The
/// semantic hash rides regardless — it is the workflow's identity.
#[test]
fn prologue_omits_absent_boundary_and_backend() {
    let yaml = "nika: floor\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let fields = started_fields(yaml, None);
    assert_eq!(
        get(&fields, "permits_json"),
        None,
        "no boundary = nothing journaled"
    );
    assert_eq!(
        get(&fields, "sandbox"),
        None,
        "an unnamed backend stays absent"
    );
    assert!(
        get(&fields, "semantic_hash").is_some(),
        "the identity is always journaled"
    );
}

/// #889 — the waiver is ATTESTED: the policy posture and the waived flag
/// ride the opening frame when the run proceeds unconfined with
/// `permits:` declared under `NIKA_SANDBOX=off`; every other posture
/// leaves them absent (never an invented claim).
#[test]
fn the_waiver_is_attested_on_the_opening_frame() {
    let yaml = "nika: pay\npermits:\n  fs: { read: [\"./in/**\"] }\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let waived = started_fields_full(
        yaml,
        Some("noop"),
        Some("off"),
        true,
        &BTreeMap::new(),
        None,
        None,
        None,
        None,
    );
    assert_eq!(get(&waived, "sandbox"), Some("noop"));
    assert_eq!(get(&waived, "sandbox_policy"), Some("off"));
    assert_eq!(get(&waived, "sandbox_waived"), Some("true"));

    let auto = started_fields(yaml, Some("seatbelt"));
    assert_eq!(
        get(&auto, "sandbox_policy"),
        None,
        "an unrecorded posture stays absent"
    );
    assert_eq!(
        get(&auto, "sandbox_waived"),
        None,
        "no waiver = nothing attested"
    );
}

/// F-P13 (NEP-0014 law 2) — the boot manifest journals the origin of
/// every input the run binds (`inputs` = one JSON map field), and a run
/// without origins carries NO claim (absent is honest).
#[test]
fn prologue_journals_the_input_origins() {
    let yaml = "nika: pay\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let origins = BTreeMap::from([
        ("count".to_owned(), InputOrigin::CiContext),
        ("region".to_owned(), InputOrigin::File),
    ]);
    let fields = started_fields_with_origins(yaml, None, &origins);
    let inputs: serde_json::Value =
        serde_json::from_str(get(&fields, "inputs").expect("the origins are journaled"))
            .expect("inputs is valid JSON");
    assert_eq!(
        inputs,
        serde_json::json!({ "count": "ci-context", "region": "file" }),
        "each bound input names its channel"
    );

    // Empty = no claim (a run without inputs never speaks).
    let bare = started_fields(yaml, None);
    assert_eq!(get(&bare, "inputs"), None, "no origins = nothing journaled");
}

/// F-P21 (NEP-0014 law 4) — a resume under a DECLARED compat attests
/// the crossing on the boot manifest (`resumed_from_engine` +
/// `resume_compat: declared`); an exact resume journals no claim.
#[test]
fn prologue_attests_the_declared_cross_version_compat() {
    let yaml = "nika: pay\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let fields = started_fields_full(
        yaml,
        None,
        None,
        false,
        &BTreeMap::new(),
        Some("0.105.0"),
        None,
        None,
        None,
    );
    assert_eq!(get(&fields, "resumed_from_engine"), Some("0.105.0"));
    assert_eq!(get(&fields, "resume_compat"), Some("declared"));

    // No crossing declared → both fields absent (never a guess).
    let exact = started_fields(yaml, None);
    assert_eq!(get(&exact, "resumed_from_engine"), None);
    assert_eq!(get(&exact, "resume_compat"), None);
}

/// ADR-099 trust amendment (2026-08-08) — a resume that proceeded PAST
/// a chain finding attests the opt-out on the boot manifest
/// (`resume_unverified: declared` + the walk's finding): a laundered
/// trace can never claim a clean ancestry silently. A verified resume
/// (or none) journals NO claim — never a flag echo.
#[test]
fn prologue_attests_the_resume_unverified_opt_out() {
    let yaml = "nika: pay\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let fields = started_fields_full(
        yaml,
        None,
        None,
        false,
        &BTreeMap::new(),
        None,
        Some(&crate::resume::ResumeUnverified::Declared(
            "chain BROKEN at line 31 — recorded 856411a17a21b83f · computed 82585b166114d2f2"
                .to_owned(),
        )),
        None,
        None,
    );
    assert_eq!(get(&fields, "resume_unverified"), Some("declared"));
    assert!(
        get(&fields, "resume_unverified_finding").is_some_and(|f| f.contains("BROKEN at line 31")),
        "the walk's finding rides: {fields:?}"
    );

    // A verified resume (or none) → both fields absent (no claim).
    let clean = started_fields(yaml, None);
    assert_eq!(get(&clean, "resume_unverified"), None);
    assert_eq!(get(&clean, "resume_unverified_finding"), None);
}

/// ADR-099 trust amendment, the strip-attack arm — a resume over a
/// CHAINLESS trace (a `--json` stream capture · a pre-0.96 journal · a
/// forgery whose `chain` fields were deleted to convert the walker's
/// `Broken` into `Unchained`) proceeds under the chainless-capture
/// compat, and the boot manifest ATTESTS it (`unchained` + the reason).
/// The posture token is NOT `declared`: no opt-out flag was named, and
/// the journal never claims one.
#[test]
fn prologue_attests_the_unchained_resume_compat() {
    let yaml = "nika: pay\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let fields = started_fields_full(
        yaml,
        None,
        None,
        false,
        &BTreeMap::new(),
        None,
        Some(&crate::resume::ResumeUnverified::Unchained(
            "the trace carries no tamper-evidence chain".to_owned(),
        )),
        None,
        None,
    );
    assert_eq!(get(&fields, "resume_unverified"), Some("unchained"));
    assert!(
        get(&fields, "resume_unverified_finding")
            .is_some_and(|f| f.contains("no tamper-evidence chain")),
        "the reason rides: {fields:?}"
    );
}

/// F-P18 (NEP-0017 · la table de prix DANS le pin) — the boot manifest
/// pins the pricing table the run's costs were billed against, as ONE
/// JSON object naming the schema marker + the snapshot's `as_of` +
/// sha256 prefix. The pin is byte-stable in its field name (`pricing`)
/// and reads EXACTLY the compile-time catalog identity — « un coût
/// rejoué en 2031 se lit contre la table 2026 pinnée ».
#[test]
fn prologue_pins_the_pricing_table_identity() {
    let yaml = "nika: pay\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let fields = started_fields(yaml, None);
    let pin: serde_json::Value = serde_json::from_str(
        get(&fields, "pricing").expect("the pricing pin rides the boot frame"),
    )
    .expect("pricing is one JSON document");
    let snapshot = nika_catalog::pricing_snapshot();
    assert_eq!(
        pin,
        serde_json::json!({
            "schema": nika_catalog::PRICING_SCHEMA,
            "as_of": snapshot.as_of,
            "sha256_16": snapshot.source_sha256_16,
        }),
        "the pin IS the compile-time table identity, no more, no less"
    );
    // The schema marker is the @1.3 law's own — locked, never drifted.
    assert_eq!(pin["schema"], "nika/model-pricing@1.3");
}

/// F-P18 — the resolved operator budget rides the boot frame as
/// `{"max_cost_usd": dollars}` with dollars a JSON NUMBER (the
/// `total_cost_usd` float convention); an unbounded run journals NO
/// budget key (absent is honest — never a fake zero/unbounded claim),
/// and a non-finite budget can never reach the journal as `null`.
#[test]
fn prologue_journals_the_budget_only_when_bounded() {
    let yaml = "nika: pay\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let bounded = started_fields_full(
        yaml,
        None,
        None,
        false,
        &BTreeMap::new(),
        None,
        None,
        Some(0.05),
        None,
    );
    let budget: serde_json::Value =
        serde_json::from_str(get(&bounded, "budget").expect("a bounded run journals its budget"))
            .expect("budget is one JSON document");
    assert_eq!(budget, serde_json::json!({ "max_cost_usd": 0.05 }));
    assert!(
        budget["max_cost_usd"].is_number(),
        "dollars ride as a number, never a string"
    );

    // Unbounded → the key stays ABSENT (no claim, never a guess).
    let unbounded = started_fields(yaml, None);
    assert_eq!(
        get(&unbounded, "budget"),
        None,
        "no budget = nothing journaled"
    );
    // …and a NaN/inf budget is filtered before the wire (the CLI
    // refuses it at the flag; the journal guard is the second wall).
    let non_finite = started_fields_full(
        yaml,
        None,
        None,
        false,
        &BTreeMap::new(),
        None,
        None,
        Some(f64::NAN),
        None,
    );
    assert_eq!(get(&non_finite, "budget"), None);
}

/// Issue 772 — the composer's `--model` override rides the boot frame
/// so a resume can judge the seat it would run on; an override-less run
/// journals NO `model_override` key (absent is honest — no claim,
/// never a guess).
#[test]
fn prologue_journals_the_model_override_only_when_declared() {
    let yaml = "nika: pay\ntasks:\n  t:\n    exec: { command: [\"echo\", \"x\"] }\n";
    let overridden = started_fields_full(
        yaml,
        None,
        None,
        false,
        &BTreeMap::new(),
        None,
        None,
        None,
        Some("mock/override"),
    );
    assert_eq!(
        get(&overridden, "model_override"),
        Some("mock/override"),
        "an override run journals the seat it actually ran on"
    );

    // No override → the key stays ABSENT (no claim, never a guess).
    let bare = started_fields(yaml, None);
    assert_eq!(
        get(&bare, "model_override"),
        None,
        "no override = nothing journaled"
    );
}
