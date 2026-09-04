// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as check_run_equivalence: this suite's WHOLE JOB is the
// real binary, and NIKA_SPEC_DIR is harness plumbing. println is the
// suite's DELIVERABLE — the tier census is DERIVED at execution, and a
// gate that hides how much of its corpus it skipped is the very defect
// this file exists to make unshippable.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::print_stdout
)]

//! The GENERALIZED check ⇔ run equivalence oracle.
//!
//! `check_run_equivalence.rs` proves the law for ONE domain (permits ·
//! 0.106 · F-O6 · NEP-0007). This file proves it for the corpus, under
//! the governing law of `docs/plans/2026-07-28-verdict-coverage.md`:
//!
//! > A verdict must either COVER its claim, or NARROW its claim to what
//! > it covers.
//!
//! ## The contradiction predicate, and why it is sound
//!
//! The naive predicate — "check was green and the run failed" — is
//! WRONG, and measuring it is what found the right one. A taught
//! workflow run in a hermetic tempdir fails constantly for reasons no
//! static pass could ever own: the CSV it reads is not there, DNS is
//! not there, `docker` is not there, `--var` was not supplied. Calling
//! any of those a contradiction would make this oracle the fifteenth
//! instance of the defect it exists to catch.
//!
//! The runtime already publishes the exact signal, structured, in its
//! own event stream — the `permit_checked` witness frame (NEP-0007):
//!
//! ```text
//! {"kind":"permit_checked","fields":[{"key":"decision","value":"deny"},
//!                                    {"key":"why","value":"absent permits: = zero authority (NEP-0003)"}]}
//! ```
//!
//! Measured on `t2-csv-chart-report` with and without its fixture, the
//! two causes that render IDENTICALLY as `NIKA-SEC-004` separate
//! cleanly at the witness:
//!
//! | run condition | rendered code | `permit_checked` |
//! |---|---|---|
//! | `data/sales.csv` absent | `NIKA-SEC-004` | `allow` — path resolution failed AFTER the grant |
//! | `data/sales.csv` present | (none · task green) | `allow` |
//! | no `permits:` block at all | `NIKA-SEC-004` | **`deny`** — the authority decision itself |
//!
//! But "any deny" is still too broad, and narrowing it is the same law
//! applied to this file. A deny carries a `plane`, and the planes do
//! not all answer the same question:
//!
//! | plane | the question | statically decidable? |
//! |---|---|---|
//! | `tool` | is this tool id inside `permits.tools` at all | **ALWAYS** — a fact about the file, no argument involved |
//! | `fs` · `net` | does this RESOLVED path/host fall in the set | only when the argument is static |
//! | `regate` | did a runtime-tainted arg leaf escape | never — taint is a runtime value |
//! | `exec` | is this program in the allowlist | only when the argv is static |
//!
//! `permits_fit.rs` states the deferral in its own header: *"A
//! path/host built from a `${{ }}` value is dynamic and stays the
//! runtime `NIKA-SEC-004` check."* So an `fs` deny on a dynamic path is
//! `check` correctly NARROWING its claim — the healthy case, not a
//! contradiction. Asserting on it would make this oracle cry wolf at
//! the exact discipline it exists to reward.
//!
//! So the asserted predicate is the always-decidable plane:
//!
//! > **check GREEN + a `permit_checked` frame with `plane: tool` and
//! > `decision: deny` ⇒ CONTRADICTION.**
//!
//! Whether a tool id appears in `permits.tools` needs no argument, no
//! file, no network and no variable. If `check` passed a workflow whose
//! very first dispatch the runtime refuses on authority grounds, the
//! static verdict reported on a domain it did not observe.
//!
//! Denies on the other planes are COUNTED and PRINTED, never asserted
//! and never dropped — the reader gets the triage surface without the
//! oracle overclaiming.
//!
//! ## The tiers, DERIVED not declared
//!
//! Tier is read off the observed run, never off an allowlist (an
//! allowlist is a second claim needing its own proof, and it rots):
//!
//! ```text
//! T1  ran to a clean verdict              full equivalence asserted
//! T2  ran, then failed environmentally    excluded · COUNTED · code printed
//! T3  never reached a terminal            narrower assertion: check must
//!                                         still produce a reachable verdict
//! ```
//!
//! Every tier count is printed on every run. A corpus gate that
//! silently skips most of its corpus is this document's defect wearing
//! one more coat.
//!
//! ## The DECLARED RESIDUAL — what this oracle does NOT cover
//!
//! Stated because the law binds this file too. An equivalence oracle
//! can only see check and run DISAGREEING. It is structurally blind to
//! the class where both agree and both are wrong — which is exactly
//! F11, the fail-open glob (`data/*.csv` admitting
//! `data/sub/deeper/private.key`): check said green, run said green,
//! the key was read. Nothing here would have fired.
//!
//! That class belongs to a DIFFERENTIAL between two independent
//! implementations of one predicate (`nika_cap::glob_admits`, and the
//! generator in `nika-builtin/src/permits.rs` that now emits `*`,
//! `*.csv`, `*/x`, `*/**`). The two instruments are complements, not
//! substitutes, and neither subsumes the other.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// How long one corpus run may take before it is counted as T3. A taught
/// workflow that hangs (a live provider dial, a `nika:wait`) must not
/// wedge the suite — it is simply not runnable here, which is a tier,
/// not a failure.
const RUN_TIMEOUT_SECS: u64 = 45;

