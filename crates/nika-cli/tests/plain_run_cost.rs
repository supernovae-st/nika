// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real PTY → the production host review (`run_cost::review`) on its local
//! `ReviewChannel::Terminal` → an INJECTED fixture transport and the admitted account.
//! Hermetic mechanics only: no provider socket and no model, billing or UX qualification.
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]
use expectrl::{Eof, Expect};
use nika_kernel::ai::provider::{InferRequest, Message, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use std::collections::BTreeMap;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

const MODEL: &str = "deepseek/s94-unpriced-fixture";
const SOURCE: &str = "nika: fresh-run\nmodel: deepseek/s94-unpriced-fixture\ntasks:\n  answer:\n    infer: { prompt: Say OK, max_tokens: 32 }\noutputs:\n  result: ${{ tasks.answer.output }}\n";

fn fixture_stdout(value: impl std::fmt::Display) {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{value}").unwrap();
    out.flush().unwrap();
}

fn fixture_root() -> Option<PathBuf> {
    let root = std::env::current_dir().unwrap();
    root.join(".s94-terminal-fixture").is_file().then_some(root)
}

/// The injected fixture transport (never a provider): one line per request.
struct FixtureHttp {
    root: PathBuf,
}

impl HttpPostDyn for FixtureHttp {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().unwrap()).unwrap();
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("http.ndjson"))
            .unwrap();
        writeln!(log, "{body}").unwrap();
        let body = r#"{"model":"s94-unpriced-fixture","id":"fixture","choices":[{"message":{"content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_cache_hit_tokens":0,"prompt_cache_miss_tokens":10,"total_tokens":12}}"#;
        Ok(HttpResponse::new(
            200,
            BTreeMap::new(),
            body.as_bytes().to_vec().into(),
            request.url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("unknown-cost Run cannot stream a provider request")
    }
}

fn plan() -> nika_providers::ExecutionAccessPlan {
    let probe = nika_providers::probe::ProviderProbe::new(
        "deepseek",
        true,
        true,
        "DEEPSEEK_API_KEY",
        false,
        nika_providers::probe::ProviderReadiness::new(
            true,
            true,
            None,
            None,
            true,
            nika_providers::probe::ExecutionLocus::Cloud,
            nika_types::access::AccessClass::Api,
        ),
        "https://api.deepseek.com",
    );
    let plan = nika_providers::resolve_execution_plan(
        &[nika_providers::ModelNeed::new(MODEL, true, false)],
        &[probe],
        Some("api"),
    );
    assert!(plan.is_admitted());
    plan
}

/// Invoked only by the drivers below, through this test executable: the local
/// terminal review exactly as an interactive `nika run` reaches it.
#[test]
fn terminal_run_child() {
    let Some(root) = fixture_root() else {
        return;
    };
    if root.join(".s94-prebuffer").is_file() {
        // One byte read through std: the rest of that line stays in the Rust buffer.
        let mut first = [0u8; 1];
        std::io::stdin().read_exact(&mut first).unwrap();
    }
    let source = std::fs::read_to_string(root.join("one.nika")).unwrap();
    let wf = nika_schema::parse(
        &source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .unwrap();
    let result = nika_cli_host::run_cost::review(
        &root,
        root.join("one.nika").to_str().unwrap(),
        &source,
        nika_types::id::ExecutionId::generate().to_string(),
        &wf,
        &plan(),
        &BTreeMap::new(),
        Some(0.25),
        nika_cli_host::run_cost::ReviewChannel::Terminal,
    );
    let cost = match result {
        Ok(Some(cost)) => cost,
        Ok(None) => panic!("an unpriced route asked nothing"),
        Err(why) => {
            fixture_stdout(serde_json::json!({"error":{"message":why}}));
            std::process::exit(3);
        }
    };
    let registry = ProviderRegistry::new(
        Arc::new(FixtureHttp { root: root.clone() }),
        ProvidersConfig::new().with_key(
            "deepseek",
            nika_kernel::secret::Secret::new("fixture-not-a-key"),
        ),
    )
    .with_inference_admission(cost.config.inference_admission.clone().unwrap());
    let provider = registry.resolve(MODEL).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut request = InferRequest::new(MODEL, vec![Message::text(Role::User, "Say OK")]);
    request.max_tokens = Some(32);
    let sent = runtime.block_on(provider.infer_reported(request));
    cost.finish().unwrap();
    fixture_stdout(format!("fixture Run observed · sent {}", sent.is_ok()));
    std::process::exit(if sent.is_ok() { 0 } else { 3 });
}

type LoggedPty = expectrl::session::Session<
    expectrl::process::unix::UnixProcess,
    expectrl::stream::log::LogStream<expectrl::process::unix::PtyStream, std::io::Stderr>,
>;

fn child(root: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "terminal_run_child", "--nocapture"])
        .current_dir(root)
        .env_clear()
        .env("TERM", "dumb")
        .env("HOME", root);
    command
}

