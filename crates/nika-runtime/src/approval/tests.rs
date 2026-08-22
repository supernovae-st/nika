// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! F-P4 tests — the ticket law, unit-plane (book · digest · TTL) and
//! end-to-end over the mock seams (the dossier's fixtures (a)–(f)).
//!
//! The harness mirrors `crate::pause`'s: the builtin dispatcher is an
//! L1.5 sideways dep, so the mock tool executor PLAYS the prompt (a
//! queued `ToolResult` is the human's answer; a PROMPT-001 error result
//! is the blocked non-interactive ask).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

use nika_event::EventKind;
use nika_kernel::tool_executor::{ToolErrorMeta, ToolResult};
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::Value;

use super::*;
use crate::resume::{PriorSuccess, ResumePlan, fields};
use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};

// ─── unit plane · the ticket + the book ─────────────────────────────

fn ticket(step: &str) -> ApprovalTicket {
    ApprovalTicket::new(
        "a".repeat(64),
        "00000000-0000-0000-0000-000000000001".to_owned(),
        step.to_owned(),
        1_000_000,
        APPROVAL_TTL_SECONDS,
    )
}

fn resolved(value: Value) -> crate::task::SettleAs {
    crate::task::SettleAs::Ran(Box::new(crate::task::RanTask {
        decisions: Vec::new(),
        note: "invoke · nika:prompt".to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        evidence: None,
        duration_ms: 0,
        result: crate::task::RunResult::Success {
            value,
            tokens: None,
            recovered_from: None,
            warning: None,
            child: None,
            cost_usd: None,
            cost_unpriced: None,
            model: None,
        },
    }))
}

#[test]
fn the_digest_binds_the_mint_fields_and_ignores_the_decision() {
    let mut a = ticket("ask");
    let digest = a.digest().expect("digest");
    assert_eq!(digest.len(), 64, "blake3 hex");
    // The decision is the ticket's USE, never its identity: deciding
    // leaves the digest byte-identical (the pause's shown digest equals
    // the resume's signed digest — WYSIWYS).
    a.decision = ApprovalDecision::Allow;
    assert_eq!(a.digest().as_deref(), Some(digest.as_str()));
    // Every MINT field binds: another step · another nonce · another
    // mint · another TTL · another content — each a new capability.
    for changed in [
        ApprovalTicket::new(
            "b".repeat(64),
            a.run_nonce.clone(),
            a.step.clone(),
            a.minted_at_ms,
            a.ttl_seconds,
        ),
        ApprovalTicket::new(
            a.content_hash.clone(),
            "ffffffff-0000-0000-0000-000000000000".to_owned(),
            a.step.clone(),
            a.minted_at_ms,
            a.ttl_seconds,
        ),
        ApprovalTicket::new(
            a.content_hash.clone(),
            a.run_nonce.clone(),
            "other".to_owned(),
            a.minted_at_ms,
            a.ttl_seconds,
        ),
        ApprovalTicket::new(
            a.content_hash.clone(),
            a.run_nonce.clone(),
            a.step.clone(),
            a.minted_at_ms + 1,
            a.ttl_seconds,
        ),
        ApprovalTicket::new(
            a.content_hash.clone(),
            a.run_nonce.clone(),
            a.step.clone(),
            a.minted_at_ms,
            60,
        ),
    ] {
        assert_ne!(
            changed.digest().as_deref(),
            Some(digest.as_str()),
            "a mint field escaped the digest"
        );
    }
}

#[test]
fn the_ttl_window_is_sudo_shaped() {
    let t = ticket("ask"); // minted at 1_000_000 ms · ttl 900 s
    assert!(!t.is_expired(1_000_000), "fresh");
    assert_eq!(t.ttl_remaining_seconds(1_000_000), 900);
    assert!(!t.is_expired(1_000_000 + 899_000), "the last live second");
    assert_eq!(t.ttl_remaining_seconds(1_000_000 + 899_000), 1);
    assert!(t.is_expired(1_000_000 + 900_000), "the boundary is stale");
    assert_eq!(t.ttl_remaining_seconds(1_000_000 + 1_000_000), 0);
}

