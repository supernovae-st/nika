// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reasoning budget of a conversation's document. A model that reasons before it answers
//! spends that reasoning inside the call's output cap: a cap with no room left for the answer
//! ends the run with no answer (NIKA-INFER-004) or with one cut short. The native door sizes
//! the caps it writes itself (the reasoning room, bounded by the known output limit) and never
//! rewrites a stated one; a conversation's author states its own, so each one is judged here,
//! before any proposal. The facts are a typed input, what a route says of its model: whether it
//! reasons by default, the room its reasoning and answer need, whether `run.reasoning.effort:
//! low` reaches it. Only a remedy that reaches the route is named: raising the cap always, the
//! low effort where the model documents it; never turning thinking off, which no route fact
//! reaches yet (a `thinking:` block the route does not map changes nothing, so it settles no
//! task). A cap the person typed stays theirs. An `agent:` task states `max_tokens_total`,
//! cumulative across its requests: no single request's cap is stated there, so none is judged.

use serde_json::Value;

use super::{Diagnostic, resolution};

/// What a route says of its model's output, as the reasoning-budget law reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReasoningFacts {
    /// The model reasons before it answers unless the request turns it down or off.
    pub reasons_by_default: bool,
    /// The output cap that leaves room for the reasoning and the answer.
    pub room: Option<u32>,
    /// Whether `run.reasoning.effort: low` reaches the route: the exact model documents that
    /// level (its direct route carries it).
    pub low_effort: bool,
}

impl ReasoningFacts {
    /// The facts of a route: whether it reasons by default, and the room its answer needs.
    #[must_use]
    pub fn new(reasons_by_default: bool, room: Option<u32>) -> Self {
        Self {
            reasons_by_default,
            room,
            low_effort: false,
        }
    }

    /// Whether `run.reasoning.effort: low` reaches the route.
    #[must_use]
    pub fn with_low_effort(mut self, reaches: bool) -> Self {
        self.low_effort = reaches;
        self
    }