fn spec_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NIKA_SPEC_DIR") {
        return PathBuf::from(dir);
    }
    repo_root().parent().expect("engine parent").join("spec")
}

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

/// Spawn, poll, and kill on the deadline — stdio lands in files so a
/// chatty `--json` run can never deadlock on a full pipe. `None` means
/// the deadline killed it.
fn output_with_timeout(mut cmd: Command, cwd: &Path, secs: u64) -> Option<(bool, String)> {
    let out_path = cwd.join(".oracle-stdout");
    let err_path = cwd.join(".oracle-stderr");
    let out_file = std::fs::File::create(&out_path).expect("stdout sink");
    let err_file = std::fs::File::create(&err_path).expect("stderr sink");
    let mut child = cmd
        .stdout(Stdio::from(out_file))
        .stderr(Stdio::from(err_file))
        .spawn()
        .expect("binary spawns");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    let status = loop {
        match child.try_wait().expect("wait") {
            Some(status) => break Some(status),
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    };
    let text = format!(
        "{}\n{}",
        std::fs::read_to_string(&out_path).unwrap_or_default(),
        std::fs::read_to_string(&err_path).unwrap_or_default()
    );
    status.map(|s| (s.success(), text))
}

/// One observed run, reduced to what the equivalence law needs: the
/// permit decisions (the witness), the per-task terminals, the codes,
/// and whether the process reached a verdict at all.
struct Observed {
    /// Denies on the `tool` plane — the ASSERTED signal. Authority is a
    /// fact about the file, so a deny here under a green check is
    /// unambiguous.
    authority_denials: Vec<String>,
    /// Denies on every other plane — REPORTED, never asserted: an `fs`
    /// or `net` deny on a dynamic argument is lawful deferral.
    deferred_denials: Vec<String>,
    statuses: BTreeMap<String, String>,
    codes: BTreeMap<String, String>,
    outputs: BTreeMap<String, String>,
    reached_terminal: bool,
    ok: bool,
    text: String,
}

fn field(ev: &serde_json::Value, key: &str) -> Option<String> {
    ev["fields"]
        .as_array()?
        .iter()
        .find(|f| f["key"] == key)
        .and_then(|f| f["value"].as_str())
        .map(str::to_owned)
}

fn observe(text: &str, timed_out: bool) -> Observed {
    let mut authority_denials = Vec::new();
    let mut deferred_denials = Vec::new();
    let mut statuses = BTreeMap::new();
    let mut codes = BTreeMap::new();
    let mut outputs = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(ev) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if ev["kind"] == "permit_checked" && field(&ev, "decision").as_deref() == Some("deny") {
            let plane = field(&ev, "plane").unwrap_or_default();
            let rendered = format!(
                "plane `{plane}` · task `{}` · gate `{}` · {}",
                field(&ev, "task").unwrap_or_default(),
                field(&ev, "gate").unwrap_or_default(),
                field(&ev, "why").unwrap_or_default()
            );
            // `tool` is the one plane whose question ("is this id in
            // permits.tools") has no runtime input at all.
            if plane == "tool" {
                authority_denials.push(rendered);
            } else {
                deferred_denials.push(rendered);
            }
        }
        let status = match ev["kind"].as_str() {
            Some("task_completed") => "success",
            Some("task_failed") => "failure",
            Some("task_skipped") => "skipped",
            Some("task_cancelled") => "cancelled",
            _ => continue,
        };
        let Some(task) = field(&ev, "task") else {
            continue;
        };
        // A terminal's wire code rides the `outcome` JSON (spec 13 ·
        // `payload.error.code`); a bare `code` is the recover frame's shape.
        if let Some(code) = field(&ev, "outcome")
            .and_then(|o| serde_json::from_str::<serde_json::Value>(&o).ok())
            .and_then(|o| o["payload"]["error"]["code"].as_str().map(str::to_owned))
            .or_else(|| field(&ev, "code"))
        {
            codes.insert(task.clone(), code);
        }
        if let Some(output) = field(&ev, "output") {
            outputs.insert(task.clone(), output);
        }
        statuses.insert(task, status.to_owned());
    }
    Observed {
        authority_denials,
        deferred_denials,
        reached_terminal: !timed_out,
        statuses,
        codes,
        outputs,
        ok: false, // overwritten by the caller that owns the exit status
        text: text.to_owned(),
    }
}

