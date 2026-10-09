// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The request a remote compile door reads, in the only spellings its contract publishes: a JSON
//! object and nothing else, a present value never `null`, literal answers kept as the exact text
//! the caller sent (a repeated key selects neither), and the input one round answers, with the
//! caller's observation admitted against the words it states and the core's request it states.
//! Pure: the door keeps its envelope, its bounds and its words.

use std::collections::BTreeMap;

use nika_compile::CompileRequest;
use serde_json::value::RawValue;

use super::{Observed, Refusal};

/// A JSON object and nothing else. A derived struct would also accept a positional
/// array (`[1, "create", "hello"]`), a spelling this contract never publishes.
pub struct Object<T>(pub T);

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Object<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for ObjectVisitor<T> {
            type Value = T;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
                // The derived visitor still reads the ORIGINAL map, so unknown
                // fields, duplicates and raw literals are judged exactly as before.
                T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }

        deserializer
            .deserialize_map(ObjectVisitor(std::marker::PhantomData))
            .map(Object)
    }
}

/// Literal answers keyed by stable question key, with their sent text preserved.
#[derive(Default)]
pub struct Answers(pub BTreeMap<String, Box<RawValue>>);

impl<'de> serde::Deserialize<'de> for Answers {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AnswersVisitor;

        impl<'de> serde::de::Visitor<'de> for AnswersVisitor {
            type Value = Answers;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an object of literal answers")
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut answers = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    let literal = map.next_value::<Box<RawValue>>()?;
                    // Two values for one question select neither (the core's own law).
                    if answers.insert(key, literal).is_some() {
                        return Err(serde::de::Error::custom("duplicate answer key"));
                    }
                }
                Ok(Answers(answers))
            }
        }

        deserializer.deserialize_map(AnswersVisitor)
    }
}

/// A present value must have its type; JSON `null` never means "absent".
///
/// # Errors
/// The value's own refusal: `null` or a value of another type.
pub fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// What one round answers — the input a replay must repeat byte for byte.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Input {
    /// A creation, optionally named.
    Create {
        /// The request, in words.
        intent: String,
        /// The workflow's name, when the caller chose one.
        workflow_id: Option<String>,
        /// The caller's admitted observation.
        observed: Option<Observed>,
    },
    /// A revision in words of an accepted base, beside the request that base answered.
    Revise {
        /// The accepted base.
        source: String,
        /// The change, in words.
        change: String,
        /// The request the base answered.
        original_intent: String,
        /// The caller's admitted observation.
        observed: Option<Observed>,
    },
    /// One constant set from a literal: the deterministic door, zero calls.
    Constant {
        /// The accepted base.
        source: String,
        /// The constant.
        name: String,
        /// Its new value, the literal exactly as sent.
        literal: String,
    },
}

impl Input {
    /// This input with its caller's observation (`world`, the `trial` inputs beside it), admitted
    /// against the words it states: a creation's intent, a revision's original request and its
    /// change. `None` when it takes none: a structured constant states no file, and an input is
    /// observed once.
    #[must_use]
    pub fn observe(self, world: &str, trial: Option<&str>) -> Option<Result<Self, Refusal>> {
        let admit = |text: &str| Observed::admit(text, world, trial).map(Some);
        Some(match self {
            Self::Create {
                intent,
                workflow_id,
                observed: None,
            } => admit(&intent).map(|observed| Self::Create {
                intent,
                workflow_id,
                observed,
            }),
            Self::Revise {
                source,
                change,
                original_intent,
                observed: None,
            } => admit(&format!("{original_intent}\n{change}")).map(|observed| Self::Revise {
                source,
                change,
                original_intent,
                observed,
            }),
            _ => return None,
        })
    }

    /// The trial inputs this input carries, if any.
    #[must_use]
    pub fn trial(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Create { observed, .. } | Self::Revise { observed, .. } => {
                observed.as_ref()?.trial.as_ref()
            }
            Self::Constant { .. } => None,
        }
    }

    /// The core's request for this input with these literal answers.
    #[must_use]
    pub fn request(&self, answers: &BTreeMap<String, Box<RawValue>>) -> CompileRequest {
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
