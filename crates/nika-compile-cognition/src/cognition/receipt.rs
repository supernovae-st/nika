// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One authoring call and what it leaves in the outcome's receipt: the bounded request every
//! authoring call makes, whatever its messages, and the call's journal entry (its role, the
//! digests of what it was shown, its bounds, its result, its usage).

use nika_compile::AuthoringReasoning;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderError, ProviderInferDyn,
    ReasoningEffort, ResponseFormat, Role,
};
use serde_json::{Value, json};

use crate::{
    AuthoringCognition, AuthoringPolicy, AuthoringReceipt, CompileOutcome, DiagnosticKind,
};

/// What a host may observe of its own compile's authoring calls (slice C): each answer exactly as
/// the compiler received it at `compiler_provider_response`, before any decode, delivered to a
/// sink the host scopes around its compile future. Authoring and repair calls only (a judge's
/// choice is never observed), the prompt by identity only, no request or response serialized,
/// nothing stored here: persistence, caps and redaction are the host's. Received is not
/// persisted, and an observation claims no decode disposition. The sink is trusted host code
/// that cannot change a candidate, an authority, a call count or a public record; it must not
/// panic. Nothing outlives the process: no crash durability is promised.
pub(crate) mod observe {
    use std::fmt::Write as _;
    use std::future::Future;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use nika_kernel::ai::provider::{
        ContentBlock, InferResponse, Message, ProviderError, Role, StopReason,
    };
    use sha2::{Digest as _, Sha256};

    /// The host's observer of one scope.
    pub type Sink = Arc<dyn for<'a, 'b> Fn(&'a AuthoringObservation<'b>) + Send + Sync>;

