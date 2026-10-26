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

use core::future::Future;
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
    /// The session mode the caller wants, in the caller's word (`read-only`): the client maps it
    /// to a mode the agent advertises (`plan` · `read-only` · …) when one exists, best effort.
    pub requested_mode: Option<String>,
    /// The reasoning effort the caller asks, as the route's NATIVE value, verbatim. The client
    /// applies it through the session's own reasoning option after the model is selected and
    /// reads it back before the prompt, or refuses; it is never dropped or translated.
    pub requested_effort: Option<String>,
    /// The confinement the session must hold (NIK-266): the workflow's `permits:` projected
    /// onto the harness's own approval mode. Unlike [`Self::requested_mode`] this is never best
    /// effort — the client applies the measured mode and reads it back before the prompt, or
    /// refuses. `None` keeps the harness's own mode (the one-shot and conversation profiles,
    /// which close their surfaces their own way).
    pub confinement: Option<HarnessConfinement>,
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
            requested_mode: None,
            requested_effort: None,
            confinement: None,
        }
    }

    /// Require the session to hold `confinement` (NIK-266).
    #[must_use]
    pub fn with_confinement(mut self, confinement: HarnessConfinement) -> Self {
        self.confinement = Some(confinement);
        self
    }

    /// Ask for an exact native reasoning effort (`None` keeps the session's own).
    #[must_use]
    pub fn with_requested_effort(mut self, effort: Option<String>) -> Self {
        self.requested_effort = effort;
        self
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
    /// Ask for a session mode by intent (`read-only`).
    #[must_use]
    pub fn with_requested_mode(mut self, mode: impl Into<String>) -> Self {
        self.requested_mode = Some(mode.into());
        self
    }
}

/// What an `agent:` session must hold to (NIK-266) — the workflow's declared `permits:`
/// projected onto the harness's own approval mode. Either way the client selects only a
/// mode MEASURED to send every approval to the engine (never one that approves on its own:
/// an auto-reviewer, accept-edits, full access, bypass) and reads it back before the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HarnessConfinement {
    /// The workflow grants no `fs.write`: the most restrictive mode the harness offers.
    ReadOnly,
    /// The workflow grants `fs.write`: a mode that may write inside the session root, every
    /// approval it asks still reaching the engine.
    WorkspaceWrite,
}

