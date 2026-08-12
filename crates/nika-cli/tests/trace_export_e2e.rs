#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
// The suite EXECUTES the real binary — `std::process::Command` is the
// point here (the disallowed-types rule guards src/, not bin-e2e).
#![allow(clippy::disallowed_types)]

//! `nika trace export` at the REAL binary — run → journal → OTLP file.
//!
//! A battery-era suite (the 2026-07-06 full-battery flip runs tests/ in
//! CI): the pins here guard the projection against the journal the
//! engine ACTUALLY writes — the settle-burst timestamp shape, the real
//! retry fields, the skip-why — not a hand-built fixture's idea of it.

use std::process::Command;

fn run_and_export(dir: &std::path::Path) -> serde_json::Value {
    std::fs::write(
        dir.join("w.nika.yaml"),
        r#"nika: export-e2e
permits: { exec: ["sleep", "true"] }
tasks:
  seed:
    exec:
      command: ["sleep", "0.3"]
  gated:
    with:
      seed_status: ${{ tasks.seed.status }}
    when: "${{ with.seed_status == 'failure' }}"
    exec:
      command: ["true"]
"#,
    )
    .expect("write workflow");

    let run = Command::new(env!("CARGO_BIN_EXE_nika-cli"))
        .args(["run", "w.nika.yaml", "--json", "--color", "never"])
        .current_dir(dir)
        .output()
        .expect("run spawns");
    assert!(
        run.status.success(),
        "run: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let traces = dir.join(".nika/traces");
    let journal = std::fs::read_dir(&traces)
        .expect("traces dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .expect("one journal");

    let export = Command::new(env!("CARGO_BIN_EXE_nika-cli"))
        .args(["trace", "export"])
        .arg(&journal)
        .current_dir(dir)
        .output()
        .expect("export spawns");
    assert!(
        export.status.success(),
        "export: {}",
        String::from_utf8_lossy(&export.stderr)
    );

    let otlp = journal.with_extension("").with_extension("otlp.jsonl");
    let line = std::fs::read_to_string(&otlp).expect("otlp file written beside the journal");
    serde_json::from_str(line.trim()).expect("one valid OTLP JSON value per line")
}

#[test]
fn export_projects_the_real_journal_with_true_durations() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let v = run_and_export(dir.path());

    let spans = v["resourceSpans"][0]["scopeSpans"][0]["spans"]
        .as_array()
        .expect("spans");
    let root = &spans[0];

    // Identity laws on the REAL journal: 32/16-char hex, string nanos.
    assert_eq!(root["traceId"].as_str().unwrap().len(), 32);
    assert_eq!(root["spanId"].as_str().unwrap().len(), 16);
    assert!(root["startTimeUnixNano"].is_string());
    assert_eq!(root["name"], "invoke_workflow export-e2e");

    // The #210 identity rides the root of a real run.
    let sha = root["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|kv| kv["key"] == "nika.workflow.sha256")
        .expect("workflow_sha256 on root");
    assert_eq!(sha["value"]["stringValue"].as_str().unwrap().len(), 64);

    // THE settle-burst law: the engine stamps started+terminal in one
    // burst, so a naive frame-gap span would be ~0ns wide. The exporter
    // must walk duration_ms back from the settle — a 300ms sleep yields
    // a span at least that wide.
    let seed = spans
        .iter()
        .find(|sp| sp["name"] == "seed")
        .expect("seed span");
    let start: i64 = seed["startTimeUnixNano"].as_str().unwrap().parse().unwrap();
    let end: i64 = seed["endTimeUnixNano"].as_str().unwrap().parse().unwrap();
    assert!(
        end - start >= 250_000_000,
        "span width carries the TRUE duration (got {}ns)",
        end - start
    );

    // The #211 why rides the skip span of a real run.
    let gated = spans
        .iter()
        .find(|sp| sp["name"] == "gated")
        .expect("gated span");
    let when = gated["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|kv| kv["key"] == "nika.task.when")
        .expect("when on skip");
    assert!(
        when["value"]["stringValue"]
            .as_str()
            .unwrap()
            .contains("with.seed_status"),
        "the recorded when: carries the author's LOCAL read (the status \
         itself crossed the boundary as a with: binding)"
    );
}

/// The failure moment teaches its own forensics (runtime UX): a failed
/// run's trace pointer carries the autopsy line naming the failed task;
/// a clean run stays autopsy-free.
#[test]
fn a_failed_run_prints_its_autopsy_a_clean_run_does_not() {
    let dir = tempfile::tempdir().expect("tmpdir");
    std::fs::write(
        dir.path().join("fail.nika.yaml"),
        "nika: fail-ux\npermits: { exec: [\"false\"] }\ntasks:\n  boom:\n    exec:\n      command: [\"false\"]\n",
    )
    .expect("write");
    let out = Command::new(env!("CARGO_BIN_EXE_nika-cli"))
        .args(["run", "fail.nika.yaml", "--no-progress"])
        .current_dir(dir.path())
        .output()
        .expect("spawns");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("autopsy: nika trace peek") && text.contains(" boom"),
        "the autopsy names the failed task: {text}"
    );
    assert!(
        text.contains("trace replay"),
        "the replay path rides: {text}"
    );

    std::fs::write(
        dir.path().join("ok.nika.yaml"),
        "nika: ok-ux\npermits: { exec: [\"true\"] }\ntasks:\n  fine:\n    exec:\n      command: [\"true\"]\n",
    )
    .expect("write");
    let out = Command::new(env!("CARGO_BIN_EXE_nika-cli"))
        .args(["run", "ok.nika.yaml", "--no-progress"])
        .current_dir(dir.path())
        .output()
        .expect("spawns");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        !text.contains("autopsy:"),
        "a clean run is autopsy-free: {text}"
    );
}
