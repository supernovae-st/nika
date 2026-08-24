// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The EXTERNAL execution path (D-2026-08-04-N1 · P3 B4 · feature
//! `access-harness`, default OFF) — the `agent:` task delegated to the
//! user's own authenticated harness through the kernel's
//! [`DynAgentBackend`] seam.
//!
//! The native loop is untouched: without the feature this module does
//! not compile; with it but no seat configured, `run` still takes the
//! native loop. What this path never does (B4 honesty · refusals with
//! witnesses, not silent degradation):
//!
//! - a task `schema:` refuses — structured output on a harness is P4's
//!   capability attestation, never assumed;
//! - a `tools:` whitelist refuses — the harness runs its OWN tools;
//!   the enforceable boundary is the permission bridge (B5), and a
//!   declared whitelist the engine cannot enforce would be a lie;
//! - a permission ask is judged by the B5 AUTHORITY BRIDGE: inside the
//!   workflow's `permits:` grants it is answered `AllowOnce` (never
//!   `allow_always` — A-5) and witnessed; outside every grant the run
//!   PAUSES for the operator (NIKA-1806 · the ADR-099 durable gate's
//!   harness twin) unless a `--answer` verdict is already bound. No
//!   harness action escapes the engine's authority by default.

use std::sync::Arc;

use nika_cap::{HarnessAskFacts, HarnessGate, judge_harness_ask};
use nika_kernel::ai::harness::{
    DynAgentBackend, HarnessError, HarnessEvent, HarnessRequest, PermissionDecision,
    PermissionReply,
};
use nika_kernel::runtime::agent::AgentStopReason;

use crate::{AgentEvent, AgentInput, AgentObserver, AgentOutput, AgentValue, VerbAgentError};

/// The configured harness seat — the backend plus the session facts
/// the VERB cannot know (the composer owns cwd · the runtime owns the
/// permits bridge at B5).
#[derive(Clone)]
pub struct HarnessSeat {
    /// The erased backend (a [`SpawnedHarness`] in production ·
    /// scripted mocks in tests).
    ///
    /// [`SpawnedHarness`]: https://docs.rs/nika-harness
    pub backend: Arc<dyn DynAgentBackend>,
    /// The session's working directory (absolute · the composer's
    /// sandbox root in production).
    pub cwd: std::path::PathBuf,
}

impl std::fmt::Debug for HarnessSeat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HarnessSeat")
            .field("cwd", &self.cwd)
            .finish_non_exhaustive()
    }
}

impl HarnessSeat {
    /// Construct from a backend, reading the session cwd — the
    /// composer's one-liner (P3 B4.5).
    ///
    /// # Errors
    ///
    /// The process cwd is unreadable.
    pub fn from_backend(backend: Arc<dyn DynAgentBackend>) -> Result<Self, String> {
        let cwd = std::env::current_dir().map_err(|e| format!("harness seat: no cwd: {e}"))?;
        Ok(Self::new(backend, cwd))
    }

    /// Construct (INV-019).
    #[must_use]
    pub fn new(backend: Arc<dyn DynAgentBackend>, cwd: impl Into<std::path::PathBuf>) -> Self {
        Self {
            backend,
            cwd: cwd.into(),
        }
    }
}

