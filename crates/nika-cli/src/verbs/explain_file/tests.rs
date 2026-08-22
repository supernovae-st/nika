// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `explain <file>` tests — split from `explain_file.rs` 2026-08-12 at the
//! 1,500-LOC ceiling (the `evidence/tests.rs` precedent · the descent's
//! forecast battery pushed the file to 1,579).

use super::*;
use crate::verbs::exit;

fn tmp(name: &str, content: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "nika-explain-file-{}-{name}.nika.yaml",
        std::process::id(),
    ));
    std::fs::write(&path, content).expect("fixture written");
    path
}

const DIAMOND: &str = "nika: brief-factory\n\nmodel: mock/echo\n\ntasks:\n  root:\n    infer: { prompt: \"r\", max_tokens: 10 }\n  left:\n    after:\n      root: success\n    infer: { prompt: \"l\", max_tokens: 10 }\n  right:\n    after:\n      root: success\n    infer: { prompt: \"x\", max_tokens: 10 }\n  join:\n    after:\n      left: success\n      right: success\n    infer: { prompt: \"j\", max_tokens: 10 }\noutputs:\n  result: ${{ tasks.join.output }}\n";

#[test]
fn narrates_the_diamond_with_cost_and_handoff() {
    let path = tmp("diamond", DIAMOND);
    let out = run(path.to_str().expect("utf8"), false, false);
    std::fs::remove_file(&path).ok();
    assert_eq!(out.code, exit::OK, "{}", out.text);
    for needle in [
        "brief-factory",
        "4 tasks · 3 waves · checks clean",
        "the story",
        "wave 2 — 2 in parallel",
        "asks mock/echo",
        "cost before a token is spent",
        "what it touches",
        "mock/echo (4 tasks)",
        "run it",
        "nika run",
        "flight recorder",
    ] {
        assert!(
            out.text.contains(needle),
            "missing `{needle}`:\n{}",
            out.text
        );
    }
    // The default model IS mock — no redundant mock-rehearsal line.
    assert!(
        !out.text.contains("offline rehearsal"),
        "mock workflows need no mock hint:\n{}",
        out.text
    );
    // If root fails, everything downstream is named.
    assert!(
        out.text
            .contains("if root fails, 3 downstream tasks never run"),
        "{}",
        out.text
    );
}

#[test]
fn unbounded_cost_claims_no_bound_and_names_the_priced_portion() {
    // qwen has no max_tokens → NoTokenLimit. The 2026-07-29 FLOOR
    // finding: `≥ $X — a FLOOR` claimed a lower bound over a number
    // that bounds nothing from below (render.rs documents the 126×
    // measurement). The narration now claims NEITHER bound — it shows
    // the priced portion, names the uncapped, and never renders a
    // fake $0 ceiling nor the word FLOOR.
    let path = tmp(
        "floor",
        "nika: floor-story\ntasks:\n  think:\n    infer: { prompt: \"x\" }\n",
    );
    let out = run(path.to_str().expect("utf8"), false, false);
    std::fs::remove_file(&path).ok();
    assert_eq!(out.code, exit::OK, "{}", out.text);
    assert!(
        out.text.contains("bounded portion"),
        "the priced portion, named as exactly that:\n{}",
        out.text
    );
    assert!(
        out.text.contains("no total ceiling"),
        "the honest no-ceiling verdict:\n{}",
        out.text
    );
    assert!(
        out.text.contains("no max_tokens declared"),
        "names the reason:\n{}",
        out.text
    );
    assert!(
        !out.text.contains("FLOOR") && !out.text.contains('≥'),
        "neither the word nor the sign survives (the finding's two wrongs):\n{}",
        out.text
    );
    assert!(
        !out.text.contains("≤ $"),
        "an unbounded workflow never shows a ceiling:\n{}",
        out.text
    );
}

