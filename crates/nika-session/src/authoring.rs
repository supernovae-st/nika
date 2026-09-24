// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's door to the ONE compiler (product convergence · wave 1).
//!
//! A free-text turn that describes work reaches the canonical typed
//! Compile CREATE ([`nika_onboard::compile`]) — never a reasoner that
//! writes YAML. The compiler reads the intent; what it cannot settle it
//! asks as a typed [`CompileQuestion`] with a stable key; the human's next
//! line answers THAT key; the private plan the first round produced is
//! replayed ([`CompileRequest::with_plan`]) so an answer round costs zero
//! provider calls; a Ready candidate is exact bytes the human reviews and
//! consents to elsewhere ([`crate::change`] · [`crate::review`]).
//!
//! The seat the compiler may reason with is the ONE the human chose for
//! this session (`/intelligence`): an API or a local engine becomes an
//! explicit [`AuthoringPolicy`] on that same model; a supported subscription
//! uses its own tool-free harness. No intelligence stays deterministic. Ambient keys are never
//! consent; nothing here selects a provider the human did not name.
//!
//! Under a provider seat the policy carries the session's
//! [`AuthoringContext`] — the strategy (when the seat writes the candidate
//! itself, the same default as `nika compile`: escalate) and the Foundry
//! knowledge snapshot pinned when the session opened, its pack composed for
//! each request and verified before a byte is sent. What the session
//! attached is stamped beside the compiler's own record
//! (`decision.session.authoring`): the pinned identity, the pack's digest,
//! the references, and whether the native door presented them to the seat.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use nika_cli_host::compile::knowledge::{PACK_BUILDER, pack_sha256};
use nika_event::source_id::sha256_hex;
use nika_onboard::compile::{
    AuthoringKnowledge, AuthoringPolicy, AuthoringReceipt, Cognition, CompileError, CompileOutcome,
    CompileQuestion, CompileRequest, CompileStatus, DiagnosticKind, QuestionType, Strategy,
    compile, compile_with_cognition,
};
use serde_json::{Value, json};

use crate::intelligence::{IntelligenceKind, ResolvedSessionIntelligence};
use crate::reasoner::SessionReasoner;

mod context;
pub(crate) mod decision;
mod harness;
pub use context::{AuthoringContext, AuthoringContextError, KnowledgePin};
pub use decision::{DECISION_ENV, DECISION_SCHEMA, DecisionSetup, MAX_DECISION_CALLS};

/// The compiler's question for a whole replacement request (its own key).
const CLARIFICATION_KEY: &str = "intent.clarification";

/// The stronger authoring model of a provider, when the catalog holds one
/// the preflight proved — the escalation the product law permits (quality
/// first): `None` when the model already is the strongest, or unknown.
#[must_use]
pub fn stronger_model(model: &str) -> Option<&'static str> {
    stronger_model_under(model, false)
}

/// The table behind [`stronger_model`], with the gateway fact explicit: an
/// OpenAI-compatible base URL (Scaleway's gateway, a local server) serves
/// its OWN models under the `openai` provider id — the provider's flagship
/// is not there, so no escalation is offered across it.
#[must_use]
pub fn stronger_model_under(model: &str, openai_base_overridden: bool) -> Option<&'static str> {
    let (provider, name) = model.split_once('/')?;
    let strongest = match provider {
        "openai" if openai_base_overridden => return None,
        "openai" => "openai/gpt-5.2",
        "xai" => "xai/grok-4.7",
        "deepseek" => "deepseek/deepseek-v4-pro",
        "gemini" => "gemini/gemini-2.5-pro",
        "mistral" => "mistral/mistral-large-latest",
        _ => return None,
    };
    (format!("{provider}/{name}") != strongest).then_some(strongest)
}

/// Whether the `openai` provider's base URL is overridden in this
/// environment (an OpenAI-compatible gateway), read through the engine's
/// registry — the same fact the run path uses, never a second env read.
#[must_use]
pub fn openai_base_overridden() -> bool {
    gateway_host("openai").is_some()
}

/// The host a provider's calls really go to when its base URL is
/// overridden (an OpenAI-compatible gateway such as Scaleway's, a local
/// server): `Some(host)` when the effective URL's host differs from the
/// provider profile's own; `None` when the provider talks to its own API.
/// Read through the engine's registry — the fact the run path uses.
#[must_use]
pub fn gateway_host(provider: &str) -> Option<String> {
    let http = crate::reasoner::provider_http().ok()?;
    let registry = nika_providers::ProviderRegistry::new(
        Arc::new(http),
        nika_runtime::compose::config_from_env(),
    );
    let effective = host_of(registry.effective_base_url(provider)?);
    let seed = registry
        .profiles()
        .iter()
        .find(|p| p.id == nika_providers::canonical_provider(provider))
        .map(|p| host_of(p.base_url))?;
    (effective != seed).then_some(effective)
}

/// The host part of a URL (`https://api.scaleway.ai/v1` → `api.scaleway.ai`).
#[must_use]
pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split('/').next().unwrap_or(rest).to_owned()
}
/// Output tokens one authoring call may spend (the compiler's ceiling; deep work needs room).
const AUTHORING_MAX_TOKENS: u32 = 8192;
/// Wall time one authoring call may take (the compiler's own ceiling).
const AUTHORING_TIMEOUT: Duration = Duration::from_secs(120);

