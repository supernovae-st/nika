// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The audited ACP completion profiles (Claude Code · Codex), never a generic agent permission.
use futures_core::Stream;
use nika_kernel::ai::harness::{HarnessError, HarnessEvent, HarnessRequest};
use nika_kernel::ai::provider::StopReason;
use serde_json::{Value, json};
use std::{fmt::Write as _, pin::Pin};

mod call;
pub(crate) mod codex;

pub(crate) use call::{Deadline, Door, Failure, Milestone, Opened, Phase, Progress, conclude};

/// The one wire stop a completion profile accepts: the driver refuses every other before the
/// turn closes, so an accepted answer's stop reason is observed (the kernel's canonical
/// `end_turn`), and a failed call has none.
pub(crate) const ACCEPTED_STOP: &str = "end_turn";

fn safe_error(error: &HarnessError) -> String {
    // Neither adapter stderr nor JSON-RPC error text is a safe user-facing diagnostic. A lost
    // sign-in is read from the adapter's typed kind, and told in our own words.
    match error {
        HarnessError::Unavailable { reason } if reason == crate::client::SIGN_IN_EXPIRED => {
            "ACP authoring unavailable: the app's sign-in has expired or was revoked; sign in again in that app, then retry; no fallback".into()
        }
        HarnessError::Unavailable { .. } => "ACP authoring unavailable: check the installed adapter, version and account; no fallback".into(),
        HarnessError::Refused { .. } => "ACP authoring refused: the audited ACP completion profile, selected model or text-only contract was not satisfied; no answer accepted".into(),
        // Nika's own words about the offer (never adapter text): verbatim,
        // so the author sees the exact option, value and discovered offer.
        HarnessError::Selection { reason } => {
            format!("ACP authoring refused: {reason}; no answer accepted")
        }
        _ => "ACP authoring transport ended before a complete answer; no answer accepted".into(),
    }
}

pub(crate) const MAX_ANSWER: usize = 512 * 1024;
const NAME: &str = "@agentclientprotocol/claude-agent-acp";
const VERSION: &str = "0.81.1";

/// Which one-shot the audited profile serves. Only the words of a refusal
/// differ: the identity, options and judgments are the same for both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Completion {
    /// A conversational or Compiler authoring round.
    Authoring,
    /// One Run `infer:` task declared over `run.access.protocol: acp`.
    Infer,
}

impl Completion {
    /// The subject of every refusal this profile speaks.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Authoring => "ACP authoring",
            Self::Infer => "ACP infer",
        }
    }
}

/// Whose audited profile a one-shot runs under: each adapter closes its tool surface its own
/// way before the prompt (Claude Code through its SDK options, Codex through its config,
/// proven at spawn, and a read-only session mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Profile {
    /// claude-agent-acp 0.81.1.
    ClaudeCode,
    /// codex-acp 1.13.1 (codex 0.156.1).
    Codex,
}

impl Profile {
    /// The profile a registry seat carries, when it has one.
    pub(crate) fn for_seat(seat: &str) -> Option<Self> {
        match seat {
            "claude-code" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }

    const fn identity(self) -> (&'static str, &'static str) {
        match self {
            Self::ClaudeCode => (NAME, VERSION),
            Self::Codex => (codex::NAME, codex::VERSION),
        }
    }
}

/// One audited one-shot: the role that asked and the adapter profile it runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OneShot {
    pub(crate) role: Completion,
    pub(crate) profile: Profile,
}

impl OneShot {
    /// The subject of every refusal this one-shot speaks.
    pub(crate) const fn label(self) -> &'static str {
        self.role.label()
    }

    /// The options sent with `session/new` before any prompt, when the profile has any.
    pub(crate) fn session_meta(self) -> Option<Value> {
        match self.profile {
            Profile::ClaudeCode => Some(profile()),
            Profile::Codex => None,
        }
    }

    /// The session mode applied and read back before the prompt, when the profile needs one.
    pub(crate) const fn required_mode(self) -> Option<&'static str> {
        match self.profile {
            Profile::ClaudeCode => None,
            Profile::Codex => Some(codex::MODE),
        }
    }

    /// The exact adapter version the profile was audited on.
    pub(crate) const fn version(self) -> &'static str {
        self.profile.identity().1
    }
}

