// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;

/// One stateless authoring request. Answers belong to this request, never a chat session.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CompileRequest {
    pub(super) input: Input,
    pub(super) answers: BTreeMap<String, String>,
    pub(super) workflow_id: Option<String>,
    pub(super) authoring: Option<AuthoringPolicy>,
    pub(super) hot: HotPolicy,
    /// A previously produced private plan to replay for the same intent (see [`Self::with_plan`]).
    pub(super) plan: Option<serde_json::Value>,
}

/// How much the deterministic reader may decide on its own. False HOT is the P0 defect:
/// the default admits HOT only on positive structural evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum HotPolicy {
    /// HOT only when every clause is explicit (short canonical operation, typed literal,
    /// no coordinated residue) and no ambiguity, policy or authority question remains.
    #[default]
    Strict,
    /// The pre-refactor admission: every clause consumed by the reader. Ablation only.
    Legacy,
    /// Never HOT for free prose: exact skeletons and the support grammar only; everything
    /// else needs a seat. Ablation only.
    Off,
}

impl HotPolicy {
    /// The stable machine word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Legacy => "legacy",
            Self::Off => "off",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum Input {
    Create(String),
    Edit { source: String, change: EditChange },
}

#[derive(Clone, Debug)]
pub(super) enum EditChange {
    Text(String),
    Constant { name: String, literal_json: String },
}

impl CompileRequest {
    /// Allow one bounded authoring call through `compile_with_provider`.
    /// This does not select a runtime model or grant workflow authority.
    #[must_use]
    pub fn with_authoring_policy(mut self, policy: AuthoringPolicy) -> Self {
        self.authoring = Some(policy);
        self
    }
    /// Choose the HOT admission contract (Strict by default; Legacy/Off are ablations).
    #[must_use]
    pub fn with_hot_policy(mut self, hot: HotPolicy) -> Self {
        self.hot = hot;
        self
    }
    /// Replay the private semantic plan a previous round produced for the SAME intent:
    /// `plan` is the exact `provenance.plan` value of that outcome. The compiler then skips
    /// reading, decision seats and generative proposals entirely and assembles this plan
    /// with the request's answers, so every answer round of one authoring conversation
    /// reaches the same candidate with zero provider calls. The caller guarantees the
    /// intent is unchanged; the intent's sha256 is recorded in provenance either way. A
    /// plan that does not parse, is not anchored in the intent or still carries unknown
    /// work is a finding, never a candidate. Skeletons, `hello` and EDIT ignore it.
    #[must_use]
    pub fn with_plan(mut self, plan: serde_json::Value) -> Self {
        self.plan = Some(plan);
        self
    }
    /// Create from an exact skeleton or bounded support clauses. Other intents
    /// remain incomplete unless an explicit provider authoring call resolves them.
    #[must_use]
    pub fn create(intent: impl Into<String>) -> Self {
        Self {
            input: Input::Create(intent.into()),
            answers: BTreeMap::new(),
            workflow_id: None,
            authoring: None,
            hot: HotPolicy::default(),
            plan: None,
        }
    }

    /// Edit accepted source using a textual change request, without hidden conversation state.
    ///
    /// This foundation supports `Set const.NAME to JSON_LITERAL` and
    /// `Set const.NAME` followed by an answer to the returned question.
    /// Unsupported changes preserve the source and remain incomplete.
    #[must_use]
    pub fn edit(base_workflow: impl Into<String>, change_request: impl Into<String>) -> Self {
        Self {
            input: Input::Edit {
                source: base_workflow.into(),
                change: EditChange::Text(change_request.into()),
            },
            answers: BTreeMap::new(),
            workflow_id: None,
            authoring: None,
            hot: HotPolicy::default(),
            plan: None,
        }
    }