/// The cognition the compiler may use for this session's authoring —
/// derived from the intelligence the human chose, never from the
/// environment.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthoringSeat {
    /// Exact skeletons, the bounded support grammar and strictly explicit
    /// intents only: no model reads a free intent.
    Deterministic {
        /// Why no model authors here, when there is a reason to name.
        why: Option<String>,
    },
    /// One explicitly permitted generative call on the model the human chose.
    Provider {
        /// `<provider>/<name>`.
        model: String,
    },
    /// The selected subscription adapter; never a provider fallback.
    Harness {
        /// Native adapter id (for example `codex` or `claude-code`).
        seat: String,
        /// Caller selection, or the harness default when absent.
        model: Option<String>,
    },
    /// A selected capability that this host/build cannot honor.
    Unavailable {
        /// Precise refusal, never interpreted as deterministic success.
        why: String,
    },
}

impl AuthoringSeat {
    /// The seat the human's choice permits: the reasoner that reasons with
    /// them names its API/local model or its subscription capability. No
    /// intelligence keeps deterministic facts; unavailable choices refuse.
    #[must_use]
    pub fn from_reasoner(
        reasoner: &dyn SessionReasoner,
        intelligence: &ResolvedSessionIntelligence,
    ) -> Self {
        if let IntelligenceKind::Harness { seat } = &intelligence.kind {
            if !intelligence.ready {
                return Self::Unavailable {
                    why: intelligence
                        .why
                        .clone()
                        .unwrap_or_else(|| format!("harness `{seat}` is not available")),
                };
            }
            #[cfg(feature = "access-harness")]
            if let Err(why) =
                nika_harness::authoring::HarnessAuthoring::meet(seat, intelligence.model.as_deref())
            {
                return Self::Unavailable { why };
            }
            #[cfg(not(feature = "access-harness"))]
            return Self::Unavailable {
                why: format!(
                    "subscription authoring `{seat}` requires access-harness in this build"
                ),
            };
            #[cfg(feature = "access-harness")]
            return if reasoner.authoring_harness().as_deref() == Some(seat.as_str()) {
                Self::Harness {
                    seat: seat.clone(),
                    model: intelligence.model.clone(),
                }
            } else {
                Self::Unavailable {
                    why: format!(
                        "the selected `{seat}` reasoner exposes no subscription authoring capability"
                    ),
                }
            };
        }
        match reasoner.authoring_model() {
            Some(model) => Self::Provider { model },
            None => Self::Deterministic {
                why: match &intelligence.kind {
                    IntelligenceKind::None => Some(
                        "no conversational intelligence: free intents are read deterministically — `/intelligence` to choose a model that reads them"
                            .to_owned(),
                    ),
                    _ => None,
                },
            },
        }
    }

    /// Whether authoring may use the explicitly selected model backend.
    pub(crate) fn has_model(&self) -> bool {
        matches!(self, Self::Provider { .. } | Self::Harness { .. })
    }

    /// The banner's word for the seat.
    #[must_use]
    pub fn line(&self) -> String {
        match self {
            Self::Deterministic { .. } => {
                "authoring · deterministic (exact intents only)".to_owned()
            }
            Self::Provider { model } => {
                format!("authoring · {model} (bounded calls per fresh intent)")
            }
            Self::Harness { seat, model } => format!(
                "authoring · {seat} subscription · {} · cost unknown",
                model.as_deref().unwrap_or("harness default")
            ),
            Self::Unavailable { why } => format!("authoring unavailable · {why}"),
        }
    }
}

/// What an authoring call could not do — machinery, never a missing value
/// (those are outcomes the compiler returns).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AuthoringError {
    /// The compiler's own machinery failure (a corrupt skeleton · representation).
    #[error("the compiler could not represent the candidate: {0}")]
    Compiler(#[from] CompileError),
    /// The provider seat could not be built or resolved (the model's name · the wire).
    #[error("the authoring seat is unavailable: {0}")]
    Seat(String),
    /// The session's own async runtime.
    #[error("the session runtime failed: {0}")]
    Runtime(String),
    /// The session's authoring configuration cannot be honored (a knowledge
    /// source that is not a snapshot, is stale or changed under the session;
    /// knowledge named under `off`; an unknown strategy): nothing was sent.
    #[error("the authoring configuration cannot be used: {0}")]
    Context(#[from] AuthoringContextError),
}

/// One authoring conversation over ONE intent: the answers the human gave
/// by stable key, the plan the first round read (replayed on every answer
/// round · zero provider calls), and the questions still open in order.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthoringRound {
    /// The intent, verbatim — the compiler's own key (its sha256 is recorded).
    pub intent: String,
    /// `key → JSON literal`, exactly what [`CompileRequest::answer`] takes.
    pub answers: BTreeMap<String, String>,
    /// The compiler's opaque continuation of this round (today `provenance.plan`,
    /// replayed through [`CompileRequest::with_plan`]): kept, never inspected.
    pub continuation: Option<Value>,
    /// The mandatory questions still open, first one first.
    pub questions: Vec<CompileQuestion>,
    /// The compiler's reasons for the open questions (its own words).
    pub reasons: Vec<String>,
    /// How many times the human restated a clause in words in place of a
    /// rule the compiler could only ask as code (bounded: one).
    pub restatements: u8,
    /// The session's record of the knowledge the call that authored
    /// `continuation` presented to the seat (the pinned identity, the pack's
    /// digest, its references): carried to every answer round that replays
    /// it, so the candidate keeps naming what it was authored from.
    pub knowledge: Option<Value>,
    /// Subscription evidence of the round that authored the replayed plan.
    /// Kept independently of optional knowledge; replay does not call this seat.
    pub authoring_receipt: Option<AuthoringReceipt>,
}

