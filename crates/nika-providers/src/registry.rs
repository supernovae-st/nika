// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Registry — `model: provider/name` resolution + key loading.
//!
//! [`ProviderRegistry`] is constructed once by the wiring layer with the
//! injected http effect; verbs call [`ProviderRegistry::resolve`] per
//! inference and drive the returned [`ResolvedProvider`] through the kernel
//! `ProviderInferDyn` / `ProviderStreamDyn` / `ProviderMeta` traits.
//!
//! Fail-fast discipline: everything that can be diagnosed *before* a wire
//! call (unknown provider · malformed model string · missing API key ·
//! missing http effect) errors at `resolve` time with a message that names
//! the exact fix (first-error UX · B7.2).
//!
//! Key sovereignty: this crate NEVER reads process env or any secret store.
//! The composition root resolves secrets (kernel `SecretResolver` · or env
//! at the L4 CLI) and injects them via [`ProvidersConfig::with_key`]. The
//! profile env ladder is *data* used for error hints + `nika doctor`.

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_kernel::ai::provider::{
    InferEventStream, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ProviderMeta,
    ProviderStreamDyn,
};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_types::access::AccessClass;

use crate::profile::{Profile, WireFormat, seed};
use crate::retry::TransportReport;
use crate::wire;

/// Operator-owned configuration (overrides on top of the profile defaults).
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct ProvidersConfig {
    base_urls: BTreeMap<String, String>,
    keys: BTreeMap<String, Secret>,
}

impl ProvidersConfig {
    /// Empty config — profile defaults · no keys (inject via [`Self::with_key`]).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Override a provider's endpoint (local servers · openrouter-style
    /// `base_url` escape hatch). Operator config only — never workflow YAML.
    ///
    /// Shape per wire: for the OpenAI-compat and Anthropic wires the value
    /// is the COMPLETE endpoint URL; for `gemini` it is a STEM — the
    /// adapter appends `/models/{model}:generateContent` itself (do NOT
    /// include the model segment in the override).
    #[must_use]
    pub fn with_base_url(mut self, provider: impl Into<String>, url: impl Into<String>) -> Self {
        self.base_urls.insert(provider.into(), url.into());
        self
    }

    /// Inject an API key explicitly (wins over the env ladder).
    #[must_use]
    pub fn with_key(mut self, provider: impl Into<String>, key: Secret) -> Self {
        self.keys.insert(provider.into(), key);
        self
    }

    /// Check the model law using only this configuration's injected endpoint
    /// for the model's canonical provider. Other providers' overrides and
    /// credentials cannot change the result; no environment is consulted.
    #[must_use]
    pub fn resolve_refusal(&self, model: &str) -> Option<crate::ResolveRefusal> {
        let endpoint = model
            .split_once('/')
            .and_then(|(provider, _)| self.base_urls.get(crate::canonical_provider(provider)))
            .map(String::as_str);
        crate::profile::resolve_refusal_at(model, endpoint)
    }
}

/// Placeholder http effect for registries that only serve the `mock`
/// profile (doc examples · zero-network tests). Any real call through it
/// is a wiring bug and errors accordingly.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHttp;

impl HttpPostDyn for NoHttp {
    async fn post(&self, _request: HttpRequest) -> Result<HttpResponse, HttpError> {
        Err(HttpError::Unsupported {
            reason: "no http effect wired (ProviderRegistry::without_http)".to_owned(),
        })
    }

    async fn send_streaming(&self, _request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        Err(HttpError::Unsupported {
            reason: "no http effect wired (ProviderRegistry::without_http)".to_owned(),
        })
    }
}

/// The provider registry — canonical profiles + resolved keys + the http
/// effect handle.
#[derive(Debug)]
pub struct ProviderRegistry<H = NoHttp> {
    http: Option<Arc<H>>,
    profiles: Vec<Profile>,
    config: ProvidersConfig,
    pub(crate) admission: Option<crate::InferenceAdmission>,
    admission_http: Option<Arc<H>>,
}

