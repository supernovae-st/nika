// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `inputs:` — the per-beat `--var` pairs (#1370): the scalar law, the
//! key law, the known-keys remedy, and the generation's stability (a beat
//! without inputs keeps the exact `nika/arm-gen@2` bytes it always had).

use super::{HEALTHY, kinds};
use crate::{ArmGeneration, Beat, CadenceErrorKind, parse_registry, validate};

/// A beat binds its workflow's declared `inputs:` the way a run's
/// `--var KEY=VALUE` does: every scalar (string · number · bool) is
/// carried as its `--var` text, in key order, so a tenant-parameterized
/// workflow is armed ONCE per tenant instead of rendered once per tenant.
#[test]
fn a_beat_carries_scalar_inputs_as_var_pairs() {
    let reg = parse_registry(
        "
nika: proj
arm:
  - workflow: workflows/tenant-report.nika
    cadence: TZ=Europe/Paris 0 9 * * 1
    plafond: 0.35
    manqué: sauter
    inputs:
      tenant: acme
      limit: 5
      dry: true
      note: \"quoted text\"
",
    )
    .expect("parse");
    assert_eq!(validate(&reg).count(), 0, "inputs are a lawful beat key");
    let beat = reg.beats().next().expect("un beat");
    let pairs: Vec<String> = beat.input_vars().collect();
    assert_eq!(
        pairs,
        ["dry=true", "limit=5", "note=quoted text", "tenant=acme"],
        "the --var text, key-sorted (BTreeMap · deterministic for the hash)"
    );
    assert_eq!(beat.inputs.get("limit").map(String::as_str), Some("5"));
    // Absent `inputs:` reads as EMPTY — a beat without inputs is the
    // beat it always was.
    let plain = parse_registry(HEALTHY).expect("parse");
    let plain = plain.beats().next().expect("un beat");
    assert!(plain.inputs.is_empty());
    assert_eq!(plain.input_vars().count(), 0);
}

/// The `--var` door takes ONE scalar per key: a list, a map or a null
/// refuses at parse (the closed grammar's voice), naming the key and
/// teaching the JSON-text spelling a typed array input accepts.
#[test]
fn a_beat_input_is_one_scalar_never_a_collection_or_a_null() {
    for (value, what) in [
        ("[a, b]", "a list"),
        ("{ k: v }", "a map"),
        ("null", "a null"),
        ("~", "a tilde null"),
    ] {
        let yaml = format!(
            "nika: proj\narm:\n  - workflow: workflows/a.nika\n    cadence: on-webhook\n    \
             plafond: 0.35\n    manqué: sauter\n    inputs:\n      flags: {value}\n"
        );
        let err = parse_registry(&yaml).expect_err(what);
        assert_eq!(err.kind(), CadenceErrorKind::Grammar, "{what}");
        assert!(
            err.detail().contains("inputs") && err.detail().contains("scalar"),
            "{what} · the refusal names the door and its shape — vu {}",
            err.detail()
        );
    }
    // A key written twice never last-wins: two values for one `--var
    // KEY` would be a guess about which tenant fires.
    let err = parse_registry(
        "nika: proj\narm:\n  - workflow: workflows/a.nika\n    cadence: on-webhook\n    \
         plafond: 0.35\n    manqué: sauter\n    inputs:\n      tenant: acme\n      tenant: globex\n",
    )
    .expect_err("a duplicate key");
    assert_eq!(err.kind(), CadenceErrorKind::Grammar);
    assert!(err.detail().contains("tenant"), "vu {}", err.detail());
    // The quoted JSON text IS the spelling for a typed array/object
    // input (`--var flags=["a","b"]` — the declared type coerces it).
    let reg = parse_registry(
        "nika: proj\narm:\n  - workflow: workflows/a.nika\n    cadence: on-webhook\n    \
         plafond: 0.35\n    manqué: sauter\n    inputs:\n      flags: '[\"a\",\"b\"]'\n",
    )
    .expect("json text is a string scalar");
    let beat = reg.beats().next().expect("un beat");
    assert_eq!(
        beat.input_vars().next().as_deref(),
        Some("flags=[\"a\",\"b\"]")
    );
}

