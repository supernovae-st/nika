// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An ACP harness as an AUTHORING BACKEND (the 2026-09-22 addendum): the operator's own agent
//! harness (Claude Code · Codex · Gemini CLI · Kimi Code · Qwen Code), driven through the Agent
//! Client Protocol the engine already speaks for the `agent:` verb and the run door's
//! `--access`, answers the compiler's authoring calls behind the same `ProviderInferDyn` seam
//! a direct API provider answers. The compile core stays backend-neutral. What the harness
//! received is what the compiler sent: the system message as the session's system instruction,
//! the conversation folded into one prompt, the answer schema stated in words (ACP enforces
//! none, so the first complete JSON object of the answer is the answer). Every tool permission
//! the harness asks for is DENIED — authoring is tool-free by contract — and the receipt names
//! the backend, the adapter, the observed model, whether usage was reported and the cost basis
//! (a subscription: no token meter is fabricated).
use nika_harness::{SpawnedHarness, seat_from_id};
use nika_kernel::ai::harness::{
    AgentBackendDyn, HarnessEvent, HarnessOutcome, HarnessRequest, PermissionDecision,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderError, ProviderInferDyn,
    ResponseFormat, Role, StopReason, TokenUsage,
};
use nika_types::access::HarnessRuntime;
use serde_json::{Value, json};
use std::sync::Mutex;

/// One harness seat for the length of a compile: one ACP session per authoring call.
pub(super) struct HarnessSeat {
    seat: SpawnedHarness,
    runtime: HarnessRuntime,
    requested_model: Option<String>,
    cwd: std::path::PathBuf,
    /// What each call observed: the model the harness named, whether it reported usage, the
    /// tool asks denied.
    observed: Mutex<Vec<Value>>,
}

impl HarnessSeat {
    /// Whether `<adapter>/<model>` names a harness the engine knows (`claude-code/default`).
    pub(super) fn names_a_harness(spec: &str) -> bool {
        spec.split_once('/')
            .is_some_and(|(id, _)| HarnessRuntime::lookup(id).is_some())
    }

    /// Seat the harness `<adapter>/<model>` names (`default` or an empty model leaves the
    /// harness's own choice).
    pub(super) fn meet(spec: &str) -> Result<Self, String> {
        let (id, model) = spec.split_once('/').ok_or_else(|| {
            format!("a harness seat is `<adapter>/<model>` (`claude-code/default`), not `{spec}`")
        })?;
        let runtime = HarnessRuntime::lookup(id).ok_or_else(|| {
            format!(
                "`{id}` is not a harness the engine knows; the tokens are {}",
                HarnessRuntime::ALL
                    .iter()
                    .map(|r| r.id)
                    .collect::<Vec<_>>()
                    .join(" · ")
            )
        })?;
        let seat = seat_from_id(id)?.ok_or_else(|| format!("harness `{id}`: no adapter row"))?;
        let cwd = std::env::current_dir()
            .map_err(|e| format!("harness seat: no working directory: {e}"))?;
        let requested_model = match model.trim() {
            "" | "default" => None,
            m => Some(m.to_owned()),
        };
        Ok(Self {
            seat,
            runtime,
            requested_model,
            cwd,
            observed: Mutex::new(Vec::new()),
        })
    }

    /// The receipt's backend descriptor: the addendum's fields, observed where observable.
    pub(super) fn descriptor(&self) -> Value {
        let observed = self.observed.lock().map(|o| o.clone()).unwrap_or_default();
        json!({
            "kind": "acp_harness",
            "adapter": self.runtime.id,
            "display": self.runtime.display,
            "acp_bin": self.runtime.acp_bin,
            "requested_model": self.requested_model,
            "observed": observed,
            "protocol": "ACP · one session per authoring call · the harness's own tools ride its permission bridge and every ask is denied here",
            "tools_exposed": "the harness's own (every ask denied for authoring)",
            "context_exposed": "the compiler's messages only (card · callables · references · request · diagnostics)",
            "effort": "the harness's default; not configurable through this seat",
            "cost_basis": "subscription-backed/unknown",
        })
    }

    fn finish(&self, outcome: HarnessOutcome, denied: u32) -> InferResponse {
        let reported = outcome.usage.is_some();
        if let Ok(mut observed) = self.observed.lock() {
            observed.push(json!({
                "observed_model": outcome.observed_model,
                "usage_reported": reported,
                "asks_denied": denied,
            }));
        }
        let text =
            json_object(&outcome.output).map_or_else(|| outcome.output.clone(), str::to_owned);
        let mut response = InferResponse::new(
            vec![ContentBlock::Text { text }],
            outcome.usage.unwrap_or_else(|| TokenUsage::new(0, 0)),
            StopReason::EndTurn,
        );
        // A subscription quota is not a token meter: only what the harness itself reported.
        response.usage_reported = reported;
        response
    }
}