/// Check + run one workflow hermetically: a fresh tempdir per file, the
/// mock model on BOTH sides (`check --model` is documented as the
/// preview of `run --model`, so the two judge the same world), and the
/// run's cwd inside the sandbox — the taught corpus contains
/// `cargo test --workspace --lib` and `rm -rf ./target/tmp`, which must
/// never meet the engine tree.
struct Probe {
    check_ok: bool,
    check_text: String,
    observed: Option<Observed>,
}

fn probe(workflow: &Path, vars: &[(String, String)], envs: &[(String, String)]) -> Probe {
    let sandbox = tempfile::tempdir().expect("tempdir");
    let root = sandbox.path();
    let local = root.join("w.nika.yaml");
    std::fs::copy(workflow, &local).expect("stage workflow");

    let mut check = Command::new(env!("CARGO_BIN_EXE_nika"));
    check
        .arg("check")
        .arg("--model")
        .arg("mock/echo")
        .arg("w.nika.yaml")
        .current_dir(root);
    let (check_ok, check_text) =
        output_with_timeout(check, root, RUN_TIMEOUT_SECS).unwrap_or((false, String::new()));

    let mut run = Command::new(env!("CARGO_BIN_EXE_nika"));
    run.arg("run")
        .arg("--model")
        .arg("mock/echo")
        .arg("--json")
        .arg("w.nika.yaml")
        .current_dir(root);
    for (k, v) in vars {
        run.arg("--var").arg(format!("{k}={v}"));
    }
    // A fixture's `run.json` may declare the env its body probes
    // (`permits/006` passes `NIKA_RT_CANARY` through to a child).
    // Dropping it makes the harness itself report on a domain it did
    // not set up — the defect this file is about.
    for (k, v) in envs {
        run.env(k, v);
    }
    let observed = match output_with_timeout(run, root, RUN_TIMEOUT_SECS) {
        Some((ok, text)) => {
            let mut o = observe(&text, false);
            o.ok = ok;
            Some(o)
        }
        None => Some(observe("", true)),
    };
    Probe {
        check_ok,
        check_text,
        observed,
    }
}

