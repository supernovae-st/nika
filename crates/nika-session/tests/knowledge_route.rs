// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

//! The real public session route to the native compiler and the Foundry knowledge, end to end
//! over a controlled seat: the session a host door opens (`SessionRuntime::open_with`, the
//! configuration read ONCE from the environment it was started with), the public turn adapter
//! (`turn` · `consent`), the provider registry over the ONE env boundary, the compiler's native
//! door, and a loopback seat on the OpenAI-compatible wire that keeps every byte it received.
//! The environment is the child process's own (a test cannot set one in-process): each scenario
//! re-runs this binary as a child that drives the session and writes a report; the parent holds
//! the seat and judges the report against the bytes the seat received.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::{
    CHANGE, Foundry, INTENT, LoopbackSeat, SEAT_MODEL, candidate, message, native_answer, sha256,
};
use nika_cli_host::compile::knowledge::Snapshot;
use nika_onboard::compile::revise_intent;
use serde_json::{Value, json};

/// One scenario's world: the project, the home, the snapshot, the seat, the report.
struct World {
    _dir: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    foundry: Foundry,
    report: PathBuf,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(root.join("a.md"), "Some notes worth rewriting.\n").unwrap();
    let foundry = Foundry::create(&dir.path().join("knowledge"));
    let report = dir.path().join("report.json");
    World {
        root,
        home,
        foundry,
        report,
        _dir: dir,
    }
}

/// Run one scenario in a child with this environment; the child's report.
fn run_child(world: &World, scenario: &str, seat: &LoopbackSeat, env: &[(&str, &str)]) -> Value {
    let log = world.report.with_extension("log");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &world.home)
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .env("S03_CHILD", scenario)
        .env("S03_ROOT", &world.root)
        .env("S03_FOUNDRY_ROOT", &world.foundry.root)
        .env("S03_REPORT", &world.report)
        .stdin(Stdio::null())
        .stdout(Stdio::from(std::fs::File::create(&log).unwrap()))
        .stderr(Stdio::from(
            std::fs::OpenOptions::new().append(true).open(&log).unwrap(),
        ));
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("the child scenario `{scenario}` did not finish within 180 s");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        status.success(),
        "child `{scenario}` failed:\n{}",
        std::fs::read_to_string(&log).unwrap_or_default()
    );
    serde_json::from_str(&std::fs::read_to_string(&world.report).unwrap()).unwrap()
}

/// The request as the compiler reads a revision of [`INTENT`] with [`CHANGE`].
fn revision_intent() -> String {
    revise_intent(
        &nika_onboard::compile::CompileRequest::edit("nika: x\ntasks: {}\n", CHANGE)
            .with_original_intent(INTENT),
    )
    .unwrap()
}

/// The digest of the pack the one knowledge door composes for `intent` from the snapshot.
fn expected_pack(foundry: &Foundry, intent: &str) -> nika_onboard::compile::AuthoringKnowledge {
    Snapshot::open(&foundry.snapshot)
        .unwrap()
        .pack(intent, None)
        .unwrap()
}

fn assert_presented(system: &str, pack: &nika_onboard::compile::AuthoringKnowledge) {
    assert!(!pack.references.is_empty());
    for reference in &pack.references {
        assert!(
            system.contains(&reference.text),
            "the seat's instruction carries `{}` byte for byte",
            reference.id
        );
    }
}

