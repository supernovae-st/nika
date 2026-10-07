// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a host may observe of its own compile's authoring calls (slice C): each answer exactly as
//! the compiler received it at `compiler_provider_response`, before any decode, delivered to a
//! sink the host scopes around its compile future. Authoring and repair calls only (a judge's
//! choice is never observed), the prompt by identity only, no request or response serialized,
//! nothing stored here: persistence, caps and redaction are the host's. Received is not
//! persisted, and an observation claims no decode disposition. The sink is trusted host code
//! that cannot change a candidate, an authority, a call count or a public record; it must not
//! panic. Nothing outlives the process: no crash durability is promised.

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

/// Where one authoring call stands, as it happens.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallState {
    /// The request is about to leave.
    Started,
    /// It returned: an answer, a provider failure or a timeout alike (the observation of
    /// the answer says which).
    Finished,
    /// The compile was dropped while it was in flight.
    Cancelled,
}

/// One authoring call's activity: no prompt, response, schema or secret.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct CallActivity<'a> {
    /// 1-based within the activity scope that observed it.
    pub ordinal: u32,
    /// The compiler phase that asked, as [`AuthoringObservation::role`] names it.
    pub role: &'static str,
    /// The model the policy requested, never an identity a response reported.
    pub model: &'a str,
    /// Where the call stands.
    pub state: CallState,
}

/// The host's observer of one activity scope.
pub type ActivitySink = Arc<dyn for<'a, 'b> Fn(&'a CallActivity<'b>) + Send + Sync>;

struct ActivityScope {
    sink: ActivitySink,
    ordinal: AtomicU32,
}

tokio::task_local! {
    /// The activity observer of the compile being polled.
    static ACTIVITY: ActivityScope;
}

/// Run `future` with `sink` told when each of its authoring calls starts, finishes or is
/// cancelled with the compile; independent of [`observe_authoring`], in either order.
///
/// CANCEL SAFETY: cancel-safe; dropping it reports the call in flight as cancelled.
pub async fn observe_activity<F: Future>(sink: ActivitySink, future: F) -> F::Output {
    let scope = ActivityScope {
        sink,
        ordinal: AtomicU32::new(0),
    };
    ACTIVITY.scope(scope, future).await
}

/// One call in flight under an activity scope: started when made, finished when told, and
/// cancelled when dropped first (it holds its own sink, so the report survives the scope).
#[doc(hidden)]
pub struct Activity {
    sink: ActivitySink,
    ordinal: u32,
    role: &'static str,
    model: String,
    finished: bool,
}

impl Activity {
    /// The call `role` asks of `model` starts, when a scope observes it.
    #[must_use]
    pub fn started(role: &'static str, model: &str) -> Option<Self> {
        let (sink, ordinal) = ACTIVITY
            .try_with(|scope| {
                let ordinal = scope.ordinal.fetch_add(1, Ordering::SeqCst);
                (Arc::clone(&scope.sink), ordinal.saturating_add(1))
            })
            .ok()?;
        let model = model.to_owned();
        let call = Self {
            sink,
            ordinal,
            role,
            model,
            finished: false,
        };
        call.tell(CallState::Started);
        Some(call)
    }

    /// The call returned.
    pub fn finish(mut self) {
        self.finished = true;
        self.tell(CallState::Finished);
    }

    fn tell(&self, state: CallState) {
        let (ordinal, role, model) = (self.ordinal, self.role, self.model.as_str());
        (self.sink)(&CallActivity {
            ordinal,
            role,
            model,
            state,
        });
    }
}

impl Drop for Activity {
    fn drop(&mut self) {
        if !self.finished {
            self.tell(CallState::Cancelled);
        }
    }
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

/// A prompt's identity as the observer reads it (its legacy digest and its shape).
#[doc(hidden)]
pub struct Prompt {
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
#[doc(hidden)]
#[must_use]
pub fn prompt(messages: &[Message]) -> Option<Prompt> {
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
#[doc(hidden)]
pub fn answered(
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
#[doc(hidden)]
pub fn emit(
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
