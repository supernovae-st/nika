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
#[cfg(test)]
#[path = "authoring/money_restatement_tests.rs"]
mod money_restatement_tests;
use std::sync::Arc;
use std::time::Duration;

use nika_onboard::compile::{
    AuthoringPolicy, AuthoringReceipt, Cognition, CompileError, CompileOutcome, CompileQuestion,
    CompileRequest, NativeMode, compile, compile_with_cognition, revise_intent, round,
};
// The records a compile outcome carries live beside the snapshot door (C7 · D1).
use nika_onboard::knowledge::pin::{
    carried_record, composed_record, observed_in, stamp, stamp_seat,
};
use serde_json::Value;

use crate::intelligence::{IntelligenceKind, ResolvedSessionIntelligence};
use crate::reasoner::SessionReasoner;

mod context;
pub(crate) mod decision;
mod harness;
pub use context::{AuthoringContext, AuthoringContextError};
pub use decision::{DECISION_ENV, DECISION_SCHEMA, DecisionSetup, MAX_DECISION_CALLS};
// What an outcome means and the literal a line is live beside the compile unit (C7).
use nika_onboard::compile::reading::{CLARIFICATION_KEY, clarified};
pub use nika_onboard::compile::reading::{Reading, literal_for, reasons};
pub use nika_onboard::knowledge::pin::KnowledgePin;
// The static flagship table lives beside the seat facts it sits with (C11); the gateway fact it
// takes is read here, through the engine's registry.
pub use nika_onboard::compile::seat::{stronger_model, stronger_model_under};

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
    nika_cli_host::compile::authoring_host(url).unwrap_or_else(|| "unknown endpoint".to_owned())
}
/// The hard output ceiling of one Session authoring call (the compiler's own maximum: a
/// reasoning seat spends part of it on its reasoning, and a complete candidate needs the rest).
pub const AUTHORING_MAX_TOKENS: u32 = 32_768;
/// The first native generation's output limit: a REPORTED truncation spends one of the native
/// repairs to widen it, up to [`AUTHORING_MAX_TOKENS`] — never a transport retry.
pub const AUTHORING_INITIAL_TOKENS: u32 = 16_384;
/// Wall time one Session authoring call may take.
pub const AUTHORING_TIMEOUT: Duration = Duration::from_secs(180);
/// The native repair rounds one Session authoring may buy (one call each).
pub const AUTHORING_REPAIRS: u32 = 3;
/// The most provider calls one seated Session compile may make under the escalate strategy:
/// the COLD plan and its one evidence repair (2), then the native candidate (1) and its repairs.
/// The turn's classification is one more call outside the compile; a fresh unknown-cost review
/// admits exactly those (`SESSION_REVIEW_MAX_REQUESTS`).
pub const AUTHORING_CALLS_PER_COMPILE: u32 = 2 + 1 + AUTHORING_REPAIRS;