    /// One authoring call as the compiler received it.
    #[non_exhaustive]
    pub struct AuthoringObservation<'a> {
        /// 1-based within the scope that observed it (not the public call counter).
        pub ordinal: u32,
        /// The compiler phase that asked (plan, repair, native, sketch, fill, their repairs,
        /// transform); never a judge.
        pub role: &'static str,
        /// The identity of the ordered messages sent, each framed by its role and length; no text.
        pub prompt_sha256: &'a str,
        /// The identity of the answer schema the call named.
        pub schema_sha256: &'a str,
        /// Ordered role/block shape only; unknown on an unrepresentable count or future role.
        pub prompt_shape_sha256: Option<&'a str>,
        /// Number of messages in the admitted shape; no dynamic prompt bytes.
        pub prompt_messages: Option<u64>,
        /// Number of Text blocks in the admitted shape, including empty blocks.
        pub prompt_text_blocks: Option<u64>,
        /// Number of non-Text blocks; their kind and content are not observed.
        pub prompt_other_blocks: Option<u64>,
        /// UTF-8 Text bytes in the admitted shape.
        pub prompt_text_bytes: Option<u64>,
        /// Existing identity of the first System message's concatenated Text.
        pub instruction_sha256: &'a str,
        /// None without a response, false for unreported usage, true even for reported zero.
        pub usage_reported: Option<bool>,
        /// Reported input tokens; absent when usage was not reported.
        pub input_tokens: Option<u64>,
        /// Reported output tokens; absent when usage was not reported.
        pub output_tokens: Option<u64>,
        /// Reported reasoning tokens; no default for an absent meter.
        pub reasoning_tokens: Option<u64>,
        /// The same returned response's model, borrowed; the host must admit this metadata.
        pub response_model: Option<&'a str>,
        /// Closed stop projection; future variants are unknown, never Debug formatted.
        pub stop_reason_kind: Option<&'static str>,
        /// Only Unknown's provider detail, borrowed; the host must admit this metadata.
        pub stop_reason_detail: Option<&'a str>,
        /// What came back.
        pub answered: Answered<'a>,
    }

    /// What one call returned: its text blocks, or why it returned nothing.
    #[non_exhaustive]
    pub enum Answered<'a> {
        /// Every text block in order, exactly as received (a response with no text block holds
        /// none here: that is not an empty text), how many blocks of other kinds it held, and a
        /// private identity over the blocks framed by their lengths (distinct from the public
        /// digest of their concatenation).
        Text {
            blocks: TextBlocks<'a>,
            other_blocks: usize,
            framed_sha256: String,
        },
        /// No response came back: never an empty text, never a proof of zero spend.
        NoResponse(Failure),
    }

    /// An opaque borrowed view of returned Text only; no block contents can be constructed,
    /// serialized or debug-printed through this type. Iteration allocates nothing.
    pub struct TextBlocks<'a> {
        content: &'a [ContentBlock],
    }

    impl TextBlocks<'_> {
        /// Text in response order, including empty blocks, without copying payload bytes.
        pub fn iter(&self) -> impl Iterator<Item = &str> + Clone {
            self.content.iter().filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
        }

        /// Number of Text blocks, determined by a bounded-state scan of the borrowed slice.
        #[must_use]
        pub fn len(&self) -> usize {
            self.iter().count()
        }

        /// Whether the response contained no Text block (an empty Text is not absent).
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.iter().next().is_none()
        }
    }

    /// Why a call returned no response.
    #[non_exhaustive]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Failure {
        /// The call's deadline passed.
        Timeout,
        /// The local admission refused it.
        AdmissionRefused,
        /// The provider failed it.
        ProviderError,
    }

    struct Scope {
        sink: Sink,
        ordinal: AtomicU32,
    }

    tokio::task_local! {
        /// The observer of the compile being polled.
        static SCOPE: Scope;
    }

    /// Run `future` with `sink` observing its authoring calls. A scope never polled, or dropped,
    /// observes nothing more; two scopes never share an observation or an ordinal.
    ///
    /// CANCEL SAFETY: cancel-safe; dropping it drops the scope with nothing left behind.
    pub async fn observe_authoring<F: Future>(sink: Sink, future: F) -> F::Output {
        let scope = Scope {
            sink,
            ordinal: AtomicU32::new(0),
        };
        SCOPE.scope(scope, future).await
    }

    // A fmt sink retains only hash state. Legacy Role Debug and decimal framing stay exact;
    // no dynamic request/response DTO is serialized or Debug formatted here.
    struct HashWriter(Sha256);

    impl std::fmt::Write for HashWriter {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            self.0.update(text.as_bytes());
            Ok(())
        }
    }

    pub(super) struct Prompt {
        legacy: String,
        shape: Option<PromptShape>,
    }

    struct PromptShape {
        sha256: String,
        messages: u64,
        text_blocks: u64,
        other_blocks: u64,
        text_bytes: u64,
    }

    /// The prompt's identity, computed only inside a scope (the ordered messages, each framed
    /// by its role and its text's length). Hash updates preserve the legacy byte stream exactly.
    pub(super) fn prompt(messages: &[Message]) -> Option<Prompt> {
        SCOPE.try_with(|_| ()).ok()?;
        let mut framed = HashWriter(Sha256::new());
        for message in messages {
            let blocks = TextBlocks {
                content: &message.content,
            };
            let bytes = blocks
                .iter()
                .try_fold(0usize, |sum, text| sum.checked_add(text.len()))?;
            let _ = write!(framed, "{:?}:{bytes}:", message.role);
            for text in blocks.iter() {
                framed.0.update(text.as_bytes());
            }
        }
        Some(Prompt {
            legacy: format!("{:x}", framed.0.finalize()),
            shape: prompt_shape(messages),
        })
    }

    // v1 role bytes are ASCII S=0x53, U=0x55, A=0x41, T=0x54, independent of Debug.
    // A future role or any u64 conversion/addition overflow withholds the entire shape.
    fn prompt_shape(messages: &[Message]) -> Option<PromptShape> {
        let count = u64::try_from(messages.len()).ok()?;
        let mut hash = Sha256::new();
        hash.update(b"nika-authoring-prompt-shape-v1\0");
        hash.update(count.to_be_bytes());
        let (mut text_blocks, mut other_blocks, mut text_bytes) = (0u64, 0u64, 0u64);
        for message in messages {
            let role = match message.role {
                Role::System => b'S',
                Role::User => b'U',
                Role::Assistant => b'A',
                Role::Tool => b'T',
                _ => return None,
            };
            hash.update([role]);
            hash.update(u64::try_from(message.content.len()).ok()?.to_be_bytes());
            for block in &message.content {
                if let ContentBlock::Text { text } = block {
                    let bytes = u64::try_from(text.len()).ok()?;
                    text_blocks = text_blocks.checked_add(1)?;
                    text_bytes = text_bytes.checked_add(bytes)?;
                    hash.update(b"T");
                    hash.update(bytes.to_be_bytes());
                } else {
                    other_blocks = other_blocks.checked_add(1)?;
                    hash.update(b"O");
                }
            }
        }
        Some(PromptShape {
            sha256: format!("{:x}", hash.finalize()),
            messages: count,
            text_blocks,
            other_blocks,
            text_bytes,
        })
    }

    fn stop(reason: &StopReason) -> (&'static str, Option<&str>) {
        match reason {
            StopReason::EndTurn => ("end_turn", None),
            StopReason::MaxTokens => ("max_tokens", None),
            StopReason::StopSequence => ("stop_sequence", None),
            StopReason::ToolUse => ("tool_use", None),
            StopReason::ContentFilter => ("content_filter", None),
            StopReason::Unknown(detail) => ("unknown", Some(detail.as_str())),
            _ => ("unknown", None),
        }
    }

    /// What a call's result is to an observer: the response, or why none came back.
    pub(super) fn answered(
        result: &Result<Result<InferResponse, ProviderError>, tokio::time::error::Elapsed>,
    ) -> Result<&InferResponse, Failure> {
        match result {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(ProviderError::AdmissionDenied { .. })) => Err(Failure::AdmissionRefused),
            Ok(Err(_)) => Err(Failure::ProviderError),
            Err(_) => Err(Failure::Timeout),
        }
    }

    /// One call's observation to the scope this task runs in, if any; a judge's never.
    pub(super) fn emit(
        role: &'static str,
        prompt: Option<&Prompt>,
        schema_sha256: &str,
        instruction_sha256: &str,
        answered: Result<&InferResponse, Failure>,
    ) {
        let Some(prompt) = prompt.filter(|_| !role.starts_with("judge")) else {
            return;
        };
        let response = answered.as_ref().ok().copied();
        let usage = response.filter(|r| r.usage_reported).map(|r| &r.usage);
        let stopped = response.map(|r| stop(&r.stop_reason));
        let shape = prompt.shape.as_ref();
        let answered = match answered {
            Ok(response) => {
                let blocks = TextBlocks {
                    content: &response.content,
                };
                let mut framed = HashWriter(Sha256::new());
                for text in blocks.iter() {
                    let _ = write!(framed, "{}:", text.len());
                    framed.0.update(text.as_bytes());
                }
                Answered::Text {
                    other_blocks: response.content.len() - blocks.len(),
                    framed_sha256: format!("{:x}", framed.0.finalize()),
                    blocks,
                }
            }
            Err(failure) => Answered::NoResponse(failure),
        };
        let _ = SCOPE.try_with(|scope| {
            let ordinal = scope.ordinal.fetch_add(1, Ordering::SeqCst) + 1;
            (scope.sink)(&AuthoringObservation {
                ordinal,
                role,
                prompt_sha256: &prompt.legacy,
                schema_sha256,
                prompt_shape_sha256: shape.map(|s| s.sha256.as_str()),
                prompt_messages: shape.map(|s| s.messages),
                prompt_text_blocks: shape.map(|s| s.text_blocks),
                prompt_other_blocks: shape.map(|s| s.other_blocks),
                prompt_text_bytes: shape.map(|s| s.text_bytes),
                instruction_sha256,
                usage_reported: response.map(|r| r.usage_reported),
                input_tokens: usage.map(|u| u.input_tokens),
                output_tokens: usage.map(|u| u.output_tokens),
                reasoning_tokens: usage.and_then(|u| u.reasoning_tokens),
                response_model: response.and_then(|r| r.gen_ai.response_model.as_deref()),
                stop_reason_kind: stopped.map(|s| s.0),
                stop_reason_detail: stopped.and_then(|s| s.1),
                answered,
            });
        });
    }
}

