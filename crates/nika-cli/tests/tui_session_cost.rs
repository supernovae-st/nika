// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real PTY → renderer → Live → the actual Session one-time unknown-cost
//! question → the production provider registry and admission account →
//! injected HTTP. Hermetic mechanics only: no provider socket, and no model,
//! billing or UX qualification. Only a greeting is sent: work would reach the
//! compiler's own transport, which this fixture does not inject.
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
use nika_providers::{InferenceAdmission, ProviderRegistry, ProvidersConfig};
use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, UserIntelligencePreference,
};
use nika_session::{CostHostEvidence, ReasonError, Reply, ScriptedReasoner, SessionReasoner};
use nika_tui::session::{Live, Runners};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

const MODEL: &str = "deepseek/s90-unpriced-fixture";

fn fixture_root() -> Option<PathBuf> {
    let root = std::env::current_dir().unwrap();
    root.join(".s90-fixture").is_file().then_some(root)
}

/// The injected transport: one line per request that reached the wire.
struct FixtureHttp {
    root: PathBuf,
}

impl HttpPostDyn for FixtureHttp {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        use std::io::Write as _;
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().unwrap()).unwrap();
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("http.ndjson"))
            .unwrap();
        writeln!(log, "{body}").unwrap();
        let body = r#"{"model":"s90-unpriced-fixture","id":"fixture","choices":[{"message":{"content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_cache_hit_tokens":0,"prompt_cache_miss_tokens":10,"total_tokens":12}}"#;
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

/// The selected unpriced route: every call rides the admission account the
/// Session hands over, through the production registry, to the fixture
/// transport; only the reply's words are scripted.
struct FixtureRoute {
    root: PathBuf,
    words: ScriptedReasoner,
}

impl FixtureRoute {
    fn send(&mut self, prompt: &str, account: &InferenceAdmission) -> Result<Reply, ReasonError> {
        let registry = ProviderRegistry::new(
            Arc::new(FixtureHttp {
                root: self.root.clone(),
            }),
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
        "S90 fixture route".to_owned()
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

/// Invoked only by the PTY driver below, through this test executable.
#[test]
fn native_tui_session_cost_parent() {
    let Some(root) = fixture_root() else {
        return;
    };
    let mut census = IntelligenceCensus::empty();
    census.api_keys.push("deepseek".to_owned());
    let pref = UserIntelligencePreference::new(
        IntelligenceKind::Api {
            provider: "deepseek".to_owned(),
        },
        Some(MODEL.to_owned()),
    );
    let runners = Runners {
        run_once: Box::new(|_, _| panic!("this fixture never runs a workflow")),
        run_resume: Box::new(|_, _, _, _| panic!("this fixture has no resume")),
        run_tapped: None,
    };
    let route_root = root.clone();
    let live = Live::new(
        root,
        census,
        Some(pref),
        None,
        Box::new(move |_| {
            Box::new(FixtureRoute {
                root: route_root.clone(),
                words: ScriptedReasoner::new(vec!["Hello from the S90 fixture route.".to_owned()]),
            })
        }),
        runners,
    )
    .with_cost_host_evidence(CostHostEvidence::unmanaged_interactive_local());
    let mut options = nika_tui::app::Options::new(nika_tui::model::Presentation::Inline);
    options.term = Some("xterm-256color".into());
    options.reduced_motion = true;
    nika_tui::app::run(live, options).unwrap();
}

type LoggedPty = expectrl::session::Session<
    expectrl::process::unix::UnixProcess,
    expectrl::stream::log::LogStream<expectrl::process::unix::PtyStream, std::io::Stderr>,
>;

fn spawn(root: &Path) -> LoggedPty {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "native_tui_session_cost_parent", "--nocapture"])
        .current_dir(root)
        .env_clear()
        .env("TERM", "xterm-256color")
        .env("HOME", root);
    let raw = expectrl::session::OsSession::spawn(command).unwrap();
    let mut p = expectrl::session::log(raw, std::io::stderr()).unwrap();
    p.set_expect_timeout(Some(Duration::from_secs(30)));
    p.expect("\x1b[c").unwrap();
    p.send("\x1b[?62;22c").unwrap();
    p.expect("\x1b[6n").unwrap();
    p.send("\x1b[24;1R").unwrap();
    p.expect("nika ›").unwrap();
    p
}

fn new_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".s90-fixture"), "test executable only").unwrap();
    dir
}

