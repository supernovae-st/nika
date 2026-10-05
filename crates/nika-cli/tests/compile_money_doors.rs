// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_types
)]

//! The B15 money law end to end (R4 · frozen EN/FR matrix): the real CLI compiles each request
//! over one fixed CSV, `nika check` judges the candidate, `nika run` executes it, and the rows it
//! writes are compared whole, every typed field, as an exact unordered multiset (FREEZE v1.3).
//! Money and business are judged apart: a money misreading always fails, a gated READY with
//! other rows always fails, and a case whose
//! operation the reader does not cover yet is recorded, never scored as money. A loopback seat
//! counts every accepted connection and every received request body apart; a positive control
//! proves it sees an attempt. The Session door runs its saved proposal through the same binary.
//! Hermetic: no key, no network beyond 127.0.0.1, no PTY.
use nika_session::money::MonetarySource;
use nika_session::turn::{RoutingMethod, TurnAct, TurnClassifier, TurnContext, TurnDecision};
use nika_session::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, SessionRuntime, TurnOutcome,
    UserIntelligencePreference,
};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;

/// The frozen B15 fixture (`./data/input.csv`), exact bytes: LF, a final newline.
const INPUT: &str = "id,cost,budget,price,plafond,montant,amount_usd\na,3,1500,10,3,120,100\nb,5,1000,15,5,250,250\nc,7,2000,20,3,300,300\nd,4,1500,14,10,80,251\ne,0,999,0,0,0,0\nf,12,1200,30,3,600,600\n";

/// Work the reader cannot settle alone (« harmonise the tone »): a named seat is asked.
const FREE: &str = "Review this customer request and harmonise the tone of the support reply";

/// The local seat a request names; its base URL is the loopback recorder's.
const SEAT: &str = "ollama/qwen3.5:4b";

/// Frozen case S08-EN: deterministic work under an explicit zero ceiling.
const ZERO: &str = "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json, budget 0 USD";

/// The business outcome a case must reach.
#[derive(Clone, Copy, Debug)]
enum Business {
    /// The reader covers the operation: READY with exactly these rows (sorted ids).
    Rows(&'static [&'static str]),
    /// Recorded, not gated: the reader may not cover the operation yet, or cover it with other
    /// rows; either is the reader's, recorded and reported, never a money verdict (FREEZE v1.2:
    /// S05-FR writes no row on the base binary already, a reader defect).
    Target(&'static [&'static str]),
    /// The stated money refuses the request before anything is read.
    Refused,
}

