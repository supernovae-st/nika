// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Test-only effect substitution: tariff identity stays native `DeepSeek`, while
//! the injected kernel HTTP effect sends canned mechanics to one loopback seat.
//! This module and its thread-local hook do not exist in production builds.
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_providers::ProvidersConfig;
use std::cell::RefCell;
thread_local! { static TARGET: RefCell<Option<String>> = const { RefCell::new(None) }; }

pub(crate) struct Installed;
impl Drop for Installed {
    fn drop(&mut self) {
        TARGET.with(|v| *v.borrow_mut() = None);
    }
}
pub(crate) fn install(target: &str) -> Installed {
    assert!(target.starts_with("http://127.0.0.1:"));
    TARGET.with(|v| *v.borrow_mut() = Some(target.to_owned()));
    Installed
}
pub(crate) fn config() -> Option<ProvidersConfig> {
    TARGET.with(|v| {
        v.borrow().as_ref().map(|_| {
            ProvidersConfig::new().with_key("deepseek", Secret::new("s34-test-not-a-credential"))
        })
    })
}
#[derive(Debug)]
pub(crate) struct Client {
    inner: nika_http::ReqwestHttp,
    target: Option<String>,
}
impl Client {
    pub(crate) fn new(inner: nika_http::ReqwestHttp) -> Self {
        Self {
            inner,
            target: TARGET.with(|v| v.borrow().clone()),
        }
    }
}
impl HttpPostDyn for Client {
    fn supports_single_attempt(&self) -> bool {
        self.inner.supports_single_attempt()
    }

    async fn post(&self, mut request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let original = request.url.clone();
        if let Some(target) = &self.target {
            assert!(matches!(
                request.url.as_str(),
                "https://api.deepseek.com/v1/chat/completions"
                    | "https://api.deepseek.com/chat/completions"
            ));
            assert!(
                !request.follow_redirects,
                "bounded requests cannot follow another endpoint"
            );
            request.url.clone_from(target);
        }
        let mut response = self.inner.post(request).await?;
        if self.target.is_some() {
            response.final_url = original;
        }
        Ok(response)
    }
    async fn send_streaming(&self, request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        assert!(
            self.target.is_none(),
            "bounded streaming must refuse before transport"
        );
        self.inner.send_streaming(request).await
    }
}
