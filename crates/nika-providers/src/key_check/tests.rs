// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The key check against a scripted listing endpoint: no network, no live provider.
use super::*;
use nika_kernel::http::{HttpError, HttpMethod, HttpResponse};
use nika_kernel::secret::Secret;
use std::{collections::BTreeMap, sync::Mutex};

const KEY: &str = "sk-ant-key-check-fixture-0000000000";
const DEADLINE: Duration = Duration::from_secs(5);

/// A listing endpoint answering one scripted status and body (none: a transport failure),
/// from `moved` when it reports another final endpoint; it records every request.
struct Listing {
    sent: Mutex<Vec<HttpRequest>>,
    answer: Option<(u16, String)>,
    moved: Option<&'static str>,
}

impl Listing {
    fn new(answer: Option<(u16, &str)>) -> Self {
        Self {
            sent: Mutex::new(Vec::new()),
            answer: answer.map(|(status, body)| (status, body.to_owned())),
            moved: None,
        }
    }

    fn sent(&self) -> Vec<HttpRequest> {
        self.sent.lock().expect("requests").clone()
    }
}

impl HttpGetDyn for Listing {
    async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let url = self
            .moved
            .map_or_else(|| request.url.clone(), str::to_owned);
        self.sent.lock().expect("requests").push(request);
        match &self.answer {
            Some((status, body)) => Ok(HttpResponse::new(
                *status,
                BTreeMap::new(),
                body.clone().into_bytes().into(),
                url,
            )),
            None => Err(HttpError::Connection {
                reason: "connection refused".to_owned(),
            }),
        }
    }
}

fn anthropic(base: Option<&str>) -> ProvidersConfig {
    let config = ProvidersConfig::new().with_key("anthropic", Secret::new(KEY));
    match base {
        Some(base) => config.with_base_url("anthropic", base),
        None => config,
    }
}

/// An Anthropic key is checked with one bodyless GET of the Models API on Anthropic's own
/// endpoint, the key in `x-api-key` beside the API version, no redirect, the caller's deadline:
/// a 200 is a valid key.
#[tokio::test]
async fn an_anthropic_key_check_lists_models_once_with_its_own_headers() {
    let body = r#"{"data":[{"id":"claude-sonnet-5-5","type":"model"}],"has_more":false}"#;
    let wire = Listing::new(Some((200, body)));
    let got = check_key(&wire, anthropic(None), "anthropic", DEADLINE).await;
    assert_eq!(got, KeyCheck::Valid);
    let sent = wire.sent();
    assert_eq!(sent.len(), 1, "one request, never re-sent");
    assert_eq!(sent[0].url, "https://api.anthropic.com/v1/models");
    assert_eq!(sent[0].method, HttpMethod::Get);
    assert_eq!(
        sent[0].headers.get("x-api-key").map(String::as_str),
        Some(KEY)
    );
    assert_eq!(
        sent[0].headers.get("anthropic-version").map(String::as_str),
        Some("2023-06-01")
    );
    assert_eq!(sent[0].headers.len(), 2, "no other header");
    assert!(sent[0].body.is_none() && !sent[0].follow_redirects);
    assert_eq!(sent[0].timeout, Some(DEADLINE));
}

/// The check asks the endpoint a run would use: the operator's `NIKA_ANTHROPIC_BASE_URL`
/// (`…/messages` → `…/models`), named through the provider alias too.
#[tokio::test]
async fn the_check_honours_the_configured_endpoint_and_the_alias() {
    let wire = Listing::new(Some((200, "{}")));
    let config = anthropic(Some("https://proxy.example/anthropic/v1/messages"));
    assert_eq!(
        check_key(&wire, config, "claude", DEADLINE).await,
        KeyCheck::Valid
    );
    assert_eq!(
        wire.sent()[0].url,
        "https://proxy.example/anthropic/v1/models"
    );
    let wire = Listing::new(Some((200, "{}")));
    let odd = anthropic(Some("https://proxy.example/anthropic"));
    assert!(matches!(
        check_key(&wire, odd, "anthropic", DEADLINE).await,
        KeyCheck::Unknown { .. }
    ));
    assert!(
        wire.sent().is_empty(),
        "no check without a Messages endpoint"
    );
}

/// A 401 or a 403 refuses the key, in the provider's reduced words with the key withheld.
#[tokio::test]
async fn a_refused_key_is_named_without_the_key() {
    for status in [401, 403] {
        let body = format!(
            r#"{{"type":"error","error":{{"type":"authentication_error","message":"invalid x-api-key {KEY}"}}}}"#
        );
        let wire = Listing::new(Some((status, &body)));
        let KeyCheck::Refused { status: got, said } =
            check_key(&wire, anthropic(None), "anthropic", DEADLINE).await
        else {
            panic!("{status} refuses the key");
        };
        assert_eq!(got, status);
        assert!(!said.contains(KEY), "the key is withheld: {said}");
        assert!(said.contains("authentication_error"), "{said}");
        // The provider's words only, never a failed run's command-line hint (wave 6 r02).
        assert!(
            said.starts_with("invalid x-api-key")
                && !said.contains("NIKA-")
                && !said.contains("--"),
            "{said}"
        );
    }
}

