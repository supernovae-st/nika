// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The physical request counters a seat's calls run under: an [`Envelope`] holds a bound (or
//! none) and counts what was sent and refused; a [`Seat`] counts the invocations of any seat and
//! the model identities its responses report; a [`Wire`] counts the physical requests of a
//! direct API seat where its bytes leave, so a transport retry or a structured-output fallback
//! inside one invocation is a request too. A request past the bound is refused before any byte
//! leaves, and every refusal is counted beside what was sent. Which bound a door grants is its
//! caller's authority (the compiler's `authority::Authority`), never this module's. The counters
//! bound requests, never dollars.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use nika_kernel::ai::provider::{InferRequest, InferResponse, ProviderError, ProviderInferDyn};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use serde_json::{Value, json};

/// One counter of an authority: at most `max` sent (no bound when `None`), every attempt past it
/// refused and counted.
#[derive(Debug)]
pub struct Envelope {
    max: Option<u32>,
    remedy: &'static str,
    sent: AtomicU32,
    refused: AtomicU32,
}

impl Envelope {
    /// A counter of at most `max` requests; `remedy` tells the refused caller how to authorize
    /// more at its own door.
    #[must_use]
    pub const fn new(max: u32, remedy: &'static str) -> Self {
        Self::bounded(Some(max), remedy)
    }

    /// A counter with no bound: every request counted, none refused.
    #[must_use]
    pub const fn uncapped(remedy: &'static str) -> Self {
        Self::bounded(None, remedy)
    }

    const fn bounded(max: Option<u32>, remedy: &'static str) -> Self {
        Self {
            max,
            remedy,
            sent: AtomicU32::new(0),
            refused: AtomicU32::new(0),
        }
    }

    /// Takes one send, or records one refusal: atomic, so concurrent attempts never pass `max`.
    fn admit(&self) -> bool {
        let taken = self
            .sent
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |sent| {
                (self.max.is_none_or(|max| sent < max)).then(|| sent.saturating_add(1))
            })
            .is_ok();
        if !taken {
            self.refused.fetch_add(1, Ordering::SeqCst);
        }
        taken
    }

    fn refusal(&self) -> String {
        format!(
            "the authoring authority is spent ({} of {} sent): this request was refused before any byte left; {}",
            self.sent.load(Ordering::SeqCst),
            self.max.unwrap_or_default(),
            self.remedy
        )
    }

    /// What was sent and refused, as a receipt states it.
    #[must_use]
    pub fn account(&self) -> Value {
        json!({
            "sent": self.sent.load(Ordering::SeqCst),
            "refused": self.refused.load(Ordering::SeqCst),
        })
    }
}

/// A direct API seat's transport: one wire attempt per POST, no redirect followed (a followed
/// redirect is another request carrying the same prompt, uncounted), and no POST past the
/// authority.
pub struct Wire<H> {
    inner: H,
    requests: Arc<Envelope>,
}

impl<H> Wire<H> {
    /// The transport `inner`, its POSTs counted by `requests`.
    #[must_use]
    pub const fn new(inner: H, requests: Arc<Envelope>) -> Self {
        Self { inner, requests }
    }
}

impl<H: HttpPostDyn + Send + Sync> HttpPostDyn for Wire<H> {
    fn supports_single_attempt(&self) -> bool {
        self.inner.supports_single_attempt()
    }

    async fn post(&self, mut request: HttpRequest) -> Result<HttpResponse, HttpError> {
        if !self.requests.admit() {
            return Err(HttpError::Other {
                reason: self.requests.refusal(),
            });
        }
        request.follow_redirects = false;
        self.inner.post(request).await
    }

    async fn send_streaming(
        &self,
        mut request: HttpRequest,
    ) -> Result<HttpStreamResponse, HttpError> {
        if !self.requests.admit() {
            return Err(HttpError::Other {
                reason: self.requests.refusal(),
            });
        }
        request.follow_redirects = false;
        self.inner.send_streaming(request).await
    }
}

/// A seat under the authority: its invocations counted and refused past it, and the model
/// identities its responses report, kept apart from the model the operator requested; a
/// response that reports no nonblank identity is counted, its identity unknown.
pub struct Seat<P> {
    inner: P,
    invocations: Arc<Envelope>,
    observed: Mutex<Vec<String>>,
    unreported: AtomicU32,
}

impl<P> Seat<P> {
    /// The seat `inner`, its invocations counted by `invocations`.
    #[must_use]
    pub const fn new(inner: P, invocations: Arc<Envelope>) -> Self {
        Self {
            inner,
            invocations,
            observed: Mutex::new(Vec::new()),
            unreported: AtomicU32::new(0),
        }
    }

