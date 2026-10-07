// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
#![allow(clippy::disallowed_methods)]

//! Spec conformance · the FULL `core` suite.
//!
//! Walks every fixture under `nika-spec/conformance/tests/core/**`
//! (`input.yaml` + `expected.json`) and verdicts `parse` + `analyze` plus
//! every hard-invalidating `check` code (no namespace filter · tier scoping
//! lives in the fixture) against `conformance/runner-protocol.md` ·
//!
//! - `valid: true`  → the engine MUST accept (zero errors).
//! - `valid: false` → the engine MUST reject AND at least one emitted
//!   error matches at least one expected entry · exact `code` match OR
//!   `namespace`-prefix + `category` match.
//! - `mode` defaults to `strict` (« the test default »).
//!
//! The spec dir resolves from `$NIKA_SPEC_DIR` or the sibling checkout
//! (`<engine>/../spec`) — the suite HARD-FAILS when missing (the
//! conformance gate must never silently skip). Harness plumbing is
//! shared with the `deep` tier (`tests/common/mod.rs`).
//!
//! The `CORE_GAPS` ledger mirrors the deep tier's ratchet: every entry
//! names the wave that owns the fixture's expected verdict, and the
//! suite asserts BOTH directions — a fixture NOT in the ledger MUST
//! pass, a fixture IN the ledger MUST still fail (a landed wave forces
//! removing its row · the ledger cannot go stale).

mod common;

use std::path::PathBuf;

use common::{
    check_extra, fixture_dirs, fixture_verdict, run_engine, skip_in_mutants_sandbox, spec_dir,
};
use nika_schema::ParseMode;

/// The core-tier gap ledger · fixture-name prefix → why the engine does
/// not hold the verdict yet. Closing a gap = implement + DELETE the row.
const CORE_GAPS: &[(&str, &str)] = &[
    // ── The R5 predicates wave CLOSED (spec #118 · the engine speaks
    // the outcome-class spellings success·failure·skipped·terminal —
    // the 8 after:-carrying rows deleted the day the rename landed, per
    // the ratchet « a landed wave forces removing its row »).
    // ── R3b · envelope/010 CLOSED 2026-07-30: the operator locked TYPE
    // (conformance `runner-protocol.md` class D — « the type system owns
    // the type-fit ») and the spec re-pointed the fixture at the
    // NIKA-TYPE-001 both oracles already emitted. The row deleted the
    // day the pin landed, per the ratchet.
    // ── EMPTY on purpose. Every core fixture must pass and the ledger is
    // bidirectional: adding a row here is a claim that the engine
    // CANNOT hold a verdict, and the suite proves that claim by
    // failing when the fixture starts passing.
];

