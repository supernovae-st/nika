// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as gate_matrix_conformance: this suite's WHOLE JOB is
// the real binary, and NIKA_SPEC_DIR is harness plumbing. println is
// the suite's DELIVERABLE — the DERIVED coverage lines (families × legs
// counted at execution) that the doc law forbids hand-writing.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::print_stdout
)]

//! The check ⇔ run equivalence oracle (spec 17 · NEP-0007 · invariant 4:
//! judged = executed = attested) — the conformance corpus replayed
//! through BOTH evaluators AT THE REAL BINARY, under the mapping law:
//!
//! * **static-red ⇒ never executes** — every `core/authority` fixture
//!   the oracle judges invalid is fed to `nika run`: the run refuses
//!   BEFORE the prologue (zero task frames · the judged verdict is the
//!   executed verdict) and the refusal voices the fixture's code.
//! * **static-clean + DEFER ⇒ the run twin decides** — every
//!   `runtime/permits` fixture must be check-CLEAN, and its real run's
//!   per-task terminals must match `expected-run.json` (the same
//!   comparison law as `gate_matrix_conformance`).
//! * **executed ⇒ attested** — a run that dispatched an effect carries
//!   `permit_checked` frames in its own event stream (NEP-0007 · the
//!   witness), tying the third leg at the shipped surface.
//! * **`e_diff` runtime legs (fs · net)** — the named follow-on from
//!   `nika-check/tests/e_diff_soundness.rs`, landed: a small exhaustive
//!   lattice drives the REAL builtin sinks (`nika:read` under a tempdir
//!   root · `nika:fetch` on its deny/floor sides — the allow-side
//!   network dial is the declared residual: no test dials out).
//!
//! Coverage is DERIVED: the suite prints what it replayed — families ×
//! legs counted at execution, never hand-written.

use std::path::{Path, PathBuf};
use std::process::Command;

fn spec_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("NIKA_SPEC_DIR") {
        return PathBuf::from(dir);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .parent()
        .expect("engine parent")
        .join("spec")
}

/// One observed run at the real binary: per-task terminal status,
/// per-task failure code, the raw event kinds, and the process verdict.
struct Observed {
    statuses: std::collections::BTreeMap<String, String>,
    codes: std::collections::BTreeMap<String, String>,
    outputs: std::collections::BTreeMap<String, String>,
    kinds: Vec<String>,
    effectful: bool,
    ok: bool,
    text: String,
}

fn run_observed(workflow: &Path, envs: &[(String, String)], vars: &[(String, String)]) -> Observed {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika-cli"));
    cmd.arg("run").arg(workflow).arg("--json");
    for (k, v) in vars {
        cmd.arg("--var").arg(format!("{k}={v}"));
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let mut statuses = std::collections::BTreeMap::new();
    let mut codes = std::collections::BTreeMap::new();
    let mut outputs = std::collections::BTreeMap::new();
    let mut kinds = Vec::new();
    let mut effectful = false;
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(ev) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(kind) = ev["kind"].as_str() {
            kinds.push(kind.to_owned());
            // The effect predicate (same as the verify rule): a started
            // task whose note names a dispatch-boundary verb.
            if kind == "task_started"
                && let Some(fields) = ev["fields"].as_array()
                && let Some(note) = fields
                    .iter()
                    .find(|f| f["key"] == "note")
                    .and_then(|f| f["value"].as_str())
                && ["exec ·", "invoke ·", "agent ·"]
                    .iter()
                    .any(|p| note.starts_with(p))
            {
                effectful = true;
            }
        }
        let status = match ev["kind"].as_str() {
            Some("task_completed") => "success",
            Some("task_failed") => "failure",
            Some("task_skipped") => "skipped",
            Some("task_cancelled") => "cancelled",
            _ => continue,
        };
        let Some(fields) = ev["fields"].as_array() else {
            continue;
        };
        let field = |key: &str| {
            fields
                .iter()
                .find(|f| f["key"] == key)
                .and_then(|f| f["value"].as_str())
                .map(str::to_owned)
        };
        if let Some(task) = field("task") {
            // A terminal's wire code rides the `outcome` JSON (spec 13 ·
            // `payload.error.code`) — a bare `code` field is the recover
            // frame's shape, kept as the fallback.
            let code = field("outcome")
                .and_then(|o| serde_json::from_str::<serde_json::Value>(&o).ok())
                .and_then(|o| o["payload"]["error"]["code"].as_str().map(str::to_owned))
                .or_else(|| field("code"));
            if let Some(code) = code {
                codes.insert(task.clone(), code);
            }
            if let Some(output) = field("output") {
                outputs.insert(task.clone(), output);
            }
            statuses.insert(task, status.to_owned());
        }
    }
    Observed {
        statuses,
        codes,
        outputs,
        kinds,
        effectful,
        ok: out.status.success(),
        text: format!("{stdout}\n{stderr}"),
    }
}