pub(crate) fn refusal(reason: &str) -> HarnessError {
    HarnessError::Refused {
        reason: reason.into(),
    }
}

/// Exact local admission, checked on the active connection before session/new.
/// A future version requires a renewed contract test; a name alone grants nothing.
pub(crate) fn admit(init: &Value, one_shot: OneShot) -> Result<(), HarnessError> {
    let (name, version) = one_shot.profile.identity();
    if init.pointer("/agentInfo/name").and_then(Value::as_str) != Some(name)
        || init.pointer("/agentInfo/version").and_then(Value::as_str) != Some(version)
        || init.get("protocolVersion").and_then(Value::as_u64) != Some(1)
    {
        return Err(refusal(&format!(
            "{} requires the audited {name} {version} profile; no native or API fallback",
            one_shot.label()
        )));
    }
    Ok(())
}

/// Options passed by this exact adapter to SDK query before any prompt.
/// MCP and disk settings are excluded independently of the empty built-in tools.
pub(crate) fn profile() -> Value {
    json!({"claudeCode":{"options":{
        "tools":[], "mcpServers":{}, "strictMcpConfig":true,
        "settingSources":[], "plugins":[], "skills":[], "agents":{},
        "allowDangerouslySkipPermissions":false, "maxTurns":1, "persistSession":false
    }}})
}

/// No tool/media-bearing answer is accepted, even from an admitted implementation.
/// This is a second check, never the source of pre-execution authority.
pub(crate) fn judge_update(update: &Value, completion: OneShot) -> Result<(), HarnessError> {
    match update.get("sessionUpdate").and_then(Value::as_str) {
        Some("agent_message_chunk")
            if update.pointer("/content/type").and_then(Value::as_str) == Some("text")
                && update
                    .pointer("/content/text")
                    .is_some_and(Value::is_string) =>
        {
            Ok(())
        }
        Some(
            "agent_thought_chunk"
            | "usage_update"
            | "config_option_update"
            | "current_mode_update"
            | "session_info_update"
            | "available_commands_update",
        ) => Ok(()),
        tag => {
            // Name the beat in protocol words only (the update type, an ACP tool kind),
            // never its titles or paths.
            let kind = update.get("kind").and_then(Value::as_str);
            let beat = match (protocol_word(tag), protocol_word(kind)) {
                (Some(tag), Some(kind)) => format!(" (`{tag}` · kind `{kind}`)"),
                (Some(tag), None) => format!(" (`{tag}`)"),
                _ => String::new(),
            };
            Err(refusal(&format!(
                "{} emitted a tool, media or unsupported event{beat}; no answer accepted",
                completion.label()
            )))
        }
    }
}

/// A protocol word (an update type, an ACP tool kind) fit to name in a refusal: never a
/// title, a path or free text.
fn protocol_word(value: Option<&str>) -> Option<&str> {
    value.filter(|word| {
        !word.is_empty()
            && word.len() <= 40
            && word.chars().all(|c| c.is_ascii_lowercase() || c == '_')
    })
}

pub(crate) fn descriptor(
    profile: Profile,
    adapter: &str,
    requested: Option<&str>,
    forwarded: &str,
    observed: &[Value],
) -> Value {
    let (tools, bounds) = match profile {
        Profile::ClaudeCode => (
            "none; audited empty tools and strict empty MCP",
            "one prompt per call; SDK maxTurns 1; the call's own deadline; 512 KiB answer",
        ),
        Profile::Codex => (
            "apply_patch only, confined to the per-call scratch removed after the call (not an empty-tools profile); tool features, plugins and every configured MCP server disabled and read back at spawn; read-only mode: no network, every approval denied; any tool beat refuses the answer",
            "one prompt per call; the call's own deadline; 512 KiB answer",
        ),
    };
    json!({"kind":"harness_infer", "transport":"acp", "adapter":adapter, "requested_model":requested,
        "forwarded_model":forwarded, "observed":observed,
        "cost_basis":"subscription-backed/unknown", "billed_cost_usd":null,
        "numeric_usage_reported":false, "tools_exposed":tools,
        "context_exposed":"compiler messages only; fresh isolated scratch; wrapped ACP prompt",
        "token_ceiling":"requested by Compiler; ACP does not enforce token cap",
        "bounds":bounds,
        "schema":"schema included in request; whole returned text judged by Compiler",
        "served_model":null, "adapter_version":profile.identity().1})
}