/// The provider level an authoring level names (R4 B16): the same word, or `None` for a level
/// the provider seam does not know, which no call may silently drop.
pub(crate) fn effort(reasoning: AuthoringReasoning) -> Option<ReasoningEffort> {
    ReasoningEffort::parse(reasoning.word())
}

/// One call's reasoning, each fact apart (R4 B16): the level the policy configured, the keys the
/// adapter read back from the body it dispatched (`unobserved` when it reports none, or when no
/// response came), the effort the provider served internally (never observable here), the
/// reasoning tokens it reported (null when unreported) and the model it named.
pub(crate) fn reasoning_record(
    configured: Option<AuthoringReasoning>,
    response: Option<&InferResponse>,
) -> Value {
    let transmitted = response
        .and_then(|r| r.reasoning_wire.as_ref())
        .map_or_else(
            || json!("unobserved"),
            |wire| json!({"thinking": wire.thinking, "effort": wire.effort}),
        );
    json!({
        "configured": configured.map(AuthoringReasoning::word),
        "transmitted": transmitted,
        "served": "unknown",
        "reasoning_tokens": response
            .filter(|r| r.usage_reported)
            .and_then(|r| r.usage.reasoning_tokens),
        "response_model": response.and_then(|r| r.gen_ai.response_model.clone()),
    })
}

