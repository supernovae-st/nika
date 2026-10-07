// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Generation 2 of the compile request, spoken only by a server that seats native authoring.
//!
//! Every generation-1 law holds (unknown fields, a present `null`, duplicate keys and
//! positional arrays refused; the same configured HTTP body ceiling). Added: explicit cognition,
//! the request a revised base answered, the caller's narrowing of the operator's bounds, and the
//! token of a round this server kept. New to this generation: a literal that repeats an object
//! key at any depth is refused, never read as its last value.

use std::collections::BTreeMap;
use std::time::Duration;

use hyper::StatusCode;
use nika_onboard::compile::remote::{self, Observed};
use nika_onboard::compile::{AuthoringCognition, CompileRequest};
use serde_json::value::RawValue;

use super::super::error::ApiError;
use super::{Answers, Change, Object, limit, present, unsupported_mode};

/// This generation's number.
pub(super) const GENERATION: u64 = 2;
/// The answer that replaces a request: never a same-plan round (S09).
pub(super) const CLARIFICATION: &str = "intent.clarification";
const EXPLICIT_PROVIDER: &str = AuthoringCognition::ExplicitProvider.word();
const DETERMINISTIC_ONLY: &str = AuthoringCognition::DeterministicOnly.word();
/// A replay token: 32 random bytes, lowercase hex.
pub(super) const TOKEN_HEX: usize = 64;

/// The bounds of one native round: the operator's, or narrower ones a caller asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Bounds {
    /// Output tokens per call.
    pub(super) max_tokens: u32,
    /// First completion's route capacity, bounded by any explicit token limit.
    pub(super) initial_tokens: u32,
    /// The wait for one call.
    pub(super) call_timeout: Duration,
    /// The whole round: calls, judging, assembly.
    pub(super) deadline: Option<Duration>,
    /// Desired repair rounds, bounded separately by explicit request authority.
    pub(super) repairs: Option<u32>,
    /// Explicit request authority, separate from repair preferences.
    pub(super) max_calls: Option<u32>,
    /// The operator or caller that narrowed the request grant.
    pub(super) grant: &'static str,
}

impl Bounds {
    pub(super) fn authority(
        self,
    ) -> Result<
        nika_onboard::compile::authority::Authority,
        nika_onboard::compile::authority::Refusal,
    > {
        use nika_onboard::compile::authority::{Authority, Door, Typed};
        Authority::resolve(
            self.max_calls,
            nika_cli_host::compile::config::DEFAULT_STRATEGY,
            Typed::new(false).with_repairs(self.repairs),
            Door::new(
                if self.max_calls.is_some() {
                    self.grant
                } else {
                    "continuous preparation: no request count"
                },
                "the explicit max_calls limit was reached; a request may only narrow its ceiling",
            ),
        )
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    compile_version: u64,
    mode: String,
    cognition: String,
    #[serde(default, deserialize_with = "present")]
    intent: Option<String>,
    #[serde(default, deserialize_with = "present")]
    workflow_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    source: Option<String>,
    #[serde(default, deserialize_with = "present")]
    change: Option<Object<Change>>,
    /// The request the base answered: a revision in words is read against the whole meaning.
    #[serde(default, deserialize_with = "present")]
    original_intent: Option<String>,
    #[serde(default)]
    answers: Answers,
    #[serde(default, deserialize_with = "present")]
    limits: Option<Object<Limits>>,
    #[serde(default, deserialize_with = "present")]
    replay_token: Option<String>,
    /// `nika compile --observe-only`'s `observed_world` and `trial_inputs`, admitted by law.
    #[serde(default, deserialize_with = "present")]
    observed_world: Option<Box<RawValue>>,
    #[serde(default, deserialize_with = "present")]
    trial_inputs: Option<Box<RawValue>>,
}

/// The caller's narrowing of the operator's bounds; each value optional.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    #[serde(default, deserialize_with = "present")]
    max_calls: Option<u32>,
    #[serde(default, deserialize_with = "present")]
    repairs: Option<u32>,
    #[serde(default, deserialize_with = "present")]
    max_tokens: Option<u32>,
    #[serde(default, deserialize_with = "present")]
    call_timeout_ms: Option<u64>,
    #[serde(default, deserialize_with = "present")]
    deadline_ms: Option<u64>,
}

/// What one round answers — the input a replay must repeat byte for byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Input {
    /// A creation, optionally named.
    Create {
        intent: String,
        workflow_id: Option<String>,
        observed: Option<Observed>,
    },
    /// A revision in words of an accepted base, beside the request that base answered.
    Revise {
        source: String,
        change: String,
        original_intent: String,
        observed: Option<Observed>,
    },
    /// One constant set from a literal: the deterministic door, zero calls.
    Constant {
        source: String,
        name: String,
        literal: String,
    },
}