fn fixture_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .expect("readable fixture root")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Mapping-law row 1 — static-red ⇒ never executes. Every invalid
/// `core/authority` fixture runs at the real binary: the refusal lands
/// BEFORE the prologue (zero task frames) and voices the judged code.
#[test]
fn statically_refused_fixtures_never_execute() {
    let root = spec_dir().join("conformance/tests/core/authority");
    assert!(
        root.is_dir(),
        "authority tier missing: {} — set NIKA_SPEC_DIR",
        root.display()
    );
    let mut red = 0usize;
    let mut clean = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for dir in fixture_dirs(&root) {
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let input = dir.join("input.yaml");
        if !input.is_file() {
            continue;
        }
        let expected: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("expected.json")).expect("expected.json"),
        )
        .expect("expected parses");
        if expected["valid"].as_bool() == Some(true) {
            clean += 1;
            continue; // clean core fixtures may declare real effects — never run here
        }
        red += 1;
        let observed = run_observed(&input, &[], &[]);
        if observed.ok {
            failures.push(format!("{name}: judged red but the run SUCCEEDED"));
        }
        if observed.kinds.iter().any(|k| k.starts_with("task_")) {
            failures.push(format!(
                "{name}: judged red but tasks ran — the refusal must land pre-prologue ({:?})",
                observed.kinds
            ));
        }
        for err in expected["errors"].as_array().into_iter().flatten() {
            let code = err["code"].as_str().unwrap_or_default();
            if !code.is_empty() && !observed.text.contains(code) {
                failures.push(format!("{name}: the refusal never voices {code}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "the judged verdict must BE the executed verdict:\n{}",
        failures.join("\n")
    );
    assert!(
        red >= 10,
        "the authority corpus carries red fixtures (saw {red})"
    );
    println!(
        "equivalence leg 1 (static-red never executes): {red} red replayed · {clean} clean checked-only"
    );
}

/// Mapping-law row 1, second tier — the POLICY corpus replays at the
/// binary under the same contract: every statically-refused fixture
/// refuses BEFORE the prologue (zero task frames) and voices its judged
/// code. The tier's clean consent pattern (`002-affirmative-clean` ·
/// NEP-0020) does the opposite walk: the run is NOT refused, the
/// non-interactive confirm settles on its `default: false` (a refusal,
/// per the spec's substitution), the affirmative `when:` closes, and the
/// irreversible exec NEVER starts — the runtime half of NIKA-SEC-014,
/// proven on the one fixture whose whole point is that the gate works.
#[test]
fn consent_fixtures_replay_at_the_binary() {
    // The `policy` tier split when the block died (2026-08-12): what it
    // held that was unconditional moved to `consent` and to `order`.
    // The consent half is this test's subject.
    let root = spec_dir().join("conformance/tests/core/consent");
    assert!(
        root.is_dir(),
        "consent tier missing: {} — set NIKA_SPEC_DIR",
        root.display()
    );
    let mut red = 0usize;
    let mut clean_skipped = 0usize;
    let mut consent_clean = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for dir in fixture_dirs(&root) {
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let input = dir.join("input.yaml");
        if !input.is_file() {
            continue;
        }
        let expected: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("expected.json")).expect("expected.json"),
        )
        .expect("expected parses");
        if expected["valid"].as_bool() == Some(true) {
            if name.starts_with("002") {
                consent_clean += 1;
                let observed = run_observed(&input, &[], &[]);
                if !observed.ok {
                    failures.push(format!(
                        "{name}: the clean consent pattern must RUN (the refusal is a decision, never a failure)\n{}",
                        observed.text
                    ));
                }
                // ask settles success (the default false IS the refusal);
                // push settles skipped and so never started — the gate
                // consumed the answer and the exec fired zero times.
                if observed.statuses.get("ask").map(String::as_str) != Some("success") {
                    failures.push(format!(
                        "{name}: the gate must settle success on its default (observed {:?})",
                        observed.statuses.get("ask")
                    ));
                }
                if observed.statuses.get("push").map(String::as_str) != Some("skipped") {
                    failures.push(format!(
                        "{name}: the affirmative gate must close on the refusal (observed {:?})",
                        observed.statuses.get("push")
                    ));
                }
            } else {
                clean_skipped += 1; // clean core fixtures may declare real effects — never run here
            }
            continue;
        }
        red += 1;
        let observed = run_observed(&input, &[], &[]);
        if observed.ok {
            failures.push(format!("{name}: judged red but the run SUCCEEDED"));
        }
        if observed.kinds.iter().any(|k| k.starts_with("task_")) {
            failures.push(format!(
                "{name}: judged red but tasks ran — the refusal must land pre-prologue ({:?})",
                observed.kinds
            ));
        }
        for err in expected["errors"].as_array().into_iter().flatten() {
            let code = err["code"].as_str().unwrap_or_default();
            if !code.is_empty() && !observed.text.contains(code) {
                failures.push(format!("{name}: the refusal never voices {code}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "the consent tier must replay under the same mapping law:\n{}",
        failures.join("\n")
    );
    assert!(red >= 2, "the consent corpus carries its reds (saw {red})");
    // The CI judges at SPEC_PIN, which may predate the clean fixture:
    // the clean-run proof is mandatory WHEN the pin carries the fixture,
    // and silently absent would mean silently unproven — so the count is
    // pinned at most one, and the printout names which mode ran.
    assert!(
        consent_clean <= 1,
        "at most one clean consent fixture (saw {consent_clean})"
    );
    println!(
        "equivalence leg 1b (consent tier): {red} red replayed · {clean_skipped} clean checked-only · {}",
        if consent_clean == 1 {
            "012 ran its refusal to a closed gate"
        } else {
            "012 absent at this pin (the clean-run proof rides the next pin bump)"
        }
    );
}

/// One fixture's `expected-run.json` against one observed run: the
/// workflow verdict, per-task terminal status, wire code, and output.
/// Output assertions are SUBSTRING contracts: the frame carries the
/// JSON-encoded value and a provider voice may frame a canary (mock
/// prefixes) — the fixture asserts the substance.
fn compare_run_contract(
    name: &str,
    observed: &Observed,
    expected: &serde_json::Value,
    failures: &mut Vec<String>,
) {
    let want_ok = expected["workflow_state"] == "success";
    if observed.ok != want_ok {
        failures.push(format!(
            "{name}: workflow verdict — expected {} · binary rc says {}\n{}",
            expected["workflow_state"], observed.ok, observed.text
        ));
    }
    for (task, spec) in expected["tasks"].as_object().expect("tasks map") {
        let want = spec["status"].as_str().expect("status");
        match observed.statuses.get(task) {
            Some(got) if got == want => {}
            got => failures.push(format!(
                "{name}: task `{task}` — expected {want} · observed {got:?}"
            )),
        }
        if let Some(code) = spec["error_code"].as_str() {
            match observed.codes.get(task) {
                Some(got) if got == code => {}
                got => failures.push(format!(
                    "{name}: task `{task}` — expected code {code} · observed {got:?}"
                )),
            }
        }
        if let Some(output) = spec["output"].as_str() {
            match observed.outputs.get(task) {
                Some(got) if got.contains(output) => {}
                got => failures.push(format!(
                    "{name}: task `{task}` — expected output containing {output:?} · observed {got:?}"
                )),
            }
        }
    }
}

/// Mapping-law rows 2+3 — check-clean + DEFER ⇒ the run twin decides,
/// and an executed effect is attested. Every `runtime/permits` fixture
/// must be check-CLEAN — unless a statically-decidable conjunct
/// lawfully pre-empts the defer (the refusal must then NAME its static
/// law, and the pre-emption is counted and told); its real run's
/// terminals must match `expected-run.json`; a run that dispatched an
/// effect must carry `permit_checked` frames (NEP-0007 · the witness).
#[test]
fn deferred_fixtures_match_their_run_contract_and_are_witnessed() {
    let root = spec_dir().join("conformance/tests/runtime/permits");
    assert!(
        root.is_dir(),
        "runtime permits tier missing: {} — set NIKA_SPEC_DIR",
        root.display()
    );
    let mut total = 0usize;
    let mut witnessed = 0usize;
    let mut preempted = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for dir in fixture_dirs(&root) {
        let name = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let input = dir.join("input.nika.yaml");
        if !input.is_file() {
            continue;
        }
        total += 1;

        // check-CLEAN is the DEFER law — the static judge saw nothing
        // refusable, the run twin owns the verdict. The one lawful
        // exception: a statically-DECIDABLE conjunct pre-empts the
        // defer (the recovered-conjunct law · e.g. an absent `permits:`
        // block is zero authority on the tools axis — the dynamic
        // resolved-host argument cannot mask it), so the refusal is
        // asserted to be a NAMED static finding (never a crash, never
        // an empty mouth) and the pre-emption is counted and told.
        let check = Command::new(env!("CARGO_BIN_EXE_nika-cli"))
            .arg("check")
            .arg(&input)
            .output()
            .expect("binary checks");
        if !check.status.success() {
            let out = String::from_utf8_lossy(&check.stdout);
            if out.contains("NIKA-") {
                preempted += 1;
            } else {
                failures.push(format!(
                    "{name}: a static pre-emption must NAME its law (a NIKA- finding): {out}"
                ));
            }
            continue;
        }

        let run_spec: serde_json::Value = if dir.join("run.json").is_file() {
            serde_json::from_str(&std::fs::read_to_string(dir.join("run.json")).expect("run.json"))
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
        let observed = run_observed(&input, &kv("env"), &kv("inputs"));

        let expected: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join("expected-run.json")).expect("expected-run.json"),
        )
        .expect("expected parses");
        compare_run_contract(&name, &observed, &expected, &mut failures);
        // executed ⇒ attested: an effectful dispatch carries its witness
        // (pure-compute infer runs owe none — NEP-0007 scopes the
        // witness to the dispatch boundary).
        if observed.effectful {
            if observed.kinds.iter().any(|k| k == "permit_checked") {
                witnessed += 1;
            } else {
                failures.push(format!(
                    "{name}: effects ran with ZERO permit_checked frames — the witness is REQUIRED (NEP-0007)"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "the run twin must honor the contract:\n{}",
        failures.join("\n")
    );
    assert!(total >= 7, "the permits runtime corpus (saw {total})");
    println!(
        "equivalence legs 2+3 (defer contract + witness): {total} fixtures · {witnessed} witnessed · {preempted} statically pre-empted"
    );
}

/// The `e_diff` runtime FS leg — the named follow-on from
/// `e_diff_soundness.rs`, landed at the real binary: a small exhaustive
/// lattice of `permits.fs` shapes × read paths drives the REAL
/// `nika:read` sink under a tempdir root. Each row names its law; the
/// binary's verdict must match it.
#[test]
fn e_diff_runtime_fs_leg() {
    let root = std::env::temp_dir().join(format!("nika-ediff-fs-{}", std::process::id()));
    let data = root.join("data");
    std::fs::create_dir_all(&data).expect("tempdir");
    std::fs::write(data.join("in.txt"), "payload").expect("seed file");
    std::fs::write(root.join("outside.txt"), "secret").expect("seed outside");

    // (name · permits block · path · expect_success · the law)
    let rows: &[(&str, &str, &str, bool)] = &[
        (
            "absent-block-refused",
            "",
            "data/in.txt",
            false, // F-O8 · no permits block = zero authority
        ),
        (
            "declared-empty-refused",
            "permits: {}\n",
            "data/in.txt",
            false, // NEP-0003 · empty declared = pure compute
        ),
        (
            "covering-glob-allows",
            "permits:\n  fs: { read: [\"data/**\"] }\n  tools: [\"nika:read\"]\n",
            "data/in.txt",
            true, // inside the declared boundary
        ),
        (
            "non-covering-glob-refused",
            "permits:\n  fs: { read: [\"data/**\"] }\n  tools: [\"nika:read\"]\n",
            "outside.txt",
            false, // outside the declared boundary
        ),
        (
            "traversal-escape-refused",
            "permits:\n  fs: { read: [\"data/**\"] }\n  tools: [\"nika:read\"]\n",
            "data/../outside.txt",
            false, // canonicalize-then-confine · the resolved path decides
        ),
    ];
    let mut failures: Vec<String> = Vec::new();
    for (name, permits, path, want_ok) in rows {
        let yaml = format!(
            "nika: ediff-fs\n{permits}tasks:\n  grab:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"{path}\" }}\n"
        );
        let wf = root.join(format!("{name}.nika.yaml"));
        std::fs::write(&wf, yaml).expect("write workflow");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika-cli"));
        cmd.arg("run").arg(&wf).arg("--json").current_dir(&root);
        let out = cmd.output().expect("binary runs");
        let ok = out.status.success();
        if ok != *want_ok {
            failures.push(format!(
                "fs leg `{name}`: expected success={want_ok} · observed {ok}\n{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        failures.is_empty(),
        "the REAL fs sink must decide as the boundary law says:\n{}",
        failures.join("\n")
    );
    println!(
        "e_diff runtime fs leg: {} rows driven at the real binary",
        rows.len()
    );
}

/// The `e_diff` runtime NET leg — deny and floor sides only (the
/// allow-side network dial is the DECLARED residual: no test dials
/// out). A host outside `permits.net.http` refuses at the boundary; a
/// loopback host UNDER a permit still refuses (the SSRF floor stays on
/// top of the declared boundary — floors never widen).
#[test]
fn e_diff_runtime_net_leg() {
    let root = std::env::temp_dir().join(format!("nika-ediff-net-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("tempdir");
    // (name · permits block · url · the law)
    let rows: &[(&str, &str, &str)] = &[
        (
            "absent-block-refused",
            "",
            "https://example.com/data.csv", // F-O8 · zero authority
        ),
        (
            "host-not-covered-refused",
            "permits:\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\"]\n",
            "https://evil.example.net/data.csv", // outside the boundary
        ),
        (
            "loopback-under-permit-floor-refused",
            "permits:\n  net: { http: [\"127.0.0.1\"] }\n  tools: [\"nika:fetch\"]\n",
            "http://127.0.0.1:9/data.csv", // the SSRF floor stays on top
        ),
    ];
    let mut failures: Vec<String> = Vec::new();
    let mut observed_codes: Vec<String> = Vec::new();
    for (name, permits, url) in rows {
        let yaml = format!(
            "nika: ediff-net\n{permits}tasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"{url}\" }}\n"
        );
        let wf = root.join(format!("{name}.nika.yaml"));
        std::fs::write(&wf, yaml).expect("write workflow");
        let observed = run_observed(&wf, &[], &[]);
        if observed.ok {
            failures.push(format!(
                "net leg `{name}`: the refusal law did not hold — the fetch SUCCEEDED\n{}",
                observed.text
            ));
        }
        observed_codes.push(format!(
            "{name} → {}",
            observed
                .codes
                .get("grab")
                .cloned()
                .unwrap_or_else(|| "(pre-prologue)".into())
        ));
    }
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        failures.is_empty(),
        "the REAL net sink must refuse as the boundary law says:\n{}",
        failures.join("\n")
    );
    println!(
        "e_diff runtime net leg: {} deny/floor rows · codes: {}",
        rows.len(),
        observed_codes.join(" · ")
    );
}