/// One bounded call under any answer schema (the plan's, the transform's), accounted in the
/// outcome's receipt: the raw response, or None with the finding recorded. Never retries.
pub(super) async fn call_with_schema<P: ProviderInferDyn>(
    policy: &AuthoringPolicy,
    provider: &P,
    role: &'static str,
    messages: Vec<Message>,
    schema: Value,
    out: &mut CompileOutcome,
) -> Option<InferResponse> {
    let entry = context_entry(role, &messages, &schema);
    let prompt = observe::prompt(&messages);
    let identity = |key: &str| entry[key].as_str().unwrap_or_default().to_owned();
    let schema_sha256 = identity("schema_sha256");
    let instruction_sha256 = identity("instruction_sha256");
    let Some(request) = authoring_request(policy, messages, schema) else {
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "authoring_provider",
            "The configured reasoning effort has no provider level; no request was sent.",
        );
        return None;
    };
    out.provenance.cognition = AuthoringCognition::ExplicitProvider;
    let receipt = out
        .provenance
        .authoring
        .get_or_insert_with(|| AuthoringReceipt::new(policy.model.clone()));
    receipt.calls += 1;
    receipt.context.push(entry);
    if let Some(context) = receipt.context.last_mut() {
        context["max_output_tokens"] = json!(policy.max_tokens);
        context["timeout_ms"] = json!(policy.timeout.as_millis());
    }
    let start = std::time::Instant::now();
    let result = tokio::time::timeout(policy.timeout, provider.infer(request)).await;
    // The answer as received, before any decode, to a host's scope (a request never built above
    // made no call and is not observed).
    observe::emit(
        role,
        prompt.as_ref(),
        &schema_sha256,
        &instruction_sha256,
        observe::answered(&result),
    );
    if let Some(receipt) = out.provenance.authoring.as_mut() {
        let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        receipt.elapsed_ms = receipt.elapsed_ms.saturating_add(elapsed_ms);
        if let Some(context) = receipt.context.last_mut() {
            context["elapsed_ms"] = json!(elapsed_ms);
            context["result"] = match &result {
                Ok(Ok(response)) => json!({
                    "stop_reason": format!("{:?}", response.stop_reason),
                    "usage_reported": response.usage_reported,
                    "input_tokens": response.usage_reported.then_some(response.usage.input_tokens),
                    "output_tokens": response.usage_reported.then_some(response.usage.output_tokens),
                }),
                Ok(Err(ProviderError::AdmissionDenied { .. })) => {
                    json!({"failure_kind": "admission_refused"})
                }
                Ok(Err(_)) => json!({"failure_kind": "provider_error"}),
                Err(_) => json!({"failure_kind": "timeout"}),
            };
            let answered = result.as_ref().ok().and_then(|r| r.as_ref().ok());
            context["reasoning"] = reasoning_record(policy.reasoning, answered);
            context["response"] = answered.map_or(Value::Null, response_identity);
        }
    }
    let response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            // A local refusal says why in its own words; the kernel's prefix names another door.
            let message = match error {
                ProviderError::AdmissionDenied { reason } => reason,
                other => other.to_string(),
            };
            crate::finding(out, DiagnosticKind::Unknown, "authoring_provider", message);
            return None;
        }
        Err(_) => {
            crate::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_provider",
                "An authorized authoring call timed out. No retry occurred.",
            );
            return None;
        }
    };
    // The totals keep every usage a call reported; whether they are complete is read from the
    // calls' own results (`authority::usage_complete`), never assumed from a partial sum.
    if let Some(receipt) = out.provenance.authoring.as_mut()
        && response.usage_reported
    {
        receipt.input_tokens =
            Some(receipt.input_tokens.unwrap_or(0) + response.usage.input_tokens);
        receipt.output_tokens =
            Some(receipt.output_tokens.unwrap_or(0) + response.usage.output_tokens);
    }
    Some(response)
}