#[test]
fn the_book_mints_counts_and_refuses_the_sixth_distinct() {
    let book = ApprovalBook::new();
    book.begin_run(
        &nika_schema::parse(
            "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses"),
        "nonce".to_owned(),
    );
    for i in 0..APPROVAL_MAX_TICKETS_PER_RUN {
        let hash = format!("{i:064x}");
        let admit = book.admit("ask", "confirm", &hash, 0, None, "builtin");
        assert!(matches!(admit, Admit::Run { .. }), "mint {i} runs");
    }
    let admit = book.admit("ask", "confirm", &"f".repeat(64), 0, None, "builtin");
    let Admit::Refused(r) = admit else {
        panic!("the sixth distinct mint is refused");
    };
    assert_eq!(r.attestation.why, Some("approval.rate_limited"));
    assert_eq!(r.attestation.decision, "deny");
    assert!(r.detail.contains(APPROVAL_CODE), "{}", r.detail);
    // A SIXTH distinct step is refused; a SIXTH mint of KNOWN content
    // still passes (the dedup path is not the storm).
    let admit = book.admit("ask", "confirm", &format!("{:064x}", 0), 0, None, "builtin");
    assert!(matches!(admit, Admit::Run { .. }), "known content re-runs");
}

#[test]
fn the_book_consumes_a_decided_ticket_instead_of_replaying_it() {
    let book = ApprovalBook::new();
    book.begin_run(
        &nika_schema::parse(
            "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses"),
        "nonce".to_owned(),
    );
    let hash = "a".repeat(64);
    // Mint + decide (the attest path records the answer for dedup).
    let Admit::Run { .. } = book.admit("one", "confirm", &hash, 0, None, "builtin") else {
        panic!("the first mint runs");
    };
    let mut settle = resolved(Value::Bool(true));
    let att = book
        .attest_outcome("one", &mut settle, 1_000)
        .expect("a resolved ask attests");
    assert_eq!(att.decision, "allow");
    assert_eq!(att.source, "builtin");
    // A decision consumes the capability. Even if a caller presents the
    // same content hash again inside the TTL, the recorded answer cannot
    // bind a second task: fresh consent and a fresh ticket are required.
    let Admit::Run { bind } = book.admit("two", "confirm", &hash, 2_000, None, "builtin") else {
        panic!("the second ask re-mints");
    };
    assert_eq!(bind, None, "the consumed answer never replays");
    let entry_digest = book
        .ticket_for("two")
        .and_then(|t| t.digest())
        .expect("the fresh ticket");
    assert_ne!(entry_digest, att.digest.clone().expect("digest"));
    // The second ask resolves independently, never as `dedup`.
    let att2 = book
        .attest_outcome("two", &mut settle, 3_000)
        .expect("the fresh decision attests");
    assert_eq!(att2.decision, "allow");
    assert_eq!(att2.source, "builtin");
    // A later use also re-mints (fresh consent · it counts).
    let stale_at = 1_000_000 + 901_000;
    let Admit::Run { bind } = book.admit("three", "confirm", &hash, stale_at, None, "builtin")
    else {
        panic!("the stale ticket re-mints");
    };
    assert_eq!(bind, None, "the stale answer never replays");
    let fresh = book.ticket_for("three").expect("the fresh ticket");
    assert_eq!(fresh.minted_at_ms, stale_at, "a NEW capability issued");
}

#[test]
fn the_first_terminal_decision_is_immutable() {
    let book = ApprovalBook::new();
    book.begin_run(
        &nika_schema::parse(
            "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses"),
        "nonce".to_owned(),
    );
    let hash = "e".repeat(64);
    let Admit::Run { .. } = book.admit("ask", "confirm", &hash, 0, None, "builtin") else {
        panic!("the ticket mints");
    };

    let mut denied = resolved(Value::Bool(false));
    let first = book
        .attest_outcome("ask", &mut denied, 1)
        .expect("the refusal settles");
    assert_eq!(first.decision, "deny");
    assert!(matches!(
        denied,
        crate::task::SettleAs::Ran(ref ran)
            if matches!(ran.result, crate::task::RunResult::Failed { .. })
    ));

    // A duplicate/racing success cannot rewrite the already terminal deny.
    let mut late_allow = resolved(Value::Bool(true));
    let second = book
        .attest_outcome("ask", &mut late_allow, 2)
        .expect("the duplicate observes the terminal");
    assert_eq!(second.decision, "deny");
    assert!(matches!(
        late_allow,
        crate::task::SettleAs::Ran(ref ran)
            if matches!(ran.result, crate::task::RunResult::Failed { .. })
    ));
}