impl AuthoringRound {
    /// A fresh round over one intent.
    #[must_use]
    pub fn new(intent: impl Into<String>) -> Self {
        Self {
            intent: intent.into(),
            answers: BTreeMap::new(),
            continuation: None,
            questions: Vec::new(),
            reasons: Vec::new(),
            restatements: 0,
            knowledge: None,
            authoring_receipt: None,
        }
    }

    /// The intent the compiler reads for this round: an answered
    /// `intent.clarification` replaces the request (the compiler's own law),
    /// else the request as stated.
    #[must_use]
    pub fn effective_intent(&self) -> String {
        self.answers
            .get(CLARIFICATION_KEY)
            .and_then(|literal| serde_json::from_str::<Value>(literal).ok())
            .and_then(|value| value.as_str().map(str::to_owned))
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| self.intent.clone())
    }

    /// This round through the seat under the session's authoring context: a
    /// fresh round composes the knowledge pack for the intent the compiler
    /// reads; an answer round replays its recorded plan (zero calls) and
    /// carries the knowledge record of the round that authored it.
    ///
    /// # Errors
    /// As [`compile_in`].
    pub fn compile(
        &self,
        seat: &AuthoringSeat,
        context: &AuthoringContext,
    ) -> Result<CompileOutcome, AuthoringError> {
        let intent = self.effective_intent();
        let attach = if self.continuation.is_some() {
            Attach::Carried(self.knowledge.as_ref())
        } else {
            Attach::Compose(&intent)
        };
        let mut out = compile_attached(seat, context, &self.request(), attach, None)?;
        self.carry_subscription_receipt(&mut out);
        Ok(out)
    }

    /// The typed request this round is: the intent, every answer, the
    /// recorded plan when one settled.
    #[must_use]
    pub fn request(&self) -> CompileRequest {
        let mut request = CompileRequest::create(self.intent.clone());
        for (key, literal) in &self.answers {
            request = request.answer(key.clone(), literal.clone());
        }
        if let Some(plan) = &self.continuation {
            request = request.with_plan(plan.clone());
        }
        request
    }

    /// Keep what the outcome settled: the plan once a strategy settled it
    /// (a plan that still carries unknown work is never replayed) with the
    /// knowledge record of the call that authored it when the native door
    /// presented a pack, the mandatory questions in the compiler's order,
    /// its reasons.
    pub fn absorb(&mut self, out: &CompileOutcome) {
        if self.continuation.is_none() && out.provenance.strategy.is_some() {
            self.continuation.clone_from(&out.provenance.plan);
            if self.continuation.is_some() {
                self.knowledge = presented_knowledge(out);
                self.authoring_receipt = out
                    .provenance
                    .authoring
                    .as_ref()
                    .filter(|r| {
                        r.backend
                            .as_ref()
                            .is_some_and(|b| b["kind"] == "harness_infer")
                    })
                    .cloned();
            }
        }
        self.questions = out
            .questions
            .iter()
            .filter(|q| q.mandatory)
            .cloned()
            .collect();
        self.reasons = out
            .diagnostics
            .iter()
            .filter(|d| matches!(d.kind, DiagnosticKind::Unknown | DiagnosticKind::Missed))
            .map(|d| d.message.clone())
            .collect();
    }

    fn carry_subscription_receipt(&self, out: &mut CompileOutcome) {
        if self.continuation.is_none() || out.provenance.authoring.is_some() {
            return;
        }
        if let Some(mut receipt) = self.authoring_receipt.clone() {
            if let Some(backend) = receipt.backend.as_mut().and_then(Value::as_object_mut) {
                backend.insert("carried_from_authoring_round".into(), Value::Bool(true));
            }
            out.provenance.authoring = Some(receipt);
        }
    }

    /// The question the next line answers, when one is open.
    #[must_use]
    pub fn current(&self) -> Option<&CompileQuestion> {
        self.questions.first()
    }

    /// Answer the current question with the human's line, typed to the
    /// question's shape; the key it answered.
    pub fn answer_current(&mut self, line: &str) -> Option<String> {
        let question = self.questions.first()?.clone();
        let literal = literal_for(&question, line);
        self.answers.insert(question.key.clone(), literal);
        self.questions.remove(0);
        Some(question.key)
    }
    /// Compile this same round under the aggregate account (replay stays free).
    /// # Errors
    /// The same compiler/context errors as `compile`, or admission refusal.
    pub fn compile_with_admission(
        &self,
        seat: &AuthoringSeat,
        context: &AuthoringContext,
        account: &nika_providers::InferenceAdmission,
    ) -> Result<CompileOutcome, AuthoringError> {
        let intent = self.effective_intent();
        let attach = if self.continuation.is_some() {
            Attach::Carried(self.knowledge.as_ref())
        } else {
            Attach::Compose(&intent)
        };
        compile_attached(seat, context, &self.request(), attach, Some(account))
    }
}

/// The human's line as the JSON literal the question's shape takes: a
/// `Text` question takes the line as one string; a `Literal` question
/// takes the line verbatim when it already is JSON (`5` · `true` ·
/// `["a"]`), else as a string (`./notes` is a path, not a parse error);
/// a `Choice` takes the line verbatim when it already is a JSON string
/// (`"montant"`, the shape the compiler asks of a choice), else as one.
#[must_use]
pub fn literal_for(question: &CompileQuestion, line: &str) -> String {
    let line = line.trim();
    let already = match question.answer_type {
        QuestionType::Literal => serde_json::from_str::<Value>(line).is_ok(),
        QuestionType::Choice => matches!(serde_json::from_str::<Value>(line), Ok(Value::String(_))),
        _ => false,
    };
    if already {
        line.to_owned()
    } else {
        Value::String(line.to_owned()).to_string()
    }
}

