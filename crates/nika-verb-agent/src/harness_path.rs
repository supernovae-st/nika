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
    DynAgentBackend, HarnessConfinement, HarnessError, HarnessEvent, HarnessRequest,
    PermissionDecision, PermissionReply,
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

/// What a harness access cannot honour, refused before its session starts: a structured-output
/// schema it does not attest, and a tool whitelist it cannot enforce.
fn refuse_unenforceable(input: &AgentInput) -> Result<(), VerbAgentError> {
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
    Ok(())
}

/// Run the task on the harness seat — the external half of
/// `run_observed`.
pub(crate) async fn run_on_harness(
    seat: &HarnessSeat,
    input: AgentInput,
    observer: &dyn AgentObserver,
    images: Option<&Arc<dyn crate::spill::SpillStoreDyn>>,
) -> Result<AgentOutput, VerbAgentError> {
    refuse_unenforceable(&input)?;

    let typed = input.requirement.is_some();
    let request = session_request(seat, &input);
    // B5 · the operator's bound verdict is CONSUMED by the first
    // out-of-grants ask it decides: the human answered ONE question, so
    // a second ask (or a different one on a nondeterministic replay)
    // pauses again instead of riding a stale grant. `GateAnswerSpent`
    // carries the consumption past this attempt: a `retry:` replay of
    // the task never receives the answer again.
    let mut gate_answer = input.gate_answer.clone();

    let mut stream = seat
        .backend
        .run_agent_boxed(request)
        .await
        .map_err(|e| harness_err(e, typed))?;

    loop {
        let next = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await;
        match next {
            Some(Ok(
                ask @ (HarnessEvent::PermissionAsked { .. } | HarnessEvent::EscalationAsked { .. }),
            )) => {
                // B5 · the AUTHORITY BRIDGE (extracted under the
                // fn-length law): judged against the workflow's
                // declared `permits:` — answered ONCE and witnessed
                // inside, the operator's bound verdict outside, and
                // absent one the run PAUSES (the reply lane drops
                // unanswered: the harness hears `cancelled`).
                if let Some((facts, question, reply)) = asked(ask) {
                    let permits = input.permits.as_ref();
                    bridge_ask(&facts, question, reply, permits, &mut gate_answer, observer)?;
                }
            }
            Some(Ok(HarnessEvent::SessionConfined {
                mode,
                selected,
                unasked,
            })) => {
                observer.on_event(&confinement_witness(mode.as_deref(), selected, unasked));
            }
            Some(Ok(HarnessEvent::ClientCallRefused { method })) => {
                observer.on_event(&refused_call_witness(method));
            }
            Some(Ok(HarnessEvent::ImageActivityObserved { tool_call_id })) => {
                observer.on_event(&AgentEvent::HarnessImageActivity { tool_call_id });
            }
            Some(Ok(HarnessEvent::ImageObserved { image })) => {
                store_image(*image, images, observer).await?;
            }
            Some(Ok(HarnessEvent::Completed { outcome })) => {
                // Under a declaration the selection is exact: a model or an
                // effort the agent moved after it was applied refuses.
                if typed && let Some(refusal) = outcome.selection.moved_refusal() {
                    return Err(harness_err(refusal, typed));
                }
                return Ok(completed_output(*outcome, &input));
            }
            Some(Ok(_)) => {
                // Chunks (the outcome carries the accumulated text ·
                // per-chunk observer taps arrive with B5) and any
                // future event kind: observed, never fatal.
            }
            Some(Err(e)) => return Err(harness_err(e, typed)),
            None => {
                return Err(harness_err(
                    HarnessError::Session {
                        reason: "the harness stream ended without a Completed beat".to_owned(),
                    },
                    typed,
                ));
            }
        }
    }
}