/// The session request one completion sends: the wrapped prompt, the selected model when one
/// was asked, and the explicit effort verbatim (the client applies it or refuses before the
/// prompt) — never dropped.
pub(crate) fn session_request(
    native: crate::HarnessInferRequest,
    requested: Option<&str>,
    cwd: &std::path::Path,
) -> HarnessRequest {
    let mut prompt = native.prompt;
    if let Some(schema) = native.schema {
        let _ = write!(
            prompt,
            "\n\nReturn only the complete JSON value matching this schema:\n{schema}"
        );
    }
    let mut request = HarnessRequest::new(prompt, cwd).with_requested_effort(native.effort);
    request.system = native.system;
    if requested.is_some() && native.requested_model != "session" {
        request = request.with_requested_model(native.requested_model);
    }
    request
}

/// A harness error told in the profile's safe words, its typed identity kept beside them.
fn failed(error: &HarnessError) -> Failure {
    Failure::told(safe_error(error), error)
}

/// One authoring completion through `door`, inside the call's `deadline`, each boundary it
/// crosses told to `progress`. The deadline stays the caller's to enforce; nothing is retried.
pub(crate) async fn run(
    door: &dyn Door,
    native: crate::HarnessInferRequest,
    requested: Option<&str>,
    deadline: Deadline,
    progress: &Progress,
) -> Result<(String, Value), Failure> {
    let Some(one_shot) = door.one_shot() else {
        return Err(Failure::refused(
            "ACP authoring requires an audited completion profile; no answer accepted",
        ));
    };
    let scratch = tempfile::tempdir().map_err(|_| {
        let message = "ACP authoring cannot create its scratch directory; no answer accepted";
        Failure::told(
            message,
            &HarnessError::Unavailable {
                reason: message.to_owned(),
            },
        )
    })?;
    let request = session_request(native, requested, scratch.path());
    let Opened {
        mut stream,
        allowance,
    } = (door.open(request, deadline, progress.clone()).await).map_err(|e| failed(&e))?;
    progress.opened(allowance);
    while let Some(event) = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
        match event.map_err(|e| failed(&e))? {
            HarnessEvent::MessageChunk { .. } => {
                progress.reach(Phase::Answer);
                progress.mark(Milestone::AnswerChunk);
            }
            HarnessEvent::Completed { outcome } if outcome.images.is_empty() => {
                progress.reach(Phase::Completion);
                return completed(*outcome, one_shot.version());
            }
            HarnessEvent::PermissionAsked { reply, .. } => {
                reply.respond(nika_kernel::ai::harness::PermissionDecision::Deny);
                return Err(Failure::refused(
                    "ACP authoring requested a tool; no answer accepted",
                ));
            }
            _ => {
                return Err(Failure::refused(
                    "ACP authoring returned an unsupported event; no answer accepted",
                ));
            }
        }
    }
    Err(Failure::ended(
        "ACP authoring ended without a completed answer",
    ))
}

/// A completed turn's answer and its record. An explicit selection is exact: an answer during
/// which the agent moved a model or an effort the client applied was not produced under it, so
/// none is accepted (the access contract's own refusal); a move of a dimension nobody set rides
/// the record, never hidden. The stop reason is the one the profile accepted.
fn completed(
    outcome: nika_kernel::ai::harness::HarnessOutcome,
    attested: &str,
) -> Result<(String, Value), Failure> {
    if let Some(refusal) = outcome.selection.moved_refusal() {
        return Err(Failure::told(refusal.to_string(), &refusal));
    }
    let selection = &outcome.selection;
    let mut metadata = json!({"status":"returned", "configured_model":outcome.observed_model,
        "model_evidence":outcome.observed_model_source.map(nika_kernel::ai::harness::ModelProvenance::as_str),
        "effort_option":selection.effort_option, "transmitted_effort":selection.transmitted_effort,
        "configured_effort":selection.configured_effort,
        "served_model":null, "usage_observed":outcome.usage.is_some(),
        "stop_reason":StopReason::EndTurn, "attested_version":attested});
    if !selection.changed_mid_turn.is_empty()
        && let Some(record) = metadata.as_object_mut()
    {
        record.insert("changed_mid_turn".into(), json!(selection.changed_mid_turn));
    }
    Ok((outcome.output, metadata))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod deadline_tests;