/// Round one: the pack composed for the request, every reference byte for byte in the
/// instruction the seat received; the receipt names that instruction's sha256 and the pack's.
fn assert_first_round(world: &World, body: &Value, details: &str, port: u16) {
    // The replayed candidate still names what authored it: the seat, its host, its usage.
    assert!(
        details.contains(&format!(
            "by {SEAT_MODEL} · host 127.0.0.1:{port} · 1 call in that round · 1000 in / 200 out tokens"
        )),
        "{details}"
    );
    let first = message(body, "system");
    let pack = expected_pack(&world.foundry, INTENT);
    assert_presented(&first, &pack);
    assert!(first.contains("S03-BLOCK-MARKER") && first.contains("S03-SKILL-MARKER"));
    assert!(
        details.contains(&format!("native · instruction sha256 {}", sha256(&first))),
        "{details}"
    );
    let pack_sha = pack.identity["door"]["pack_sha256"].as_str().unwrap();
    assert!(
        details.contains(&format!("pack sha256 {pack_sha}")),
        "{details}"
    );
    // The proposal came from the answer round, which replayed the native record: it carries the
    // knowledge of the round that authored the candidate, and called no one.
    assert!(
        details.contains(
            "presented to the seat in 1 call of the round that authored this candidate (this answer round replayed it · zero calls)"
        ),
        "{details}"
    );
    assert!(
        details.contains("authoring backend: none"),
        "the answer round itself made no call: {details}"
    );
    assert!(
        details.contains("knowledge: knowledge-s03 · declared digest digest-s03-a · manifest "),
        "{details}"
    );
    assert!(
        details.contains("authoring strategy: only (environment)"),
        "{details}"
    );
    for reference in &pack.references {
        assert!(
            details.contains(&format!(
                "{} {} · {} B · sha256 {}",
                reference.kind,
                reference.id,
                reference.text.len(),
                &sha256(&reference.text)[..12]
            )),
            "{details}"
        );
    }
    let opening: Value = serde_json::from_str(&message(body, "user")).unwrap();
    assert_eq!(opening["request"], INTENT);
}

/// The revision: the change read beside the request the proposal answered, the pack composed
/// for that same request from the SAME pinned snapshot, the base candidate beside it.
fn assert_revision(world: &World, body: &Value, details: &str, port: u16) {
    // The receipt says where the calls really went: the loopback seat, a base URL override.
    assert!(
        details.contains(&format!(
            "sent to: vllm · host 127.0.0.1:{port} (base URL overridden"
        )),
        "{details}"
    );
    let revised = revision_intent();
    let second = message(body, "system");
    let revision_pack = expected_pack(&world.foundry, &revised);
    assert_presented(&second, &revision_pack);
    let opening: Value = serde_json::from_str(&message(body, "user")).unwrap();
    assert_eq!(
        opening["request"],
        revised.as_str(),
        "the original intent is kept"
    );
    assert_eq!(opening["change"], CHANGE);
    assert!(
        opening["base_candidate"]
            .as_str()
            .is_some_and(|b| b.contains(&format!("model: {SEAT_MODEL}"))),
        "the base is the proposal's own bytes: {opening:#}"
    );
    assert!(
        details.contains("knowledge: knowledge-s03 · declared digest digest-s03-a · manifest "),
        "the revision keeps the pinned identity: {details}"
    );
    assert!(
        details.contains("presented to the seat in 1 call\n"),
        "the revision presented its own pack in its own call: {details}"
    );
    assert!(
        details.contains(&format!("native · instruction sha256 {}", sha256(&second))),
        "{details}"
    );
    let revision_sha = revision_pack.identity["door"]["pack_sha256"]
        .as_str()
        .unwrap();
    assert!(
        details.contains(&format!("pack sha256 {revision_sha}")),
        "{details}"
    );
}

