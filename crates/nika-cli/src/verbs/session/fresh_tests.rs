// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real PTY → the plain session door (`drive` over the real `read_burst` line source)
//! → the actual Save consent boundary, and continuous preparation through the
//! production provider registry into an INJECTED fixture transport. Hermetic
//! mechanics only: no provider socket, model, billing or UX qualification. Save
//! fixtures use the deterministic compiler with no intelligence; only greetings
//! reach the injected transport, with available usage and unknown prices retained.
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
use nika_session::intelligence::IntelligenceKind;
use nika_session::reasoner::{NoReasoner, SessionReasoner};
use nika_session::{ReasonError, Reply, ResolvedSessionIntelligence, ScriptedReasoner};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const MODEL: &str = "deepseek/s94-unpriced-fixture";
const MARKER: &str = ".s94-plain-fixture";
const SAVE_MARKER: &str = ".s94-save-fixture";
const COPY: &str = "Read ./notes/brief.md and write it to ./out/copy.md";
const WORKFLOW: &str = "compiled-workflow.nika";

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

/// The selected unpriced route observes continuous calls through the real journal;
/// explicitly bounded callers still use their admission account. Words are scripted.
struct FixtureRoute {
    words: ScriptedReasoner,
}

impl FixtureRoute {
    fn send(
        &mut self,
        prompt: &str,
        account: Option<&InferenceAdmission>,
    ) -> Result<Reply, ReasonError> {
        let root = std::env::current_dir().map_err(|e| ReasonError::Provider(e.to_string()))?;
        let mut registry = ProviderRegistry::new(
            Arc::new(FixtureHttp { root }),
            ProvidersConfig::new().with_key(
                "deepseek",
                nika_kernel::secret::Secret::new("fixture-not-a-key"),
            ),
        );
        if let Some(account) = account {
            registry = registry.with_inference_admission(account.clone());
        }
        let provider = registry
            .resolve(MODEL)
            .map_err(|e| ReasonError::Provider(e.to_string()))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| ReasonError::Runtime(e.to_string()))?;
        let mut request = InferRequest::new(MODEL, vec![Message::text(Role::User, prompt)]);
        request.max_tokens = Some(32);
        let sent = runtime
            .block_on(
                nika_providers::authoring::preparation::PreparationCosts::capture(
                    provider.infer_reported(request),
                ),
            )
            .map_err(|(e, _)| ReasonError::Provider(e.to_string()))?;
        let mut reply = self.words.reason(prompt)?;
        reply.usage_observed = sent.0.usage_reported;
        Ok(reply)
    }
}

impl SessionReasoner for FixtureRoute {
    fn name(&self) -> String {
        "S94 fixture route".to_owned()
    }
    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.send(prompt, None)
    }
    fn supports_admission(&self) -> bool {
        true
    }
    fn reason_with_admission(
        &mut self,
        prompt: &str,
        account: &InferenceAdmission,
    ) -> Result<Reply, ReasonError> {
        self.send(prompt, Some(account))
    }
    fn reason_label_with_admission(
        &mut self,
        prompt: &str,
        account: &InferenceAdmission,
    ) -> Result<Reply, ReasonError> {
        self.send(prompt, Some(account))
    }
    fn authoring_model(&self) -> Option<String> {
        Some(MODEL.to_owned())
    }
}