    /// Set one existing constant from structured input, without a textual prompt.
    ///
    /// `name` is the bare constant name (ASCII letters, digits or underscores),
    /// not a path. `literal_json` is exactly one JSON literal, decoded and judged
    /// by the same operation as `Set const.NAME to JSON_LITERAL`. Invalid names,
    /// absent constants and malformed literals preserve the original source.
    /// Expression islands and root objects with both `type` and `value` are refused.
    /// An integer outside the exact `i64` range is refused rather than rounded.
    /// The caller owns source selection, revision checks (CAS) and materialization.
    #[must_use]
    pub fn set_constant(
        base_workflow: impl Into<String>,
        name: impl Into<String>,
        literal_json: impl Into<String>,
    ) -> Self {
        Self {
            input: Input::Edit {
                source: base_workflow.into(),
                change: EditChange::Constant {
                    name: name.into(),
                    literal_json: literal_json.into(),
                },
            },
            answers: BTreeMap::new(),
            workflow_id: None,
            authoring: None,
            hot: HotPolicy::default(),
            plan: None,
        }
    }

    /// Name a newly created workflow explicitly, independently of its skeleton.
    /// EDIT refuses this option: destination selection must not alter an accepted base.
    #[must_use]
    pub fn with_workflow_id(mut self, id: impl Into<String>) -> Self {
        self.workflow_id = Some(id.into());
        self
    }

    /// Supply a JSON literal for a stable question key. Invalid answers are
    /// reported as incomplete authoring data by [`super::compile`], not thrown.
    /// Objects with both `type` and `value` are refused by this literal-only slice.
    /// An integer outside the exact `i64` range is refused rather than rounded;
    /// fraction and exponent literals are floats and quoted digits stay text.
    #[must_use]
    pub fn answer(mut self, key: impl Into<String>, literal_json: impl Into<String>) -> Self {
        self.answers.insert(key.into(), literal_json.into());
        self
    }
}

/// Completeness of authoring, never permission to execute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompileStatus {
    /// Candidate exists, all questions are answered and its pure preview is clean.
    Ready,
    /// A value, clarification or unsupported semantic region remains unresolved.
    Incomplete,
    /// The request violates literal-only or source-preservation policy.
    Refused,
}

/// The shape an authoring answer must have; it is not a runtime approval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum QuestionType {
    /// A JSON string used as literal text.
    Text,
    /// A literal JSON value. Its Nika type is also checked after emission.
    Literal,
    /// A JSON string that is the `key` of one of the question's own [`CompileQuestion::options`].
    Choice,
}

/// One admissible answer of a [`QuestionType::Choice`] question: its key, as the answer is
/// written, and its human label. The keys are the owning grammar's own spellings.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChoiceOffer {
    /// The answer, verbatim (`rattraper-une-fois`).
    pub key: String,
    /// What choosing it means.
    pub label: String,
}

impl ChoiceOffer {
    /// One admissible answer.
    #[must_use]
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
        }
    }
}

/// A stable question shared by future TTY, SDK and Serve adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompileQuestion {
    /// Semantic hole path, such as `const.request`; never a random session id.
    pub key: String,
    /// Human wording of the missing value.
    pub label: String,
    /// Required answer shape.
    pub answer_type: QuestionType,
    /// Why compilation cannot complete without it.
    pub why: String,
    /// Whether this question blocks Ready.
    pub mandatory: bool,
    /// The admissible answers of a [`QuestionType::Choice`] question; empty for any other.
    pub options: Vec<ChoiceOffer>,
}

/// What happened to a requested part of authoring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticKind {
    /// An explicit request or answer was applied.
    Applied,
    /// The requested change was not applied.
    Missed,
    /// The compiler does not know the requested semantics.
    Unknown,
    /// A literal value or clarification must come from the caller.
    RequiresHuman,
    /// An explicit compiler policy refused the request.
    Refused,
}

/// A structured authoring finding, separate from the Check report.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompileDiagnostic {
    /// Disposition of the request fragment.
    pub kind: DiagnosticKind,
    /// The request or hole this finding concerns.
    pub target: String,
    /// Explanation for the caller; never parsed to recover compiler state.
    pub message: String,
}