#[test]
fn the_book_validates_the_resumed_ticket_laws() {
    let book = ApprovalBook::new();
    book.begin_run(
        &nika_schema::parse(
            "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses"),
        "nonce-b".to_owned(),
    );
    let hash = "c".repeat(64);
    // cross-run: the ticket's nonce ≠ the trace's nonce → refused.
    book.set_paused(Some(PausedApproval::new(
        ApprovalTicket::new(hash.clone(), "nonce-a".to_owned(), "ask".to_owned(), 0, 900),
        "nonce-else".to_owned(),
    )));
    let admit = book.admit(
        "ask",
        "confirm",
        &hash,
        1_000,
        Some(&Value::Bool(true)),
        "cli",
    );
    let Admit::Refused(r) = admit else {
        panic!("a cross-run replay is refused");
    };
    assert_eq!(r.attestation.why, Some("approval.scope_mismatch"));
    // content mismatch: the resolved hash ≠ the shown hash → refused.
    book.set_paused(Some(PausedApproval::new(
        ApprovalTicket::new(hash.clone(), "nonce-a".to_owned(), "ask".to_owned(), 0, 900),
        "nonce-a".to_owned(),
    )));
    let admit = book.admit(
        "ask",
        "confirm",
        &"d".repeat(64),
        1_000,
        Some(&Value::Bool(true)),
        "cli",
    );
    let Admit::Refused(r) = admit else {
        panic!("a content mismatch is refused");
    };
    assert_eq!(r.attestation.why, Some("approval.content_mismatch"));
    assert!(
        r.attestation.digest.is_some(),
        "the refused ticket's digest rides the deny"
    );
    // expired: the stale answer does NOT bind — a fresh mint re-asks.
    book.set_paused(Some(PausedApproval::new(
        ApprovalTicket::new(hash.clone(), "nonce-a".to_owned(), "ask".to_owned(), 0, 900),
        "nonce-a".to_owned(),
    )));
    let admit = book.admit(
        "ask",
        "confirm",
        &hash,
        901_000,
        Some(&Value::Bool(true)),
        "cli",
    );
    let Admit::Run { bind } = admit else {
        panic!("the expired ticket re-mints");
    };
    assert_eq!(bind, None, "the stale answer is dropped (re-prompt)");
    // valid: the answer binds against the SHOWN ticket · no new mint.
    book.set_paused(Some(PausedApproval::new(
        ApprovalTicket::new(hash.clone(), "nonce-a".to_owned(), "ask".to_owned(), 0, 900),
        "nonce-a".to_owned(),
    )));
    let admit = book.admit(
        "ask",
        "confirm",
        &hash,
        60_000,
        Some(&Value::Bool(true)),
        "cli",
    );
    let Admit::Run { bind } = admit else {
        panic!("the valid ticket binds");
    };
    assert_eq!(bind, Some(Value::Bool(true)));
    let kept = book.ticket_for("ask").expect("the resumed ticket rides");
    assert_eq!(kept.run_nonce, "nonce-a", "the SHOWN ticket, not a re-mint");
    assert_eq!(kept.minted_at_ms, 0);
}