#[test]
fn json_twin_is_versioned_and_speaks_the_report_dialect() {
    let path = tmp("json", DIAMOND);
    let out = run(path.to_str().expect("utf8"), true, false);
    std::fs::remove_file(&path).ok();
    assert_eq!(out.code, exit::OK, "{}", out.text);
    let v: serde_json::Value = serde_json::from_str(&out.text).expect("parses");
    assert_eq!(v["explain_version"], 1);
    assert_eq!(v["workflow"], "brief-factory");
    assert_eq!(v["clean"], true);
    assert_eq!(v["waves"].as_array().map(Vec::len), Some(3));
    assert_eq!(v["waves"][1].as_array().map(Vec::len), Some(2));
    assert_eq!(v["tasks"][0]["story"], "asks mock/echo");
    // The report's own vocabulary rides through (one dialect).
    assert!(v["cost"]["bounded_total_usd"].is_number(), "{}", out.text);
    assert!(v["requirements"]["models"].is_array(), "{}", out.text);
}

#[test]
fn a_dirty_file_gets_findings_first_never_a_story() {
    // `when:` as a bare string is a conformance finding — explain
    // must refuse to narrate and hand over to check (exit 2).
    let path = tmp(
        "dirty",
        "nika: dirty\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n  b:\n    after:\n      a: success\n    when: maybe\n    exec: { command: [\"echo\", \"y\"] }\n",
    );
    let out = run(path.to_str().expect("utf8"), false, false);
    std::fs::remove_file(&path).ok();
    assert_eq!(out.code, exit::FILE, "{}", out.text);
    assert!(out.text.contains("does not check clean"), "{}", out.text);
    assert!(out.text.contains("fix first: nika check"), "{}", out.text);
    assert!(!out.text.contains("the story"), "{}", out.text);
}

#[test]
fn traces_glance_finds_the_lexicographically_latest_journal() {
    let dir = std::env::temp_dir().join(format!("nika-explain-traces-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("2026-07-01T10-00-00Z-aaaa.ndjson"), "x").expect("write");
    std::fs::write(dir.join("2026-07-08T09-49-09Z-9c3f.ndjson"), "x").expect("write");
    std::fs::write(dir.join("notes.txt"), "x").expect("write");
    let glance = traces_glance(&dir);
    std::fs::remove_dir_all(&dir).ok();
    let (n, latest) = glance.expect("two journals found");
    assert_eq!(n, 2);
    assert!(
        latest.ends_with("2026-07-08T09-49-09Z-9c3f.ndjson"),
        "{latest}"
    );
}

/// `explain` routes codes to the teacher and paths to the narrator —
/// and the tie (a string that exists on disk) goes to the file.
#[test]
fn dispatch_routes_codes_and_files() {
    // Codes: registry + spec + bare forms stay the teaching surface.
    for code in ["NIKA-440", "440", "DAG-003"] {
        let out = dispatch(
            code,
            false,
            false,
            crate::display::theme::Theme::new(false, false, false),
        );
        assert_eq!(out.code, exit::OK, "{code}: {}", out.text);
    }
    // A path-shaped query routes to the file narrator — missing file
    // = the loader's own error, never a "unknown code" 404.
    let out = dispatch(
        "no/such/dir/flow.nika.yaml",
        false,
        false,
        crate::display::theme::Theme::new(false, false, false),
    );
    assert!(
        !out.text.contains("unknown code"),
        "paths never 404 as codes: {}",
        out.text
    );
    // The code form refuses --json loudly instead of ignoring it.
    let out = dispatch(
        "NIKA-440",
        true,
        false,
        crate::display::theme::Theme::new(false, false, false),
    );
    assert_eq!(out.code, exit::FILE);
    assert!(out.text.contains("--json"), "{}", out.text);
}

#[test]
fn unbounded_glosses_never_say_zero() {
    assert!(unbounded_gloss("t", Some("x/y"), UnboundedReason::NoPrice).contains("never $0"));
    assert!(unbounded_gloss("t", None, UnboundedReason::NoTokenLimit).contains("no ceiling"));
    assert!(unbounded_gloss("t", None, UnboundedReason::UnknownIterations).contains("run time"));
}

// ─── the forecast surface · staged-history integration ─────────────
// Every case drives the REAL seam (`run_with_traces`) over traces
// staged through the SAME serde path the sink writes — the honesty
// matrix rules the plan pins, proven at the render/JSON surface.

// The staged-journal fixtures — local twins of the set that descended
// with the trace plane to `nika-trace` (2026-08-11): a `#[cfg(test)]`
// fixture module cannot cross a crate boundary, and the house pattern
// is one fixture set per crate (the nika-dap store-tests twin).
use nika_event::EventKind;
use nika_types::resource::Value;
use std::time::Duration;

/// A fresh per-test trace directory under the cargo tmp root.
fn temp_store(name: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join("nika-cli-trace-store");
    let dir = base.join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("store dir");
    dir
}

/// Stage one trace file and BACKDATE its mtime by `age` — the age
/// clock the policy reads is the file's mtime.
fn stage_trace(dir: &Path, name: &str, body: &str, age: Duration) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).expect("trace staged");
    let mtime = std::time::SystemTime::now()
        .checked_sub(age)
        .expect("test age fits the clock");
    let file = std::fs::File::options()
        .write(true)
        .open(&path)
        .expect("reopen for times");
    file.set_times(std::fs::FileTimes::new().set_modified(mtime))
        .expect("mtime set");
    path
}

