// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real PTY → renderer → Live → continuous preparation observations →
//! the production provider registry → injected HTTP. Hermetic mechanics only: no provider socket, and no model,
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
#[path = "../../nika-tui/tests/qa_support/vt.rs"]
#[expect(dead_code, reason = "this suite reads part of the shared VT screen")]
mod vt;

use expectrl::process::unix::WaitStatus;
use expectrl::session::OsSession;
use nika_kernel::ai::provider::{InferRequest, Message, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_providers::{InferenceAdmission, ProviderRegistry, ProvidersConfig};
use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, UserIntelligencePreference,
};
use nika_session::{CostHostEvidence, ReasonError, Reply, ScriptedReasoner, SessionReasoner};
use nika_tui::session::{Live, Runners};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
        drop(log);
        if self.root.join("hold-response").exists() {
            std::future::pending::<()>().await;
        }
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

/// The selected unpriced route captures actual preparation requests; bounded
/// callers still carry their admission account. Only reply words are scripted.
struct FixtureRoute {
    root: PathBuf,
    words: ScriptedReasoner,
}

impl FixtureRoute {
    fn send(
        &mut self,
        prompt: &str,
        account: Option<&InferenceAdmission>,
    ) -> Result<Reply, ReasonError> {
        let mut registry = ProviderRegistry::new(
            Arc::new(FixtureHttp {
                root: self.root.clone(),
            }),
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
                nika_providers::authoring::preparation::PreparationCosts::while_active(
                    provider.infer_reported(request),
                ),
            )
            .ok_or(ReasonError::Cancelled)?
            .map_err(|(e, _)| ReasonError::Provider(e.to_string()))?;
        let mut reply = self.words.reason(prompt)?;
        reply.usage_observed = sent.0.usage_reported;
        Ok(reply)
    }
}

