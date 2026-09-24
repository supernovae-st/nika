// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! PTY → real Live/TUI → retained child transport → host review → real Runtime,
//! real infer/invoke verbs and filesystem builtins, with only HTTP mocked.
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
use nika_cli_host::lane::{ChildSlot, drive_reviewed_child};
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

fn fixture_stdout(value: impl std::fmt::Display) {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{value}").unwrap();
    out.flush().unwrap();
}
const MODEL: &str = "deepseek/s87-unpriced-fixture";
const INPUT: &str = "alpha café\r\nbeta\n";
const ONE: &str = r#"nika: compound-run
model: deepseek/s87-unpriced-fixture
permits:
  tools: ["nika:read", "nika:write", "nika:jq"]
  fs: { read: ["./input.txt"], write: ["./output.txt"] }
tasks:
  read:
    invoke: { tool: "nika:read", args: { path: "./input.txt" } }
  first:
    with: { text: "${{ tasks.read.output }}" }
    infer:
      prompt: "SUMMARIZE:\n${{ with.text }}"
      max_tokens: 32
  write:
    with: { text: "${{ tasks.first.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./output.txt", content: "${{ with.text }}" }
outputs:
  answer: ${{ tasks.first.output }}
"#;
const SECOND: &str = r#"  second:
    with: { text: "${{ tasks.first.output }}" }
    infer:
      prompt: "REFINE:\n${{ with.text }}"
      max_tokens: 32