/// The delegated session's request: the prompt, the system, the model the lane resolved,
/// the authored effort — which the session applies through its own option and reads
/// back before the prompt, or refuses (typed, under the declaration) — and the
/// confinement the workflow's `permits:` require of the session's own mode (NIK-266).
fn session_request(seat: &HarnessSeat, input: &AgentInput) -> HarnessRequest {
    let mut request = HarnessRequest::new(input.prompt.clone(), seat.cwd.clone())
        .with_requested_effort(input.requirement.as_ref().and_then(|r| r.effort.clone()))
        .with_confinement(confinement_for(input.permits.as_ref()));
    if let Some(system) = &input.system {
        request = request.with_system(system.clone());
    }
    if let Some(model) = &input.model {
        request = request.with_requested_model(model.clone());
    }
    request
}

/// The most restrictive confinement the declared boundary allows (NIK-266): no
/// `permits.fs.write` grant (an absent block is zero authority, F-O8) holds the
/// session read-only; a write grant admits a workspace-write mode. Neither ever
/// admits a mode that approves on its own — the client selects only measured
/// modes whose every approval reaches the bridge.
pub(crate) fn confinement_for(permits: Option<&nika_schema::types::Permits>) -> HarnessConfinement {
    let writes = permits
        .and_then(|p| p.fs.as_ref())
        .is_some_and(|fs| !fs.write.is_empty());
    if writes {
        HarnessConfinement::WorkspaceWrite
    } else {
        HarnessConfinement::ReadOnly
    }
}

/// The session's confinement as the NEP-0007 witness frame (`permit_checked`,
/// plane `agent`), journaled before any ask: WHICH mode the harness runs under
/// and what it still does without asking. `confine` when the client selected a
/// measured mode and read it back; `unmeasured` when only the harness's own
/// mode could be observed — the residual then names that nothing is selected.
fn confinement_witness(mode: Option<&str>, selected: bool, unasked: String) -> AgentEvent {
    AgentEvent::PermissionJudged {
        plane: "agent",
        gate: format!("session mode · {}", mode.unwrap_or("<none advertised>")),
        decision: if selected { "confine" } else { "unmeasured" },
        why: unasked,
    }
}

/// A client call the harness made that this client refused: a method it does not
/// offer (no fs and no terminal capability is ever advertised), or a permission
/// asked before the session was confined. Witnessed as a deny on the plane it
/// would have exercised.
fn refused_call_witness(method: String) -> AgentEvent {
    if method == "session/request_permission" {
        return AgentEvent::PermissionJudged {
            plane: "agent",
            gate: method,
            decision: "deny",
            why: "a permission asked before the prompt is answered `cancelled`: nothing is \
                  granted before the session holds its confinement"
                .to_owned(),
        };
    }
    let plane = if method.starts_with("fs/") {
        "fs"
    } else if method.starts_with("terminal/") {
        "exec"
    } else {
        "agent"
    };
    AgentEvent::PermissionJudged {
        plane,
        gate: method,
        decision: "deny",
        why: "Nika offers the harness no client fs or terminal method: the call is refused \
              on the wire (JSON-RPC method not found), never served"
            .to_owned(),
    }
}

/// Persist only bytes received on the wire. The receipt is published BEFORE the
/// store is awaited, so a cancelled put still leaves `storage: unconfirmed`; the
/// store's answer follows as `HarnessImageStored`. A failed store is a permanent
/// failure, never a second generation.
async fn store_image(
    mut image: nika_kernel::ai::harness::HarnessImage,
    store: Option<&Arc<dyn crate::spill::SpillStoreDyn>>,
    observer: &dyn AgentObserver,
) -> Result<(), VerbAgentError> {
    let Some(data) = image.data.take() else {
        observer.on_event(&AgentEvent::HarnessImageObserved { image });
        return Ok(());
    };
    let tool_call_id = image.tool_call_id.clone();
    observer.on_event(&AgentEvent::HarnessImageActivity {
        tool_call_id: tool_call_id.clone(),
    });
    observer.on_event(&AgentEvent::HarnessImageObserved { image });
    let stored = match store {
        Some(store) => store
            .put(data, "image/png")
            .await
            .map_err(|error| format!("image bytes were received but could not be stored: {error}")),
        None => Err("image bytes were received but no image blob store is configured".into()),
    };
    observer.on_event(&AgentEvent::HarnessImageStored {
        tool_call_id,
        stored: stored.clone(),
    });
    stored
        .map(drop)
        .map_err(|reason| harness_err(HarnessError::Refused { reason }, false))
}