#[test]
fn core_conformance_suite() {
    if skip_in_mutants_sandbox() {
        return;
    }
    let core = spec_dir().join("conformance/tests/core");
    assert!(
        core.is_dir(),
        "conformance dir missing: {} — set NIKA_SPEC_DIR",
        core.display()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut total = 0_usize;
    let mut gaps_hit = 0_usize;

    for dir in fixture_dirs(&core) {
        total += 1;
        let label = dir
            .strip_prefix(&core)
            .unwrap_or(&dir)
            .display()
            .to_string();
        let gap = CORE_GAPS.iter().find(|(name, _)| label.starts_with(name));
        let verdict = fixture_verdict(&dir, false);

        match (gap, verdict) {
            // Not in the ledger · must pass.
            (None, Some(failure)) => failures.push(format!("{label} · {failure}")),
            (None, None) => {}
            // In the ledger · must STILL fail (else the row is stale).
            (Some((name, reason)), None) => failures.push(format!(
                "{label} · PASSES but is in CORE_GAPS (`{name}` · {reason}) — \
                 the wave landed · DELETE its ledger row"
            )),
            (Some(_), Some(_)) => gaps_hit += 1,
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {total} core fixtures FAILED ·\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert_eq!(
        gaps_hit,
        CORE_GAPS.len(),
        "ledger drift — {gaps_hit} gap fixtures hit vs {} ledger rows \
         (a renamed/removed fixture leaves a dead row)",
        CORE_GAPS.len()
    );
    // Sanity floor — the suite ships 5 groups · 40+ fixtures.
    assert!(total >= 30, "only {total} fixtures walked — layout drift?");
}

// ── Fixture-owned context (spec `conformance/runner-protocol.md`: a
// fixture's context files belong to the fixture, never to the caller).

/// The private marker a parent sets on its re-entrant child runs.
const CONTEXT_CHILD: &str = "NIKA_CORE_CONTEXT_CHILD";
/// What a child prints once every check of the pair has run.
const CONTEXT_CHILD_DONE: &str = "core-context child: pair complete";
const MCP_CONTEXT_TEST: &str = "mcp_fixture_context_is_file_relative_and_absence_stays_refused";

/// A unique scratch directory this test owns and removes.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("nika-core-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create scratch");
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn verbs_shape(case: &str) -> PathBuf {
    spec_dir()
        .join("conformance/tests/core/verbs-shape")
        .join(case)
}

fn code_names(codes: &[nika_schema::SpecCode]) -> Vec<String> {
    codes.iter().map(ToString::to_string).collect()
}

/// The 012 pair, judged from the CURRENT process cwd: the fixture with its
/// declared `.nika/mcp_servers.json` is valid; a private copy of the same
/// workflow without that file is refused `NIKA-INVOKE-001`.
fn mcp_context_pair() {
    let fixture = verbs_shape("012-mcp-digit-server-valid");
    let yaml = std::fs::read_to_string(fixture.join("input.yaml")).expect("012 input");
    assert!(
        fixture.join(".nika/mcp_servers.json").is_file(),
        "012 declares its registry"
    );
    let analyzed = run_engine(&yaml, ParseMode::Strict);
    assert!(analyzed.is_empty(), "012 analyze: {analyzed:?}");
    let present = check_extra(&yaml, ParseMode::Strict, &fixture);
    assert!(
        present.is_empty(),
        "012 with its own registry: {:?}",
        code_names(&present)
    );
    assert_eq!(fixture_verdict(&fixture, false), None, "012 verdict");

    let bare = Scratch::new("mcp-absent");
    std::fs::write(bare.0.join("input.yaml"), &yaml).expect("private copy");
    let absent = code_names(&check_extra(&yaml, ParseMode::Strict, &bare.0));
    assert!(
        absent.iter().any(|c| c == "NIKA-INVOKE-001"),
        "012 without a registry must stay refused: {absent:?}"
    );
}

/// The 012 context is the fixture's own file, wherever the runner stands.
///
/// The pair runs in two child processes of this same test binary (`--exact`
/// plus a private marker): one from a cwd holding no registry, one from a cwd
/// holding a byte-exact copy of the fixture's registry. A reader that
/// resolves `.nika/mcp_servers.json` against the cwd fails the positive in
/// the first child and accepts the negative in the second. Only the children
/// get a cwd and HOME; the parent's process state is never changed.
#[test]
#[allow(
    clippy::disallowed_types,
    reason = "re-runs this same test binary from scratch cwds; no workflow or tool is spawned"
)]
fn mcp_fixture_context_is_file_relative_and_absence_stays_refused() {
    if std::env::var_os(CONTEXT_CHILD).is_some() {
        mcp_context_pair();
        #[allow(clippy::disallowed_macros, clippy::print_stderr)]
        {
            eprintln!("{CONTEXT_CHILD_DONE}");
        }
        return;
    }
    if skip_in_mutants_sandbox() {
        return;
    }
    let registry =
        std::fs::read(verbs_shape("012-mcp-digit-server-valid").join(".nika/mcp_servers.json"))
            .expect("012 registry bytes");
    let scratch = Scratch::new("mcp-cwd");
    let bare = scratch.0.join("cwd-without-registry");
    let ambient = scratch.0.join("cwd-with-registry");
    let home = scratch.0.join("home");
    for dir in [&bare, &ambient, &home] {
        std::fs::create_dir_all(dir).expect("child dir");
    }
    std::fs::create_dir_all(ambient.join(".nika")).expect("ambient .nika");
    std::fs::write(ambient.join(".nika/mcp_servers.json"), &registry).expect("ambient registry");
    let exe = std::env::current_exe().expect("this test binary");
    for cwd in [&bare, &ambient] {
        let out = std::process::Command::new(&exe)
            .args([
                MCP_CONTEXT_TEST,
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .current_dir(cwd)
            .env(CONTEXT_CHILD, "1")
            .env("NIKA_SPEC_DIR", spec_dir())
            .env("HOME", &home)
            .output()
            .expect("spawn the child run");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let report = format!(
            "cwd {}\n--- stdout\n{stdout}\n--- stderr\n{stderr}",
            cwd.display()
        );
        assert!(out.status.success(), "child failed · {report}");
        assert!(
            stdout.contains("running 1 test"),
            "exactly one child test · {report}"
        );
        assert!(
            stdout.contains("1 passed"),
            "the child test ran and passed · {report}"
        );
        assert!(
            stderr.contains(CONTEXT_CHILD_DONE),
            "the pair completed · {report}"
        );
    }
}

/// Replace the one line `from` by `to`, insisting it occurs exactly once.
fn replace_line(yaml: &str, from: &str, to: &str) -> String {
    assert_eq!(yaml.matches(from).count(), 1, "`{from}` occurs once");
    yaml.replace(from, to)
}

/// 003 is valid as corrected (schema + done, no read tool) and its former
/// shape stays refused: `nika:read` named in `permits.tools` and the agent's
/// tools without any `permits.fs.read` is `NIKA-SEC-004` through the same
/// harness projection, so the harness does not mask the effect.
#[test]
fn agent_schema_fixture_keeps_missing_read_authority_invalid() {
    if skip_in_mutants_sandbox() {
        return;
    }
    let fixture = verbs_shape("003-agent-with-schema-valid");
    assert_eq!(fixture_verdict(&fixture, false), None, "003 as corrected");
    let yaml = std::fs::read_to_string(fixture.join("input.yaml")).expect("003 input");
    assert!(yaml.contains("schema:") && !yaml.contains("nika:read") && !yaml.contains("fs:"));
    // The former shape (spec before the correction): the read tool in both lists.
    let witness = replace_line(
        &replace_line(
            &yaml,
            "\n  tools: [\"nika:done\"]\n",
            "\n  tools: [\"nika:done\", \"nika:read\"]\n",
        ),
        "\n      tools: [\"nika:done\"]\n",
        "\n      tools: [\"nika:read\", \"nika:done\"]\n",
    );
    assert!(witness.contains("schema:") && !witness.contains("fs:"));
    let analyzed = run_engine(&witness, ParseMode::Strict);
    assert!(analyzed.is_empty(), "witness parses: {analyzed:?}");
    let codes = code_names(&check_extra(&witness, ParseMode::Strict, &fixture));
    assert!(
        codes.iter().any(|c| c == "NIKA-SEC-004"),
        "the read tool without a read grant stays refused: {codes:?}"
    );
}
