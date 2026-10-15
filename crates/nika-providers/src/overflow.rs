// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A request over the model's context window, told apart without reading the
//! provider's words. A wire with an overflow code answers it typed
//! (`wire::status_error`); a wire without one answers a 400
//! `invalid_request_error`, which is an overflow only when the engine's own
//! estimate of the request reaches the route's known window. Evidence for that
//! codeless 400 (checked 2026-10-12): documented by Anthropic (« prompt is too
//! long », a message, no code) and Mistral (a 400 with no overflow code);
//! observed on `DeepSeek` (type and code `invalid_request_error`). The window is
//! the provider catalog's curated limit for the exact wire model; an unknown
//! window classifies nothing. A success that reports more input than that
//! window holds is marked window-full, for the loop to compact.

use nika_kernel::ai::provider::{ContentBlock, InferRequest, InferResponse, ProviderError};

/// The route's context window in tokens, when the provider catalog lists the
/// exact wire model.
pub(crate) fn route_window(provider: &str, wire_model: &str) -> Option<u64> {
    nika_catalog::find_provider(provider)?
        .models
        .iter()
        .find(|model| model.model == wire_model)
        .map(|model| u64::from(model.context_window_tokens))
}

/// A low estimate of the request's input, in tokens: the UTF-8 bytes of its
/// text, thinking, tool calls, tool results and tool definitions, four bytes a
/// token. Images and opaque reasoning state count nothing, so the estimate errs
/// low and never calls a request the provider could take an overflow.
pub(crate) fn estimate_input_tokens(request: &InferRequest) -> u64 {
    let mut bytes = 0_usize;
    for block in request.messages.iter().flat_map(|message| &message.content) {
        bytes += match block {
            ContentBlock::Text { text } | ContentBlock::Thinking { text } => text.len(),
            ContentBlock::ToolUse { name, input, .. } => name.len() + input.to_string().len(),
            ContentBlock::ToolResult { content, .. } => content.len(),
            _ => 0,
        };
    }
    for tool in &request.tools {
        bytes += tool.name.len() + tool.description.len() + tool.parameters.to_string().len();
    }
    u64::try_from(bytes / 4).unwrap_or(u64::MAX)
}

/// The error a caller sees: a documented overflow gains the route's window when
/// known, and a 400 `invalid_request_error` for a request estimated at the
/// window or above becomes a typed overflow. Everything else passes unchanged.
pub(crate) fn classified(
    error: ProviderError,
    estimate: u64,
    window: Option<u64>,
) -> ProviderError {
    match error {
        ProviderError::ContextOverflow {
            limit: None,
            details,
        } => ProviderError::ContextOverflow {
            limit: window,
            details,
        },
        ProviderError::HttpResponse { details }
            if details.is_invalid_request() && window.is_some_and(|window| estimate >= window) =>
        {
            ProviderError::ContextOverflow {
                limit: window,
                details: Some(details),
            }
        }
        other => other,
    }
}

/// A success that reports more input than the route's window holds: the
/// provider took less than it was sent without saying so. The marker tells the
/// loop to compact; the stop stays what the provider said.
pub(crate) fn mark_silent_overflow(response: &mut InferResponse, window: Option<u64>) {
    if let Some(window) = window
        && response.usage_reported
        && response.usage.input_tokens > window
    {
        response.window_full = true;
    }
}

#[cfg(test)]
mod tests;