/// The identity of what one answered call returned (its text blocks, by digest and length, and
/// how many blocks of any kind it held), so a later decode, refusal or repair names the bytes it
/// read. A call that returned nothing records `null`, never an empty answer.
fn response_identity(response: &InferResponse) -> Value {
    let text: String = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    json!({
        "sha256": super::knowledge::sha256(&text),
        "bytes": text.len(),
        "blocks": response.content.len(),
    })
}

/// The semantic object a call's answer proposed, on that call's journal entry. An object that
/// decoded as the door's closed shape (`known` are its keys) is kept exactly; a refused one is
/// [`withheld`]: a model's arbitrary text never reaches the shared record. Data for forensics,
/// never read back as a plan.
pub(super) fn record_proposed(
    out: &mut CompileOutcome,
    object: &str,
    decoded: bool,
    known: &[&str],
) {
    if let Some(call) = out
        .provenance
        .authoring
        .as_mut()
        .and_then(|receipt| receipt.context.last_mut())
    {
        call["proposed"] = if decoded {
            json!({
                "decoded": true,
                "sha256": super::knowledge::sha256(object),
                "object": serde_json::from_str::<Value>(object).ok(),
            })
        } else {
            let mut kept = withheld(object, known, "not the door's closed shape; never read");
            kept["decoded"] = json!(false);
            kept
        };
    }
}

/// The keys of the closed plan shape (`assets/plan_schema.json`): a refused proposal keeps only
/// these names, by shape.
pub(super) const PLAN_KEYS: &[&str] = &[
    "steps",
    "effects",
    "obligations",
    "constraints",
    "unknowns",
    "regions",
    "approval_bypass",
];

/// A refused or ignored model payload as a record keeps it: its digest and length, its shape (the
/// JSON type, the door's own `known` keys it carries and how many other keys) and the reason it
/// was not used — never its text, which may echo anything the model was shown.
pub(super) fn withheld(text: &str, known: &[&str], reason: &str) -> Value {
    let shape = match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => {
            let mut keys: Vec<&str> = known
                .iter()
                .copied()
                .filter(|key| map.contains_key(*key))
                .collect();
            keys.sort_unstable();
            json!({"type": "object", "known_keys": keys, "other_keys": map.len() - keys.len()})
        }
        Ok(Value::Array(items)) => json!({"type": "array", "items": items.len()}),
        Ok(_) => json!({"type": "scalar"}),
        Err(_) => json!({"type": "not_json"}),
    };
    json!({
        "withheld": true,
        "sha256": super::knowledge::sha256(text),
        "bytes": text.len(),
        "shape": shape,
        "reason": reason,
    })
}

