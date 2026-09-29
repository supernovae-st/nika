// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Extraction must not turn transport userinfo into document links.

use super::*;
use nika_kernel_mock::{MockFs, MockHttp};

#[cfg(test)]
#[tokio::test]
async fn successful_extraction_never_inherits_transport_url_credentials() {
    let html = br#"<html><head><title>Useful report</title>
        <link rel="canonical" href="/canonical"></head><body><article>
        <h1>Useful report</h1><p>A detailed report about the local launch.
        This paragraph has enough useful prose to remain an article.</p>
        <a href="/other">Other report</a></article></body></html>"#;
    for mode in ["links", "metadata", "article"] {
        for final_url in ["", "https://response-user:response-secret@x.test/dir/page"] {
            let request_url = "https://request-user:request-secret@x.test/start";
            let http = MockHttp::new().enqueue_ok_final_url(200, html.to_vec(), final_url);
            let args = serde_json::json!({"url": request_url, "mode": mode})
                .as_object()
                .expect("args")
                .clone();
            let out = fetch(&http, &MockFs::new(), &FsBoundary::unbounded(), &args)
                .await
                .expect("successful extraction");
            let text = out.to_string();
            for credential in [
                "request-user",
                "request-secret",
                "response-user",
                "response-secret",
            ] {
                assert!(!text.contains(credential), "{mode}/{final_url}: {text}");
            }
            assert!(
                text.contains("https://x.test/"),
                "resolved URL must remain: {text}"
            );
            assert_eq!(
                http.sent_requests()[0].url,
                request_url,
                "transport input is unchanged"
            );
        }
    }
}