/// The few words that abandon an authoring round (a `no` is an ANSWER —
/// « should each filename be a heading? » — never an abandonment).
#[must_use]
pub fn is_cancel(line: &str) -> bool {
    matches!(
        line.trim().to_lowercase().as_str(),
        "cancel"
            | "/cancel"
            | "stop"
            | "drop"
            | "discard"
            | "abandon"
            | "annule"
            | "annuler"
            | "laisse tomber"
            | "forget it"
            | "never mind"
    )
}

/// The few words that ask WHY beside what waits (a question, a gate) —
/// answered from the machine's state, consuming nothing. A closed set of
/// whole lines, punctuation aside.
#[must_use]
pub fn is_why(line: &str) -> bool {
    let word = line
        .trim()
        .trim_end_matches(['?', '!', '.', ' '])
        .to_lowercase();
    matches!(
        word.as_str(),
        "why"
            | "/why"
            | "why this"
            | "why this question"
            | "why do you ask"
            | "explain"
            | "explain this"
            | "pourquoi"
            | "pourquoi cette question"
            | "pourquoi ça"
            | "explique"
            | "c'est quoi"
            | "c'est pour quoi"
    )
}

/// The few words that ask what Nika understood of the request — the
/// Meaning view from the compiler's ledger; beside a proposal it holds it.
#[must_use]
pub fn is_meaning(line: &str) -> bool {
    let word = line
        .trim()
        .trim_end_matches(['?', '!', '.', ' '])
        .to_lowercase();
    matches!(
        word.as_str(),
        "/meaning"
            | "meaning"
            | "what did you understand"
            | "what did you keep"
            | "did you keep everything"
            | "qu'as-tu compris"
            | "qu'as-tu retenu"
            | "tu as tout gardé"
    )
}

/// The few words that ask what just went wrong — answered by the last
/// recovery card, from memory, never by another call.
#[must_use]
pub fn is_what_happened(line: &str) -> bool {
    let word = line
        .trim()
        .trim_end_matches(['?', '!', '.', ' '])
        .to_lowercase();
    matches!(
        word.as_str(),
        "what happened"
            | "what just happened"
            | "what went wrong"
            | "what was that"
            | "/last"
            | "de quoi"
            | "quoi"
            | "hein"
            | "comment ça"
            | "qu'est-ce qui s'est passé"
            | "qu'est-ce qui se passe"
    )
}

/// A bare greeting or thanks — the conversation's, never the compiler's
/// (whose exact-skeleton door would read a lone `hello` as the `hello`
/// lesson). A closed set of whole lines, punctuation aside.
#[must_use]
pub fn is_greeting(input: &str) -> bool {
    let word = input
        .trim()
        .trim_end_matches(['!', '.', '?', ',', ' '])
        .to_lowercase();
    matches!(
        word.as_str(),
        "hello"
            | "hi"
            | "hey"
            | "yo"
            | "bonjour"
            | "salut"
            | "coucou"
            | "thanks"
            | "thank you"
            | "merci"
            | "bye"
            | "au revoir"
    )
}

/// What one compile outcome means for the conversation — a closed
/// reading of the compiler's own typed fields, never of its prose.
#[derive(Debug)]
#[non_exhaustive]
pub enum Reading {
    /// A candidate exists and every mandatory question is answered.
    Ready(CompileOutcome),
    /// Mandatory questions remain: the next line answers the first.
    Questions(CompileOutcome),
    /// Work was read but not settled under this seat's policy (the
    /// compiler says so): a wider policy may settle it, or the human
    /// rephrases. How the compiler tried is its own business.
    Unsettled(CompileOutcome),
    /// Nothing recognizable as work: no route, no plan, no question.
    NotWork(CompileOutcome),
    /// The authoring budget (time) ran out before a trusted candidate;
    /// not a verdict on the request.
    BudgetExhausted(CompileOutcome),
    /// The authorized authoring call failed at the provider; nothing was
    /// substituted.
    ProviderFailed(CompileOutcome),
    /// The compiler refused the request under its own policy.
    Refused(CompileOutcome),
}

impl Reading {
    /// Classify an outcome by its typed fields.
    #[must_use]
    pub fn of(out: CompileOutcome) -> Self {
        if out.status == CompileStatus::Refused {
            return Self::Refused(out);
        }
        if out.status == CompileStatus::Ready && out.candidate.is_some() {
            return Self::Ready(out);
        }
        // `intent.clarification` is the compiler asking for a whole new
        // request: not a hole a line fills but a reading a seat may settle —
        // or the human rephrases. Every other mandatory key is a hole.
        if out
            .questions
            .iter()
            .any(|q| q.mandatory && q.key != CLARIFICATION_KEY)
        {
            return Self::Questions(out);
        }
        let provider_findings: Vec<&str> = out
            .diagnostics
            .iter()
            .filter(|d| d.target == "authoring_provider")
            .map(|d| d.message.as_str())
            .collect();
        if provider_findings.iter().any(|m| m.contains("timed out")) {
            return Self::BudgetExhausted(out);
        }
        if !provider_findings.is_empty() {
            return Self::ProviderFailed(out);
        }
        let routed = out
            .provenance
            .decision
            .as_ref()
            .and_then(|d| d.get("route"))
            .is_some();
        if routed || out.provenance.plan.is_some() {
            return Self::Unsettled(out);
        }
        Self::NotWork(out)
    }

    /// The outcome behind the reading.
    #[must_use]
    pub fn outcome(&self) -> &CompileOutcome {
        match self {
            Self::Ready(o)
            | Self::Questions(o)
            | Self::Unsettled(o)
            | Self::NotWork(o)
            | Self::BudgetExhausted(o)
            | Self::ProviderFailed(o)
            | Self::Refused(o) => o,
        }
    }
}