    /// The facts a catalog records for a seat, as `nika_compile::surface::output_caps` reports
    /// them: the room the core gives its own caps, bounded by the known output limit, and the
    /// effort levels the exact model documents. A model reasons by default when its row records
    /// reasoning and documents effort levels (thinking on unless lowered); a row recording
    /// reasoning alone records a capability, which a Claude row only uses when a task asks.
    #[must_use]
    pub fn of_caps(caps: &Value) -> Self {
        let efforts = caps["reasoning_efforts"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let reasons = caps["reasoning_capability"] == "recorded" && !efforts.is_empty();
        let room = caps["suggested_default_max_tokens"].as_u64();
        let room = room.and_then(|room| u32::try_from(room).ok());
        let low = efforts.iter().any(|level| level == "low");
        Self::new(reasons, room).with_low_effort(low)
    }
}

/// Each `infer:` task whose cap cannot cover its model's reasoning plus the answer: refused,
/// the remedies that reach its route stated, unless the run lowers the effort where that
/// reaches it, or the person typed the cap. A templated cap or seat is judged at run time.
pub fn reasoning(
    stated: &str,
    doc: &Value,
    facts: &dyn Fn(&str) -> Option<ReasoningFacts>,
    out: &mut Vec<Diagnostic>,
) {
    let lowered = doc["run"]["reasoning"]["effort"] == "low";
    for (id, task) in doc["tasks"].as_object().into_iter().flatten() {
        let infer = &task["infer"];
        if !infer.is_object() {
            continue;
        }
        let Some(seat) = infer["model"].as_str().or(doc["model"].as_str()) else {
            continue;
        };
        let Some(facts) = facts(seat).filter(|known| known.reasons_by_default) else {
            continue;
        };
        // A stated low effort settles the task where it reaches the route.
        if lowered && facts.low_effort {
            continue;
        }
        let Some(room) = facts.room else {
            continue;
        };
        let low = if facts.low_effort {
            ", or set `run.reasoning.effort: low` (a level this model documents)"
        } else {
            ""
        };
        let cap = &infer["max_tokens"];
        let message = match cap.as_u64() {
            Some(cap) if cap >= u64::from(room) || resolution::typed(stated, &cap.to_string()) => {
                continue;
            }
            Some(cap) => format!(
                "REASONING BUDGET: task `{id}` asks `{seat}`, a model that reasons before it answers, for at most {cap} output tokens: its reasoning can spend them all and leave no answer (NIKA-INFER-004), or cut the answer short. Raise `max_tokens` to {room}{low}."
            ),
            None if cap.is_null() => format!(
                "REASONING BUDGET: task `{id}` asks `{seat}`, a model that reasons before it answers, with no output cap: the run's default may leave no room for the answer after the reasoning (NIKA-INFER-004). State `max_tokens: {room}`{low}."
            ),
            None => continue,
        };
        out.push(Diagnostic {
            kind: "reasoning_budget",
            message,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ASKED: &str = "Summarize the tech news of today in three bullet points";

    /// A route that reasons by default, its room 16384, the low effort reaching it.
    fn thinker(model: &str) -> Option<ReasoningFacts> {
        let facts = ReasoningFacts::new(true, Some(16_384)).with_low_effort(true);
        (model == "acme/thinker").then_some(facts)
    }

    fn summary(infer: &Value) -> Value {
        json!({"model": "acme/thinker", "tasks": {"summarize": {"infer": infer}}})
    }

    fn refused(stated: &str, doc: &Value) -> Vec<String> {
        let mut out = Vec::new();
        reasoning(stated, doc, &thinker, &mut out);
        out.into_iter().map(|d| d.message).collect()
    }

    /// A reasoning model's cap leaves room for the answer, or the author is told how to: raise
    /// it, or lower the effort. A room, the low effort or the person's own number settles it; a
    /// thinking block the route does not map, or a higher effort, does not; a model whose facts
    /// record no reasoning is not judged.
    #[test]
    fn a_reasoning_model_s_cap_leaves_room_for_the_answer() {
        let cut = summary(&json!({"prompt": "Summarize", "max_tokens": 4096}));
        let said = refused(ASKED, &cut);
        assert!(
            said.len() == 1
                && said[0].starts_with("REASONING BUDGET: task `summarize` asks `acme/thinker`")
                && said[0].contains("at most 4096 output tokens")
                && said[0]
                    .contains("Raise `max_tokens` to 16384, or set `run.reasoning.effort: low`")
                && !said[0].contains("thinking"),
            "{said:?}"
        );
        let open = summary(&json!({"prompt": "Summarize"}));
        let said = refused(ASKED, &open);
        assert!(
            said.len() == 1 && said[0].contains("State `max_tokens: 16384`"),
            "{said:?}"
        );
        let effort = |level: &str| {
            json!({"model": "acme/thinker", "run": {"reasoning": {"effort": level}},
                "tasks": {"summarize": {"infer": {"prompt": "Summarize", "max_tokens": 4096}}}})
        };
        for doc in [
            summary(&json!({"prompt": "Summarize", "max_tokens": 16_384})),
            effort("low"),
        ] {
            assert!(refused(ASKED, &doc).is_empty(), "{doc}");
        }
        for doc in [
            summary(&json!({"prompt": "Summarize", "max_tokens": 4096,
                "thinking": {"enabled": false}})),
            effort("high"),
        ] {
            assert_eq!(refused(ASKED, &doc).len(), 1, "{doc}");
        }
        assert!(refused("Summarize the news in at most 4096 tokens", &cut).is_empty());
        let plain = json!({"model": "acme/plain",
            "tasks": {"summarize": {"infer": {"prompt": "Summarize", "max_tokens": 1024}}}});
        assert!(refused(ASKED, &plain).is_empty());
        let templated = summary(&json!({"prompt": "Summarize", "max_tokens": "${{ inputs.cap }}"}));
        assert!(refused(ASKED, &templated).is_empty());
    }

    /// The catalog's facts, as `output_caps` reports them: a row documenting effort levels
    /// reasons by default, `low` among them reaching it; reasoning recorded alone is a capability.
    #[test]
    fn the_catalog_facts_read_reasoning_by_default_from_documented_efforts() {
        let caps = |efforts: Value| {
            json!({"reasoning_capability": "recorded", "reasoning_efforts": efforts,
                "suggested_default_max_tokens": 16_384})
        };
        let flash = ReasoningFacts::of_caps(&caps(json!(["low", "high", "max"])));
        assert_eq!(
            flash,
            ReasoningFacts::new(true, Some(16_384)).with_low_effort(true)
        );
        let claude = ReasoningFacts::of_caps(&caps(json!([])));
        assert!(
            !claude.reasons_by_default && !claude.low_effort,
            "{claude:?}"
        );
        let high = ReasoningFacts::of_caps(&caps(json!(["high"])));
        assert!(high.reasons_by_default && !high.low_effort, "{high:?}");
    }

    /// The low effort is named, and settles a task, only where it reaches the route: elsewhere the
    /// cap alone is the remedy, and a stated low effort leaves the cap judged.
    #[test]
    fn the_low_effort_is_named_only_where_it_reaches_the_route() {
        let doc = summary(&json!({"prompt": "Summarize", "max_tokens": 4096}));
        let lowered = json!({"model": "acme/thinker", "run": {"reasoning": {"effort": "low"}},
            "tasks": {"summarize": {"infer": {"prompt": "Summarize", "max_tokens": 4096}}}});
        let unreached = |_: &str| Some(ReasoningFacts::new(true, Some(16_384)));
        for doc in [&doc, &lowered] {
            let mut out = Vec::new();
            reasoning(ASKED, doc, &unreached, &mut out);
            let said: Vec<String> = out.into_iter().map(|d| d.message).collect();
            assert!(
                said.len() == 1
                    && said[0].contains("Raise `max_tokens` to 16384.")
                    && !said[0].contains("effort"),
                "{said:?}"
            );
        }
    }
}