/// The depth of the preview. Environment checks and admission still belong to Run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PreviewScope {
    /// The real pure Check ladder over the source. No child files, skill files,
    /// credential probes, access plan or execution admission were evaluated.
    SourceOnly,
}

/// The ordinary Check result, with its intentionally limited evaluation depth.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CompilePreview {
    /// The same pure report used by the engine's Check ladder.
    pub report: nika_check::CheckReport,
    /// What was judged; a clean report is not an environment/admission claim.
    pub scope: PreviewScope,
}

/// Authoring cognition is explicit and independent of runtime workflow models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthoringCognition {
    /// Deterministic resolution only. Unknown intent cannot silently contact a model.
    DeterministicOnly,
    /// One explicitly authorized call through the kernel provider seam.
    ExplicitProvider,
    /// Bounded closed choices through an explicitly seated decision capability; no generative call.
    ExplicitDecision,
}

/// Which internal resolution settled a CREATE; observational, never authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Strategy {
    /// An exact embedded skeleton.
    Skeleton,
    /// The bounded support clause grammar.
    Support,
    /// Every clause read deterministically; zero seat calls.
    Hot,
    /// Finite ambiguities settled by a bounded decision seat.
    Warm,
    /// One generative proposal, constrained by the deterministic facts.
    Cold,
    /// A native candidate the seat wrote from the authoring workspace's knowledge, judged by
    /// the parser, the Check and the fidelity laws, repaired from their diagnostics.
    Native,
}

impl Strategy {
    /// The stable machine word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Skeleton => "skeleton",
            Self::Support => "support",
            Self::Hot => "hot",
            Self::Warm => "warm",
            Self::Cold => "cold",
            Self::Native => "native",
        }
    }
    /// The strategy a recorded plan names, if the word is one of ours.
    pub(super) fn parse(word: &str) -> Option<Self> {
        [
            Self::Skeleton,
            Self::Support,
            Self::Hot,
            Self::Warm,
            Self::Cold,
            Self::Native,
        ]
        .into_iter()
        .find(|strategy| strategy.word() == word)
    }
}

/// When the native strategy (a seat-written candidate judged by the parser, the Check and the
/// fidelity laws) is engaged for a free intent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum NativeMode {
    /// Never: the private plan is the only generative path (the library's default, so that
    /// a caller's calls stay exactly what it asked for; the CLI opts into `Escalate`).
    #[default]
    Off,
    /// After the private plan ends without a candidate, fails the fidelity laws or hands
    /// the human a machine's problem (a rewrite, a jq expression, a glob).
    Escalate,
    /// Straight to the native candidate, before the deterministic door and without the
    /// private plan (the ablation, and the arena's treatment D).
    Only,
}

impl NativeMode {
    /// The stable machine word.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Escalate => "escalate",
            Self::Only => "only",
        }
    }
}

/// Explicit limits for one authoring call. Ambient credentials are not consent.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct AuthoringPolicy {
    pub(super) model: String,
    pub(super) max_tokens: u32,
    pub(super) timeout: std::time::Duration,
    pub(super) samples: u32,
    pub(super) native: NativeMode,
    pub(super) repairs: u32,
}
impl AuthoringPolicy {
    /// When the native candidate is written (default: after the private plan fails a human).
    #[must_use]
    pub fn with_native(mut self, native: NativeMode) -> Self {
        self.native = native;
        self
    }
    /// How many repair rounds a native candidate may buy (0..=5, default 3): one call each.
    #[must_use]
    pub fn with_repairs(mut self, repairs: u32) -> Self {
        self.repairs = repairs.min(5);
        self
    }
    /// Ask for `samples` independent proposals (1..=5) and keep the one the others agree
    /// with most; disagreement is recorded, never voted away. Each sample is one call.
    #[must_use]
    pub fn with_samples(mut self, samples: u32) -> Self {
        self.samples = samples.clamp(1, 5);
        self
    }
    /// Permit one call with an explicit model, output-token cap and timeout.
    /// Invalid or unbounded limits yield Incomplete without calling a provider.
    #[must_use]
    pub fn new(model: impl Into<String>, max_tokens: u32, timeout: std::time::Duration) -> Self {
        Self {
            model: model.into(),
            max_tokens,
            timeout,
            samples: 1,
            native: NativeMode::default(),
            repairs: 3,
        }
    }
}