#[test]
fn a_paused_capability_clone_admits_exactly_one_runtime() {
    let workflow = nika_schema::parse(
        "nika: t\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n",
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parses");
    let hash = "e".repeat(64);
    let paused = PausedApproval::new(
        ApprovalTicket::new(hash.clone(), "nonce-a".to_owned(), "ask".to_owned(), 0, 900),
        "nonce-a".to_owned(),
    );
    let first = ApprovalBook::new();
    first.begin_run(&workflow, "nonce-a".to_owned());
    first.set_paused(Some(paused.clone()));
    let second = ApprovalBook::new();
    second.begin_run(&workflow, "nonce-a".to_owned());
    second.set_paused(Some(paused));

    assert!(matches!(
        first.admit(
            "ask",
            "confirm",
            &hash,
            1_000,
            Some(&Value::Bool(true)),
            "cli",
        ),
        Admit::Run { .. }
    ));
    let Admit::Refused(refusal) = second.admit(
        "ask",
        "confirm",
        &hash,
        1_000,
        Some(&Value::Bool(true)),
        "cli",
    ) else {
        panic!("the cloned capability must be single-use across runtimes");
    };
    assert_eq!(refusal.attestation.why, Some("approval.replayed"));
    assert!(refusal.detail.contains(APPROVAL_CODE));
}

#[test]
fn the_closure_stops_at_the_nearest_gate() {
    let wf = nika_schema::parse(
        "nika: t\npermits: { exec: [\"echo\"], tools: [\"nika:prompt\"] }\ntasks:\n  first:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"one?\" }\n  second:\n    after: { first: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"two?\" }\n  act:\n    after: { second: success }\n    exec: { command: [\"echo\", \"x\"] }\n",
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parses");
    let closures = gated_closures(&wf);
    assert!(
        closures["first"].is_empty(),
        "the first gate's closure stops AT the second gate: {:?}",
        closures["first"]
    );
    assert_eq!(closures["second"].len(), 1);
    assert_eq!(closures["second"][0].task, "act");
    assert_eq!(closures["second"][0].classes, vec!["exec"]);
}

// ─── the mock-seam harness (the pause.rs precedent) ─────────────────

struct Seams {
    shell: Vec<&'static str>,
    tool: Vec<ToolResult>,
    pause: bool,
    plan: Option<ResumePlan>,
    answers: BTreeMap<String, Value>,
    paused: Option<PausedApproval>,
}

fn str_field<'a>(event: &'a nika_event::Event, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|kv| kv.key == key)
        .and_then(|kv| match &kv.value {
            FieldValue::String(s) => Some(s.as_str()),
            _ => None,
        })
}

/// Fold a sink's stream back into a resume plan (what the CLI does).
fn plan_from(sink: &VecSink) -> ResumePlan {
    let mut plan = ResumePlan::new();
    for event in sink.events() {
        if event.kind != EventKind::TaskCompleted {
            continue;
        }
        let (Some(task), Some(def), Some(input), Some(output)) = (
            str_field(event, "task"),
            str_field(event, fields::DEF_HASH),
            str_field(event, fields::INPUT_HASH),
            str_field(event, fields::OUTPUT),
        ) else {
            continue;
        };
        let Ok(output) = serde_json::from_str(output) else {
            continue;
        };
        plan.insert(
            task.to_owned(),
            PriorSuccess::new(def.to_owned(), input.to_owned(), output),
        );
    }
    plan
}

/// Fold a sink's `workflow_paused` frame back into the F-P4 resume
/// authority (the nika-dap fold's twin — same fields, same last-wins).
fn paused_from(sink: &VecSink) -> PausedApproval {
    let event = sink
        .events()
        .iter()
        .rev()
        .find(|e| e.kind == EventKind::WorkflowPaused)
        .expect("the run paused");
    let int = |key: &str| {
        event
            .fields
            .iter()
            .find(|kv| kv.key == key)
            .and_then(|kv| match kv.value {
                FieldValue::Int(v) => Some(v),
                _ => None,
            })
            .unwrap_or_else(|| panic!("the pause frame carries {key}"))
    };
    let nonce = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .map(|e| e.id.uuid.to_string())
        .expect("the run started");
    PausedApproval::new(
        ApprovalTicket::new(
            str_field(event, "approval_shown_hash")
                .expect("shown hash")
                .to_owned(),
            str_field(event, "approval_nonce")
                .expect("nonce")
                .to_owned(),
            str_field(event, "task").expect("task").to_owned(),
            int("approval_minted_at_ms"),
            u32::try_from(int("approval_ttl_seconds")).expect("ttl"),
        ),
        nonce,
    )
}

