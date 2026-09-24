// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real PTY → Live → retained child → host review → injected HTTP/account.
//! Hermetic mechanics only: no provider socket and no model qualification.
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]
use expectrl::{Eof, Expect};
use nika_cli_host::lane::{ChildSlot, RunProgress, drive_reviewed_child, run_args};
use nika_kernel::ai::provider::{InferRequest, Message, Role};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_session::ScriptedReasoner;
use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, UserIntelligencePreference,
};
use nika_tui::session::{Live, Runners};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

const MODEL: &str = "deepseek/s85-unpriced-fixture";
const SOURCE: &str = "nika: fresh-run\nmodel: deepseek/s85-unpriced-fixture\ntasks:\n  answer:\n    infer: { prompt: Say OK, max_tokens: 32 }\noutputs:\n  result: ${{ tasks.answer.output }}\n";
fn fixture_stdout(value: impl std::fmt::Display) {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{value}").unwrap();
    out.flush().unwrap();
}
fn fixture_root() -> Option<PathBuf> {
    let root = std::env::current_dir().unwrap();
    root.join(".s85-fixture").is_file().then_some(root)
}
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
        assert_eq!(body["model"], "s85-unpriced-fixture");
        assert_eq!(body["max_tokens"], 32);
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("http.ndjson"))
            .unwrap();
        writeln!(log, "{body}").unwrap();
        let uncertain = self.root.join("uncertain").exists();
        let body = if uncertain {
            r#"{"error":"possibly billed"}"#
        } else {
            r#"{"model":"s85-unpriced-fixture","id":"fixture","choices":[{"message":{"content":"OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"prompt_cache_hit_tokens":0,"prompt_cache_miss_tokens":10,"total_tokens":12}}"#
        };
        Ok(HttpResponse::new(
            if uncertain { 503 } else { 200 },
            BTreeMap::new(),
            body.as_bytes().to_vec().into(),
            request.url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("unknown-cost Run cannot stream a provider request")
    }
}

