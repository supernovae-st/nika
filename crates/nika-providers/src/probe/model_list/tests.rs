// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::*;
use nika_kernel::http::{HttpError, HttpResponse};
use std::{collections::BTreeMap, sync::Mutex};

#[test]
fn a_port_or_unrelated_document_never_proves_models() {
    for body in [
        "",
        "{}",
        "<html>hi</html>",
        r#"{"models":["x"]}"#,
        r#"{"data":[{}]}"#,
        r#"{"data":[{"id":3}]}"#,
        r#"{"data":[{"id":""}]}"#,
        r#"{"data":[{"id":"a\nb"}]}"#,
        r#"{"data":[],"data":[{"id":"x"}]}"#,
        r#"{"data":[{"id":"x","id":"y"}]}"#,
        r#"{"data":[{"id":"x"},{"id":"x"}]}"#,
        r#"{"object":"unrelated","data":[]}"#,
        r#"{"data":[{"id":"x","object":"unrelated"}]}"#,
    ] {
        let got = ModelListing::response(200, body.as_bytes());
        assert_eq!(got.protocol_compatible, Some(false), "{body}");
        assert_eq!(got.available(), None, "{body}");
        assert!(got.models.is_empty());
    }
}
#[test]
fn empty_compatible_listing_is_distinct_from_unknown_and_advertised_models() {
    let empty = ModelListing::response(200, br#"{"object":"list","data":[]}"#);
    assert_eq!(empty.protocol_compatible, Some(true));
    assert_eq!(empty.available(), Some(false));
    let full = ModelListing::response(
        200,
        br#"{"data":[{"id":"local-a"},{"id":"local-b","object":"model"}]}"#,
    );
    assert_eq!(full.available(), Some(true));
    assert_eq!(full.models, ["local-a", "local-b"]);
    assert_eq!(full.failure, None);
    for status in [204, 301, 302, 401, 404, 429, 500] {
        assert_eq!(
            ModelListing::response(status, br#"{"data":[{"id":"x"}]}"#).available(),
            None
        );
    }
}
#[test]
fn cardinality_size_and_nested_duplicate_bounds_are_fail_closed() {
    let rows = vec![serde_json::json!({"id":"x"}); 1025];
    let many = serde_json::json!({"data":rows}).to_string();
    assert_eq!(
        ModelListing::response(200, many.as_bytes()).available(),
        None
    );
    let huge = vec![b' '; 262_145];
    assert_eq!(ModelListing::response(200, &huge).available(), None);
    let long = serde_json::json!({"data":[{"id":"x".repeat(513)}]}).to_string();
    assert_eq!(
        ModelListing::response(200, long.as_bytes()).available(),
        None
    );
    let nested = br#"{"data":[{"id":"x","metadata":{"a":1,"a":2}}]}"#;
    assert_eq!(ModelListing::response(200, nested).available(), None);
}
struct Wire(Mutex<Vec<HttpRequest>>, bool);
impl HttpGetDyn for Wire {
    async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let endpoint = if self.1 {
            "http://elsewhere/models".to_owned()
        } else {
            request.url.clone()
        };
        self.0.lock().expect("requests").push(request);
        Ok(HttpResponse::new(
            200,
            BTreeMap::new(),
            br#"{"data":[{"id":"fixture"}]}"#.as_slice().into(),
            endpoint,
        ))
    }
}
#[tokio::test]
async fn one_get_has_no_auth_body_redirect_or_inference_and_exact_endpoint() {
    let endpoint = "http://127.0.0.1:11434/v1/models";
    for changed in [false, true] {
        let wire = Wire(Mutex::new(Vec::new()), changed);
        let got = probe_model_listing(&wire, endpoint).await;
        assert_eq!(got.available(), if changed { None } else { Some(true) });
        let sent = wire.0.lock().expect("requests");
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].url, endpoint);
        assert_eq!(sent[0].method, nika_kernel::http::HttpMethod::Get);
        assert!(sent[0].headers.is_empty() && sent[0].body.is_none());
        assert!(!sent[0].follow_redirects);
        assert_eq!(sent[0].timeout, Some(Duration::from_millis(1000)));
    }
}
