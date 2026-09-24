// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real loopback transport regressions; no key, DNS or paid endpoint.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn bounded_post_does_not_follow_307_or_308() {
    for status in [307, 308] {
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/chat/completions", origin.local_addr().unwrap());
        let location = format!("http://{}/other", target.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = origin.accept().await.unwrap();
            let mut b = [0; 4096];
            let _ = stream.read(&mut b).await.unwrap();
            stream.write_all(format!("HTTP/1.1 {status} Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        });
        let config = HttpConfig {
            ssrf: SsrfMode::Disabled,
            retry_protocol_nacks: false,
            ..Default::default()
        };
        let http = ReqwestHttp::with_config(config).unwrap();
        assert!(http.supports_single_attempt());
        let mut req = HttpRequest::post(&url);
        req.follow_redirects = false;
        let response = http.post(req).await.unwrap();
        assert_eq!(response.status, status);
        assert_eq!(response.final_url, url);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), target.accept())
                .await
                .is_err()
        );
        server.await.unwrap();
    }
}

// Minimal HTTP/2 peer: SETTINGS plus REFUSED_STREAM for every request. This
// exercises reqwest's real protocol-NACK retry machinery without TLS fixtures
// or another HTTP dependency; h2c prior-knowledge is test-only composition.
async fn nack_peer(mut stream: tokio::net::TcpStream, count: Arc<AtomicUsize>) {
    let mut preface = [0; 24];
    if stream.read_exact(&mut preface).await.is_err() {
        return;
    }
    assert_eq!(&preface, b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
    if stream
        .write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0])
        .await
        .is_err()
    {
        return;
    }
    loop {
        let mut h = [0u8; 9];
        if stream.read_exact(&mut h).await.is_err() {
            return;
        }
        let size = (usize::from(h[0]) << 16) | (usize::from(h[1]) << 8) | usize::from(h[2]);
        let mut data = vec![0; size];
        if stream.read_exact(&mut data).await.is_err() {
            return;
        }
        if h[3] == 4 && h[4] & 1 == 0 {
            if stream
                .write_all(&[0, 0, 0, 4, 1, 0, 0, 0, 0])
                .await
                .is_err()
            {
                return;
            }
        } else if h[3] == 1 {
            count.fetch_add(1, Ordering::SeqCst);
            let rst = [0, 0, 4, 3, 0, h[5], h[6], h[7], h[8], 0, 0, 0, 7];
            if stream.write_all(&rst).await.is_err() {
                return;
            }
        }
    }
}
#[tokio::test]
async fn protocol_nack_retry_is_disabled_only_in_bounded_configuration() {
    for retry in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        let server = tokio::spawn(async move {
            let mut peers = tokio::task::JoinSet::new();
            loop {
                let (s, _) = listener.accept().await.unwrap();
                peers.spawn(nack_peer(s, seen.clone()));
            }
        });
        let config = HttpConfig {
            ssrf: SsrfMode::Disabled,
            retry_protocol_nacks: retry,
            timeout: Duration::from_secs(2),
            ..Default::default()
        };
        let builder = retry_policy(
            reqwest::Client::builder()
                .no_proxy()
                .http2_prior_knowledge(),
            &config,
        );
        let http = ReqwestHttp {
            inner: builder.build().unwrap(),
            config,
        };
        let mut req = HttpRequest::post(url);
        req.follow_redirects = false;
        assert!(http.post(req).await.is_err());
        assert_eq!(count.load(Ordering::SeqCst), if retry { 3 } else { 1 });
        assert_eq!(http.supports_single_attempt(), !retry);
        server.abort();
        let _ = server.await;
    }
}