/// One `approval_decided` frame, projected to the fields the fixtures
/// judge.
#[derive(Debug)]
struct ApprovalFrame {
    task: String,
    decision: String,
    source: String,
    shown_hash: String,
    digest: Option<String>,
    why: Option<String>,
}

/// The `approval_decided` frames of a sink, projected.
fn decisions(sink: &VecSink) -> Vec<ApprovalFrame> {
    sink.events()
        .iter()
        .filter(|e| e.kind == EventKind::ApprovalDecided)
        .map(|e| ApprovalFrame {
            task: str_field(e, "task").unwrap_or("").to_owned(),
            decision: str_field(e, "decision").unwrap_or("").to_owned(),
            source: str_field(e, "source").unwrap_or("").to_owned(),
            shown_hash: str_field(e, "shown_hash").unwrap_or("").to_owned(),
            digest: str_field(e, "digest").map(str::to_owned),
            why: str_field(e, "why").map(str::to_owned),
        })
        .collect()
}

async fn run_gated(yaml: &str, seams: Seams) -> (RunOutcome, VecSink) {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "the fixture checks clean");
    let mut shell = MockShell::new();
    for out in seams.shell {
        shell = shell.enqueue_ok(out);
    }
    let mut tools = MockToolExecutor::new();
    for result in seams.tool {
        tools = tools.enqueue_ok(result);
    }
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    let mut runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell)),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
    .with_prompt_pause(seams.pause)
    .with_prompt_answers(seams.answers)
    .with_paused_approval(seams.paused);
    if let Some(plan) = seams.plan {
        runtime = runtime.with_resume_plan(plan);
    }
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    (outcome, sink)
}

fn blocked_prompt() -> ToolResult {
    let mut result = ToolResult::error("tc1", "non-interactive and no `default:`");
    result.error_meta = Some(ToolErrorMeta::new(
        Some("NIKA-BUILTIN-PROMPT-001".to_owned()),
        false,
    ));
    result
}

// ─── (a) the storm — the 6ᵗʰ distinct prompt is the typed HALT ──────

#[tokio::test]
async fn fixture_a_the_sixth_distinct_prompt_halts_typed() {
    let mut yaml = String::from("nika: storm\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n");
    for i in 1..=6 {
        let after = if i == 1 {
            String::new()
        } else {
            format!("    after: {{ q{}: success }}\n", i - 1)
        };
        write!(
            yaml,
            "  q{i}:\n{after}    invoke:\n      tool: \"nika:prompt\"\n      args: {{ mode: \"confirm\", message: \"question {i}?\" }}\n"
        )
        .expect("writing to a String is infallible");
    }
    let (outcome, sink) = run_gated(
        &yaml,
        Seams {
            shell: vec![],
            tool: (0..5).map(|_| ToolResult::success("tc", "true")).collect(),
            pause: true,
            plan: None,
            answers: BTreeMap::new(),
            paused: None,
        },
    )
    .await;
    assert!(!outcome.ok, "the run halts on the sixth distinct mint");
    let failure = &outcome.records["q6"];
    let error = failure.error.as_ref().expect("the typed refusal");
    assert_eq!(error.code, APPROVAL_CODE);
    assert!(error.message.contains("approval.rate_limited"), "{error:?}");
    // Five allows attested, then the typed deny — never a queue.
    let ds = decisions(&sink);
    assert_eq!(ds.len(), 6, "{ds:?}");
    assert!(
        ds[..5]
            .iter()
            .all(|d| d.decision == "allow" && d.source == "builtin" && d.why.is_none())
    );
    let refuse = &ds[5];
    assert_eq!(refuse.task, "q6");
    assert_eq!(refuse.decision, "deny");
    assert_eq!(refuse.source, "engine");
    assert_eq!(refuse.why.as_deref(), Some("approval.rate_limited"));
    assert!(refuse.digest.is_none(), "no ticket was issued to sign");
    assert!(
        sink.events()
            .iter()
            .any(|e| e.kind == EventKind::WorkflowFailed)
    );
}

// ─── (b) content mismatch — the answer signs what was never shown ───