/// Requests that reached the fixture transport.
fn calls(root: &Path) -> usize {
    std::fs::read_to_string(root.join("http.ndjson"))
        .unwrap_or_default()
        .lines()
        .count()
}

/// The painted authoring cost question; its reply prompt appears only after
/// the shell discarded what was typed before it.
fn asked(p: &mut LoggedPty) {
    p.expect("Fresh").unwrap();
    p.expect("authoring").unwrap();
    p.expect("decision").unwrap();
    p.expect("Continue").unwrap();
    p.expect("once?").unwrap();
    p.expect("reply ›").unwrap();
}

fn leave(p: &mut LoggedPty) {
    p.send("\x03\x03").unwrap();
    p.expect(Eof).unwrap();
    p.get_process_mut().wait().unwrap();
}

#[test]
fn prequestion_typeahead_paste_and_ctrl_c_never_approve_the_session_question() {
    let root = new_root();
    let mut p = spawn(root.path());
    // Everything after the first line is typed before the question can be
    // painted: a yes, a pasted yes and its Enter all fall on the discarded side.
    p.send("hello\ryes\r\x1b[200~yes\x1b[201~\r").unwrap();
    asked(&mut p);
    assert_eq!(calls(root.path()), 0);
    // details: the same review's evidence, answering nothing.
    p.send("details\r").unwrap();
    p.expect("invocation").unwrap();
    p.expect("endpoint").unwrap();
    p.expect("reply ›").unwrap();
    assert_eq!(calls(root.path()), 0);
    // A paste after the question is data in the composer; Ctrl+C cancels.
    p.send("\x1b[200~yes\x1b[201~").unwrap();
    p.send("\x03").unwrap();
    p.expect("cancelled").unwrap();
    p.expect("nothing").unwrap();
    p.expect("sent").unwrap();
    assert_eq!(calls(root.path()), 0);
    // The kept pasted yes, submitted now, answers nothing.
    p.send("\r").unwrap();
    p.expect("waits").unwrap();
    assert_eq!(calls(root.path()), 0);
    leave(&mut p);
    assert_eq!(calls(root.path()), 0);
}

#[test]
fn a_revision_keeps_the_review_and_only_a_fresh_yes_after_it_is_accepted_once() {
    let root = new_root();
    let mut p = spawn(root.path());
    p.send("hello\r").unwrap();
    asked(&mut p);
    // Other words never answer the question: the same review is asked again,
    // naming the request it still covers, and nothing is sent (one grammar
    // with the Run's cost decision: never a yes, never a silent cancel). Its
    // reply prompt returns only after the shell discarded any typeahead.
    p.send("hello again\r").unwrap();
    p.expect("unchanged:").unwrap();
    p.expect("Continue").unwrap();
    p.expect("once?").unwrap();
    p.expect("reply ›").unwrap();
    assert_eq!(calls(root.path()), 0);
    // Cancelling is an explicit no; a later yes answers nothing.
    p.send("no\r").unwrap();
    p.expect("cancelled").unwrap();
    p.expect("nothing").unwrap();
    p.expect("sent").unwrap();
    p.send("yes\r").unwrap();
    p.expect("waits").unwrap();
    assert_eq!(calls(root.path()), 0);
    p.send("hello\r").unwrap();
    asked(&mut p);
    assert_eq!(calls(root.path()), 0);
    p.send("yes\r").unwrap();
    p.expect("nika ›").unwrap();
    assert_eq!(calls(root.path()), 1);
    p.send("yes\r").unwrap();
    p.expect("waits").unwrap();
    assert_eq!(calls(root.path()), 1);
    leave(&mut p);
}