impl HarnessConfinement {
    /// The stable word (journal · refusal lines).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
        }
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
#[derive(Debug)]
#[non_exhaustive]
pub enum HarnessEvent {
    /// A chunk of the agent's message text.
    MessageChunk {
        /// The text delta.
        text: String,
    },
    /// The peer reported image-generation activity; this does not grant permission.
    ImageActivityObserved {
        /// The peer-local operation id.
        tool_call_id: String,
    },
    /// Image evidence received from a harness; never permission or a verified file.
    ImageObserved {
        /// The peer-reported payload and path.
        image: Box<HarnessImage>,
    },
    /// The harness asked to do something — the QUESTION verbatim plus
    /// the reply lane. The engine answers (permits bridge · B5); an
    /// unanswered drop reads as [`PermissionDecision::Deny`] fail-closed.
    PermissionAsked {
        /// The harness's own description of the action.
        question: String,
        /// The one-shot reply lane.
        reply: PermissionReply,
        /// The wire's toolCall `kind` (`execute` · `edit` · `fetch` …)
        /// when the agent declared one — the bridge's judge reads this
        /// (B5); absent means unverifiable, which pauses (fail-closed).
        kind: Option<String>,
        /// The paths the action touches (`toolCall.locations[].path`), as the client
        /// anchored them on the session root (NIK-266): root-relative inside it — the
        /// vocabulary `permits.fs` globs are authored in — absolute outside it, each followed
        /// by its effective identity when a symlink moves it, so every one must fit.
        locations: Vec<String>,
        /// The argv of an `execute` ask (`rawInput.command`, array
        /// form) — empty for a prose-only ask.
        command: Vec<String>,
        /// The URL of a `fetch` ask (`rawInput.url`).
        url: Option<String>,
    },
    /// An ask that reaches beyond its own kind (NIK-266): it opens network access (a fetch, or
    /// a command the harness says needs the network) or asks the harness to widen its own
    /// sandbox. The same reply lane as [`Self::PermissionAsked`], but no grant answers it:
    /// only the operator does. A consumer that does not know this variant drops the reply,
    /// which the harness hears as `cancelled` (fail-closed).
    EscalationAsked {
        /// The harness's own description of the action.
        question: String,
        /// The one-shot reply lane.
        reply: PermissionReply,
        /// The wire's toolCall `kind` (`execute` for a command), as in
        /// [`Self::PermissionAsked`].
        kind: Option<String>,
        /// The argv of the command (`rawInput.command`, array form), empty for a shell string.
        command: Vec<String>,
        /// The paths the ask names, anchored like [`Self::PermissionAsked`]'s `locations`.
        locations: Vec<String>,
        /// What the ask reaches beyond its kind.
        escalation: HarnessEscalation,
    },
    /// The confinement the session settled before its prompt (NIK-266): the mode it runs
    /// under and what the harness still does without asking. An auditor reads WHICH authority
    /// the harness kept for itself before any permission ask.
    SessionConfined {
        /// The approval mode the session runs under, as the harness names it (`None`: it
        /// advertises no mode at all).
        mode: Option<String>,
        /// `true` when the client selected the measured mode and read it back; `false` when
        /// no measured mode exists for this harness and its own was only observed.
        selected: bool,
        /// What the harness still decides itself, without asking, in the engine's words.
        unasked: String,
    },
    /// The harness called a client method this client does not offer (`fs/*` ·
    /// `terminal/*` · any other): answered with a JSON-RPC error, never served (NIK-266).
    ClientCallRefused {
        /// The method the agent called, verbatim.
        method: String,
    },
    /// The run completed — the terminal beat. Boxed: the outcome is
    /// the fat variant and every OTHER beat is hot-path (the clippy
    /// large-variant law · one allocation at the single terminal beat).
    Completed {
        /// The final outcome.
        outcome: Box<HarnessOutcome>,
    },
}

/// What an ask reaches beyond its own kind (NIK-266), read by the wire client from every
/// structured signal the ask carries. Never covered by a grant: the bridge pauses it for the
/// operator, the target in view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct HarnessEscalation {
    /// The network target the ask opens: a URL (`https://host`), the `<protocol>://<host>`
    /// of a Codex network approval (`socks5Tcp://host`), or how the harness named the access
    /// when it named no target.
    pub network: Option<String>,
    /// The widening of the harness's own sandbox the ask asks, summarized (Codex
    /// `additionalPermissions`: `network`, `write /`; Claude Code `dangerouslyDisableSandbox`).
    pub widens: Option<String>,
}

impl HarnessEscalation {
    /// Construct (INV-019): nothing asked beyond the command.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The network target the ask opens.
    #[must_use]
    pub fn with_network(mut self, target: impl Into<String>) -> Self {
        self.network = Some(target.into());
        self
    }

    /// The sandbox widening the ask asks.
    #[must_use]
    pub fn with_widens(mut self, widens: impl Into<String>) -> Self {
        self.widens = Some(widens.into());
        self
    }

    /// Whether the ask reaches nothing beyond its kind.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.network.is_none() && self.widens.is_none()
    }
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
    /// How `observed_model` was learned. `None` = unspecified, which is never
    /// treated as a response attestation.
    pub observed_model_source: Option<ModelProvenance>,
    /// Images received during this turn; paths are peer claims, never opened here.
    pub images: Vec<HarnessImage>,
    /// What the client SENT to configure the session and what the session read back for the
    /// effort — kept apart from the request (what was asked) and from `observed_model`.
    pub selection: HarnessSelection,
}

