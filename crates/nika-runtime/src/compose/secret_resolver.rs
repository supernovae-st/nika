// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Injected production resolver for authored `secrets:` stores.

use crate::{SecretResolveError, WorkflowSecretResolver};
use nika_schema::types::{SecretRef, SecretSource};

/// The production workflow-`secrets:` resolver — the `env` + `file` stores
/// (MINOR-B). This is the composition root's sanctioned secret-store
/// boundary; the runtime core never reads env/files itself.
///
/// A resolved value is returned directly to the in-memory `secrets`
/// namespace and is never logged here.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct EnvFileSecretResolver;

impl WorkflowSecretResolver for EnvFileSecretResolver {
    fn resolve(&self, name: &str, reference: &SecretRef) -> Result<String, SecretResolveError> {
        let miss = |reason: &str| SecretResolveError {
            name: name.to_owned(),
            reason: reason.to_owned(),
        };
        match reference.source {
            SecretSource::Env => {
                #[allow(clippy::disallowed_methods)]
                let value = std::env::var(&reference.key)
                    .map_err(|_| miss(&format!("env var `{}` is not set", reference.key)))?;
                if value.is_empty() {
                    return Err(miss(&format!("env var `{}` is empty", reference.key)));
                }
                Ok(value)
            }
            SecretSource::File => {
                let raw = std::fs::read_to_string(&reference.key) // seam-bypass-ok: the injected resolver IS the sanctioned store boundary (MINOR-B · runtime cores never read files)
                    .map_err(|error| {
                        miss(&format!("file `{}` unreadable: {error}", reference.key))
                    })?;
                let value = raw.trim_end_matches(['\n', '\r']).to_owned();
                if value.is_empty() {
                    return Err(miss(&format!("file `{}` is empty", reference.key)));
                }
                Ok(value)
            }
            SecretSource::Vault => Err(miss("`vault` secrets are not yet runtime-resolvable")),
        }
    }
}
