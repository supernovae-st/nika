// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The harness access class seam (D-2026-08-04-N1 · P3 B2) — `agent:`
//! tasks delegated to the user's OWN authenticated agent harness
//! (gemini-cli · qwen-code · codex-acp · claude-agent-acp).
//!
//! Sibling of [`crate::provider`]: [`AgentBackend`] is to an external
//! agentic loop what `ProviderStream` is to a raw model — ONE method,
//! a stream of typed events, the terminal event carrying the outcome.
//! Lane-agnostic on purpose: the trait never names a wire (the engine's
//! hand-rolled wire-v1 client implements it; a future SDK-backed impl
//! could too). The harness owns auth (A-3): nothing in this seam ever
//! carries a credential.
//!
//! Authority stays the ENGINE's: a permission the harness requests
//! rides the event stream as [`HarnessEvent::PermissionAsked`] and is
//! answered through the [`PermissionReply`] channel the request
//! carries — the runtime's permits bridge (P3 B5) answers inside
//! grants and pauses outside them; `allow_always` is NEVER granted
//! (A-5 · the `GOOSE_MODE=auto` anti-pattern is the named counter-example).

use std::pin::Pin;

use futures_core::Stream;

pub use nika_error::token_usage::TokenUsage;

/// One agentic run delegated to a harness — the narrow-waist request
/// (`#[non_exhaustive]` · P3/P4 dimensions join additively).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct HarnessRequest {
    /// The initial user message (required · spec §agent).
    pub prompt: String,
    /// Optional system prompt — WRAPPED by the harness's own frame
    /// (`prompt_fidelity` is `wrapped` on this class · never `exact`).
    pub system: Option<String>,
    /// The working directory the session is rooted at.
    pub cwd: std::path::PathBuf,
    /// The model the AUTHOR asked for, verbatim (`provider/name`) —
    /// the harness may report a different observed identity; recording
    /// both is the receipt's job, never a silent substitution (A-2).
    pub requested_model: Option<String>,
}

impl HarnessRequest {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(prompt: impl Into<String>, cwd: impl Into<std::path::PathBuf>) -> Self {
        Self {
            prompt: prompt.into(),
            system: None,
            cwd: cwd.into(),
            requested_model: None,
        }
    }

    /// Attach a system prompt.
    #[must_use]
    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// Record the author's requested model id.
    #[must_use]
    pub fn with_requested_model(mut self, model: impl Into<String>) -> Self {
        self.requested_model = Some(model.into());
        self
    }
}

/// The reply lane for ONE permission request — carried BY the event so
/// the authority decision rides beside the question it answers (a
/// confused deputy cannot arise from correlating ids across streams).
/// A boxed once-callback, NOT a channel type: the kernel trait layer
/// is executor-free (L0.5 bans tokio in prod deps) — the adapter impl
/// backs it with whatever channel its runtime uses, under the contract
/// that a responder DROPPED unanswered reads as
/// [`PermissionDecision::Deny`] (fail-closed · pinned by the adapter's
/// own tests, the only place a drop can be observed).
pub struct PermissionReply {
    respond: Box<dyn FnOnce(PermissionDecision) + Send>,
}

impl PermissionReply {
    /// Wrap the adapter's answer lane (INV-019).
    #[must_use]
    pub fn new(respond: Box<dyn FnOnce(PermissionDecision) + Send>) -> Self {
        Self { respond }
    }

    /// Deliver the engine's verdict — consumes the lane (one question,
    /// one answer, never two).
    pub fn respond(self, decision: PermissionDecision) {
        (self.respond)(decision);
    }
}

impl core::fmt::Debug for PermissionReply {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PermissionReply(..)")
    }
}

/// The engine's verdict on a harness permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PermissionDecision {
    /// Inside a `permits:` grant — allowed ONCE (never `allow_always`).
    AllowOnce,
    /// Outside every grant and the human said no (or the run is
    /// non-interactive) — the harness must stop the action.
    Deny,
}

/// One observed beat of a delegated run.
#[non_exhaustive]
pub enum HarnessEvent {
    /// A chunk of the agent's message text.
    MessageChunk {
        /// The text delta.
        text: String,
    },
    /// The harness asked to do something — the QUESTION verbatim plus
    /// the reply lane. The engine answers (permits bridge · B5); an
    /// unanswered drop reads as [`PermissionDecision::Deny`] fail-closed.
    PermissionAsked {
        /// The harness's own description of the action.
        question: String,
        /// The one-shot reply lane.
        reply: PermissionReply,
    },
    /// The run completed — the terminal beat. Boxed: the outcome is
    /// the fat variant and every OTHER beat is hot-path (the clippy
    /// large-variant law · one allocation at the single terminal beat).
    Completed {
        /// The final outcome.
        outcome: Box<HarnessOutcome>,
    },
}

/// The terminal facts of a delegated run — what the receipt records.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct HarnessOutcome {
    /// The agent's final message text.
    pub output: String,
    /// Token usage WHEN the harness reports it — `None` stays none
    /// (`usage_evidence` `harness-reported` or absent · never estimated
    /// here · A-7).
    pub usage: Option<TokenUsage>,
    /// The model identity the harness REPORTED, when observable —
    /// recorded beside `requested_model`, never reconciled silently.
    pub observed_model: Option<String>,
}

