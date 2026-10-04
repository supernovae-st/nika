// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The observed room running `nika:jq` through this binary's isolated helper: real room, real
//! runtime, real helper processes, synthetic files only. A counting workflow is executed and
//! judged on what it wrote: two exposed rows, one paid; the expected count is computed here from
//! the rows, never by the product. A graph whose count reads the unfiltered rows writes 2, the
//! graph whose count reads its filter writes 1. Runaway programs are stopped by their bounds,
//! drained and reaped, never reported as a program's failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_types)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use nika_onboard::compile::rehearse::{
    Attempt, FinalState, Held, Refusal, Rehearsal, RehearsalReport, Rehearse,
};
use nika_onboard::compile::room::ObservedRoom;
use nika_onboard::compile::{
    AuthoringPolicy, Cognition, CompileOutcome, CompileRequest, CompileStatus, NativeMode,
    compile_with_cognition_rehearsed,
};
use nika_service_execution::JqHelper;
use serde_json::{Value, json};
use std::sync::Mutex;

/// The two exposed rows, one paid.
const ROWS: &str =
    r#"[{"id":1,"amount_usd":5,"status":"paid"},{"id":2,"amount_usd":20,"status":"late"}]"#;
const INPUT: &str = "./in/input.json";
const RESULT: &str = "./out/result.json";
const FILTER: &str = r#"fromjson | map(select(.status == "paid"))"#;

/// The count this file expects, from the rows alone.
fn expected_count() -> u64 {
    let rows: Vec<Value> = serde_json::from_str(ROWS).unwrap();
    let paid = rows.iter().filter(|row| row["status"] == "paid").count();
    u64::try_from(paid).unwrap()
}

/// A project holding the rows, and a scratch parent beside it.
struct World {
    base: tempfile::TempDir,
}

impl World {
    fn new() -> Self {
        let base = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("project/in")).unwrap();
        std::fs::create_dir_all(base.path().join("scratch")).unwrap();
        std::fs::write(base.path().join("project/in/input.json"), ROWS).unwrap();
        Self { base }
    }

    fn project(&self) -> PathBuf {
        self.base.path().join("project")
    }

    fn room(&self) -> ObservedRoom {
        ObservedRoom::new(self.project())
            .with_scratch_parent(self.base.path().join("scratch"))
            .with_jq_helper(JqHelper::new(env!("CARGO_BIN_EXE_nika")))
    }

    /// Every file of the project, by relative path.
    fn files(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut files = Vec::new();
        walk(&self.project(), &self.project(), &mut files);
        files.sort();
        files
    }
}

fn walk(root: &Path, dir: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            // A session's own state and traces are no project file.
            if path.file_name().is_some_and(|name| name == ".nika") {
                continue;
            }
            walk(root, &path, files);
        } else {
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            files.push((rel, std::fs::read(&path).unwrap()));
        }
    }
}

/// Read the rows, filter the paid ones, count what `count_from` returns with `count`, write it.
fn graph(count_from: &str, count: &str) -> String {
    format!(
        r#"nika: paid-count
permits:
  tools: ["nika:read", "nika:jq", "nika:write"]
  fs:
    read: ["{INPUT}"]
    write: ["{RESULT}"]
tasks:
  read_input:
    invoke:
      tool: "nika:read"
      args: {{ path: "{INPUT}" }}
  filter:
    with: {{ rows: "${{{{ tasks.read_input.output }}}}" }}
    invoke:
      tool: "nika:jq"
      args: {{ input: "${{{{ with.rows }}}}", expression: '{FILTER}' }}
  count:
    with: {{ rows: "${{{{ tasks.{count_from}.output }}}}" }}
    invoke:
      tool: "nika:jq"
      args: {{ input: "${{{{ with.rows }}}}", expression: '{count}' }}
  write_result:
    with: {{ content: "${{{{ tasks.count.output }}}}" }}
    invoke:
      tool: "nika:write"
      args: {{ path: "{RESULT}", content: "${{{{ with.content }}}}", overwrite: true, create_dirs: true }}
"#
    )
}