impl ProviderRegistry<NoHttp> {
    /// Registry without an http effect — only the `mock` profile is
    /// resolvable. For doc examples and zero-network tests.
    #[must_use]
    pub fn without_http(config: ProvidersConfig) -> Self {
        Self {
            http: None,
            profiles: seed(),
            config,
            admission: None,
            admission_http: None,
        }
    }
}

// Capability queries that need only the profiles (no http · no key) —
// the keyless surface the composition's per-call bridge consults.
impl<H> ProviderRegistry<H> {
    /// Attach one shared catalog account. The injected HTTP effect must make
    /// one physical attempt per post (`ReqwestHttp`: disable protocol retries).
    #[must_use]
    pub fn with_inference_admission(mut self, admission: crate::InferenceAdmission) -> Self {
        self.admission = Some(admission);
        self.admission_http = None;
        self
    }

    /// Attach an account with a separate single-attempt HTTP transport for
    /// the routes it observes. Other routes keep the registry's original
    /// transport, including its protocol retry policy and wire family.
    #[must_use]
    pub fn with_inference_admission_http(
        mut self,
        admission: crate::InferenceAdmission,
        http: Arc<H>,
    ) -> Self {
        self.admission = Some(admission);
        self.admission_http = Some(http);
        self
    }

    /// The base URL a run against this provider would ACTUALLY hit —
    /// the operator's `with_base_url` override when present, else the
    /// profile seed. Diagnostic surfaces (doctor `--ping`) must probe
    /// THIS, not the seed: pinging 127.0.0.1 while runs talk to the
    /// operator's GPU box is an anti-doctor.
    #[must_use]
    pub fn effective_base_url(&self, provider: &str) -> Option<&str> {
        let provider = crate::profile::canonical_provider(provider);
        if let Some(url) = self.config.base_urls.get(provider) {
            return Some(url.as_str());
        }
        self.profiles
            .iter()
            .find(|p| p.id == provider)
            .map(|p| p.base_url)
    }

    /// Whether `model`'s resolved provider supports native
    /// `response_format: json_schema` (structured output).
    ///
    /// WIRE-INTRINSIC and KEYLESS: the answer is a property of the
    /// provider's wire family (see [`WireFormat::supports_response_format`]),
    /// not of the key or the http effect — so a caller can ask BEFORE a
    /// `resolve()` (which would also demand a key). An unknown provider or
    /// a malformed model string answers `false` — the SAFE default: a
    /// caller that can't confirm native support falls back to a schema
    /// INSTRUCTION — correct (if no longer required) on every wire.
    #[must_use]
    pub fn supports_response_format(&self, model: &str) -> bool {
        model
            .split_once('/')
            .and_then(|(provider_id, rest)| {
                let provider_id = crate::profile::canonical_provider(provider_id);
                self.profiles
                    .iter()
                    .find(|p| p.id == provider_id)
                    .map(|p| (p, rest))
            })
            .is_some_and(|(p, rest)| p.supports_response_format_for(p.resolve_model(rest)))
    }
}