/// The compiler's own reasons in an outcome (unknown · missed · refused),
/// for the human — never parsed back into state.
#[must_use]
pub fn reasons(out: &CompileOutcome) -> Vec<String> {
    out.diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.kind,
                DiagnosticKind::Unknown | DiagnosticKind::Missed | DiagnosticKind::Refused
            )
        })
        .map(|d| d.message.clone())
        .collect()
}

/// Compile one request deterministically: exact skeletons, the support
/// grammar, strictly explicit intents, and the replay of a recorded plan.
/// Zero provider calls, always.
///
/// # Errors
/// The compiler's machinery failures only; missing values are outcomes.
pub fn compile_deterministic(request: &CompileRequest) -> Result<CompileOutcome, AuthoringError> {
    Ok(compile(request)?)
}

/// Compile one request through the seat under the default context (the
/// escalate strategy, no knowledge): [`compile_in`] with nothing configured.
/// A deterministic seat never contacts a model.
///
/// # Errors
/// The compiler's machinery, an unresolvable seat, or the session's runtime.
pub fn compile_through(
    seat: &AuthoringSeat,
    request: &CompileRequest,
) -> Result<CompileOutcome, AuthoringError> {
    compile_attached(
        seat,
        &AuthoringContext::default(),
        request,
        Attach::Compose(""),
        None,
    )
}

/// Compile one request through the seat under the session's authoring
/// context: a deterministic seat compiles deterministically (zero calls; the
/// context and its knowledge are never read); a provider seat carries the
/// context's strategy on its one policy (the deterministic ladder first, the
/// compiler's own order) and, when a snapshot is pinned, the pack composed for
/// `intent` — the request as the compiler reads it (a revision's request with
/// its change) — verified against the pin first. The outcome carries the
/// session's record of what it attached (`decision.session.authoring`).
///
/// # Errors
/// The compiler's machinery, an unresolvable seat, the session's runtime, or
/// a context that cannot be honored ([`AuthoringError::Context`]: nothing
/// was sent).
pub fn compile_in(
    seat: &AuthoringSeat,
    context: &AuthoringContext,
    request: &CompileRequest,
    intent: &str,
) -> Result<CompileOutcome, AuthoringError> {
    compile_attached(seat, context, request, Attach::Compose(intent), None)
}

/// Compile under a shared catalog account; it never grants Save or Run.
/// # Errors
/// As `compile_in`, plus local admission refusal.
pub fn compile_in_with_admission(
    seat: &AuthoringSeat,
    context: &AuthoringContext,
    request: &CompileRequest,
    intent: &str,
    account: &nika_providers::InferenceAdmission,
) -> Result<CompileOutcome, AuthoringError> {
    compile_attached(
        seat,
        context,
        request,
        Attach::Compose(intent),
        Some(account),
    )
}