/// One jq step over the rows' text, its value written.
fn single(expression: &str) -> String {
    format!(
        r#"nika: runaway
permits:
  tools: ["nika:read", "nika:jq", "nika:write"]
  fs:
    read: ["{INPUT}"]
    write: ["{RESULT}"]
tasks:
  read_input:
    invoke:
      tool: "nika:read"
      args: {{ path: "{INPUT}" }}
  compute:
    with: {{ rows: "${{{{ tasks.read_input.output }}}}" }}
    invoke:
      tool: "nika:jq"
      args: {{ input: "${{{{ with.rows }}}}", expression: '{expression}' }}
  write_result:
    with: {{ content: "${{{{ tasks.compute.output }}}}" }}
    invoke:
      tool: "nika:write"
      args: {{ path: "{RESULT}", content: "${{{{ with.content }}}}", overwrite: true, create_dirs: true }}
"#
    )
}

async fn rehearsed(world: &World, candidate: &str) -> RehearsalReport {
    let room = world.room();
    room.rehearse_reading(candidate, &[INPUT.to_owned()], &[RESULT.to_owned()])
        .await
}

/// The count the room read back from the result file, when the run wrote one.
fn written_count(report: &RehearsalReport) -> Option<u64> {
    let read = report
        .observation
        .finals
        .iter()
        .find(|read| read.path.ends_with("out/result.json"))?;
    match &read.state {
        FinalState::File {
            held: Held::Whole(text),
            ..
        } => serde_json::from_str::<Value>(text).ok()?["count"].as_u64(),
        _ => None,
    }
}

#[tokio::test]
async fn the_room_runs_both_graphs_and_only_the_connected_filter_writes_the_paid_count() {
    assert_eq!(expected_count(), 1);
    let world = World::new();
    let before = world.files();
    let disconnected = graph("read_input", "fromjson | {count: length}");
    let connected = graph("filter", "{count: length}");
    let wrong = rehearsed(&world, &disconnected).await;
    let right = rehearsed(&world, &connected).await;
    for report in [&wrong, &right] {
        assert!(
            matches!(report.outcome, Rehearsal::Passed { .. }),
            "{report:#?}"
        );
        assert!(
            matches!(report.attempt, Attempt::Completed { .. }),
            "{report:#?}"
        );
        assert!(report.room.cleaned, "{report:#?}");
        assert!(report.observation.ledger.drained, "{report:#?}");
    }
    assert_eq!(written_count(&wrong), Some(2), "{wrong:#?}");
    assert_eq!(written_count(&right), Some(expected_count()), "{right:#?}");
    assert_ne!(wrong.candidate_sha256, right.candidate_sha256);
    assert_eq!(world.files(), before, "the project is unchanged");
}

#[tokio::test]
async fn a_runaway_program_is_stopped_by_its_bound_drained_and_never_a_failure() {
    let world = World::new();
    let before = world.files();
    for expression in [
        "[range(1e12)] | length",
        "reduce range(40) as $i (.; . + .) | length",
        "def f: f; f",
    ] {
        let started = Instant::now();
        let report = rehearsed(&world, &single(expression)).await;
        let elapsed = started.elapsed();
        assert!(
            matches!(&report.outcome, Rehearsal::NotRun { reason } if reason.contains("stopped by its bound")),
            "{expression}: {report:#?}"
        );
        assert!(
            matches!(report.attempt, Attempt::Stopped { .. }),
            "{expression}: {report:#?}"
        );
        assert_eq!(
            report.observation.refusal,
            Some(Refusal::DataBounds),
            "{expression}"
        );
        assert!(
            report.observation.failure.is_none(),
            "{expression}: {report:#?}"
        );
        assert!(report.room.cleaned, "{expression}: {report:#?}");
        assert!(
            report.observation.ledger.drained,
            "{expression}: {report:#?}"
        );
        assert_eq!(written_count(&report), None, "{expression}");
        let ceiling = ObservedRoom::BOUND + Duration::from_secs(5);
        assert!(elapsed < ceiling, "{expression}: {elapsed:?}");
    }
    assert_eq!(world.files(), before, "the project is unchanged");
}

