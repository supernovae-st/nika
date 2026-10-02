// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The weekly radar's HTTP ingredient, owned by the examples traversal.
//! Only the staged URL and its exact network permit change. The shipped
//! example keeps its public endpoint; this fixture proves the local data path,
//! not that the public feed or its TLS service is available.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

const FEED_MARKER: &str = "radar-local-feed-witness";
const NOTES_MARKER: &str = "shipped the retry ladder on the ingest job";
const RSS: &str = concat!(
    "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel>",
    "<title>Fixture news</title><item><title>radar-local-feed-witness</title>",
    "<description>A deterministic local news item.</description>",
    "</item></channel></rss>"
);
const REQUEST: &str = "GET /frontpage HTTP/1.1";

pub(super) struct RadarHttp {
    listener: TcpListener,
    endpoint: String,
    requests: Vec<String>,
    failure: Option<String>,
}

impl RadarHttp {
    pub(super) fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("owned feed endpoint");
        listener.set_nonblocking(true).expect("nonblocking feed");
        let endpoint = format!(
            "http://{}/frontpage",
            listener.local_addr().expect("feed address")
        );
        Self {
            listener,
            endpoint,
            requests: Vec::new(),
            failure: None,
        }
    }

    pub(super) fn staged_body(&self, body: &str) -> Result<String, String> {
        local_copy(body, &self.endpoint)
    }

    /// Serviced by the existing child poll loop: no detached server or thread.
    /// One connection per poll, at most eight; headers and socket I/O are bounded.
    pub(super) fn poll(&mut self, deadline: Instant) {
        if self.failure.is_some() || Instant::now() >= deadline {
            return;
        }
        match self.listener.accept() {
            Ok((stream, _)) => {
                if self.requests.len() >= 8 {
                    self.failure = Some("too many requests to the fixture feed".to_owned());
                    return;
                }
                match respond(stream, deadline) {
                    Ok(request) => self.requests.push(request),
                    Err(error) => self.failure = Some(error),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => self.failure = Some(format!("feed accept: {error}")),
        }
    }

    pub(super) fn verify(&self, cwd: &Path) -> Result<(), String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = std::fs::read_to_string(cwd.join("radar/radar.md"))
            .map_err(|error| format!("radar result: {error}"))?;
        verify_evidence(&self.requests, &result)
    }
}

fn local_copy(body: &str, endpoint: &str) -> Result<String, String> {
    const URL: &str = "        url: \"https://hnrss.org/frontpage\"\n";
    const PERMIT: &str = "  net: { http: [\"hnrss.org\"] }\n";
    if body.matches(URL).count() != 1 || body.matches(PERMIT).count() != 1 {
        return Err("weekly radar transport changed; review its local fixture".to_owned());
    }
    Ok(body
        .replacen(URL, &format!("        url: \"{endpoint}\"\n"), 1)
        .replacen(PERMIT, "  net: { http: [\"127.0.0.1\"] }\n", 1))
}

fn respond(mut stream: TcpStream, run_deadline: Instant) -> Result<String, String> {
    stream
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    // A slow peer may use at most two seconds, within the existing run ceiling.
    // Both reads and writes are nonblocking and share this absolute deadline.
    let deadline = run_deadline.min(Instant::now() + Duration::from_secs(2));
    let mut header = Vec::new();
    let mut chunk = [0u8; 1024];
    while !header.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        if Instant::now() >= deadline || header.len() >= 8192 {
            return Err("fixture request header exceeded its time or byte bound".to_owned());
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Err("fixture request closed before its headers".to_owned()),
            Ok(n) => header.extend_from_slice(&chunk[..n]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => return Err(format!("fixture request: {error}")),
        }
    }
    if header.len() > 8192 {
        return Err("fixture request header exceeded its byte bound".to_owned());
    }
    let header = String::from_utf8(header).map_err(|error| error.to_string())?;
    let request = header.lines().next().unwrap_or("").to_owned();
    let (status, body) = if request == REQUEST {
        ("200 OK", RSS)
    } else {
        ("404 Not Found", "unexpected fixture request")
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/rss+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut remaining = response.as_bytes();
    while !remaining.is_empty() {
        if Instant::now() >= deadline {
            return Err("fixture response exceeded its time bound".to_owned());
        }
        match stream.write(remaining) {
            Ok(0) => return Err("fixture response closed early".to_owned()),
            Ok(n) => remaining = &remaining[n..],
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => return Err(format!("fixture response: {error}")),
        }
    }
    Ok(request)
}

fn verify_evidence(requests: &[String], result: &str) -> Result<(), String> {
    if requests.len() != 1 || requests[0] != REQUEST {
        return Err(format!(
            "expected one GET /frontpage, observed {requests:?}"
        ));
    }
    if !result.contains(FEED_MARKER) || !result.contains(NOTES_MARKER) {
        return Err("radar result must carry both fetched news and local notes".to_owned());
    }
    Ok(())
}

#[test]
fn transport_drift_is_refused_before_staging() {
    let body = nika_pack::example("snippets/weekly-radar").expect("radar example");
    let endpoint = "http://127.0.0.1:12345/frontpage";
    let staged = local_copy(body, endpoint).expect("current transport");
    let mut expected: serde_json::Value = serde_yaml_bw::from_str(body).expect("source YAML");
    expected["tasks"]["fetch_news"]["invoke"]["args"]["url"] = endpoint.into();
    expected["permits"]["net"]["http"][0] = "127.0.0.1".into();
    let actual: serde_json::Value = serde_yaml_bw::from_str(&staged).expect("staged YAML");
    assert_eq!(actual, expected, "only the transport boundary may change");
    for changed in [
        body.replace("https://hnrss.org/frontpage", "https://hnrss.org/new-feed"),
        body.replace("http: [\"hnrss.org\"]", "http: [\"other.example\"]"),
        format!("{body}        url: \"https://hnrss.org/frontpage\"\n"),
    ] {
        assert!(local_copy(&changed, endpoint).is_err(), "source drift");
    }
}

#[test]
fn a_green_result_requires_the_request_and_both_data_sources() {
    let result = format!("mock(echo) · {FEED_MARKER} · {NOTES_MARKER}");
    let request = vec![REQUEST.to_owned()];
    assert!(verify_evidence(&request, &result).is_ok());
    for requests in [
        vec![],
        vec!["GET /wrong-path HTTP/1.1".to_owned()],
        vec!["POST /frontpage HTTP/1.1".to_owned()],
        vec![REQUEST.to_owned(), REQUEST.to_owned()],
    ] {
        assert!(verify_evidence(&requests, &result).is_err());
    }
    for incomplete in ["", FEED_MARKER, NOTES_MARKER] {
        assert!(verify_evidence(&request, incomplete).is_err());
    }
}