/// The frozen matrix at the CLI door: each request, the directive it states (its exact text),
/// and its business outcome. Interval semantics (S04): inclusive at both ends.
const MATRIX: [(&str, &str, Option<&str>, Business); 24] = [
    (
        "S01-EN",
        "read ./data/input.csv, keep the rows where cost is under 5 USD, write them to ./out/result.json",
        None,
        Business::Rows(&["a", "d", "e"]),
    ),
    (
        "S01-FR",
        "lis ./data/input.csv, garde les lignes où cost est inférieur à 5 USD, écris-les dans ./out/result.json",
        None,
        Business::Target(&["a", "d", "e"]),
    ),
    (
        "S02-EN",
        "read ./data/input.csv, keep the rows whose budget is 1500 USD, write them to ./out/result.json",
        None,
        Business::Rows(&["a", "d"]),
    ),
    (
        "S02-FR",
        "lis ./data/input.csv, garde les lignes dont le budget est 1500 USD, écris-les dans ./out/result.json",
        None,
        Business::Rows(&["a", "d"]),
    ),
    (
        "S03-EN",
        "read ./data/input.csv, keep the rows whose budget is over 1000 USD, write them to ./out/result.json",
        None,
        Business::Rows(&["a", "c", "d", "f"]),
    ),
    (
        "S03-FR",
        "lis ./data/input.csv, garde les lignes dont budget dépasse 1000 USD, écris-les dans ./out/result.json",
        None,
        Business::Rows(&["a", "c", "d", "f"]),
    ),
    (
        "S04-EN",
        "read ./data/input.csv, keep the rows whose budget is between 1000 USD and 1600 USD, write them to ./out/result.json",
        None,
        Business::Target(&["a", "b", "d", "f"]),
    ),
    (
        "S04-FR",
        "lis ./data/input.csv, garde les lignes dont le budget est entre 1000 USD et 1600 USD, écris-les dans ./out/result.json",
        None,
        Business::Target(&["a", "b", "d", "f"]),
    ),
    (
        "S05-EN",
        "read ./data/input.csv, keep the rows where cost is under $5, write them to ./out/result.json",
        None,
        Business::Rows(&["a", "d", "e"]),
    ),
    (
        "S05-FR",
        "lis ./data/input.csv, garde les lignes où plafond=3USD, écris-les dans ./out/result.json",
        None,
        Business::Target(&["a", "c", "f"]),
    ),
    (
        "S06-EN",
        "read ./data/input.csv, keep the rows where price is under 15 USD, write them to ./out/result.json",
        None,
        Business::Rows(&["a", "d", "e"]),
    ),
    (
        "S06-FR",
        "lis ./data/input.csv, garde les lignes dont le montant est supérieur à 100 dollars, écris-les dans ./out/result.json",
        None,
        Business::Rows(&["a", "b", "c", "f"]),
    ),
    (
        "S07-EN",
        "read ./data/input.csv, keep the rows with a budget of 1500 USD, write them to ./out/result.json",
        None,
        Business::Target(&["a", "d"]),
    ),
    (
        "S07-FR",
        "lis ./data/input.csv, garde les lignes avec un budget de 1500 USD, écris-les dans ./out/result.json",
        None,
        Business::Target(&["a", "d"]),
    ),
    (
        "S08-EN",
        ZERO,
        Some("budget 0 USD"),
        Business::Rows(&["c", "d", "f"]),
    ),
    (
        "S08-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json, plafond de 0 dollar",
        Some("plafond de 0 dollar"),
        Business::Rows(&["c", "d", "f"]),
    ),
    (
        "S09-EN",
        "read ./data/input.csv, keep the rows whose budget is 1500 USD, write them to ./out/result.json. Budget: 0 USD.",
        Some("Budget: 0 USD"),
        Business::Rows(&["a", "d"]),
    ),
    (
        "S09-FR",
        "lis ./data/input.csv, garde les lignes dont le budget est 1500 USD, écris-les dans ./out/result.json. Budget : 0 dollar.",
        Some("Budget : 0 dollar"),
        Business::Rows(&["a", "d"]),
    ),
    (
        "S10-EN",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json with a budget of 2 USD",
        Some("with a budget of 2 USD"),
        Business::Rows(&["c", "d", "f"]),
    ),
    (
        "S10-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json avec un plafond de 2 dollars",
        Some("avec un plafond de 2 dollars"),
        Business::Rows(&["c", "d", "f"]),
    ),
    (
        "S11-EN",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json. Budget: 1 USD. Cap: 2 USD.",
        None,
        Business::Refused,
    ),
    (
        "S11-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json. Budget : 1 dollar. Plafond : 2 dollars.",
        None,
        Business::Refused,
    ),
    (
        "S12-EN",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json. Budget: $abc.",
        None,
        Business::Refused,
    ),
    (
        "S12-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json avec un plafond de $NaN",
        None,
        Business::Refused,
    ),
];

fn command(room: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika"));
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", room)
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_FILE", room.join("absent-run.key"))
        .env("NIKA_RUN_PUB_FILE", room.join("absent-run.pub"))
        .env("NO_COLOR", "1")
        .current_dir(room)
        .stdin(Stdio::null());
    cmd
}

/// A fresh room holding the frozen fixture.
fn room() -> tempfile::TempDir {
    let room = tempfile::tempdir().expect("room");
    std::fs::create_dir_all(room.path().join("data")).expect("data");
    std::fs::write(room.path().join("data/input.csv"), INPUT).expect("fixture");
    room
}

