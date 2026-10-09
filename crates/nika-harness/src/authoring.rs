// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tool-free subscription completion behind the kernel provider seam.
//! No Compiler dependency, provider fallback, prompt-side file access or JSON
//! prefix extraction: the owning Compiler validates the entire returned answer.
pub(crate) mod acp;
mod connection;
pub use connection::{reason, reason_with_effort, validate_selection};

use nika_types::access::HarnessTransport;
use nika_types::cancel::CancelCtx;
use std::sync::Mutex;
use std::task::Poll;
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
    seat: Connection,
    adapter: String,
    requested_model: Option<String>,
    wire_model: String,
    observed: Mutex<Vec<Value>>,
}

#[derive(Debug)]
enum Connection {
    Native(InferGradeSeat),
    /// The audited completion door: the registry adapter under its profile.
    Acp(Box<dyn acp::Door>),
}

impl HarnessAuthoring {
    /// Select only the named infer-grade harness. Unknown/ACP-only seats refuse.
    /// `None`/`default` retains the harness default; other model names must be
    /// forwardable by that exact adapter, never silently ignored.
    /// # Errors
    /// Unsupported capability, mismatched model namespace or invalid model.
    pub fn meet(adapter: &str, model: Option<&str>) -> Result<Self, String> {
        Self::meet_with_transport(adapter, model, HarnessTransport::Native)
    }

    /// Select a completion transport explicitly; unsupported pairs refuse without fallback.
    /// # Errors
    /// The selected adapter cannot enforce this connection's authoring contract.
    pub fn meet_with_transport(
        adapter: &str,
        model: Option<&str>,
        transport: HarnessTransport,
    ) -> Result<Self, String> {
        let wire_model = validate_selection(adapter, model, transport)?;
        let seat = if transport == HarnessTransport::Acp {
            Connection::Acp(Box::new(
                crate::seat_from_id(adapter)?
                    .ok_or_else(|| "ACP adapter is unavailable".to_owned())?
                    .for_completion(acp::Completion::Authoring)
                    .map_err(|e| e.to_string())?,
            ))
        } else {
            // One rule for every door: the native seat is the infer-grade row.
            // Codex starts only under its measured pre-execution empty-tools
            // profile, re-attested at spawn (measured minor, effective
            // features); an unmeasured version refuses before any prompt.
            let seat = meet_infer_grade(adapter, StructuredOutputGrade::JsonSchema)
                .map_err(|e| e.to_string())?;
            Connection::Native(seat)
        };
        let requested_model = model.map(str::to_owned);
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
        if let Connection::Acp(seat) = &self.seat {
            let profile = seat
                .one_shot()
                .map_or(acp::Profile::ClaudeCode, |one_shot| one_shot.profile);
            return Ok(acp::descriptor(
                profile,
                &self.adapter,
                self.requested_model.as_deref(),
                &self.wire_model,
                &observed,
            ));
        }
        Ok(json!({"kind": "harness_infer", "adapter": self.adapter,
            "requested_model": self.requested_model, "forwarded_model": self.wire_model,
            "observed": observed, "cost_basis": "subscription-backed/unknown",
            "billed_cost_usd": null, "numeric_usage_reported": false,
            "tools_exposed": if self.adapter == "codex" {
                "none executable; measured pre-execution empty-tools profile (shell, web, apps, MCP, skills, sub-agents, images disabled; code-mode exec fails closed); tool events reject the answer"
            } else {
                "none; empty native tool list; tool or permission events refuse the answer"
            },
            "context_exposed": "compiler messages only; isolated transport scratch",
            "effort": "harness default; an explicit effort or thinking budget is refused",
            "token_ceiling": "requested by Compiler; native CLI does not enforce a token cap",
            "bounds": "one turn per call; the call's own deadline; transport output byte ceiling"}))
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
            seat: Connection::Native(seat),
            adapter: adapter.to_owned(),
            requested_model: None,
            wire_model: "session".into(),
            observed: Mutex::new(Vec::new()),
        }
    }

    /// An ACP authoring seat over a scripted completion door.
    #[cfg(test)]
    pub(crate) fn with_test_door(adapter: &str, door: Box<dyn acp::Door>) -> Self {
        Self {
            seat: Connection::Acp(door),
            adapter: adapter.to_owned(),
            requested_model: None,
            wire_model: "session".into(),
            observed: Mutex::new(Vec::new()),
        }
    }

    /// One call's answer, or how it ended: Stop first, then the deadline, then the call. The
    /// waiting select only wakes; the verdict reads Stop and the clock again at the very poll that
    /// would admit the answer (the cancel flag has no waker, and a deadline timer may not have
    /// fired yet), so neither loses to an answer ready at the same instant. Nothing is retried.
    async fn call(
        &self,
        native: crate::HarnessInferRequest,
        cancel: Option<&CancelCtx>,
        deadline: acp::Deadline,
        progress: &acp::Progress,
    ) -> Ended {
        let call = async {
            match &self.seat {
                Connection::Native(seat) => seat.run(native).await.map(|out| {
                    let metadata = json!({"status":"returned", "observed_model":out.observed_model,
                        "usage_observed":out.usage_observed, "attested_version":out.attested_version});
                    (out.output, metadata)
                }).map_err(|e| Failed::Native(e.to_string())),
                Connection::Acp(door) => {
                    let requested = self.requested_model.as_deref();
                    acp::run(door.as_ref(), native, requested, deadline, progress)
                        .await
                        .map_err(Failed::Acp)
                }
            }
        };
        let ended = tokio::select! {
            biased;
            () = stopped(cancel) => Ended::Cancelled,
            () = deadline.passed() => Ended::TimedOut,
            result = call => Ended::Done(result),
        };
        match ended {
            Ended::Done(_) if cancel.is_some_and(CancelCtx::is_cancelled) => Ended::Cancelled,
            Ended::Done(_) if deadline.expired() => Ended::TimedOut,
            ended => ended,
        }
    }

    /// Record how one call ended, and answer it. An ACP record also says where the call stood,
    /// how long it ran and its bounds; a direct (native) seat's records are unchanged.
    fn settle(
        &self,
        ended: Ended,
        progress: &acp::Progress,
        deadline: acp::Deadline,
    ) -> Result<InferResponse, ProviderError> {
        let conclude = |record: Value| match &self.seat {
            Connection::Acp(_) => acp::conclude(record, progress, deadline),
            Connection::Native(_) => record,
        };
        match ended {
            Ended::Cancelled => {
                self.record(conclude(
                    json!({"status": "cancelled", "answer_accepted": false}),
                ))?;
                Err(refused("harness authoring cancelled; no answer accepted"))
            }
            Ended::TimedOut => {
                self.record(conclude(
                    json!({"status": "timed_out", "answer_accepted": false}),
                ))?;
                // Typed as the adapters type a deadline met before any status exists: a timeout
                // to every classifier, never a provider error.
                Err(ProviderError::Api {
                    status: 408,
                    message: "harness authoring timed out; no answer accepted".to_owned(),
                })
            }
            Ended::Done(Ok((text, metadata))) => {
                self.record(conclude(metadata))?;
                // The transport intentionally does not expose numeric usage. A
                // protocol usage marker is not a zero-token or zero-cost bill.
                Ok(InferResponse::new(
                    vec![ContentBlock::Text { text }],
                    TokenUsage::new(0, 0),
                    StopReason::EndTurn,
                )
                .with_usage_reported(false))
            }
            Ended::Done(Err(Failed::Native(error))) => {
                self.record(json!({"status": "failed", "reason": error}))?;
                Err(refused(error))
            }
            Ended::Done(Err(Failed::Acp(failure))) => {
                self.record(conclude(failure.record()))?;
                Err(refused(failure.message))
            }
        }
    }
}