impl<H> ProviderRegistry<H>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    /// Construct with the injected http effect (the wiring layer passes
    /// `nika-http`'s `ReqwestHttp`).
    #[must_use]
    pub fn new(http: Arc<H>, config: ProvidersConfig) -> Self {
        Self {
            http: Some(http),
            profiles: seed(),
            config,
            admission: None,
            admission_http: None,
        }
    }

    /// The canonical profiles (read-only view).
    #[must_use]
    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    /// Resolve a `provider/name` model string into a ready-to-call
    /// provider. All pre-wire diagnostics happen here (fail fast).
    ///
    /// # Errors
    ///
    /// `ProviderError::Other` on a malformed model string (no `/`), an
    /// unknown provider id, or a cloud profile resolved without an http
    /// effect; `ProviderError::AuthFailed` when the profile requires a key
    /// and none was injected — the message prints the env ladder (operator
    /// voice); embedders inject via [`ProvidersConfig::with_key`].
    pub fn resolve(&self, model: &str) -> Result<ResolvedProvider<H>, ProviderError> {
        let Some((provider_id, model_rest)) = model.split_once('/') else {
            let reason = match nika_catalog::pasteable_for(model) {
                Some(id) => format!("model '{model}' must be 'provider/name' — write '{id}'"),
                None => format!(
                    "model '{model}' must be 'provider/name' (e.g. 'anthropic/sonnet') · \
                     known providers: {}",
                    crate::profile::CANONICAL_IDS.join(", ")
                ),
            };
            return Err(ProviderError::Other { reason });
        };
        if model_rest.is_empty() {
            return Err(ProviderError::Other {
                reason: format!(
                    "model '{model}' is missing the model name after '/' \
                     (e.g. '{provider_id}/sonnet')"
                ),
            });
        }
        // Catalog aliases (`grok` → `xai`) are the same seat as the
        // canonical id — check already cleared `grok/grok-3`; run used
        // to fail NIKA-INFER-001 unknown provider `grok` (B18).
        let provider_id = crate::profile::canonical_provider(provider_id);
        let Some(profile) = self.profiles.iter().find(|p| p.id == provider_id) else {
            return Err(ProviderError::Other {
                reason: format!(
                    "unknown provider '{provider_id}' · known providers: {}",
                    crate::profile::CANONICAL_IDS.join(", ")
                ),
            });
        };

        let key = self.config.keys.get(profile.id).cloned();
        if profile.requires_key && key.is_none() {
            let ladder = profile.env_candidates();
            // Operator voice only — the embedder's road (`ProvidersConfig::
            // with_key`) lives in rustdoc, never in a rendered error.
            return Err(ProviderError::AuthFailed {
                reason: format!(
                    "no API key for '{}' · set one of [{}] (e.g. `export {}=…`)",
                    profile.id,
                    ladder.join(", "),
                    ladder.last().map_or("", String::as_str),
                ),
            });
        }

        let http = match profile.wire {
            WireFormat::Mock => None,
            _ => match &self.http {
                Some(h) => Some(Arc::clone(h)),
                None => {
                    return Err(ProviderError::Other {
                        reason: format!(
                            "provider '{}' needs an http effect · construct the registry with \
                             ProviderRegistry::new(http, config)",
                            profile.id
                        ),
                    });
                }
            },
        };

        let base_url = self
            .config
            .base_urls
            .get(profile.id)
            .cloned()
            .unwrap_or_else(|| profile.base_url.to_owned());
        let wire_model = profile.resolve_model(model_rest).to_owned();
        let admission = self
            .admission
            .as_ref()
            .filter(|account| account.tracks_route(profile.id, &wire_model, &base_url))
            .cloned();
        if admission.is_none() {
            self.refuse_unreviewed(model, profile.id)?;
        }
        let http = if admission.is_some() {
            self.admission_http.as_ref().map(Arc::clone).or(http)
        } else {
            http
        };

        Ok(ResolvedProvider {
            profile: profile.clone(),
            wire_model,
            base_url,
            key,
            http,
            admission,
        })
    }

    /// A Run's observer judges the route actually rendered: one its review
    /// calls unknown-cost was never chosen, so no byte may leave for it.
    fn refuse_unreviewed(&self, model: &str, provider: &str) -> Result<(), ProviderError> {
        let Some(account) = self.admission.as_ref().filter(|a| a.refuses_unknown_cost()) else {
            return Ok(());
        };
        if crate::profile::access_class_for(provider) != AccessClass::Api
            || matches!(
                crate::admission::unknown_cost_route(model, self.config.clone()),
                Ok(None)
            )
        {
            return Ok(());
        }
        Err(account.refuse(&format!(
            "model `{model}` has an unknown USD cost and no fresh choice covers a route rendered at run time: refused before any provider call (a literal `model:` gets the review)"
        )))
    }
}