fn document(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// `nika compile <request> workflow.nika --json` in the room, with any extra flags.
fn compile(room: &Path, request: &str, extra: &[&str]) -> Value {
    let out = command(room)
        .args(["compile", request, "workflow.nika", "--json"])
        .args(extra)
        .output()
        .expect("compile");
    document(&out)
}

/// The same compile with the local seat named, its base URL the recorder's.
fn seated(room: &Path, request: &str, recorder: &Recorder, extra: &[&str]) -> Value {
    let out = command(room)
        .env("NIKA_OLLAMA_BASE_URL", recorder.base())
        .args(["compile", request, "workflow.nika", "--json"])
        .args(["--authoring-model", SEAT, "--authoring-timeout", "5"])
        .args(extra)
        .output()
        .expect("compile");
    document(&out)
}

/// The directive texts an outcome records as stated money.
fn stated(doc: &Value) -> Vec<String> {
    doc["provenance"]["decision"]["money"]["directives"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["text"].as_str().map(str::to_owned))
        .collect()
}

/// The authoring requests the host counted before transport (absent when none was prepared).
fn prepared(doc: &Value) -> u64 {
    doc["provenance"]["authoring"]["backend"]["authority"]["http_requests"]["sent"]
        .as_u64()
        .unwrap_or(0)
}

fn says(doc: &Value, text: &str) -> bool {
    doc["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|d| d["message"].as_str().is_some_and(|m| m.contains(text)))
}

/// The frozen fixture's rows with these ids, as `nika:convert` types them (csv → json with a
/// header): each an object of the seven header keys, every value its cell's text as a JSON
/// string (FREEZE v1.3), sorted as an unordered multiset.
fn fixture_rows(ids: &[&str]) -> Vec<Value> {
    let mut lines = INPUT.lines();
    let header: Vec<&str> = lines.next().expect("header").split(',').collect();
    let mut rows: Vec<Value> = lines
        .map(|line| {
            Value::Object(
                header
                    .iter()
                    .zip(line.split(','))
                    .map(|(key, cell)| ((*key).to_owned(), Value::String(cell.to_owned())))
                    .collect(),
            )
        })
        .filter(|row| ids.contains(&row["id"].as_str().unwrap_or_default()))
        .collect();
    rows.sort_by_key(Value::to_string);
    rows
}

/// The ids of rows, for a diagnostic label only.
fn ids(rows: &[Value]) -> Vec<&str> {
    rows.iter()
        .map(|row| row["id"].as_str().unwrap_or_default())
        .collect()
}

/// `nika check`, then `nika run`, of `workflow`: every row it wrote, whole, sorted as an
/// unordered multiset.
fn executed_rows(room: &Path, workflow: &str) -> Result<Vec<Value>, String> {
    let check = command(room)
        .args(["check", workflow])
        .output()
        .expect("check");
    if !check.status.success() {
        return Err(format!(
            "check {:?}: {}",
            check.status.code(),
            String::from_utf8_lossy(&check.stdout)
        ));
    }
    let run = command(room).args(["run", workflow]).output().expect("run");
    if !run.status.success() {
        return Err(format!(
            "run {:?}: {} {}",
            run.status.code(),
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    let bytes = std::fs::read(room.join("out/result.json")).map_err(|e| e.to_string())?;
    let rows: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut rows = rows
        .as_array()
        .ok_or_else(|| format!("not a JSON array: {rows}"))?
        .clone();
    rows.sort_by_key(Value::to_string);
    Ok(rows)
}

/// One case against its frozen expectation: the failure, if any, and the case recorded as an
/// operation the reader does not cover yet.
fn judge(
    id: &str,
    request: &str,
    directive: Option<&str>,
    business: Business,
) -> (Option<String>, Option<String>) {
    let room = room();
    let doc = compile(room.path(), request, &[]);
    let status = doc["status"].as_str().unwrap_or_default().to_owned();
    let (rows, gated) = match business {
        Business::Refused if status == "refused" => return (None, None),
        Business::Refused => {
            return (
                Some(format!("{id}: {status}, expected a money refusal: {doc}")),
                None,
            );
        }
        Business::Rows(rows) => (rows, true),
        Business::Target(rows) => (rows, false),
    };
    let money = stated(&doc);
    if money.iter().map(String::as_str).ne(directive) {
        return (
            Some(format!(
                "{id}: stated money {money:?}, expected {directive:?}"
            )),
            None,
        );
    }
    if status != "ready" {
        return if gated {
            (Some(format!("{id}: {status}, expected READY: {doc}")), None)
        } else {
            (None, Some(format!("{id}: {status}")))
        };
    }
    let expected = fixture_rows(rows);
    match executed_rows(room.path(), "workflow.nika") {
        Ok(got) if got == expected => (None, None),
        // A case the reader is not gated on keeps its money verdict above; its other rows are
        // the reader's, recorded and reported (FREEZE v1.2), never scored as money.
        Ok(got) if !gated => (
            None,
            Some(format!(
                "{id}: WRONG_OUTPUT (reader) {:?}, expected {rows:?}",
                ids(&got)
            )),
        ),
        Ok(got) => (
            Some(format!("{id}: WRONG_OUTPUT {got:?}, expected {expected:?}")),
            None,
        ),
        Err(why) => (Some(format!("{id}: not executed: {why}")), None),
    }
}

#[test]
fn the_frozen_matrix_runs_its_business_and_states_only_its_money() {
    let mut wrong = Vec::new();
    let mut uncovered = Vec::new();
    for &(id, request, directive, business) in &MATRIX {
        let (failure, recorded) = judge(id, request, directive, business);
        wrong.extend(failure);
        uncovered.extend(recorded);
    }
    // The non-gated outcomes are reported, never passed: they stay open V9 obligations.
    #[allow(clippy::disallowed_macros, clippy::print_stderr)]
    {
        eprintln!(
            "recorded, never a money verdict (OPERATION_UNSUPPORTED, or the reader's WRONG_OUTPUT): {uncovered:#?}"
        );
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// A loopback seat that counts every accepted connection before reading it and every complete
/// request body it received, apart, and answers each with a 503. Dropping it stops and joins.
struct Recorder {
    port: u16,
    accepts: Arc<AtomicUsize>,
    bodies: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl Recorder {
    // The synchronous fixture owns this blocking socket thread and joins it on drop.
    #[allow(clippy::disallowed_methods)]
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback seat");
        let port = listener.local_addr().expect("seat address").port();
        let accepts = Arc::new(AtomicUsize::new(0));
        let bodies = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, received, halt) = (Arc::clone(&accepts), Arc::clone(&bodies), Arc::clone(&stop));
        let server = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                seen.fetch_add(1, Ordering::SeqCst);
                let Ok(mut stream) = stream else { continue };
                if complete_request(&mut stream) {
                    received.fetch_add(1, Ordering::SeqCst);
                }
                let _ = stream.write_all(
                    b"HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                );
            }
        });
        Self {
            port,
            accepts,
            bodies,
            stop,
            server: Some(server),
        }
    }

    fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Accepted connections and complete request bodies, apart.
    fn counts(&self) -> (usize, usize) {
        (
            self.accepts.load(Ordering::SeqCst),
            self.bodies.load(Ordering::SeqCst),
        )
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        // The flag ends the accept loop at its next connection; the wake connection is that one.
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

/// Whether one whole HTTP request arrived: its headers, then every byte its length declares.
fn complete_request(stream: &mut TcpStream) -> bool {
    if stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .is_err()
    {
        return false;
    }
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let Ok(n) = stream.read(&mut chunk) else {
            return false;
        };
        if n == 0 || buffer.len() > 4 << 20 {
            return false;
        }
        buffer.extend_from_slice(&chunk[..n]);
        let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&buffer[..end]).to_lowercase();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if buffer.len() >= end + 4 + length {
            return length > 0;
        }
    }
}

/// Positive control: free work with no money under the default authority is sent, and the
/// recorder sees the connection and the body the host counted before transport.
#[test]
fn the_recorder_sees_the_request_a_named_seat_is_sent() {
    let recorder = Recorder::start();
    let room = room();
    let doc = seated(room.path(), FREE, &recorder, &[]);
    let (accepts, bodies) = recorder.counts();
    assert!(accepts >= 1 && bodies >= 1, "{accepts}/{bodies}: {doc}");
    assert_eq!(prepared(&doc), 1, "{doc}");
}

/// F3: deterministic work under an explicit zero stays HOT with the seat named, and nothing is
/// prepared, connected or received.
#[test]
fn deterministic_work_under_a_stated_zero_stays_hot_with_nothing_sent() {
    let recorder = Recorder::start();
    let room = room();
    let doc = seated(room.path(), ZERO, &recorder, &[]);
    assert_eq!(recorder.counts(), (0, 0), "{doc}");
    assert_eq!(prepared(&doc), 0, "{doc}");
    assert_eq!(doc["status"], "ready", "{doc}");
    assert!(
        doc["provenance"]["decision"]["route"]
            .to_string()
            .contains("hot"),
        "{doc}"
    );
    assert_eq!(stated(&doc), ["budget 0 USD"], "{doc}");
    assert_eq!(
        executed_rows(room.path(), "workflow.nika"),
        Ok(fixture_rows(&["c", "d", "f"]))
    );
}

/// An explicit zero opens no seat for work HOT cannot settle: honestly incomplete, the reason
/// named, nothing prepared, connected or received.
#[test]
fn a_stated_zero_opens_no_seat_for_work_hot_cannot_settle() {
    let recorder = Recorder::start();
    let room = room();
    let doc = seated(
        room.path(),
        &format!("{FREE}, budget 0 USD"),
        &recorder,
        &[],
    );
    assert_eq!(recorder.counts(), (0, 0), "{doc}");
    assert_eq!(prepared(&doc), 0, "{doc}");
    assert_ne!(doc["status"], "ready", "{doc}");
    assert!(says(&doc, "no request was sent"), "{doc}");
}

/// A positive stated ceiling cannot bind an unpriced seat on this door: nothing is sent, and
/// the reason is named; naming a model grants no budget.
#[test]
fn a_stated_positive_ceiling_never_dispatches_an_unpriced_seat() {
    let recorder = Recorder::start();
    let room = room();
    let doc = seated(
        room.path(),
        &format!("{FREE}. Budget: 2 USD."),
        &recorder,
        &[],
    );
    assert_eq!(recorder.counts(), (0, 0), "{doc}");
    assert_eq!(prepared(&doc), 0, "{doc}");
    assert!(says(&doc, "no request was sent"), "{doc}");
    assert_eq!(stated(&doc), ["Budget: 2 USD"], "{doc}");
}

/// A skeleton's name beside a stated zero (primary review of 73291db3d, hypothesis 1): the law
/// reads the ceiling, the request is read as written and never as that skeleton, and nothing is
/// prepared, connected or received. On 73291db3d each of these sent one request.
#[test]
fn a_skeleton_name_beside_a_stated_zero_sends_nothing() {
    let recorder = Recorder::start();
    let room = room();
    for request in [
        "hello budget 0 USD",
        "01-hello budget 0 USD",
        "chain budget 0 USD",
    ] {
        let doc = seated(room.path(), request, &recorder, &[]);
        assert_eq!(recorder.counts(), (0, 0), "{request}: {doc}");
        assert_eq!(prepared(&doc), 0, "{request}: {doc}");
        assert_eq!(stated(&doc), ["budget 0 USD"], "{request}: {doc}");
        assert!(says(&doc, "no request was sent"), "{request}: {doc}");
        assert!(doc["provenance"]["skeleton"].is_null(), "{request}: {doc}");
    }
}

/// A readable base with its original request admits the typed source-revision reading.
const REVISION_INTENT: &str = "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json";
const CHANGE: &str = "write the result to ./out/revised.json instead";

/// A revision of the room's `workflow.nika` with the local seat named, its base URL the
/// recorder's.
fn revised(room: &Path, change: &str, recorder: &Recorder) -> Value {
    let out = command(room)
        .env("NIKA_OLLAMA_BASE_URL", recorder.base())
        .args([
            "compile",
            REVISION_INTENT,
            "--base",
            "workflow.nika",
            "--change",
            change,
        ])
        .args(["--output", "revised.nika", "--json"])
        .args(["--authoring-model", SEAT, "--authoring-timeout", "5"])
        .output()
        .expect("revise");
    document(&out)
}

/// A revision's change states its operator's money (hypothesis 2): read as money, never as the
/// change, no seat revises the base under it, and nothing is prepared, connected or received.
/// The same revision without it reaches the seat; on 73291db3d both sent one request.
#[test]
fn a_revision_stating_a_zero_sends_nothing() {
    let room = room();
    assert_eq!(
        compile(room.path(), REVISION_INTENT, &[])["status"],
        "ready"
    );
    let base = std::fs::read(room.path().join("workflow.nika")).expect("saved base");
    let control = Recorder::start();
    let doc = revised(room.path(), CHANGE, &control);
    let (accepts, bodies) = control.counts();
    assert!(accepts >= 1 && bodies >= 1, "{accepts}/{bodies}: {doc}");
    let recorder = Recorder::start();
    let doc = revised(room.path(), &format!("{CHANGE}, budget 0 USD"), &recorder);
    assert_eq!(recorder.counts(), (0, 0), "{doc}");
    assert_eq!(prepared(&doc), 0, "{doc}");
    assert_eq!(stated(&doc), ["budget 0 USD"], "{doc}");
    assert!(says(&doc, "no request was sent"), "{doc}");
    assert_eq!(
        std::fs::read(room.path().join("workflow.nika")).unwrap(),
        base
    );
    assert!(!room.path().join("revised.nika").exists());
}

/// The question keys an outcome asks, in order.
fn keys(doc: &Value) -> Vec<String> {
    doc["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|q| q["key"].as_str().map(str::to_owned))
        .collect()
}

/// A clarification replaces the request: its own directive is read afresh and its zero binds —
/// nothing is prepared, connected or received — and its money never changes the replacement's
/// business reading: the same outcome as the replacement without it (FREEZE v1.2: whether a
/// replacement reaches READY is its clarification grounding's, not its money's).
#[test]
fn a_replacement_request_states_its_own_money() {
    let replaced = |replacement: &str, recorder: &Recorder| {
        let room = room();
        let answer = format!(
            "intent.clarification={}",
            serde_json::to_string(replacement).unwrap()
        );
        seated(room.path(), FREE, recorder, &["--answer", &answer])
    };
    let recorder = Recorder::start();
    let doc = replaced(ZERO, &recorder);
    assert_eq!(recorder.counts(), (0, 0), "{doc}");
    assert_eq!(prepared(&doc), 0, "{doc}");
    assert_eq!(stated(&doc), ["budget 0 USD"], "{doc}");
    let plain = replaced(ZERO.trim_end_matches(", budget 0 USD"), &Recorder::start());
    assert_eq!(doc["status"], plain["status"], "{doc}\n{plain}");
    assert_eq!(keys(&doc), keys(&plain), "{doc}\n{plain}");
}

/// A routing double: the Session in these cases has no intelligence to ask.
struct Unknown;

impl TurnClassifier for Unknown {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        TurnDecision::new(TurnAct::Unknown, RoutingMethod::Fallback)
    }
}

/// The file a proposal creates, as its preview names it.
fn created(preview: &str) -> Option<String> {
    let (_, rest) = preview.split_once("creates `")?;
    rest.split_once('`').map(|(file, _)| file.to_owned())
}

/// T9: the Session door keeps the business rule; its saved workflow, run by the same binary,
/// writes exactly the rows the request asked for, and its ceiling is what the request stated.
#[test]
fn a_session_proposal_runs_the_business_rule_it_was_asked() {
    let cases: [(&str, &[&str], MonetarySource); 3] = [
        (
            "read ./data/input.csv, keep the rows where cost is under 5 USD, write them to ./out/result.json",
            &["a", "d", "e"],
            MonetarySource::SessionDefault,
        ),
        (
            "read ./data/input.csv, keep the rows whose budget is 1500 USD, write them to ./out/result.json",
            &["a", "d"],
            MonetarySource::SessionDefault,
        ),
        (ZERO, &["c", "d", "f"], MonetarySource::Explicit),
    ];
    for (request, rows, source) in cases {
        let room = room();
        let selected = ResolvedSessionIntelligence::resolve(
            &UserIntelligencePreference::new(IntelligenceKind::None, None),
            &IntelligenceCensus::empty(),
        );
        let mut session = SessionRuntime::open(
            room.path(),
            selected,
            Box::new(nika_session::ScriptedReasoner::new(Vec::new())),
        );
        session.with_classifier(Box::new(Unknown));
        let out = session.turn(request);
        let TurnOutcome::Proposal { preview, .. } = out else {
            panic!("{request}: a proposal, got {out:?}");
        };
        let file = created(&preview).unwrap_or_else(|| panic!("{request}: {preview}"));
        let held = session.monetary_decision().expect("money");
        assert_eq!(held.source, source, "{request}");
        let saved = session.consent("yes");
        assert!(
            matches!(saved, TurnOutcome::Facts(_)),
            "{request}: {saved:?}"
        );
        assert_eq!(
            executed_rows(room.path(), &file),
            Ok(fixture_rows(rows)),
            "{request}"
        );
    }
}
