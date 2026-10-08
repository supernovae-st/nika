// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The audited Claude ACP completion profile, never a generic agent permission.
use futures_core::Stream;
use nika_kernel::ai::harness::{AgentBackendDyn, HarnessError, HarnessEvent, HarnessRequest};
use serde_json::{Value, json};
use std::{fmt::Write as _, pin::Pin};

fn safe_error(error: &HarnessError) -> String {
    // Neither adapter stderr nor JSON-RPC error text is a safe user-facing diagnostic. A lost
    // sign-in is read from the adapter's typed kind, and told in our own words.
    match error {
        HarnessError::Unavailable { reason } if reason == crate::client::SIGN_IN_EXPIRED => {
            "ACP authoring unavailable: the app's sign-in has expired or was revoked; sign in again in that app, then retry; no fallback".into()
        }
        HarnessError::Unavailable { .. } => "ACP authoring unavailable: check the installed adapter, version and account; no fallback".into(),
        HarnessError::Refused { .. } => "ACP authoring refused: the audited claude-agent-acp 0.81.1 profile, selected model or text-only contract was not satisfied; no answer accepted".into(),
        _ => "ACP authoring transport ended before a complete answer; no answer accepted".into(),
    }
}

pub(crate) const MAX_ANSWER: usize = 512 * 1024;
const NAME: &str = "@agentclientprotocol/claude-agent-acp";
const VERSION: &str = "0.81.1";

pub(crate) fn refusal(reason: &str) -> HarnessError {
    HarnessError::Refused {
        reason: reason.into(),
    }
}

/// Exact local admission, checked on the active connection before session/new.
/// A future version requires a renewed contract test; a name alone grants nothing.
pub(crate) fn admit(init: &Value) -> Result<(), HarnessError> {
    if init.pointer("/agentInfo/name").and_then(Value::as_str) != Some(NAME)
        || init.pointer("/agentInfo/version").and_then(Value::as_str) != Some(VERSION)
        || init.get("protocolVersion").and_then(Value::as_u64) != Some(1)
    {
        return Err(refusal(
            "ACP authoring requires the audited claude-agent-acp 0.81.1 profile; no native or API fallback",
        ));
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
pub(crate) fn judge_update(update: &Value) -> Result<(), HarnessError> {
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
        _ => Err(refusal(
            "ACP authoring emitted a tool, media or unsupported event; no answer accepted",
        )),
    }
}

pub(crate) fn descriptor(
    adapter: &str,
    requested: Option<&str>,
    forwarded: &str,
    observed: &[Value],
) -> Value {
    json!({"kind":"harness_infer", "transport":"acp", "adapter":adapter, "requested_model":requested,
        "forwarded_model":forwarded, "observed":observed,
        "cost_basis":"subscription-backed/unknown", "billed_cost_usd":null,
        "numeric_usage_reported":false, "tools_exposed":"none; audited empty tools and strict empty MCP",
        "context_exposed":"compiler messages only; fresh isolated scratch; wrapped ACP prompt",
        "token_ceiling":"requested by Compiler; ACP does not enforce token cap",
        "bounds":"one prompt per call; SDK maxTurns 1; the call's own deadline; 512 KiB answer",
        "schema":"schema included in request; whole returned text judged by Compiler",
        "served_model":null, "adapter_version":VERSION})
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

pub(crate) async fn run(
    seat: &crate::SpawnedHarness,
    native: crate::HarnessInferRequest,
    requested: Option<&str>,
) -> Result<(String, Value), String> {
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let request = session_request(native, requested, scratch.path());
    let mut stream = seat.run_agent(request).await.map_err(|e| safe_error(&e))?;
    while let Some(event) = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
        match event.map_err(|e| safe_error(&e))? {
            HarnessEvent::MessageChunk { .. } => {}
            HarnessEvent::Completed { outcome } if outcome.images.is_empty() => {
                let selection = &outcome.selection;
                let metadata = json!({"status":"returned", "configured_model":outcome.observed_model,
                    "model_evidence":outcome.observed_model_source.map(nika_kernel::ai::harness::ModelProvenance::as_str),
                    "effort_option":selection.effort_option, "transmitted_effort":selection.transmitted_effort,
                    "configured_effort":selection.configured_effort,
                    "served_model":null, "usage_observed":outcome.usage.is_some(), "attested_version":VERSION});
                return Ok((outcome.output, metadata));
            }
            HarnessEvent::PermissionAsked { reply, .. } => {
                reply.respond(nika_kernel::ai::harness::PermissionDecision::Deny);
                return Err("ACP authoring requested a tool; no answer accepted".into());
            }
            _ => {
                return Err(
                    "ACP authoring returned an unsupported event; no answer accepted".into(),
                );
            }
        }
    }
    Err("ACP authoring ended without a completed answer".into())
}

#[cfg(test)]
mod tests;