/// The forecast journal fixtures (the descended `gather::tests` set,
/// verb-era call sites kept: `fx::…`).
mod fx {
    use nika_event::{Event, EventKind};
    use nika_types::id::EventId;
    use nika_types::resource::{KeyValue, Value};
    use nika_types::timestamp::Timestamp;
    use uuid::Uuid;

    /// One journal event with arbitrary KV fields (the store helper
    /// carries exactly one string field — forecasts need several).
    pub(super) fn ev(kind: EventKind, ms: u64, fields: &[(&str, Value)]) -> Event {
        let mut event = Event::new(EventId::new(Uuid::nil()), Timestamp::from_unix_ms(ms), kind);
        for (key, value) in fields {
            event = event.with_field(KeyValue::new(*key, value.clone()));
        }
        event
    }

    /// Serialize events as one NDJSON trace body.
    fn ndjson(events: &[Event]) -> String {
        let mut body = String::new();
        for e in events {
            body.push_str(&serde_json::to_string(e).expect("event serializes"));
            body.push('\n');
        }
        body
    }

    /// A complete run journal: started (with hashes) · tasks · terminal.
    pub(super) fn run_body(
        workflow: &str,
        sha: Option<&str>,
        started_ms: u64,
        tasks: &[Event],
        terminal: Option<Event>,
    ) -> String {
        let mut events = Vec::new();
        let mut fields = vec![("workflow", Value::string(workflow))];
        if let Some(sha) = sha {
            fields.push(("workflow_sha256", Value::string(sha)));
            fields.push(("workflow_sha256_lf", Value::string(sha)));
        }
        events.push(ev(EventKind::WorkflowStarted, started_ms, &fields));
        events.extend_from_slice(tasks);
        if let Some(t) = terminal {
            events.push(t);
        }
        ndjson(&events)
    }

    /// One completed-task frame with duration + optional cost/model.
    pub(super) fn task_done(
        id: &str,
        ms: u64,
        dur: u64,
        cost: Option<f64>,
        model: Option<&str>,
    ) -> Event {
        let mut fields = vec![
            ("task", Value::string(id)),
            ("duration_ms", Value::Int(i64::try_from(dur).unwrap_or(0))),
        ];
        if let Some(c) = cost {
            fields.push(("cost_usd", Value::Float(c)));
        }
        if let Some(m) = model {
            fields.push(("note", Value::string(format!("infer · {m}"))));
        }
        ev(EventKind::TaskCompleted, ms, &fields)
    }

    pub(super) fn done(ms: u64, total: Option<f64>) -> Event {
        let mut fields = vec![("workflow", Value::string("wf"))];
        if let Some(t) = total {
            fields.push(("total_cost_usd", Value::Float(t)));
        }
        ev(EventKind::WorkflowCompleted, ms, &fields)
    }
}

const FC: &str = "nika: fc-fix\n\nmodel: mock/echo\n\ntasks:\n  fetch:\n    exec: { command: [\"echo\", \"x\"] }\n  think:\n    after:\n      fetch: success\n    infer: { prompt: \"p\", max_tokens: 10 }\n";