fn fixture_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

fn workflows_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with(".nika.yaml"))
        .collect();
    files.sort();
    files
}

/// One fixture's `expected-run.json` against one observed run — the
/// same comparison law as `gate_matrix_conformance`. Output assertions
/// are SUBSTRING contracts: the frame carries the JSON-encoded value and
/// a provider voice may frame a canary, so the fixture asserts the
/// substance.
fn compare_run_contract(
    name: &str,
    observed: &Observed,
    expected: &serde_json::Value,
    failures: &mut Vec<String>,
) {
    let want_ok = expected["workflow_state"] == "success";
    if observed.ok != want_ok {
        failures.push(format!(
            "{name}: workflow verdict — contract says {} · binary says ok={}\n{}",
            expected["workflow_state"], observed.ok, observed.text
        ));
    }
    for (task, spec) in expected["tasks"].as_object().expect("tasks map") {
        let want = spec["status"].as_str().expect("status");
        match observed.statuses.get(task) {
            Some(got) if got == want => {}
            got => failures.push(format!(
                "{name}: task `{task}` — contract {want} · observed {got:?}"
            )),
        }
        if let Some(code) = spec["error_code"].as_str() {
            match observed.codes.get(task) {
                Some(got) if got == code => {}
                got => failures.push(format!(
                    "{name}: task `{task}` — contract code {code} · observed {got:?}"
                )),
            }
        }
        if let Some(output) = spec["output"].as_str() {
            match observed.outputs.get(task) {
                Some(got) if got.contains(output) => {}
                got => failures.push(format!(
                    "{name}: task `{task}` — contract output {output:?} · observed {got:?}"
                )),
            }
        }
    }
}

// ---------------------------------------------------------------------
// LEG 1 · the equivalence law over EVERY runtime tier
// ---------------------------------------------------------------------