#[tokio::test]
async fn without_a_helper_a_jq_step_is_screened_before_any_room() {
    let world = World::new();
    let room =
        ObservedRoom::new(world.project()).with_scratch_parent(world.base.path().join("scratch"));
    assert!(room.jq_helper().is_none());
    let report = room
        .rehearse_reading(
            &graph("filter", "{count: length}"),
            &[INPUT.to_owned()],
            &[],
        )
        .await;
    assert!(
        matches!(report.attempt, Attempt::NeverAttempted),
        "{report:#?}"
    );
    assert_eq!(
        report.observation.refusal,
        Some(Refusal::DataBounds),
        "{report:#?}"
    );
    assert!(!report.room.prepared, "{report:#?}");
}

// ─── the repair, through the compile entry, on actual output ───────────────────────────────

const INTENT: &str = "read ./in/input.json, count the rows where status is paid, write the count to ./out/result.json";

/// A SCRIPTED author (a provider double, no model): its queued sketches and fills in order, the
/// last one repeated, and an approving whole-request judge. It records what each call asked.
struct Author {
    sketches: Mutex<Vec<String>>,
    fills: Mutex<Vec<String>>,
    asked: Mutex<Vec<&'static str>>,
}

fn next(queue: &Mutex<Vec<String>>) -> String {
    let mut queued = queue.lock().unwrap();
    if queued.len() > 1 {
        queued.remove(0)
    } else {
        queued[0].clone()
    }
}

impl ProviderInferDyn for Author {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let schema = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema.clone(),
            _ => Value::Null,
        };
        let properties = &schema["properties"];
        let (kind, text) = if let Some(keys) = properties["choice"]["enum"].as_array() {
            let approve = ["faithful", "carried"]
                .into_iter()
                .find(|key| keys.iter().any(|value| value == *key))
                .unwrap_or("none");
            ("judge", json!({"choice": approve}).to_string())
        } else if properties.get("fills").is_some() {
            ("fills", next(&self.fills))
        } else {
            ("sketch", next(&self.sketches))
        };
        self.asked.lock().unwrap().push(kind);
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// Read, filter, count, write; the count reads `count_from`. The filter task exists either way.
fn sketch(count_from: &str) -> String {
    let task = |id: &str, tool: &str, extra: Value| {
        let mut task = json!({"id": id, "verb": "invoke", "tool": tool, "purpose": id});
        for (key, value) in extra.as_object().unwrap() {
            task[key] = value.clone();
        }
        task
    };
    json!({"name": "paid-count", "tasks": [
        task("read_input", "nika:read", json!({"reads": [INPUT]})),
        task("filter", "nika:jq", json!({"with": [{"name": "rows", "from": "read_input"}]})),
        task("count", "nika:jq", json!({"with": [{"name": "rows", "from": count_from}]})),
        task("write_result", "nika:write", json!({"writes": [RESULT],
            "with": [{"name": "text", "from": "count"}]})),
    ], "questions": [], "gaps": [], "notes": "read, filter, count, write"})
    .to_string()
}

fn fills(count: &str) -> String {
    json!({"fills": [
        {"task": "filter", "field": "expression", "value": FILTER},
        {"task": "count", "field": "expression", "value": count},
    ], "notes": "two holes"})
    .to_string()
}

/// The disconnected filter: the count parses and counts the read rows.
const RAW: &str = "fromjson | {count: length}";
/// The reconnected filter: the count counts what the filter returned.
const FILTERED: &str = "{count: length}";
/// A decoy: the filter expression is present, its result unused; the raw rows are counted.
const DECOY: &str =
    r#"fromjson | (map(select(.status == "paid")) | length) as $paid | {count: length}"#;

async fn compiled(world: &World, author: &Author) -> CompileOutcome {
    let request = CompileRequest::create(INTENT).with_authoring_policy(
        AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
            .with_native(NativeMode::Sketch)
            .with_repairs(2),
    );
    let cognition = Cognition {
        provider: Some(author),
        seat: None,
    };
    let room = world.room();
    compile_with_cognition_rehearsed(&request, cognition, Some(&room))
        .await
        .unwrap()
}

/// The count each rehearsed run wrote, in order, as the compile's journal kept it.
fn counts_written(out: &CompileOutcome) -> Vec<Option<u64>> {
    let reports = &out.provenance.decision.as_ref().unwrap()["rehearsal"]["reports"];
    (reports.as_array().into_iter().flatten())
        .map(|report| {
            let text = report["read_back"][0]["text"].as_str()?;
            serde_json::from_str::<Value>(text).ok()?["count"].as_u64()
        })
        .collect()
}

