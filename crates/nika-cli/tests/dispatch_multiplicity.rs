// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! B12 · PTY → real Live/TUI → retained child transport → host review → real
//! Runtime and infer verb, with only HTTP mocked: a finite fan and an authored
//! retry are reviewed once and run inside the confirmed total and in-flight
//! bound; a decline, zero work and a count only the run decides send nothing.
//! No provider socket, billing qualification, or replacement workflow interpreter.
#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]
use expectrl::{Eof, Expect};
use nika_cli_host::lane::{ChildSlot, drive_reviewed_child_observed};
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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const MODEL: &str = "deepseek/b12-unpriced-fixture";
const FAN: &str = r#"nika: fan-run
model: deepseek/b12-unpriced-fixture
permits: {}
tasks:
  review:
    for_each: { items: [a, b, c], max_parallel: 2 }
    retry: { max_attempts: 2, backoff_ms: 1 }
    infer:
      prompt: "REVIEW ${{ item }}"
      max_tokens: 32
outputs:
  answer: ${{ tasks.review.output }}
"#;
const RETRY: &str = r#"nika: retry-run
model: deepseek/b12-unpriced-fixture
permits: {}
tasks:
  ask:
    retry: { max_attempts: 2, backoff_ms: 1 }
    infer:
      prompt: "ASK"
      max_tokens: 32
outputs:
  answer: ${{ tasks.ask.output }}
"#;

