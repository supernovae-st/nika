// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used)]

//! Tests for the `run` verb — split out of `mod.rs` when the F4
//! `--answer` pre-seed pushed the file past the 1500-LOC hard cap
//! (ADR-023). A pure move at the `#[cfg(test)]` boundary: same
//! cases, same imports, no logic touched.

use std::collections::BTreeMap;

use super::dry_run::swap as dry_run_swap;
use super::sink::TraceSurface;
use super::{RenderMode, capture_mock_outputs, exit, run, surfaced_trace};
use crate::Theme;
use serde_json::json;

#[test]
fn registry_run_refusal_keeps_copy_guidance_and_never_names_cache_as_fixable() {
    let dir =
        std::env::temp_dir().join(format!("nika-registry-run-refusal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("registry-run arena");
    let path = dir.join("cached.nika.yaml");
    std::fs::write(
        &path,
        "nika: cached\npermits: { exec: [date] }\ntasks:\n  clock:\n    exec: { command: [date] }\n  broken:\n    infer: { prompt: '${{ tasks.ghost.output }}', model: mock/echo }\n",
    )
    .expect("dirty registry fixture");
    let cache_path = path.to_string_lossy().into_owned();
    let (source, _wf, report) = super::provenance::capture_checked_source(
        &cache_path,
        Some(nika_display::check_render::RepairTarget::RegistryArtifact),
        (false, false),
    )
    .ok()
    .expect("registry acquisition reads the cached bytes");
    assert!(!report.is_clean(), "the run must stop at its clean gate");
    assert_eq!(
        source.repair_target(),
        nika_display::check_render::RepairTarget::RegistryArtifact
    );

    let out = crate::verbs::check::run_source_with_profile(
        &source,
        false,
        false,
        crate::verbs::check::Profile::Advisory,
        (None, None),
        Theme::new(false, false, false),
    );
    assert_eq!(out.code, exit::FILE);
    assert!(
        out.text
            .contains("copy the registry artifact into your workspace")
    );
    assert!(out.text.contains("nika check --fix <copy>"));
    assert!(!out.text.contains(&format!("--fix {cache_path}")));
}

/// The #332 plan object: waves resolve indices → task ids, one
/// `{id, verb}` row per task, the report's cost/permits/requirements
/// ride verbatim, and `effects_executed` states the contract.
#[test]
fn dry_run_payload_projects_the_versioned_plan() {
    let yaml = "nika: demo\nmodel: mock/echo\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n  b:\n    with:\n      prev: ${{ tasks.a.output }}\n    infer: { prompt: \"go ${{ with.prev }}\", max_tokens: 10 }\n\noutputs:\n  out: \"${{ tasks.b.output }}\"\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::source::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    let p = nika_check::plan::payload("demo.nika.yaml", &wf, &report);
    assert_eq!(p["plan_version"], 1);
    assert_eq!(p["workflow"], "demo");
    assert_eq!(p["waves"], json!([["a"], ["b"]]));
    assert_eq!(p["tasks"][0]["verb"], "exec");
    assert_eq!(p["tasks"][1]["verb"], "infer");
    assert_eq!(p["effects_executed"], false);
    assert_eq!(p["permits"]["source"], "absent");
    assert!(p["cost"].is_object() && p["requirements"].is_object());
}

/// The empty-state voice (design §3 rider): an ENV refusal (missing
/// file) carries the house prefix + ONE fix line + a closing
/// newline; FILE findings pass through to the check card untouched.
#[test]
fn refusal_text_teaches_only_the_env_class() {
    let env = crate::verbs::VerbOutput {
        text: "cannot read demo.yaml: No such file or directory (os error 2)".to_owned(),
        code: exit::ENV,
    };
    let voiced = super::refusal_text(&env);
    assert!(
        voiced.starts_with("nika run: cannot read demo.yaml"),
        "{voiced}"
    );
    assert!(voiced.contains("fix: check the path"), "{voiced}");
    assert!(voiced.ends_with('\n'), "closes its own line: {voiced:?}");

    let findings = crate::verbs::VerbOutput {
        text: "PARSE X  [NIKA-PARSE-009] two verbs".to_owned(),
        code: exit::FILE,
    };
    assert_eq!(super::refusal_text(&findings), findings.text);
}

#[test]
fn arbitrary_trace_note_error_is_env_with_the_exact_path() {
    let path = std::path::PathBuf::from(".nika/traces/exact.ndjson");
    let surface = TraceSurface {
        path: Some(path.clone()),
        proof: None,
        note_error: Some(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "injected note refusal",
        )),
    };
    let verdict = surfaced_trace(surface).expect_err("the note refusal must be terminal");
    assert_eq!(verdict.code, exit::ENV);
    assert_eq!(verdict.trace, Some(path));
}

/// A noiseless theme (no colour · no animation) for the run tests — they
/// exercise the COMPOSITION + exit code, not the render surface.
fn plain_theme() -> Theme {
    Theme::new(false, true, false)
}

fn stage(name: &str, yaml: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("nika-cli-run-mod-tests");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(name);
    std::fs::write(&path, yaml).expect("fixture written");
    path
}

/// `--model mock/echo` on a workflow whose envelope is a LOCAL model
/// resolves + runs to SUCCESS offline — mock/echo needs no provider, so
/// the override is the offline-preview path the example tip suggests.
#[test]
fn model_override_runs_a_local_model_workflow_offline() {
    let wf = stage(
        "override-infer.nika.yaml",
        "nika: override-infer\nmodel: ollama/llama3.1\ntasks:\n  think:\n    infer: { prompt: \"hello\" }\n",
    );
    let code = run(
        &wf.to_string_lossy(),
        false,
        None,
        plain_theme(),
        RenderMode::Plain,
        false,
        Some("mock/echo"),
        None, // no --access pin
        &[],
        None,
        true, // tests never write .nika/traces (cwd hygiene)
        None, // whole-workflow runs (scoping has its own tests)
        false,
        None,
        true,
        false, // unsigned-tolerant (the signature gate has its own test)
    );
    assert_eq!(
        code,
        exit::OK,
        "the mock/echo override runs the local-model workflow offline"
    );
}

/// The override actually CHANGES the resolved model: the same workflow
/// that needs a provider (`ollama/llama3.1`) succeeds because the
/// override swapped in the keyless/networkless mock — proving the
/// envelope model was not the one resolved.
#[test]
fn model_override_replaces_the_resolved_model() {
    let wf = stage(
        "override-swap.nika.yaml",
        "nika: override-swap\nmodel: ollama/llama3.1\ntasks:\n  ask:\n    infer: { prompt: \"bonjour\" }\n",
    );
    // With the override → mock/echo resolves with no provider → OK.
    let overridden = run(
        &wf.to_string_lossy(),
        false,
        None,
        plain_theme(),
        RenderMode::Plain,
        false,
        Some("mock/echo"),
        None, // no --access pin
        &[],
        None,
        true, // tests never write .nika/traces (cwd hygiene)
        None, // whole-workflow runs (scoping has its own tests)
        false,
        None,
        true,
        false, // unsigned-tolerant (the signature gate has its own test)
    );
    assert_eq!(
        overridden,
        exit::OK,
        "the override resolved mock/echo, not the envelope's ollama model"
    );
}

// ── `--answer` without `--resume` (the 2026-07-30 audit · F4) ───────

/// The CI one-pass gate: a FRESH run with a pre-seeded answer
/// consumes it at the gate and completes — no trace, no resume, no
/// TTY. Before F4 the clap surface refused the pairing outright
/// (`requires = "resume"`); the gate map always could consume it.
#[test]
fn answer_without_resume_preseeds_the_gate() {
    let wf = stage(
        "answer-fresh.nika.yaml",
        "nika: gated\npermits: { exec: [\"echo\"], tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke: { tool: \"nika:prompt\", args: { mode: \"confirm\", message: \"ship?\" } }\n  done:\n    after: { ask: success }\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"echo\", \"shipped\"] }\n",
    );
    let req = nika_dap::resume::ResumeRequest {
        trace: None, // the answers-only form — no plan, no paused ticket
        from: None,
        answers: vec!["ask=true".to_owned()],
        compat: None,
        allow_unverified: false,
    };
    let code = run(
        &wf.to_string_lossy(),
        false,
        None,
        plain_theme(),
        RenderMode::Plain,
        false,
        None,
        None, // no --access pin
        &[],
        Some(&req),
        true, // tests never write .nika/traces (cwd hygiene)
        None, // whole-workflow runs (scoping has its own tests)
        false,
        None,
        true,
        false, // unsigned-tolerant (the signature gate has its own test)
    );
    assert_eq!(
        code,
        exit::OK,
        "the pre-seeded answer clears the gate on a fresh run"
    );
}

/// The same pairing still validates the answer keys against the
/// workflow — an unknown task id refuses at admission (the parse
/// never relaxes with the new form).
#[test]
fn answer_without_resume_still_refuses_an_unknown_task() {
    let wf = stage(
        "answer-unknown.nika.yaml",
        "nika: gated\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke: { tool: \"nika:prompt\", args: { mode: \"confirm\", message: \"ship?\" } }\n",
    );
    let req = nika_dap::resume::ResumeRequest {
        trace: None,
        from: None,
        answers: vec!["ghost=true".to_owned()],
        compat: None,
        allow_unverified: false,
    };
    let code = run(
        &wf.to_string_lossy(),
        false,
        None,
        plain_theme(),
        RenderMode::Plain,
        false,
        None,
        None, // no --access pin
        &[],
        Some(&req),
        true,
        None,
        false,
        None,
        true,
        false,
    );
    assert_eq!(
        code,
        exit::ENV,
        "an answer for a task that does not exist refuses at admission"
    );
}

// ── `--var` (F4) — the required-var class was UNRUNNABLE from the CLI ──

/// The workflow of the field repro: a `required: true` var with no
/// default. Before F4 there was NO way to run it from the CLI.
const REQUIRED_VAR_WF: &str = "nika: needs-var\nmodel: mock/echo\ninputs:\n  topic:\n    type: string\n    required: true\ntasks:\n  ask:\n    infer: { prompt: \"about ${{ inputs.topic }}\" }\n";

fn run_with_vars(name: &str, vars: &[String]) -> u8 {
    let wf = stage(name, REQUIRED_VAR_WF);
    run(
        &wf.to_string_lossy(),
        false,
        None,
        plain_theme(),
        RenderMode::Plain,
        false,
        None,
        None, // no --access pin
        vars,
        None,
        true, // tests never write .nika/traces (cwd hygiene)
        None, // whole-workflow runs (scoping has its own tests)
        false,
        None,
        true,
        false, // unsigned-tolerant (the signature gate has its own test)
    )
}

#[test]
fn var_flag_satisfies_a_required_var() {
    // Without the flag the run refuses at ADMISSION (issue #603 ·
    // NIKA-1708 · exit 3) — before the DAG spends a task; the mid-DAG
    // NIKA-VAR-001 at the first `${{ inputs.topic }}` read was the bug.
    assert_eq!(
        run_with_vars("var-missing.nika.yaml", &[]),
        exit::ENV,
        "an unsatisfied required input refuses at admission (#603)"
    );
    // With `--var topic=rust` the SAME workflow runs green.
    assert_eq!(
        run_with_vars("var-provided.nika.yaml", &["topic=rust".to_owned()]),
        exit::OK,
        "--var makes the required-var workflow runnable"
    );
}

#[test]
fn var_flag_refuses_unknown_keys_and_bad_shapes() {
    // A typo'd key must refuse LOUDLY (exit 3 · never silently ignored).
    assert_eq!(
        run_with_vars("var-unknown.nika.yaml", &["topik=rust".to_owned()]),
        exit::ENV,
        "unknown --var key is refused"
    );
    // A pair without `=` is an operator input error, same class.
    assert_eq!(
        run_with_vars("var-shape.nika.yaml", &["topic".to_owned()]),
        exit::ENV,
        "malformed --var pair is refused"
    );
}

/// #473 e2e (mock · offline): the resolved-skills wiring is
/// LOAD-BEARING through the production composition — the same
/// skills-carrying agent workflow settles GREEN when the composer's
/// map rides `with_skills`, and fails with the check-time code when
/// an embedder skips it (the wiring, proven from the CLI seam; the
/// injected system BYTES are pinned at the runtime's provider seam).
#[test]
fn capture_mock_outputs_carries_the_resolved_skills() {
    // Uniqueness: pid + an atomic discriminator — a pid-only dir is
    // shared by EVERY test in the process and parallel tests collide
    // (one's cleanup wiped the other's fixture under gate load — the
    // 2026-07-21 NIKA-AGENT-003 flake).
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "nika-run-skills-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let skill = dir.join("SKILL.md");
    std::fs::write(&skill, "---\nname: s\ndescription: d\n---\nBe careful.\n")
        .expect("fixture skill");
    // The grant is a PRECONDITION since the skills fs edge moved inside the
    // boundary · the fixture declares exactly the path it reaches.
    let yaml = format!(
        "nika: w\nmodel: mock/echo\npermits:\n  fs:\n    read: [\"{}\"]\ntasks:\n  go:\n    agent: {{ prompt: \"hi\", skills: [\"{}\"] }}\n",
        skill.display(),
        skill.display()
    );
    let wf = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "the pure ladder is fs-free");

    // Absolute fixture path — the base is moot here.
    let resolved = crate::verbs::resolve_workflow_skills(&wf, std::path::Path::new(""));
    assert!(resolved.findings.is_empty(), "the skill file resolves");
    let theme = Theme::new(false, true, false);
    let (code, _) =
        capture_mock_outputs(&wf, &report, resolved.texts, theme).expect("composition succeeds");
    assert_eq!(code, exit::OK, "skills composed → the mock run is green");

    // The control: WITHOUT the map the dispatch refuses (proves the
    // seam is load-bearing, not decorative).
    let (code, _) = capture_mock_outputs(&wf, &report, BTreeMap::new(), theme)
        .expect("composition still succeeds");
    assert_eq!(
        code,
        exit::WORKFLOW,
        "no skills map → NIKA-AGENT-003 task failure"
    );
}