fn fixture_route(choice: &ResolvedSessionIntelligence) -> Box<dyn SessionReasoner> {
    if matches!(choice.kind, IntelligenceKind::None) {
        return Box::new(NoReasoner);
    }
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
    let (kind, model) = if root.join(SAVE_MARKER).is_file() {
        (IntelligenceKind::None, None)
    } else {
        (
            IntelligenceKind::Api {
                provider: "deepseek".to_owned(),
            },
            Some(MODEL.to_owned()),
        )
    };
    UserIntelligencePreference::new(kind, model)
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

/// Save is still an explicit decision after continuous preparation finishes.
fn asked(p: &mut LoggedPty) {
    p.expect("Nika proposes `compiled-workflow.nika`:").unwrap();
    p.expect("apply? ›").unwrap();
}

fn save_root() -> tempfile::TempDir {
    let root = new_root();
    std::fs::write(root.path().join(SAVE_MARKER), "no intelligence").unwrap();
    std::fs::create_dir(root.path().join("notes")).unwrap();
    std::fs::write(
        root.path().join("notes/brief.md"),
        "Keep the source unchanged.\n",
    )
    .unwrap();
    root
}

fn nothing_saved_or_run(root: &Path) {
    assert!(!root.join(WORKFLOW).exists(), "Save was not authorized");
    assert!(!root.join("out/copy.md").exists(), "Run was not authorized");
    assert_eq!(calls(root), 0, "deterministic preparation needs no model");
}

#[test]
fn an_unterminated_prequestion_yes_then_a_bare_enter_never_approves() {
    let root = save_root();
    let mut p = spawn(root.path());
    // The unterminated yes arrives before the proposal and must be drained.
    p.send(format!("{COPY}\ryes")).unwrap();
    asked(&mut p);
    nothing_saved_or_run(root.path());
    p.send("\r").unwrap();
    p.expect("that line is not a consent").unwrap();
    p.expect("apply? ›").unwrap();
    nothing_saved_or_run(root.path());
    p.send("non\r").unwrap();
    p.expect("discarded · nothing was written").unwrap();
    p.expect("nika ›").unwrap();
    p.send("yes\r").unwrap();
    p.expect("nothing waits for a yes or a no").unwrap();
    p.expect("nika ›").unwrap();
    nothing_saved_or_run(root.path());
    p.send("\x04").unwrap();
    p.expect(Eof).unwrap();
    nothing_saved_or_run(root.path());
}

#[test]
fn only_a_yes_typed_after_the_prompt_approves_and_it_never_replays() {
    let root = save_root();
    let mut p = spawn(root.path());
    p.send(format!("{COPY}\r")).unwrap();
    asked(&mut p);
    p.send("/show\r").unwrap();
    p.expect("the proposal still waits").unwrap();
    p.expect("apply? ›").unwrap();
    nothing_saved_or_run(root.path());
    p.send("yes\r").unwrap();
    p.expect("applied · wrote `compiled-workflow.nika`")
        .unwrap();
    p.expect("check · `compiled-workflow.nika` · clean")
        .unwrap();
    p.expect("nika ›").unwrap();
    let saved = std::fs::read(root.path().join(WORKFLOW)).unwrap();
    assert!(!saved.is_empty());
    p.send("yes\r").unwrap();
    p.expect("nothing waits for a yes or a no").unwrap();
    p.expect("nika ›").unwrap();
    p.send("\x04").unwrap();
    p.expect(Eof).unwrap();
    assert!(
        std::fs::read(root.path().join(WORKFLOW)).unwrap() == saved,
        "a second yes must not change the saved workflow"
    );
    assert!(
        !root.path().join("out/copy.md").exists(),
        "Save never grants Run"
    );
    assert_eq!(calls(root.path()), 0);
}

#[test]
fn end_of_input_and_an_interrupt_at_the_prompt_save_nothing() {
    for key in ["\x04", "\x03"] {
        let root = save_root();
        let mut p = spawn(root.path());
        p.send(format!("{COPY}\r")).unwrap();
        asked(&mut p);
        p.send(key).unwrap();
        p.expect(Eof).unwrap();
        nothing_saved_or_run(root.path());
    }
}

/// Complete usage is evidence, not a price: this fixture has no catalog tariff.
fn assert_unknown_observed(root: &Path) {
    let state = nika_session::SessionState::load(root).unwrap().unwrap();
    let observation = state
        .inference_observations
        .iter()
        .find(|o| o["schema"] == "nika/preparation-cost-observation@1")
        .expect("the real plain door must persist its preparation observation");
    assert!(
        observation["calls"].as_array().unwrap().len() == 1,
        "one dispatch must be retained"
    );
    assert!(
        observation["unknown_calls"] == 1,
        "the unpriced call must remain unknown"
    );
    assert!(
        observation["billing"] == "unknown",
        "usage does not establish a bill"
    );
    assert!(
        observation["authority"] == "observation_only",
        "observation grants no authority"
    );
    assert!(
        observation["state"] == "Closed",
        "the fixture response must settle its scope"
    );
    let call = &observation["calls"][0];
    assert!(
        call["estimated_usd"].is_null(),
        "the fixture has no catalog tariff"
    );
    assert!(
        call["usage"]["input_tokens"] == 10,
        "reported input usage must survive"
    );
    assert!(
        call["usage"]["output_tokens"] == 2,
        "reported output usage must survive"
    );
    assert!(
        call["usage_complete"] == true,
        "the reported usage is complete"
    );
}

#[test]
fn continuous_preparation_observes_an_unpriced_call_without_a_cost_question_or_replay() {
    let root = new_root();
    let mut p = spawn(root.path());
    p.send("hello\r").unwrap();
    p.expect("Hello from the S94 fixture route.").unwrap();
    p.expect("nika ›").unwrap();
    assert_eq!(calls(root.path()), 1);
    assert_unknown_observed(root.path());
    p.send("yes\r").unwrap();
    p.expect("nothing waits for a yes or a no").unwrap();
    p.expect("nika ›").unwrap();
    p.send("\x04").unwrap();
    p.expect(Eof).unwrap();
    let mut reopened = spawn(root.path());
    assert_eq!(calls(root.path()), 1, "reopening cannot replay the call");
    assert_unknown_observed(root.path());
    reopened.send("\x04").unwrap();
    reopened.expect(Eof).unwrap();
    assert_eq!(calls(root.path()), 1);
}

#[test]
fn the_plain_driver_with_piped_input_keeps_continuous_observations() {
    // This invokes drive directly. The public binary's pipe concierge is tested
    // separately in session_pty; it does not open this plain Session driver.
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
    assert!(
        out.status.success(),
        "the injected plain driver must finish"
    );
    assert!(
        !text.contains("continue once?"),
        "continuous preparation must not ask a cost question"
    );
    assert!(text.contains("Hello from the S94 fixture route."));
    assert_eq!(calls(root.path()), 1);
    assert_unknown_observed(root.path());
}