/// How one authoring call ended.
enum Ended {
    Cancelled,
    TimedOut,
    Done(Result<(String, Value), Failed>),
}

/// A failed call: a direct seat's own words, or an ACP failure kept typed.
enum Failed {
    Native(String),
    Acp(acp::Failure),
}

/// How often a Stop is looked for between the polls of a waiting call: its flag has no waker.
const STOP_POLL: Duration = Duration::from_millis(10);

/// Resolves as soon as `cancel` reads set: at every poll of the waiting select, and every
/// [`STOP_POLL`] between them. Never without a cancel context.
fn stopped(cancel: Option<&CancelCtx>) -> impl Future<Output = ()> + Send + '_ {
    let mut tick = Box::pin(tokio::time::sleep(STOP_POLL));
    std::future::poll_fn(move |cx| {
        let Some(cancel) = cancel else {
            return Poll::Pending;
        };
        loop {
            if cancel.is_cancelled() {
                return Poll::Ready(());
            }
            if tick.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }
            tick.as_mut().reset(tokio::time::Instant::now() + STOP_POLL);
        }
    })
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

/// Codex sends `--output-schema` as a strict response format: every object must
/// list all of its properties as required and forbid additional ones (measured
/// on codex-cli 0.160.1: « 'required' is required to include every key in
/// properties »). A schema outside that dialect is stated in words instead, as
/// the ACP transport does, and the Compiler still validates the whole answer.
/// Rewriting optional fields as required would change the contract, so it is
/// never done here.
fn schema_route(
    adapter: &str,
    prompt: String,
    schema: Option<Value>,
) -> (String, Option<Value>, Option<bool>) {
    match schema {
        Some(schema) if adapter == "codex" && !strict_dialect(&schema) => (
            format!(
                "{prompt}\n\nAnswer with ONLY one JSON object, with no prose before or after \
                 and no code fence, that matches this JSON Schema:\n{schema}"
            ),
            None,
            Some(false),
        ),
        Some(schema) => (prompt, Some(schema), Some(true)),
        None => (prompt, None, None),
    }
}