/// Measured authoring metadata; not workflow authority or execution proof.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuthoringReceipt {
    /// Explicit model requested for this authoring attempt.
    pub model: String,
    /// Provider calls attempted: one per proposal sample, plus at most one bounded repair
    /// call per sample when the proposal cited an evidence the request never wrote.
    pub calls: u32,
    /// Reported input tokens, or unknown when the provider omitted usage.
    pub input_tokens: Option<u64>,
    /// Reported output tokens, or unknown when the provider omitted usage.
    pub output_tokens: Option<u64>,
    /// Wall time spent awaiting the provider, including timeout/failure.
    pub elapsed_ms: u64,
    /// What each call received, in call order: its role (`plan` · `repair` · `transform`),
    /// the sha256 of the instruction and of the answer schema it was given, the bytes of its
    /// messages, and the references sent with it (none today: recall is recorded, never
    /// sent). A journal of what the seat actually read, never of what the repository holds.
    pub context: Vec<serde_json::Value>,
    /// The backend that answered, named by the transport that seated it: `direct_api` (a
    /// provider of the registry, tokens metered) or `acp_harness` (the operator's own agent
    /// harness through ACP: adapter, observed model, cost basis, no fabricated token meter).
    /// None when the transport did not say.
    pub backend: Option<serde_json::Value>,
}

/// Authoring provenance is not program identity or execution Proof.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompileProvenance {
    /// Present only on the explicit provider path (wire generation 2).
    pub authoring: Option<AuthoringReceipt>,
    /// Version of the compiling engine.
    pub compiler_version: String,
    /// Exact embedded language-pack source revision.
    pub spec_pin: String,
    /// The explicitly selected skeleton, if this is a CREATE request.
    pub skeleton: Option<String>,
    /// Authoring cognition policy; independent of runtime model configuration.
    pub cognition: AuthoringCognition,
    /// The internal strategy that settled a free intent, when one was engaged.
    pub strategy: Option<Strategy>,
    /// The private semantic plan projection (operations, effects, obligations), when read.
    pub plan: Option<serde_json::Value>,
    /// Bounded decision records (seat, questions, choices, reported usage), when a seat was asked.
    pub decision: Option<serde_json::Value>,
    /// A file name for the candidate, derived from what it writes or does (`open-sorted.nika`):
    /// a suggestion for whoever saves it, never a path the compiler touched.
    pub suggested_file: Option<String>,
}

/// What starts a run of the candidate, when the request names it ("Every morning at 9,
/// …", "for each incoming ticket, …"): a requirement stated beside the candidate, never
/// inside its bytes. The same bytes run locally, in two workspaces and on a server with
/// different bindings; binding the trigger is an operator or product gesture through the
/// schedule contract (`nika.yaml arm:`, `PUT /v1/schedules`), never the compiler's. A
/// requirement is not a grant and not a schedule row.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TriggerRequirement {
    /// How the request expects runs to start.
    pub kind: TriggerKind,
    /// The request's own words for the trigger, verbatim ("every morning at 9"): a hint
    /// for whoever binds it, never a binding.
    pub source_hint: Option<String>,
    /// The event the words name, when the compiler reads one.
    pub event_hint: Option<String>,
    /// The cadence the words state, when they state one: `daily` · `weekdays` · `weekly`
    /// · `monthly` · `hourly` · `minutely`.
    pub cadence: Option<String>,
    /// The time of day the words state, as `HH:MM`, when they state one.
    pub at: Option<String>,
    /// The declared input each firing supplies (`item`), when the candidate declares one.
    pub payload_input: Option<String>,
    /// Whether the requirement is met by the candidate alone or needs a binding.
    pub status: TriggerStatus,
    /// The IANA timezone answered for the cadence (`trigger.timezone`), when answered.
    pub timezone: Option<String>,
    /// The missed-run policy answered (`trigger.missed`): one of the project grammar's own
    /// spellings (`manqué:`), when answered.
    pub missed: Option<String>,
    /// The overlap policy answered (`trigger.overlap`): one of the cadence grammar's own
    /// spellings (`chevauchement:`), when answered.
    pub overlap: Option<String>,
    /// The per-run spend ceiling answered (`trigger.ceiling`): the positive JSON number, as
    /// its canonical text (`0.1`, `2`).
    pub ceiling: Option<String>,
}

