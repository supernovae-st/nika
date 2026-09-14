// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Preserve typed transport failures at send, buffered-read and stream boundaries.

use std::error::Error;
use std::io::ErrorKind;
use std::time::Duration;

use nika_kernel::HttpError;

/// Kernel refusals win before retry classification. A request error alone is
/// insufficient: malformed HTTP and decoding failures are not socket outages.
pub(super) fn map_send_error(
    error: &reqwest::Error,
    timeout: Duration,
    endpoint: &str,
) -> HttpError {
    if let Some(guard) = super::find_http_error(error) {
        return guard;
    }
    if error.is_timeout() {
        return HttpError::Timeout {
            duration_ms: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
        };
    }
    let cause = connection_cause(error);
    let reason = format!(
        "{}: {}",
        endpoint_origin(endpoint),
        cause.unwrap_or_else(|| generic_cause(error)),
    );
    if error.is_connect() || cause.is_some() {
        HttpError::Connection { reason }
    } else {
        HttpError::Other { reason }
    }
}

/// Walk typed sources, never retry by matching provider-controlled error text.
fn connection_cause(error: &(dyn Error + 'static)) -> Option<&'static str> {
    let mut source = error.source();
    let mut http_transport = false;
    while let Some(err) = source {
        if let Some(io) = err.downcast_ref::<std::io::Error>() {
            let cause = match io.kind() {
                ErrorKind::ConnectionRefused => Some("connection refused"),
                ErrorKind::ConnectionReset => Some("connection reset by peer"),
                ErrorKind::ConnectionAborted => Some("connection aborted"),
                ErrorKind::BrokenPipe => Some("connection closed while sending"),
                ErrorKind::UnexpectedEof if http_transport => {
                    Some("connection closed before the response completed")
                }
                ErrorKind::NotConnected => Some("socket is not connected"),
                _ => None,
            };
            if cause.is_some() {
                return cause;
            }
        }
        if let Some(http) = err.downcast_ref::<hyper::Error>() {
            http_transport = true;
            if http.is_incomplete_message() {
                return Some("connection closed before the response completed");
            }
            if http.is_closed() {
                return Some("connection closed");
            }
        }
        source = err.source();
    }
    None
}

fn generic_cause(error: &reqwest::Error) -> &'static str {
    if error.is_connect() {
        "connection establishment failed (check DNS, TLS and service availability)"
    } else if error.is_decode() {
        "response body decoding failed"
    } else if error.is_builder() {
        "invalid HTTP request"
    } else {
        "HTTP exchange failed without a proven connection interruption"
    }
}

/// An endpoint identifies the service, not credentials or private URL contents.
/// Some body errors have no reqwest URL, so callers supply the final vetted URL.
fn endpoint_origin(endpoint: &str) -> String {
    url::Url::parse(endpoint).map_or_else(
        |_| "configured endpoint".to_owned(),
        |url| url.origin().ascii_serialization(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_diagnostic_excludes_userinfo_path_query_and_fragment() {
        assert_eq!(
            endpoint_origin("https://user:secret@localhost:443/private?token=secret#secret"),
            "https://localhost"
        );
        assert_eq!(
            endpoint_origin("http://[::1]:8123/v1/messages"),
            "http://[::1]:8123"
        );
        assert_eq!(endpoint_origin("not a URL"), "configured endpoint");
    }
}
