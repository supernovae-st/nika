// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real PTY → the plain session door (`drive` over the real `read_burst` line source)
//! → the actual Session one-time unknown-cost question → the production provider
//! registry and the Session's admission account → an INJECTED fixture transport that
//! logs each request. Hermetic mechanics only: no provider socket, and no model,
//! billing or UX qualification. Only greetings are sent: work would reach the
//! compiler's own transport, which this fixture does not inject.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]
use super::*;
use expectrl::{Eof, Expect};
use nika_kernel::ai::provider::{InferRequest, Message, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_providers::{InferenceAdmission, ProviderRegistry, ProvidersConfig};
use nika_session::{ReasonError, Reply, ScriptedReasoner};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const MODEL: &str = "deepseek/s94-unpriced-fixture";
const MARKER: &str = ".s94-plain-fixture";

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
        panic!("an unknown-cost Session call cannot stream")
    }
}

/// The selected unpriced route: every call rides the Session's admission account
/// through the production registry into the fixture transport; words are scripted.
struct FixtureRoute {
    words: ScriptedReasoner,
}

impl FixtureRoute {
    fn send(&mut self, prompt: &str, account: &InferenceAdmission) -> Result<Reply, ReasonError> {
        let root = std::env::current_dir().map_err(|e| ReasonError::Provider(e.to_string()))?;
        let registry = ProviderRegistry::new(
            Arc::new(FixtureHttp { root }),
            ProvidersConfig::new().with_key(
                "deepseek",
                nika_kernel::secret::Secret::new("fixture-not-a-key"),
            ),
        )
        .with_inference_admission(account.clone());
        let provider = registry
            .resolve(MODEL)
            .map_err(|e| ReasonError::Provider(e.to_string()))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ReasonError::Runtime(e.to_string()))?;
        let mut request = InferRequest::new(MODEL, vec![Message::text(Role::User, prompt)]);
        request.max_tokens = Some(32);
        let _sent = runtime
            .block_on(provider.infer_reported(request))
            .map_err(|(e, _)| ReasonError::Provider(e.to_string()))?;
        self.words.reason(prompt)
    }
}

impl SessionReasoner for FixtureRoute {
    fn name(&self) -> String {
        "S94 fixture route".to_owned()
    }
    fn reason(&mut self, _prompt: &str) -> Result<Reply, ReasonError> {
        Err(ReasonError::Provider(
            "an unknown-cost route never answers unmetered".to_owned(),
        ))
    }
    fn supports_admission(&self) -> bool {
        true
    }
    fn reason_with_admission(
        &mut self,
        prompt: &str,
        account: &InferenceAdmission,
    ) -> Result<Reply, ReasonError> {
        self.send(prompt, account)
    }
    fn reason_label_with_admission(
        &mut self,
        prompt: &str,
        account: &InferenceAdmission,
    ) -> Result<Reply, ReasonError> {
        self.send(prompt, account)
    }
    fn authoring_model(&self) -> Option<String> {
        Some(MODEL.to_owned())
    }
}

fn fixture_route(_: &ResolvedSessionIntelligence) -> Box<dyn SessionReasoner> {
    Box::new(FixtureRoute {
        words: ScriptedReasoner::new(vec!["Hello from the S94 fixture route.".to_owned()]),
    })
}

/// Invoked only by the PTY drivers below, through this test executable: the plain door
/// exactly as `run` opens it, with the fixture route kept as the chosen intelligence.
#[test]
fn plain_cost_parent() {
    let root = std::env::current_dir().unwrap();
    if !root.join(MARKER).is_file() {
        return;
    }
    let mut census = IntelligenceCensus::empty();
    census.api_keys.push("deepseek".to_owned());
    let api = IntelligenceKind::Api {
        provider: "deepseek".to_owned(),
    };
    UserIntelligencePreference::new(api, Some(MODEL.to_owned()))
        .save(&root)
        .unwrap();
    let mut input = PerCallLines::new(read_burst);
    let mut output = std::io::stdout();
    let theme = Theme::new(false, false, false);
    let home = Some(root.as_path());
    let code = drive(
        &mut input,
        &mut output,
        &census,
        home,
        &root,
        theme,
        Box::new(fixture_route),
    );
    assert_eq!(code.unwrap(), exit::OK);
}

