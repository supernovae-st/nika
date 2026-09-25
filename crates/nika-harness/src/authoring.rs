// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tool-free subscription completion behind the kernel provider seam.
//! No Compiler dependency, provider fallback, prompt-side file access or JSON
//! prefix extraction: the owning Compiler validates the entire returned answer.
use std::sync::Mutex;
use std::time::Duration;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderError, ProviderInferDyn,
    ResponseFormat, Role, StopReason, TokenUsage,
};
use serde_json::{Value, json};

use crate::{HarnessInferRequest, InferGradeSeat, StructuredOutputGrade, meet_infer_grade};

/// One explicitly selected subscription backend for one bounded authoring round.
/// The transport owns version checks, tool refusal, isolated scratch and cleanup.
#[derive(Debug)]
#[non_exhaustive]
pub struct HarnessAuthoring {
    seat: InferGradeSeat,
    adapter: String,
    requested_model: Option<String>,
    wire_model: String,
    observed: Mutex<Vec<Value>>,
}

impl HarnessAuthoring {
    /// Select only the named infer-grade harness. Unknown/ACP-only seats refuse.
    /// `None`/`default` retains the harness default; other model names must be
    /// forwardable by that exact adapter, never silently ignored.
    /// # Errors
    /// Unsupported capability, mismatched model namespace or invalid model.
    pub fn meet(adapter: &str, model: Option<&str>) -> Result<Self, String> {
        // A read-only sandbox and rejection of tool events after return do
        // not prevent native shell/tool execution. Do not start authoring
        // until this adapter has an attested pre-execution no-tools mode.
        if adapter == "codex" {
            return Err("subscription authoring `codex` is unavailable: pre-execution tool disabling is not attested; read-only scratch and post-return tool-event rejection are insufficient; no provider fallback".into());
        }
        let seat = meet_infer_grade(adapter, StructuredOutputGrade::JsonSchema)
            .map_err(|e| e.to_string())?;
        let requested_model = model.map(str::to_owned);
        let wire_model = model_argument(adapter, model)?;
        Ok(Self {
            seat,
            adapter: adapter.to_owned(),
            requested_model,
            wire_model,
            observed: Mutex::new(Vec::new()),
        })
    }

    /// Backend evidence, independent of any Compiler or invoice interpretation.
    /// # Errors
    /// An unreadable observation ledger is not represented as zero calls.
    pub fn descriptor(&self) -> Result<Value, String> {
        let observed = self.observed.lock().map_err(|e| e.to_string())?.clone();
        Ok(json!({"kind": "harness_infer", "adapter": self.adapter,
            "requested_model": self.requested_model, "forwarded_model": self.wire_model,
            "observed": observed, "cost_basis": "subscription-backed/unknown",
            "billed_cost_usd": null, "numeric_usage_reported": false,
            "tools_exposed": if self.adapter == "codex" {
                "read-only scratch; tool events reject the answer; pre-execution tool disable not attested"
            } else {
                "none; empty native tool list; tool or permission events refuse the answer"
            },
            "context_exposed": "compiler messages only; isolated transport scratch",
            "effort": "harness default; explicit thinking budget unsupported",
            "token_ceiling": "requested by Compiler; native CLI does not enforce a token cap",
            "bounds": "one turn per call; at most 600 seconds; transport output byte ceiling"}))
    }

