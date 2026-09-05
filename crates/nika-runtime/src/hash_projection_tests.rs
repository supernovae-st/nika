// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cross-crate tool schema transport: real definitions and wire builders,
//! injected HTTP only. This does not test remote schema enforcement.

#![allow(clippy::expect_used)]

use std::sync::Arc;

use nika_kernel::ai::provider::{InferRequest, Message, ProviderError, ProviderInferDyn, Role};
use nika_kernel::secret::Secret;
use nika_kernel_mock::MockHttp;
use nika_providers::{ProviderRegistry, ProvidersConfig};
use serde_json::Value;

#[tokio::test]
async fn hash_schema_reaches_each_wire_unchanged() {
    let tool = nika_builtin::tool_defs()
        .into_iter()
        .find(|tool| tool.name == "nika:hash")
        .expect("hash definition");
    for (provider, model, pointer) in [
        ("anthropic", "anthropic/sonnet", "/tools/0/input_schema"),
        (
            "openai",
            "openai/gpt-4.1-mini",
            "/tools/0/function/parameters",
        ),
        (
            "gemini",
            "gemini/flash-25",
            "/tools/0/functionDeclarations/0/parametersJsonSchema",
        ),
    ] {
        let http =
            Arc::new(MockHttp::new().enqueue_ok(400, r#"{"error":{"message":"capture only"}}"#));
        let registry = ProviderRegistry::new(
            Arc::clone(&http),
            ProvidersConfig::new().with_key(provider, Secret::new("test-key")),
        );
        let resolved = registry.resolve(model).expect("in-memory provider");
        let mut request = InferRequest::new(model, vec![Message::text(Role::User, "hash")]);
        request.tools = vec![tool.clone()];
        // The fixture deliberately refuses the response. Only the emitted
        // request is evidence; no model, network, retry or secret store runs.
        let response = resolved.infer(request).await;
        assert!(matches!(
            response,
            Err(ProviderError::Api { status: 400, .. })
        ));
        let requests = http.sent_requests();
        assert_eq!(requests.len(), 1);
        let body: Value = serde_json::from_slice(requests[0].body.as_ref().expect("request body"))
            .expect("JSON request");
        assert_eq!(body.pointer(pointer), Some(&tool.parameters), "{provider}");
        if provider == "gemini" {
            assert!(
                body["tools"][0]["functionDeclarations"][0]
                    .get("parameters")
                    .is_none()
            );
        }
    }
}