#[test]
fn the_public_turn_presents_the_pinned_pack_and_the_receipt_names_the_bytes_the_seat_received() {
    let world = world();
    let seat = LoopbackSeat::start(vec![
        native_answer(&candidate("mock/echo", false)),
        native_answer(&candidate(SEAT_MODEL, true)),
    ]);
    let snapshot = world.foundry.snapshot.display().to_string();
    let report = run_child(
        &world,
        "route",
        &seat,
        &[
            ("NIKA_KNOWLEDGE", &snapshot),
            ("NIKA_AUTHORING_STRATEGY", "only"),
        ],
    );
    seat.shutdown();
    let bodies = seat.bodies();
    let kinds: Vec<&str> = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["question", "proposal", "proposal"],
        "the model question, the proposal, the revised proposal: {report:#}"
    );
    // Two calls, both native: `only` opens the native door at once; the answer round replays
    // (zero calls); no label call reached the seat (the door's classifier is the host's).
    assert_eq!(bodies.len(), 2, "{bodies:#?}");
    for body in &bodies {
        assert_eq!(
            body["model"], "s03-seat",
            "the human's model, never another"
        );
        assert!(
            body["response_format"].is_object(),
            "the native answer schema"
        );
    }
    assert_first_round(
        &world,
        &bodies[0],
        report["details_first"].as_str().unwrap(),
        seat.port,
    );
    assert_revision(
        &world,
        &bodies[1],
        report["details_second"].as_str().unwrap(),
        seat.port,
    );
    // The revision authored under exactly the identity the first round was: version, declared
    // digest, manifest bytes and rows.
    let identity = |details: &str| -> String {
        let line = details
            .lines()
            .find(|l| l.trim_start().starts_with("knowledge: "))
            .unwrap_or_default();
        line.split(" · ").take(4).collect::<Vec<_>>().join(" · ")
    };
    let first = identity(report["details_first"].as_str().unwrap());
    assert!(
        first.contains(" · manifest ") && first.contains(" · rows "),
        "{first}"
    );
    assert_eq!(first, identity(report["details_second"].as_str().unwrap()));
    // The seat never moved, a new proposal is a new identity, and nothing was written or run.
    assert_eq!(report["seat_before"], report["seat_after"]);
    assert_eq!(
        report["seat_before"],
        json!(format!("Provider {{ model: \"{SEAT_MODEL}\" }}"))
    );
    assert_ne!(report["steps"][1]["id"], report["steps"][2]["id"]);
    assert_eq!(report["pending_after"], report["steps"][2]["id"]);
    assert_eq!(
        report["project_files"],
        json!(["a.md"]),
        "a candidate is not authority: nothing lands before a consent"
    );
}

#[test]
fn the_default_strategy_escalates_and_only_the_native_door_reads_the_pack() {
    let world = world();
    // The private plan's answer is not a plan: the cold round ends without a candidate and the
    // native door opens (the CLI's default, escalate), with the pack beside the card.
    let seat = LoopbackSeat::start(vec![
        json!({"not": "a plan"}).to_string(),
        native_answer(&candidate("mock/echo", false)),
    ]);
    let snapshot = world.foundry.snapshot.display().to_string();
    let report = run_child(&world, "escalate", &seat, &[("NIKA_KNOWLEDGE", &snapshot)]);
    seat.shutdown();
    let bodies = seat.bodies();
    assert_eq!(
        bodies.len(),
        2,
        "one plan call, one native call: {bodies:#?}"
    );
    let plan = message(&bodies[0], "system");
    assert!(
        !plan.contains("S03-BLOCK-MARKER"),
        "the private plan never reads knowledge"
    );
    let native = message(&bodies[1], "system");
    assert_presented(&native, &expected_pack(&world.foundry, INTENT));
    let details = report["details_first"].as_str().unwrap();
    assert!(
        details.contains("authoring strategy: escalate (environment)"),
        "{details}"
    );
    assert!(
        details.contains("presented to the seat in 1 call"),
        "{details}"
    );
    assert!(
        details.contains(&format!("native · instruction sha256 {}", sha256(&native))),
        "{details}"
    );
}

#[test]
fn a_snapshot_that_goes_stale_under_the_session_refuses_the_revision_and_the_proposal_waits() {
    let world = world();
    let seat = LoopbackSeat::start(vec![native_answer(&candidate("mock/echo", false))]);
    let snapshot = world.foundry.snapshot.display().to_string();
    let report = run_child(
        &world,
        "stale",
        &seat,
        &[
            ("NIKA_KNOWLEDGE", &snapshot),
            ("NIKA_AUTHORING_STRATEGY", "only"),
        ],
    );
    seat.shutdown();
    assert_eq!(
        seat.bodies().len(),
        1,
        "the refused revision sent nothing to the seat"
    );
    let steps = report["steps"].as_array().unwrap();
    let last = steps.last().unwrap();
    assert_eq!(last["kind"], "refusal", "{report:#}");
    let text = last["text"].as_str().unwrap();
    assert!(
        text.contains("is stale") && text.contains("blocks/s03-transform.nika"),
        "{text}"
    );
    assert!(
        text.contains("nothing was sent to the authoring model"),
        "{text}"
    );
    assert_eq!(
        report["pending_after"], steps[1]["id"],
        "the proposal still waits, unchanged"
    );
}