type LoggedPty = expectrl::session::Session<
    expectrl::process::unix::UnixProcess,
    expectrl::stream::log::LogStream<expectrl::process::unix::PtyStream, std::io::Stderr>,
>;

fn spawn(root: &Path) -> LoggedPty {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "verbs::session::fresh_tests::plain_cost_parent",
            "--nocapture",
        ])
        .current_dir(root)
        .env_clear()
        .env("TERM", "dumb")
        .env("NO_COLOR", "1")
        .env("HOME", root);
    let raw = expectrl::session::OsSession::spawn(command).unwrap();
    let mut p = expectrl::session::log(raw, std::io::stderr()).unwrap();
    p.set_expect_timeout(Some(Duration::from_secs(30)));
    p.expect("nika ›").unwrap();
    p
}

fn new_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(MARKER), "test executable only").unwrap();
    dir
}

/// Requests that reached the injected fixture transport.
fn calls(root: &Path) -> usize {
    std::fs::read_to_string(root.join("http.ndjson"))
        .unwrap_or_default()
        .lines()
        .count()
}

/// The Session's question shows, then — only after the fresh boundary — its prompt.
fn asked(p: &mut LoggedPty) {
    p.expect("USD").unwrap();
    p.expect("once?").unwrap();
    p.expect("continue once? ›").unwrap();
}

#[test]
fn an_unterminated_prequestion_yes_then_a_bare_enter_never_approves() {
    let root = new_root();
    let mut p = spawn(root.path());
    // `yes` typed without Enter before the question exists sits in the line discipline.
    p.send("hello\ryes").unwrap();
    asked(&mut p);
    assert_eq!(calls(root.path()), 0);
    p.send("\r").unwrap();
    p.expect("cancelled").unwrap();
    p.expect("nothing").unwrap();
    p.expect("sent").unwrap();
    p.send("yes\r").unwrap();
    p.expect("waits").unwrap();
    assert_eq!(calls(root.path()), 0);
    p.send("\x04").unwrap();
    p.expect(Eof).unwrap();
    assert_eq!(calls(root.path()), 0);
}

#[test]
fn only_a_yes_typed_after_the_prompt_approves_and_it_never_replays() {
    let root = new_root();
    let mut p = spawn(root.path());
    p.send("hello\r").unwrap();
    asked(&mut p);
    p.send("/details\r").unwrap();
    p.expect("invocation").unwrap();
    p.expect("endpoint").unwrap();
    p.expect("continue once? ›").unwrap();
    assert_eq!(calls(root.path()), 0, "reading the details answered");
    p.send("yes\r").unwrap();
    p.expect("nika ›").unwrap();
    assert_eq!(calls(root.path()), 1);
    p.send("yes\r").unwrap();
    p.expect("waits").unwrap();
    assert_eq!(calls(root.path()), 1);
    p.send("\x04").unwrap();
    p.expect(Eof).unwrap();
}

#[test]
fn end_of_input_and_an_interrupt_at_the_prompt_send_nothing() {
    for (key, what) in [("\x04", "EOF"), ("\x03", "Ctrl+C")] {
        let root = new_root();
        let mut p = spawn(root.path());
        p.send("hello\r").unwrap();
        asked(&mut p);
        p.send(key).unwrap();
        p.expect(Eof).unwrap();
        assert_eq!(calls(root.path()), 0, "{what}");
    }
}

#[test]
fn a_non_terminal_stdin_never_gets_the_plain_question() {
    let root = new_root();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "verbs::session::fresh_tests::plain_cost_parent",
            "--nocapture",
        ])
        .current_dir(root.path())
        .env_clear()
        .env("HOME", root.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"hello\n").unwrap();
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(!text.contains("continue once?"), "{text}");
    assert_eq!(calls(root.path()), 0);
}