"#;
fn two(skipped: bool) -> String {
    let second = if skipped {
        SECOND.replace("    infer:", "    when: false\n    infer:")
    } else {
        SECOND.into()
    };
    let mut source = ONE.replace(
        "  write:\n",
        &format!("{second}  write:\n    after: {{ second: terminal }}\n"),
    );
    if !skipped {
        source = source.replace("  write:\n    after: { second: terminal }\n    with: { text: \"${{ tasks.first.output }}\" }", "  write:\n    after: { second: terminal }\n    with: { text: \"${{ tasks.second.output }}\" }");
    }
    source
}
fn fixture_root() -> Option<PathBuf> {
    let root = std::env::current_dir().unwrap();
    root.join(".s87-fixture").is_file().then_some(root)
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
        assert_eq!(body["model"], "s87-unpriced-fixture");
        assert_eq!(body["max_tokens"], 32);
        assert!(request.url.starts_with("https://api.deepseek.com/"));
        let call = calls(&self.root) + 1;
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("http.ndjson"))
            .unwrap();
        writeln!(log, "{body}").unwrap();
        let uncertain = self.root.join(format!("uncertain-{call}")).exists();
        let response = serde_json::json!({"model":"s87-unpriced-fixture", "id":format!("fixture-{call}"),
            "choices":[{"message":{"content":format!("SUMMARY-{call}\n")},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":10,"completion_tokens":3,"prompt_cache_hit_tokens":0,
                "prompt_cache_miss_tokens":10,"total_tokens":13}});
        Ok(HttpResponse::new(
            if uncertain { 503 } else { 200 },
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
fn compound_fixture_child() {
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
    let registry = Arc::new(
        ProviderRegistry::new(
            Arc::new(FixtureHttp {
                root: root.to_path_buf(),
            }),
            ProvidersConfig::new().with_key(
                "deepseek",
                nika_kernel::secret::Secret::new("fixture-not-a-key"),
            ),
        )
        .with_inference_admission(cost.config.inference_admission.clone().unwrap()),
    );
    let dispatcher = Arc::new(
        nika_builtin::BuiltinDispatcher::new(
            Arc::new(nika_fs::TokioFs),
            Arc::new(MockHttp::new()),
            Arc::new(MockClock::new()),
            Arc::new(nika_builtin::NullEmitter::default()),
            Arc::new(nika_builtin::NonInteractive::default()),
            Arc::new(nika_builtin::NoWorkflow::default()),
        )
        .with_fs_boundary(nika_runtime::compose::fs_boundary_of(wf)),
    );
    let invoke = Arc::new(nika_verb_invoke::InvokeVerb::new(dispatcher));
    let runtime = nika_runtime::Runtime::new(
        nika_verb_exec::ExecVerb::new(Arc::new(MockShell::new())),
        invoke.clone(),
        nika_verb_infer::InferVerb::new(registry.clone(), MODEL),
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
    if root.join("attempt-extra").exists() {
        use nika_kernel::ai::provider::{InferRequest, Message, Role};
        let mut extra =
            InferRequest::new(MODEL, vec![Message::text(Role::User, "unauthorized extra")]);
        extra.max_tokens = Some(32);
        assert!(
            executor
                .block_on(registry.resolve(MODEL).unwrap().infer_reported(extra))
                .is_err()
        );
    }
    cost.finish().unwrap();
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
    // A settled workflow failure is exit 1; exit 3 is a host/admission refusal.
    std::process::exit(if ok {
        0
    } else if outcome.is_ok() {
        1
    } else {
        3
    });
}
#[test]
fn compound_fixture_parent() {
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
            root.join(".s87-invocation-default"),
            run.max_cost_usd.to_string(),
        )
        .unwrap();
        let exe = std::env::current_exe().unwrap();
        // The extra protocol argv is a transport negotiation; this fixture
        // shell does not consume it or grant authority. The child uses the
        // production review and observes its real controlling terminal.
        let args = vec![
            "-c".into(),
            "exec \"$1\" --exact compound_fixture_child --nocapture".into(),
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
        .args(["--exact", "compound_fixture_parent", "--nocapture"])
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
    std::fs::write(root.path().join(".s87-fixture"), "fixture executable only").unwrap();
    std::fs::write(root.path().join("one.nika"), source).unwrap();
    std::fs::write(root.path().join("input.txt"), INPUT).unwrap();
    root
}
fn calls(root: &Path) -> usize {
    std::fs::read_to_string(root.join("http.ndjson"))
        .unwrap_or_default()
        .lines()
        .count()
}
fn ask(p: &mut LoggedPty, bound: u32) {
    p.send("run one.nika\r").unwrap();
    p.expect("Fresh").unwrap();
    p.expect("decision").unwrap();
    p.expect("At").unwrap();
    p.expect("most").unwrap();
    p.expect(bound.to_string()).unwrap();
    p.expect("requests").unwrap();
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
fn observation(root: &Path) -> serde_json::Value {
    let text =
        std::fs::read_to_string(root.join(".nika/inference-cost-observations.ndjson")).unwrap();
    serde_json::from_str(text.lines().last().unwrap()).unwrap()
}
fn wire(root: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(root.join("http.ndjson"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn assert_unspent(root: &Path) {
    assert_eq!(calls(root), 0);
    assert!(!root.join("output.txt").exists());
    assert!(!root.join("events.ndjson").exists());
}
fn assert_observation(root: &Path, bound: u64, observed: u64) {
    let row = observation(root);
    assert_eq!(row["phase"], "settled");
    assert_eq!(row["observation"]["unknown_calls"], observed);
    assert!(row["observation"]["limit_nano_usd"].is_null());
    for attempt in row["observation"]["unknown_attempts"].as_array().unwrap() {
        assert_eq!(attempt["choice"]["max_requests"], bound);
        assert!(attempt["estimated_nano_usd"].is_null());
    }
}

#[test]
fn read_infer_write_uses_exact_bytes_and_one_physical_call() {
    let root = new_root(ONE);
    let mut p = spawn(root.path());
    ask(&mut p, 1);
    assert_unspent(root.path());
    answer(&mut p, true);
    assert_eq!(calls(root.path()), 1);
    assert_eq!(
        wire(root.path())[0]["messages"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["content"],
        format!("SUMMARIZE:\n{INPUT}")
    );
    assert_eq!(
        std::fs::read(root.path().join("output.txt")).unwrap(),
        b"SUMMARY-1\n"
    );
    assert_observation(root.path(), 1, 1);
    let trace = std::fs::read_to_string(root.path().join("events.ndjson")).unwrap();
    assert!(trace.contains("task_completed"), "{trace}");
    assert!(
        trace.contains("permit_checked"),
        "filesystem authority still witnessed: {trace}"
    );
    assert!(
        trace.contains("workflow_completed"),
        "real Runtime settlement: {trace}"
    );
    leave(&mut p);
}

#[test]
fn two_sequential_infers_have_bound_two_and_live_account_refuses_a_third() {
    let root = new_root(&two(false));
    std::fs::write(
        root.path().join("attempt-extra"),
        "fixture probes account after runtime",
    )
    .unwrap();
    let mut p = spawn(root.path());
    ask(&mut p, 2);
    assert_unspent(root.path());
    answer(&mut p, true);
    assert_eq!(calls(root.path()), 2);
    assert_eq!(
        wire(root.path())[1]["messages"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["content"],
        "REFINE:\nSUMMARY-1\n"
    );
    assert_eq!(
        std::fs::read(root.path().join("output.txt")).unwrap(),
        b"SUMMARY-2\n"
    );
    assert_observation(root.path(), 2, 2);
    leave(&mut p);
}

#[test]
fn skipped_infer_is_a_reviewed_upper_bound_not_an_observed_call() {
    let root = new_root(&two(true));
    let mut p = spawn(root.path());
    ask(&mut p, 2);
    answer(&mut p, true);
    assert_eq!(calls(root.path()), 1);
    assert_eq!(
        std::fs::read(root.path().join("output.txt")).unwrap(),
        b"SUMMARY-1\n"
    );
    assert_observation(root.path(), 2, 1);
    let trace = std::fs::read_to_string(root.path().join("events.ndjson")).unwrap();
    assert!(trace.contains("task_skipped"), "{trace}");
    leave(&mut p);
}

#[test]
fn decline_and_changed_source_input_or_configuration_make_no_call_or_output() {
    for change in ["no", "source", "input", "configuration"] {
        let root = new_root(ONE);
        let mut p = spawn(root.path());
        ask(&mut p, 1);
        match change {
            "source" => std::fs::write(
                root.path().join("one.nika"),
                ONE.replace("SUMMARIZE", "CHANGED"),
            )
            .unwrap(),
            "input" => std::fs::write(root.path().join("input.txt"), "changed bytes").unwrap(),
            "configuration" => std::fs::write(
                root.path().join("nika.yaml"),
                "nika: changed-project\nceiling: 3.0\n",
            )
            .unwrap(),
            _ => {}
        }
        if change == "no" {
            answer(&mut p, false);
        } else {
            p.send("yes\r").unwrap();
            p.expect("changed").unwrap();
            p.expect("nika ›").unwrap();
        }
        assert_unspent(root.path());
        leave(&mut p);
    }
}

#[test]
fn repeated_run_and_restart_require_distinct_live_reviews() {
    let root = new_root(ONE);
    let mut p = spawn(root.path());
    ask(&mut p, 1);
    answer(&mut p, true);
    let first = observation(root.path())["invocation"].clone();
    ask(&mut p, 1);
    assert_eq!(calls(root.path()), 1);
    answer(&mut p, true);
    assert_eq!(calls(root.path()), 2);
    assert_ne!(observation(root.path())["invocation"], first);
    leave(&mut p);
    let mut p = spawn(root.path());
    p.send("yes\r").unwrap();
    p.expect("waits").unwrap();
    assert_eq!(calls(root.path()), 2);
    ask(&mut p, 1);
    answer(&mut p, false);
    assert_eq!(calls(root.path()), 2);
    leave(&mut p);
}

#[test]
fn cancelling_pending_compound_run_kills_the_child_without_effects() {
    let root = new_root(ONE);
    let mut p = spawn(root.path());
    ask(&mut p, 1);
    p.send("\x03").unwrap();
    p.expect("nothing").unwrap();
    p.expect("sent").unwrap();
    assert_unspent(root.path());
    leave(&mut p);
}

#[test]
fn uncertain_first_call_stops_second_and_journal_blocks_replay() {
    let root = new_root(&two(false));
    std::fs::write(root.path().join("uncertain-1"), "503 possibly billed").unwrap();
    let mut p = spawn(root.path());
    ask(&mut p, 2);
    answer(&mut p, true);
    assert_eq!(calls(root.path()), 1);
    assert!(!root.path().join("output.txt").exists());
    let row = observation(root.path());
    assert_eq!(row["observation"]["state"], "Uncertain");
    assert_eq!(row["observation"]["unknown_calls"], 1);
    p.send("run one.nika\r").unwrap();
    p.expect("uncertain").unwrap();
    p.expect("billing").unwrap();
    p.expect("environment").unwrap();
    assert_eq!(calls(root.path()), 1);
    assert!(
        std::fs::read_to_string(root.path().join("refusal.txt"))
            .unwrap()
            .contains("uncertain")
    );
    leave(&mut p);
}

#[test]
fn unsupported_shapes_refuse_before_any_model_or_output_effect() {
    let variants = [
        ONE.replace("    infer:\n", "    retry: { max_attempts: 2, backoff_ms: 1 }\n    infer:\n"),
        ONE.replace("    infer:\n", "    for_each: { items: [a, b] }\n    infer:\n"),
        ONE.replace("    infer:\n", "    agent:\n").replace("max_tokens:", "max_tokens_total:"),
        ONE.replace("      max_tokens: 32", "      max_tokens: 32\n      model: openai/gpt-4o-mini"),
        ONE.replace("      max_tokens: 32", "      max_tokens: 32\n      schema: { type: string }"),
        ONE.replace("  write:\n", "  extra:\n    invoke: { tool: 'nika:fetch', args: { url: 'https://example.com/' } }\n  write:\n"),
        ONE.replace("  write:\n", "  extra:\n    invoke: { workflow: child.nika }\n  write:\n"),
    ];
    for (index, source) in variants.into_iter().enumerate() {
        // Missing fetch permission and missing nested source are rejected by
        // the normal Check door before the child; other cases reach review.
        let check_refuses = index >= 5;
        let root = new_root(&source);
        let mut p = spawn(root.path());
        p.send("run one.nika\r").unwrap();
        if check_refuses {
            p.expect("findings").unwrap();
            p.expect("started").unwrap();
        } else {
            p.expect("refused").unwrap();
            p.expect("environment").unwrap();
        }
        assert_unspent(root.path());

        leave(&mut p);
    }
}

#[test]
fn second_uncertain_call_preserves_the_completed_first_attempt_without_output() {
    let root = new_root(&two(false));
    std::fs::write(root.path().join("uncertain-2"), "503 possibly billed").unwrap();
    let mut p = spawn(root.path());
    ask(&mut p, 2);
    answer(&mut p, true);
    assert_eq!(calls(root.path()), 2);
    assert!(!root.path().join("output.txt").exists());
    let row = observation(root.path());
    assert_eq!(row["observation"]["state"], "Uncertain");
    assert_eq!(row["observation"]["unknown_calls"], 2);
    let attempts = row["observation"]["unknown_attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2);
    assert!(attempts[0]["usage"].is_object());
    assert!(attempts[1]["usage"].is_null());
    assert!(attempts.iter().all(|a| a["estimated_nano_usd"].is_null()));
    leave(&mut p);
}