impl SessionReasoner for FixtureRoute {
    fn name(&self) -> String {
        "S90 fixture route".to_owned()
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

/// Read the composed screen: the diff renderer may split any word over
/// cursor moves, and an unchanged prompt is not repainted after a turn.
struct Term {
    pty: OsSession,
    screen: vt::Screen,
    eof: bool,
}

impl Term {
    fn pump(&mut self) {
        let mut bytes = [0; 16 * 1024];
        while !self.eof {
            match self.pty.try_read(&mut bytes) {
                Ok(0) => self.eof = true,
                Ok(n) => {
                    self.screen.feed(&bytes[..n]);
                    for reply in self.screen.take_replies() {
                        self.pty.write_all(&reply).unwrap();
                        self.pty.flush().unwrap();
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => self.eof = true,
            }
        }
    }

    fn send(&mut self, keys: &str) {
        self.pty.write_all(keys.as_bytes()).unwrap();
        self.pty.flush().unwrap();
    }

    fn wait_until(&mut self, what: &str, done: impl Fn(&vt::Screen) -> bool) {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            self.pump();
            if done(&self.screen) {
                return;
            }
            assert!(
                !self.eof && Instant::now() < until,
                "{what}: no matching terminal state.\n{}",
                self.screen.text()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn turn(&mut self, keys: &str, reply: &str) {
        self.pump();
        let count = |screen: &vt::Screen| {
            screen
                .transcript()
                .iter()
                .filter(|line| line.contains(reply))
                .count()
        };
        let before = count(&self.screen);
        self.send(keys);
        // The appended reply excludes an old reply recovered on reopen; the
        // current idle hint excludes the still-visible prompt of a busy turn.
        self.wait_until(reply, |screen| count(screen) > before && idle(screen));
    }

    /// Keep reading for `window`, so a later frame of the same state lands.
    fn settle(&mut self, window: Duration) {
        let until = Instant::now() + window;
        while !self.eof && Instant::now() < until {
            self.pump();
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn idle(screen: &vt::Screen) -> bool {
    screen.row_starting("nika ›").is_some()
        && screen.contains("describe work · /help")
        && !screen.contains("Preparing: Ctrl+C requests Stop;")
}

fn spawn(root: &Path) -> Term {
    spawn_sized(root, 80, 24)
}

fn spawn_sized(root: &Path, cols: u16, rows: u16) -> Term {
    let size = format!("stty cols {cols} rows {rows} && exec \"$0\" \"$@\"");
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", &size])
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "native_tui_session_cost_parent", "--nocapture"])
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TERM", "xterm-256color")
        .env("HOME", root);
    let mut pty = OsSession::spawn(command).unwrap();
    pty.get_process_mut().set_window_size(cols, rows).unwrap();
    let mut screen = vt::Screen::new(cols, rows);
    screen.park_at_bottom();
    let mut term = Term {
        pty,
        screen,
        eof: false,
    };
    term.wait_until("initial idle prompt", idle);
    term
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

/// Complete usage is not a tariff; an interrupted request has no returned usage.
fn assert_unknown_observed(root: &Path, returned: bool) {
    let state = nika_session::SessionState::load(root).unwrap().unwrap();
    let observed: Vec<_> = state
        .inference_observations
        .iter()
        .filter(|o| o["schema"] == "nika/preparation-cost-observation@1")
        .collect();
    assert_eq!(observed.len(), 1, "one retained preparation scope");
    let observation = observed[0];
    assert!(observation["calls"].as_array().unwrap().len() == 1);
    assert!(observation["unknown_calls"] == 1, "no tariff means unknown");
    assert!(observation["billing"] == "unknown", "usage is not a bill");
    assert!(observation["authority"] == "observation_only");
    assert!(observation["unbudgeted"] == true);
    assert!(observation["state"] == if returned { "Closed" } else { "Uncertain" });
    let call = &observation["calls"][0];
    assert!(call["estimated_usd"].is_null(), "unknown is not zero USD");
    if returned {
        assert!(
            call["pricing"]["kind"] == "unknown",
            "no catalog tariff exists"
        );
        assert!(call["usage"]["input_tokens"] == 10);
        assert!(call["usage"]["output_tokens"] == 2);
        assert!(call["usage_complete"] == true);
    } else {
        assert!(call["pricing"].is_null(), "no response was settled");
        assert!(call["usage"].is_null(), "Stop invents no response usage");
        assert!(call["usage_complete"] == false);
    }
    assert!(state.pending.is_none(), "no Run gate was opened");
    assert!(
        state.inference_checkpoint.is_none(),
        "no allowance was granted"
    );
    assert!(
        nika_session::ConsentRecord::read_all(root)
            .unwrap()
            .is_empty(),
        "a greeting, yes or Stop never consents to Save"
    );
    assert!(!root.join(".nika/traces").exists(), "no workflow ran");
    assert!(
        std::fs::read_dir(root).unwrap().all(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_none_or(|ext| ext != "nika")
        }),
        "no workflow was saved"
    );
}

/// Synchronize on the injected transport, not a delay or an early busy frame.
fn wait_for_request(root: &Path) {
    let until = std::time::Instant::now() + Duration::from_secs(30);
    while calls(root) == 0 {
        assert!(
            std::time::Instant::now() < until,
            "fixture request did not start"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(calls(root), 1);
}

fn leave(term: &mut Term) {
    term.send("\x03\x03");
    let until = Instant::now() + Duration::from_secs(30);
    while !term.eof {
        term.pump();
        assert!(Instant::now() < until, "the fixture did not exit");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(
        term.pty.get_process().wait().unwrap(),
        WaitStatus::Exited(_, 0)
    ));
}

/// The hint row while a preparation works.
const PREPARING: &str = "Preparing: Ctrl+C requests Stop;";

/// The three transcript rows a 60 × 8 focus view shows while Nika works.
fn reading_rows(screen: &vt::Screen) -> Vec<String> {
    screen.lines().into_iter().take(3).collect()
}

/// `Enter` on a correction while the focus transcript is scrolled back asks
/// Stop and sends the correction next. The stop, the correction's echo, its
/// busy row and its reply land below the rows being read; End shows them.
#[test]
fn a_stop_and_its_queued_correction_keep_a_scrolled_focus_reading_position() {
    let root = new_root();
    std::fs::write(root.path().join("hold-response"), "wait for Stop").unwrap();
    let mut p = spawn_sized(root.path(), 60, 8);
    // Below the workspace size, Ctrl+T opens the focus view.
    p.send("\x14");
    p.wait_until("the focus view", |screen| screen.on_alt() && idle(screen));
    p.send("hello\r");
    wait_for_request(root.path());
    p.wait_until("the busy preparation", |screen| screen.contains(PREPARING));
    let latest = reading_rows(&p.screen);
    // More pages than the transcript holds: the reading position is its first row.
    p.send("\x1b[5~\x1b[5~\x1b[5~\x1b[5~yes");
    p.wait_until("scrolled back with a typed correction", |screen| {
        reading_rows(screen) != latest
            && screen.contains("nika › yes")
            && screen.contains(PREPARING)
    });
    let before = reading_rows(&p.screen);
    p.send("\r");
    p.wait_until(
        "the stopped preparation and its sent correction",
        |screen| !screen.contains(PREPARING) && screen.row_starting("nika ›").is_some(),
    );
    p.settle(Duration::from_millis(1500));
    assert_eq!(
        reading_rows(&p.screen),
        before,
        "the stop and its correction moved the reading position\n{}",
        p.screen.text()
    );
    assert_eq!(calls(root.path()), 1, "Stop never retries the sent request");
    p.send("\x1b[F");
    p.wait_until("End shows the correction's reply", |screen| {
        screen.contains("nothing waits for a yes or a no") && idle(screen)
    });
    assert_unknown_observed(root.path(), false);
    leave(&mut p);
}

#[test]
fn continuous_preparation_observes_unknown_cost_without_a_question_or_replay() {
    let root = new_root();
    let mut p = spawn(root.path());
    // No spending answer is sent: the selected route actually answers once.
    p.turn("hello\r", "Hello from the S90 fixture route.");
    assert_eq!(calls(root.path()), 1);
    assert_unknown_observed(root.path(), true);
    p.turn("yes\r", "nothing waits for a yes or a no");
    assert_eq!(calls(root.path()), 1);
    leave(&mut p);
    let mut reopened = spawn(root.path());
    assert_eq!(calls(root.path()), 1, "reopening does not replay inference");
    assert_unknown_observed(root.path(), true);
    reopened.turn("yes\r", "nothing waits for a yes or a no");
    leave(&mut reopened);
    assert_eq!(calls(root.path()), 1);
    assert_unknown_observed(root.path(), true);
}

#[test]
fn typeahead_paste_and_stop_keep_unknown_exposure_without_consent() {
    // Enter while busy asks Stop and queues a correction. A paste followed
    // by Ctrl+C stays a draft. Neither path is a fresh spending/Save answer.
    for (queued, explicit_stop) in [
        ("yes\r", false),
        ("\x1b[200~yes\x1b[201~\r", false),
        ("\x1b[200~yes\x1b[201~\x03", true),
    ] {
        let root = new_root();
        std::fs::write(root.path().join("hold-response"), "wait for Stop").unwrap();
        let mut p = spawn(root.path());
        p.send("hello\r");
        wait_for_request(root.path());
        if explicit_stop {
            p.turn(queued, "preparation stopped;");
            // Submitting the retained pasted yes cannot revive any authority.
            p.turn("\r", "nothing waits for a yes or a no");
        } else {
            p.turn(queued, "nothing waits for a yes or a no");
        }
        assert!(p.screen.seen("preparation stopped;"));
        assert_eq!(calls(root.path()), 1, "Stop never retries the sent request");
        assert_unknown_observed(root.path(), false);
        leave(&mut p);
        assert_unknown_observed(root.path(), false);
        // Reopened with the request still unanswered, nothing is replayed:
        // the held transport would count a resent request and never answer it.
        let mut reopened = spawn(root.path());
        assert_eq!(
            calls(root.path()),
            1,
            "reopening does not replay the stopped request"
        );
        assert_unknown_observed(root.path(), false);
        // A yes after the reopen still answers nothing and revives no authority.
        reopened.turn("yes\r", "nothing waits for a yes or a no");
        leave(&mut reopened);
        assert_eq!(calls(root.path()), 1);
        assert_unknown_observed(root.path(), false);
    }
}