/// Run the task on the harness seat — the external half of
/// `run_observed`.
pub(crate) async fn run_on_harness(
    seat: &HarnessSeat,
    input: AgentInput,
    observer: &dyn AgentObserver,
) -> Result<AgentOutput, VerbAgentError> {
    if input.schema.is_some() {
        return Err(VerbAgentError::InvalidParam {
            param: "schema",
            detail: "not attested on a harness access (P3) — structured output \
                     arrives with P4's capability attestation; drop the schema \
                     or pick an infer-grade access"
                .to_owned(),
        });
    }
    if !input.tools.is_empty() {
        return Err(VerbAgentError::InvalidParam {
            param: "tools",
            detail: "a whitelist is not enforceable on a harness access — the \
                     harness runs its OWN tools under the permission bridge; \
                     drop `tools:` or drop the harness pin"
                .to_owned(),
        });
    }

    let mut request = HarnessRequest::new(input.prompt.clone(), seat.cwd.clone());
    if let Some(system) = &input.system {
        request = request.with_system(system.clone());
    }
    if let Some(model) = &input.model {
        request = request.with_requested_model(model.clone());
    }
    // B5 · the operator's bound verdict is CONSUMED by the first
    // out-of-grants ask it decides: the human answered ONE question, so
    // a second ask (or a different one on a nondeterministic replay)
    // pauses again instead of riding a stale grant.
    let mut gate_answer = input.gate_answer.clone();

    let mut stream = seat
        .backend
        .run_agent_boxed(request)
        .await
        .map_err(|e| harness_err(&e))?;

    loop {
        let next = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await;
        match next {
            Some(Ok(HarnessEvent::PermissionAsked {
                question,
                reply,
                kind,
                locations,
                command,
                url,
            })) => {
                // B5 · the AUTHORITY BRIDGE (extracted under the
                // fn-length law): judged against the workflow's
                // declared `permits:` — answered ONCE and witnessed
                // inside, the operator's bound verdict outside, and
                // absent one the run PAUSES (the reply lane drops
                // unanswered: the harness hears `cancelled`).
                let facts = HarnessAskFacts::new()
                    .with_kind(kind)
                    .with_locations(locations)
                    .with_command(command)
                    .with_url(url);
                bridge_ask(
                    &facts,
                    question,
                    reply,
                    input.permits.as_ref(),
                    &mut gate_answer,
                    observer,
                )?;
            }
            Some(Ok(HarnessEvent::Completed { outcome })) => {
                return Ok(completed_output(*outcome));
            }
            Some(Ok(_)) => {
                // Chunks (the outcome carries the accumulated text ·
                // per-chunk observer taps arrive with B5) and any
                // future event kind: observed, never fatal.
            }
            Some(Err(e)) => return Err(harness_err(&e)),
            None => {
                return Err(harness_err(&HarnessError::Session {
                    reason: "the harness stream ended without a Completed beat".to_owned(),
                }));
            }
        }
    }
}

/// The witness `gate` label for one ask — what the bridge judged, in
/// one string (the program · the paths · the kind): an auditor reads
/// WHICH authority was exercised without re-parsing wire facts.
fn gate_label(facts: &HarnessAskFacts) -> String {
    let kind = facts.kind.as_deref().unwrap_or("<undeclared>");
    match kind {
        "execute" => match facts.command.first() {
            Some(program) => format!("execute · {program}"),
            None => "execute · <prose>".to_owned(),
        },
        "read" | "search" | "edit" | "delete" | "move" => {
            format!("{kind} · {}", facts.locations.join(","))
        }
        "fetch" => match &facts.url {
            Some(url) => format!("fetch · {url}"),
            None => "fetch · <no url>".to_owned(),
        },
        other => other.to_owned(),
    }
}

/// The terminal beat → the pre-shaped honest `AgentOutput` (extracted
/// under the fn-length law): usage stays harness-reported-or-zero ·
/// `model_resolved` stays None — the receipt records the REQUESTED
/// model; an observed identity is the trace's fact, never reconciled
/// here (A-2/A-7).
fn completed_output(outcome: nika_kernel::ai::harness::HarnessOutcome) -> AgentOutput {
    let mut out = AgentOutput::new(
        AgentValue::Text(outcome.output.clone()),
        AgentStopReason::Completed,
        1,
        outcome
            .usage
            .as_ref()
            .map_or(0, |u| u.input_tokens + u.output_tokens),
    );
    if let Some(usage) = outcome.usage {
        out.usage = usage;
    }
    out
}