fn fixture_stdout(value: impl std::fmt::Display) {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{value}").unwrap();
    out.flush().unwrap();
}
fn fixture_root() -> Option<PathBuf> {
    let root = std::env::current_dir().unwrap();
    root.join(".b12-fixture").is_file().then_some(root)
}
/// The provider seam: each request is logged, held briefly so a fan's
/// concurrency is physical, and answered 200 unless `status-<n>` names the
/// n-th request's status. The widest concurrency it saw is kept.
struct FixtureHttp {
    root: PathBuf,
    in_flight: AtomicUsize,
    widest: AtomicUsize,
}
impl HttpPostDyn for FixtureHttp {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["model"], "b12-unpriced-fixture");
        assert_eq!(body["max_tokens"], 32);
        assert!(request.url.starts_with("https://api.deepseek.com/"));
        let call = calls(&self.root) + 1;
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("http.ndjson"))
            .unwrap();
        writeln!(log, "{body}").unwrap();
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.widest.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(60)).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        let status = std::fs::read_to_string(self.root.join(format!("status-{call}")))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(200);
        let response = serde_json::json!({"model":"b12-unpriced-fixture", "id":format!("fixture-{call}"),
            "choices":[{"message":{"content":format!("DONE-{call}")},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":10,"completion_tokens":3,"prompt_cache_hit_tokens":0,
                "prompt_cache_miss_tokens":10,"total_tokens":13}});
        Ok(HttpResponse::new(
            status,
            BTreeMap::new(),
            serde_json::to_vec(&response).unwrap().into(),
            request.url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("unknown-cost Run cannot stream a provider request")
    }
}
fn plan() -> nika_providers::ExecutionAccessPlan {
    use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
    let probe = ProviderProbe::new(
        "deepseek",
        true,
        true,
        "DEEPSEEK_API_KEY",
        false,
        ProviderReadiness::new(
            true,
            true,
            None,
            None,
            true,
            ExecutionLocus::Cloud,
            nika_types::access::AccessClass::Api,
        ),
        "https://api.deepseek.com",
    );
    nika_providers::resolve_execution_plan(
        &[nika_providers::ModelNeed::new(MODEL, true, false)],
        &[probe],
        Some("api"),
    )
}

/// Child of the actual TUI lane; only the integration-test executable enters here.
#[test]
fn dispatch_fixture_child() {
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
    let report = nika_check::check(&wf);
    let plan = plan();
    let reviewed = nika_cli_host::run_cost::review(
        &root,
        root.join("one.nika").to_str().unwrap(),
        &source,
        nika_types::id::ExecutionId::generate().to_string(),
        &wf,
        &plan,
        &BTreeMap::new(),
        Some(1.0),
        nika_cli_host::run_cost::ReviewChannel::Stdio,
    );
    let cost = match reviewed {
        Ok(Some(cost)) => cost,
        other => {
            let why = other
                .err()
                .unwrap_or_else(|| "no unknown-cost review".into());
            std::fs::write(root.join("refusal.txt"), &why).unwrap();
            fixture_stdout(serde_json::json!({"error":{"message":why}}));
            std::process::exit(3);
        }
    };
    execute_reviewed(&root, &wf, &report, plan, &cost);
}

fn execute_reviewed(
    root: &Path,
    wf: &nika_schema::raw::RawWorkflow,
    report: &nika_check::CheckReport,
    plan: nika_providers::ExecutionAccessPlan,
    cost: &nika_cli_host::run_cost::RunCost,
) {
    use nika_kernel_mock::{
        MockClock, MockHttp, MockProvider, MockShell, MockToolDefinitionProvider,
    };
    assert!(report.is_clean(), "{report:?}");
    let http = Arc::new(FixtureHttp {
        root: root.to_path_buf(),
        in_flight: AtomicUsize::new(0),
        widest: AtomicUsize::new(0),
    });
    let registry = Arc::new(
        ProviderRegistry::new(
            http.clone(),
            ProvidersConfig::new().with_key(
                "deepseek",
                nika_kernel::secret::Secret::new("fixture-not-a-key"),
            ),
        )
        .with_inference_admission(cost.config.inference_admission.clone().unwrap()),
    );
    let dispatcher = Arc::new(nika_builtin::BuiltinDispatcher::new(
        Arc::new(nika_fs::TokioFs),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(nika_builtin::NullEmitter::default()),
        Arc::new(nika_builtin::NonInteractive::default()),
        Arc::new(nika_builtin::NoWorkflow::default()),
    ));
    let invoke = Arc::new(nika_verb_invoke::InvokeVerb::new(dispatcher));
    let runtime = nika_runtime::Runtime::new(
        nika_verb_exec::ExecVerb::new(Arc::new(MockShell::new())),
        invoke.clone(),
        nika_verb_infer::InferVerb::new(registry, MODEL),
        nika_verb_agent::AgentVerb::new(
            Arc::new(MockProvider::new("unused")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            MODEL,
        ),
        MockClock::new(),
        cost.config.clone(),
    )
    .with_access_plan(plan);
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut sink = nika_runtime::VecSink::new();
    let outcome = executor.block_on(runtime.run(
        wf,
        report,
        &mut nika_runtime::DeterministicStamper::new(),
        &mut sink,
    ));
    cost.finish().unwrap();
    let widest = http.widest.load(Ordering::SeqCst).to_string();
    std::fs::write(root.join("widest.txt"), widest).unwrap();
    let events = sink.into_events();
    let trace = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(root.join("events.ndjson"), &trace).unwrap();
    fixture_stdout(trace);
    let ok = outcome.as_ref().is_ok_and(|o| o.ok);
    std::fs::write(root.join("outcome.txt"), format!("{outcome:?}")).unwrap();
    std::process::exit(if ok {
        0
    } else if outcome.is_ok() {
        1
    } else {
        3
    });
}
#[test]
fn dispatch_fixture_parent() {
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
    .with_run_review_observed(Box::new(move |root, _, busy| {
        let exe = std::env::current_exe().unwrap();
        let args = vec![
            "-c".into(),
            "exec \"$1\" --exact dispatch_fixture_child --nocapture".into(),
            "fixture".into(),
            exe.display().to_string(),
        ];
        drive_reviewed_child_observed(Path::new("/bin/sh"), &args, root, busy, &slot)
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
        .args(["--exact", "dispatch_fixture_parent", "--nocapture"])
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
fn new_root(source: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(".b12-fixture"), "fixture executable only").unwrap();
    std::fs::write(root.path().join("one.nika"), source).unwrap();
    root
}
fn calls(root: &Path) -> usize {
    std::fs::read_to_string(root.join("http.ndjson"))
        .unwrap_or_default()
        .lines()
        .count()
}
fn widest(root: &Path) -> usize {
    std::fs::read_to_string(root.join("widest.txt"))
        .unwrap()
        .parse()
        .unwrap()
}
/// The fresh question, read in its order through single-word needles.
fn ask(p: &mut LoggedPty, needles: &[&str]) {
    p.send("run one.nika\r").unwrap();
    for needle in ["Fresh", "decision"].iter().chain(needles) {
        p.expect(*needle).unwrap();
    }
    p.expect("reply ›").unwrap();
}
fn answer(p: &mut LoggedPty, yes: bool) {
    p.send(if yes { "yes\r" } else { "no\r" }).unwrap();
    if yes {
        p.expect("tasks").unwrap();
        p.expect("ms").unwrap();
    } else {
        p.expect("nothing").unwrap();
        p.expect("sent").unwrap();
    }
    p.expect("nika ›").unwrap();
}
fn leave(p: &mut LoggedPty) {
    p.send("\x03\x03").unwrap();
    p.expect(Eof).unwrap();
    p.get_process_mut().wait().unwrap();
}
/// The journal's settled row: its choice, attempts and state.
fn settled(root: &Path) -> serde_json::Value {
    let text =
        std::fs::read_to_string(root.join(".nika/inference-cost-observations.ndjson")).unwrap();
    let row: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(row["phase"], "settled");
    row["observation"].clone()
}
fn prepared_rows(root: &Path) -> usize {
    std::fs::read_to_string(root.join(".nika/inference-cost-observations.ndjson"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("\"prepared\""))
        .count()
}

#[test]
fn a_fan_with_authored_retries_is_reviewed_once_and_runs_inside_its_bounds() {
    let root = new_root(FAN);
    let mut p = spawn(root.path());
    let needles = [
        "At", "most", "6", "requests", "items", "attempts", "flight", "Task", "retries",
    ];
    ask(&mut p, &needles);
    assert_eq!(calls(root.path()), 0, "nothing sent before the answer");
    answer(&mut p, true);
    assert_eq!(
        calls(root.path()),
        3,
        "one request per item, all answered 200"
    );
    assert_eq!(
        widest(root.path()),
        2,
        "the reviewed width, physically reached"
    );
    let observation = settled(root.path());
    let choice = &observation["unknown_cost"];
    assert_eq!(choice["max_requests"], 6);
    assert_eq!(choice["max_in_flight"], 2);
    assert_eq!(choice["authored_retry"], true);
    assert_eq!(observation["unknown_attempts"].as_array().unwrap().len(), 3);
    let outcome = std::fs::read_to_string(root.path().join("outcome.txt")).unwrap();
    assert!(outcome.contains("ok: true"), "{outcome}");
    // No authority from the old approval: the next Run asks afresh.
    ask(&mut p, &["At", "most", "6", "requests"]);
    answer(&mut p, false);
    assert_eq!(
        calls(root.path()),
        3,
        "a declined fresh review sends nothing"
    );
    leave(&mut p);
}

#[test]
fn an_undeclared_width_runs_every_item_at_once_inside_the_review() {
    let open = FAN
        .replace(", max_parallel: 2", "")
        .replace("    retry: { max_attempts: 2, backoff_ms: 1 }\n", "");
    let root = new_root(&open);
    let mut p = spawn(root.path());
    ask(&mut p, &["At", "most", "3", "requests", "items", "flight"]);
    answer(&mut p, true);
    assert_eq!(calls(root.path()), 3);
    assert_eq!(widest(root.path()), 3);
    let choice = settled(root.path())["unknown_cost"].clone();
    assert_eq!(choice["max_in_flight"], 3);
    assert!(choice.get("authored_retry").is_none(), "{choice}");
    leave(&mut p);
}

#[test]
fn a_received_429_is_retried_as_authored_and_a_500_stops_every_request() {
    for (status, sent, state) in [("429", 2, "Closed"), ("500", 1, "Uncertain")] {
        let root = new_root(RETRY);
        std::fs::write(root.path().join("status-1"), status).unwrap();
        let mut p = spawn(root.path());
        ask(
            &mut p,
            &["At", "most", "2", "requests", "attempts", "Task", "retries"],
        );
        answer(&mut p, true);
        assert_eq!(calls(root.path()), sent, "{status}");
        let observation = settled(root.path());
        assert_eq!(observation["state"], state, "{status}");
        assert_eq!(observation["unknown_calls"], sent, "{status}");
        let first = &observation["unknown_attempts"][0];
        assert!(
            first["estimated_nano_usd"].is_null(),
            "never priced as zero"
        );
        if status == "429" {
            assert_eq!(
                first["note"],
                "answered HTTP 429; usage and USD cost unknown"
            );
        }
        leave(&mut p);
    }
}

#[test]
fn a_decline_zero_work_and_a_run_decided_count_send_nothing() {
    let root = new_root(FAN);
    let mut p = spawn(root.path());
    ask(&mut p, &["At", "most", "6", "requests"]);
    answer(&mut p, false);
    assert_eq!((calls(root.path()), prepared_rows(root.path())), (0, 0));
    leave(&mut p);
    // Zero items buy no allowance: no question, and the Run completes.
    let root = new_root(&FAN.replace("[a, b, c]", "[]"));
    let mut p = spawn(root.path());
    p.send("run one.nika\r").unwrap();
    for needle in ["skipped", "tasks", "ms", "succeeded"] {
        p.expect(needle).unwrap();
    }
    assert_eq!((calls(root.path()), prepared_rows(root.path())), (0, 0));
    let outcome = std::fs::read_to_string(root.path().join("outcome.txt")).unwrap();
    assert!(outcome.contains("ok: true"), "{outcome}");
    leave(&mut p);
    // A count only the run decides is refused before any request.
    let upstream = FAN
        .replace("permits: {}", "permits: { tools: ['nika:jq'] }")
        .replace(
            "  review:\n    for_each: { items: [a, b, c], max_parallel: 2 }\n",
            "  load:\n    invoke: { tool: 'nika:jq', args: { input: [1, 2], expression: '.' } }\n  review:\n    with: { list: '${{ tasks.load.output }}' }\n    for_each: { items: '${{ with.list }}' }\n",
        );
    let root = new_root(&upstream);
    let mut p = spawn(root.path());
    p.send("run one.nika\r").unwrap();
    p.expect("refused").unwrap();
    p.expect("environment").unwrap();
    assert_eq!(calls(root.path()), 0);
    let refusal = std::fs::read_to_string(root.path().join("refusal.txt")).unwrap();
    assert!(
        refusal.contains("a count only the run decides"),
        "{refusal}"
    );
    leave(&mut p);
}