/// The system instruction and the one prompt a session receives: every system message joined,
/// then the conversation in order, each earlier answer labeled; the answer schema stated in
/// words since ACP enforces none.
fn fold(messages: &[Message], schema: Option<&Value>) -> (Option<String>, String) {
    let text_of = |m: &Message| -> String {
        m.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut system: Vec<String> = Vec::new();
    let mut prompt: Vec<String> = Vec::new();
    for message in messages {
        match message.role {
            Role::System => system.push(text_of(message)),
            Role::Assistant => {
                prompt.push(format!("[your previous answer]\n{}", text_of(message)));
            }
            _ => prompt.push(text_of(message)),
        }
    }
    if let Some(schema) = schema {
        prompt.push(format!(
            "Answer with ONLY one JSON object — no prose before or after, no code fence — that matches this JSON Schema:\n{schema}"
        ));
    }
    let system = (!system.is_empty()).then(|| system.join("\n\n"));
    (system, prompt.join("\n\n"))
}

/// The first complete JSON object in a harness answer (a harness may wrap it in prose or a
/// fence); None when the text carries no balanced object.
fn json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in text[start..].char_indices() {
        if in_string {
            match ch {
                '\\' if !escaped => {
                    escaped = true;
                    continue;
                }
                '"' if !escaped => in_string = false,
                _ => {}
            }
            escaped = false;
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + ch.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

impl ProviderInferDyn for HarnessSeat {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let schema = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => Some(schema),
            _ => None,
        };
        let (system, prompt) = fold(&request.messages, schema);
        let mut session = HarnessRequest::new(prompt, self.cwd.clone());
        if let Some(system) = system {
            session = session.with_system(system);
        }
        if let Some(model) = &self.requested_model {
            session = session.with_requested_model(model.clone());
        }
        let refusal = |why: String| ProviderError::Other {
            reason: format!("harness `{}`: {why}", self.runtime.id),
        };
        let mut stream = self
            .seat
            .run_agent(session)
            .await
            .map_err(|e| refusal(e.to_string()))?;
        let mut denied: u32 = 0;
        loop {
            let next =
                std::future::poll_fn(|cx| futures_core::Stream::poll_next(stream.as_mut(), cx))
                    .await;
            match next {
                Some(Ok(HarnessEvent::PermissionAsked {
                    question, reply, ..
                })) => {
                    denied += 1;
                    reply.respond(PermissionDecision::Deny);
                    if let Ok(mut observed) = self.observed.lock() {
                        observed.push(json!({"denied_ask": question}));
                    }
                }
                Some(Ok(HarnessEvent::Completed { outcome })) => {
                    return Ok(self.finish(*outcome, denied));
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(refusal(e.to_string())),
                None => {
                    return Err(refusal("the session ended without an answer".to_owned()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_conversation_folds_into_one_system_and_one_labeled_prompt_with_the_schema_in_words() {
        let messages = vec![
            Message::text(Role::System, "CARD"),
            Message::text(Role::User, "{\"request\":\"x\"}"),
            Message::text(Role::Assistant, "{\"candidate\":\"nika: a\"}"),
            Message::text(Role::User, "COMPILER DIAGNOSTICS"),
        ];
        let (system, prompt) = fold(&messages, Some(&json!({"type": "object"})));
        assert_eq!(system.as_deref(), Some("CARD"));
        assert!(prompt.starts_with(
            "{\"request\":\"x\"}\n\n[your previous answer]\n{\"candidate\":\"nika: a\"}\n\nCOMPILER DIAGNOSTICS\n\nAnswer with ONLY one JSON object"
        ));
        assert!(prompt.ends_with("{\"type\":\"object\"}"));
    }

    #[test]
    fn the_first_complete_json_object_is_the_answer_whatever_wraps_it() {
        assert_eq!(
            json_object("Sure!\n```json\n{\"candidate\": \"a}b\", \"n\": {\"k\": 1}}\n```"),
            Some("{\"candidate\": \"a}b\", \"n\": {\"k\": 1}}")
        );
        assert_eq!(json_object("no object here"), None);
        assert_eq!(json_object("{\"open\": true"), None);
        assert_eq!(
            json_object("{\"q\": \"\\\"}\"}"),
            Some("{\"q\": \"\\\"}\"}")
        );
    }

    #[test]
    fn a_harness_seat_is_named_by_a_token_the_engine_knows() {
        assert!(HarnessSeat::names_a_harness("claude-code/default"));
        assert!(HarnessSeat::names_a_harness("kimi-code/kimi-k2"));
        assert!(!HarnessSeat::names_a_harness("xai/grok-4"));
        assert!(!HarnessSeat::names_a_harness("claude-code"));
    }
}