// ── P0-16 (audit 2026-07-30) · the mock plane disables EFFECTS ────────
//
// `nika test` swaps the MODEL for mock/echo — but before the simulated
// plane, the tool/exec seams stayed REAL: a workflow whose check passed
// wrote files, ran subprocesses, fetched URLs under a verb documented
// « offline ». The simulated plane refuses net/exec/write with an honest
// « effects disabled » message; these sentinels prove zero bytes and zero
// children escape.

/// A unique-per-test temp dir (pid + atomic discriminator — the parallel-
/// test collision class the skills e2e above names).
fn sentinel_dir(tag: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "nika-test-fx-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

/// P0-16 · `invoke nika:write` under `capture_mock_outputs` never lands a
/// byte: the task is REFUSED (« effects disabled » · the run fails), and
/// the target file does not exist after the run.
#[test]
fn capture_mock_outputs_refuses_write_effects() {
    let dir = sentinel_dir("write");
    let _lease = crate::cwd::enter(&dir).expect("enter write sentinel");
    let sentinel = dir.join("sentinel.txt");
    let yaml = "nika: fx-write\nmodel: mock/echo\npermits: { tools: [\"nika:write\"], fs: { write: [\"./sentinel.txt\"] } }\ntasks:\n  w:\n    invoke: { tool: \"nika:write\", args: { path: \"./sentinel.txt\", content: \"must never land\" } }\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "a permitted write passes check");

    let (code, _) = capture_mock_outputs(&wf, &report, BTreeMap::new(), plain_theme())
        .expect("composition succeeds");
    assert_eq!(
        code,
        exit::WORKFLOW,
        "the write is refused under the simulated plane — the run cannot go green on a disabled effect"
    );
    assert!(
        !sentinel.exists(),
        "P0-16 · zero writes under `nika test`: {} must not exist",
        sentinel.display()
    );
}

/// P0-16 · an `exec` task under `capture_mock_outputs` spawns no child:
/// the sentinel the command would create never appears.
#[test]
fn capture_mock_outputs_refuses_exec_effects() {
    let dir = sentinel_dir("exec");
    let _lease = crate::cwd::enter(&dir).expect("enter exec sentinel");
    let sentinel = dir.join("sentinel.txt");
    let yaml = "nika: fx-exec\nmodel: mock/echo\npermits: { exec: [\"touch\"] }\ntasks:\n  t:\n    exec: { command: [\"touch\", \"./sentinel.txt\"] }\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "a permitted exec passes check");

    let (code, _) = capture_mock_outputs(&wf, &report, BTreeMap::new(), plain_theme())
        .expect("composition succeeds");
    assert_eq!(
        code,
        exit::WORKFLOW,
        "the exec is refused under the simulated plane — the run cannot go green on a disabled effect"
    );
    assert!(
        !sentinel.exists(),
        "P0-16 · zero subprocesses under `nika test`: {} must not exist",
        sentinel.display()
    );
}

/// `--require-signature` refuses an unsigned workflow BEFORE any task
/// executes: exit 2, and the exec task's sentinel is never created.
/// The counterfactual (the file itself is runnable) rides a dry-run —
/// plan only, zero effects.
#[test]
fn require_signature_refuses_unsigned_before_execution() {
    let dir = sentinel_dir("sig");
    let _lease = crate::cwd::enter(&dir).expect("enter sig sentinel");
    let sentinel = dir.join("sentinel.txt");
    let yaml = "nika: sig-gate\nmodel: mock/echo\npermits: { exec: [\"touch\"] }\ntasks:\n  touch:\n    exec: { command: [\"touch\", \"./sentinel.txt\"] }\n";
    let wf = dir.join("sig-gate.nika.yaml");
    std::fs::write(&wf, yaml).expect("sig fixture");
    let gated = run(
        &wf.to_string_lossy(),
        false,
        None,
        plain_theme(),
        RenderMode::Plain,
        false,
        None,
        None, // no --access pin
        &[],
        None,
        true, // tests never write .nika/traces (cwd hygiene)
        None,
        false,
        None,
        true,
        true, // --require-signature
    );
    assert_eq!(
        gated,
        exit::FILE,
        "unsigned + --require-signature must refuse (exit 2)"
    );
    assert!(
        !sentinel.exists(),
        "the gate fired BEFORE execution — the exec task never ran"
    );
    // The counterfactual: the SAME file without the flag plans green.
    let planned = run(
        &wf.to_string_lossy(),
        false,
        None,
        plain_theme(),
        RenderMode::Plain,
        true, // --dry-run: plan only, zero effects
        None,
        None, // no --access pin
        &[],
        None,
        true,
        None,
        false,
        None,
        true,
        false, // unsigned-tolerant default
    );
    assert_eq!(planned, exit::OK, "the workflow itself is runnable");
}

/// A `.nika.yaml` naming a local seat, so the swap is VISIBLE: whatever
/// the plan prints, it cannot have come from both models.
#[cfg(test)]
fn ollama_fixture() -> (nika_schema::raw::RawWorkflow, nika_check::CheckReport) {
    let yaml = "nika: hello-ai\nmodel: ollama/llama3.2:3b\npermits: {}\ntasks:\n  greet:\n    infer: { prompt: \"Say hello.\", max_tokens: 64 }\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::source::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    (wf, report)
}

/// #1051 · the preview prices and NAMES the model the run would use.
///
/// Before the fix this asserted `ollama/llama3.2:3b` — the plan card and
/// the `plan_version: 1` object both carried the file's model while the
/// run would have used the flag's. Reproduced against the published
/// 0.111.0 on `nika new snippets/hello-ai`, unedited.
#[test]
fn dry_run_plan_names_the_overridden_model() {
    let (wf, _) = ollama_fixture();
    let (wf, report) = dry_run_swap(&wf, "mock/echo").expect("mock/echo resolves in this binary");
    let p = nika_check::plan::payload("hello-ai.nika.yaml", &wf, &report);
    let model = p["cost"]["tasks"][0]["model"]
        .as_str()
        .expect("the plan prices one task");
    assert_eq!(
        model, "mock/echo",
        "the preview must price the model the RUN will use, not the file's"
    );
}

/// #1051, second half · the preview REFUSES a model this binary cannot
/// resolve, where before it printed a green plan at exit 0 — the only one
/// of `check` / `run` / `run --dry-run` that could not see the flag.
#[test]
fn dry_run_refuses_a_model_that_does_not_resolve() {
    let (wf, _) = ollama_fixture();
    let refused = dry_run_swap(&wf, "nonexistent/model");
    assert!(
        refused.is_err(),
        "a swap to an unresolvable provider must not render a plan"
    );
}

/// The unflagged preview keeps the file's own seat — the swap is a
/// transform the lane applies only when asked, never a normalisation.
#[test]
fn dry_run_without_the_flag_keeps_the_files_model() {
    let (wf, report) = ollama_fixture();
    let p = nika_check::plan::payload("hello-ai.nika.yaml", &wf, &report);
    assert_eq!(p["cost"]["tasks"][0]["model"], "ollama/llama3.2:3b");
}

#[test]
fn dry_run_refuses_an_unknown_access_pin() {
    let wf = stage(
        "dry-run-unknown-access.nika.yaml",
        "nika: dry-run-access\nmodel: mock/echo\npermits: {}\ntasks:\n  ask:\n    infer: { prompt: \"hello\" }\n",
    );
    let code = run(
        &wf.to_string_lossy(),
        false,
        None,
        plain_theme(),
        RenderMode::Plain,
        true,
        None,
        Some("nonsense-seat"),
        &[],
        None,
        true,
        None,
        false,
        None,
        true,
        false,
    );
    assert_eq!(code, exit::ENV, "dry-run must enforce the NIKA-1802 gate");
}

#[cfg(feature = "access-harness")]
#[test]
fn mock_model_with_harness_pin_refuses_before_a_live_seat() {
    let wf = stage(
        "mock-never-harness.nika.yaml",
        "nika: mock-never-harness\nmodel: mock/echo\npermits: {}\ntasks:\n  ask:\n    infer: { prompt: \"hello\" }\n",
    );
    let code = run(
        &wf.to_string_lossy(),
        false,
        None,
        plain_theme(),
        RenderMode::Plain,
        false,
        None,
        Some("harness"),
        &[],
        None,
        true,
        None,
        false,
        None,
        true,
        false,
    );
    assert_eq!(
        code,
        exit::ENV,
        "mock/echo must refuse before any harness backend can be seated"
    );
}