/// Judge ONE ask and answer it (the B5 bridge's heart): inside the
/// grants → `AllowOnce` + the witness; outside → the operator's bound
/// verdict (CONSUMED on use) or the gate error that pauses the run.
/// Every decision rides the observer as `PermissionJudged` (NEP-0007's
/// harness row).
fn bridge_ask(
    facts: &HarnessAskFacts,
    question: String,
    reply: PermissionReply,
    permits: Option<&nika_schema::types::Permits>,
    gate_answer: &mut Option<serde_json::Value>,
    observer: &dyn AgentObserver,
) -> Result<(), VerbAgentError> {
    let gate = gate_label(facts);
    match judge_harness_ask(facts, permits) {
        HarnessGate::Inside { plane, why } => {
            observer.on_event(&AgentEvent::PermissionJudged {
                plane,
                gate,
                decision: "allow",
                why,
            });
            reply.respond(PermissionDecision::AllowOnce);
        }
        HarnessGate::Outside { plane, why } => {
            match gate_answer
                .take()
                .as_ref()
                .and_then(serde_json::Value::as_bool)
            {
                Some(true) => {
                    observer.on_event(&AgentEvent::PermissionJudged {
                        plane,
                        gate,
                        decision: "allow",
                        why: format!("operator granted at the gate (--answer) · {why}"),
                    });
                    reply.respond(PermissionDecision::AllowOnce);
                }
                Some(false) => {
                    observer.on_event(&AgentEvent::PermissionJudged {
                        plane,
                        gate,
                        decision: "deny",
                        why: format!("operator denied at the gate (--answer) · {why}"),
                    });
                    reply.respond(PermissionDecision::Deny);
                }
                None => {
                    observer.on_event(&AgentEvent::PermissionJudged {
                        plane,
                        gate,
                        decision: "escalate",
                        why: why.clone(),
                    });
                    drop(reply);
                    return Err(VerbAgentError::HarnessGate {
                        question,
                        detail: why,
                        spend: Box::default(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Harness failures speak the verb's inference-family error — the
/// provider seam's own class (a harness IS this task's provider). The
/// spend box is empty-honest: a failed harness run reports no usage.
///
/// TRANSIENCE SURVIVES THE WRAP (review 2026-08-06): collapsing every
/// variant into `ProviderError::Other` lost it — `Other` is never
/// transient, so a retryable session death (a transport hiccup the
/// harness itself calls transient) arrived at the retry layer as
/// permanent, and the workflow's automatic retry silently stopped
/// working. A transient harness failure now rides the ONE provider
/// variant that carries the same verdict (a 5xx-class `Api`), so
/// `is_transient()` reads the same on both sides of the seam.
fn harness_err(e: &HarnessError) -> VerbAgentError {
    let source = if e.is_transient() {
        nika_kernel::ProviderError::Api {
            status: 503,
            message: e.to_string(),
        }
    } else {
        nika_kernel::ProviderError::Other {
            reason: e.to_string(),
        }
    };
    VerbAgentError::Inference {
        source,
        spend: Box::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::task::{Context, Poll};

    use nika_error::traits::NikaErrorCode as _;
    use nika_kernel::ai::harness::{HarnessEventStream, HarnessOutcome, PermissionReply};

    /// A scripted backend: plays a fixed event tape (permission
    /// verdicts are recorded by the reply closures the tape carries).
    struct TapeBackend {
        tape: Mutex<Vec<HarnessEvent>>,
    }

    struct TapeStream {
        tape: Vec<HarnessEvent>,
    }

    impl futures_core::Stream for TapeStream {
        type Item = Result<HarnessEvent, HarnessError>;
        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            if self.tape.is_empty() {
                Poll::Ready(None)
            } else {
                Poll::Ready(Some(Ok(self.tape.remove(0))))
            }
        }
    }

    impl DynAgentBackend for TapeBackend {
        fn run_agent_boxed(
            &self,
            _request: HarnessRequest,
        ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>>
        {
            let tape = std::mem::take(&mut *self.tape.lock().expect("tape lock"));
            Box::pin(async move { Ok(Box::pin(TapeStream { tape }) as HarnessEventStream) })
        }
    }

    use core::future::Future;

    /// A recording observer — the bridge's witness decisions under test.
    #[derive(Default)]
    struct VecObserver(Mutex<Vec<AgentEvent>>);

    impl AgentObserver for VecObserver {
        fn on_event(&self, event: &AgentEvent) {
            self.0.lock().expect("observer lock").push(event.clone());
        }
    }

    fn seat_with(tape: Vec<HarnessEvent>) -> HarnessSeat {
        let backend = TapeBackend {
            tape: Mutex::new(tape),
        };
        HarnessSeat::new(Arc::new(backend), "/tmp")
    }

    /// The witness label names WHAT was judged (mutation-killers for
    /// `gate_label` — an empty or constant label is a blind auditor).
    #[test]
    fn the_gate_label_names_the_fact() {
        let exec = HarnessAskFacts::new()
            .with_kind(Some("execute".to_owned()))
            .with_command(vec!["git".to_owned(), "status".to_owned()]);
        assert_eq!(gate_label(&exec), "execute · git");
        let prose = HarnessAskFacts::new().with_kind(Some("execute".to_owned()));
        assert_eq!(gate_label(&prose), "execute · <prose>");
        let edit = HarnessAskFacts::new()
            .with_kind(Some("edit".to_owned()))
            .with_locations(vec!["a.rs".to_owned(), "b.rs".to_owned()]);
        assert_eq!(gate_label(&edit), "edit · a.rs,b.rs");
        let fetch = HarnessAskFacts::new()
            .with_kind(Some("fetch".to_owned()))
            .with_url(Some("https://x.sh".to_owned()));
        assert_eq!(gate_label(&fetch), "fetch · https://x.sh");
        let no_url = HarnessAskFacts::new().with_kind(Some("fetch".to_owned()));
        assert_eq!(gate_label(&no_url), "fetch · <no url>");
        let think = HarnessAskFacts::new().with_kind(Some("think".to_owned()));
        assert_eq!(gate_label(&think), "think");
        let bare = HarnessAskFacts::new();
        assert_eq!(gate_label(&bare), "<undeclared>");
    }

    /// The seat's Debug shows the cwd and never the backend (the
    /// `finish_non_exhaustive` shape — mutation-pinned).
    #[test]
    fn the_seat_debug_is_cwd_only_and_non_exhaustive() {
        let seat = seat_with(vec![]);
        let dbg = format!("{seat:?}");
        assert!(dbg.contains("/tmp"), "{dbg}");
        assert!(dbg.contains("HarnessSeat"), "{dbg}");
        assert!(dbg.contains(".."), "{dbg}");
    }

    fn completed(text: &str) -> HarnessEvent {
        HarnessEvent::Completed {
            outcome: Box::new(HarnessOutcome::new(text)),
        }
    }

    /// A permission ask event whose reply closure records the verdict.
    fn ask(
        question: &str,
        kind: Option<&str>,
        command: Vec<String>,
        verdicts: Arc<Mutex<Vec<PermissionDecision>>>,
    ) -> HarnessEvent {
        HarnessEvent::PermissionAsked {
            question: question.to_owned(),
            reply: PermissionReply::new(Box::new(move |d| {
                verdicts.lock().expect("verdict lock").push(d);
            })),
            kind: kind.map(str::to_owned),
            locations: Vec::new(),
            command,
            url: None,
        }
    }

    #[tokio::test]
    async fn a_completed_tape_yields_the_pre_shaped_honest_output() {
        let seat = seat_with(vec![
            HarnessEvent::MessageChunk {
                text: "partial".to_owned(),
            },
            completed("the harness answered"),
        ]);
        let out = run_on_harness(&seat, AgentInput::new("do it"), &crate::NoopObserver)
            .await
            .expect("a completed tape succeeds");
        let AgentValue::Text(text) = &out.output else {
            panic!("harness output is text in P3");
        };
        assert_eq!(text, "the harness answered");
        assert_eq!(out.turns, 1);
        assert_eq!(
            out.total_tokens, 0,
            "no usage reported → honest zero, never invented"
        );
        assert!(
            out.model_resolved.is_none(),
            "the pre-shaped None (pricing key absent)"
        );
        assert!(out.tools_cost_usd.is_none());
    }

    /// B5 · inside the grants, the bridge answers `AllowOnce` itself and
    /// witnesses the decision (the `permit_checked` channel's verb half).
    #[tokio::test]
    async fn an_ask_inside_the_grants_is_allowed_once_and_witnessed() {
        let verdicts = Arc::new(Mutex::new(Vec::new()));
        let tape = vec![
            ask(
                "run `git status`",
                Some("execute"),
                vec!["git".to_owned(), "status".to_owned()],
                Arc::clone(&verdicts),
            ),
            completed("done"),
        ];
        let mut input = AgentInput::new("try");
        input.permits = Some({
            let mut p = nika_schema::types::Permits::new();
            p.exec = Some(nika_schema::types::ExecPermit::Any);
            p
        });
        let observer = VecObserver::default();
        let out = run_on_harness(&seat_with(tape), input, &observer)
            .await
            .expect("an in-grants ask completes the run");
        let AgentValue::Text(text) = &out.output else {
            panic!("text output");
        };
        assert_eq!(text, "done");
        assert_eq!(
            verdicts.lock().expect("verdict lock").as_slice(),
            &[PermissionDecision::AllowOnce],
            "inside grants → AllowOnce (never allow_always · A-5)"
        );
        let events = observer.0.lock().expect("observer lock");
        assert!(
            events.iter().any(|e| matches!(
                e,
                AgentEvent::PermissionJudged {
                    plane: "exec",
                    decision: "allow",
                    ..
                }
            )),
            "the allow decision is witnessed: {events:?}"
        );
    }

    /// B5 · outside the grants with NO operator answer: the run pauses
    /// (`HarnessGate` error), the question rides VERBATIM, and the reply
    /// lane was never answered — zero harness action before a human.
    #[tokio::test]
    async fn an_ask_outside_the_grants_pauses_with_the_question_verbatim() {
        let verdicts = Arc::new(Mutex::new(Vec::new()));
        let tape = vec![
            ask(
                "run `rm -rf /`",
                Some("execute"),
                vec!["rm".to_owned(), "-rf".to_owned(), "/".to_owned()],
                Arc::clone(&verdicts),
            ),
            completed("never reached"),
        ];
        let observer = VecObserver::default();
        let err = run_on_harness(&seat_with(tape), AgentInput::new("try"), &observer)
            .await
            .expect_err("an out-of-grants ask pauses the run");
        let VerbAgentError::HarnessGate { question, .. } = &err else {
            panic!("the gate error is the pause branch, got {err:?}");
        };
        assert_eq!(question, "run `rm -rf /`", "the question rides verbatim");
        assert_eq!(
            err.nika_code(),
            nika_error::codes::NIKA_1806,
            "the gate speaks the access family's code"
        );
        assert!(
            verdicts.lock().expect("verdict lock").is_empty(),
            "the ask was NEVER answered — the dropped lane reads cancelled fail-closed"
        );
        let events = observer.0.lock().expect("observer lock");
        let allows = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    AgentEvent::PermissionJudged {
                        decision: "allow",
                        ..
                    }
                )
            })
            .count();
        assert_eq!(allows, 0, "zero allow before the refusal");
        assert!(
            events.iter().any(|e| matches!(
                e,
                AgentEvent::PermissionJudged {
                    decision: "escalate",
                    ..
                }
            )),
            "the escalation itself is witnessed: {events:?}"
        );
    }

    /// B5 · the operator's bound `--answer` decides: `true` grants ONCE
    /// (witnessed as operator-granted), `false` denies.
    #[tokio::test]
    async fn the_operators_bound_answer_decides_the_ask() {
        for (answer, expected, word) in [
            (true, PermissionDecision::AllowOnce, "operator granted"),
            (false, PermissionDecision::Deny, "operator denied"),
        ] {
            let verdicts = Arc::new(Mutex::new(Vec::new()));
            let tape = vec![
                ask(
                    "run `make deploy`",
                    Some("execute"),
                    vec!["make".to_owned(), "deploy".to_owned()],
                    Arc::clone(&verdicts),
                ),
                completed("done"),
            ];
            let mut input = AgentInput::new("try");
            input.gate_answer = Some(serde_json::Value::Bool(answer));
            let observer = VecObserver::default();
            run_on_harness(&seat_with(tape), input, &observer)
                .await
                .expect("an answered gate completes the run");
            assert_eq!(
                verdicts.lock().expect("verdict lock").as_slice(),
                &[expected],
                "answer={answer}"
            );
            let events = observer.0.lock().expect("observer lock");
            assert!(
                events.iter().any(|e| {
                    matches!(e, AgentEvent::PermissionJudged { why, .. } if why.contains(word))
                }),
                "the operator's verdict is witnessed as such ({word}): {events:?}"
            );
        }
    }

    /// B5 · the bound answer is CONSUMED by the first out-of-grants ask:
    /// a second ask pauses again rather than ride a stale grant (the
    /// human answered ONE question).
    #[tokio::test]
    async fn the_bound_answer_is_consumed_by_the_first_ask_only() {
        let verdicts = Arc::new(Mutex::new(Vec::new()));
        let tape = vec![
            ask(
                "run `make deploy`",
                Some("execute"),
                vec!["make".to_owned(), "deploy".to_owned()],
                Arc::clone(&verdicts),
            ),
            ask(
                "run `make clean`",
                Some("execute"),
                vec!["make".to_owned(), "clean".to_owned()],
                Arc::clone(&verdicts),
            ),
            completed("never reached"),
        ];
        let mut input = AgentInput::new("try");
        input.gate_answer = Some(serde_json::Value::Bool(true));
        let observer = VecObserver::default();
        let err = run_on_harness(&seat_with(tape), input, &observer)
            .await
            .expect_err("the SECOND out-of-grants ask pauses again");
        let VerbAgentError::HarnessGate { question, .. } = &err else {
            panic!("the second ask hits the gate, got {err:?}");
        };
        assert_eq!(question, "run `make clean`");
        assert_eq!(
            verdicts.lock().expect("verdict lock").as_slice(),
            &[PermissionDecision::AllowOnce],
            "exactly ONE ask rode the operator's grant"
        );
    }

    #[tokio::test]
    async fn schema_and_tools_refuse_with_witnesses() {
        let seat = seat_with(vec![completed("never reached")]);
        let schema_err = run_on_harness(
            &seat,
            {
                let mut i = AgentInput::new("x");
                i.schema = Some(serde_json::json!({"type": "object"}));
                i
            },
            &crate::NoopObserver,
        )
        .await
        .expect_err("schema refuses on a harness in P3");
        assert!(schema_err.to_string().contains("P4"), "{schema_err}");

        let seat2 = seat_with(vec![completed("never reached")]);
        let tools_err = run_on_harness(
            &seat2,
            {
                let mut i = AgentInput::new("x");
                i.tools = vec!["nika:*".to_owned()];
                i
            },
            &crate::NoopObserver,
        )
        .await
        .expect_err("tools refuse on a harness");
        assert!(
            tools_err.to_string().contains("permission bridge"),
            "{tools_err}"
        );
    }

    #[tokio::test]
    async fn a_tape_without_completed_is_a_session_death() {
        let seat = seat_with(vec![HarnessEvent::MessageChunk {
            text: "then silence".to_owned(),
        }]);
        let err = run_on_harness(&seat, AgentInput::new("x"), &crate::NoopObserver)
            .await
            .expect_err("no Completed beat is an error");
        assert!(err.to_string().contains("without a Completed"), "{err}");
    }

    #[tokio::test]
    async fn harness_reported_usage_rides_the_output() {
        use nika_kernel::ai::harness::TokenUsage;
        let mut usage = TokenUsage::default();
        usage.input_tokens = 100;
        usage.output_tokens = 40;
        let outcome = HarnessOutcome::new("counted").with_usage(usage);
        let seat = seat_with(vec![HarnessEvent::Completed {
            outcome: Box::new(outcome),
        }]);
        let out = run_on_harness(&seat, AgentInput::new("x"), &crate::NoopObserver)
            .await
            .expect("completes");
        assert_eq!(out.total_tokens, 140);
        assert_eq!(out.usage.input_tokens, 100);
    }
}

#[cfg(test)]
mod transience_tests {
    use super::*;
    use nika_error::traits::NikaErrorCode as _;

    /// The review's finding (2026-08-06), pinned: a transient harness
    /// failure must STILL read transient after the wrap, or the retry
    /// layer silently stops retrying a recoverable disconnect.
    #[test]
    fn a_transient_harness_failure_stays_transient_through_the_wrap() {
        let session = HarnessError::Session {
            reason: "pipe closed".to_owned(),
        };
        assert!(session.is_transient(), "the harness calls this transient");
        let wrapped = harness_err(&session);
        assert!(
            wrapped.is_transient(),
            "and so must the verb error the retry layer reads"
        );
    }

    #[test]
    fn a_permanent_harness_failure_stays_permanent() {
        for e in [
            HarnessError::Unavailable {
                reason: "binary absent".to_owned(),
            },
            HarnessError::Refused {
                reason: "auth absent".to_owned(),
            },
        ] {
            assert!(!e.is_transient());
            assert!(
                !harness_err(&e).is_transient(),
                "a structural failure must never earn a retry"
            );
        }
    }
}