#[test]
fn a_deterministic_session_calls_no_one_and_reads_no_pack_whatever_is_configured() {
    let world = world();
    let seat = LoopbackSeat::start(vec![native_answer(&candidate("mock/echo", false))]);
    let snapshot = world.foundry.snapshot.display().to_string();
    let report = run_child(
        &world,
        "deterministic",
        &seat,
        &[
            ("NIKA_KNOWLEDGE", &snapshot),
            ("NIKA_AUTHORING_STRATEGY", "only"),
        ],
    );
    seat.shutdown();
    assert!(
        seat.bodies().is_empty(),
        "no provider call: {:?}",
        seat.bodies()
    );
    let kinds: Vec<&str> = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["proposal", "facts"], "{report:#}");
    let details = report["details_first"].as_str().unwrap();
    assert!(details.contains("authoring backend: none"), "{details}");
    assert!(!details.contains("pack sha256"), "{details}");
    let status = report["status"].as_str().unwrap();
    assert!(status.contains("authoring · deterministic"), "{status}");
    assert!(
        status.contains("knowledge knowledge-s03"),
        "the configuration is stated (pinned at open), never composed nor presented by this seat: {status}"
    );
}

#[test]
fn a_configuration_the_session_cannot_honor_is_said_at_open_and_refused_at_the_first_seated_turn() {
    let world = world();
    let seat = LoopbackSeat::start(vec![native_answer(&candidate("mock/echo", false))]);
    let snapshot = world.foundry.snapshot.display().to_string();
    let report = run_child(
        &world,
        "misconfigured",
        &seat,
        &[
            ("NIKA_KNOWLEDGE", &snapshot),
            ("NIKA_AUTHORING_STRATEGY", "off"),
        ],
    );
    seat.shutdown();
    assert!(seat.bodies().is_empty(), "{:?}", seat.bodies());
    let banner = report["banner"].as_str().unwrap();
    assert!(
        banner.contains("⚠ authoring knowledge:") && banner.contains("never reads it"),
        "{banner}"
    );
    let step = &report["steps"][0];
    assert_eq!(step["kind"], "refusal", "{report:#}");
    assert!(
        step["text"].as_str().unwrap().contains("never reads it"),
        "{step}"
    );
}

