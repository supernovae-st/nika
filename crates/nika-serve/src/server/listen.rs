// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::net::SocketAddr;

/// Operator stderr after bind. Next hops live here, not on GET /health
/// (ADR-117 identity allowlist).
pub(crate) fn listen_line(addr: SocketAddr) -> String {
    let hops =
        "GET /health · GET /v1/openapi.json (Bearer) · POST /v1/jobs (Bearer + Idempotency-Key)";
    if addr.ip().is_loopback() {
        format!("nika serve · listening http://{addr} · {hops}")
    } else {
        format!(
            "nika serve · listening http://{addr} · {hops}\n  non-loopback · blast radius is every workflow in --workflows · auth unchanged"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn listen_line_names_the_authenticated_next_hops() {
        let loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8787);
        let line = listen_line(loopback);
        assert!(line.contains("http://127.0.0.1:8787"), "{line}");
        assert!(line.contains("GET /health"), "{line}");
        assert!(line.contains("GET /v1/openapi.json"), "{line}");
        assert!(line.contains("Bearer"), "{line}");
        assert!(line.contains("POST /v1/jobs"), "{line}");
        assert!(line.contains("Idempotency-Key"), "{line}");
        assert!(!line.contains("blast radius"), "{line}");

        let remote = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8787);
        let line = listen_line(remote);
        assert!(line.contains("0.0.0.0:8787"), "{line}");
        assert!(line.contains("blast radius"), "{line}");
        assert!(line.contains("--workflows"), "{line}");
    }
}