/// The references a call's messages actually carried, on the journal entry of the call made
/// after `before` entries (R4 A11, E36): a call that was never journaled is left alone.
pub(super) fn stamp_references(out: &mut CompileOutcome, before: usize, receipts: &Value) {
    if let Some(receipt) = out.provenance.authoring.as_mut()
        && receipt.context.len() > before
        && let Some(entry) = receipt.context.last_mut()
    {
        entry["references"] = receipts.clone();
    }
}

/// The number of journaled calls so far.
pub(super) fn journaled(out: &CompileOutcome) -> usize {
    out.provenance
        .authoring
        .as_ref()
        .map_or(0, |receipt| receipt.context.len())
}

/// What one call received: its role, the sha256 of its instruction (the system message)
/// and of its answer schema, the bytes of its messages, and the references sent with it.
fn context_entry(role: &str, messages: &[Message], schema: &Value) -> Value {
    let sha = super::knowledge::sha256;
    let text_of = |m: &Message| -> String {
        m.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    };
    let instruction = messages
        .iter()
        .find(|m| matches!(m.role, Role::System))
        .map(text_of)
        .unwrap_or_default();
    let bytes: usize = messages.iter().map(|m| text_of(m).len()).sum();
    json!({
        "call": role,
        "instruction_sha256": sha(&instruction),
        "schema_sha256": sha(&schema.to_string()),
        "message_bytes": bytes,
        "references": [],
    })
}

/// The bounded JSON-schema request every authoring call makes, whatever its messages, with the
/// policy's explicit reasoning effort; `None` when that effort has no provider level.
fn authoring_request(
    policy: &AuthoringPolicy,
    messages: Vec<Message>,
    schema: Value,
) -> Option<InferRequest> {
    let mut infer = InferRequest::new(&policy.model, messages);
    infer.max_tokens = Some(policy.max_tokens);
    infer.timeout = Some(policy.timeout);
    infer.response_format = ResponseFormat::JsonSchema(schema);
    if let Some(reasoning) = policy.reasoning {
        infer.reasoning_effort = Some(effort(reasoning)?);
    }
    Some(infer)
}

#[cfg(test)]
mod tests {
    use super::{PLAN_KEYS, withheld};
    use serde_json::{Value, json};

    #[test]
    fn the_plan_keys_are_the_closed_schema_keys() {
        let schema: Value =
            serde_json::from_str(include_str!("../../assets/plan_schema.json")).unwrap();
        let mut keys: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut known = PLAN_KEYS.to_vec();
        known.sort_unstable();
        assert_eq!(keys, known);
    }

    #[test]
    fn a_withheld_payload_keeps_its_digest_and_shape_never_its_text() {
        let canary = "sk-withheld-canary-42";
        for text in [
            json!({"steps": [], "api_key": canary}).to_string(),
            json!({canary: 1}).to_string(),
            json!([canary, canary]).to_string(),
            format!("\"{canary}\""),
            format!("not json {canary}"),
        ] {
            let kept = withheld(&text, PLAN_KEYS, "refused");
            assert!(!kept.to_string().contains(canary), "{kept}");
            assert_eq!(kept["sha256"], crate::cognition::knowledge::sha256(&text));
            assert_eq!(kept["bytes"], text.len());
            assert_eq!(kept["withheld"], true);
        }
        let kept = withheld(
            &json!({"steps": [], "api_key": 1}).to_string(),
            PLAN_KEYS,
            "r",
        );
        assert_eq!(
            kept["shape"],
            json!({"type": "object", "known_keys": ["steps"], "other_keys": 1})
        );
    }
}