fn evidence(out: &CompileOutcome) -> Vec<Value> {
    let rounds = &out.provenance.decision.as_ref().unwrap()["native"]["rounds"];
    (rounds.as_array().into_iter().flatten())
        .filter_map(|round| round.get("evidence").cloned())
        .collect()
}

#[tokio::test]
async fn a_wrong_count_observed_in_the_room_is_repaired_by_reconnecting_the_filter() {
    let world = World::new();
    let before = world.files();
    let author = Author {
        sketches: Mutex::new(vec![sketch("read_input"), sketch("filter")]),
        fills: Mutex::new(vec![fills(RAW), fills(FILTERED)]),
        asked: Mutex::new(Vec::new()),
    };
    let out = compiled(&world, &author).await;
    assert_eq!(world.files(), before, "the project is unchanged");
    // Two candidates ran: the first wrote 2, the repair wrote the expected count.
    assert_eq!(
        counts_written(&out),
        [Some(2), Some(expected_count())],
        "{:#}",
        out.provenance.decision.as_ref().unwrap()["rehearsal"]
    );
    let found = evidence(&out);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert_eq!(found[0]["outcome"], "defect", "{found:#?}");
    assert_eq!(found[1]["outcome"], "holds", "{found:#?}");
    assert_eq!(
        found[0]["behaviour"]["contract_sha256"], found[1]["behaviour"]["contract_sha256"],
        "one request contract"
    );
    assert_ne!(found[0]["candidate_sha256"], found[1]["candidate_sha256"]);
    assert_eq!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    // The READY candidate is the reconnected graph: its count reads the filter.
    let candidate = out.candidate.as_deref().unwrap();
    assert!(candidate.contains("tasks.filter.output"), "{candidate}");
    assert_eq!(
        found[1]["candidate_sha256"],
        nika_compile_surface_sha256(candidate),
        "the evidence names the READY bytes"
    );
    let asked = author.asked.lock().unwrap().clone();
    assert_eq!(
        asked,
        ["sketch", "fills", "sketch", "fills", "judge"],
        "{asked:?}"
    );
    let usage = &out.provenance.decision.as_ref().unwrap()["rehearsal"]["usage"];
    assert_eq!(
        usage["fixtures"], 2,
        "one journal, each run charged once: {usage}"
    );
}

#[tokio::test]
async fn a_decoy_repair_holding_the_filter_text_still_counts_raw_rows_and_is_never_ready() {
    let world = World::new();
    let before = world.files();
    let author = Author {
        sketches: Mutex::new(vec![sketch("read_input")]),
        fills: Mutex::new(vec![fills(RAW), fills(DECOY)]),
        asked: Mutex::new(Vec::new()),
    };
    let out = compiled(&world, &author).await;
    assert_eq!(world.files(), before, "the project is unchanged");
    let counts = counts_written(&out);
    assert!(counts.len() >= 2, "the decoy ran: {counts:?}");
    assert!(counts.iter().all(|count| *count == Some(2)), "{counts:?}");
    assert!(
        evidence(&out)
            .iter()
            .all(|entry| entry["outcome"] == "defect"),
        "{:#?}",
        evidence(&out)
    );
    assert_ne!(out.status, CompileStatus::Ready, "{:#?}", out.diagnostics);
    assert!(out.candidate.is_none());
}