/// How a request expects runs of its candidate to start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TriggerKind {
    /// Started by hand (`nika run`).
    Manual,
    /// A cadence: "every morning at 9", "chaque lundi à 8h".
    Schedule,
    /// An incoming HTTP call from a named system.
    Webhook,
    /// One run per incoming item or occurrence: "for each incoming ticket".
    Event,
}

/// Whether a trigger requirement is met by the candidate alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TriggerStatus {
    /// Nothing to bind: the candidate runs when invoked.
    Satisfied,
    /// An operator or product binds it through the schedule contract.
    RequiresBinding,
    /// The compiler cannot express the trigger it read.
    Unsupported,
}

/// A reviewable authoring result. No field grants authority, writes or executes source.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CompileOutcome {
    /// Whether authoring completed under the supported semantics.
    pub status: CompileStatus,
    /// Ordinary `.nika` source in memory; may still be incomplete. Path-free.
    pub candidate: Option<String>,
    /// Mandatory holes; callers recompile with explicit answers.
    pub questions: Vec<CompileQuestion>,
    /// Applied, missed and unknown request fragments.
    pub diagnostics: Vec<CompileDiagnostic>,
    /// Requested boundary, derived from the candidate's Check report; not a grant.
    pub requested_boundary: Option<nika_check::EffectivePermits>,
    /// The trigger the request names, as a requirement beside the candidate (nika#1720);
    /// the candidate's bytes carry no cadence, host or event.
    pub requested_trigger: Option<TriggerRequirement>,
    /// The candidate's in-memory static judgment, when it parses.
    pub check_preview: Option<CompilePreview>,
    /// Reproduction metadata, not run evidence.
    pub provenance: CompileProvenance,
}

/// Compiler machinery failures. Missing answers and unsupported requests are outcomes.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum CompileError {
    /// A declared embedded skeleton has no source body.
    #[error("embedded skeleton `{0}` has no source")]
    MissingSkeleton(String),
    /// The embedded registry supplied a source that the current parser cannot read.
    #[error("embedded skeleton cannot be parsed: {0}")]
    Registry(#[source] nika_schema::SchemaError),
    /// A parsed source could not be represented or emitted by the deterministic assembler.
    #[error("candidate representation failed: {0}")]
    Representation(#[from] RepresentationError),
}

impl CompileError {
    pub(super) fn representation(error: serde_yaml_bw::Error) -> Self {
        Self::Representation(RepresentationError { source: error })
    }
}

/// The opaque representation backend failure, retained as an error source.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
#[non_exhaustive]
pub struct RepresentationError {
    source: serde_yaml_bw::Error,
}

impl nika_error::traits::NikaErrorCode for CompileError {
    fn nika_code(&self) -> nika_error::codes::NikaCode {
        match self {
            Self::Registry(error) => error.nika_code(),
            Self::MissingSkeleton(_) | Self::Representation(_) => nika_error::codes::NIKA_999,
        }
    }

    fn spec_code(&self) -> String {
        match self {
            Self::Registry(error) => error.spec_code().to_string(),
            Self::MissingSkeleton(_) | Self::Representation(_) => self.nika_code().to_string(),
        }
    }
}