/// The judged facts of one ask beat, with its question and reply lane (B5): a plain ask's
/// kind, paths, command and URL; an escalation's (NIK-266) beside the network target and
/// the sandbox widening it asks, which no grant covers. `None` for every other beat.
fn asked(event: HarnessEvent) -> Option<(HarnessAskFacts, String, PermissionReply)> {
    match event {
        HarnessEvent::PermissionAsked {
            question,
            reply,
            kind,
            locations,
            command,
            url,
        } => {
            let facts = HarnessAskFacts::new()
                .with_kind(kind)
                .with_locations(locations)
                .with_command(command)
                .with_url(url);
            Some((facts, question, reply))
        }
        HarnessEvent::EscalationAsked {
            question,
            reply,
            kind,
            command,
            locations,
            escalation,
        } => {
            let facts = HarnessAskFacts::new()
                .with_kind(kind)
                .with_locations(locations)
                .with_command(command)
                .with_network(escalation.network)
                .with_widens(escalation.widens)
                .with_mcp(escalation.mcp)
                .with_escalated_command(escalation.escalated_command);
            Some((facts, question, reply))
        }
        _ => None,
    }
}

/// The witness `gate` label for one ask — what the bridge judged, in
/// one string (the MCP tool call · the network target · the widening ·
/// a command run outside the sandbox · the program · the paths · the
/// kind): an auditor reads WHICH authority was exercised without
/// re-parsing wire facts.
fn gate_label(facts: &HarnessAskFacts) -> String {
    if let Some(call) = &facts.mcp {
        return format!("mcp · {call}");
    }
    if let Some(target) = &facts.network {
        return format!("network · {target}");
    }
    if let Some(widens) = &facts.widens {
        return format!("sandbox · {widens}");
    }
    if facts.escalated_command.is_some() {
        return "sandbox · command".to_owned();
    }
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
/// `model_resolved` stays None (it is the requested name and a pricing key) ·
/// the session model the harness REPORTED rides as `model_reported` with its
/// provenance: ACP attests no response model, so nothing here is "served" (A-2/A-7).
fn completed_output(
    outcome: nika_kernel::ai::harness::HarnessOutcome,
    input: &AgentInput,
) -> AgentOutput {
    let selection = input.requirement.as_ref().map(|requirement| {
        crate::selection::harness_evidence(
            requirement,
            input.model.clone(),
            outcome.observed_model.clone(),
            outcome
                .observed_model_source
                .map(nika_kernel::ai::harness::ModelProvenance::as_str),
            &outcome.selection,
        )
    });
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
    out.model_reported = outcome.observed_model;
    out.model_reported_source = outcome.observed_model_source;
    out.selection = selection;
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
            let answer = gate_answer.take();
            if answer.is_some() {
                // Spent for the whole task: the runtime never re-arms it for a replay.
                observer.on_event(&AgentEvent::GateAnswerSpent);
            }
            match answer.as_ref().and_then(serde_json::Value::as_bool) {
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
///
/// Under an AUTHORED access requirement (`typed`) the harness keeps its own
/// typed class instead (NIKA-1803/1804/1805 · [`VerbAgentError::Harness`]):
/// the declared contract names which access failed; transience is the same.
fn harness_err(e: HarnessError, typed: bool) -> VerbAgentError {
    if typed {
        return VerbAgentError::Harness {
            source: e,
            spend: Box::default(),
        };
    }
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

    /// NIK-266 · the bridge's escalation asks (network · sandbox widening).
    mod escalation;

    async fn run_on_harness(
        seat: &HarnessSeat,
        input: AgentInput,
        observer: &dyn AgentObserver,
    ) -> Result<AgentOutput, VerbAgentError> {
        super::run_on_harness(seat, input, observer, None).await
    }

    #[tokio::test]
    async fn received_image_bytes_are_stored_beside_text_and_never_inlined() {
        let blob = Arc::new(ReceivingImageStore::default());
        let store: Arc<dyn crate::spill::SpillStoreDyn> = blob.clone();
        let bytes = bytes::Bytes::from(vec![7; 1024 * 1024]);
        let mut image = nika_kernel::ai::harness::HarnessImage::new("image-1");
        image.data = Some(bytes.clone());
        image.received_bytes = Some(bytes.len() as u64);
        image.mime_type = Some("image/png".into());
        image.sha256 = Some("wire-received-digest".into());
        image.reported_saved_path = Some("/outside/not-to-be-opened.png".into());
        let seat = seat_with(vec![
            HarnessEvent::ImageObserved {
                image: Box::new(image),
            },
            completed("unchanged text"),
        ]);
        let observer = VecObserver::default();
        let out = super::run_on_harness(&seat, AgentInput::new("draw"), &observer, Some(&store))
            .await
            .unwrap();
        assert_eq!(out.output, AgentValue::Text("unchanged text".into()));
        let events = observer.0.lock().unwrap().clone();
        let at = |pred: fn(&AgentEvent) -> bool| events.iter().position(pred).unwrap();
        let receipt_at = at(|e| matches!(e, AgentEvent::HarnessImageObserved { .. }));
        let stored = at(|e| matches!(e, AgentEvent::HarnessImageStored { .. }));
        assert!(
            receipt_at < stored,
            "the receipt precedes the store's answer"
        );
        let mut receipt = events[receipt_at].clone();
        let image = receipt.harness_image().unwrap().clone();
        assert!(image.data.is_none(), "no bytes enter the observer/journal");
        assert_eq!(image.observation()["storage"], "unconfirmed");
        assert_eq!(image.observation()["file_verified"], false);
        assert!(image.observation().to_string().len() < 1024);
        assert!(
            receipt.settle_image(&events[stored]),
            "the answer settles its receipt"
        );
        let settled = receipt.harness_image().unwrap();
        assert_eq!(settled.observation()["storage"], "stored");
        let metadata = settled.stored_blob.clone().unwrap();
        assert_eq!(metadata.size, bytes.len() as u64);
        assert_eq!(metadata.hash, "fixture-blob");
        assert_eq!(blob.0.lock().unwrap().as_ref(), Some(&bytes));
        assert!(
            !observer
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|event| matches!(event, AgentEvent::PermissionJudged { .. }))
        );
    }

    /// A store whose put never answers: the controlled stand-in for a put a
    /// task timeout cancels. It records that the put was entered.
    #[derive(Default)]
    struct PendingImageStore(std::sync::atomic::AtomicBool);
    impl crate::spill::SpillStoreDyn for PendingImageStore {
        fn put(
            &self,
            _: bytes::Bytes,
            _: &'static str,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<nika_kernel::BlobMetadata, nika_kernel::BlobError>>
                    + Send
                    + '_,
            >,
        > {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            Box::pin(std::future::pending())
        }
    }

    #[tokio::test]
    async fn a_cancelled_put_keeps_the_received_receipt_with_unconfirmed_storage() {
        let pending = Arc::new(PendingImageStore::default());
        let store: Arc<dyn crate::spill::SpillStoreDyn> = pending.clone();
        let mut image = nika_kernel::ai::harness::HarnessImage::new("image-1");
        image.data = Some(bytes::Bytes::from_static(b"received"));
        image.received_bytes = Some(8);
        image.sha256 = Some("wire-received-digest".into());
        let seat = seat_with(vec![
            HarnessEvent::ImageObserved {
                image: Box::new(image),
            },
            completed("never reached"),
        ]);
        let observer = VecObserver::default();
        let mut cx = Context::from_waker(std::task::Waker::noop());
        let mut run = Box::pin(super::run_on_harness(
            &seat,
            AgentInput::new("draw"),
            &observer,
            Some(&store),
        ));
        assert!(
            run.as_mut().poll(&mut cx).is_pending(),
            "the put never answers"
        );
        assert!(
            pending.0.load(std::sync::atomic::Ordering::SeqCst),
            "the put was entered"
        );
        drop(run); // the task timeout's cancellation
        let events = observer.0.lock().unwrap();
        let receipt = events
            .iter()
            .find_map(AgentEvent::harness_image)
            .expect("the received bytes kept their receipt");
        assert_eq!(receipt.received_bytes, Some(8));
        assert_eq!(receipt.observation()["storage"], "unconfirmed");
        assert_eq!(
            receipt.observation()["received_sha256"],
            "wire-received-digest"
        );
        assert!(receipt.data.is_none() && receipt.stored_blob.is_none());
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, AgentEvent::HarnessImageStored { .. })),
            "no store answer is invented"
        );
        assert!(
            events.iter().any(AgentEvent::is_harness_image),
            "replay stays vetoed"
        );
    }

    #[derive(Default)]
    struct ReceivingImageStore(Mutex<Option<bytes::Bytes>>);
    impl crate::spill::SpillStoreDyn for ReceivingImageStore {
        fn put(
            &self,
            data: bytes::Bytes,
            mime: &'static str,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<nika_kernel::BlobMetadata, nika_kernel::BlobError>>
                    + Send
                    + '_,
            >,
        > {
            let size = data.len() as u64;
            *self.0.lock().unwrap() = Some(data);
            Box::pin(async move { Ok(nika_kernel::BlobMetadata::new("fixture-blob", mime, size)) })
        }
    }

    struct RefusingImageStore;
    impl crate::spill::SpillStoreDyn for RefusingImageStore {
        fn put(
            &self,
            _: bytes::Bytes,
            _: &'static str,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<nika_kernel::BlobMetadata, nika_kernel::BlobError>>
                    + Send
                    + '_,
            >,
        > {
            Box::pin(async {
                Err(nika_kernel::BlobError::Io {
                    reason: "fixture refused".into(),
                })
            })
        }
    }

    #[tokio::test]
    async fn missing_or_failed_store_never_turns_received_image_into_success() {
        let refused: Arc<dyn crate::spill::SpillStoreDyn> = Arc::new(RefusingImageStore);
        for store in [None, Some(&refused)] {
            let mut image = nika_kernel::ai::harness::HarnessImage::new("image-1");
            image.data = Some(bytes::Bytes::from_static(b"fixture"));
            image.received_bytes = Some(7);
            let seat = seat_with(vec![
                HarnessEvent::ImageObserved {
                    image: Box::new(image),
                },
                completed("must not succeed"),
            ]);
            let observer = VecObserver::default();
            let error = super::run_on_harness(&seat, AgentInput::new("draw"), &observer, store)
                .await
                .unwrap_err();
            assert!(!error.is_transient());
            let events = observer.0.lock().unwrap();
            assert!(
                events
                    .iter()
                    .any(|event| matches!(event, AgentEvent::HarnessImageActivity { .. }))
            );
            assert!(events.iter().any(|event| matches!(event, AgentEvent::HarnessImageObserved { image } if image.received_bytes == Some(7) && image.stored_blob.is_none() && image.data.is_none())));
            assert!(events.iter().any(|event| matches!(event, AgentEvent::HarnessImageStored { stored: Err(reason), .. } if reason.contains("received"))));
        }
    }

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

    #[tokio::test]
    async fn harness_image_is_observed_beside_unchanged_text_without_permission_fabrication() {
        let mut image = nika_kernel::ai::harness::HarnessImage::new("image-1");
        image.reported_saved_path = Some("/unverified.png".into());
        let observer = VecObserver::default();
        let seat = seat_with(vec![
            HarnessEvent::ImageActivityObserved {
                tool_call_id: "image-1".into(),
            },
            HarnessEvent::ImageObserved {
                image: Box::new(image.clone()),
            },
            HarnessEvent::Completed {
                outcome: Box::new(HarnessOutcome::new("unchanged text")),
            },
        ]);
        let out = run_on_harness(&seat, AgentInput::new("draw"), &observer)
            .await
            .unwrap();
        assert_eq!(out.output, AgentValue::Text("unchanged text".into()));
        let events = observer.0.lock().unwrap();
        assert!(matches!(
            &events[0],
            AgentEvent::HarnessImageActivity { .. }
        ));
        assert!(
            matches!(&events[1], AgentEvent::HarnessImageObserved { image: seen } if seen == &image)
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, AgentEvent::PermissionJudged { .. })),
            "a result is never an invented permission witness"
        );
    }

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

    /// NIK-266 · the declared write grants decide the confinement: no
    /// `fs.write` (or no permits at all, zero authority) holds the session
    /// read-only; any write grant admits workspace-write — never more.
    #[test]
    fn the_confinement_follows_the_declared_write_grants() {
        use nika_schema::types::{ExecPermit, FsPermits, Permits};
        let mut exec_only = Permits::new();
        exec_only.exec = Some(ExecPermit::Any);
        let mut reads = Permits::new();
        reads.fs = Some(FsPermits::new(vec!["src/**".to_owned()], Vec::new()));
        let mut writes = Permits::new();
        writes.fs = Some(FsPermits::new(Vec::new(), vec!["out/**".to_owned()]));
        for (permits, expected) in [
            (None, HarnessConfinement::ReadOnly),
            (Some(Permits::new()), HarnessConfinement::ReadOnly),
            (Some(exec_only), HarnessConfinement::ReadOnly),
            (Some(reads), HarnessConfinement::ReadOnly),
            (Some(writes), HarnessConfinement::WorkspaceWrite),
        ] {
            assert_eq!(confinement_for(permits.as_ref()), expected, "{permits:?}");
        }
    }

    /// A backend recording every request it receives, then completing.
    struct RecordingBackend(Mutex<Vec<HarnessRequest>>);

    impl DynAgentBackend for RecordingBackend {
        fn run_agent_boxed(
            &self,
            request: HarnessRequest,
        ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>>
        {
            self.0.lock().expect("requests").push(request);
            let tape = vec![completed("done")];
            Box::pin(async move { Ok(Box::pin(TapeStream { tape }) as HarnessEventStream) })
        }
    }

    /// NIK-266 · the session the verb opens is rooted at the seat (the
    /// admitted project) and REQUIRES the confinement its permits project —
    /// the old request carried no confinement at all, so the harness kept
    /// whatever self-approving mode it started in.
    #[tokio::test]
    async fn the_session_request_carries_the_seat_root_and_the_confinement() {
        let backend = Arc::new(RecordingBackend(Mutex::new(Vec::new())));
        let seat = HarnessSeat::new(backend.clone(), "/work/project");
        let mut input = AgentInput::new("go");
        input.permits = Some({
            let mut p = nika_schema::types::Permits::new();
            p.fs = Some(nika_schema::types::FsPermits::new(
                Vec::new(),
                vec!["./out/report.md".to_owned()],
            ));
            p
        });
        run_on_harness(&seat, input, &crate::NoopObserver)
            .await
            .expect("completes");
        run_on_harness(&seat, AgentInput::new("go"), &crate::NoopObserver)
            .await
            .expect("completes");
        let requests = backend.0.lock().expect("requests");
        let seen: Vec<_> = requests
            .iter()
            .map(|r| (r.cwd.clone(), r.confinement))
            .collect();
        assert_eq!(
            seen,
            [
                (
                    std::path::PathBuf::from("/work/project"),
                    Some(HarnessConfinement::WorkspaceWrite)
                ),
                (
                    std::path::PathBuf::from("/work/project"),
                    Some(HarnessConfinement::ReadOnly)
                ),
            ]
        );
    }

    /// NIK-266 · the confinement the session settled and a client method it
    /// was refused both ride the `permit_checked` witness, BEFORE the outcome.
    #[tokio::test]
    async fn the_session_confinement_and_a_refused_client_call_are_witnessed() {
        let seat = seat_with(vec![
            HarnessEvent::SessionConfined {
                mode: Some("read-only".to_owned()),
                selected: true,
                unasked: "reads any file without asking".to_owned(),
            },
            HarnessEvent::ClientCallRefused {
                method: "fs/write_text_file".to_owned(),
            },
            HarnessEvent::ClientCallRefused {
                method: "terminal/create".to_owned(),
            },
            HarnessEvent::ClientCallRefused {
                method: "session/request_permission".to_owned(),
            },
            HarnessEvent::SessionConfined {
                mode: None,
                selected: false,
                unasked: "no mode to select".to_owned(),
            },
            completed("done"),
        ]);
        let observer = VecObserver::default();
        run_on_harness(&seat, AgentInput::new("go"), &observer)
            .await
            .expect("completes");
        let judged: Vec<(&str, String, &str, String)> = observer
            .0
            .lock()
            .expect("observer")
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
            .collect();
        assert_eq!(judged.len(), 5, "{judged:?}");
        assert_eq!(
            (judged[0].0, judged[0].1.as_str(), judged[0].2),
            ("agent", "session mode · read-only", "confine")
        );
        assert_eq!(judged[0].3, "reads any file without asking");
        assert_eq!(
            (judged[1].0, judged[1].1.as_str(), judged[1].2),
            ("fs", "fs/write_text_file", "deny")
        );
        assert!(judged[1].3.contains("never served"), "{}", judged[1].3);
        assert_eq!(
            (judged[2].0, judged[2].1.as_str(), judged[2].2),
            ("exec", "terminal/create", "deny")
        );
        assert_eq!(
            (judged[3].0, judged[3].1.as_str(), judged[3].2),
            ("agent", "session/request_permission", "deny")
        );
        assert!(judged[3].3.contains("before the prompt"), "{}", judged[3].3);
        assert_eq!(
            (judged[4].0, judged[4].1.as_str(), judged[4].2),
            ("agent", "session mode · <none advertised>", "unmeasured")
        );
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
        // NIK-266 · what an ask reaches beyond its kind names the gate first.
        let net = HarnessAskFacts::new()
            .with_kind(Some("execute".to_owned()))
            .with_command(vec!["curl".to_owned()])
            .with_network(Some("socks5Tcp://exfil.example".to_owned()))
            .with_widens(Some("write /".to_owned()));
        assert_eq!(gate_label(&net), "network · socks5Tcp://exfil.example");
        let widened = HarnessAskFacts::new()
            .with_kind(Some("execute".to_owned()))
            .with_widens(Some("write /".to_owned()));
        assert_eq!(gate_label(&widened), "sandbox · write /");
        let mcp = HarnessAskFacts::new()
            .with_kind(Some("execute".to_owned()))
            .with_mcp(Some("server web".to_owned()));
        assert_eq!(gate_label(&mcp), "mcp · server web");
        let unsandboxed = HarnessAskFacts::new()
            .with_kind(Some("execute".to_owned()))
            .with_escalated_command(Some("curl -sS https://example.com".to_owned()));
        assert_eq!(gate_label(&unsandboxed), "sandbox · command");
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

    #[tokio::test]
    async fn the_one_door_native_choice_keeps_typed_repairs_on_the_provider() {
        use crate::tests::{rig, text_response};
        use nika_kernel_mock::{MockProvider, MockToolExecutor};

        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(text_response("unfinished"))
                .enqueue_response(text_response(r#"{"score":7}"#)),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let backend = Arc::new(TapeBackend {
            tape: Mutex::new(vec![completed("harness answer")]),
        });
        let verb = r
            .verb
            .with_harness_seat(HarnessSeat::new(backend.clone(), "/tmp"));
        let mut input = AgentInput::new("return a score");
        input.native_only = true;
        input.max_tokens_total = Some(15);
        input.schema = Some(serde_json::json!({"type":"object"}));
        let err = verb.run(input).await.expect_err("native token budget");
        assert!(matches!(
            err,
            VerbAgentError::MaxTokens {
                total_tokens: 15,
                ..
            }
        ));
        assert_eq!(r.provider.captured_requests().len(), 1);
        assert_eq!(backend.tape.lock().unwrap().len(), 1, "seat unused");

        // Negative control: an unpinned untyped task uses that same seat.
        let out = verb
            .run(AgentInput::new("finish"))
            .await
            .expect("seat runs");
        assert_eq!(out.output, AgentValue::Text("harness answer".to_owned()));
        assert!(backend.tape.lock().unwrap().is_empty());
        assert_eq!(r.provider.captured_requests().len(), 1);
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
        assert!(out.model_reported.is_none(), "no identity invented");
        assert!(out.model_reported_source.is_none());
    }

    /// The identity the harness reported rides as observed, distinct from the
    /// requested model, and is never written into the requested/pricing slot.
    #[tokio::test]
    async fn an_observed_identity_is_kept_apart_from_the_requested_model() {
        for (observed, expected) in [(Some("gpt-observed"), Some("gpt-observed")), (None, None)] {
            let mut outcome = HarnessOutcome::new("answer");
            if let Some(model) = observed {
                outcome = outcome.with_observed_model(model);
                outcome.observed_model_source =
                    Some(nika_kernel::ai::harness::ModelProvenance::ConfirmedSelection);
            }
            let seat = seat_with(vec![HarnessEvent::Completed {
                outcome: Box::new(outcome),
            }]);
            let mut input = AgentInput::new("do it");
            input.model = Some("openai/requested".to_owned());
            let out = run_on_harness(&seat, input, &crate::NoopObserver)
                .await
                .expect("a completed tape succeeds");
            assert_eq!(out.model_reported.as_deref(), expected);
            assert_eq!(out.model_reported_source.is_some(), expected.is_some());
            assert!(
                out.model_resolved.is_none(),
                "requested name is not a served claim"
            );
        }
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
        for typed in [false, true] {
            let session = HarnessError::Session {
                reason: "pipe closed".to_owned(),
            };
            assert!(session.is_transient(), "the harness calls this transient");
            let wrapped = harness_err(session, typed);
            assert!(
                wrapped.is_transient(),
                "and so must the verb error the retry layer reads (typed={typed})"
            );
        }
    }

    #[test]
    fn a_permanent_harness_failure_stays_permanent() {
        for typed in [false, true] {
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
                    !harness_err(e, typed).is_transient(),
                    "a structural failure must never earn a retry (typed={typed})"
                );
            }
        }
    }

    /// Under an authored requirement the harness keeps its own access
    /// code; without one the historical inference class is unchanged.
    #[test]
    fn a_declared_selection_keeps_the_harness_access_code() {
        let refused = || HarnessError::Refused {
            reason: "the harness offers no reasoning effort `xhigh`".to_owned(),
        };
        let typed = harness_err(refused(), true);
        assert_eq!(typed.spec_code(), "NIKA-1805");
        assert!(typed.to_string().contains("`xhigh`"), "{typed}");
        let session = harness_err(
            HarnessError::Session {
                reason: "pipe closed".to_owned(),
            },
            true,
        );
        assert_eq!(session.spec_code(), "NIKA-1804");
        let legacy = harness_err(refused(), false);
        assert_eq!(legacy.spec_code(), "NIKA-INFER-001");
    }
}