/// One resolved provider — implements the kernel provider traits.
///
/// Fully owned (no registry borrow): streams returned by
/// `infer_stream` are `'static` as the kernel contract requires.
#[derive(Debug)]
pub struct ResolvedProvider<H = NoHttp> {
    pub(crate) profile: Profile,
    pub(crate) wire_model: String,
    pub(crate) base_url: String,
    pub(crate) key: Option<Secret>,
    pub(crate) http: Option<Arc<H>>,
    pub(crate) admission: Option<crate::InferenceAdmission>,
}

impl<H> ResolvedProvider<H> {
    /// The wire-level model identifier (nickname already resolved).
    #[must_use]
    pub fn wire_model(&self) -> &str {
        &self.wire_model
    }

    /// The credential this provider sends — the key the operator injected for its canonical
    /// id, resolved by [`ProviderRegistry::resolve`] — for a composition root that must
    /// withhold it from what it returns. `None` when the provider is keyless. The [`Secret`]
    /// stays redacted in `Debug`; nothing here prints or logs it.
    #[must_use]
    pub fn key(&self) -> Option<&Secret> {
        self.key.as_ref()
    }

    /// Does this provider's STRICT structured mode reject UNDERSPECIFIED
    /// schemas (an object without `properties` · an array without
    /// `items`)? Delegates to the wire-family source of truth
    /// ([`WireFormat::strict_rejects_underspecified`]) — the F2 fallback
    /// decision seam (native JSON mode + local validation instead).
    #[must_use]
    pub fn strict_schema_rejects_underspecified(&self) -> bool {
        self.profile.wire.strict_rejects_underspecified()
    }
}

// Soft-seal opt-in (workspace crate): with the ISP Dyn impls below, the
// blanket `Provider` super-trait applies via trait-variant's reverse
// blankets.
impl<H> nika_kernel::sealed::Sealed for ResolvedProvider<H> {}

impl<H> ResolvedProvider<H>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    /// One inference WITH the transport's own account of it: one round-trip, never re-sent
    /// (a 429 · 503 · 529 ends the call with its typed transient rejection,
    /// [`crate::retry::transient_rejection`]), plus the [`TransportReport`] a verb folds into
    /// its receipt. On failure the typed error stays intact — its transience drives the author's
    /// `retry:` or the Session's own counted policy — and the report rides beside it.
    ///
    /// # Errors
    ///
    /// The provider's error for that one round-trip (a transient rejection · a wrong request · a
    /// dead key · an exhausted quota · a dropped connection).
    pub async fn infer_reported(
        &self,
        request: InferRequest,
    ) -> Result<(InferResponse, TransportReport), (ProviderError, Box<TransportReport>)> {
        let mut report = TransportReport::new();
        if let Some(a) = &self.admission {
            if let Err(error) = a.check_route(self.profile.id, &self.wire_model, &self.base_url) {
                return Err((a.refuse(&error.to_string()), Box::new(report)));
            }
            let mut sent = false;
            let mut call = None;
            let mut route = None;
            // Keep this alternate wire future boxed as well: even a mock call
            // carries the largest branch in the async state machine.
            let entry = crate::dispatch_journal::open();
            let result = Box::pin(wire::openai_compat::infer_tracked(
                self, request, &mut sent, &mut route, &mut call,
            ))
            .await;
            if let Some(entry) = entry {
                entry.settle(call.as_ref(), result.is_ok());
            }
            report.record(call);
            report.attempts = u32::from(sent);
            return result
                .map(|mut r| {
                    r.inference_calls.clone_from(&report.inference_calls);
                    (r, report.clone())
                })
                .map_err(|e| (e, Box::new(report)));
        }
        let mut call = None;
        let mut route = None;
        // The attempt is boxed: its state machine would otherwise carry the largest wire future
        // inline, and a nested run (a workflow invoking a workflow) polls it from a deeper stack
        // than a 2 MiB thread affords (the pre-push gate's child-run test).
        let entry = crate::dispatch_journal::open();
        let result = Box::pin(self.infer_once(request, &mut route, &mut call)).await;
        if let Some(entry) = entry {
            entry.settle(call.as_ref(), result.is_ok());
        }
        report.record(call);
        match result {
            Ok(mut response) => {
                response.inference_calls.clone_from(&report.inference_calls);
                Ok((response, report))
            }
            Err(err) => Err((err, Box::new(report))),
        }
    }

    /// One round-trip on the profile's wire.
    async fn infer_once(
        &self,
        request: InferRequest,
        route: &mut Option<crate::retry::BillingRoute>,
        call: &mut Option<nika_types::cost::InferenceCall>,
    ) -> Result<InferResponse, ProviderError> {
        match self.profile.wire {
            WireFormat::Anthropic => {
                wire::anthropic::infer_routed(self, request, route, call).await
            }
            WireFormat::OpenAiCompat => {
                wire::openai_compat::infer_routed(self, request, route, call).await
            }
            WireFormat::Gemini => wire::gemini::infer_routed(self, request, route, call).await,
            WireFormat::Mock => wire::mock::answer(self, &request),
        }
    }

    /// One streaming open on the profile's wire.
    async fn infer_stream_once(
        &self,
        request: InferRequest,
    ) -> Result<InferEventStream, ProviderError> {
        match self.profile.wire {
            WireFormat::Anthropic => wire::anthropic::infer_stream(self, request).await,
            WireFormat::OpenAiCompat => wire::openai_compat::infer_stream(self, request).await,
            WireFormat::Gemini => wire::gemini::infer_stream(self, request).await,
            WireFormat::Mock => wire::mock::answer_stream(self, &request),
        }
    }
}

