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
//! explicit [`AuthoringPolicy`] on that same model; a harness seat or no
//! intelligence keeps authoring deterministic. Ambient keys are never
//! consent; nothing here selects a provider the human did not name.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use nika_onboard::compile::{
    AuthoringPolicy, Cognition, CompileError, CompileOutcome, CompileQuestion, CompileRequest,
    CompileStatus, DiagnosticKind, QuestionType, compile, compile_with_cognition,
};
use serde_json::Value;

use crate::intelligence::{IntelligenceKind, ResolvedSessionIntelligence};
use crate::reasoner::SessionReasoner;

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
    let Ok(http) = crate::reasoner::provider_http() else {
        return false;
    };
    let registry = nika_providers::ProviderRegistry::new(
        Arc::new(http),
        nika_runtime::compose::config_from_env(),
    );
    !registry
        .effective_base_url("openai")
        .is_some_and(|url| url.contains("api.openai.com"))
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
}

impl AuthoringSeat {
    /// The seat the human's choice permits: the reasoner that reasons with
    /// them names its model when it is an API or a local engine; a harness
    /// seat reasons in words only, and no intelligence keeps the facts.
    #[must_use]
    pub fn from_reasoner(
        reasoner: &dyn SessionReasoner,
        intelligence: &ResolvedSessionIntelligence,
    ) -> Self {
        match reasoner.authoring_model() {
            Some(model) => Self::Provider { model },
            None => Self::Deterministic {
                why: match &intelligence.kind {
                    IntelligenceKind::Harness { seat } => Some(format!(
                        "the {seat} seat reasons in words; free intents are read deterministically — name an API or a local model (`/intelligence`) to let one read them"
                    )),
                    IntelligenceKind::None => Some(
                        "no conversational intelligence: free intents are read deterministically — `/intelligence` to choose a model that reads them"
                            .to_owned(),
                    ),
                    _ => None,
                },
            },
        }
    }

    /// The banner's word for the seat.
    #[must_use]
    pub fn line(&self) -> String {
        match self {
            Self::Deterministic { .. } => {
                "authoring · deterministic (exact intents only)".to_owned()
            }
            Self::Provider { model } => format!("authoring · {model} (one call per fresh intent)"),
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
        }
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
    /// (a plan that still carries unknown work is never replayed), the
    /// mandatory questions in the compiler's order, its reasons.
    pub fn absorb(&mut self, out: &CompileOutcome) {
        if self.continuation.is_none() && out.provenance.strategy.is_some() {
            self.continuation.clone_from(&out.provenance.plan);
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
}

/// The human's line as the JSON literal the question's shape takes: a
/// `Text` question takes the line as one string; a `Literal` question
/// takes the line verbatim when it already is JSON (`5` · `true` ·
/// `["a"]`), else as a string (`./notes` is a path, not a parse error).
#[must_use]
pub fn literal_for(question: &CompileQuestion, line: &str) -> String {
    let line = line.trim();
    match question.answer_type {
        QuestionType::Literal if serde_json::from_str::<Value>(line).is_ok() => line.to_owned(),
        _ => Value::String(line.to_owned()).to_string(),
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

/// Compile one request through the seat: the deterministic ladder first
/// (the compiler's own order), then ONE explicitly permitted call on the
/// seat's model when the seat is a provider and the ladder needs it. A
/// deterministic seat never contacts a model.
///
/// # Errors
/// The compiler's machinery, an unresolvable seat, or the session's runtime.
pub fn compile_through(
    seat: &AuthoringSeat,
    request: &CompileRequest,
) -> Result<CompileOutcome, AuthoringError> {
    let AuthoringSeat::Provider { model } = seat else {
        return compile_deterministic(request);
    };
    let request = request.clone().with_authoring_policy(AuthoringPolicy::new(
        model,
        AUTHORING_MAX_TOKENS,
        AUTHORING_TIMEOUT,
    ));
    // The provider plane's client (SSRF off · the transport ceiling), as
    // the conversation's reasoner and the engine's run path use.
    let http = crate::reasoner::provider_http().map_err(AuthoringError::Seat)?;
    let registry = nika_providers::ProviderRegistry::new(
        Arc::new(http),
        nika_runtime::compose::config_from_env(),
    );
    let provider = registry
        .resolve(model)
        .map_err(|e| AuthoringError::Seat(e.to_string()))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| AuthoringError::Runtime(e.to_string()))?;
    Ok(runtime.block_on(compile_with_cognition(
        &request,
        Cognition {
            provider: Some(&provider),
            seat: None,
        },
    ))?)
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
            AuthoringSeat::Deterministic { why: Some(why) } => {
                assert!(why.contains("codex"), "{why}");
            }
            other => panic!("a harness seat authors deterministically: {other:?}"),
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