    /// The responses whose model identity is unknown (absent or blank), never assumed to be the
    /// model requested.
    #[must_use]
    pub fn unreported(&self) -> u32 {
        self.unreported.load(Ordering::SeqCst)
    }

    /// The seat itself.
    #[must_use]
    pub const fn inner(&self) -> &P {
        &self.inner
    }

    /// The model identities the responses reported, each once, in order.
    #[must_use]
    pub fn observed(&self) -> Vec<String> {
        self.observed
            .lock()
            .map(|models| models.clone())
            .unwrap_or_default()
    }
}

impl<P: ProviderInferDyn + Send + Sync> ProviderInferDyn for Seat<P> {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        // A local refusal before any request: typed as one, so the core knows nothing was used.
        if !self.invocations.admit() {
            return Err(ProviderError::AdmissionDenied {
                reason: self.invocations.refusal(),
            });
        }
        let response = self.inner.infer(request).await?;
        match (
            response.gen_ai.response_model.as_ref(),
            self.observed.lock(),
        ) {
            (Some(model), Ok(mut observed)) if !model.trim().is_empty() => {
                if !observed.contains(model) {
                    observed.push(model.clone());
                }
            }
            _ => {
                self.unreported.fetch_add(1, Ordering::SeqCst);
            }
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::ai::provider::{ContentBlock, StopReason, TokenUsage};

    const REMEDY: &str = "authorize more with --authoring-max-calls";

    #[test]
    fn a_bound_refuses_past_it_and_no_bound_refuses_nothing() {
        let uncapped = Envelope::uncapped(REMEDY);
        for _ in 0..100 {
            assert!(uncapped.admit());
        }
        assert_eq!(uncapped.account(), json!({"sent": 100, "refused": 0}));
        let capped = Envelope::new(2, REMEDY);
        assert!(capped.admit() && capped.admit() && !capped.admit());
        assert_eq!(capped.account(), json!({"sent": 2, "refused": 1}));
        assert!(
            capped.refusal().contains("(2 of 2 sent)"),
            "{}",
            capped.refusal()
        );
    }

    /// A transport that counts what reached it.
    struct Counting(AtomicU32);

    impl HttpPostDyn for Counting {
        fn supports_single_attempt(&self) -> bool {
            true
        }
        async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse::new(
                200,
                std::collections::BTreeMap::default(),
                Vec::new().into(),
                request.url,
            ))
        }
        async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(HttpError::Other {
                reason: "no stream here".to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn the_wire_refuses_past_the_authority_before_the_transport_sees_it() {
        let requests = Arc::new(Envelope::new(2, REMEDY));
        let wire = Wire::new(Counting(AtomicU32::new(0)), Arc::clone(&requests));
        assert!(
            wire.post(HttpRequest::post("https://a.test/v1"))
                .await
                .is_ok()
        );
        assert!(
            wire.send_streaming(HttpRequest::post("https://a.test/v1"))
                .await
                .is_err()
        );
        let refused = wire.post(HttpRequest::post("https://a.test/v1")).await;
        let expected = "the authoring authority is spent (2 of 2 sent): this request was refused \
                        before any byte left; authorize more with --authoring-max-calls";
        assert!(matches!(refused, Err(HttpError::Other { ref reason }) if reason == expected));
        assert_eq!(
            wire.inner.0.load(Ordering::SeqCst),
            2,
            "the third never left"
        );
        assert_eq!(requests.account(), json!({"sent": 2, "refused": 1}));
        assert!(wire.supports_single_attempt());
    }

    /// A transport that keeps whether each request it received would follow a redirect.
    struct Recording(Mutex<Vec<bool>>);

    impl HttpPostDyn for Recording {
        async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            self.0.lock().expect("log").push(request.follow_redirects);
            Ok(HttpResponse::new(
                307,
                std::collections::BTreeMap::default(),
                Vec::new().into(),
                request.url,
            ))
        }
        async fn send_streaming(
            &self,
            request: HttpRequest,
        ) -> Result<HttpStreamResponse, HttpError> {
            self.0.lock().expect("log").push(request.follow_redirects);
            Err(HttpError::Other {
                reason: "no stream here".to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn the_wire_never_follows_a_redirect() {
        let requests = Arc::new(Envelope::new(3, REMEDY));
        let wire = Wire::new(Recording(Mutex::new(Vec::new())), Arc::clone(&requests));
        // A request asks to follow redirects by default; what reaches the transport never does.
        let request = HttpRequest::post("https://a.test/v1");
        assert!(request.follow_redirects);
        let answered = wire.post(request).await.expect("the 307 is the answer");
        assert_eq!(answered.status, 307);
        let _ = wire
            .send_streaming(HttpRequest::post("https://a.test/v1"))
            .await;
        assert_eq!(*wire.inner.0.lock().expect("log"), [false, false]);
        assert_eq!(requests.account(), json!({"sent": 2, "refused": 0}));
    }

    struct ReportedModel(Option<&'static str>);

    impl ProviderInferDyn for ReportedModel {
        async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
            let mut response =
                InferResponse::new(Vec::new(), TokenUsage::new(1, 1), StopReason::EndTurn);
            response.gen_ai.response_model = self.0.map(str::to_owned);
            Ok(response)
        }
    }

    #[tokio::test]
    async fn a_blank_reported_model_is_unknown_not_an_observed_identity() {
        for reported in [None, Some(""), Some(" "), Some("\t\n")] {
            let seat = Seat::new(ReportedModel(reported), Arc::new(Envelope::new(1, REMEDY)));
            let response = seat
                .infer(InferRequest::new("requested-model", Vec::new()))
                .await
                .expect("provider answer");
            assert_eq!(
                response.gen_ai.response_model.as_deref(),
                reported,
                "the provider response remains unchanged"
            );
            assert!(
                seat.observed().is_empty(),
                "blank is not model evidence: {reported:?}"
            );
            assert_eq!(seat.unreported(), 1);
        }
        let seat = Seat::new(
            ReportedModel(Some("actually-served")),
            Arc::new(Envelope::new(2, REMEDY)),
        );
        for _ in 0..2 {
            seat.infer(InferRequest::new("requested-model", Vec::new()))
                .await
                .expect("provider answer");
        }
        assert_eq!(seat.observed(), ["actually-served"]);
        assert_eq!(seat.unreported(), 0);
    }

    /// A seat that answers with one reported identity and counts its invocations.
    struct Answering(AtomicU32);

    impl ProviderInferDyn for Answering {
        async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            let mut response = InferResponse::new(
                vec![ContentBlock::Text { text: "{}".into() }],
                TokenUsage::new(1, 1),
                StopReason::EndTurn,
            );
            response.gen_ai.response_model = Some("served-model".to_owned());
            Ok(response)
        }
    }

    #[tokio::test]
    async fn a_seat_past_the_authority_is_refused_and_its_observed_identity_kept() {
        let invocations = Arc::new(Envelope::new(2, REMEDY));
        let seat = Seat::new(Answering(AtomicU32::new(0)), Arc::clone(&invocations));
        let request = || InferRequest::new("vllm/requested", Vec::new());
        // Five concurrent invocations under a ceiling of two: two reach the seat, three refused.
        let joined = tokio::join!(
            seat.infer(request()),
            seat.infer(request()),
            seat.infer(request()),
            seat.infer(request()),
            seat.infer(request()),
        );
        let results = [joined.0, joined.1, joined.2, joined.3, joined.4];
        let answered = results.iter().filter(|result| result.is_ok()).count();
        assert_eq!((answered, seat.inner().0.load(Ordering::SeqCst)), (2, 2));
        // Each refusal is a local admission refusal, never a provider's answer.
        assert!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .all(|error| matches!(error, ProviderError::AdmissionDenied { .. }))
        );
        assert_eq!(invocations.account(), json!({"sent": 2, "refused": 3}));
        assert_eq!(seat.observed(), ["served-model"]);
        assert_eq!(seat.unreported(), 0);
    }

    /// A seat whose responses report no model identity.
    struct Unnamed;

    impl ProviderInferDyn for Unnamed {
        async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
            Ok(InferResponse::new(
                vec![ContentBlock::Text { text: "{}".into() }],
                TokenUsage::new(1, 1),
                StopReason::EndTurn,
            ))
        }
    }

    #[tokio::test]
    async fn a_response_without_a_model_is_counted_unknown_never_the_requested_one() {
        let seat = Seat::new(Unnamed, Arc::new(Envelope::new(2, REMEDY)));
        let request = || InferRequest::new("vllm/requested", Vec::new());
        assert!(seat.infer(request()).await.is_ok());
        assert!(seat.infer(request()).await.is_ok());
        assert!(seat.observed().is_empty());
        assert_eq!(seat.unreported(), 2);
    }
}