fn spawn(root: &Path) -> LoggedPty {
    let raw = expectrl::session::OsSession::spawn(child(root)).unwrap();
    let mut p = expectrl::session::log(raw, std::io::stderr()).unwrap();
    p.set_expect_timeout(Some(Duration::from_secs(30)));
    p
}

fn new_root(prebuffer: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".s94-terminal-fixture"),
        "test executable only",
    )
    .unwrap();
    std::fs::write(dir.path().join("one.nika"), SOURCE).unwrap();
    if prebuffer {
        std::fs::write(dir.path().join(".s94-prebuffer"), "test only").unwrap();
    }
    dir
}

/// Requests that reached the injected fixture transport.
fn calls(root: &Path) -> usize {
    std::fs::read_to_string(root.join("http.ndjson"))
        .unwrap_or_default()
        .lines()
        .count()
}

#[test]
fn queued_lines_and_an_unterminated_yes_never_answer_the_run_question() {
    let root = new_root(false);
    let mut p = spawn(root.path());
    // Typed before the question can exist: one whole line, then one without Enter.
    p.send("yes\ryes").unwrap();
    p.expect("Fresh").unwrap();
    p.expect("once?").unwrap();
    p.expect("continue once? ›").unwrap();
    assert_eq!(calls(root.path()), 0);
    p.send("\r").unwrap();
    p.expect("declined").unwrap();
    p.expect(Eof).unwrap();
    assert_eq!(calls(root.path()), 0);
}

#[test]
fn a_line_already_in_the_rust_stdin_buffer_never_answers() {
    let root = new_root(true);
    let mut p = spawn(root.path());
    p.send("Xyes\r").unwrap();
    p.expect("continue once? ›").unwrap();
    assert_eq!(calls(root.path()), 0);
    p.send("\r").unwrap();
    p.expect("declined").unwrap();
    p.expect(Eof).unwrap();
    assert_eq!(calls(root.path()), 0);
}

#[test]
fn only_a_yes_after_the_prompt_approves_once_and_details_answer_nothing() {
    let root = new_root(false);
    let mut p = spawn(root.path());
    p.expect("continue once? ›").unwrap();
    p.send("details\r").unwrap();
    p.expect("Source").unwrap();
    p.expect("SHA-256").unwrap();
    p.expect("continue once? ›").unwrap();
    assert_eq!(calls(root.path()), 0, "reading the details answered");
    p.send("yes\ryes\r").unwrap();
    p.expect("observed").unwrap();
    p.expect(Eof).unwrap();
    assert_eq!(
        calls(root.path()),
        1,
        "one fresh yes, one request, no replay"
    );
}

#[test]
fn end_of_input_or_an_interrupt_at_the_prompt_sends_nothing() {
    for (key, what) in [("\x04", "EOF"), ("\x03", "Ctrl+C")] {
        let root = new_root(false);
        let mut p = spawn(root.path());
        p.expect("continue once? ›").unwrap();
        p.send(key).unwrap();
        p.expect(Eof).unwrap();
        assert_eq!(calls(root.path()), 0, "{what}");
    }
}

#[test]
fn a_non_terminal_stdin_gets_no_question_and_sends_nothing() {
    let root = new_root(false);
    let out = child(root.path())
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("cannot obtain a fresh one-time choice"),
        "{text}"
    );
    assert!(!String::from_utf8_lossy(&out.stderr).contains("continue once?"));
    assert_eq!(calls(root.path()), 0);
}
