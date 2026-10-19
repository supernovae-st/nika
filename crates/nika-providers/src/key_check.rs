// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A key check a person asks for (« Test the key »): whether the provider accepts the key a run
//! would send, on one free request that runs no model. Nothing calls it on its own (no probe at
//! startup, no retry): only an explicit caller sends the one request.
//!
//! The request is a listing that authenticates the key, on the endpoint a run would use: Anthropic's
//! model list (an operator's `NIKA_ANTHROPIC_BASE_URL` honored) with the `x-api-key` and
//! `anthropic-version` headers; on their own endpoints only, the model list of `OpenAI`, xAI,
//! `DeepSeek`, Mistral and Groq with a bearer key, `OpenRouter`'s key endpoint (its model list is
//! served to anyone) and Gemini's model list with its `x-goog-api-key` header. Listing consumes
//! no token. No redirect is followed, and the key rides that one header of that one request: it
//! is never logged, returned or kept, and a refusal's words are reduced as every provider error
//! is, the key withheld. Live gauntlet r02, 2026-10-16: « Test the key » on an invalid `OpenAI`
//! key answered « unknown », and the 401 came only with the first message.

use std::time::Duration;

use nika_kernel::http::{HttpGetDyn, HttpRequest};

use crate::{ProviderRegistry, ProvidersConfig};

/// How a key check sends the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyAuth {
    /// Anthropic's `x-api-key` and `anthropic-version` headers.
    Anthropic,
    /// `Authorization: Bearer <key>`.
    Bearer,
    /// Gemini's `x-goog-api-key` header.
    Goog,
}

/// What one key check observed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyCheck {
    /// The provider listed its models with the key (HTTP 200).
    Valid,
    /// The provider refused the key: HTTP 401 or 403, in its own reduced words.
    Refused {
        /// The HTTP status (401 or 403).
        status: u16,
        /// The provider's answer, reduced, the key withheld.
        said: String,
    },
    /// No HTTP answer came back (a connection, name, TLS or deadline failure).
    Unreachable {
        /// What the transport reported.
        why: String,
    },
    /// Nothing judges the key: no key is configured, this route has no free check here, the
    /// transport changed the endpoint, or another status answered.
    Unknown {
        /// Why the check could not judge the key.
        why: String,
    },
}

/// Check `provider`'s key from `config` (`nika_runtime::compose::config_from_env` for the
/// person's environment) with one bounded GET of its model list through `http`, within
/// `deadline`. Anthropic (or its `claude` alias) on its configured Messages endpoint is
/// checked; any other route answers [`KeyCheck::Unknown`] without a request.
pub async fn check_key<H: HttpGetDyn>(
    http: &H,
    config: ProvidersConfig,
    provider: &str,
    deadline: Duration,
) -> KeyCheck {
    let registry = ProviderRegistry::without_http(config);
    let Some((endpoint, key, auth)) = registry.key_check_target(provider) else {
        return KeyCheck::Unknown {
            why: format!(
                "no key is configured for `{provider}`, or its route has no free key check here \
                 (an operator's endpoint, or a provider whose listing proves no key); nothing was \
                 sent"
            ),
        };
    };
    let mut request = HttpRequest::get(&endpoint);
    request.follow_redirects = false;
    request.timeout = Some(deadline);
    match auth {
        KeyAuth::Anthropic => {
            request
                .headers
                .insert("x-api-key".to_owned(), key.expose().to_owned());
            request.headers.insert(
                "anthropic-version".to_owned(),
                crate::wire::anthropic::ANTHROPIC_VERSION.to_owned(),
            );
        }
        KeyAuth::Bearer => {
            request.headers.insert(
                "authorization".to_owned(),
                format!("Bearer {}", key.expose()),
            );
        }
        KeyAuth::Goog => {
            request
                .headers
                .insert("x-goog-api-key".to_owned(), key.expose().to_owned());
        }
    }
    // Gemini refuses an invalid key with HTTP 400 `API_KEY_INVALID`, not 401.
    let refused = |status: u16, body: &[u8]| {
        matches!(status, 401 | 403)
            || (auth == KeyAuth::Goog
                && status == 400
                && String::from_utf8_lossy(body).contains("API_KEY_INVALID"))
    };
    match http.get(request).await {
        Ok(response) if response.final_url != endpoint => KeyCheck::Unknown {
            why: "the transport changed the endpoint; the key was not judged".to_owned(),
        },
        Ok(response) if response.status == 200 => KeyCheck::Valid,
        Ok(response) if refused(response.status, &response.body) => KeyCheck::Refused {
            status: response.status,
            said: said(&crate::wire::status_error(
                response.status,
                &response.body,
                None,
                Some(&key),
            )),
        },
        Ok(response) => KeyCheck::Unknown {
            why: format!(
                "the model list answered HTTP {}; the key was not judged",
                response.status
            ),
        },
        Err(error) => KeyCheck::Unreachable {
            why: error.to_string(),
        },
    }
}

/// What the provider said refusing a key: its own words and its error's kind, never the hint a
/// failed run prints for the command line (live gauntlet wave 6 r02, 2026-10-18: a key check
/// answered « NIKA-INFER-001 … `--access <seat>` » to a person picking a connection).
fn said(error: &nika_kernel::ai::provider::ProviderError) -> String {
    let nika_kernel::ai::provider::ProviderError::HttpResponse { details } = error else {
        return error.to_string();
    };
    match (details.message(), details.error_type().or(details.code())) {
        (Some(message), Some(kind)) => format!("{message} ({kind})"),
        (Some(message), None) => message.to_owned(),
        (None, Some(kind)) => kind.to_owned(),
        (None, None) => "the provider refused this key".to_owned(),
    }
}

#[cfg(test)]
mod tests;