/// `check_run_equivalence.rs` applies the DEFER law to
/// `runtime/permits` (7 fixtures). Every other runtime tier carries the
/// same `input.nika.yaml` + `expected-run.json` contract and is never
/// fed through it: `runtime/errors`, `runtime/for-each` and
/// `runtime/agent` are referenced by NO engine test at all, and
/// `runtime/gates` has its RUN half checked by
/// `gate_matrix_conformance.rs` while its CHECK half is unasserted.
/// Specialized contracts without `workflow_state` + `tasks` (for example
/// `runtime/access-harness`) belong to their own exogenous runner and are
/// deliberately outside this binary-equivalence leg.
///
/// This walks the tiers generically — a new tier is covered the day it
/// is added, with no edit here.
#[test]
fn every_runtime_tier_honors_the_equivalence_law() {
    let root = spec_dir().join("conformance/tests/runtime");
    assert!(
        root.is_dir(),
        "runtime conformance tier missing: {} — set NIKA_SPEC_DIR",
        root.display()
    );

    let mut census: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut preempted = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for tier in fixture_dirs(&root) {
        let tier_name = tier
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        for dir in fixture_dirs(&tier) {
            let name = format!(
                "{tier_name}/{}",
                dir.file_name().unwrap_or_default().to_string_lossy()
            );
            let input = dir.join("input.nika.yaml");
            let contract = dir.join("expected-run.json");
            if !input.is_file() || !contract.is_file() {
                continue; // a tier without a run contract (runtime/trace) owes nothing here
            }
            let expected: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&contract).expect("contract"))
                    .expect("contract parses");
            if expected["workflow_state"].as_str().is_none()
                || expected["tasks"].as_object().is_none()
            {
                continue;
            }
            let entry = census.entry(tier_name.clone()).or_insert((0, 0));
            entry.0 += 1;

            let run_spec: serde_json::Value = if dir.join("run.json").is_file() {
                serde_json::from_str(
                    &std::fs::read_to_string(dir.join("run.json")).expect("run.json"),
                )
                .expect("run spec parses")
            } else {
                serde_json::json!({})
            };
            let kv = |key: &str| -> Vec<(String, String)> {
                run_spec[key]
                    .as_object()
                    .map(|m| {
                        m.iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            // `inputs` and `vars` both appear across the tiers; `env` is
            // the one permits/006 needs.
            let mut vars = kv("inputs");
            vars.extend(kv("vars"));

            let probe = probe(&input, &vars, &kv("env"));

            // The DEFER law: a runtime fixture is check-CLEAN — the
            // static judge saw nothing refusable, so the run twin owns
            // the verdict. The one lawful exception (the recovered
            // conjunct): a statically-decidable refusal pre-empts the
            // defer — it must NAME its static law, never a crash.
            if !probe.check_ok {
                if probe.check_text.contains("NIKA-") {
                    preempted += 1;
                } else {
                    failures.push(format!(
                        "{name}: a static pre-emption must NAME its law (a NIKA- finding):\n{}",
                        probe.check_text
                    ));
                }
                continue;
            }
            let Some(observed) = probe.observed else {
                continue;
            };
            if !observed.reached_terminal {
                failures.push(format!("{name}: the run never reached a verdict (timeout)"));
                continue;
            }

            compare_run_contract(&name, &observed, &expected, &mut failures);
            entry.1 += 1;
        }
    }

    let total: usize = census.values().map(|(n, _)| n).sum();
    assert!(
        failures.is_empty(),
        "the judged verdict must BE the executed verdict, on every tier:\n{}",
        failures.join("\n")
    );
    assert!(
        census.len() >= 4,
        "the runtime corpus carries several tiers (saw {}) — layout drift?",
        census.len()
    );
    assert!(total >= 45, "the runtime run-contract corpus (saw {total})");
    let rendered: Vec<String> = census
        .iter()
        .map(|(tier, (n, agreed))| format!("{tier} {agreed}/{n}"))
        .collect();
    println!(
        "equivalence over every runtime tier: {total} fixtures · {} · {preempted} statically pre-empted",
        rendered.join(" · ")
    );
}

// ---------------------------------------------------------------------
// LEG 2 · the taught corpus never contradicts its own check verdict
// ---------------------------------------------------------------------

/// The corpus authors COPY — spec examples, the showcase, the
/// templates, and the engine's own workflows. F8's closing line is the
/// reason this leg exists: *"the two shipped examples sitting in the
/// quiet half of the same hole mean we are teaching the shape."*
///
/// Every file is checked and run hermetically. The assertion is the
/// witness predicate (a `deny` under a green check); everything else is
/// TIERED and PRINTED, never silently skipped.
#[test]
fn the_taught_corpus_never_contradicts_its_check_verdict() {
    let spec = spec_dir();
    let mut corpus: Vec<(String, PathBuf)> = Vec::new();
    for (label, dir) in [
        ("examples", spec.join("examples")),
        ("showcase", spec.join("examples/showcase")),
        ("templates", spec.join("templates")),
        ("engine", repo_root().join("workflows")),
        ("engine", repo_root().join("workflows/catalog")),
    ] {
        for wf in workflows_in(&dir) {
            let name = wf
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            corpus.push((format!("{label}/{name}"), wf));
        }
    }
    assert!(
        corpus.len() >= 40,
        "the taught corpus (saw {}) — layout drift, or a root moved",
        corpus.len()
    );

    let (mut t1, mut t3) = (0usize, 0usize);
    let mut t2: BTreeMap<String, usize> = BTreeMap::new();
    let mut check_red: Vec<String> = Vec::new();
    let mut deferred: Vec<String> = Vec::new();
    let mut contradictions: Vec<String> = Vec::new();

    for (name, wf) in &corpus {
        let probe = probe(wf, &[], &[]);
        let Some(observed) = probe.observed else {
            continue;
        };

        if !probe.check_ok {
            // The reverse direction — check REFUSES. The mapping law
            // still binds: a statically-refused file must not then run
            // clean, or the refusal was friction with nothing behind it.
            check_red.push(name.clone());
            if observed.ok {
                contradictions.push(format!(
                    "{name}: check REFUSED (rc≠0) but the run SUCCEEDED — the refusal is unbacked"
                ));
            }
            continue;
        }

        // Lawful deferral — recorded so it is visible, never asserted.
        for d in &observed.deferred_denials {
            deferred.push(format!("{name}: {d}"));
        }

        // The asserted predicate: authority is a fact about the FILE.
        if !observed.authority_denials.is_empty() {
            contradictions.push(format!(
                "{name}: check GREEN, runtime DENIED the AUTHORITY —\n    {}",
                observed.authority_denials.join("\n    ")
            ));
            continue;
        }

        if !observed.reached_terminal {
            t3 += 1;
        } else if observed.ok {
            t1 += 1;
        } else {
            let code = observed
                .codes
                .values()
                .next()
                .cloned()
                .unwrap_or_else(|| "(refused at admission)".into());
            *t2.entry(code).or_insert(0) += 1;
        }
    }

    let t2_total: usize = t2.values().sum();
    let excluded: Vec<String> = t2.iter().map(|(c, n)| format!("{c}×{n}")).collect();
    // The census prints BEFORE the assertion: a run that fails must
    // still tell the reader how much of the corpus it actually covered.
    println!(
        "taught corpus · {} files · T1 ran clean {t1} · T2 environmental {t2_total} \
         [{}] · T3 no verdict {t3} · check-red {} [{}] · lawful deferrals {}",
        corpus.len(),
        excluded.join(" "),
        check_red.len(),
        check_red.join(" "),
        deferred.len()
    );
    for d in &deferred {
        println!("    deferred (reported · not asserted) · {d}");
    }
    assert!(
        contradictions.is_empty(),
        "a check verdict must not be contradicted by the run:\n\n{}",
        contradictions.join("\n\n")
    );
    assert!(
        t1 + t2_total + check_red.len() >= 30,
        "too little of the corpus reached a verdict (T1 {t1} + T2 {t2_total} + static \
         refusals {}) — the oracle would be claiming coverage it does not have",
        check_red.len()
    );
}

// ---------------------------------------------------------------------
// LEG 3 · the negative control — the oracle observed FAILING
// ---------------------------------------------------------------------

/// An oracle never observed failing is the vacuous proptest this
/// session already found once (200 cases that all died at a schema gate
/// and passed by comparing two identical errors). So the detector is
/// driven against known-denied streams and asserted to fire.
///
/// This arm is at the PARSER, deliberately: it pins the predicate
/// itself — plane discrimination included — independently of whether
/// any given engine version still has a bug that produces the frame. It
/// can never go vacuous, because it carries its own input.
#[test]
fn the_detector_fires_on_an_authority_deny_and_only_on_that() {
    let allow_only = r#"{"kind":"permit_checked","fields":[{"key":"task","value":"pin"},{"key":"plane","value":"tool"},{"key":"gate","value":"nika:read"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}]}"#;
    let authority_deny = r#"{"kind":"permit_checked","fields":[{"key":"task","value":"upload"},{"key":"plane","value":"tool"},{"key":"gate","value":"nika:fetch"},{"key":"decision","value":"deny"},{"key":"why","value":"absent permits: = zero authority (NEP-0003)"}]}"#;
    // A dynamic-path fs deny: the runtime deciding what `check` LAWFULLY
    // deferred. The oracle must NOT call this a contradiction.
    let deferred_deny = r#"{"kind":"permit_checked","fields":[{"key":"task","value":"grab"},{"key":"plane","value":"fs"},{"key":"gate","value":"data/**"},{"key":"decision","value":"deny"},{"key":"why","value":"fs.path_mismatch · resolves to `/etc/passwd` · outside the declared read set (NEP-0009)"}]}"#;

    let silent = observe(allow_only, false);
    assert!(
        silent.authority_denials.is_empty() && silent.deferred_denials.is_empty(),
        "an allow-only stream must not trip anything — the oracle would cry wolf on every clean run"
    );

    let fired = observe(&format!("{allow_only}\n{authority_deny}"), false);
    assert_eq!(
        fired.authority_denials.len(),
        1,
        "the authority deny is the ASSERTED signal and it must be seen: {:?}",
        fired.authority_denials
    );

    let lawful = observe(&format!("{allow_only}\n{deferred_deny}"), false);
    assert!(
        lawful.authority_denials.is_empty(),
        "a dynamic-path fs deny is `check` narrowing its claim correctly — asserting on it \
         would punish the discipline this oracle exists to reward"
    );
    assert_eq!(
        lawful.deferred_denials.len(),
        1,
        "…but it must still be REPORTED, never dropped"
    );
    println!("detector control: silent on allow · fires on plane=tool deny · defers plane=fs deny");
}

/// The same control END TO END, at the real binary, on F8's shape: one
/// `tasks:` block, byte-for-byte identical across arms, and only the
/// `permits:` block edited. This is the arm that proves the predicate
/// survives contact with the actual event stream.
///
/// Each arm carries the outcome it must produce, so the suite is
/// non-vacuous in BOTH directions: arms that must stay silent (no false
/// positive on an honest declaration, and none on a file the checker
/// already refuses) — and the FIRING witness, which since F13 lives
/// only in the detector's own synthesized-journal test above.
///
/// The original firing arm is RETIRED BY LAW, not flipped: it was the
/// shipped `templates/api-upload-and-create.nika.yaml` shape —
///
/// ```text
///   url: "https://api.example.com/upload"      → check REFUSES (rc=2)
///   url: "${{ const.api_base }}/upload"        → check GREEN · run DENIES
/// ```
///
/// Same resolved value, opposite verdicts: deferring the ARGUMENT
/// silently dropped the tool-authority question, which never needed the
/// argument (the F11 lesson). F13 landed — the decidable conjunct is
/// recovered, so `check GREEN · tool-plane DENY` is unproducible by any
/// workflow (`tool` is the one plane with no runtime input at all: a
/// static judge that answers it always can). The author's own
/// instruction governs the retirement: « retarget the arm at whatever
/// inversion still exists, or delete it if none does » — and a tool-
/// plane inversion is exactly what the fix closed. Every net/fs deny
/// stays a LAWFUL defer the detector already knows to leave alone.
/// An arm kept green by flipping its expectation to `false` is how a
/// control goes vacuous; this one went out telling the truth.
#[test]
fn the_oracle_is_observed_both_firing_and_staying_silent() {
    let sandbox = tempfile::tempdir().expect("tempdir");
    let root = sandbox.path();
    std::fs::create_dir_all(root.join("data")).expect("data dir");
    std::fs::write(root.join("data/pin.txt"), "0.106.0").expect("seed the file it reads");

    let read_task = "tasks:\n  pin:\n    invoke:\n      tool: \"nika:read\"\n      args: { path: \"data/pin.txt\" }\n";
    // The two fetch bodies differ ONLY in whether the url is written
    // The literal-url dispatch — check decides it, so the inversion
    // never opens (the retired const-url twin's story is the doc above).
    let fetch_literal = "tasks:\n  upload:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/upload\" }\n";

    // (arm · permits block · body · must the predicate FIRE? · expected
    // check verdict — ASSERTED, no longer narrated: the comments used to
    // carry "check refuses it now" with nothing pinning it, and a check
    // that quietly flipped green would have hollowed the arm's story)
    let arms: &[(&str, &str, &str, bool, bool)] = &[
        // the honest declaration — green at check, silent at run
        (
            "honest",
            "permits:\n  fs: { read: [\"data/**\"] }\n  tools: [\"nika:read\"]\n",
            read_task,
            false,
            true,
        ),
        // F8 · the body still reads, the fs grant is deleted
        (
            "grant-deleted",
            "permits:\n  tools: [\"nika:read\"]\n",
            read_task,
            false, // check refuses it now — the F8 repair
            false,
        ),
        // the tool id itself is outside the declared set
        (
            "tool-not-granted",
            "permits:\n  fs: { read: [\"data/**\"] }\n  tools: [\"nika:jq\"]\n",
            read_task,
            false, // check refuses it now
            false,
        ),
        // the same dispatch with a LITERAL url — check decides it, so
        // the inversion never opens. The paired baseline.
        ("fetch-literal-url", "", fetch_literal, false, false),
    ];

    let mut verdicts: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for (name, permits, body, must_fire, expect_check_ok) in arms {
        let wf = root.join(format!("{name}.nika.yaml"));
        std::fs::write(&wf, format!("nika: oracle-control-{name}\n{permits}{body}"))
            .expect("write arm");

        let probe = probe(&wf, &[], &[]);
        let observed = probe.observed.expect("a verdict");
        let denied = !observed.authority_denials.is_empty();

        // The arm's check verdict is an ASSERTION, not a narration:
        // green arms must be green; refused arms must name their law
        // (the leg-1 idiom — a refusal without a NIKA- finding is a
        // crash, not a verdict).
        if probe.check_ok != *expect_check_ok {
            failures.push(format!(
                "control arm `{name}`: check verdict — expected {} · got {}:\n{}",
                if *expect_check_ok { "green" } else { "red" },
                if probe.check_ok { "green" } else { "red" },
                probe.check_text
            ));
        }
        if !*expect_check_ok && !probe.check_text.contains("NIKA-") {
            failures.push(format!(
                "control arm `{name}`: a refusal must NAME its law (a NIKA- finding):\n{}",
                probe.check_text
            ));
        }

        // The law, on every arm: green at check AND denied on authority
        // at run is the inversion, and nothing makes that pair lawful.
        let inverted = probe.check_ok && denied;
        if inverted != *must_fire {
            failures.push(if *must_fire {
                format!(
                    "control arm `{name}` no longer reconstructs the inversion \
                     (check {} · authority-deny {denied}). If F13 landed, this is the SUCCESS \
                     signal — retarget the arm at a live inversion, or delete it. Do not simply \
                     flip the expectation to false: that is how a control goes vacuous.",
                    if probe.check_ok { "green" } else { "red" }
                )
            } else {
                format!(
                    "control arm `{name}`: check GREEN, runtime DENIED the authority —\n    {}",
                    observed.authority_denials.join("\n    ")
                )
            });
        }
        // The honest arm additionally pins the no-false-positive side:
        // a truthful declaration must sail through both judges.
        if *name == "honest" && (!probe.check_ok || denied) {
            failures.push(format!(
                "control arm `honest`: a truthful declaration must pass BOTH judges \
                 (check_ok={} · authority_denied={denied}) — the oracle is punishing honesty:\n{}",
                probe.check_ok, probe.check_text
            ));
        }
        verdicts.push(format!(
            "{name} → check {} · predicate {}",
            if probe.check_ok { "green" } else { "red" },
            if denied { "FIRES" } else { "silent" }
        ));
    }

    println!("end-to-end control: {}", verdicts.join(" · "));
    assert!(
        failures.is_empty(),
        "the oracle must be observed both firing and staying silent:\n{}",
        failures.join("\n")
    );
}
