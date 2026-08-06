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
//! - a permission ask is answered **Deny** (fail-closed) — B5 replaces
//!   this constant with the runtime's permits bridge; until then no
//!   harness action escapes the engine's authority by default.

use std::sync::Arc;

use nika_kernel::ai::harness::{
    DynAgentBackend, HarnessError, HarnessEvent, HarnessRequest, PermissionDecision,
};
use nika_kernel::runtime::agent::AgentStopReason;

use crate::{AgentInput, AgentOutput, AgentValue, VerbAgentError};

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

    let mut stream = seat
        .backend
        .run_agent_boxed(request)
        .await
        .map_err(|e| harness_err(&e))?;

    loop {
        let next = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await;
        match next {
            Some(Ok(HarnessEvent::PermissionAsked { reply, .. })) => {
                // B4 fail-closed constant — B5 swaps in the permits
                // bridge (auto-answer inside grants · pause outside).
                reply.respond(PermissionDecision::Deny);
            }
            Some(Ok(HarnessEvent::Completed { outcome })) => {
                let mut out = AgentOutput::new(
                    AgentValue::Text(outcome.output.clone()),
                    AgentStopReason::Completed,
                    1,
                    outcome
                        .usage
                        .as_ref()
                        .map_or(0, |u| u.input_tokens + u.output_tokens),
                );
                // Harness-built output honesty (the pre-shaped AgentOutput
                // docs): usage stays harness-reported-or-zero ·
                // model_resolved stays None — the receipt records the
                // REQUESTED model; an observed identity is the trace's
                // fact, never reconciled here (A-2/A-7).
                if let Some(usage) = outcome.usage {
                    out.usage = usage;
                }
                return Ok(out);
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

/// Harness failures speak the verb's inference-family error — the
/// provider seam's own class (a harness IS this task's provider). The
/// spend box is empty-honest: a failed harness run reports no usage.
fn harness_err(e: &HarnessError) -> VerbAgentError {
    VerbAgentError::Inference {
        source: nika_kernel::ProviderError::Other {
            reason: e.to_string(),
        },
        spend: Box::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::task::{Context, Poll};

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

    fn seat_with(tape: Vec<HarnessEvent>) -> HarnessSeat {
        let backend = TapeBackend {
            tape: Mutex::new(tape),
        };
        HarnessSeat::new(Arc::new(backend), "/tmp")
    }

    fn completed(text: &str) -> HarnessEvent {
        HarnessEvent::Completed {
            outcome: Box::new(HarnessOutcome::new(text)),
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
        let out = run_on_harness(&seat, AgentInput::new("do it"))
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

    #[tokio::test]
    async fn a_permission_ask_is_denied_fail_closed_in_b4() {
        let verdicts = Arc::new(Mutex::new(Vec::new()));
        let verdicts_in = Arc::clone(&verdicts);
        let ask = HarnessEvent::PermissionAsked {
            question: "run `rm -rf /`".to_owned(),
            reply: PermissionReply::new(Box::new(move |d| {
                verdicts_in.lock().expect("verdict lock").push(d);
            })),
        };
        let seat2 = seat_with(vec![ask, completed("done")]);
        let out = run_on_harness(&seat2, AgentInput::new("try"))
            .await
            .expect("the run completes past the denied ask");
        let AgentValue::Text(text) = &out.output else {
            panic!("text output");
        };
        assert_eq!(text, "done");
        assert_eq!(
            verdicts.lock().expect("verdict lock").as_slice(),
            &[PermissionDecision::Deny],
            "B4 answers every ask Deny — fail-closed until the B5 bridge"
        );
    }

    #[tokio::test]
    async fn schema_and_tools_refuse_with_witnesses() {
        let seat = seat_with(vec![completed("never reached")]);
        let schema_err = run_on_harness(&seat, {
            let mut i = AgentInput::new("x");
            i.schema = Some(serde_json::json!({"type": "object"}));
            i
        })
        .await
        .expect_err("schema refuses on a harness in P3");
        assert!(schema_err.to_string().contains("P4"), "{schema_err}");

        let seat2 = seat_with(vec![completed("never reached")]);
        let tools_err = run_on_harness(&seat2, {
            let mut i = AgentInput::new("x");
            i.tools = vec!["nika:*".to_owned()];
            i
        })
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
        let err = run_on_harness(&seat, AgentInput::new("x"))
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
        let out = run_on_harness(&seat, AgentInput::new("x"))
            .await
            .expect("completes");
        assert_eq!(out.total_tokens, 140);
        assert_eq!(out.usage.input_tokens, 100);
    }
}