impl Input {
    /// The trial inputs this input carries, if any.
    pub(super) fn trial(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Create { observed, .. } | Self::Revise { observed, .. } => {
                observed.as_ref()?.trial.as_ref()
            }
            Self::Constant { .. } => None,
        }
    }

    /// The core's request for this input with these literal answers.
    pub(super) fn request(&self, answers: &BTreeMap<String, Box<RawValue>>) -> CompileRequest {
        let mut request = match self {
            Self::Create {
                intent,
                workflow_id,
                observed,
            } => {
                let request = CompileRequest::create(intent.as_str());
                let request = match workflow_id {
                    Some(id) => request.with_workflow_id(id.as_str()),
                    None => request,
                };
                // The caller's admitted observation rides as the CLI observer's own does.
                match observed {
                    Some(observed) => request.with_knowledge(observed.world.clone()),
                    None => request,
                }
            }
            Self::Revise {
                source,
                change,
                original_intent,
                observed,
            } => {
                let request = CompileRequest::edit(source.as_str(), change.as_str())
                    .with_original_intent(original_intent.as_str());
                match observed {
                    Some(observed) => request.with_knowledge(observed.world.clone()),
                    None => request,
                }
            }
            Self::Constant {
                source,
                name,
                literal,
            } => CompileRequest::set_constant(source.as_str(), name.as_str(), literal.as_str()),
        };
        for (key, literal) in answers {
            request = request.answer(key.as_str(), literal.get());
        }
        request
    }
}

/// What the round does.
pub(super) enum Action {
    /// A fresh round under the operator's seat, within these bounds.
    Author(Bounds),
    /// A zero-call round of the plan a kept round's token names.
    Replay(String),
    /// The plan a kept round's token names, replayed with this round's answers and judged by
    /// the operator's seat within these bounds: its judge calls, never an authoring call.
    Judge(String, Bounds),
}

/// A judged generation-2 request.
pub(super) struct Request {
    pub(super) input: Input,
    /// The literal answers, as sent: the core parses each once.
    pub(super) answers: BTreeMap<String, Box<RawValue>>,
    pub(super) action: Action,
}

/// Judge a generation-2 body: vocabulary first (mode · cognition), then bounds, then shape,
/// then the literal rule — the order generation 1 keeps.
pub(super) fn parse(body: &[u8], operator: Bounds) -> Result<Request, ApiError> {
    let Ok(Object(envelope)) = serde_json::from_slice::<Object<Envelope>>(body) else {
        return Err(malformed());
    };
    if envelope.compile_version != GENERATION {
        return Err(malformed());
    }
    if !matches!(envelope.mode.as_str(), "create" | "edit") {
        return Err(unsupported_mode());
    }
    let fresh = match envelope.cognition.as_str() {
        EXPLICIT_PROVIDER => true,
        DETERMINISTIC_ONLY => false,
        _ => return Err(unsupported_cognition()),
    };
    let bounds = within_limits(&envelope, operator)?;
    let Envelope {
        mode,
        intent,
        workflow_id,
        source,
        change,
        original_intent,
        answers: Answers(answers),
        limits,
        replay_token,
        observed_world,
        trial_inputs,
        ..
    } = envelope;
    let mut input = input(&mode, intent, workflow_id, source, change, original_intent)?;
    match (observed_world, trial_inputs) {
        (Some(world), trial) => input = observed(input, &world, trial.as_deref())?,
        (None, Some(_)) => return Err(malformed()),
        (None, None) => {}
    }
    let action = match (fresh, replay_token, limits) {
        (true, None, _) => Action::Author(bounds),
        (true, Some(token), _) if is_token(&token) => Action::Judge(token, bounds),
        (false, Some(token), None) if is_token(&token) => Action::Replay(token),
        _ => return Err(malformed()),
    };
    if answers.values().any(|literal| repeats_a_key(literal))
        || matches!(&input, Input::Constant { literal, .. } if remote::repeats_a_key(literal))
    {
        return Err(malformed());
    }
    if fresh && answers.contains_key(CLARIFICATION) {
        return Err(new_intent_required());
    }
    Ok(Request {
        input,
        answers,
        action,
    })
}