/// The child: drive the session the scenario names, write the report. Runs only when a parent
/// started it (`S03_CHILD`).
#[test]
#[ignore = "run by the parent scenarios in a child process"]
fn child() {
    let Some(scenario) = std::env::var_os("S03_CHILD") else {
        return;
    };
    let scenario = scenario.to_string_lossy().into_owned();
    let root = PathBuf::from(std::env::var_os("S03_ROOT").unwrap());
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let report = child_drive(&scenario, &root, &home);
    std::fs::write(
        std::env::var_os("S03_REPORT").unwrap(),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
}

mod drive {
    use nika_session::intelligence::{
        IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence,
        UserIntelligencePreference,
    };
    use nika_session::reasoner::{NoReasoner, ProviderReasoner, SessionReasoner};
    use nika_session::runtime::{SessionRuntime, TurnOutcome};
    use nika_session::turn::{
        RoutingMethod, SessionPhase, TurnAct, TurnClassifier, TurnContext, TurnDecision,
    };
    use serde_json::{Value, json};
    use std::path::Path;

    /// The host's classifier: a line at a question answers it, a line at a proposal changes it.
    struct Scripted;

    impl TurnClassifier for Scripted {
        fn classify(&mut self, context: &TurnContext, _raw: &str) -> TurnDecision {
            let act = match context.phase {
                SessionPhase::QuestionPending => TurnAct::Answer,
                SessionPhase::ProposalPending => TurnAct::Modify,
                _ => TurnAct::NewWork,
            };
            TurnDecision::new(act, RoutingMethod::Model)
        }
    }

    pub(super) fn open(root: &Path, home: &Path, kind: IntelligenceKind) -> SessionRuntime {
        let mut census = IntelligenceCensus::empty();
        census.locals.push("vllm".to_owned());
        let model =
            matches!(kind, IntelligenceKind::Local { .. }).then(|| super::SEAT_MODEL.to_owned());
        let pref = UserIntelligencePreference::new(kind, model);
        let factory = Box::new(
            |resolved: &ResolvedSessionIntelligence| -> Box<dyn SessionReasoner> {
                match (&resolved.kind, &resolved.model) {
                    (IntelligenceKind::Local { provider }, Some(model)) => {
                        Box::new(ProviderReasoner {
                            model: model.clone(),
                            label: format!("{provider} · local"),
                        })
                    }
                    _ => Box::new(NoReasoner),
                }
            },
        );
        let mut session = SessionRuntime::open_with(root, census, &pref, Some(home), factory);
        session.with_classifier(Box::new(Scripted));
        session
    }

    pub(super) fn step(outcome: &TurnOutcome) -> Value {
        match outcome {
            TurnOutcome::Question { key, question } => {
                json!({"kind": "question", "key": key, "text": question})
            }
            TurnOutcome::Proposal { id, preview } => {
                json!({"kind": "proposal", "id": id.to_string(), "text": preview})
            }
            TurnOutcome::Refusal(refusal) => json!({"kind": "refusal", "text": refusal.text}),
            TurnOutcome::Facts(text) => json!({"kind": "facts", "text": text}),
            TurnOutcome::Held { id, preview } => {
                json!({"kind": "held", "id": id.to_string(), "text": preview})
            }
            other => json!({"kind": "other", "text": format!("{other:?}")}),
        }
    }
}

fn project_files(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
}

fn child_drive(scenario: &str, root: &Path, home: &Path) -> Value {
    use nika_session::intelligence::IntelligenceKind;
    let local = IntelligenceKind::Local {
        provider: "vllm".to_owned(),
    };
    let mut steps = Vec::new();
    match scenario {
        "route" | "escalate" | "stale" => {
            let mut session = drive::open(root, home, local);
            let seat_before = format!("{:?}", session.authoring_seat());
            steps.push(drive::step(&session.turn(INTENT)));
            // An empty line takes the model the human already chose.
            steps.push(drive::step(&session.turn("")));
            let details_first = session.details();
            if scenario == "stale" {
                let block = PathBuf::from(std::env::var_os("S03_FOUNDRY_ROOT").unwrap())
                    .join("blocks/s03-transform.nika");
                std::fs::write(block, "# edited after the export\n").unwrap();
            }
            if scenario != "escalate" {
                steps.push(drive::step(&session.consent(CHANGE)));
            }
            json!({
                "steps": steps,
                "details_first": details_first,
                "details_second": session.details(),
                "seat_before": seat_before,
                "seat_after": format!("{:?}", session.authoring_seat()),
                "pending_after": session.pending_proposal().map(|id| id.to_string()),
                "project_files": project_files(root),
            })
        }
        "deterministic" => {
            let mut session = drive::open(root, home, IntelligenceKind::None);
            steps.push(drive::step(
                &session.turn("Read ./notes/brief.md and write it to ./out/copy.md"),
            ));
            steps.push(drive::step(&session.turn(INTENT)));
            json!({
                "steps": steps,
                "details_first": session.details(),
                "status": session.status(),
            })
        }
        "misconfigured" => {
            let mut session = drive::open(root, home, local);
            let banner = session.banner();
            steps.push(drive::step(&session.turn(INTENT)));
            json!({"steps": steps, "banner": banner})
        }
        other => panic!("unknown scenario {other}"),
    }
}