/// One completed fc-fix run body: fetch (exec) + think (infer),
/// distinct durations, optional sha/model/extras.
fn fc_run(sha: Option<&str>, at: u64, think_ms: u64, extras: &[nika_event::Event]) -> String {
    // Extras (retries · notes) precede think's terminal — the fold's
    // LAST state wins, so a retry after the completion would leave
    // the row Retrying instead of Ok.
    let mut tasks = vec![fx::task_done("fetch", at + 10, 20, None, None)];
    tasks.extend_from_slice(extras);
    tasks.push(fx::task_done(
        "think",
        at + 40,
        think_ms,
        None,
        Some("mock/echo"),
    ));
    fx::run_body("fc-fix", sha, at, &tasks, Some(fx::done(at + 50, None)))
}

#[test]
fn forecast_flag_forces_the_section_and_absence_stays_silent() {
    let dir = temp_store("explain-fc-empty");
    let path = tmp("fc-empty", FC);
    let p = path.to_str().expect("utf8");
    let forced = run_with_traces(p, false, true, &dir);
    assert!(
        forced
            .text
            .contains("no forecast — this workflow has never run here"),
        "{}",
        forced.text
    );
    let silent = run_with_traces(p, false, false, &dir);
    assert!(!silent.text.contains("FORECAST"), "{}", silent.text);
    // JSON twin: ALWAYS present under the flag (honest empty shape) ·
    // absent without it below the auto threshold.
    let j = run_with_traces(p, true, true, &dir);
    let v: serde_json::Value = serde_json::from_str(&j.text).expect("parses");
    assert_eq!(v["forecast"]["runs"]["total"], 0);
    assert_eq!(v["forecast"]["run_duration"]["kind"], "never_ran");
    let j2 = run_with_traces(p, true, false, &dir);
    let v2: serde_json::Value = serde_json::from_str(&j2.text).expect("parses");
    assert!(v2.get("forecast").is_none(), "{}", j2.text);
    std::fs::remove_file(&path).ok();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ladder_rungs_render_from_staged_history() {
    let dir = temp_store("explain-fc-ladder");
    let path = tmp("fc-ladder", FC);
    let p = path.to_str().expect("utf8");
    let sha = crate::verbs::run::sha256_hex(FC.as_bytes());
    stage_trace(
        &dir,
        "2026-07-08T01-00-00Z-0001.ndjson",
        &fc_run(Some(&sha), 1_000, 100, &[]),
        Duration::from_secs(60),
    );
    let one = run_with_traces(p, false, true, &dir);
    assert!(one.text.contains("based on last 1 run "), "{}", one.text);
    assert!(one.text.contains("last run "), "{}", one.text);
    assert!(!one.text.contains("p90"), "{}", one.text);

    for (i, ms) in [(2u64, 200u64), (3, 300)] {
        stage_trace(
            &dir,
            &format!("2026-07-08T0{i}-00-00Z-000{i}.ndjson"),
            &fc_run(Some(&sha), i * 10_000, ms, &[]),
            Duration::from_secs(60 - i),
        );
    }
    // n = 3: the section arrives UNPROMPTED (auto threshold) · range
    // vocabulary only.
    let auto = run_with_traces(p, false, false, &dir);
    assert!(auto.text.contains("based on last 3 runs"), "{}", auto.text);
    assert!(!auto.text.contains("p90"), "{}", auto.text);

    for i in 4u64..=6 {
        stage_trace(
            &dir,
            &format!("2026-07-08T0{i}-00-00Z-000{i}.ndjson"),
            &fc_run(Some(&sha), i * 10_000, 100 * i, &[]),
            Duration::from_secs(60 - i),
        );
    }
    let bands = run_with_traces(p, false, true, &dir);
    assert!(
        bands.text.contains("based on last 6 runs"),
        "{}",
        bands.text
    );
    assert!(bands.text.contains("(p90 "), "{}", bands.text);
    assert!(
        bands.text.contains("low confidence (n<10)"),
        "{}",
        bands.text
    );
    std::fs::remove_file(&path).ok();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn honesty_composition_stale_unknown_unpriced_and_retry() {
    let dir = temp_store("explain-fc-honesty");
    let path = tmp("fc-honesty", FC);
    let p = path.to_str().expect("utf8");
    let sha = crate::verbs::run::sha256_hex(FC.as_bytes());
    // Same-hash run whose think RETRIED then completed (flaky ≠ failed).
    let retry = fx::ev(
        EventKind::TaskRetrying,
        10_020,
        &[("task", Value::string("think"))],
    );
    stage_trace(
        &dir,
        "2026-07-08T01-00-00Z-aaaa.ndjson",
        &fc_run(Some(&sha), 10_000, 100, &[retry]),
        Duration::from_secs(50),
    );
    // A stale-hash run and an unverifiable (hashless) run.
    stage_trace(
        &dir,
        "2026-07-08T02-00-00Z-bbbb.ndjson",
        &fc_run(Some("beef"), 20_000, 120, &[]),
        Duration::from_secs(40),
    );
    stage_trace(
        &dir,
        "2026-07-08T03-00-00Z-cccc.ndjson",
        &fc_run(None, 30_000, 140, &[]),
        Duration::from_secs(30),
    );
    // An unpriced think occurrence (local-model class) — ≥ composes.
    let unpriced = fx::ev(
        EventKind::TaskCompleted,
        40_040,
        &[
            ("task", Value::string("think")),
            ("duration_ms", Value::Int(160)),
            ("cost_unpriced", Value::string("local_model")),
        ],
    );
    let body = fx::run_body(
        "fc-fix",
        Some(&sha),
        40_000,
        &[fx::task_done("fetch", 40_010, 20, None, None), unpriced],
        Some(fx::done(40_050, None)),
    );
    stage_trace(
        &dir,
        "2026-07-08T04-00-00Z-dddd.ndjson",
        &body,
        Duration::from_secs(20),
    );

    let out = run_with_traces(p, false, true, &dir);
    for needle in [
        "1 predate the last edit",
        "1 unverifiable",
        "passed on retry 1/",
        "unpriced: local_model",
        "≥",
    ] {
        assert!(
            out.text.contains(needle),
            "missing `{needle}`:\n{}",
            out.text
        );
    }
    // fetch is exec: no cost sample and no unpriced slug — the honest
    // dash, never an invented $. Scope to the FORECAST block: the
    // shape section's wires line also starts with `fetch`.
    let fc_start = out.text.find("FORECAST").expect("forecast section");
    let fetch_row = out.text[fc_start..]
        .lines()
        .find(|l| l.trim_start().starts_with("fetch"))
        .expect("fetch row renders");
    assert!(fetch_row.contains('—'), "{fetch_row}");
    assert!(!fetch_row.contains('$'), "{fetch_row}");
    std::fs::remove_file(&path).ok();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn json_twin_tags_rungs_census_and_retention_knob() {
    let dir = temp_store("explain-fc-json");
    let path = tmp("fc-json", FC);
    let p = path.to_str().expect("utf8");
    let sha = crate::verbs::run::sha256_hex(FC.as_bytes());
    for i in 1u64..=6 {
        stage_trace(
            &dir,
            &format!("2026-07-08T0{i}-00-00Z-json{i}.ndjson"),
            &fc_run(Some(&sha), i * 10_000, 100 * i, &[]),
            Duration::from_secs(60 - i),
        );
    }
    let j = run_with_traces(p, true, true, &dir);
    let v: serde_json::Value = serde_json::from_str(&j.text).expect("parses");
    let fc = &v["forecast"];
    assert_eq!(fc["run_duration"]["kind"], "bands");
    assert_eq!(fc["runs"]["completed"], 6);
    assert_eq!(fc["runs"]["same_hash"], 6);
    let expected_keep = nika_cli_host::retention::RetentionConfig::from_env()
        .0
        .keep_last;
    assert_eq!(
        fc["window"]["retention_keep"],
        serde_json::json!(expected_keep)
    );
    std::fs::remove_file(&path).ok();
    let _ = std::fs::remove_dir_all(dir);
}

/// The code form refuses --forecast as loudly as --json — an error
/// code has no run history; silence would strand an agent.
#[test]
fn code_form_refuses_forecast_loudly() {
    let out = dispatch(
        "NIKA-440",
        false,
        true,
        crate::display::theme::Theme::new(false, false, false),
    );
    assert_eq!(out.code, exit::FILE);
    assert!(out.text.contains("--forecast"), "{}", out.text);
}

// ─── the recovery rail · P0-12 trace-first ─────────────────────────
// The 2026-07-30 UX audit: the forecast reduced a failure to a stat
// and NO surface named `--resume` (ADR-099). A workflow whose LATEST
// run failed must open on the repair — task · cause · trace pointer
// · targeted resume — never on the naked re-run CTA.

/// One FAILED fc-fix run: fetch completed (optionally WITH the
/// ADR-099 resume keys), think failed with a coded detail, terminal
/// `workflow_failed`.
fn failed_run(sha: Option<&str>, at: u64, resume_keys: bool) -> String {
    let mut fetch_fields = vec![
        ("task", Value::string("fetch")),
        ("duration_ms", Value::Int(20)),
    ];
    if resume_keys {
        fetch_fields.push(("def_hash", Value::string("d".repeat(64))));
        fetch_fields.push(("input_hash", Value::string("i".repeat(64))));
        fetch_fields.push(("output", Value::string("\"ok\"")));
    }
    let fetch = fx::ev(EventKind::TaskCompleted, at + 10, &fetch_fields);
    let think = fx::ev(
        EventKind::TaskFailed,
        at + 40,
        &[
            ("task", Value::string("think")),
            ("note", Value::string("infer")),
            ("detail", Value::string("NIKA-TEST-1 · the model refused")),
        ],
    );
    let terminal = fx::ev(
        EventKind::WorkflowFailed,
        at + 50,
        &[("workflow", Value::string("fc-fix"))],
    );
    fx::run_body("fc-fix", sha, at, &[fetch, think], Some(terminal))
}

/// Re-write a staged body behind the tamper-evidence chain (the
/// sink's own genesis + per-line hashes — the same construction the
/// chain.rs tests use) so the rail can CLAIM the chain intact.
fn chain_wrap(body: &str) -> String {
    let mut chain = crate::verbs::run::sha256_hex(nika_dap::chain::CHAIN_GENESIS);
    let mut out = String::new();
    for line in body.lines().filter(|l| !l.trim().is_empty()) {
        let mut v: serde_json::Value = serde_json::from_str(line).expect("staged line parses");
        v["chain"] = serde_json::Value::String(chain);
        let line = serde_json::to_string(&v).expect("re-serializes");
        chain = crate::verbs::run::sha256_hex(line.as_bytes());
        out.push_str(&line);
        out.push('\n');
    }
    out
}

#[test]
fn a_failed_latest_run_opens_on_the_recovery_rail() {
    let dir = temp_store("explain-rail");
    let path = tmp("rail", FC);
    let p = path.to_str().expect("utf8");
    let sha = crate::verbs::run::sha256_hex(FC.as_bytes());
    // Older clean run, then the NEWEST fc-fix run failed (chain
    // wrapped) — and a still-newer trace of ANOTHER workflow: the
    // rail folds the latest run of THIS workflow, never the global
    // latest.
    stage_trace(
        &dir,
        "2026-07-08T01-00-00Z-0001.ndjson",
        &fc_run(Some(&sha), 1_000, 100, &[]),
        Duration::from_secs(90),
    );
    stage_trace(
        &dir,
        "2026-07-08T02-00-00Z-0002.ndjson",
        &chain_wrap(&failed_run(Some(&sha), 2_000, true)),
        Duration::from_secs(60),
    );
    let other = fx::run_body("other", None, 3_000, &[], Some(fx::done(3_010, None)));
    stage_trace(
        &dir,
        "2026-07-08T03-00-00Z-0003.ndjson",
        &other,
        Duration::from_secs(30),
    );
    let out = run_with_traces(p, false, false, &dir);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    // The rail OPENS the render — before the header, before any CTA.
    let rail = out
        .text
        .find("last run failed")
        .expect("the rail opens the render");
    let header = out.text.find("fc-fix").expect("the header renders");
    assert!(rail < header, "the rail opens:\n{}", out.text);
    // Task + cause + the trace pointer + the chain claim, and the
    // RESUME names THIS workflow's failed trace (never the other
    // workflow's newer one).
    for needle in [
        "think",
        "NIKA-TEST-1",
        "nika trace show",
        "chain intact",
        "2026-07-08T02-00-00Z-0002.ndjson",
    ] {
        assert!(
            out.text.contains(needle),
            "missing `{needle}`:\n{}",
            out.text
        );
    }
    // The naked CTA is REPLACED by the targeted resume while the
    // failure stands unaudited.
    assert!(!out.text.contains("\nrun it\n"), "{}", out.text);
    assert!(
        out.text.contains(&format!("nika run {p} --resume")),
        "{}",
        out.text
    );
    std::fs::remove_file(&path).ok();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_keyless_unverified_failure_routes_to_recheck_never_resume() {
    let dir = temp_store("explain-rail-keyless");
    let path = tmp("rail-keyless", FC);
    let p = path.to_str().expect("utf8");
    stage_trace(
        &dir,
        "2026-07-08T02-00-00Z-0002.ndjson",
        &failed_run(None, 2_000, false),
        Duration::from_secs(30),
    );
    let out = run_with_traces(p, false, false, &dir);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    assert!(out.text.contains("last run failed"), "{}", out.text);
    // No resume keys → the rail never suggests --resume; the route is
    // re-check, and the chain absence is said out loud.
    assert!(!out.text.contains("--resume"), "{}", out.text);
    assert!(out.text.contains("nika check"), "{}", out.text);
    assert!(out.text.contains("unverified"), "{}", out.text);
    assert!(!out.text.contains("\nrun it\n"), "{}", out.text);
    std::fs::remove_file(&path).ok();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_clean_latest_run_keeps_the_naked_cta_and_no_rail() {
    let dir = temp_store("explain-rail-clean");
    let path = tmp("rail-clean", FC);
    let p = path.to_str().expect("utf8");
    let sha = crate::verbs::run::sha256_hex(FC.as_bytes());
    stage_trace(
        &dir,
        "2026-07-08T01-00-00Z-0001.ndjson",
        &fc_run(Some(&sha), 1_000, 100, &[]),
        Duration::from_secs(30),
    );
    let out = run_with_traces(p, false, false, &dir);
    assert_eq!(out.code, exit::OK, "{}", out.text);
    // A clean newest trace → the current render is UNTOUCHED.
    assert!(!out.text.contains("last run failed"), "{}", out.text);
    assert!(out.text.contains("\nrun it\n"), "{}", out.text);
    std::fs::remove_file(&path).ok();
    let _ = std::fs::remove_dir_all(dir);
}

fn plain_theme() -> crate::display::theme::Theme {
    crate::display::theme::Theme::new(false, true, false)
}

/// #1106 taught `nika explain native-first/006` (the token check prints
/// in `[brackets]`). The library `explain::run` already answers. The CLI
/// dispatch used to treat any `/` as a file path, so the taught form
/// 404'd as `cannot read native-first/006`. A test on `dispatch` is the
/// one that would have caught it — `explain::run` alone is blind to the
/// router.
#[test]
fn a_numbered_native_first_hint_is_not_a_file_path() {
    let out = dispatch("native-first/006", false, false, plain_theme());
    assert_eq!(out.code, exit::OK, "{}", out.text);
    assert!(
        out.text.starts_with("native-first/006 · hint"),
        "slash-containing hint ids teach, they are not paths:\n{}",
        out.text
    );
    assert!(out.text.contains("nika:wait"), "{}", out.text);
    assert!(
        !out.text.contains("cannot read"),
        "must not fall through to the file form:\n{}",
        out.text
    );
}

#[test]
fn every_numbered_native_first_rule_teaches_through_dispatch() {
    for n in 1..=6 {
        let id = format!("native-first/{n:03}");
        let out = dispatch(&id, false, false, plain_theme());
        assert_eq!(out.code, exit::OK, "{id} → {}", out.text);
        assert!(
            out.text.starts_with(&format!("{id} · hint")),
            "{id} must teach:\n{}",
            out.text
        );
    }
}

#[test]
fn jq_as_map_still_teaches_through_dispatch() {
    let out = dispatch("jq-as-map", false, false, plain_theme());
    assert_eq!(out.code, exit::OK, "{}", out.text);
    assert!(out.text.starts_with("jq-as-map · hint"), "{}", out.text);
}

#[test]
fn an_existing_yaml_path_still_narrates_as_a_file() {
    let path = tmp("still-a-file", DIAMOND);
    let out = dispatch(path.to_str().expect("utf8"), false, false, plain_theme());
    std::fs::remove_file(&path).ok();
    assert_eq!(out.code, exit::OK, "{}", out.text);
    assert!(out.text.contains("brief-factory"), "{}", out.text);
    assert!(out.text.contains("the story"), "{}", out.text);
}
