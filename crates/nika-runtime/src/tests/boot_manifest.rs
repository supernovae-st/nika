// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

// ─── the F-P2 boot manifest (attestation inputs follow the declaration) ─────

/// The boot manifest claims only what exists, per the `run:`
/// declaration: absent → the system stamper + the system clock, no seed
/// claim · `entropy: none` → deterministic + virtual + the zero seed ·
/// `seeded(42)` → seed 42 · a lone `clock: virtual` is a test
/// configuration (system stamper · virtual clock · no seed). `spec_pin`
/// always rides (the workspace `SPEC_PIN` carries a hash line).
#[test]
fn the_boot_manifest_follows_the_run_declaration() {
    const HEAD: &str =
        "nika: w\npermits: { exec: [\"x\"] }\ntasks:\n  t:\n    exec: { command: [\"x\"] }\n";
    let dump = |yaml: &str| {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        format!("{:?}", crate::prologue::boot_attestation_fields(&wf))
    };
    let ambient = dump(HEAD);
    assert!(ambient.contains("spec_pin"), "{ambient}");
    assert!(
        ambient.contains(crate::engine_identity().spec_sha()),
        "{ambient}"
    );
    assert!(
        ambient.contains("stamper_kind") && ambient.contains("system"),
        "{ambient}"
    );
    assert!(ambient.contains("clock"), "{ambient}");
    assert!(
        !ambient.contains("seed"),
        "no seed claim on ambient: {ambient}"
    );
    let strict = dump(&format!("{HEAD}run: {{ entropy: none }}\n"));
    assert!(strict.contains("deterministic"), "{strict}");
    assert!(
        strict.contains("virtual"),
        "the forced virtual clock: {strict}"
    );
    assert!(
        strict.contains("seed"),
        "the zero stream is a claim: {strict}"
    );
    let seeded = dump(&format!("{HEAD}run: {{ entropy: {{ seeded: 42 }} }}\n"));
    assert!(seeded.contains("42"), "{seeded}");
    let lone_clock = dump(&format!("{HEAD}run: {{ clock: virtual }}\n"));
    assert!(
        lone_clock.contains("virtual") && !lone_clock.contains("seed"),
        "a lone virtual clock claims no determinism: {lone_clock}"
    );
}