impl<H> ProviderInferDyn for ResolvedProvider<H>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    /// The kernel contract over [`Self::infer_reported`]: one round-trip, never re-sent.
    /// Per-dispatch monetary observations survive both results.
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        match self.infer_reported(request).await {
            Ok((response, _)) => Ok(response),
            Err((err, report)) => Err(err.with_inference_calls(report.inference_calls)),
        }
    }
}

impl<H> ProviderStreamDyn for ResolvedProvider<H>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    /// One streaming open, never re-sent: an open refused with a 429 · 503 · 529 ends the call
    /// with its typed transient rejection ([`crate::retry::transient_rejection`]).
    async fn infer_stream(&self, request: InferRequest) -> Result<InferEventStream, ProviderError> {
        if let Some(a) = &self.admission {
            return Err(a.refuse("streaming has no qualified aggregate admission settlement"));
        }
        self.infer_stream_once(request).await
    }
}

impl<H> ProviderMeta for ResolvedProvider<H>
where
    H: Send + Sync,
{
    fn name(&self) -> &str {
        self.profile.id
    }

    /// The resolved seat's actual capability · the wire-family answer
    /// ([`WireFormat::supports_response_format`]) refined by the catalog's
    /// per-model `json_mode` ([`Profile::supports_response_format_for`]).
    fn supports_response_format(&self) -> bool {
        self.profile.supports_response_format_for(&self.wire_model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_law_uses_only_the_canonical_providers_injected_endpoint() {
        let model = "openai/deepseek-v4-flash-0731";
        let endpoint = "https://compatible.example/v1/chat/completions";
        let refusal = crate::resolve_refusal(model).expect("default refuses wrong seat");
        assert_eq!(
            ProvidersConfig::new().resolve_refusal(model),
            Some(refusal.clone())
        );
        assert_eq!(
            ProvidersConfig::new()
                .with_base_url("scaleway", endpoint)
                .resolve_refusal(model),
            Some(refusal.clone()),
            "another provider's endpoint cannot change this model's route"
        );
        assert_eq!(
            ProvidersConfig::new()
                .with_base_url(
                    "openai",
                    "HTTPS://API.OPENAI.COM:443/v1/chat/completions#view"
                )
                .resolve_refusal(model),
            Some(refusal)
        );
        let custom = ProvidersConfig::new().with_base_url("openai", endpoint);
        assert!(custom.resolve_refusal(model).is_none());
        for malformed in ["deepseek-v4-flash-0731", "unknown/deepseek-v4-flash-0731"] {
            assert_eq!(
                custom.resolve_refusal(malformed),
                crate::resolve_refusal(malformed)
            );
        }
        assert!(
            ProvidersConfig::new()
                .with_base_url("xai", endpoint)
                .resolve_refusal("grok/deepseek-v4-flash-0731")
                .is_none(),
            "model aliases select their canonical provider's override"
        );
        assert_eq!(
            ProvidersConfig::new()
                .with_base_url("grok", endpoint)
                .resolve_refusal("grok/deepseek-v4-flash-0731"),
            crate::resolve_refusal("grok/deepseek-v4-flash-0731"),
            "an alias-keyed override is not used by the registry either"
        );
    }

    fn hermetic() -> ProvidersConfig {
        ProvidersConfig::new()
    }

    #[tokio::test]
    async fn profiles_view_exposes_the_canonical_eighteen() {
        let reg = ProviderRegistry::new(Arc::new(NoHttp), hermetic());
        assert_eq!(reg.profiles().len(), 18);
    }

    #[test]
    fn mock_resolves_without_http() {
        let reg = ProviderRegistry::without_http(hermetic());
        let p = reg.resolve("mock/echo").expect("mock needs no http");
        assert_eq!(p.name(), "mock");
        assert_eq!(p.wire_model(), "echo");
    }

    #[test]
    fn missing_slash_is_a_guided_error() {
        let reg = ProviderRegistry::without_http(hermetic());
        let err = reg.resolve("sonnet").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("provider/name"), "guides the shape: {msg}");
    }

    #[test]
    fn supports_response_format_is_keyless_and_per_wire() {
        // Keyless (no `with_key`): the capability is wire-intrinsic, so a
        // caller can ask BEFORE resolving (which would demand a key).
        let reg = ProviderRegistry::without_http(hermetic());
        // gemini + openai-family → native structured output (robust).
        assert!(reg.supports_response_format("gemini/flash"));
        assert!(reg.supports_response_format("openai/gpt-4o"));
        // deepseek is the per-PROFILE correction on an openai-compat wire:
        // its API accepts only text|json_object (json_schema out-of-enum →
        // 4xx · api-docs.deepseek.com · 2026-07-08) → instruction fallback.
        assert!(!reg.supports_response_format("deepseek/chat"));
        assert!(reg.supports_response_format("ollama/llama3")); // openai-compat local
        assert!(reg.supports_response_format("mock/echo"));
        // anthropic → native since 2026-07-07 (output_config.format ·
        // GA 2026-01-29 · the wire normalizes to its narrower dialect).
        assert!(reg.supports_response_format("anthropic/sonnet"));
    }

    #[test]
    fn supports_response_format_unknown_or_malformed_is_false() {
        // The SAFE default — a caller that can't confirm native support
        // takes the universally-correct instruction fallback.
        let reg = ProviderRegistry::without_http(hermetic());
        assert!(
            !reg.supports_response_format("acme/gpt-99"),
            "unknown provider"
        );
        assert!(!reg.supports_response_format("sonnet"), "no slash");
        assert!(!reg.supports_response_format(""), "empty model");
    }

    #[test]
    fn unknown_provider_lists_the_knowns() {
        let reg = ProviderRegistry::without_http(hermetic());
        let err = reg.resolve("acme/gpt-99").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown provider 'acme'"), "{msg}");
        assert!(msg.contains("anthropic"), "lists knowns: {msg}");
        assert!(msg.contains("ollama"), "lists knowns: {msg}");
    }

    #[test]
    fn missing_key_fails_fast_with_export_hint() {
        let reg = ProviderRegistry::without_http(hermetic());
        let err = reg.resolve("anthropic/sonnet").unwrap_err();
        assert!(matches!(err, ProviderError::AuthFailed { .. }));
        let msg = err.to_string();
        assert!(msg.contains("NIKA_ANTHROPIC_API_KEY"), "{msg}");
        assert!(msg.contains("ANTHROPIC_API_KEY"), "{msg}");
        assert!(msg.contains("export "), "actionable fix: {msg}");
    }

    #[test]
    fn injected_key_resolves_and_maps_nickname() {
        let reg = ProviderRegistry::without_http(
            hermetic().with_key("anthropic", Secret::new("sk-ant-test")),
        );
        // anthropic with a key but no http → the http gate fires (after auth).
        let err = reg.resolve("anthropic/sonnet").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("needs an http effect"), "{msg}");
    }

    #[tokio::test]
    async fn cloud_profile_resolves_with_http_and_key() {
        let http = Arc::new(NoHttp);
        let reg = ProviderRegistry::new(
            http,
            hermetic().with_key("anthropic", Secret::new("sk-ant-test")),
        );
        let p = reg.resolve("anthropic/sonnet").expect("resolves");
        assert_eq!(p.name(), "anthropic");
        assert!(
            p.wire_model().starts_with("claude-"),
            "nickname mapped: {}",
            p.wire_model()
        );
    }

    #[test]
    fn local_provider_needs_no_key_but_needs_http() {
        let reg = ProviderRegistry::without_http(hermetic());
        let err = reg.resolve("ollama/llama3.2").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("needs an http effect"),
            "no-key path reaches the http gate: {msg}"
        );
    }

    #[tokio::test]
    async fn base_url_override_wins() {
        let reg = ProviderRegistry::new(
            Arc::new(NoHttp),
            hermetic().with_base_url("ollama", "http://10.0.0.5:11434/v1/chat/completions"),
        );
        let p = reg.resolve("ollama/llama3.2").expect("resolves");
        assert_eq!(p.base_url, "http://10.0.0.5:11434/v1/chat/completions");
    }

    #[tokio::test]
    async fn scaleway_and_openai_keep_separate_routes_keys_and_unknown_prices() {
        use nika_kernel::ai::provider::{Message, Role};
        let scw_reply = r#"{"model":"deepseek-v4-flash-0731","choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":3}}"#;
        let oa_reply = r#"{"model":"gpt-4o-mini","choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":3}}"#;
        let http = crate::test_support::FakeHttp::with_sequence(&[
            (200, scw_reply, &[]),
            (200, oa_reply, &[]),
        ]);
        let scw_url =
            "https://api.scaleway.ai/11111111-2222-4333-8444-555555555555/v1/chat/completions";
        let reg = ProviderRegistry::new(
            Arc::clone(&http),
            ProvidersConfig::new()
                .with_key("openai", Secret::new("openai-test-key"))
                .with_key("scaleway", Secret::new("scaleway-test-key"))
                .with_base_url("scaleway", scw_url),
        );
        let scw = reg
            .resolve("scaleway/deepseek-v4-flash-0731")
            .expect("Scaleway");
        let oa = reg.resolve("openai/gpt-4o-mini").expect("OpenAI");
        assert_eq!(scw.name(), "scaleway");
        assert_eq!(oa.name(), "openai");
        let request = || InferRequest::new("ignored", vec![Message::text(Role::User, "hello")]);
        let (response, report) = scw
            .infer_reported(request())
            .await
            .expect("Scaleway response");
        oa.infer(request()).await.expect("OpenAI response");
        assert_eq!(response.usage.input_tokens, 9);
        assert_eq!(report.inference_calls.len(), 1);
        let call = &report.inference_calls[0];
        let route = call.route.as_ref().expect("observed route");
        assert_eq!(route.provider, "scaleway");
        assert_eq!(route.model, "deepseek-v4-flash-0731");
        assert_eq!(route.endpoint, scw_url);
        assert_eq!(
            call.response_model.as_deref(),
            Some("deepseek-v4-flash-0731")
        );
        assert!(call.usage.is_some());
        assert!(call.estimated_usd.is_none());
        assert!(call.known_estimate().is_none());
        assert!(nika_catalog::find_pricing_scoped("scaleway", "deepseek-v4-flash-0731").is_none());
        assert!(nika_catalog::find_pricing_scoped("scaleway", "deepseek-v4-pro").is_none());
        assert!(
            nika_catalog::admission::InferenceTariff::new("scaleway", "deepseek-v4-pro", scw_url)
                .is_none()
        );
        let sent = http.captured();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].url, scw_url);
        assert_eq!(
            sent[0].headers.get("authorization").map(String::as_str),
            Some("Bearer scaleway-test-key")
        );
        assert_eq!(sent[1].url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(
            sent[1].headers.get("authorization").map(String::as_str),
            Some("Bearer openai-test-key")
        );
        let body: serde_json::Value =
            serde_json::from_slice(sent[0].body.as_ref().expect("body")).expect("json");
        assert_eq!(body["model"], "deepseek-v4-flash-0731");
        let openai_only = ProviderRegistry::new(
            Arc::new(NoHttp),
            ProvidersConfig::new().with_key("openai", Secret::new("other-test-key")),
        );
        assert!(matches!(
            openai_only.resolve("scaleway/deepseek-v4-flash-0731"),
            Err(ProviderError::AuthFailed { .. })
        ));
    }

    #[test]
    fn debug_never_leaks_the_key() {
        let reg = ProviderRegistry::without_http(
            hermetic().with_key("mock", Secret::new("super-secret-value")),
        );
        let p = reg.resolve("mock/echo").expect("resolves");
        let dbg = format!("{p:?}");
        assert!(!dbg.contains("super-secret-value"), "redacted: {dbg}");
    }

    /// B18 / issue 1306: `grok/grok-3` resolves as xAI and takes `XAI_API_KEY`.
    #[tokio::test]
    async fn grok_alias_resolves_as_xai_using_xai_key() {
        let reg = ProviderRegistry::new(
            Arc::new(NoHttp),
            hermetic().with_key("xai", Secret::new("xai-test")),
        );
        let p = reg
            .resolve("grok/grok-3")
            .expect("grok is the xAI alias — runnable");
        assert_eq!(p.name(), "xai");
        assert_eq!(p.wire_model(), "grok-3");
        assert!(reg.supports_response_format("grok/grok-3"));
    }

    #[test]
    fn grok_alias_without_xai_key_teaches_xai_env() {
        let reg = ProviderRegistry::without_http(hermetic());
        let err = reg.resolve("grok/grok-3").unwrap_err();
        let msg = err.to_string();
        assert!(
            matches!(err, ProviderError::AuthFailed { .. }),
            "alias must reach the xAI key gate, not unknown-provider: {msg}"
        );
        assert!(msg.contains("XAI_API_KEY"), "{msg}");
        assert!(!msg.contains("unknown provider"), "{msg}");
        assert!(!msg.contains("groq"), "{msg}");
    }

    #[test]
    fn bare_grok_3_repair_names_the_pasteable_id() {
        let reg = ProviderRegistry::without_http(hermetic());
        let err = reg.resolve("grok-3").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("xai/grok-3"), "{msg}");
        assert!(!msg.contains("groq"), "{msg}");
    }

    #[test]
    fn gemini_profile_needs_http_like_other_clouds() {
        let reg = ProviderRegistry::without_http(hermetic());
        // gemini requires a key (catalog row) → inject one; no http → gate.
        let reg2 =
            ProviderRegistry::without_http(hermetic().with_key("gemini", Secret::new("test-key")));
        drop(reg);
        let err = reg2.resolve("gemini/flash-25").unwrap_err();
        assert!(err.to_string().contains("needs an http effect"));
    }
}