/// Invoked only by the PTY parent below, through this test executable.
#[test]
fn native_tui_fixture_child() {
    let Some(root) = fixture_root() else {
        return;
    };
    let source = std::fs::read_to_string(root.join("one.nika")).unwrap();
    let wf = nika_schema::parse(
        &source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .unwrap();
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
    let result = nika_cli_host::run_cost::review(
        &root,
        root.join("one.nika").to_str().unwrap(),
        &source,
        nika_types::id::ExecutionId::generate().to_string(),
        &wf,
        &plan,
        &BTreeMap::new(),
        Some(
            std::fs::read_to_string(root.join(".s85-invocation-default"))
                .unwrap()
                .parse()
                .unwrap(),
        ),
        nika_cli_host::run_cost::ReviewChannel::Stdio,
    );
    let cost = match result {
        Ok(Some(cost)) => cost,
        other => {
            let why = match other {
                Err(why) => why,
                _ => "missing cost choice".into(),
            };
            fixture_stdout(serde_json::json!({"error":{"message":why}}));
            std::process::exit(3);
        }
    };
    let http = Arc::new(FixtureHttp { root: root.clone() });
    let registry = ProviderRegistry::new(
        http,
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
    if let Err((error, _)) = &sent {
        fixture_stdout(serde_json::json!({"error":{"message":error.to_string()}}));
    }
    cost.finish().unwrap();
    fixture_stdout(serde_json::json!({"kind":"workflow_completed","fields":[
        {"key":"status","value":"fixture Run observed"}, {"key":"tasks_ok","value":u8::from(sent.is_ok())},
        {"key":"tasks_total","value":1}, {"key":"elapsed_ms","value":1}]}));
    // The real CLI settlement/proof path is independently exercised below.
    std::process::exit(if sent.is_ok() { 0 } else { 3 });
}

#[test]
fn native_tui_fixture_parent() {
    let Some(root) = fixture_root() else {
        return;
    };
    let mut census = IntelligenceCensus::empty();
    census.locals.push("ollama".into());
    let pref = UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "ollama".into(),
        },
        None,
    );
    let runners = Runners {
        run_once: Box::new(|_, _| panic!("fresh Run uses the reviewed lane")),
        run_resume: Box::new(|_, _, _, _| panic!("fixture has no resume")),
        run_tapped: None,
    };
    let slot: ChildSlot = Arc::default();
    let live = Live::new(
        root,
        census,
        Some(pref),
        None,
        Box::new(|_| Box::new(ScriptedReasoner::new(Vec::new()))),
        runners,
    )
    .with_run_review(Box::new(move |root, run, busy| {
        std::fs::write(
            root.join(".s85-invocation-default"),
            run.max_cost_usd.to_string(),
        )
        .unwrap();
        let exe = std::env::current_exe().unwrap();
        // The extra protocol argv is a transport negotiation; this fixture
        // shell does not consume it or grant authority. The child uses the
        // production review and observes its real controlling terminal.
        let args = vec![
            "-c".into(),
            "exec \"$1\" --exact native_tui_fixture_child --nocapture".into(),
            "fixture".into(),
            exe.display().to_string(),
        ];
        drive_reviewed_child(Path::new("/bin/sh"), &args, root, busy, &slot)
    }));
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
        .args(["--exact", "native_tui_fixture_parent", "--nocapture"])
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
    std::fs::write(dir.path().join(".s85-fixture"), "test executable only").unwrap();
    std::fs::write(dir.path().join("one.nika"), SOURCE).unwrap();
    dir
}
fn calls(root: &Path) -> usize {
    std::fs::read_to_string(root.join("http.ndjson"))
        .unwrap_or_default()
        .lines()
        .count()
}
fn ask(p: &mut LoggedPty) {
    p.send("run one.nika\r").unwrap();
    p.expect("Fresh").unwrap();
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
fn real_tui_yes_dispatches_once_and_observation_survives_without_authority() {
    let root = new_root();
    let mut p = spawn(root.path());
    ask(&mut p);
    assert_eq!(calls(root.path()), 0);
    p.send("details\r").unwrap();
    p.expect("Source").unwrap();
    p.expect("SHA-256").unwrap();
    p.expect("reply ›").unwrap();
    assert_eq!(calls(root.path()), 0);
    p.send("yes\r").unwrap();
    p.expect("fixture").unwrap();
    p.expect("observed").unwrap();
    p.expect("nika ›").unwrap();
    assert_eq!(calls(root.path()), 1);
    let text =
        std::fs::read_to_string(root.path().join(".nika/inference-cost-observations.ndjson"))
            .unwrap();
    let row: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(row["phase"], "settled");
    assert_eq!(row["observation"]["unknown_calls"], 1);
    assert!(row["observation"]["limit_nano_usd"].is_null());
    assert!(row["observation"]["unknown_attempts"][0]["estimated_nano_usd"].is_null());
    leave(&mut p);
    let mut p = spawn(root.path());
    p.send("yes\r").unwrap();
    p.expect("waits").unwrap();
    assert_eq!(calls(root.path()), 1);
    ask(&mut p);
    assert_eq!(calls(root.path()), 1);
    p.send("no\r").unwrap();
    p.expect("nothing").unwrap();
    p.expect("sent").unwrap();
    leave(&mut p);
}
#[test]
fn no_cancel_revision_changed_source_and_restart_send_nothing() {
    for response in ["no", "cancel"] {
        let root = new_root();
        let mut p = spawn(root.path());
        ask(&mut p);
        p.send(format!("{response}\r")).unwrap();
        p.expect("nothing").unwrap();
        p.expect("sent").unwrap();
        p.send("yes\r").unwrap();
        p.expect("waits").unwrap();
        assert_eq!(calls(root.path()), 0);
        leave(&mut p);
    }
    let root = new_root();
    let mut p = spawn(root.path());
    ask(&mut p);
    std::fs::write(
        root.path().join("one.nika"),
        SOURCE.replace("Say OK", "changed"),
    )
    .unwrap();
    p.send("yes\r").unwrap();
    p.expect("changed").unwrap();
    p.expect("review").unwrap();
    assert_eq!(calls(root.path()), 0);
    leave(&mut p);
    let root = new_root();
    let mut p = spawn(root.path());
    ask(&mut p);
    p.send("\x03").unwrap();
    p.expect("nothing").unwrap();
    p.expect("sent").unwrap();
    p.send("yes\r").unwrap();
    p.expect("waits").unwrap();
    assert_eq!(calls(root.path()), 0);
    leave(&mut p);
    let mut p = spawn(root.path());
    p.send("yes\r").unwrap();
    p.expect("waits").unwrap();
    assert_eq!(calls(root.path()), 0);
    leave(&mut p);
}
#[test]
fn an_unknown_cost_answer_keeps_the_review_pending_until_explicitly_declined() {
    let root = new_root();
    let mut p = spawn(root.path());
    ask(&mut p);
    p.send("change input to revised\r").unwrap();
    p.expect("not").unwrap();
    p.expect("nothing").unwrap();
    p.expect("sent").unwrap();
    p.expect("still").unwrap();
    p.expect("waits").unwrap();
    p.expect("reply ›").unwrap();
    assert_eq!(calls(root.path()), 0);
    // An unknown answer is neither a decline nor consent. Inspect the still-live
    // review, then explicitly decline it before testing a stale approval.
    // Sending yes here could approve the live review; matching its buffered
    // "waits" text would not prove that the approval was rejected.
    p.send("details\r").unwrap();
    p.expect("SHA-256").unwrap();
    p.expect("reply ›").unwrap();
    assert_eq!(calls(root.path()), 0);
    p.send("no\r").unwrap();
    p.expect("cancelled").unwrap();
    p.expect("declined").unwrap();
    p.expect("nika ›").unwrap();
    p.send("yes\r").unwrap();
    p.expect("waits").unwrap();
    assert_eq!(calls(root.path()), 0);
    leave(&mut p);
}
#[test]
fn uncertain_dispatch_blocks_a_new_run_without_automatic_retry() {
    let root = new_root();
    std::fs::write(root.path().join("uncertain"), "503").unwrap();
    let mut p = spawn(root.path());
    ask(&mut p);
    p.send("yes\r").unwrap();
    p.expect("fixture").unwrap();
    p.expect("observed").unwrap();
    p.expect("nika ›").unwrap();
    assert_eq!(calls(root.path()), 1);
    p.send("run one.nika\r").unwrap();
    p.expect("uncertain").unwrap();
    p.expect("billing").unwrap();
    p.expect("environment").unwrap();
    assert_eq!(calls(root.path()), 1);
    leave(&mut p);
}
#[test]
fn negotiated_deterministic_child_keeps_real_cli_settlement_and_no_question() {
    let root = new_root();
    std::fs::write(
        root.path().join("one.nika"),
        SOURCE.replace(MODEL, "mock/echo"),
    )
    .unwrap();
    let slot: ChildSlot = Arc::default();
    let (busy, _) = std::sync::mpsc::channel();
    let args = run_args(root.path(), Path::new("one.nika"), 0.25, &[]);
    let result = drive_reviewed_child(
        Path::new(env!("CARGO_BIN_EXE_nika")),
        &args,
        root.path(),
        &busy,
        &slot,
    );
    let RunProgress::Complete((code, trace, story)) = result else {
        panic!("deterministic Run asked for money")
    };
    assert_eq!(code, 0, "{story:?}");
    let trace = trace.unwrap();
    assert!(
        if trace.is_absolute() {
            trace
        } else {
            root.path().join(trace)
        }
        .is_file()
    );
    assert!(slot.lock().unwrap().is_none());
    assert_eq!(calls(root.path()), 0);
}

#[test]
fn typeahead_and_bracketed_paste_cannot_answer_a_not_yet_shown_question() {
    let root = new_root();
    let mut p = spawn(root.path());
    p.send("run one.nika\ryes\r").unwrap();
    p.expect("Fresh").unwrap();
    p.expect("decision").unwrap();
    p.expect("reply ›").unwrap();
    p.send("details\r").unwrap();
    p.expect("Source").unwrap();
    p.expect("SHA-256").unwrap();
    p.expect("reply ›").unwrap();
    assert_eq!(calls(root.path()), 0);
    p.send("\x1b[200~yes\x1b[201~").unwrap();
    p.send("\x03").unwrap();
    p.expect("nothing").unwrap();
    p.expect("sent").unwrap();
    assert_eq!(calls(root.path()), 0);
    leave(&mut p);
}

#[test]
fn zero_invocation_and_unnegotiated_json_refuse_before_transport() {
    let root = new_root();
    let mut p = spawn(root.path());
    p.send("run one.nika with a ceiling of 0\r").unwrap();
    p.expect("zero").unwrap();
    p.expect("ceiling").unwrap();
    p.expect("environment").unwrap();
    assert_eq!(calls(root.path()), 0);
    leave(&mut p);
    let output = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(["run", "one.nika", "--json", "--access", "api"])
        .current_dir(root.path())
        .env_clear()
        .env("HOME", root.path())
        .env("NIKA_DEEPSEEK_API_KEY", "fixture-not-a-key")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("nika/run-cost-challenge@1"), "{stdout}");
    assert!(stdout.contains("price unknown"), "{stdout}");
    assert!(
        !root
            .path()
            .join(".nika/inference-cost-observations.ndjson")
            .exists()
    );
}