/// The one policy a Session seat authors under: the hard ceiling, the first native limit, the
/// deadline (a subscription harness keeps its own longer one), the native repairs and the
/// session's strategy.
#[must_use]
pub fn session_policy(model: &str, harness: bool, strategy: NativeMode) -> AuthoringPolicy {
    AuthoringPolicy::new(
        model,
        AUTHORING_MAX_TOKENS,
        if harness {
            Duration::from_secs(300)
        } else {
            AUTHORING_TIMEOUT
        },
    )
    .with_initial_max_tokens(AUTHORING_INITIAL_TOKENS)
    .with_repairs(AUTHORING_REPAIRS)
    .with_native(strategy)
}

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
    /// A revision's EDIT — the exact base, the human's change, the request the base answered:
    /// every request of the round is that EDIT, never a fresh CREATE.
    pub(crate) edit: Option<(String, String, Option<String>)>,
    /// The monetary directives of `intent` the money gate admitted (R4 A6): every request of the
    /// round tells the compiler they are the Session's ceiling, never business clauses.
    pub(crate) money: Vec<std::ops::Range<usize>>,
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
            edit: None,
            money: Vec::new(),
        }
    }

    /// The intent the compiler reads for this round: a revision's original
    /// request and change, folded; an answered `intent.clarification` replaces
    /// the request (the compiler's own law), else the request as stated.
    #[must_use]
    pub fn effective_intent(&self) -> String {
        let read = if self.edit.is_some() {
            revise_intent(&self.request())
        } else {
            clarified(&self.answers)
        };
        read.unwrap_or_else(|| self.intent.clone())
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
            Attach::Carried(self.knowledge.as_ref(), &intent)
        } else {
            Attach::Compose(&intent)
        };
        let mut out = compile_attached(seat, context, &self.request(), attach, None)?;
        round::carry_receipt(
            &mut out,
            self.continuation
                .as_ref()
                .and(self.authoring_receipt.as_ref()),
        );
        Ok(out)
    }

    /// The typed request this round is ([`round::request`]): the intent (a revision's EDIT),
    /// every answer, the recorded plan when one settled, the admitted monetary directives.
    #[must_use]
    pub fn request(&self) -> CompileRequest {
        round::request(
            &self.intent,
            self.edit.as_ref(),
            &self.answers,
            self.continuation.as_ref(),
            &self.money,
        )
    }

    /// Keep what the outcome settled: the plan once a strategy settled it
    /// (a plan that still carries unknown work is never replayed) with the
    /// knowledge record of the call that authored it when the native door
    /// presented a pack, the mandatory questions in the compiler's order (a
    /// revision's clause dispositions too), its reasons.
    pub fn absorb(&mut self, out: &CompileOutcome) {
        if let Some(plan) = round::reanchored(self.continuation.as_ref(), out) {
            // The compiler re-anchored the plan to a changed source (R4 A6): the next answer
            // binds against the observation its question showed; no approval rides along.
            self.continuation = Some(plan);
        }
        if self.continuation.is_none()
            && let Some(settled) = round::settled(out)
        {
            self.continuation = Some(settled.plan);
            self.knowledge = settled.knowledge;
            self.authoring_receipt = settled.receipt;
        }
        (self.questions, self.reasons) = round::open(out, self.edit.is_some());
        // An answer the compiler asks for again (stale, refused) is no answer (R4 A6).
        let asked = &self.questions;
        self.answers
            .retain(|key, _| !asked.iter().any(|q| &q.key == key));
    }

    /// The question the next line answers, when one is open.
    #[must_use]
    pub fn current(&self) -> Option<&CompileQuestion> {
        self.questions.first()
    }

    /// Whether `reading` leaves a question this round asks (a value, a revision's clause
    /// disposition): only a reading that asks or is left unsettled, never a refusal.
    pub(crate) fn asks(&self, reading: &Reading) -> bool {
        let mut probe = self.clone();
        probe.absorb(reading.outcome());
        matches!(reading, Reading::Questions(_) | Reading::Unsettled(_))
            && probe.current().is_some()
    }

    /// Read one clause again while retaining only unchanged, previously admitted monetary
    /// text. Offsets follow the replacement's byte length; new or overlapping text gains no
    /// admission. Answers and the old plan belong to the old request and are not carried.
    pub(crate) fn restate_clause(&self, clause: &str, answer: &str) -> Self {
        let mut next = Self::new(self.intent.replacen(clause, answer, 1));
        next.restatements = self.restatements.saturating_add(1);
        if let Some(start) = self.intent.find(clause) {
            let end = start + clause.len();
            next.money = self
                .money
                .iter()
                .filter_map(|span| {
                    if span.end <= start {
                        Some(span.clone())
                    } else if span.start >= end {
                        let after = start + answer.len();
                        Some(after + (span.start - end)..after + (span.end - end))
                    } else {
                        None
                    }
                })
                .collect();
        }
        next
    }

    /// Answer the current question with the human's line, typed to the question's shape; the key it
    /// answered. An explicit Create clarification makes its answer the request (C11): what the
    /// earlier intent was answered and planned with is dropped, the chosen seat stays, and its
    /// lexical money spans stay only on identical bytes (the caller installs the spans admitted
    /// for new ones). A revision keeps its change.
    pub fn answer_current(&mut self, line: &str) -> Option<String> {
        let question = self.questions.first()?.clone();
        let literal = literal_for(&question, line);
        if question.key == CLARIFICATION_KEY && self.edit.is_none() {
            self.answers.retain(|key, _| key == "model");
            (self.continuation, self.knowledge, self.authoring_receipt) = (None, None, None);
            if let Some(text) = self.replacement(line) {
                if text != self.intent {
                    self.money.clear();
                }
                self.intent = text;
                self.questions.remove(0);
                return Some(question.key);
            }
        }
        self.answers.insert(question.key.clone(), literal);
        self.questions.remove(0);
        Some(question.key)
    }

    /// The Create text an answer at an explicit Create clarification makes the request, as the
    /// compiler reads that answer; `None` at any other question, in a revision, or when blank.
    pub(crate) fn replacement(&self, line: &str) -> Option<String> {
        let question = (self.questions.first()).filter(|q| q.key == CLARIFICATION_KEY)?;
        let literal = literal_for(question, line);
        (self.edit.is_none())
            .then(|| clarified(&BTreeMap::from([(CLARIFICATION_KEY.to_owned(), literal)])))
            .flatten()
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
            Attach::Carried(self.knowledge.as_ref(), &intent)
        } else {
            Attach::Compose(&intent)
        };
        compile_attached(seat, context, &self.request(), attach, Some(account))
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