/// No answer is unreachable; another status or a moved endpoint judges nothing; no key, a
/// keyless route or a wire without a free check sends nothing.
#[tokio::test]
async fn what_the_check_cannot_judge_is_said_apart() {
    let silent = Listing::new(None);
    assert!(matches!(
        check_key(&silent, anthropic(None), "anthropic", DEADLINE).await,
        KeyCheck::Unreachable { .. }
    ));
    let failing = Listing::new(Some((500, "{}")));
    assert!(matches!(
        check_key(&failing, anthropic(None), "anthropic", DEADLINE).await,
        KeyCheck::Unknown { .. }
    ));
    let mut moved = Listing::new(Some((200, "{}")));
    moved.moved = Some("https://elsewhere.example/v1/models");
    assert!(matches!(
        check_key(&moved, anthropic(None), "anthropic", DEADLINE).await,
        KeyCheck::Unknown { .. }
    ));
    for (config, provider) in [
        (ProvidersConfig::new(), "anthropic"),
        (ProvidersConfig::new(), "ollama"),
        (ProvidersConfig::new(), "openai"),
        (ProvidersConfig::new(), "no-such-provider"),
    ] {
        let wire = Listing::new(Some((200, "{}")));
        assert!(
            matches!(
                check_key(&wire, config, provider, DEADLINE).await,
                KeyCheck::Unknown { .. }
            ),
            "{provider}"
        );
        assert!(wire.sent().is_empty(), "{provider}: nothing was sent");
    }
}

/// On their own endpoints, a provider whose listing authenticates the key is judged by one GET
/// of it: the model list with a bearer key (`OpenAI`, xAI, `DeepSeek`, Mistral, Groq),
/// `OpenRouter`'s key endpoint (its model list is served to anyone) and Gemini's model list with
/// its own header (live gauntlet r02, 2026-10-16: an invalid `OpenAI` key read « unknown »).
#[tokio::test]
async fn a_provider_listing_that_authenticates_judges_its_key() {
    for (provider, url, header, value) in [
        (
            "openai",
            "https://api.openai.com/v1/models",
            "authorization",
            "Bearer sk-fixture",
        ),
        (
            "xai",
            "https://api.x.ai/v1/models",
            "authorization",
            "Bearer sk-fixture",
        ),
        (
            "deepseek",
            "https://api.deepseek.com/v1/models",
            "authorization",
            "Bearer sk-fixture",
        ),
        (
            "openrouter",
            "https://openrouter.ai/api/v1/key",
            "authorization",
            "Bearer sk-fixture",
        ),
        (
            "gemini",
            "https://generativelanguage.googleapis.com/v1beta/models",
            "x-goog-api-key",
            "sk-fixture",
        ),
    ] {
        let wire = Listing::new(Some((200, r#"{"data":[]}"#)));
        let config = ProvidersConfig::new().with_key(provider, Secret::new("sk-fixture"));
        assert_eq!(
            check_key(&wire, config, provider, DEADLINE).await,
            KeyCheck::Valid,
            "{provider}"
        );
        let sent = wire.sent();
        assert_eq!(sent.len(), 1, "{provider}: one request");
        assert_eq!(sent[0].url, url, "{provider}");
        assert_eq!(sent[0].headers.get(header).map(String::as_str), Some(value));
        assert_eq!(sent[0].headers.len(), 1, "{provider}: no other header");
        let refused = Listing::new(Some((401, r#"{"error":{"message":"Incorrect API key"}}"#)));
        let config = ProvidersConfig::new().with_key(provider, Secret::new("sk-fixture"));
        assert!(
            matches!(
                check_key(&refused, config, provider, DEADLINE).await,
                KeyCheck::Refused { status: 401, .. }
            ),
            "{provider}"
        );
    }
    // Gemini refuses an invalid key with HTTP 400 `API_KEY_INVALID`.
    let invalid = r#"{"error":{"code":400,"status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}}"#;
    let wire = Listing::new(Some((400, invalid)));
    let config = ProvidersConfig::new().with_key("gemini", Secret::new("sk-fixture"));
    assert!(matches!(
        check_key(&wire, config, "gemini", DEADLINE).await,
        KeyCheck::Refused { status: 400, .. }
    ));
}

/// An operator's endpoint is not judged: a proxy may list to anyone, so a 200 would not prove a
/// key. Nothing is sent.
#[tokio::test]
async fn an_operator_endpoint_is_not_judged() {
    for provider in ["deepseek", "openrouter", "openai"] {
        let wire = Listing::new(Some((200, r#"{"object":"list","data":[]}"#)));
        let config = ProvidersConfig::new()
            .with_key(provider, Secret::new("sk-fixture"))
            .with_base_url(provider, "https://proxy.example/v1/chat/completions");
        assert!(
            matches!(
                check_key(&wire, config, provider, DEADLINE).await,
                KeyCheck::Unknown { .. }
            ),
            "{provider}"
        );
        assert!(wire.sent().is_empty(), "{provider}: nothing was sent");
    }
}