const MISMATCH_WF: &str = "nika: gated\npermits: { exec: [\"echo\"], tools: [\"nika:prompt\"] }\ntasks:\n  prep:\n    exec: { command: [\"echo\", \"STATE\"] }\n  ask:\n    after: { prep: success }\n    with: { state: \"${{ tasks.prep.output }}\" }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"input\", message: \"ship ${{ with.state }}?\" }\n  finish:\n    after: { ask: success }\n    exec: { command: [\"echo\", \"done\"] }\n";

#[tokio::test]
async fn fixture_b_a_stale_answer_halts_with_content_mismatch() {
    // Run A — the human was shown « ship ready? » and the run paused.
    let (paused_outcome, sink_a) = run_gated(
        &MISMATCH_WF.replace("STATE", "ready"),
        Seams {
            shell: vec!["ready\n"],
            tool: vec![blocked_prompt()],
            pause: true,
            plan: None,
            answers: BTreeMap::new(),
            paused: None,
        },
    )
    .await;
    assert!(paused_outcome.paused.is_some());
    let paused = paused_from(&sink_a);

    // Run B — upstream CHANGED (« ship changed? » now): the `--answer`
    // signs content the human was never shown → HALT + the finding.
    let (outcome, sink_b) = run_gated(
        &MISMATCH_WF.replace("STATE", "changed"),
        Seams {
            shell: vec!["changed\n"],
            tool: vec![],
            pause: true,
            plan: Some(plan_from(&sink_a)),
            answers: BTreeMap::from([("ask".to_owned(), Value::String("yes".to_owned()))]),
            paused: Some(paused),
        },
    )
    .await;
    assert!(!outcome.ok, "the mismatched answer halts the run");
    let error = outcome.records["ask"]
        .error
        .as_ref()
        .expect("the typed refusal");
    assert_eq!(error.code, APPROVAL_CODE);
    assert!(
        error.message.contains("approval.content_mismatch"),
        "{error:?}"
    );
    let ds = decisions(&sink_b);
    assert_eq!(ds.len(), 1, "{ds:?}");
    let deny = &ds[0];
    assert_eq!(deny.task, "ask");
    assert_eq!(deny.decision, "deny");
    assert_eq!(deny.source, "engine");
    assert_eq!(deny.why.as_deref(), Some("approval.content_mismatch"));
    assert_eq!(
        deny.digest.as_deref(),
        sink_a
            .events()
            .iter()
            .find(|e| e.kind == EventKind::WorkflowPaused)
            .and_then(|e| str_field(e, "approval_digest")),
        "the deny signs the SHOWN ticket's digest"
    );
    // The gated action never ran (the refusal cascades).
    assert!(
        !sink_b
            .events()
            .iter()
            .any(|e| e.kind == EventKind::TaskStarted && str_field(e, "task") == Some("finish")),
        "the gated action never starts"
    );
}

// ─── (c) the TTL — expired = re-prompt · cross-run = refusal ────────