/// A closed line's words whatever the typography: spaces as one (a no-break
/// space, or the narrow one French sets before `?`), the closing marks set
/// aside, a typographic apostrophe as `'`, lower case. It adds no word.
fn closed_words(line: &str, closing: &[char]) -> String {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(|c: char| c == ' ' || closing.contains(&c))
        .replace(['\u{2018}', '\u{2019}'], "'")
        .to_lowercase()
}

/// The few words that ask WHY beside what waits (a question, a gate) —
/// answered from the machine's state, consuming nothing. A closed set of
/// whole lines, punctuation aside.
#[must_use]
pub fn is_why(line: &str) -> bool {
    let word = closed_words(line, &['?', '!', '.']);
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
    let word = closed_words(line, &['?', '!', '.']);
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
    let word = closed_words(line, &['?', '!', '.']);
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
    let word = closed_words(input, &['!', '.', '?', ',']);
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
/// context: a deterministic seat compiles deterministically (zero calls; its
/// knowledge is never read, the project it roots is observed); a provider seat carries the
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
/// candidate (a replay presents nothing) — with the intent the compiler reads,
/// whose named files the project observation describes either way.
#[derive(Clone, Copy)]
enum Attach<'a> {
    Compose(&'a str),
    Carried(Option<&'a Value>, &'a str),
}

fn compile_attached(
    seat: &AuthoringSeat,
    context: &AuthoringContext,
    request: &CompileRequest,
    attach: Attach<'_>,
    admission: Option<&nika_providers::InferenceAdmission>,
) -> Result<CompileOutcome, AuthoringError> {
    // The project as it is NOW, for the intent the compiler reads (a fresh request, its
    // answers' round, a revision's request with its change), on every seat (R4 S1): the files
    // it names, observed by the shared bounded observer under the root — never outside it.
    let contextual = match attach {
        Attach::Compose(intent) | Attach::Carried(_, intent) => intent,
    };
    let observed = context
        .project_root()
        .filter(|_| !contextual.trim().is_empty())
        .and_then(|root| nika_cli_host::compile::observe::world(root, contextual));
    let mut request = request.clone();
    if let Some(world) = &observed {
        request = request.with_knowledge(world.clone());
    }
    let model = match seat {
        AuthoringSeat::Deterministic { .. } => {
            return Ok(observed_in(compile(&request)?, observed.as_ref()));
        }
        AuthoringSeat::Unavailable { why } => return Err(AuthoringError::Seat(why.clone())),
        AuthoringSeat::Provider { model } => model.clone(),
        AuthoringSeat::Harness { seat, model } => {
            if admission.is_some() {
                return Err(AuthoringError::Seat(
                    "a subscription is not a billed-provider admission account".into(),
                ));
            }
            // Its adapter cannot carry an explicit effort: said, never silently dropped (R4 B16).
            if let Some(level) = context.reasoning() {
                return Err(AuthoringError::Seat(format!(
                    "the subscription seat `{seat}` cannot carry the explicit reasoning effort `{}` · nothing was sent",
                    level.word()
                )));
            }
            model.clone().unwrap_or_else(|| format!("{seat}/default"))
        }
    };
    if let Some(why) = context.refusal() {
        return Err(AuthoringError::Context(why.clone()));
    }
    let harness = matches!(seat, AuthoringSeat::Harness { .. });
    let policy = session_policy(&model, harness, context.strategy());
    // Every authoring and repair call asks the named effort; the caps stay the policy's (R4 B16).
    request = request.with_authoring_policy(match context.reasoning() {
        Some(level) => policy.with_reasoning(level),
        None => policy,
    });
    let pack = match attach {
        Attach::Compose(intent) => context.compose(intent)?,
        Attach::Carried(..) => None,
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
        (Attach::Carried(record, _), _, _) => record.map(carried_record),
        _ => None,
    };
    stamp(
        &mut out,
        context.strategy().word(),
        context.source(),
        knowledge.as_ref(),
    );
    Ok(observed_in(out, observed.as_ref()))
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
    // What the seat was asked, sent, answered or refused, beside the compiler's own record of
    // the same questions. A separate backend: a named level is never sent to it nor claimed (B19).
    if let Some(mut receipt) = consulted.as_ref().and_then(decision::SessionSeat::receipt) {
        if let Some(level) = request
            .authoring
            .as_ref()
            .and_then(|policy| policy.reasoning)
        {
            receipt["reasoning_effort"] = Value::from(format!(
                "not applicable · the named level `{}` rides the LLM calls only; the TypeSafe request carries no effort",
                level.word()
            ));
        }
        stamp_seat(&mut out, receipt);
    }
    // The receipt names its backend as the CLI's does, with the host the
    // calls really went to (an overridden base URL is a gateway: said).
    if let Some(receipt) = out.provenance.authoring.as_mut() {
        receipt
            .backend
            .get_or_insert_with(|| nika_cli_host::compile::authoring_backend(&registry, model));
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use nika_onboard::compile::{CompileStatus, Strategy};
    use serde_json::json;

    #[test]
    fn host_diagnostics_never_include_userinfo_or_query_values() {
        assert_eq!(
            host_of(
                "https://test-user:test-sentinel@gateway.invalid:8443/private?key=test-sentinel#frag"
            ),
            "gateway.invalid:8443"
        );
    }

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

    // The reading and a line's literal descended with their tests to
    // `nika_onboard::compile::reading` (C7); the session re-exports them.

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

    /// C10 · a replacement's proposal keeps its own round for the source basis: the replacing
    /// words and the answer given after them, never an earlier request's answer or plan, with
    /// the observation its compile round was given; the law folds the replacement and holds it.
    #[test]
    fn a_replacement_proposal_keeps_its_own_round_for_the_source_basis() {
        let orders = "read ./orders.csv, keep only the rows whose status is open and write them to ./out.json";
        let world =
            json!({"observed": [{"path": "./orders.csv", "state": "absent", "complete": false}]});
        let skeleton = compile_deterministic(&CompileRequest::create("bounded-batch"));
        let mut clarify = skeleton.expect("compiles").questions[0].clone();
        clarify.key = CLARIFICATION_KEY.to_owned();
        clarify.answer_type = nika_onboard::compile::QuestionType::Text;
        let mut round = AuthoringRound::new("make my project better");
        round
            .answers
            .insert("const.rule_field_2".to_owned(), "\"amount\"".to_owned());
        round.continuation = Some(json!({"strategy": "hot"}));
        round.questions = vec![clarify];
        round.answer_current(orders);
        // The question round of the replacing words, as the cognition door folds them.
        let folded = CompileRequest::create(orders).with_knowledge(world.clone());
        round.absorb(&compile_deterministic(&folded).expect("asks"));
        assert_eq!(
            round.answer_current("status").as_deref(),
            Some("const.rule_field_1")
        );
        let answered = CompileRequest::create(orders)
            .with_plan(round.continuation.clone().expect("the question plan"))
            .answer("const.rule_field_1", "\"status\"")
            .with_knowledge(world.clone());
        let out = compile_deterministic(&answered).expect("the answer round");
        assert_eq!(out.status, CompileStatus::Ready, "{out:?}");
        let out = observed_in(out, Some(&world));
        let kept = nika_onboard::compile::round::compiled(round.request(), &out).expect("rebuilt");
        // C11: the replacement is the request itself, never an answer riding the old text.
        assert_eq!(
            kept.answers.keys().collect::<Vec<_>>(),
            ["const.rule_field_1"],
            "the earlier answer is gone"
        );
        assert_eq!(kept.knowledge.as_ref(), Some(&world));
        let decision = out.provenance.decision.as_ref();
        assert_eq!(
            nika_onboard::compile::basis_for(&kept, decision, Some(&world)),
            nika_onboard::compile::Basis::Holds(1)
        );
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

    // `reads_knowledge` descended with the records it serves (C7 · D1): its test lives in
    // `nika_onboard::knowledge::pin`.

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

    /// C10 · a complete replacement request (`intent.clarification`) drops what the earlier
    /// intent was answered and planned with: an old column answer cannot ride into the new
    /// request, while the chosen seat stays and an answer given after the replacement is kept.
    /// C11: the replacement is the request's Create text, and the old lexical money spans do not
    /// ride its changed bytes (the account's ceiling is Session's, apart). An ordinary answer
    /// keeps the round, and a revision never replaces its change.
    #[test]
    fn a_replacement_request_starts_its_round_again_but_keeps_the_seat_and_the_money() {
        let asked = compile_deterministic(&CompileRequest::create("bounded-batch"))
            .expect("compiles")
            .questions;
        let ask = |key: &str| {
            let mut question = asked[0].clone();
            question.key = key.to_owned();
            question.answer_type = nika_onboard::compile::QuestionType::Text;
            question
        };
        let earlier = || {
            let mut round = AuthoringRound::new("keep the rows of ./data/input.csv over 250");
            round.money = std::slice::from_ref(&(0..4)).to_vec();
            round
                .answers
                .insert("const.rule_field_1".to_owned(), "\"amount\"".to_owned());
            round
                .answers
                .insert("model".to_owned(), "\"mock/echo\"".to_owned());
            round.continuation = Some(json!({"strategy": "hot"}));
            round.knowledge = Some(json!({"pack_sha256": "abc"}));
            round.authoring_receipt = Some(AuthoringReceipt::new("claude-code/default"));
            round
        };
        let mut round = earlier();
        round.questions = vec![ask(CLARIFICATION_KEY), ask("const.rule_field_1")];
        let replacement = "read ./other.csv and keep the rows whose price is over 3";
        assert_eq!(
            round.answer_current(replacement).as_deref(),
            Some(CLARIFICATION_KEY)
        );
        assert_eq!(
            round.answers.keys().collect::<Vec<_>>(),
            ["model"],
            "the old column answer is gone, the chosen seat stays"
        );
        assert!(round.continuation.is_none() && round.knowledge.is_none());
        assert!(round.authoring_receipt.is_none());
        assert!(round.money.is_empty(), "no old offsets ride the new bytes");
        assert_eq!(
            (round.intent.as_str(), round.effective_intent()),
            (replacement, replacement.to_owned())
        );
        assert!(
            round.request().plan.is_none(),
            "no old plan rides the request"
        );
        round.answer_current("price");
        assert_eq!(
            round.answers["const.rule_field_1"], "\"price\"",
            "a new answer is kept"
        );

        let mut ordinary = earlier();
        ordinary.questions = vec![ask("const.limit")];
        ordinary.answer_current("5");
        assert_eq!(
            ordinary.answers.len(),
            3,
            "an ordinary answer keeps the round"
        );
        assert!(ordinary.continuation.is_some() && ordinary.authoring_receipt.is_some());

        let mut revision = earlier();
        revision.edit = Some(("nika: base\n".to_owned(), "add a step".to_owned(), None));
        revision.questions = vec![ask(CLARIFICATION_KEY)];
        revision.answer_current("something else");
        assert!(
            revision.answers.contains_key("const.rule_field_1") && revision.continuation.is_some(),
            "a revision never replaces its change"
        );
    }
}
