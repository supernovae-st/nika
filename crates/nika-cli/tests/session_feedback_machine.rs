// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The guided loop after a first result (NIK-174/175), the real binary through its machine door
//! (`nika session --json`), over a fixed public DEV tech corpus the project keeps. The person's
//! local route is a loopback model scripted to call the Session's tools and to write each run's
//! summary. The digest the person saves and runs (`save & run`, the native lane) writes its first
//! report; the optional offer about it waits at the free prompt; two dimensions answered by its
//! identity reach the author, which asks the one precision missing, amends the workflow and
//! proposes it; the amended run writes a second report, and the neutral opinion answered by its
//! identity follows the adjustment. Each answer is bound to its request, revision, run and report;
//! none saves, runs or writes anything; a reopen keeps the answers and offers nothing again. Each
//! run asks its summary of the workflow's own model once, and the run's journal records the model
//! that answered. Keyless: an isolated HOME and an empty environment; the model is a double that
//! scripts what an author decides and what a summary says, never what Nika checks.
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

use std::io::{BufRead as _, BufReader, Lines, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

const REQUEST: &str = "fais moi un workflow tres simple qui recupere les news tech recentes, les resume et ecrit le resultat en markdown dans un dossier du projet";
/// The person's answer, a sentence the Session reads to pick the offer it names.
const ACCEPT: &str = "prends mes sources du projet";
/// The words of the request the output file is derived from.
const FOLDER: &str = "ecrit le resultat en markdown dans un dossier du projet";
const SOURCE: &str = "./news/sources.md";
const OUTPUT: &str = "./news/digest.md";
/// The fixed public DEV corpus the project keeps: every run reads it, nothing is fetched.
const CORPUS: &str = "# Tech\n- Rust 1.97 stabilise les closures async\n- Une faille critique corrigée dans OpenSSH\n- Un modèle d'IA ouvert rattrape les modèles fermés\n";
/// The kept choice of the local route this loopback serves.
const CHOICE: &str = r#"{"kind":{"kind":"local","provider":"vllm"},"model":"vllm/agent-seat","chosen_at":"2026-10-10T00:00:00Z"}"#;
/// The summary the first workflow asks for, then the amended one's.
const SUMMARY: &str = "Résume en Markdown les actualités ci-dessous, sans rien inventer :";
const DETAILED: &str = "Résume en détail, en privilégiant l'IA et la sécurité, les actualités ci-dessous, sans rien inventer :";
/// What the model answers each summary: the first run's report, then the amended run's.
const FIRST_REPORT: &str = "# Tech\n- Rust 1.97 stabilise les closures async\n";
const SECOND_REPORT: &str = "# Tech · IA et sécurité\n- Une faille critique corrigée dans OpenSSH\n- Un modèle d'IA ouvert rattrape les modèles fermés\n";
/// The precision the author asks once the person picked what to adjust, and the answer.
const PRECISION: &str = "Quels sujets dois-je privilégier ?";
const TOPICS: &str = "l'IA et la sécurité";
/// The line that marks the Session's reading of a reply.
const READING: &str = "One answer on one line, nothing else.";

/// The digest the author writes, summarizing with `prompt` in at most `max_tokens`: one read, one
/// summary by the route's own model, one write.
fn digest(prompt: &str, max_tokens: u32) -> String {
    format!(
        "nika: news-digest\nmodel: vllm/agent-seat\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"{SOURCE}\"]\n    write: [\"{OUTPUT}\"]\ntasks:\n  read_news:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"{SOURCE}\" }}\n  summarize:\n    with:\n      news: \"${{{{ tasks.read_news.output }}}}\"\n    infer:\n      max_tokens: {max_tokens}\n      prompt: \"{prompt} ${{{{ with.news }}}}\"\n  write_digest:\n    with:\n      digest: \"${{{{ tasks.summarize.output }}}}\"\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"{OUTPUT}\", content: \"${{{{ with.digest }}}}\" }}\n"
    )
}

/// What the scripted author does, one step per request that offers tools: the first digest, then,
/// once the picked dimensions reached it, the one precision and the amended digest. The person's
/// `save & run` is the native lane and asks the author nothing, so the request after the first
/// proposal is the one that carries the picked dimensions.
fn script() -> Vec<Value> {
    let offered = json!({"value": SOURCE, "kind": "offered", "role": "read_source",
        "message": "u2", "question": "sources", "option": "project"});
    let derived = json!({"value": OUTPUT, "kind": "derived", "role": "output_path",
        "message": "u1", "excerpt": FOLDER});
    let retained = |value: &str, role: &str, message: &str| json!({"value": value, "kind": "retained", "role": role, "message": message});
    vec![
        json!({"tool": "ask", "args": {"questions": [{
            "key": "sources", "question": "Quelles actualités tech dois-je résumer ?",
            "options": [
                {"key": "project", "label": "Tes sources : ./news/sources.md", "values": [
                    {"role": "read_source", "value": SOURCE, "name": "tes sources"}]},
                {"key": "other", "label": "D'autres sources"}],
            "free_text": true}]}}),
        json!({"tool": "candidate_write", "args": {"source": digest(SUMMARY, 1000),
            "summary": "Tes sources, résumé dans ./news/digest.md",
            "resolutions": [offered, derived]}}),
        json!({"tool": "propose", "args": {}}),
        json!({"tool": "ask", "args": {"questions": [
            {"key": "topics", "question": PRECISION, "free_text": true}]}}),
        json!({"tool": "candidate_write", "args": {"source": digest(DETAILED, 2000),
            "summary": "plus détaillé, consacré à l'IA et à la sécurité",
            "resolutions": [retained(SOURCE, "read_source", "u2"),
                retained(OUTPUT, "output_path", "u1")]}}),
        json!({"tool": "propose", "args": {}}),
        json!({"say": "Voici la révision : plus détaillée, consacrée à l'IA et à la sécurité."}),
    ]
}

/// A loopback model on the OpenAI-compatible wire the `vllm` override reaches: each request that
/// offers tools takes the next step of the script; every body is kept, in order.
struct Author {
    port: u16,
    bodies: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl Author {
    // The synchronous fixture owns this blocking socket thread and joins it on drop.
    #[allow(clippy::disallowed_methods)]
    fn start(script: Vec<Value>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback author");
        let port = listener.local_addr().expect("address").port();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, halt) = (Arc::clone(&bodies), Arc::clone(&stop));
        let server = std::thread::spawn(move || {
            let mut steps = script.into_iter();
            for (calls, stream) in listener.incoming().enumerate() {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let Some(body) = read_body(&mut stream) else {
                    continue;
                };
                let agent = body["tools"].as_array().is_some_and(|t| !t.is_empty());
                let plain = reply(&body);
                seen.lock().expect("author log").push(body);
                let scripted = if agent { steps.next() } else { None };
                let message = match scripted {
                    Some(call) if call.get("tool").is_some() => json!({
                        "role": "assistant", "content": null,
                        "tool_calls": [{"id": format!("call_{calls}"), "type": "function",
                            "function": {"name": call["tool"], "arguments": call["args"].to_string()}}]}),
                    Some(said) => json!({"role": "assistant", "content": said["say"]}),
                    None => plain,
                };
                answer(&mut stream, &message);
            }
        });
        Self {
            port,
            bodies,
            stop,
            server: Some(server),
        }
    }

    fn base(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    fn bodies(&self) -> Vec<Value> {
        self.bodies.lock().expect("author log").clone()
    }
}

impl Drop for Author {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

/// The options a verifier question offers when it approves.
const APPROVALS: [&str; 5] = [
    "faithful",
    "carried",
    "consistent",
    "only_requested",
    "no_task",
];

/// Every text a request carries, its messages' contents joined.
fn text(body: &Value) -> String {
    (body["messages"].as_array().into_iter().flatten())
        .filter_map(|message| message["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether `body` asks for one of the workflow's summaries: no tool, no closed choice, the
/// summary's own words.
fn summarizes(body: &Value, prompt: &str) -> bool {
    body["tools"].as_array().is_none_or(Vec::is_empty)
        && body["response_format"].is_null()
        && text(body).contains(prompt)
}

/// What the loopback answers a request that offers no tools: the verifier's closed choice
/// approved; the Session's reading of a reply, the project's sources where they are offered and
/// no offer otherwise; each summary its report; anything else a plain word.
fn reply(body: &Value) -> Value {
    let keys = &body["response_format"]["json_schema"]["schema"]["properties"]["choice"]["enum"];
    let offered = |key: &&str| keys.as_array().is_some_and(|k| k.iter().any(|k| k == *key));
    if let Some(approve) = APPROVALS.into_iter().find(offered) {
        let choice = json!({"choice": approve}).to_string();
        return json!({"role": "assistant", "content": choice});
    }
    let content = match text(body) {
        read if read.contains(READING) && read.contains("«project»") => "project",
        read if read.contains(READING) => "NONE",
        _ if summarizes(body, DETAILED) => SECOND_REPORT,
        _ if summarizes(body, SUMMARY) => FIRST_REPORT,
        _ => "D'accord.",
    };
    json!({"role": "assistant", "content": content})
}

/// One request's JSON body: the head up to its blank line, then `content-length` bytes.
fn read_body(stream: &mut TcpStream) -> Option<Value> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok()?;
    let (mut buffer, mut chunk) = (Vec::new(), [0_u8; 8192]);
    let end = loop {
        let n = stream.read(&mut chunk).ok().filter(|n| *n > 0)?;
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..end]).to_lowercase();
    let length: usize = (head.lines())
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    while buffer.len() < end + length {
        let n = stream.read(&mut chunk).ok().filter(|n| *n > 0)?;
        buffer.extend_from_slice(&chunk[..n]);
    }
    serde_json::from_slice(&buffer[end..end + length]).ok()
}

/// One chat completion carrying `message`, answered by the model the route names.
fn answer(stream: &mut TcpStream, message: &Value) {
    let finish = if message["tool_calls"].is_array() {
        "tool_calls"
    } else {
        "stop"
    };
    let body = json!({
        "id": "chatcmpl-agent", "object": "chat.completion", "model": "agent-seat",
        "choices": [{"index": 0, "message": message, "finish_reason": finish}],
        "usage": {"prompt_tokens": 1000, "completion_tokens": 100, "total_tokens": 1100},
    })
    .to_string();
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

/// `nika session --json` in `project`, keyless, its HOME keeping the loopback's local route as
/// the intelligence.
struct Door {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    /// Where the door's stderr goes, shown when it ends before an awaited frame.
    log: std::path::PathBuf,
}

impl Door {
    fn open(project: &Path, home: &Path, author: &Author) -> Self {
        std::fs::create_dir_all(home.join(".nika")).expect("home");
        std::fs::write(home.join(".nika/session-intelligence.json"), CHOICE).expect("choice");
        let log = home.join("session-stderr.log");
        let stderr = (std::fs::OpenOptions::new().create(true).append(true))
            .open(&log)
            .expect("stderr log");
        let mut child = Command::new(env!("CARGO_BIN_EXE_nika"))
            .args(["session", "--json"])
            .current_dir(project)
            .env_clear()
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin")
            .env("NIKA_KEYCHAIN", "off")
            .env("NIKA_VLLM_BASE_URL", author.base())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .spawn()
            .expect("nika session --json");
        let stdin = child.stdin.take().expect("stdin");
        let lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
        Self {
            child,
            stdin,
            lines,
            log,
        }
    }

    /// The next frame of one of `kinds`.
    fn next(&mut self, kinds: &[&str]) -> Value {
        loop {
            let Some(line) = self.lines.next() else {
                let said = std::fs::read_to_string(&self.log).unwrap_or_default();
                panic!("the door ended before a {kinds:?} frame; its stderr:\n{said}");
            };
            let line = line.expect("utf-8");
            let frame: Value = serde_json::from_str(&line).expect("one JSON object per line");
            if kinds.iter().any(|kind| frame["frame"] == *kind) {
                return frame;
            }
        }
    }

    /// `command` sent: its settled result, or the refusal answered at once.
    fn send(&mut self, command: &Value) -> Value {
        writeln!(self.stdin, "{command}").expect("command");
        self.next(&["result", "refused"])
    }

    /// `line` typed against `frame`'s snapshot.
    fn submit(&mut self, command: &str, frame: &Value, line: &str) -> Value {
        self.send(&json!({
            "contract": "nika/session-host@1", "op": "submit", "command": command,
            "snapshot": frame["snapshot"]["snapshot"], "line": line,
        }))
    }

    /// A reply bound to the question `question` of `frame`'s snapshot.
    fn answer(&mut self, command: &str, frame: &Value, question: &Value, line: &str) -> Value {
        self.send(&json!({
            "contract": "nika/session-host@1", "op": "answer", "command": command,
            "snapshot": frame["snapshot"]["snapshot"], "question": question, "line": line,
        }))
    }

    /// Stdin ends: the door closes and exits cleanly. Its frames are read through the `lines`
    /// field once `stdin` is dropped, as `next` reads them.
    fn close(mut self) {
        drop(self.stdin);
        loop {
            let Some(line) = self.lines.next() else {
                let said = std::fs::read_to_string(&self.log).unwrap_or_default();
                panic!("the door ended before its closed frame; its stderr:\n{said}");
            };
            let line = line.expect("utf-8");
            let frame: Value = serde_json::from_str(&line).expect("one JSON object per line");
            if frame["frame"] == "closed" {
                break;
            }
        }
        assert!(self.child.wait().expect("exit").success());
    }
}

/// The project as the person keeps it: the fixed corpus the digest reads.
fn project() -> tempfile::TempDir {
    let project = tempfile::tempdir().expect("project");
    std::fs::create_dir_all(project.path().join("news")).expect("news");
    std::fs::write(project.path().join(SOURCE), CORPUS).expect("corpus");
    project
}

fn read(root: &Path, path: &str) -> String {
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|why| panic!("{path}: {why}"))
}

/// The runs' journals under `root`, one per run.
fn journals(root: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(root.join(".nika/traces")) else {
        return Vec::new();
    };
    let mut found: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    found.sort();
    found
}

/// Every `model_served` value one run's journal records, at any depth of any frame. A frame keeps
/// its fields as `{"key": …, "value": …}` pairs, so the served model is the value of the pair
/// whose key names it.
fn served(journal: &Path) -> Vec<String> {
    fn collect(value: &Value, found: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                if map.get("key").and_then(Value::as_str) == Some("model_served")
                    && let Some(model) = map.get("value").and_then(Value::as_str)
                {
                    found.push(model.to_owned());
                }
                for inner in map.values() {
                    collect(inner, found);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| collect(item, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    let mut stack = vec![journal.to_path_buf()];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            let entries = std::fs::read_dir(&path).expect("journal dir");
            stack.extend(entries.flatten().map(|entry| entry.path()));
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for frame in text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
        {
            collect(&frame, &mut found);
        }
    }
    found
}

fn sha256(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// The Session's witness of `text`'s bytes, as it witnesses a project file it reads again.
fn witness(text: &str) -> String {
    nika_session::change::Witness::of(text.as_bytes()).0
}

fn offer(frame: &Value) -> &Value {
    &frame["snapshot"]["work"]["feedback"]["offer"]
}

fn recorded(frame: &Value) -> Vec<Value> {
    (frame["snapshot"]["work"]["feedback"]["recorded"]
        .as_array()
        .cloned())
    .unwrap_or_default()
}

/// The keys of the offer's options that `keep` keeps, in order.
fn keys(offered: &Value, keep: fn(&Value) -> bool) -> Vec<String> {
    (offered["question"]["options"]
        .as_array()
        .into_iter()
        .flatten())
    .filter(|option| keep(option))
    .filter_map(|option| option["key"].as_str().map(str::to_owned))
    .collect()
}

/// Whether the first request that led the conversation after the `from` first requests carried
/// `words`: what an answer told the author, before any later turn could repeat it.
fn told(author: &Author, from: usize, words: &str) -> bool {
    (author.bodies().iter().skip(from))
        .find(|body| body["tools"].as_array().is_some_and(|t| !t.is_empty()))
        .is_some_and(|body| text(body).contains(words))
}

/// The run `ran` settled once through the native lane: requested, observed exit 0, its report
/// written exactly; the bytes it ran are `source`, its summary asked once of the workflow's own
/// model after `before` requests.
fn assert_ran(
    ran: &Value,
    (root, workflow): (&Path, &str),
    source: &str,
    (author, before): (&Author, usize),
) {
    let kinds: Vec<&str> = (ran["outcomes"].as_array().into_iter().flatten())
        .filter_map(|outcome| outcome["kind"].as_str())
        .collect();
    assert_eq!(kinds, ["run_requested", "facts"], "{ran}");
    let observed = ran["outcomes"][1]["text"].as_str().expect("observation");
    assert!(observed.contains("run observed · exit 0"), "{observed}");
    assert_eq!(read(root, workflow), source, "the bytes shown ran");
    let report = if source.contains(DETAILED) {
        SECOND_REPORT
    } else {
        FIRST_REPORT
    };
    assert_eq!(
        read(root, OUTPUT),
        report,
        "the run wrote the summary its model answered"
    );
    let prompt = if source.contains(DETAILED) {
        DETAILED
    } else {
        SUMMARY
    };
    let asked: Vec<Value> = (author.bodies().into_iter().skip(before))
        .filter(|body| summarizes(body, prompt))
        .collect();
    assert_eq!(asked.len(), 1, "one summary per run: {asked:?}");
    assert_eq!(
        asked[0]["model"], "agent-seat",
        "the workflow's own model was asked"
    );
    assert_eq!(ran["snapshot"]["work"]["waiting"]["kind"], "free", "{ran}");
}

/// The first result's offer: what to adjust, from the digest's own tasks, « Nothing to change »
/// and « Later » alone, following nothing, bound to the bytes run and the report written. Its
/// scope.
fn first_offer(ran: &Value) -> Value {
    let offered = offer(ran);
    assert_eq!(offered["kind"], "adjust", "{ran}");
    assert_eq!(
        keys(offered, |_| true),
        [
            "depth",
            "length",
            "topics",
            "sources",
            "presentation",
            "nothing",
            "later"
        ]
    );
    assert_eq!(
        keys(offered, |o| o["exclusive"] == true),
        ["nothing", "later"]
    );
    assert_eq!(offered["question"]["multi_select"], true);
    assert!(
        offered["follows"].is_null(),
        "a first result follows nothing"
    );
    let scope = &offered["scope"];
    assert_eq!(
        scope["source_sha256"],
        sha256(&digest(SUMMARY, 1000)),
        "the bytes run"
    );
    assert_eq!(
        scope["artifacts"],
        json!([{"path": OUTPUT, "witness": witness(FIRST_REPORT)}])
    );
    assert!(
        scope["execution"].as_str().is_some_and(|e| !e.is_empty()),
        "{scope}"
    );
    scope.clone()
}

/// The corrected result's offer: the neutral opinion, nothing recommended, following the
/// `adjustment` record, a later revision of the same request than `scope`, another run, bound to
/// the amended bytes and the second report. Its scope.
fn opinion_offer(ran: &Value, scope: &Value, adjustment: &Value) -> Value {
    let offered = offer(ran);
    assert_eq!(offered["kind"], "opinion", "{ran}");
    assert_eq!(
        keys(offered, |_| true),
        ["suitable", "changes", "wrong", "unevaluated", "later"]
    );
    assert!(
        keys(offered, |o| o["recommended"] == true).is_empty(),
        "{offered}"
    );
    assert_eq!(
        offered["follows"], adjustment["question"],
        "the adjustment it amends"
    );
    let amended = &offered["scope"];
    assert_eq!(amended["request"], scope["request"], "the same request");
    assert!(
        amended["revision"].as_u64() > scope["revision"].as_u64(),
        "a later revision: {amended}"
    );
    assert_eq!(
        amended["source_sha256"],
        sha256(&digest(DETAILED, 2000)),
        "the amended bytes"
    );
    let first = scope["execution"].as_str().expect("the first run");
    assert_ne!(amended["execution"], first, "another run");
    assert_eq!(
        amended["artifacts"],
        json!([{"path": OUTPUT, "witness": witness(SECOND_REPORT)}])
    );
    amended.clone()
}

/// The answers come back on a reopen as evidence, never as a live offer, and the reopen writes,
/// runs and asks nothing: no model call between its opening and its close.
fn assert_reopened(root: &Path, home: &Path, author: &Author, records: &[Value]) {
    let kept = (read(root, OUTPUT), journals(root));
    let calls = author.bodies().len();
    let mut door = Door::open(root, home, author);
    let opened = door.next(&["opened"]);
    assert!(
        offer(&opened).is_null(),
        "no live offer comes back: {opened}"
    );
    assert_eq!(
        recorded(&opened),
        records,
        "the answers come back: {opened}"
    );
    door.close();
    assert_eq!(
        (read(root, OUTPUT), journals(root)),
        kept,
        "a reopen writes and runs nothing"
    );
    assert_eq!(author.bodies().len(), calls, "a reopen calls no model");
}

/// The first result: the request, the sources picked, the digest proposed, then saved and run on
/// the person's `save & run`. The run's frame and the workflow's path.
fn first_result(door: &mut Door, author: &Author, root: &Path) -> (Value, String) {
    let opened = door.next(&["opened"]);
    let asked = door.submit("c-1", &opened, REQUEST);
    assert_eq!(asked["outcomes"][0]["kind"], "question", "{asked}");
    let shown = door.submit("c-2", &asked, ACCEPT);
    assert_eq!(shown["outcomes"][0]["kind"], "proposal", "{shown}");
    let file = &shown["snapshot"]["work"]["candidate"]["files"][0]["path"];
    let workflow = file.as_str().expect("the workflow").to_owned();
    let before = author.bodies().len();
    let result = door.submit("c-3", &shown, "save & run");
    let ran = (root, workflow.as_str());
    assert_ran(&result, ran, &digest(SUMMARY, 1000), (author, before));
    (result, workflow)
}

/// Two dimensions picked by the offer's identity, then the one precision the author asks: the
/// adjustment recorded with the result's exact `scope`, the dimensions' labels and then the
/// precision told to the author as the person gave them, nothing run or written. The amended
/// proposal's frame.
fn guided_correction(
    door: &mut Door,
    author: &Author,
    (root, result, scope): (&Path, &Value, &Value),
) -> Value {
    let runs = journals(root);
    let id = offer(result)["question"]["id"].clone();
    let from = author.bodies().len();
    let adjusted = door.answer("c-4", result, &id, r#"["topics","depth"]"#);
    assert_eq!(adjusted["op"], "answer", "{adjusted}");
    assert_eq!(
        adjusted["outcomes"][0]["kind"], "question",
        "the precision: {adjusted}"
    );
    assert!(offer(&adjusted).is_null(), "answered: {adjusted}");
    let records = recorded(&adjusted);
    assert_eq!(records.len(), 1, "{adjusted}");
    assert_eq!(records[0]["kind"], "adjust");
    assert_eq!(
        records[0]["picked"],
        json!(["depth", "topics"]),
        "in the offer's order"
    );
    assert_eq!(
        &records[0]["scope"], scope,
        "bound to the offer's exact result"
    );
    let labels = "chose to adjust: Depth of the summary, Topics covered";
    assert!(
        told(author, from, labels),
        "the dimensions reach the author by their labels"
    );
    let precision = adjusted["snapshot"]["work"]["waiting"]["id"].clone();
    let from = author.bodies().len();
    let revised = door.answer("c-5", &adjusted, &precision, TOPICS);
    assert_eq!(revised["outcomes"][0]["kind"], "proposal", "{revised}");
    assert!(
        told(author, from, TOPICS),
        "the precision reaches the author as typed"
    );
    assert_eq!(journals(root), runs, "an answer runs nothing");
    assert_eq!(read(root, OUTPUT), FIRST_REPORT, "nor writes");
    revised
}

/// The whole first witness: the result, the offer, two dimensions, one precision, the amended run
/// on the person's `save & run`, the opinion bound to the adjustment, the reopen; each answer bound
/// to its request, revision, run and report, none an effect; each run's journal records that the
/// workflow's own model answered.
#[test]
fn a_result_then_a_guided_correction_then_a_neutral_opinion_through_the_machine_door() {
    let project = project();
    let (root, home) = (project.path(), tempfile::tempdir().expect("home"));
    let author = Author::start(script());
    let mut door = Door::open(root, home.path(), &author);
    let (result, workflow) = first_result(&mut door, &author, root);
    let scope = first_offer(&result);
    assert_eq!(journals(root).len(), 1, "one run");
    let revised = guided_correction(&mut door, &author, (root, &result, &scope));
    let first_bytes = digest(SUMMARY, 1000);
    assert_eq!(
        read(root, &workflow),
        first_bytes,
        "the revision waits for its consent"
    );
    let adjustment = recorded(&revised)[0].clone();

    let before = author.bodies().len();
    let rerun = door.submit("c-6", &revised, "save & run");
    let ran = (root, workflow.as_str());
    assert_ran(&rerun, ran, &digest(DETAILED, 2000), (&author, before));
    let corrected = opinion_offer(&rerun, &scope, &adjustment);
    let runs = journals(root);
    assert_eq!(runs.len(), 2, "the amended run: {runs:?}");

    let id = offer(&rerun)["question"]["id"].clone();
    let noted = door.answer("c-7", &rerun, &id, "suitable");
    assert_eq!(noted["op"], "answer", "{noted}");
    assert!(offer(&noted).is_null(), "answered: {noted}");
    let records = recorded(&noted);
    assert_eq!(records.len(), 2, "{noted}");
    assert_eq!(records[0], adjustment, "the adjustment kept as recorded");
    assert_eq!(records[1]["kind"], "opinion");
    assert_eq!(records[1]["picked"], json!(["suitable"]));
    assert_eq!(
        records[1]["scope"], corrected,
        "bound to the corrected result"
    );
    assert_eq!(
        records[1]["follows"], adjustment["question"],
        "it follows the adjustment"
    );
    assert_eq!(journals(root), runs, "an opinion runs nothing");
    assert_eq!(read(root, OUTPUT), SECOND_REPORT, "nor writes");
    let readings = (author.bodies().iter())
        .filter(|body| text(body).contains(READING))
        .count();
    assert_eq!(
        readings, 2,
        "the sources and the precision were read; never an answer"
    );
    door.close();

    assert_reopened(root, home.path(), &author, &records);
    for journal in &runs {
        assert_eq!(
            served(journal),
            ["agent-seat"],
            "the model that answered: {journal:?}"
        );
    }
}

// ─── an asked question the card covers, or not, through the machine door ───

/// The amended summary the live author first proposed, covering every topic: the default its card
/// shows for `topics` (status 65).
const DETAILED_ALL: &str = "Résume en détail, en couvrant tous les sujets présents, les actualités ci-dessous, sans rien inventer :";
/// The value that default binds, as the candidate carries it.
const ALL_TOPICS: &str = "tous les sujets présents";

fn option(key: &str, label: &str) -> Value {
    json!({"key": key, "label": label})
}

/// The topics question as the live author asked it, and as its card showed it: `card` recommends
/// `all` with the value the candidate carries.
fn topics(card: bool) -> Value {
    let mut all = option("all", "Tous les sujets présents dans sources.md");
    if card {
        all["recommended"] = json!(true);
        all["values"] = json!([{"role": "value", "value": ALL_TOPICS}]);
    }
    json!({"key": "topics", "question": "Quels sujets le digest doit-il couvrir ?",
        "role": "value", "free_text": true, "multi_select": true,
        "options": [all, option("dev", "Développement et langages"),
            option("security", "Sécurité"), option("ai", "Intelligence artificielle")]})
}

/// The live author's correction (status 65): after the first result, depth and topics asked
/// together; once depth is answered, a proposal covering every topic, its card showing topics when
/// `card`; then the focused revision.
fn live_script(card: bool) -> Vec<Value> {
    let mut steps = script();
    steps.truncate(3);
    let retained = |value: &str, role: &str, message: &str| json!({"value": value, "kind": "retained", "role": role, "message": message});
    let kept = json!([
        retained(SOURCE, "read_source", "u2"),
        retained(OUTPUT, "output_path", "u1")
    ]);
    let open = if card {
        json!({"open": [topics(true)]})
    } else {
        json!({})
    };
    steps.extend([
        json!({"tool": "ask", "args": {"questions": [
            {"key": "depth", "question": "Quelle profondeur pour le résumé ?", "role": "value",
             "free_text": true, "options": [option("headlines", "Très court : une ligne par news"),
                option("short", "Court : 2-3 phrases par news"),
                option("detailed", "Détaillé : un paragraphe par news avec contexte et enjeux")]},
            topics(false)]}}),
        json!({"tool": "candidate_write", "args": {"source": digest(DETAILED_ALL, 2000),
            "summary": "plus détaillé, tous les sujets couverts", "resolutions": kept}}),
        json!({"tool": "propose", "args": open}),
        json!({"tool": "candidate_write", "args": {"source": digest(DETAILED, 2000),
            "summary": "plus détaillé, consacré à l'IA et à la sécurité", "resolutions": kept}}),
        json!({"tool": "propose", "args": {}}),
        json!({"say": "Voici la révision : plus détaillée, consacrée à l'IA et à la sécurité."}),
    ]);
    steps
}

/// The asked question `key` of `frame`'s snapshot.
fn asked<'a>(frame: &'a Value, key: &str) -> Option<&'a Value> {
    (frame["snapshot"]["work"]["questions"]
        .as_array()
        .into_iter()
        .flatten())
    .find(|question| question["key"] == key)
}

/// The first result run, then depth and topics picked on its offer, the live author asking both,
/// and depth answered by its identity: the frame the author's proposal settles, and the topics
/// question as it was asked.
fn depth_answered(door: &mut Door, author: &Author, root: &Path) -> (Value, Value) {
    let (result, _) = first_result(door, author, root);
    let id = offer(&result)["question"]["id"].clone();
    let picked = door.answer("c-4", &result, &id, r#"["depth","topics"]"#);
    assert_eq!(
        picked["snapshot"]["work"]["waiting"]["kind"], "questions",
        "{picked}"
    );
    let depth = asked(&picked, "depth").expect("depth asked")["id"].clone();
    let question = asked(&picked, "topics").expect("topics asked").clone();
    let proposed = door.answer("c-5", &picked, &depth, "detailed");
    assert_eq!(proposed["outcomes"][0]["kind"], "proposal", "{proposed}");
    (proposed, question)
}

/// The live shape (status 65): the author asked topics, then proposed with topics shown as the
/// card's choice of the same answer domain. One authoritative choice answers it: the asked question
/// is that choice, its old identity is stale, the person's other option revises, and the revision
/// saved is exactly the bytes that run.
#[test]
fn a_card_choice_covering_the_asked_topics_is_the_one_answered_through_the_machine_door() {
    let project = project();
    let (root, home) = (project.path(), tempfile::tempdir().expect("home"));
    let author = Author::start(live_script(true));
    let mut door = Door::open(root, home.path(), &author);
    let (carded, question) = depth_answered(&mut door, &author, root);
    let work = &carded["snapshot"]["work"];
    assert_eq!(work["waiting"]["kind"], "consent", "{carded}");
    let choice = &work["open_choices"][0];
    assert_eq!(choice["key"], "topics", "{carded}");
    assert!(
        asked(&carded, "topics").is_none(),
        "the card's choice covers the asked question: {carded}"
    );
    // The asked question's own identity answers nothing any more.
    let stale = door.answer("c-6", &carded, &question["id"], r#"["security","ai"]"#);
    assert_eq!(stale["frame"], "refused", "{stale}");
    assert_eq!(stale["error"], "unknown_question", "{stale}");
    let revised = door.answer("c-7", &carded, &choice["id"], r#"["security","ai"]"#);
    assert_eq!(revised["outcomes"][0]["kind"], "proposal", "{revised}");
    let work = &revised["snapshot"]["work"];
    assert_eq!(work["waiting"]["kind"], "consent", "{revised}");
    assert!(work["open_choices"].is_null(), "{revised}");
    assert!(asked(&revised, "topics").is_none(), "{revised}");
    let workflow = work["candidate"]["files"][0]["path"]
        .as_str()
        .expect("the workflow")
        .to_owned();
    let before = author.bodies().len();
    let rerun = door.submit("c-8", &revised, "save & run");
    let ran = (root, workflow.as_str());
    assert_ran(&rerun, ran, &digest(DETAILED, 2000), (&author, before));
    door.close();
}

/// The same correction when the card shows no topics choice: the asked question waits first, under
/// its own identity, and its answer is taken; the proposal shown before it is not saved as it is,
/// and the revision the author proposes is exactly the bytes that run.
#[test]
fn an_asked_question_no_card_covers_waits_first_and_its_identity_answers_it() {
    let project = project();
    let (root, home) = (project.path(), tempfile::tempdir().expect("home"));
    let author = Author::start(live_script(false));
    let mut door = Door::open(root, home.path(), &author);
    let (proposed, question) = depth_answered(&mut door, &author, root);
    let waiting = &proposed["snapshot"]["work"]["waiting"];
    assert_eq!(
        waiting["kind"], "question",
        "the uncovered question waits: {proposed}"
    );
    assert_eq!(waiting["id"], question["id"], "{proposed}");
    let revised = door.answer("c-6", &proposed, &question["id"], r#"["security","ai"]"#);
    assert_eq!(
        revised["frame"], "result",
        "its identity answers it: {revised}"
    );
    assert_eq!(revised["outcomes"][0]["kind"], "proposal", "{revised}");
    let work = &revised["snapshot"]["work"];
    assert_eq!(work["waiting"]["kind"], "consent", "{revised}");
    assert!(asked(&revised, "topics").is_none(), "{revised}");
    let workflow = work["candidate"]["files"][0]["path"]
        .as_str()
        .expect("the workflow")
        .to_owned();
    let before = author.bodies().len();
    let rerun = door.submit("c-7", &revised, "save & run");
    let ran = (root, workflow.as_str());
    assert_ran(&rerun, ran, &digest(DETAILED, 2000), (&author, before));
    door.close();
}