impl HarnessOutcome {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            usage: None,
            observed_model: None,
            observed_model_source: None,
            images: Vec::new(),
            selection: HarnessSelection::default(),
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

/// How one session was configured: the exact values the client sent and what the session
/// reported back for the reasoning effort. `None` everywhere means nothing was set (the
/// session kept its own defaults) — never « the requested value applied ».
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct HarnessSelection {
    /// The session's own model option id the model was sent through, or the legacy method
    /// name (`session/set_model`) when the agent offers only its model list.
    pub model_option: Option<String>,
    /// The model value sent (`session/set_config_option` value · legacy `modelId`).
    pub transmitted_model: Option<String>,
    /// The session's own reasoning option id the effort was sent through (`reasoning_effort` ·
    /// `effort` …), as advertised for the selected model.
    pub effort_option: Option<String>,
    /// The native effort value sent, verbatim.
    pub transmitted_effort: Option<String>,
    /// The effort the session reported current after the last selection: `confirmed_selection`
    /// when read back from the answer to the selection, `session_config` when nothing was sent.
    pub configured_effort: Option<String>,
    /// How `configured_effort` was learned.
    pub configured_effort_source: Option<ModelProvenance>,
    /// The agent itself moved the model or the effort away from what was configured during the
    /// turn (a `config_option_update` · e.g. a rate-limit fallback): the values it announced,
    /// `dimension=value`, in arrival order. Recorded, never hidden.
    pub changed_mid_turn: Vec<String>,
}

impl HarnessSelection {
    /// The refusal an EXACT selection owes an answer during which the agent moved a model or
    /// an effort the client had applied (`fallback: none` — the answer was not produced under
    /// it). `None` when nothing applied moved: a move of a dimension nobody set stays a fact
    /// of the receipt, never a refusal.
    #[must_use]
    pub fn moved_refusal(&self) -> Option<HarnessError> {
        let moved: Vec<&str> = self
            .changed_mid_turn
            .iter()
            .filter(|change| {
                (change.starts_with("model=") && self.transmitted_model.is_some())
                    || (change.starts_with("effort=") && self.transmitted_effort.is_some())
            })
            .map(String::as_str)
            .collect();
        (!moved.is_empty()).then(|| HarnessError::Selection {
            reason: format!(
                "the route moved {} during the turn; an explicit selection is exact, so no \
                 answer is accepted",
                moved.join(" · ")
            ),
        })
    }
}

/// Where a harness-reported model identity comes from. None of these is an
/// attestation that a response was produced by that model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModelProvenance {
    /// The session's current model option, as the harness stated it at creation.
    SessionConfig,
    /// The current value the harness returned after the client selected a model.
    ConfirmedSelection,
    /// A model id the harness accepted without echoing a current value.
    AcceptedRequest,
}

impl ModelProvenance {
    /// The stable wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SessionConfig => "session_config",
            Self::ConfirmedSelection => "confirmed_selection",
            Self::AcceptedRequest => "accepted_request",
        }
    }
}