/// The input the fields state: a creation (`intent`, optional `workflow_id`), a revision in
/// words (`source`, `change.text`, `original_intent`) or a structured constant (`source`,
/// `change.set_constant`). A field of another form, or a missing one, is malformed.
fn input(
    mode: &str,
    intent: Option<String>,
    workflow_id: Option<String>,
    source: Option<String>,
    change: Option<Object<Change>>,
    original_intent: Option<String>,
) -> Result<Input, ApiError> {
    match (mode, intent, source, change) {
        ("create", Some(intent), None, None) if original_intent.is_none() => Ok(Input::Create {
            intent,
            workflow_id,
            observed: None,
        }),
        ("edit", None, Some(source), Some(Object(change))) if workflow_id.is_none() => {
            match (change.text, change.set_constant, original_intent) {
                (Some(change), None, Some(original_intent)) => Ok(Input::Revise {
                    source,
                    change,
                    original_intent,
                    observed: None,
                }),
                (None, Some(Object(constant)), None) => Ok(Input::Constant {
                    source,
                    name: constant.name,
                    literal: constant.value.get().to_owned(),
                }),
                _ => Err(malformed()),
            }
        }
        _ => Err(malformed()),
    }
}

/// The input with the caller's observation, admitted against the words it states; a structured
/// constant states no file.
fn observed(input: Input, world: &RawValue, trial: Option<&RawValue>) -> Result<Input, ApiError> {
    let admit = |text: &str| Observed::admit(text, world.get(), trial.map(RawValue::get));
    match input {
        Input::Create {
            intent,
            workflow_id,
            observed: None,
        } => Ok(Input::Create {
            observed: Some(admit(&intent).map_err(refused)?),
            intent,
            workflow_id,
        }),
        Input::Revise {
            source,
            change,
            original_intent,
            observed: None,
        } => Ok(Input::Revise {
            observed: Some(admit(&format!("{original_intent}\n{change}")).map_err(refused)?),
            source,
            change,
            original_intent,
        }),
        _ => Err(malformed()),
    }
}

/// The caller may narrow route capacities and explicitly configured operator limits.
fn within_limits(envelope: &Envelope, operator: Bounds) -> Result<Bounds, ApiError> {
    let bounds = match &envelope.limits {
        None => operator,
        Some(Object(asked)) => narrow(asked, operator).ok_or_else(limit)?,
    };
    bounds.authority().map_err(|_| limit())?;
    Ok(bounds)
}

/// The operator's bounds narrowed by the caller's: every value asked must be positive (repairs
/// may be zero). A configured operator ceiling cannot be widened; above is refused, never clamped.
fn narrow(asked: &Limits, operator: Bounds) -> Option<Bounds> {
    let mut bounds = operator;
    if let Some(max_calls) = asked.max_calls {
        bounds.max_calls = Some(
            (max_calls > 0
                && operator
                    .max_calls
                    .is_none_or(|ceiling| max_calls <= ceiling))
            .then_some(max_calls)?,
        );
        bounds.grant = "request: limits.max_calls within operator ceiling";
    }
    if let Some(repairs) = asked.repairs {
        bounds.repairs = Some(
            operator
                .repairs
                .is_none_or(|ceiling| repairs <= ceiling)
                .then_some(repairs)?,
        );
    }
    if let Some(tokens) = asked.max_tokens {
        bounds.max_tokens = (1..=operator.max_tokens)
            .contains(&tokens)
            .then_some(tokens)?;
    }
    let duration = |millis: u64, ceiling: Duration| {
        let asked = Duration::from_millis(millis);
        (millis > 0 && asked <= ceiling).then_some(asked)
    };
    if let Some(millis) = asked.call_timeout_ms {
        bounds.call_timeout = duration(millis, operator.call_timeout)?;
    }
    if let Some(millis) = asked.deadline_ms {
        let asked = Duration::from_millis(millis);
        bounds.deadline = Some(
            (millis > 0
                && operator.deadline.is_none_or(|ceiling| asked <= ceiling)
                && std::time::Instant::now().checked_add(asked).is_some())
            .then_some(asked)?,
        );
    }
    Some(bounds)
}

/// 64 lowercase hexadecimal digits: the only spelling this server issues.
fn is_token(token: &str) -> bool {
    token.len() == TOKEN_HEX
        && token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn repeats_a_key(literal: &RawValue) -> bool {
    remote::repeats_a_key(literal.get())
}

pub(super) fn malformed() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "malformed_compile_request",
        "use compile_version 2 with cognition explicitProvider (create {intent} or edit {source, change}; a text change also carries original_intent; a kept round's replay_token judges its answer round) or deterministicOnly with that token; unknown fields, null values, duplicate keys (literals included) and wrong types are refused",
    )
}

/// A caller's observation or trial inputs refused by the shared law, in its words.
pub(super) fn refused(refusal: remote::Refusal) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        refusal.code(),
        refusal.message(),
    )
}

fn unsupported_cognition() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "compile_cognition_unsupported",
        "generation 2 authors with explicitProvider under this server's own seat (which also judges a kept round's replay), or replays a kept round with deterministicOnly; no other cognition is served",
    )
}

fn new_intent_required() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "compile_new_intent_required",
        "intent.clarification replaces the request: send the complete replacement as a new create intent",
    )
}
