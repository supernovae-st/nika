// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Credential-safe projection of provider failures for authoring consumers.
//! Local admission retains its typed distinction; remote text never escapes.

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
