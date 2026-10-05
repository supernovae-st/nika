// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Credential-safe provider configuration and failure projections for authoring consumers.
//! Endpoints omit credentials; local admission retains its typed distinction.

use nika_kernel::http::HttpPostDyn;

/// The configured endpoint's host and optional nondefault port, without user info,
/// path, query or fragment. Bare local host:port values use the same HTTP spelling
/// as the local provider door. This is configuration evidence, not a remote identity.
#[must_use]
pub fn authoring_host(raw: &str) -> Option<String> {
    let raw = if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("http://{raw}")
    };
    let parsed = url::Url::parse(&raw).ok()?;
    let host = parsed.host()?.to_string();
    Some(
        parsed
            .port()
            .map_or(host.clone(), |port| format!("{host}:{port}")),
    )
}

/// The host part of a URL (`https://api.scaleway.ai/v1` → `api.scaleway.ai`).
#[must_use]
pub fn host_of(url: &str) -> String {
    authoring_host(url).unwrap_or_else(|| "unknown endpoint".to_owned())
}

/// The effective host when it differs from the provider profile's host. The caller
/// supplies its HTTP effect and configuration; this projection reads no environment,
/// sends no request and claims no served identity.
#[must_use]
pub fn configured_gateway_host<H: HttpPostDyn + Send + Sync + 'static>(
    http: H,
    config: crate::ProvidersConfig,
    provider: &str,
) -> Option<String> {
    let registry = crate::ProviderRegistry::new(std::sync::Arc::new(http), config);
    let effective = host_of(registry.effective_base_url(provider)?);
    let seed = registry
        .profiles()
        .iter()
        .find(|p| p.id == crate::canonical_provider(provider))
        .map(|p| host_of(p.base_url))?;
    (effective != seed).then_some(effective)
}

/// Describe the exact registry configuration seated by an authoring door, without
/// credentials or a fabricated price. Callers add observed identities and usage.
#[must_use]
pub fn authoring_backend<H: HttpPostDyn + Send + Sync + 'static>(
    registry: &crate::ProviderRegistry<H>,
    model: &str,
) -> serde_json::Value {
    let provider = model.split('/').next().unwrap_or(model);
    let effective = registry.effective_base_url(provider);
    let seed = registry
        .profiles()
        .iter()
        .find(|profile| profile.id == crate::canonical_provider(provider))
        .map(|profile| profile.base_url);
    serde_json::json!({
        "kind": "direct_api", "provider": provider, "requested_model": model,
        "host": effective.and_then(authoring_host),
        "base_url_overridden": effective.zip(seed).map(|(actual, seed)| actual != seed),
        "endpoint_basis": "operator_configuration",
        "cost_basis": "unpriced; billing_unverified",
    })
}

/// Sanitize provider failures at an authoring transport boundary while preserving
/// a typed local admission refusal. Provider text may carry credentials or bodies;
/// `admission_remedy` must be engine-authored, never provider response text.
#[must_use]
pub fn redact_authoring_error(
    error: nika_kernel::ai::provider::ProviderError,
    admission_remedy: &str,
) -> nika_kernel::ai::provider::ProviderError {
    use nika_kernel::ai::provider::ProviderError;
    let reason = match error {
        ProviderError::AdmissionDenied { .. } => return ProviderError::AdmissionDenied {
            reason: admission_remedy.to_owned(),
        },
        ProviderError::HttpResponse { details } => {
            format!("the authoring provider answered HTTP {}", details.status())
        }
        ProviderError::Api { status, .. } => {
            format!("the authoring provider answered HTTP {status}")
        }
        ProviderError::RateLimited { .. } => "the authoring provider rate-limited the call".to_owned(),
        ProviderError::AuthFailed { .. } => "the authoring provider refused the operator's credentials".to_owned(),
        ProviderError::ModelNotFound { .. } => "the authoring provider does not serve the seated model".to_owned(),
        ProviderError::Connection { .. } => "the connection to the authoring provider failed or was cut; the call may still be billed".to_owned(),
        _ => "the authoring provider call failed".to_owned(),
    };
    ProviderError::Other { reason }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_projection_keeps_host_semantics_without_endpoint_secrets() {
        let project = |config, provider| configured_gateway_host(crate::NoHttp, config, provider);
        assert_eq!(project(crate::ProvidersConfig::new(), "openai"), None);
        assert_eq!(
            project(crate::ProvidersConfig::new(), "unknown-provider"),
            None
        );
        let same =
            crate::ProvidersConfig::new().with_base_url("openai", "https://api.openai.com/other");
        assert_eq!(project(same, "openai"), None);
        let gateway = crate::ProvidersConfig::new().with_base_url(
            "openai",
            "https://test-user:test-secret@gateway.invalid:8443/private?key=test-secret",
        );
        assert_eq!(
            project(gateway, "openai").as_deref(),
            Some("gateway.invalid:8443")
        );
        assert_eq!(host_of("http://127.0.0.1:11434/v1"), "127.0.0.1:11434");
        assert_eq!(host_of("https://bad host/private"), "unknown endpoint");
    }

    #[test]
    fn authoring_redaction_keeps_local_refusal_type_without_provider_text() {
        use nika_kernel::ai::provider::ProviderError;
        let denied = redact_authoring_error(
            ProviderError::AdmissionDenied {
                reason: "private-provider-text".to_owned(),
            },
            "authorize max_calls",
        );
        assert!(
            matches!(denied, ProviderError::AdmissionDenied { reason } if reason == "authorize max_calls")
        );
        let failed = redact_authoring_error(
            ProviderError::Other {
                reason: "private-provider-text".to_owned(),
            },
            "authorize max_calls",
        );
        assert!(
            matches!(failed, ProviderError::Other { reason } if reason == "the authoring provider call failed")
        );
    }
}