impl HarnessOutcome {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            usage: None,
            observed_model: None,
        }
    }

    /// Attach harness-reported usage.
    #[must_use]
    pub fn with_usage(mut self, usage: TokenUsage) -> Self {
        self.usage = Some(usage);
        self
    }

    /// Attach the harness-reported model identity.
    #[must_use]
    pub fn with_observed_model(mut self, model: impl Into<String>) -> Self {
        self.observed_model = Some(model.into());
        self
    }
}

/// Harness backend failure — `#[non_exhaustive]` from day one (INV #25).
/// `Diagnostic` is the `NikaErrorCode` supertrait (the B5 one-voice
/// contract · the `ProviderError` precedent).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum HarnessError {
    /// The adapter binary is absent or outside its version pin.
    #[error("harness unavailable: {reason}")]
    Unavailable {
        /// What the probe saw (binary absent · version outside pin).
        reason: String,
    },
    /// The session died mid-run (process exit · wire breakdown).
    #[error("harness session failed: {reason}")]
    Session {
        /// The transport-level cause.
        reason: String,
    },
    /// The harness refused the request (auth absent on ITS side ·
    /// unsupported capability) — the harness's own words ride verbatim.
    #[error("harness refused: {reason}")]
    Refused {
        /// The harness's own refusal.
        reason: String,
    },
}

impl HarnessError {
    /// Whether retrying may succeed — only a session/transport death is
    /// transient; an absent binary or a refusal never heals on retry.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Session { .. })
    }
}

/// The event stream of one delegated run (the [`crate::provider`]
/// `InferEventStream` idiom — boxed for object safety, one allocation
/// per run).
pub type HarnessEventStream =
    Pin<Box<dyn Stream<Item = Result<HarnessEvent, HarnessError>> + Send>>;

/// An external agentic backend — the harness access class made a seam.
///
/// CANCEL SAFETY: dropping the returned stream MUST end the delegated
/// run (kill-on-drop at the adapter's spawn seam) — a harness never
/// outlives the task that asked for it.
#[trait_variant::make(AgentBackendDyn: Send)]
pub trait AgentBackend: Send + Sync {
    /// Run ONE delegated agentic turn; events stream until
    /// [`HarnessEvent::Completed`].
    async fn run_agent(&self, request: HarnessRequest) -> Result<HarnessEventStream, HarnessError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_send<T: Send>() {}

    #[test]
    fn the_seam_is_send_and_the_bound_form_compiles() {
        // The trait_variant contract, as the house USES it: the Dyn
        // form is a Send-variant GENERIC BOUND (`P: ProviderInferDyn`
        // in verb-agent — never a `dyn` object). The generic witness
        // compiling IS the assertion.
        fn _takes_bound<B: AgentBackendDyn>(_: &B) {}
        _assert_send::<HarnessRequest>();
        _assert_send::<HarnessOutcome>();
        _assert_send::<HarnessError>();
        _assert_send::<PermissionReply>();
    }

    #[test]
    fn request_constructor_carries_the_narrow_waist() {
        let r = HarnessRequest::new("do the thing", "/tmp/project")
            .with_system("you are terse")
            .with_requested_model("qwen/qwen3");
        assert_eq!(r.prompt, "do the thing");
        assert_eq!(r.system.as_deref(), Some("you are terse"));
        assert_eq!(r.cwd, std::path::PathBuf::from("/tmp/project"));
        assert_eq!(r.requested_model.as_deref(), Some("qwen/qwen3"));
    }

    #[test]
    fn outcome_honesty_defaults_are_absent_never_zero() {
        // usage None stays None (A-7: unknown is never $0/0 tokens) ·
        // observed_model None means "not observable", never "same as
        // requested".
        let o = HarnessOutcome::new("done");
        assert_eq!(o.output, "done");
        assert!(o.usage.is_none());
        assert!(o.observed_model.is_none());
    }

    #[test]
    fn only_the_session_death_is_transient() {
        assert!(
            HarnessError::Session {
                reason: "pipe closed".into()
            }
            .is_transient()
        );
        assert!(
            !HarnessError::Unavailable {
                reason: "binary absent".into()
            }
            .is_transient()
        );
        assert!(
            !HarnessError::Refused {
                reason: "auth absent".into()
            }
            .is_transient()
        );
    }

    #[test]
    fn the_reply_lane_delivers_exactly_once() {
        // `respond` consumes the lane — one question, one answer; the
        // move semantics ARE the never-twice proof (a second call does
        // not compile). The delivered decision arrives verbatim.
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU8, Ordering};
        let seen = Arc::new(AtomicU8::new(0));
        let seen_in = Arc::clone(&seen);
        let reply = PermissionReply::new(Box::new(move |decision| {
            // Same-crate match: `#[non_exhaustive]` binds downstream
            // crates only, so the arms stay exhaustive here on purpose
            // (a new variant is a COMPILE error until this learns it).
            let code = match decision {
                PermissionDecision::AllowOnce => 1,
                PermissionDecision::Deny => 2,
            };
            seen_in.store(code, Ordering::SeqCst);
        }));
        reply.respond(PermissionDecision::Deny);
        assert_eq!(
            seen.load(Ordering::SeqCst),
            2,
            "the verdict must arrive verbatim"
        );
    }
}
