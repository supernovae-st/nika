// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika session --json`, the real binary, led by the selected intelligence (ADR-153): the
//! person's kept local route is answered by a loopback model scripted to call the Session's tools.
//! It asks one question whose recommended option carries concrete values, writes the candidate
//! the accepted option binds, and proposes it; the snapshot types the question, the values'
//! provenance and the proposal, and the person's `yes` through the consent door saves exactly
//! the bytes shown, never a run. Keyless: an isolated HOME and an empty environment; the model is
//! a double that scripts what an author decides, never what Nika checks.
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

use std::io::{BufRead as _, BufReader, Lines, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::{Value, json};

const REQUEST: &str = "fais moi un workflow tres simple qui recupere les news tech recentes, les resume et ecrit le resultat en markdown dans un dossier du projet";
const ACCEPT: &str = "oui tout me va";
const SOURCE: &str = "https://news.ycombinator.com";
const OUTPUT: &str = "./news/digest.md";
/// The kept choice of the local route this loopback serves.
const CHOICE: &str = r#"{"kind":{"kind":"local","provider":"vllm"},"model":"vllm/agent-seat","chosen_at":"2026-10-10T00:00:00Z"}"#;

/// The candidate the accepted offer builds: one GET, one summary, one write.
fn digest() -> String {
    format!(
        "nika: news-digest\nmodel: vllm/agent-seat\npermits:\n  tools: [\"nika:fetch\", \"nika:write\"]\n  net:\n    http: [\"news.ycombinator.com\"]\n  fs:\n    write: [\"{OUTPUT}\"]\ntasks:\n  hacker_news:\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"{SOURCE}\", method: GET }}\n  summarize:\n    with:\n      news: \"${{{{ tasks.hacker_news.output }}}}\"\n    infer:\n      max_tokens: 1000\n      prompt: \"Résume en Markdown les actualités ci-dessous, sans rien inventer : ${{{{ with.news }}}}\"\n  write_digest:\n    with:\n      digest: \"${{{{ tasks.summarize.output }}}}\"\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"{OUTPUT}\", content: \"${{{{ with.digest }}}}\" }}\n"
    )
}

/// What the scripted author does, one step per request that offers tools.
fn script() -> Vec<Value> {
    let offered = |value: &str, role: &str| {
        json!({"value": value, "kind": "offered", "role": role, "message": "u2",
            "question": "plan", "option": "recommended"})
    };
    vec![
        json!({"tool": "ask", "args": {"questions": [{
            "key": "plan", "question": "Je prends Hacker News et j'écris le résumé dans ./news/digest.md : ça te va ?",
            "options": [{"key": "recommended", "label": "Oui", "recommended": true, "values": [
                {"role": "read_source", "value": SOURCE, "name": "Hacker News"},
                {"role": "output_path", "value": OUTPUT}]}],
            "free_text": true}]}}),
        json!({"tool": "candidate_write", "args": {"source": digest(), "summary": "Hacker News, résumé dans ./news/digest.md",
            "resolutions": [offered(SOURCE, "read_source"), offered(OUTPUT, "output_path")]}}),
        json!({"tool": "propose", "args": {}}),
        json!({"say": "Voici le workflow : Hacker News, résumé dans ./news/digest.md."}),
    ]
}

/// A loopback model on the OpenAI-compatible wire the `vllm` override reaches: each request that
/// offers tools takes the next step of the script; every body is kept.
struct Author {
    port: u16,
    bodies: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl Author {
    // The synchronous fixture owns this blocking socket thread and joins it on drop.
    #[allow(clippy::disallowed_methods)]
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback author");
        let port = listener.local_addr().expect("address").port();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, halt) = (Arc::clone(&bodies), Arc::clone(&stop));
        let server = std::thread::spawn(move || {
            let mut steps = script().into_iter();
            for (calls, stream) in listener.incoming().enumerate() {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let Some(body) = read_body(&mut stream) else {
                    continue;
                };
                let agent = body["tools"].as_array().is_some_and(|t| !t.is_empty());
                seen.lock().expect("author log").push(body);
                let scripted = if agent { steps.next() } else { None };
                let message = match scripted {
                    Some(call) if call.get("tool").is_some() => json!({
                        "role": "assistant", "content": null,
                        "tool_calls": [{"id": format!("call_{calls}"), "type": "function",
                            "function": {"name": call["tool"], "arguments": call["args"].to_string()}}]}),
                    Some(said) => json!({"role": "assistant", "content": said["say"]}),
                    None => json!({"role": "assistant", "content": "D'accord."}),
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

/// One chat completion carrying `message`.
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

/// One `submit` command of the contract: `line` typed against the snapshot `snapshot` names.
fn submit(command: &str, snapshot: &Value, line: &str) -> Value {
    json!({
        "contract": "nika/session-host@1", "op": "submit", "command": command,
        "snapshot": snapshot, "line": line,
    })
}

/// `nika session --json` in `project`, keyless, its HOME keeping the loopback author's local
/// route as the intelligence.
struct Door {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Door {
    fn open(project: &Path, home: &Path, author: &Author) -> Self {
        std::fs::create_dir_all(home.join(".nika")).expect("home");
        std::fs::write(home.join(".nika/session-intelligence.json"), CHOICE).expect("choice");
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
            .stderr(Stdio::null())
            .spawn()
            .expect("nika session --json");
        let stdin = child.stdin.take().expect("stdin");
        let lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
        Self {
            child,
            stdin,
            lines,
        }
    }

    /// The next frame of `kind`.
    fn next(&mut self, kind: &str) -> Value {
        loop {
            let line = self.lines.next().expect("a frame").expect("utf-8");
            let frame: Value = serde_json::from_str(&line).expect("one JSON object per line");
            if frame["frame"] == kind {
                return frame;
            }
        }
    }

    /// `line` typed against `frame`'s snapshot; the result frame.
    fn submit(&mut self, command: &str, frame: &Value, line: &str) -> Value {
        let submitted = submit(command, &frame["snapshot"]["snapshot"], line);
        writeln!(self.stdin, "{submitted}").expect("submit");
        self.next("result")
    }

    /// Stdin ends: the door closes and exits cleanly.
    fn close(mut self) {
        drop(self.stdin);
        loop {
            let line = self.lines.next().expect("a frame").expect("utf-8");
            let frame: Value = serde_json::from_str(&line).expect("one JSON object per line");
            if frame["frame"] == "closed" {
                break;
            }
        }
        assert!(self.child.wait().expect("exit").success());
    }
}

/// The request met with one question whose recommended option carries concrete values, under
/// the identity a host answers; the author was offered the Session's tools.
fn assert_asked(asked: &Value, author: &Author) {
    assert_eq!(asked["outcomes"][0]["kind"], "question", "{asked}");
    let work = &asked["snapshot"]["work"];
    assert_eq!(work["waiting"]["kind"], "question", "{asked}");
    let question = &work["questions"][0];
    assert_eq!(
        question["id"], work["waiting"]["id"],
        "the identity a host answers: {asked}"
    );
    assert_eq!(
        question["options"][0]["values"][0]["value"], SOURCE,
        "{asked}"
    );
    let tools = author.bodies()[0]["tools"].clone();
    for name in ["ask", "candidate_write", "propose", "new_request"] {
        assert!(
            tools
                .as_array()
                .into_iter()
                .flatten()
                .any(|t| t["function"]["name"] == name),
            "{name}: {tools}"
        );
    }
}

/// The accepted offer's candidate proposed with each value's provenance, its exact bytes shown
/// and nothing written; returns where it lands.
fn assert_proposed(proposed: &Value, author: &Author, project: &Path) -> String {
    assert_eq!(proposed["outcomes"][0]["kind"], "proposal", "{proposed}");
    let work = &proposed["snapshot"]["work"];
    assert_eq!(work["waiting"]["kind"], "consent", "{proposed}");
    for value in [SOURCE, OUTPUT] {
        let bound = (work["bindings"].as_array().into_iter().flatten())
            .find(|b| b["value"] == value)
            .unwrap_or_else(|| panic!("{value} is bound: {work}"));
        assert_eq!(bound["provenance"]["kind"], "offered", "{bound}");
        assert_eq!(bound["provenance"]["message"], "u2", "{bound}");
    }
    let file = &work["candidate"]["files"][0];
    let path = file["path"].as_str().expect("path").to_owned();
    assert_eq!(
        file["content"],
        digest().as_str(),
        "the exact bytes proposed"
    );
    assert!(
        !project.join(&path).exists(),
        "nothing lands before the yes"
    );
    let answered = author.bodies()[1].to_string();
    assert!(
        answered.contains(ACCEPT),
        "the reply reached the author as typed"
    );
    path
}

#[test]
fn the_agent_asks_writes_proposes_and_the_yes_saves_through_the_machine_door() {
    let project = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("home");
    let author = Author::start();
    let mut door = Door::open(project.path(), home.path(), &author);
    let opened = door.next("opened");
    let asked = door.submit("c-1", &opened, REQUEST);
    assert_asked(&asked, &author);
    let proposed = door.submit("c-2", &asked, ACCEPT);
    let path = assert_proposed(&proposed, &author, project.path());
    let saved = door.submit("c-3", &proposed, "yes");
    assert_eq!(
        saved["snapshot"]["work"]["saved"]["workflow"],
        path.as_str(),
        "{saved}"
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join(&path)).expect("saved"),
        digest(),
        "the yes saved the previewed bytes"
    );
    assert!(
        !project.path().join("news/digest.md").exists(),
        "Save is never a Run"
    );
    door.close();
}