#[tokio::test]
async fn fixture_c_an_expired_ticket_re_prompts_and_a_cross_run_replay_is_refused() {
    const WF: &str = "nika: gated\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"input\", message: \"proceed?\" }\n";
    // Run A pauses and mints.
    let (paused_outcome, sink_a) = run_gated(
        WF,
        Seams {
            shell: vec![],
            tool: vec![blocked_prompt()],
            pause: true,
            plan: None,
            answers: BTreeMap::new(),
            paused: None,
        },
    )
    .await;
    assert!(paused_outcome.paused.is_some());
    let mut paused = paused_from(&sink_a);

    // TTL — the ticket is 16 minutes stale: the `--answer` does NOT
    // bind; the run re-asks (pauses again) under a FRESH ticket.
    let stale = ApprovalTicket::new(
        paused.ticket.content_hash.clone(),
        paused.ticket.run_nonce.clone(),
        paused.ticket.step.clone(),
        paused.ticket.minted_at_ms - 16 * 60 * 1000,
        paused.ticket.ttl_seconds,
    );
    paused.ticket = stale;
    let (repaused, sink_b) = run_gated(
        WF,
        Seams {
            shell: vec![],
            tool: vec![blocked_prompt()],
            pause: true,
            plan: None,
            answers: BTreeMap::from([("ask".to_owned(), Value::String("yes".to_owned()))]),
            paused: Some(paused.clone()),
        },
    )
    .await;
    let p = repaused.paused.as_ref().expect("expired = re-prompt");
    let fresh = p.approval.as_ref().expect("the fresh ticket rides");
    assert!(
        fresh.minted_at_ms > paused.ticket.minted_at_ms,
        "a NEW capability was issued for the re-ask"
    );
    assert!(
        !sink_b
            .events()
            .iter()
            .any(|e| e.kind == EventKind::ApprovalDecided),
        "no decision — the stale answer never bound"
    );

    // Cross-run — the ticket's nonce names another run: refused.
    let foreign = PausedApproval::new(
        ApprovalTicket::new(
            paused.ticket.content_hash.clone(),
            "ffffffff-ffff-ffff-ffff-ffffffffffff".to_owned(),
            paused.ticket.step.clone(),
            paused.ticket.minted_at_ms,
            paused.ticket.ttl_seconds,
        ),
        paused.trace_nonce.clone(),
    );
    let (outcome, sink_c) = run_gated(
        WF,
        Seams {
            shell: vec![],
            tool: vec![],
            pause: true,
            plan: None,
            answers: BTreeMap::from([("ask".to_owned(), Value::String("yes".to_owned()))]),
            paused: Some(foreign),
        },
    )
    .await;
    assert!(!outcome.ok, "a cross-run replay halts");
    let ds = decisions(&sink_c);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].why.as_deref(), Some("approval.scope_mismatch"));
}

// ─── (e) single-use — identical asks consume distinct tickets ───────

#[tokio::test]
async fn fixture_e_identical_prompts_require_fresh_consent() {
    const WF: &str = "nika: dedup\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  one:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"same question?\" }\n  two:\n    after: { one: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"same question?\" }\n";
    let (outcome, sink) = run_gated(
        WF,
        Seams {
            shell: vec![],
            tool: vec![
                ToolResult::success("tc1", "true"),
                ToolResult::success("tc2", "true"),
            ],
            pause: true,
            plan: None,
            answers: BTreeMap::new(),
            paused: None,
        },
    )
    .await;
    assert!(outcome.ok, "both independently approved asks complete");
    let ds = decisions(&sink);
    assert_eq!(ds.len(), 2, "{ds:?}");
    assert_eq!(ds[0].task, "one");
    assert_eq!(ds[0].decision, "allow");
    assert_eq!(ds[1].task, "two");
    assert_eq!(ds[1].decision, "allow", "the twin was re-approved");
    assert_eq!(ds[1].source, "builtin");
    assert_ne!(
        ds[0].shown_hash, ds[1].shown_hash,
        "step identity binds the ask"
    );
    assert_ne!(
        ds[0].digest, ds[1].digest,
        "a consumed ticket is single-use"
    );
}

/// A confirm refusal is an authority failure, not successful boolean data.
/// A later identical gate reached through `terminal` must ask independently;
/// the first denial can never be replayed as approval (or as a green task).
#[tokio::test]
async fn a_refusal_fails_the_task_and_cannot_authorize_the_next_gate() {
    const WF: &str = "nika: deny-is-terminal\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  denied:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"ship?\" }\n  independent:\n    after: { denied: terminal }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"ship?\" }\n";
    let (outcome, sink) = run_gated(
        WF,
        Seams {
            shell: vec![],
            tool: vec![
                ToolResult::success("tc1", "false").with_structured(Value::Bool(false)),
                ToolResult::success("tc2", "true").with_structured(Value::Bool(true)),
            ],
            pause: true,
            plan: None,
            answers: BTreeMap::new(),
            paused: None,
        },
    )
    .await;

    assert!(!outcome.ok, "one authority denial keeps the workflow red");
    assert_eq!(outcome.records["denied"].status, crate::TaskStatus::Failure);
    assert_eq!(
        outcome.records["denied"]
            .error
            .as_ref()
            .expect("typed authority failure")
            .code,
        APPROVAL_CODE
    );
    assert_eq!(
        outcome.records["independent"].status,
        crate::TaskStatus::Success,
        "terminal reaches a new independently answered gate"
    );
    let ds = decisions(&sink);
    assert_eq!(ds.len(), 2, "{ds:?}");
    assert_eq!(ds[0].decision, "deny");
    assert_eq!(ds[1].decision, "allow");
    assert_ne!(ds[0].digest, ds[1].digest, "the denial was consumed");
}