    fn record(&self, value: Value) -> Result<(), ProviderError> {
        self.observed
            .lock()
            .map_err(|e| refused(e.to_string()))?
            .push(value);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn with_test_seat(adapter: &str, seat: InferGradeSeat) -> Self {
        Self {
            seat,
            adapter: adapter.to_owned(),
            requested_model: None,
            wire_model: "session".into(),
            observed: Mutex::new(Vec::new()),
        }
    }
}

fn refused(reason: impl Into<String>) -> ProviderError {
    ProviderError::Other {
        reason: reason.into(),
    }
}

/// Normalize a selected model for the native infer-grade adapter, or refuse.
/// `session` is the existing transport sentinel for the harness default.
/// # Errors
/// The requested namespace/model cannot be forwarded by this adapter.
pub fn model_argument(adapter: &str, model: Option<&str>) -> Result<String, String> {
    let Some(model) = model.filter(|m| !m.is_empty() && *m != "default") else {
        return Ok("session".into());
    };
    if model.trim() != model || model.chars().any(char::is_whitespace) {
        return Err("harness model must be one nonempty token".into());
    }
    let prefix = match adapter {
        "codex" => "openai",
        other => other,
    };
    let (owner, name) = model.split_once('/').unwrap_or((prefix, model));
    let accepted = match adapter {
        "codex" => matches!(owner, "codex" | "openai"),
        "claude-code" => matches!(owner, "claude-code" | "anthropic"),
        "grok-build" => matches!(owner, "grok-build" | "xai"),
        _ => owner == adapter,
    };
    if !accepted || name.is_empty() || name.contains('/') {
        return Err(format!("harness `{adapter}` cannot honor model `{model}`"));
    }
    if name == "default" {
        return Ok("session".into());
    }
    Ok(format!("{prefix}/{name}"))
}

fn fold(messages: &[Message]) -> Result<(Option<String>, String), ProviderError> {
    let mut system = Vec::new();
    let mut prompt = Vec::new();
    for message in messages {
        let mut chunks = Vec::new();
        for block in &message.content {
            match block {
                ContentBlock::Text { text } => chunks.push(text.as_str()),
                _ => return Err(refused("harness authoring supports text only")),
            }
        }
        let text = chunks.join("\n");
        match message.role {
            Role::System => system.push(text),
            Role::Assistant => prompt.push(format!("[your previous answer]\n{text}")),
            Role::User => prompt.push(text),
            _ => return Err(refused("tool-role messages are not authoring input")),
        }
    }
    Ok((
        (!system.is_empty()).then(|| system.join("\n\n")),
        prompt.join("\n\n"),
    ))
}

impl ProviderInferDyn for HarnessAuthoring {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        if !request.tools.is_empty() || request.thinking_budget.is_some() {
            return Err(refused(
                "harness authoring cannot honor tools or an explicit thinking budget",
            ));
        }
        let timeout = request.timeout.unwrap_or(Duration::from_secs(300));
        if timeout.is_zero() || timeout > Duration::from_secs(600) {
            return Err(refused(
                "harness authoring deadline must be positive and at most 600 seconds",
            ));
        }
        let (system, prompt) = fold(&request.messages)?;
        let schema = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => Some(schema.clone()),
            _ => None,
        };
        let native = HarnessInferRequest::new(prompt, &self.wire_model)
            .with_system(system)
            .with_schema(schema)
            .with_timeout(Some(timeout));
        self.record(
            json!({"status": "invoking", "requested_model": self.requested_model,
            "max_tokens_requested": request.max_tokens, "timeout_ms": timeout.as_millis()}),
        )?;
        let call = tokio::time::timeout(timeout, self.seat.run(native));
        let result = if let Some(cancel) = request.cancel {
            tokio::select! {
                biased;
                () = async {
                    while !cancel.is_cancelled() {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                } => {
                    self.record(json!({"status": "cancelled", "answer_accepted": false}))?;
                    return Err(refused("harness authoring cancelled; no answer accepted"));
                },
                result = call => result,
            }
        } else {
            call.await
        };
        let Ok(result) = result else {
            self.record(json!({"status": "timed_out", "answer_accepted": false}))?;
            return Err(refused("harness authoring timed out; no answer accepted"));
        };
        match result {
            Ok(out) => {
                self.record(json!({"status": "returned", "observed_model": out.observed_model,
                    "usage_observed": out.usage_observed, "attested_version": out.attested_version}))?;
                // The transport intentionally does not expose numeric usage. A
                // protocol usage marker is not a zero-token or zero-cost bill.
                Ok(InferResponse::new(
                    vec![ContentBlock::Text { text: out.output }],
                    TokenUsage::new(0, 0),
                    StopReason::EndTurn,
                )
                .with_usage_reported(false))
            }
            Err(error) => {
                self.record(json!({"status": "failed", "reason": error.to_string()}))?;
                Err(refused(error.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_models_are_forwarded_or_refused_never_ignored() {
        assert_eq!(
            model_argument("codex", Some("codex/chosen")).unwrap(),
            "openai/chosen"
        );
        assert_eq!(
            model_argument("claude-code", Some("anthropic/chosen")).unwrap(),
            "claude-code/chosen"
        );
        assert_eq!(model_argument("codex", None).unwrap(), "session");
        assert_eq!(
            model_argument("codex", Some("codex/default")).unwrap(),
            "session"
        );
        assert!(model_argument("codex", Some("anthropic/chosen")).is_err());
        assert!(HarnessAuthoring::meet("kimi-code", None).is_err());
        let error = HarnessAuthoring::meet("codex", None).unwrap_err();
        assert!(error.contains("pre-execution tool disabling"), "{error}");
        assert!(HarnessAuthoring::meet("claude-code", None).is_ok());
    }
    #[test]
    fn messages_keep_order_and_complete_answer_contract() {
        let (system, prompt) = fold(&[
            Message::text(Role::System, "CARD"),
            Message::text(Role::User, "original"),
            Message::text(Role::Assistant, "previous"),
            Message::text(Role::User, "diagnostic"),
        ])
        .unwrap();
        assert_eq!(system.as_deref(), Some("CARD"));
        assert_eq!(
            prompt,
            "original\n\n[your previous answer]\nprevious\n\ndiagnostic"
        );
    }
}
