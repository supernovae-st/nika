// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `session/prompt` content: the text block, then one ACP image block per image the
//! request carries (an `infer:` task's `vision:` files). An image rides only to an agent
//! whose `initialize` advertises image prompts; any other refuses before the session.

use nika_kernel::ai::harness::{HarnessError, HarnessPromptImage};
use serde_json::{Value, json};

/// Refuse a request carrying images to an agent that does not advertise image prompts
/// (`agentCapabilities.promptCapabilities.image`, ACP v1). Judged on the `initialize`
/// answer, before `session/new`: nothing of the task has been sent yet.
pub(super) fn admit_images(
    initialize: &Value,
    images: &[HarnessPromptImage],
) -> Result<(), HarnessError> {
    if images.is_empty() {
        return Ok(());
    }
    let advertised = initialize
        .pointer("/agentCapabilities/promptCapabilities/image")
        .and_then(Value::as_bool)
        == Some(true);
    if advertised {
        return Ok(());
    }
    let agent = initialize
        .pointer("/agentInfo/name")
        .and_then(Value::as_str)
        .unwrap_or("the agent");
    Err(HarnessError::Refused {
        reason: format!(
            "{agent} does not advertise image prompts (initialize \
             agentCapabilities.promptCapabilities.image), so the task's {} image(s) cannot \
             ride this session; drop `vision:` or declare a route that reads images \
             (nothing was sent)",
            images.len()
        ),
    })
}

/// The `session/prompt` params: the text first, then the images in their declared order.
pub(super) fn params(session_id: &str, text: &str, images: &[HarnessPromptImage]) -> Value {
    let mut blocks = Vec::with_capacity(1 + images.len());
    blocks.push(json!({"type": "text", "text": text}));
    blocks.extend(
        images
            .iter()
            .map(|image| json!({"type": "image", "mimeType": image.mime_type, "data": image.data})),
    );
    json!({"sessionId": session_id, "prompt": blocks})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> HarnessPromptImage {
        HarnessPromptImage::new("image/png", "iVBORw0KGgo=")
    }

    #[test]
    fn images_ride_only_where_the_agent_advertises_image_prompts() {
        let codex = json!({"agentInfo": {"name": "@agentclientprotocol/codex-acp"},
            "agentCapabilities": {"promptCapabilities": {"image": true, "embeddedContext": true}}});
        assert!(admit_images(&codex, &[png()]).is_ok());
        for silent in [
            json!({"agentInfo": {"name": "text-only"}, "agentCapabilities":
                {"promptCapabilities": {"image": false}}}),
            json!({"agentInfo": {"name": "text-only"}}),
            json!({"agentInfo": {"name": "text-only"}, "agentCapabilities":
                {"promptCapabilities": {"image": "yes"}}}),
        ] {
            let refused = admit_images(&silent, &[png()]).expect_err("no image prompts");
            let reason = refused.to_string();
            assert!(
                reason.contains("text-only does not advertise image prompts")
                    && reason.contains("nothing was sent"),
                "{reason}"
            );
            assert!(!refused.is_transient(), "a capability never heals on retry");
        }
        assert!(
            admit_images(&json!({}), &[]).is_ok(),
            "a text prompt needs no capability"
        );
    }

    #[test]
    fn the_prompt_carries_the_text_then_each_image_block() {
        let shot = HarnessPromptImage::new("image/jpeg", "/9j/4AAQ");
        let sent = params("s-1", "describe both", &[png(), shot]);
        assert_eq!(sent["sessionId"], "s-1");
        assert_eq!(
            sent["prompt"],
            json!([
                {"type": "text", "text": "describe both"},
                {"type": "image", "mimeType": "image/png", "data": "iVBORw0KGgo="},
                {"type": "image", "mimeType": "image/jpeg", "data": "/9j/4AAQ"},
            ])
        );
        assert_eq!(
            params("s-2", "hi", &[])["prompt"],
            json!([{"type": "text", "text": "hi"}]),
            "a text-only prompt keeps its one block"
        );
    }
}