#[tokio::test]
async fn automated_answers_never_claim_human_provenance() {
    let cases = [
        (
            "nika: policy\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"ship?\", default: true }\n",
            BTreeMap::new(),
            "policy",
        ),
        (
            "nika: cli\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"ship?\" }\n",
            BTreeMap::from([("ask".to_owned(), Value::Bool(true))]),
            "cli",
        ),
        (
            "nika: unverified-adapter\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"ship?\" }\n",
            BTreeMap::new(),
            "builtin",
        ),
    ];
    for (yaml, answers, expected) in cases {
        let (outcome, sink) = run_gated(
            yaml,
            Seams {
                shell: vec![],
                tool: vec![ToolResult::success("tc", "true").with_structured(Value::Bool(true))],
                pause: true,
                plan: None,
                answers,
                paused: None,
            },
        )
        .await;
        assert!(outcome.ok, "{expected} answer completes");
        let ds = decisions(&sink);
        assert_eq!(ds.len(), 1, "{ds:?}");
        assert_eq!(ds[0].source, expected);
        assert_ne!(ds[0].source, "human", "unverified automation is not human");
    }
}

// ─── (f) green — the honest gate, end to end, attested ──────────────

#[tokio::test]
async fn fixture_f_the_honest_gate_binds_and_attests() {
    // Run A — prep settles, the prompt blocks: paused with the ticket.
    let (paused_outcome, sink_a) = run_gated(
        &MISMATCH_WF.replace("STATE", "ready"),
        Seams {
            shell: vec!["ready\n"],
            tool: vec![blocked_prompt()],
            pause: true,
            plan: None,
            answers: BTreeMap::new(),
            paused: None,
        },
    )
    .await;
    let pause = paused_outcome.paused.as_ref().expect("the run paused");
    let ticket = pause.approval.as_ref().expect("the mint rides the pause");
    assert_eq!(ticket.step, "ask");
    assert_eq!(ticket.ttl_seconds, APPROVAL_TTL_SECONDS);
    assert_eq!(ticket.decision, ApprovalDecision::Pending);
    let pause_frame = sink_a
        .events()
        .iter()
        .find(|e| e.kind == EventKind::WorkflowPaused)
        .expect("the pause frame");
    let shown = str_field(pause_frame, "approval_shown_hash")
        .expect("the shown hash")
        .to_owned();
    let digest = str_field(pause_frame, "approval_digest")
        .expect("the digest")
        .to_owned();

    // Run B — resumed WITH the answer against the SAME bytes: prep
    // cache-hits, the ticket validates, the run completes, attested.
    let (outcome, sink_b) = run_gated(
        &MISMATCH_WF.replace("STATE", "ready"),
        Seams {
            shell: vec!["done\n"],
            tool: vec![ToolResult::success("tc1", "yes")],
            pause: true,
            plan: Some(plan_from(&sink_a)),
            answers: BTreeMap::from([("ask".to_owned(), Value::String("yes".to_owned()))]),
            paused: Some(paused_from(&sink_a)),
        },
    )
    .await;
    assert!(outcome.ok, "the validated answer completes the run");
    assert_eq!(outcome.cache_hits, vec!["prep".to_owned()]);
    assert!(
        sink_b
            .events()
            .iter()
            .any(|e| e.kind == EventKind::WorkflowCompleted)
    );
    let ds = decisions(&sink_b);
    assert_eq!(ds.len(), 1, "{ds:?}");
    let allow = &ds[0];
    assert_eq!(allow.task, "ask");
    assert_eq!(allow.decision, "allow");
    assert_eq!(allow.source, "resume", "the authority is the paused ticket");
    assert_eq!(allow.shown_hash, shown, "montré = signé");
    assert_eq!(
        allow.digest.as_deref(),
        Some(digest.as_str()),
        "one ticket digest"
    );
    assert!(allow.why.is_none());
}
