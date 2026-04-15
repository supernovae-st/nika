// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Null secret resolver — returns the reference string as the secret.

use nika_error::NikaError;
use nika_kernel::secret::{Secret, SecretRef, SecretResolver};

/// No-op secret resolver that returns the reference string as the value.
#[derive(Clone, Debug, Default)]
pub struct NullSecretResolver;

impl NullSecretResolver {
    /// Create a new null secret resolver.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl nika_kernel::sealed::Sealed for NullSecretResolver {}

impl SecretResolver for NullSecretResolver {
    async fn resolve(&self, secret: &SecretRef) -> Result<Secret, NikaError> {
        Ok(Secret::new(&secret.reference))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn resolve_returns_reference() {
        let resolver = NullSecretResolver::new();
        let secret = resolver
            .resolve(&SecretRef::new("$env.API_KEY"))
            .await
            .unwrap();
        assert_eq!(secret.expose(), "$env.API_KEY");
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn null_secret_resolver_is_send_sync() {
        _assert_send_sync::<NullSecretResolver>();
    }
}