/// What knowledge one seated compile attaches: a pack composed for an
/// intent, or the record carried from the round that authored a replayed
/// candidate (a replay presents nothing).
#[derive(Clone, Copy)]
enum Attach<'a> {
    Compose(&'a str),
    Carried(Option<&'a Value>),
}

fn compile_attached(
    seat: &AuthoringSeat,
    context: &AuthoringContext,
    request: &CompileRequest,
    attach: Attach<'_>,
    admission: Option<&nika_providers::InferenceAdmission>,
) -> Result<CompileOutcome, AuthoringError> {
    let model = match seat {
        AuthoringSeat::Deterministic { .. } => return compile_deterministic(request),
        AuthoringSeat::Unavailable { why } => return Err(AuthoringError::Seat(why.clone())),
        AuthoringSeat::Provider { model } => model.clone(),
        AuthoringSeat::Harness { seat, model } => {
            if admission.is_some() {
                return Err(AuthoringError::Seat(
                    "a subscription is not a billed-provider admission account".into(),
                ));
            }
            model.clone().unwrap_or_else(|| format!("{seat}/default"))
        }
    };
    if let Some(why) = context.refusal() {
        return Err(AuthoringError::Context(why.clone()));
    }
    let mut request = request.clone().with_authoring_policy(
        AuthoringPolicy::new(
            &model,
            AUTHORING_MAX_TOKENS,
            if matches!(seat, AuthoringSeat::Harness { .. }) {
                Duration::from_secs(300)
            } else {
                AUTHORING_TIMEOUT
            },
        )
        .with_native(context.strategy()),
    );
    let pack = match attach {
        Attach::Compose(intent) => context.compose(intent)?,
        Attach::Carried(_) => None,
    };
    if let Some(pack) = &pack {
        request = request.with_authoring_knowledge(pack.clone());
    }
    let mut out = match seat {
        AuthoringSeat::Harness { seat, model } => {
            harness::compile(seat, model.as_deref(), &request)?
        }
        _ => seated(&model, &request, admission, context.decision())?,
    };
    let knowledge = match (attach, &pack, context.knowledge()) {
        (Attach::Compose(_), Some(pack), Some(pin)) => Some(composed_record(pin, pack, &out)),
        (Attach::Carried(record), _, _) => record.map(carried_record),
        _ => None,
    };
    stamp(&mut out, context, knowledge.as_ref());
    Ok(out)
}

/// The session's record of the pack it attached to one call: the pinned
/// identity, the builder, the pack's digest, every reference (kind · id ·
/// bytes · sha256), whether the compiler's native door presented it to the
/// seat (its own record names the same pack digest) and, when it did, the
/// instruction digest of every call that carried it.
fn composed_record(pin: &KnowledgePin, pack: &AuthoringKnowledge, out: &CompileOutcome) -> Value {
    let digest = pack
        .identity
        .pointer("/door/pack_sha256")
        .and_then(Value::as_str)
        .map_or_else(|| pack_sha256(pack), str::to_owned);
    let presented = out
        .provenance
        .decision
        .as_ref()
        .and_then(|d| d.pointer("/native/knowledge/identity/door/pack_sha256"))
        .and_then(Value::as_str)
        == Some(digest.as_str());
    let calls: Vec<Value> = out
        .provenance
        .authoring
        .as_ref()
        .filter(|_| presented)
        .map(|receipt| {
            receipt
                .context
                .iter()
                .filter(|call| {
                    call.get("call")
                        .and_then(Value::as_str)
                        .is_some_and(reads_knowledge)
                })
                .map(|call| json!({"call": call["call"], "instruction_sha256": call["instruction_sha256"]}))
                .collect()
        })
        .unwrap_or_default();
    let why = (!presented).then(|| match out.provenance.strategy {
        Some(strategy) if strategy != Strategy::Native => format!(
            "the request settled on the {} path; only the native door reads knowledge",
            strategy.word()
        ),
        _ => "the native door did not present the pack to the seat".to_owned(),
    });
    // What authored with the pack — the round's receipt in brief — kept with the record, so a
    // candidate an answer round replays (zero calls) still names its model, host and usage.
    let seat = out
        .provenance
        .authoring
        .as_ref()
        .filter(|_| presented)
        .map(|receipt| {
            json!({
                "model": receipt.model,
                "calls": receipt.calls,
                "input_tokens": receipt.input_tokens,
                "output_tokens": receipt.output_tokens,
                "elapsed_ms": receipt.elapsed_ms,
                "backend": receipt.backend,
            })
        });
    json!({
        "identity": pin.record(),
        "pack_builder": PACK_BUILDER,
        "pack_sha256": digest,
        "references": pack.references.iter().map(|r| json!({
            "kind": r.kind,
            "id": r.id,
            "bytes": r.text.len(),
            "sha256": sha256_hex(r.text.as_bytes()),
        })).collect::<Vec<_>>(),
        "repairs": pack.repairs.len(),
        "presented": presented,
        "why": why,
        "calls": calls,
        "seat": seat,
        "carried": false,
    })
}

/// The calls of the native door — the only ones whose instruction carries the
/// pack: the native candidate, the sketch and its fills, and their repairs.
fn reads_knowledge(call: &str) -> bool {
    ["native", "sketch", "fill"]
        .iter()
        .any(|door| call == *door || call.starts_with(&format!("{door}-")))
}

/// The record of the round that authored a replayed candidate, carried: this
/// call presented nothing and called nobody.
fn carried_record(record: &Value) -> Value {
    let mut carried = record.clone();
    if let Some(map) = carried.as_object_mut() {
        map.insert("carried".to_owned(), Value::Bool(true));
    }
    carried
}

/// The session's knowledge record in an outcome when the native door
/// presented the pack (what an answer round carries).
fn presented_knowledge(out: &CompileOutcome) -> Option<Value> {
    let record = out
        .provenance
        .decision
        .as_ref()?
        .pointer("/session/authoring/knowledge")?;
    (record.get("presented") == Some(&Value::Bool(true))).then(|| record.clone())
}

/// Stamp the session's record beside the compiler's (`decision.session`), as a
/// host transport stamps its backend into the receipt: the strategy and its
/// source, the knowledge attached (or none).
fn stamp(out: &mut CompileOutcome, context: &AuthoringContext, knowledge: Option<&Value>) {
    let mut record = json!({"authoring": {
        "strategy": context.strategy().word(),
        "source": context.source(),
        "knowledge": knowledge,
    }});
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    // The decision seat's receipt, stamped by the seated call, stays beside it.
    if let Some(seat) = decision.pointer("/session/decision_seat").cloned() {
        record["decision_seat"] = seat;
    }
    if let Some(map) = decision.as_object_mut() {
        map.insert("session".to_owned(), record);
    }
    out.provenance.decision = Some(decision);
}

/// One request on the provider seat the human chose: the provider plane's
/// client (SSRF off · the transport ceiling), the registry over the ONE env
/// boundary, the compiler's cognition with that provider.
fn seated(
    model: &str,
    request: &CompileRequest,
    admission: Option<&nika_providers::InferenceAdmission>,
    selected: Option<&DecisionSetup>,
) -> Result<CompileOutcome, AuthoringError> {
    // The operator-selected decision seat for this ONE compile: consulted by the compiler only
    // for a finite ambiguity (WARM), charged only on the no-budget observation; a need met under
    // a numeric allowance is refused and recorded, never claimed as used.
    let consulted = selected.map(|setup| setup.consult(decision::admit(admission)));
    // The provider plane's client (SSRF off · the transport ceiling), as
    // the conversation's reasoner and the engine's run path use.
    let http =
        crate::reasoner::provider_http_for(admission.is_some()).map_err(AuthoringError::Seat)?;
    let mut registry =
        nika_providers::ProviderRegistry::new(Arc::new(http), crate::reasoner::provider_config());
    if let Some(a) = admission {
        registry = registry.with_inference_admission(a.clone());
    }
    let provider = registry
        .resolve(model)
        .map_err(|e| AuthoringError::Seat(e.to_string()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| AuthoringError::Runtime(e.to_string()))?;
    // The compile future carries a whole `CompileOutcome`: boxed so this
    // frame stays small (clippy::large_futures), as the CLI host does.
    let mut out = runtime.block_on(Box::pin(compile_with_cognition(
        request,
        Cognition {
            provider: Some(&provider),
            seat: consulted
                .as_ref()
                .map(|seat| seat as &dyn nika_onboard::compile::decide::DecisionSeat),
        },
    )))?;
    // What the seat was asked, sent, answered or refused (`decision.session.decision_seat`),
    // beside the compiler's own record of the same questions.
    if let Some(receipt) = consulted.as_ref().and_then(decision::SessionSeat::receipt) {
        let record = out.provenance.decision.get_or_insert_with(|| json!({}));
        if let Some(record) = record.as_object_mut()
            && let Some(session) = record
                .entry("session")
                .or_insert_with(|| json!({}))
                .as_object_mut()
        {
            session.insert("decision_seat".to_owned(), receipt);
        }
    }
    // The receipt names its backend as the CLI's does, with the host the
    // calls really went to (an overridden base URL is a gateway: said).
    if let Some(receipt) = out.provenance.authoring.as_mut() {
        let id = model.split('/').next().unwrap_or(model);
        let host = registry.effective_base_url(id).map(host_of);
        let seed = registry
            .profiles()
            .iter()
            .find(|p| p.id == nika_providers::canonical_provider(id))
            .map(|p| host_of(p.base_url));
        receipt.backend.get_or_insert_with(|| {
            json!({
                "kind": "direct_api",
                "provider": id,
                "host": host,
                "base_url_overridden": host.is_some() && host != seed,
                "cost_basis": "measured_by_tokens_at_catalog_price",
            })
        });
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The stronger model is the provider's strongest, never the same one twice.
    #[test]
    fn a_stronger_model_is_the_providers_strongest() {
        assert_eq!(stronger_model("openai/gpt-5-mini"), Some("openai/gpt-5.2"));
        assert_eq!(
            stronger_model("openai/gpt-5.2"),
            None,
            "already the strongest"
        );
        assert_eq!(stronger_model("xai/grok-4.3"), Some("xai/grok-4.7"));
        assert_eq!(
            stronger_model_under("gemini/gemini-2.5-flash", false),
            Some("gemini/gemini-2.5-pro")
        );
        assert_eq!(stronger_model_under("gemini/gemini-2.5-pro", false), None);
        assert_eq!(host_of("https://api.scaleway.ai/v1"), "api.scaleway.ai");
        assert_eq!(host_of("http://127.0.0.1:11434/v1/chat"), "127.0.0.1:11434");
        // Through an OpenAI-compatible gateway the provider's flagship is not served: no escalation.
        assert_eq!(stronger_model_under("openai/gpt-oss-120b", true), None);
        assert_eq!(
            stronger_model_under("openai/gpt-5-mini", false),
            Some("openai/gpt-5.2")
        );
        assert_eq!(
            stronger_model("ollama/qwen3.5:4b"),
            None,
            "unknown provider"
        );
        assert_eq!(stronger_model("nonsense"), None);
    }
    use crate::intelligence::DataLocus;
    use crate::reasoner::{NoReasoner, ProviderReasoner};

    fn resolved(kind: IntelligenceKind) -> ResolvedSessionIntelligence {
        ResolvedSessionIntelligence {
            kind,
            model: None,
            locus: DataLocus::None,
            ready: true,
            why: None,
        }
    }

    fn read(intent: &str) -> Reading {
        Reading::of(compile_deterministic(&CompileRequest::create(intent)).expect("compiles"))
    }

    #[test]
    fn the_seat_follows_the_reasoner_the_human_chose() {
        let api = ProviderReasoner {
            model: "openai/gpt-4.1-mini".to_owned(),
            label: "openai API".to_owned(),
        };
        assert_eq!(
            AuthoringSeat::from_reasoner(
                &api,
                &resolved(IntelligenceKind::Api {
                    provider: "openai".to_owned()
                })
            ),
            AuthoringSeat::Provider {
                model: "openai/gpt-4.1-mini".to_owned()
            }
        );
        let none = AuthoringSeat::from_reasoner(&NoReasoner, &resolved(IntelligenceKind::None));
        assert!(matches!(
            none,
            AuthoringSeat::Deterministic { why: Some(_) }
        ));
        let harness = AuthoringSeat::from_reasoner(
            &NoReasoner,
            &resolved(IntelligenceKind::Harness {
                seat: "codex".to_owned(),
            }),
        );
        match harness {
            AuthoringSeat::Unavailable { why } => {
                assert!(why.contains("codex"), "{why}");
            }
            other => panic!("an unavailable harness refuses: {other:?}"),
        }
    }

    #[test]
    fn the_reading_is_the_compilers_typed_fields() {
        assert!(matches!(
            read("hello there, how are you today?"),
            Reading::NotWork(_)
        ));
        assert!(matches!(
            read("Read ./notes/brief.md and write it to ./out/copy.md"),
            Reading::Ready(_)
        ));
        match read(
            "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md",
        ) {
            Reading::Questions(out) => {
                assert_eq!(out.questions[0].key, "model");
                assert!(
                    out.provenance.plan.is_some(),
                    "the HOT plan is recorded for replay"
                );
            }
            other => panic!("a draft needs its model, asked: {other:?}"),
        }
        assert!(matches!(
            read("Read ./a.md and do something clever with it, then write ./b.md"),
            Reading::Unsettled(_)
        ));
        match read("chain") {
            Reading::Questions(out) => assert_eq!(out.questions[0].key, "tasks.think.infer.prompt"),
            other => panic!("a skeleton with holes asks: {other:?}"),
        }
    }

    #[test]
    fn an_answer_round_replays_the_same_plan_with_zero_calls() {
        let intent = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";
        let mut round = AuthoringRound::new(intent);
        let first = compile_deterministic(&round.request()).expect("first round");
        round.absorb(&first);
        assert_eq!(round.current().map(|q| q.key.as_str()), Some("model"));
        assert!(round.continuation.is_some());
        assert_eq!(round.answer_current("mock/echo"), Some("model".to_owned()));
        assert_eq!(round.answers["model"], "\"mock/echo\"");
        assert!(round.current().is_none());
        let second = compile_deterministic(&round.request()).expect("answer round");
        assert_eq!(second.status, CompileStatus::Ready);
        let route = second.provenance.decision.as_ref().expect("decision")["route"].clone();
        assert_eq!(route, serde_json::json!(["replayed plan"]));
        assert!(
            second.provenance.authoring.is_none(),
            "no provider was contacted for an answer round"
        );
        let candidate = second.candidate.expect("candidate");
        assert!(candidate.contains("model: mock/echo"), "{candidate}");
        assert!(candidate.contains("nika:write"), "{candidate}");
    }

    #[test]
    fn a_plan_that_still_needs_cognition_is_never_replayed() {
        let mut round =
            AuthoringRound::new("Read ./a.md and do something clever with it, then write ./b.md");
        let out = compile_deterministic(&round.request()).expect("compiles");
        assert!(
            out.provenance.plan.is_some(),
            "the reader recorded what it read"
        );
        round.absorb(&out);
        assert!(
            round.continuation.is_none(),
            "unsettled work is read again, never replayed"
        );
    }

    /// The compiler's own questions of each shape: a skeleton's prompt
    /// hole is text, a skeleton's constant hole is a literal.
    fn question_of(skeleton: &str, shape: QuestionType) -> CompileQuestion {
        let out = compile_deterministic(&CompileRequest::create(skeleton)).expect("compiles");
        let question = out.questions.into_iter().next().expect("one hole");
        assert_eq!(question.answer_type, shape, "{skeleton}");
        question
    }

    #[test]
    fn a_line_is_typed_to_the_questions_shape() {
        let text = question_of("chain", QuestionType::Text);
        let literal = question_of("bounded-batch", QuestionType::Literal);
        assert_eq!(literal_for(&text, "mock/echo"), "\"mock/echo\"");
        assert_eq!(literal_for(&text, " 5 "), "\"5\"");
        assert_eq!(literal_for(&literal, "5"), "5");
        assert_eq!(literal_for(&literal, "true"), "true");
        assert_eq!(literal_for(&literal, "./notes"), "\"./notes\"");
        assert_eq!(literal_for(&literal, "[\"a\"]"), "[\"a\"]");
    }

    #[test]
    fn cancel_words_and_greetings_are_closed_protocol_sets() {
        assert!(is_cancel("cancel"));
        assert!(is_cancel(" Annule "));
        assert!(!is_cancel("no"), "a no answers a question");
        assert!(!is_cancel("./notes"));
        assert!(is_greeting("hello"), "the bare word");
        assert!(is_greeting(" Hello! "), "case and punctuation aside");
        assert!(is_greeting("merci."));
        assert!(
            !is_greeting("hello there, how are you today?"),
            "a sentence is not a bare greeting"
        );
        assert!(!is_greeting("hello.nika"), "a file is not a greeting");
    }

    /// A round keeps the knowledge record of the call that authored its
    /// continuation only when the native door presented the pack; the
    /// record it carries says it was carried.
    #[test]
    fn a_round_keeps_the_knowledge_its_candidate_was_authored_from() {
        let knowledge = |presented: bool| {
            let mut out =
                compile_deterministic(&CompileRequest::create("chain")).expect("compiles");
            out.provenance.strategy = Some(Strategy::Native);
            out.provenance.plan = Some(json!({"strategy": "native"}));
            out.provenance.decision = Some(json!({"session": {"authoring": {"knowledge": {
                "presented": presented,
                "pack_sha256": "abc",
            }}}}));
            let mut round = AuthoringRound::new("x");
            round.absorb(&out);
            assert!(round.continuation.is_some());
            round.knowledge
        };
        let kept = knowledge(true).expect("presented: kept");
        assert_eq!(kept["pack_sha256"], "abc");
        assert_eq!(
            knowledge(false),
            None,
            "attached but never read: nothing to carry"
        );
        assert_eq!(carried_record(&kept)["carried"], true);
        assert_eq!(carried_record(&kept)["pack_sha256"], "abc");
        // A clarification answered replaces the request the pack is composed for.
        let mut round = AuthoringRound::new("the first words");
        assert_eq!(round.effective_intent(), "the first words");
        round.answers.insert(
            CLARIFICATION_KEY.to_owned(),
            "\"the whole request\"".to_owned(),
        );
        assert_eq!(round.effective_intent(), "the whole request");
    }

    #[test]
    fn only_the_native_doors_calls_read_knowledge() {
        for call in [
            "native",
            "native-repair",
            "sketch",
            "sketch-repair",
            "fill",
            "fill-repair",
        ] {
            assert!(reads_knowledge(call), "{call}");
        }
        for call in ["plan", "repair", "transform", "natives"] {
            assert!(!reads_knowledge(call), "{call}");
        }
    }

    #[test]
    fn a_deterministic_seat_never_contacts_a_model() {
        let seat = AuthoringSeat::Deterministic { why: None };
        let out = compile_through(
            &seat,
            &CompileRequest::create("build me a digest of the docs"),
        )
        .expect("compiles");
        assert!(out.provenance.authoring.is_none());
        assert!(matches!(Reading::of(out), Reading::NotWork(_)));
    }
}