/// The key is the `--var KEY` — the shape `parse_var_overrides` splits
/// at the first `=` and trims: an empty key, a key carrying `=`,
/// whitespace or a control character can never round-trip through that
/// door, so the law refuses it here, by name, with the fix.
#[test]
fn a_beat_input_key_is_the_var_key_shape() {
    for key in ["\"\"", "\"a b\"", "\"a=b\"", "\"tab\\tkey\"", "\" lead\""] {
        let yaml = format!(
            "nika: proj\narm:\n  - workflow: workflows/a.nika\n    cadence: on-webhook\n    \
             plafond: 0.35\n    manqué: sauter\n    inputs:\n      {key}: x\n"
        );
        let faults = kinds(&yaml);
        assert_eq!(
            faults,
            [CadenceErrorKind::InputName],
            "{key} · refused by name"
        );
    }
    // The membership judgment (declared by the workflow or not) is the
    // fire edge's — this grammar never opens the workflow file, so a
    // well-shaped key it cannot see the declaration of passes here.
    let faults = kinds(
        "nika: proj\narm:\n  - workflow: workflows/a.nika\n    cadence: on-webhook\n    \
         plafond: 0.35\n    manqué: sauter\n    inputs:\n      tenant_id: acme\n      Region: eu\n      \
         k-1: v\n",
    );
    assert!(faults.is_empty(), "vu {faults:?}");
}

/// The closed grammar's teaching line names `inputs` among the known
/// beat keys — a typo'd `input:` learns the spelling from the refusal.
#[test]
fn the_known_keys_message_names_inputs() {
    let err = parse_registry(
        "nika: proj\narm:\n  - workflow: workflows/a.nika\n    cadence: on-webhook\n    \
         plafond: 0.35\n    manqué: sauter\n    input: { tenant: acme }\n",
    )
    .expect_err("clé inconnue");
    assert_eq!(err.kind(), CadenceErrorKind::Grammar);
    assert!(
        err.remedy().contains("inputs"),
        "the remedy lists the key — vu {}",
        err.remedy()
    );
}

/// The inputs are a DECLARED field: two beats that differ only by their
/// inputs mint two generations (a re-parameterized beat is a new
/// declaration) — while a beat WITHOUT inputs keeps the exact generation
/// it minted before the key existed (the ledger's pinned evidence stays
/// interpretable · no `@3` domain bump).
#[test]
fn the_inputs_enter_the_generation_only_when_present() {
    use sha2::Digest as _;
    let head = "nika: proj\narm:\n  - workflow: workflows/doctor.nika\n    \
                cadence: \"TZ=UTC 0 3 * * *\"\n    plafond: 0.25\n    manqué: sauter\n";
    let digest = "f".repeat(64);
    let plain = parse_registry(head).expect("parse");
    let plain = plain.beats().next().expect("un beat");
    let acme = parse_registry(&format!("{head}    inputs: {{ tenant: acme }}\n")).expect("parse");
    let acme = acme.beats().next().expect("un beat");
    let globex =
        parse_registry(&format!("{head}    inputs: {{ tenant: globex }}\n")).expect("parse");
    let globex = globex.beats().next().expect("un beat");
    let generation = |b: &Beat| ArmGeneration::compute(b, &digest);
    assert_ne!(
        generation(plain),
        generation(acme),
        "inputs are declared · a new gen"
    );
    assert_ne!(
        generation(acme),
        generation(globex),
        "one value changed · a new gen"
    );
    // The pre-#1370 canonical form, byte for byte, still hashes the
    // plain beat.
    let mut preimage = "nika/arm-gen@2\nworkflow=\"workflows/doctor.nika\"\ncadence=\"TZ=UTC 0 3 * * *\"\noù=null\nplafond=0.25\nmanqué=\"sauter\"\nchevauchement=null\naprès_saut=null\nactif=null\nraison=null\njusqu_au=null\ntolérance=null\ndécalage=null\npar=null".to_owned().into_bytes();
    preimage.push(0);
    preimage.extend_from_slice(digest.as_bytes());
    let expected = format!("{:x}", sha2::Sha256::digest(&preimage));
    assert_eq!(
        generation(plain).as_str(),
        expected,
        "no inputs · the historical form"
    );
    // With inputs: ONE extra line, `inputs=` + quoted key=value pairs,
    // key-sorted, comma-joined — the canonical spelling pinned from
    // without.
    let mut preimage = "nika/arm-gen@2\nworkflow=\"workflows/doctor.nika\"\ncadence=\"TZ=UTC 0 3 * * *\"\noù=null\nplafond=0.25\nmanqué=\"sauter\"\nchevauchement=null\naprès_saut=null\nactif=null\nraison=null\njusqu_au=null\ntolérance=null\ndécalage=null\npar=null\ninputs=\"tenant\"=\"acme\"".to_owned().into_bytes();
    preimage.push(0);
    preimage.extend_from_slice(digest.as_bytes());
    let expected = format!("{:x}", sha2::Sha256::digest(&preimage));
    assert_eq!(
        generation(acme).as_str(),
        expected,
        "inputs · one appended line"
    );
}