/// The candidate digest the compile records (the same sha256 over the exact bytes).
fn nika_compile_surface_sha256(text: &str) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(text.as_bytes());
    digest.iter().fold(String::new(), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

// ─── the Session journey, on this one binary ───────────────────────────────────────────────
//
// The plain session (a terminal, `NIKA_TUI=0`) with an isolated home and a minimal environment;
// its seat is a SCRIPTED loopback endpoint chosen as the local seat (no model). The request is
// phrased so the deterministic reader cannot carry it and the sketch door authors it. The
// production room behind the session gets this binary as its jq helper.

/// A phrasing the deterministic reader leaves to the sketch door, whose count the behavioural
/// judge reads from the request (the canonical phrasing alone is settled by the reader, with no
/// model, and writes the right count).
const SESSION_INTENT: &str = "read ./in/input.json, count the rows where status is paid, write the count to ./out/result.json as a small JSON object";

/// A phrasing the sketch door authors but the behavioural judge cannot read a count from.
const UNJUDGED_INTENT: &str = "tell me how many entries of ./in/input.json are marked paid and store that number in ./out/result.json";

/// A scripted loopback seat: an OpenAI-compatible endpoint answering each chat completion by the
/// schema it asks, from its queues; it records what each call asked.
struct LoopbackSeat {
    port: u16,
    asked: std::sync::Arc<Mutex<Vec<&'static str>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
}

impl LoopbackSeat {
    // The synchronous PTY fixture owns this blocking socket thread and joins it on drop.
    #[allow(clippy::disallowed_methods)]
    fn start(sketches: Vec<String>, fills: Vec<String>) -> Self {
        use std::sync::atomic::{AtomicBool, Ordering};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let asked = std::sync::Arc::new(Mutex::new(Vec::new()));
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let (seen, stopped) = (asked.clone(), stop.clone());
        let author = Author {
            sketches: Mutex::new(sketches),
            fills: Mutex::new(fills),
            asked: Mutex::new(Vec::new()),
        };
        let server = std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => answer(stream, &author, &seen),
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        Self {
            port,
            asked,
            stop,
            server: Some(server),
        }
    }

    fn asked(&self) -> Vec<&'static str> {
        self.asked.lock().unwrap().clone()
    }
}

