// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NIK-266 · an ask that reaches beyond its kind (network access, a wider harness sandbox)
//! is never inside a grant: the bridge pauses it, or applies the operator's bound answer.

use super::*;
use nika_kernel::ai::harness::HarnessEscalation;

/// An escalation ask event (NIK-266) whose reply closure records the verdict.
fn escalation(
    question: &str,
    escalation: HarnessEscalation,
    verdicts: Arc<Mutex<Vec<PermissionDecision>>>,
) -> HarnessEvent {
    HarnessEvent::EscalationAsked {
        question: question.to_owned(),
        reply: PermissionReply::new(Box::new(move |d| {
            verdicts.lock().expect("verdict lock").push(d);
        })),
        kind: Some("execute".to_owned()),
        command: Vec::new(),
        locations: Vec::new(),
        escalation,
    }
}

/// The judged witness frames of a run: `(plane, gate, decision, why)`.
fn judged(observer: &VecObserver) -> Vec<(&'static str, String, &'static str, String)> {
    let events = observer.0.lock().expect("observer lock");
    events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::PermissionJudged {
                plane,
                gate,
                decision,
                why,
            } => Some((*plane, gate.clone(), *decision, why.clone())),
            _ => None,
        })
        .collect()
}

/// NIK-266 · a network, sandbox-widening, MCP tool approval or unsandboxed command ask is
/// never inside `exec: true`: the run pauses with the harness's question verbatim, the
/// witness names what it reaches, and the reply lane was never answered (the reviewers'
/// `socks5Tcp`, `additionalPermissions` and `is_mcp_tool_approval` asks, and the gauntlet's
/// escalated Codex command, NIK-274, were answered `allow_once` before).
#[tokio::test]
async fn an_escalation_pauses_even_under_exec_true() {
    let socks = HarnessEscalation::new().with_network("socks5Tcp://exfil.example");
    let widened = HarnessEscalation::new().with_widens("write /");
    let mcp = HarnessEscalation::new().with_mcp("server web");
    let unsandboxed = HarnessEscalation::new().with_escalated_command(
        "curl -sS https://example.com — reason: Fetch example.com for its title",
    );
    for (title, asked, plane, gate) in [
        (
            "socks5Tcp network access to exfil.example",
            socks,
            "net",
            "network · socks5Tcp://exfil.example",
        ),
        ("Run command", widened, "exec", "sandbox · write /"),
        ("MCP tool call approval", mcp, "tools", "mcp · server web"),
        (
            "Run command: curl -sS https://example.com (reason: Fetch example.com for its title)",
            unsandboxed,
            "exec",
            "sandbox · command",
        ),
    ] {
        let verdicts = Arc::new(Mutex::new(Vec::new()));
        let tape = vec![
            escalation(title, asked, Arc::clone(&verdicts)),
            completed("never reached"),
        ];
        let mut input = AgentInput::new("try");
        input.permits = Some({
            let mut p = nika_schema::types::Permits::new();
            p.exec = Some(nika_schema::types::ExecPermit::Any);
            p
        });
        let observer = VecObserver::default();
        let err = run_on_harness(&seat_with(tape), input, &observer)
            .await
            .expect_err("an escalation pauses the run");
        let VerbAgentError::HarnessGate { question, .. } = &err else {
            panic!("the gate error is the pause branch, got {err:?}");
        };
        assert_eq!(question, title, "the question rides verbatim");
        assert!(
            verdicts.lock().expect("verdict lock").is_empty(),
            "never answered"
        );
        let frames = judged(&observer);
        assert_eq!(frames.len(), 1, "{frames:?}");
        assert_eq!(
            (frames[0].0, frames[0].1.as_str(), frames[0].2),
            (plane, gate, "escalate")
        );
    }
}

/// NIK-266 · the operator's bound answer still decides an escalation, once.
#[tokio::test]
async fn the_operator_answers_an_escalation() {
    let verdicts = Arc::new(Mutex::new(Vec::new()));
    let asked = HarnessEscalation::new().with_network("https://x.sh");
    let tape = vec![
        escalation("https network access to x.sh", asked, Arc::clone(&verdicts)),
        completed("done"),
    ];
    let mut input = AgentInput::new("try");
    input.gate_answer = Some(serde_json::Value::Bool(true));
    let observer = VecObserver::default();
    run_on_harness(&seat_with(tape), input, &observer)
        .await
        .expect("an answered escalation completes the run");
    assert_eq!(
        verdicts.lock().expect("verdict lock").as_slice(),
        &[PermissionDecision::AllowOnce]
    );
    let frames = judged(&observer);
    assert_eq!(frames.len(), 1, "{frames:?}");
    assert_eq!((frames[0].0, frames[0].2), ("net", "allow"));
    assert!(frames[0].3.contains("operator granted"), "{frames:?}");
    // The answer is reported spent, once, before its verdict: the runtime never hands it to
    // a replayed attempt of the task (the NIK-275 review).
    let events = observer.0.lock().expect("observer lock");
    let spent: Vec<usize> = (events.iter().enumerate())
        .filter(|(_, e)| matches!(e, AgentEvent::GateAnswerSpent))
        .map(|(at, _)| at)
        .collect();
    let judged_at = (events.iter())
        .position(|e| matches!(e, AgentEvent::PermissionJudged { .. }))
        .expect("the verdict is witnessed");
    assert_eq!(spent.len(), 1, "{events:?}");
    assert!(spent[0] < judged_at, "{events:?}");
}

/// B5 · an ask inside the grants spends no answer: only an ask the answer decides does.
#[tokio::test]
async fn an_ask_inside_the_grants_spends_no_answer() {
    let verdicts = Arc::new(Mutex::new(Vec::new()));
    let reply_verdicts = Arc::clone(&verdicts);
    let tape = vec![
        HarnessEvent::PermissionAsked {
            question: "run `git`".to_owned(),
            reply: PermissionReply::new(Box::new(move |d| {
                reply_verdicts.lock().expect("verdict lock").push(d);
            })),
            kind: Some("execute".to_owned()),
            locations: Vec::new(),
            command: vec!["git".to_owned(), "status".to_owned()],
            url: None,
        },
        completed("done"),
    ];
    let mut input = AgentInput::new("try");
    input.permits = Some({
        let mut p = nika_schema::types::Permits::new();
        p.exec = Some(nika_schema::types::ExecPermit::Any);
        p
    });
    input.gate_answer = Some(serde_json::Value::Bool(true));
    let observer = VecObserver::default();
    run_on_harness(&seat_with(tape), input, &observer)
        .await
        .expect("an ask inside the grants completes the run");
    assert_eq!(
        verdicts.lock().expect("verdict lock").as_slice(),
        &[PermissionDecision::AllowOnce]
    );
    let events = observer.0.lock().expect("observer lock");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::GateAnswerSpent)),
        "{events:?}"
    );
}