/// One terminal image result reported by an external harness. No file authority is implied.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct HarnessImage {
    /// Correlates only within this delegated session, never across runs.
    pub tool_call_id: String,
    /// MIME declared with received bytes; absent for a path-only observation.
    pub mime_type: Option<String>,
    /// Received decoded bytes, transient until the verb persists them to its injected store.
    pub data: Option<bytes::Bytes>,
    /// Size of the bytes actually received; absent for a path-only report.
    pub received_bytes: Option<u64>,
    /// Local CAS metadata, only after the received bytes were successfully stored.
    pub stored_blob: Option<nika_kernel_core::io::blob::BlobMetadata>,
    /// Why storing the received bytes failed, when the store answered with a refusal.
    /// With bytes, no blob and no failure, the storage outcome is unconfirmed.
    pub storage_failure: Option<String>,
    /// SHA-256 of received decoded bytes; not a digest of the reported file.
    pub sha256: Option<String>,
    /// The peer's savedPath claim. No file was opened or verified by the client.
    pub reported_saved_path: Option<String>,
}
impl HarnessImage {
    /// Start an observation with no bytes, MIME or file claim.
    #[must_use]
    pub fn new(tool_call_id: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            mime_type: None,
            data: None,
            received_bytes: None,
            stored_blob: None,
            storage_failure: None,
            sha256: None,
            reported_saved_path: None,
        }
    }

    /// What happened to the received bytes: `none` (path-only report), `stored`,
    /// `failed`, or `unconfirmed` (received, but no store answer was observed —
    /// pending, or the operation was cancelled; a blob may or may not exist).
    #[must_use]
    pub fn storage(&self) -> &'static str {
        match (
            self.received_bytes,
            &self.stored_blob,
            &self.storage_failure,
        ) {
            (None, _, _) => "none",
            (Some(_), Some(_), _) => "stored",
            (Some(_), None, Some(_)) => "failed",
            (Some(_), None, None) => "unconfirmed",
        }
    }
    /// Additive receipt evidence, separate from a task's text output and authority.
    #[must_use]
    pub fn observation(&self) -> serde_json::Value {
        serde_json::json!({"schema": "nika/harness-image-observation@1",
            "source": "harness_reported", "tool_call_id": self.tool_call_id,
            "mime_type": self.mime_type, "received_bytes": self.received_bytes,
            "blob": self.stored_blob.as_ref().map(|blob| serde_json::json!({
                "hash": blob.hash, "mime_type": blob.mime_type, "size": blob.size})),
            "storage": self.storage(), "storage_failure": self.storage_failure,
            "received_sha256": self.sha256, "reported_saved_path": self.reported_saved_path,
            "file_verified": false, "permission_evidence": "separate permit_checked frames"})
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
    /// Nika's own judgment of the route's offer against an explicit
    /// selection — a model or effort the session does not offer, or a
    /// read-back that does not confirm it — before any prompt. The words
    /// are Nika's, never the adapter's text, so any surface may show them.
    #[error("harness refused: {reason}")]
    Selection {
        /// The refusal, naming the option, the value and the offer.
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

/// The OBJECT-SAFE erasure of [`AgentBackendDyn`] — an async-fn trait
/// cannot ride behind `dyn`, and the verb's optional 4th seam must
/// (the observer precedent: a generic would infect every embedder
/// signature). The blanket impl makes every backend an
/// `Arc<dyn DynAgentBackend>` for free; consumers never implement
/// this trait directly.
pub trait DynAgentBackend: Send + Sync {
    /// [`AgentBackend::run_agent`], boxed.
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>>;
}

impl<B: AgentBackendDyn + Sync> DynAgentBackend for B {
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>> {
        Box::pin(self.run_agent(request))
    }
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

    /// Under an exact selection only a move of an APPLIED dimension refuses;
    /// a dimension nobody set stays a receipt fact.
    #[test]
    fn only_a_move_of_an_applied_dimension_refuses() {
        let mut selection = HarnessSelection {
            changed_mid_turn: vec!["model=sonnet".into(), "effort=low".into()],
            ..HarnessSelection::default()
        };
        assert!(selection.moved_refusal().is_none(), "nothing was applied");
        let reason = |selection: &HarnessSelection| match selection.moved_refusal() {
            Some(HarnessError::Selection { reason }) => reason,
            _ => String::new(),
        };
        selection.transmitted_effort = Some("max".into());
        let effort_only = reason(&selection);
        assert!(effort_only.contains("effort=low"), "{effort_only}");
        assert!(!effort_only.contains("model=sonnet"), "{effort_only}");
        selection.transmitted_model = Some("opus".into());
        let both = reason(&selection);
        assert!(both.contains("model=sonnet · effort=low"), "{both}");
        assert!(both.contains("an explicit selection is exact"), "{both}");
    }

    /// NIK-266 · an escalation is empty until it names a network target or a widening, and
    /// each builder sets exactly its own field.
    #[test]
    fn an_escalation_names_what_the_command_reaches_beyond_running() {
        assert!(HarnessEscalation::new().is_empty());
        let network = HarnessEscalation::new().with_network("socks5Tcp://exfil.example");
        assert_eq!(
            network.network.as_deref(),
            Some("socks5Tcp://exfil.example")
        );
        assert!(network.widens.is_none() && !network.is_empty());
        let widens = HarnessEscalation::new().with_widens("write /");
        assert_eq!(widens.widens.as_deref(), Some("write /"));
        assert!(widens.network.is_none() && !widens.is_empty());
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