impl Drop for LoopbackSeat {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

/// Serve one HTTP request: a model listing for a GET, a chat completion for a POST.
fn answer(mut stream: std::net::TcpStream, author: &Author, seen: &Mutex<Vec<&'static str>>) {
    use std::io::{Read, Write};
    stream.set_nonblocking(false).unwrap();
    let mut raw = Vec::new();
    let mut chunk = [0_u8; 8192];
    let (head_end, length) = loop {
        let read = stream.read(&mut chunk).unwrap_or(0);
        if read == 0 {
            return;
        }
        raw.extend_from_slice(&chunk[..read]);
        if let Some(end) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..end]).to_lowercase();
            let length = (head.lines())
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            break (end + 4, length);
        }
    };
    while raw.len() < head_end + length {
        let read = stream.read(&mut chunk).unwrap_or(0);
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
    }
    let body = if raw.starts_with(b"GET") {
        json!({"models": [{"name": "llama3.2:latest", "model": "llama3.2:latest"}],
               "data": [{"id": "llama3.2", "object": "model"}]})
    } else {
        let request: Value = serde_json::from_slice(&raw[head_end..]).unwrap_or(Value::Null);
        let format = &request["response_format"];
        let schema = if format["json_schema"]["schema"].is_object() {
            &format["json_schema"]["schema"]
        } else {
            &format["schema"]
        };
        let properties = &schema["properties"];
        let (kind, text) = if let Some(keys) = properties["choice"]["enum"].as_array() {
            let approve = ["faithful", "carried"]
                .into_iter()
                .find(|key| keys.iter().any(|value| value == *key))
                .unwrap_or("none");
            ("judge", json!({"choice": approve}).to_string())
        } else if properties.get("fills").is_some() {
            ("fills", next(&author.fills))
        } else if properties.get("tasks").is_some() {
            ("sketch", next(&author.sketches))
        } else {
            ("other", "{}".to_owned())
        };
        seen.lock().unwrap().push(kind);
        json!({"id": "scripted", "object": "chat.completion", "model": request["model"],
               "choices": [{"index": 0, "finish_reason": "stop",
                            "message": {"role": "assistant", "content": text}}],
               "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}})
    };
    let payload = body.to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

type LoggedSession = expectrl::session::Session<
    expectrl::process::unix::UnixProcess,
    expectrl::stream::log::LogStream<expectrl::process::unix::PtyStream, std::io::Stderr>,
>;

/// The plain session over `world`, its seat chosen: the scripted loopback, as the local seat.
fn session(world: &World, home: &Path, seat: &LoopbackSeat) -> LoggedSession {
    use expectrl::Expect as _;
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_nika"));
    command
        .current_dir(world.project())
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("TERM", "xterm-256color")
        .env("LANG", "en_US.UTF-8")
        .env("NIKA_TUI", "0")
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_FILE", home.join("absent-run-key"))
        .env("NIKA_RUN_PUB_FILE", home.join("absent-run-pub"))
        .env(
            "NIKA_OLLAMA_BASE_URL",
            format!("http://127.0.0.1:{}/v1", seat.port),
        )
        .env("NIKA_AUTHORING_STRATEGY", "sketch");
    let raw = expectrl::session::OsSession::spawn(command).unwrap();
    let mut session = expectrl::session::log(raw, std::io::stderr()).unwrap();
    session.set_expect_timeout(Some(Duration::from_secs(90)));
    session.expect("nika ›").unwrap();
    session.send_line("/intelligence").unwrap();
    session.expect("No AI in this conversation").unwrap();
    session.send_line("3 ollama/llama3.2").unwrap();
    session.expect("authoring · ollama/llama3.2").unwrap();
    session.expect("nika ›").unwrap();
    session
}

/// The text the session printed up to `needle`.
fn until(session: &mut LoggedSession, needle: &str) -> String {
    use expectrl::Expect as _;
    let found = session.expect(needle).unwrap();
    String::from_utf8_lossy(found.before()).into_owned()
}

#[test]
fn in_session_a_wrong_count_is_repaired_before_it_is_proposed_and_the_run_writes_the_paid_count() {
    use expectrl::Expect as _;
    let world = World::new();
    let home = tempfile::tempdir().unwrap();
    let before = world.files();
    let seat = LoopbackSeat::start(
        vec![sketch("read_input"), sketch("filter")],
        vec![fills(RAW), fills(FILTERED)],
    );
    let mut s = session(&world, home.path(), &seat);
    s.send_line(SESSION_INTENT).unwrap();
    let proposal = until(&mut s, "apply? ›");
    // The disconnected graph was rehearsed and refused before any proposal: two sketches. The
    // proposal shows what the reconnected candidate wrote on a copy.
    assert_eq!(
        seat.asked(),
        ["sketch", "fills", "sketch", "fills", "judge"],
        "{proposal}"
    );
    assert!(
        proposal.contains("Rehearsed on a copy of your files"),
        "{proposal}"
    );
    assert!(
        proposal.contains(r#"excerpt "{\"count\":1}""#),
        "{proposal}"
    );
    assert_eq!(world.files(), before, "nothing written while proposing");
    s.send_line("yes").unwrap();
    until(&mut s, "Saved · checked");
    s.send_line("run it").unwrap();
    let ran = until(&mut s, "run observed · exit 0 · succeeded");
    s.send_line("/quit").unwrap();
    let _ = s.get_process_mut().wait();
    let written = std::fs::read_to_string(world.project().join("out/result.json")).unwrap();
    let count = serde_json::from_str::<Value>(&written).unwrap()["count"].as_u64();
    assert_eq!(count, Some(expected_count()), "{ran}");
    let input = std::fs::read(world.project().join("in/input.json")).unwrap();
    assert_eq!(input, ROWS.as_bytes(), "the source is unchanged");
    let saved = std::fs::read_to_string(world.project().join("paid-count.nika")).unwrap();
    assert!(saved.contains("tasks.filter.output"), "{saved}");
}

#[test]
fn in_session_a_decoy_holding_the_filter_text_is_never_proposed_saved_or_run() {
    use expectrl::Expect as _;
    let world = World::new();
    let home = tempfile::tempdir().unwrap();
    let before = world.files();
    let seat = LoopbackSeat::start(vec![sketch("read_input")], vec![fills(RAW), fills(DECOY)]);
    let mut s = session(&world, home.path(), &seat);
    s.send_line(SESSION_INTENT).unwrap();
    let answer = until(&mut s, "nika ›");
    s.send_line("/quit").unwrap();
    let _ = s.get_process_mut().wait();
    assert!(!answer.contains("apply this?"), "{answer}");
    assert!(answer.contains("nothing is READY"), "{answer}");
    let asked = seat.asked();
    assert!(
        asked.iter().filter(|kind| **kind == "fills").count() >= 2,
        "the decoy was authored: {asked:?}"
    );
    assert!(
        !world.project().join("out/result.json").exists(),
        "nothing ran"
    );
    assert!(
        !world.project().join("paid-count.nika").exists(),
        "nothing saved"
    );
    assert_eq!(world.files(), before, "the project is unchanged");
}

/// THE COVERAGE GAP, pinned: a phrasing the behavioural judge cannot read a count from. The room
/// runs the disconnected graph and the proposal shows the user its actual result (2), but no
/// defect is judged, so nothing is repaired and the candidate is proposed. It stays visible; it is
/// never a pass of the repair law.
#[test]
fn in_session_an_unjudged_phrasing_shows_the_wrong_result_it_cannot_repair() {
    use expectrl::Expect as _;
    let world = World::new();
    let home = tempfile::tempdir().unwrap();
    let seat = LoopbackSeat::start(
        vec![sketch("read_input"), sketch("filter")],
        vec![fills(RAW), fills(FILTERED)],
    );
    let mut s = session(&world, home.path(), &seat);
    s.send_line(UNJUDGED_INTENT).unwrap();
    let proposal = until(&mut s, "apply? ›");
    s.send_line("no").unwrap();
    until(&mut s, "nika ›");
    s.send_line("/quit").unwrap();
    let _ = s.get_process_mut().wait();
    assert_eq!(seat.asked(), ["sketch", "fills", "judge"], "{proposal}");
    assert!(
        proposal.contains(r#"excerpt "{\"count\":2}""#),
        "{proposal}"
    );
    assert!(
        !world.project().join("paid-count.nika").exists(),
        "declined: nothing saved"
    );
}

/// `raw` with escape sequences (CSI `ESC [ … final`, OSC `ESC ] … BEL|ST`, two-byte `ESC x`)
/// and every whitespace character removed.
fn squash(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            if !c.is_whitespace() {
                out.push(c);
            }
            continue;
        }
        match chars.next() {
            Some('[') => {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\x07' {
                        break;
                    }
                    if next == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether the renderer drew `phrase`, however its words were placed.
fn seen(raw: &str, phrase: &str) -> bool {
    squash(raw).contains(&squash(phrase))
}

/// The inline TUI over `world`, its local seat kept in `home`: the scripted loopback.
fn tui_session(world: &World, home: &Path, seat: &LoopbackSeat) -> LoggedSession {
    use expectrl::Expect as _;
    std::fs::create_dir_all(home.join(".nika")).unwrap();
    std::fs::write(
        home.join(".nika").join("session-intelligence.json"),
        r#"{"kind":{"kind":"local","provider":"ollama"},"model":"ollama/llama3.2","chosen_at":"2026-10-05T00:00:00Z"}"#,
    )
    .unwrap();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_nika"));
    command
        .current_dir(world.project())
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("TERM", "xterm-256color")
        .env("NO_COLOR", "1")
        .env("LANG", "en_US.UTF-8")
        .env("NIKA_TUI", "inline")
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_FILE", home.join("absent-run-key"))
        .env("NIKA_RUN_PUB_FILE", home.join("absent-run-pub"))
        .env(
            "NIKA_OLLAMA_BASE_URL",
            format!("http://127.0.0.1:{}/v1", seat.port),
        )
        .env("NIKA_AUTHORING_STRATEGY", "sketch");
    let raw = expectrl::session::OsSession::spawn(command).unwrap();
    let mut session = expectrl::session::log(raw, std::io::stderr()).unwrap();
    session.set_expect_timeout(Some(Duration::from_secs(120)));
    session.expect("\x1b[c").unwrap();
    session.send("\x1b[?62;22c").unwrap();
    session.expect("\x1b[6n").unwrap();
    session.send("\x1b[24;1R").unwrap();
    session.expect("automate?").unwrap();
    session.expect("nika ›").unwrap();
    session
}

#[test]
fn in_the_tui_a_wrong_count_is_repaired_before_it_is_proposed_and_the_run_writes_the_paid_count() {
    use expectrl::Expect as _;
    let world = World::new();
    let home = tempfile::tempdir().unwrap();
    let seat = LoopbackSeat::start(
        vec![sketch("read_input"), sketch("filter")],
        vec![fills(RAW), fills(FILTERED)],
    );
    let mut s = tui_session(&world, home.path(), &seat);
    s.send(format!("{SESSION_INTENT}\r")).unwrap();
    let proposal = until(&mut s, "apply?");
    // Semantic: the disconnected graph was rehearsed and refused before any proposal.
    assert_eq!(
        seat.asked(),
        ["sketch", "fills", "sketch", "fills", "judge"],
        "{}",
        squash(&proposal)
    );
    assert!(
        !world.project().join("out/result.json").exists(),
        "rehearsal leaves the real project untouched"
    );
    // The proposal also shows what the reconnected candidate wrote on a copy.
    assert!(
        seen(&proposal, r#"excerpt "{\"count\":1}""#),
        "{}",
        squash(&proposal)
    );
    s.send("yes\r").unwrap();
    until(&mut s, "paid-count.nika");
    assert!(
        !world.project().join("out/result.json").exists(),
        "Save does not execute the workflow"
    );
    s.send("run it\r").unwrap();
    until(&mut s, "observed");
    s.send("/quit\r").unwrap();
    let _ = s.expect(expectrl::Eof);
    // Semantic: the run wrote the paid count in the project, from the reconnected graph.
    let written = std::fs::read_to_string(world.project().join("out/result.json")).unwrap();
    let count = serde_json::from_str::<Value>(&written).unwrap()["count"].as_u64();
    assert_eq!(count, Some(expected_count()));
    let saved = std::fs::read_to_string(world.project().join("paid-count.nika")).unwrap();
    assert!(saved.contains("tasks.filter.output"), "{saved}");
    let input = std::fs::read(world.project().join("in/input.json")).unwrap();
    assert_eq!(input, ROWS.as_bytes(), "the source is unchanged");
}

#[test]
fn in_the_tui_a_decoy_holding_the_filter_text_is_never_proposed_saved_or_run() {
    use expectrl::Expect as _;
    let world = World::new();
    let home = tempfile::tempdir().unwrap();
    let before = world.files();
    let seat = LoopbackSeat::start(vec![sketch("read_input")], vec![fills(RAW), fills(DECOY)]);
    let mut s = tui_session(&world, home.path(), &seat);
    s.send(format!("{SESSION_INTENT}\r")).unwrap();
    let answer = until(&mut s, "READY");
    s.send("/quit\r").unwrap();
    let _ = s.expect(expectrl::Eof);
    // Semantic: the decoy was authored and repaired within the allowance, and nothing landed.
    let asked = seat.asked();
    assert!(
        asked.iter().filter(|kind| **kind == "fills").count() >= 2,
        "the decoy was authored: {asked:?}"
    );
    assert!(
        !asked.contains(&"judge"),
        "no READY candidate reached the judge: {asked:?}"
    );
    assert!(!seen(&answer, "apply this?"), "{}", squash(&answer));
    assert!(
        !world.project().join("paid-count.nika").exists(),
        "nothing saved"
    );
    assert!(
        !world.project().join("out/result.json").exists(),
        "nothing ran"
    );
    assert_eq!(world.files(), before, "the project is unchanged");
}

/// THE COVERAGE GAP, pinned in the TUI as in the plain session (D1): the observed wrong result is
/// shown and the candidate is still proposed.
#[test]
fn in_the_tui_an_unjudged_phrasing_shows_the_wrong_result_it_cannot_repair() {
    use expectrl::Expect as _;
    let world = World::new();
    let home = tempfile::tempdir().unwrap();
    let seat = LoopbackSeat::start(
        vec![sketch("read_input"), sketch("filter")],
        vec![fills(RAW), fills(FILTERED)],
    );
    let mut s = tui_session(&world, home.path(), &seat);
    s.send(format!("{UNJUDGED_INTENT}\r")).unwrap();
    let proposal = until(&mut s, "apply?");
    s.send("no\r").unwrap();
    until(&mut s, "nika ›");
    s.send("/quit\r").unwrap();
    let _ = s.expect(expectrl::Eof);
    // Semantic: one sketch, no repair; the candidate shown wrote 2 and was declined.
    assert_eq!(
        seat.asked(),
        ["sketch", "fills", "judge"],
        "{}",
        squash(&proposal)
    );
    assert!(
        seen(&proposal, r#"excerpt "{\"count\":2}""#),
        "{}",
        squash(&proposal)
    );
    assert!(
        !world.project().join("paid-count.nika").exists(),
        "declined: nothing saved"
    );
}