/// Whether a schema node and every subschema it holds fit the strict
/// response-format dialect: each node is typed (`type`, `anyOf` or `$ref`),
/// and each object lists all its properties as required and sets
/// `additionalProperties: false` (both refusals measured on codex-cli 0.160.1).
fn strict_dialect(schema: &Value) -> bool {
    let Value::Object(node) = schema else {
        return false;
    };
    if !["type", "anyOf", "$ref"]
        .iter()
        .any(|k| node.contains_key(*k))
    {
        return false;
    }
    if let Some(properties) = node.get("properties") {
        let Value::Object(properties) = properties else {
            return false;
        };
        let required: Vec<&str> = node
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if properties.keys().any(|k| !required.contains(&k.as_str()))
            || node.get("additionalProperties") != Some(&Value::Bool(false))
            || !properties.values().all(strict_dialect)
        {
            return false;
        }
    }
    let nested = |key: &str| match node.get(key) {
        None => true,
        Some(Value::Array(all)) => all.iter().all(strict_dialect),
        Some(Value::Object(map)) if key == "$defs" || key == "definitions" => {
            map.values().all(strict_dialect)
        }
        Some(one) => strict_dialect(one),
    };
    ["items", "anyOf", "$defs", "definitions"]
        .into_iter()
        .all(nested)
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
        if timeout.is_zero() {
            return Err(refused("harness authoring deadline must be positive"));
        }
        // The call's own deadline, counted from here: setup, probes and the spawn spend it too.
        let deadline = acp::Deadline::start(timeout);
        let (system, prompt) = fold(&request.messages)?;
        let schema = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => Some(schema.clone()),
            _ => None,
        };
        let (prompt, schema, schema_enforced) = schema_route(&self.adapter, prompt, schema);
        // An explicit effort travels as its own word: the ACP session applies it through its
        // advertised reasoning option and reads it back, a direct seat refuses it — never lost.
        let effort = request
            .reasoning_effort
            .map(|level| level.word().to_owned());
        let native = HarnessInferRequest::new(prompt, &self.wire_model)
            .with_system(system)
            .with_schema(schema)
            .with_timeout(Some(timeout))
            .with_effort(effort.clone());
        self.record(
            json!({"status": "invoking", "requested_model": self.requested_model,
            "requested_effort": effort,
            "max_tokens_requested": request.max_tokens, "timeout_ms": timeout.as_millis(),
            "schema_enforced_by_harness": schema_enforced}),
        )?;
        let progress = acp::Progress::default();
        let ended = self
            .call(native, request.cancel.as_ref(), deadline, &progress)
            .await;
        self.settle(ended, &progress, deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The completion door behind the ACP connection keeps the public seat's auto traits.
    #[test]
    fn the_authoring_seat_keeps_its_auto_traits() {
        fn holds<T: Send + Sync + Unpin + std::panic::UnwindSafe + std::panic::RefUnwindSafe>() {}
        holds::<HarnessAuthoring>();
    }

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
        let codex = HarnessAuthoring::meet("codex", None).expect("attested native profile");
        let receipt = codex.descriptor().expect("descriptor");
        assert!(
            receipt["tools_exposed"]
                .as_str()
                .is_some_and(|t| t.contains("pre-execution empty-tools profile")),
            "{receipt}"
        );
        assert!(HarnessAuthoring::meet("claude-code", None).is_ok());
    }
    #[test]
    fn a_codex_schema_outside_the_strict_dialect_is_stated_in_words() {
        let loose = json!({"type": "object", "required": ["a"],
            "properties": {"a": {"type": "string"}, "b": {"type": "string"}}});
        let (prompt, schema, enforced) = schema_route("codex", "P".into(), Some(loose.clone()));
        assert!(schema.is_none());
        assert_eq!(enforced, Some(false));
        assert!(prompt.starts_with("P\n\nAnswer with ONLY one JSON object"));
        assert!(prompt.ends_with(&loose.to_string()));
        let strict = json!({"type": "object", "required": ["a"], "additionalProperties": false,
            "properties": {"a": {"type": "array", "items": {"type": "object",
                "required": ["x"], "additionalProperties": false,
                "properties": {"x": {"type": "string"}}}}}});
        let (prompt, schema, enforced) = schema_route("codex", "P".into(), Some(strict.clone()));
        assert_eq!(
            (prompt.as_str(), schema, enforced),
            ("P", Some(strict), Some(true))
        );
        let nested_loose = json!({"type": "object", "required": ["a"], "additionalProperties": false,
            "properties": {"a": {"type": "object", "properties": {"x": {"type": "string"}}}}});
        assert!(!strict_dialect(&nested_loose));
        let untyped = json!({"type": "object", "required": ["a"], "additionalProperties": false,
            "properties": {"a": {"type": "array", "items": {"type": "object",
                "required": ["value"], "additionalProperties": false,
                "properties": {"value": {}}}}}});
        assert!(
            !strict_dialect(&untyped),
            "an untyped value node is not strict"
        );
        let (_, schema, _) = schema_route("claude-code", "P".into(), Some(loose.clone()));
        assert_eq!(
            schema,
            Some(loose),
            "other adapters keep their native schema flag"
        );
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
