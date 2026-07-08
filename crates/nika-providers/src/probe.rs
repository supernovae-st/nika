// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider environment probing — key PRESENCE and local-port
//! reachability, owned by the crate that owns the providers.
//!
//! Lifted from the CLI's probe layer when the mirror family (doctor ·
//! welcome · context) pushed `nika-cli` past the 15k prod cap — the cap
//! forcing the better shape again: `env_candidates()` and
//! `effective_base_url()` live HERE, so the detection that reasons over
//! them does too. The CLI keeps the render voices; this module owns the
//! facts.
//!
//! Sovereignty invariants (alignment Rule 1):
//! - Key checks are PRESENT-NOT-PRINTED — `var_os(..).is_some` only,
//!   the value is never bound, so no secret can reach any output.
//! - `collect_local_pings` dials LOCAL surfaces only (loopback seeds or
//!   the operator's own overrides) — never a vendor endpoint, and
//!   nothing is ever written on the socket.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::ProviderRegistry;

/// One `--ping` observation · a local port either answered a TCP connect
/// within the cap or it did not (no request is ever sent on the socket).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PingState {
    /// The port accepted the connection (round-trip in ms).
    Reachable(u64),
    /// Nothing listening (or slower than the 300ms cap).
    Unreachable,
}

/// A provider's environment facts — key PRESENCE only, never the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProbe {
    pub id: String,
    pub requires_key: bool,
    pub key_present: bool,
    /// The conventional env var to name in a fix line (e.g.
    /// `ANTHROPIC_API_KEY`).
    pub fix_var: String,
    /// Whether structured output rides the provider's NATIVE
    /// `json_schema` mode (`Profile::supports_response_format` — a
    /// PROVIDER fact).
    pub structured_native: bool,
}

/// Probe every operator-facing provider in `registry` (the `mock` test
/// backend excluded) — presence booleans and names only.
#[must_use]
pub fn collect_provider_probes(registry: &ProviderRegistry) -> Vec<ProviderProbe> {
    registry
        .profiles()
        .iter()
        .filter(|p| p.id != "mock")
        .map(|p| {
            let candidates = p.env_candidates();
            ProviderProbe {
                id: p.id.to_owned(),
                requires_key: p.requires_key,
                // PRESENT-NOT-PRINTED: only presence is observed · the
                // value is never bound.
                key_present: candidates.iter().any(|v| env_present(v)),
                structured_native: p.supports_response_format(),
                // The conventional var (last candidate) is the
                // friendliest fix; `NIKA_<ID>_API_KEY` always works too
                // but reads less familiar.
                fix_var: candidates
                    .last()
                    .cloned()
                    .unwrap_or_else(|| format!("NIKA_{}_API_KEY", p.id.to_uppercase())),
            }
        })
        .collect()
}

/// Whether an env var is SET and non-empty — PRESENT-NOT-PRINTED. The
/// value is never bound (`var_os(...).is_some`), so no secret can
/// surface. This is the probe layer's sanctioned `std::env` boundary:
/// checking key PRESENCE is not a secret READ, and routing a
/// presence-bool through the kernel vault seam would be ceremony with
/// no payoff.
#[allow(clippy::disallowed_methods)]
#[must_use]
pub fn env_present(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// `host:port` extracted from a base URL, for a connect-only probe.
/// No URL crate: scheme-strip, authority up to the first `/`, default
/// port per scheme. `None` = unparseable (probed as unreachable).
#[must_use]
pub fn ping_addr(url: &str) -> Option<String> {
    let (default_port, rest) = if let Some(r) = url.strip_prefix("https://") {
        ("443", r)
    } else if let Some(r) = url.strip_prefix("http://") {
        ("80", r)
    } else {
        return None;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    // Userinfo is display/credential noise, never part of the dial —
    // `user:pass@host` must resolve (and print) as `host`.
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if authority.is_empty() {
        return None;
    }
    if authority.contains(':') {
        Some(authority.to_owned())
    } else {
        Some(format!("{authority}:{default_port}"))
    }
}

/// Connect-only TCP probe (nothing is ever written on the socket).
///
/// The whole probe — DNS resolution INCLUDED — honours the cap:
/// `to_socket_addrs` is a synchronous OS resolver call with no timeout
/// of its own, so resolution + connect run on a worker thread and the
/// caller waits `recv_timeout(cap)`; on expiry the port is reported
/// unreachable and the worker is left to finish in the background (a
/// one-shot diagnostic can afford one parked thread; it exits with the
/// process). [`collect_local_pings`] starts EVERY probe first and then
/// collects against one shared deadline, so seven surfaces answer in
/// ~one cap total instead of seven caps end to end.
#[must_use]
pub fn spawn_ping(addr: &str, timeout: Duration) -> std::sync::mpsc::Receiver<Option<u64>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let addr = addr.to_owned();
    // The probe is a synchronous one-shot diagnostic (no tokio runtime
    // on this path); plain worker threads are the whole async story.
    #[allow(clippy::disallowed_methods)]
    std::thread::spawn(move || {
        // The round-trip is measured HERE, in the worker: collection-side
        // elapsed was inflated by every earlier slot's wait (a live 3ms
        // ollama reported "(300ms)" behind a dead port — a 100× lie on a
        // diagnostic surface). DNS included: resolution is part of what
        // the operator's run will pay.
        let started = Instant::now(); // seam-bypass-ok: diagnostic RTT measurement · one-shot probe, not workflow time
        // A dropped receiver (cap expired) makes every send a no-op.
        let rtt = addr
            .to_socket_addrs()
            .ok()
            .and_then(|mut candidates| candidates.next())
            .filter(|sock| TcpStream::connect_timeout(sock, timeout).is_ok()) // seam-bypass-ok: connect-only local dial · opt-in ping
            .map(|_| u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        let _ = tx.send(rtt);
    });
    rx
}

/// Ping collection · the LOCAL surfaces only (keyless providers + the
/// media planes when their URL is configured) · 300ms cap per port.
#[must_use]
pub fn collect_local_pings(
    registry: &ProviderRegistry,
    image_url: Option<&str>,
    tts_url: Option<&str>,
) -> Vec<(String, String, PingState)> {
    const CAP: Duration = Duration::from_millis(300);
    // Start every probe first, then collect against ONE shared deadline:
    // the whole sweep answers in ~CAP total (results stay surface-ordered)
    // instead of paying the cap once per dead port.
    let mut pending = Vec::new();
    for p in registry.profiles() {
        if p.requires_key || p.id == "mock" {
            continue;
        }
        // Probe what a run would ACTUALLY hit — the operator override
        // (NIKA_<ID>_BASE_URL · OLLAMA_HOST) when present, else the seed.
        // Pinging 127.0.0.1 while runs talk to the GPU box is an
        // anti-doctor.
        let url = registry.effective_base_url(p.id).unwrap_or(p.base_url);
        if let Some(addr) = ping_addr(url) {
            let rx = spawn_ping(&addr, CAP);
            pending.push((p.id.to_owned(), addr, rx));
        }
    }
    for (id, url) in [("image-local", image_url), ("tts-local", tts_url)] {
        if let Some(addr) = url.and_then(ping_addr) {
            let rx = spawn_ping(&addr, CAP);
            pending.push((id.to_owned(), addr, rx));
        }
    }

    let deadline = Instant::now() + CAP; // seam-bypass-ok: shared ping deadline · diagnostic-only
    pending
        .into_iter()
        .map(|(id, addr, rx)| {
            let budget = deadline.saturating_duration_since(Instant::now()); // seam-bypass-ok: same deadline
            let state = match rx.recv_timeout(budget) {
                Ok(Some(ms)) => PingState::Reachable(ms),
                _ => PingState::Unreachable,
            };
            (id, addr, state)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_addr_extracts_host_and_default_port() {
        assert_eq!(
            ping_addr("http://localhost:11434"),
            Some("localhost:11434".to_owned())
        );
        assert_eq!(
            ping_addr("https://gpu.lan/v1"),
            Some("gpu.lan:443".to_owned())
        );
        // Userinfo never reaches the dial.
        assert_eq!(
            ping_addr("http://user:pass@host:8080/x"),
            Some("host:8080".to_owned())
        );
        assert_eq!(ping_addr("ftp://nope"), None);
        assert_eq!(ping_addr("http://"), None);
    }

    #[test]
    fn env_present_observes_without_binding() {
        assert!(env_present("PATH"), "PATH is set + non-empty");
        assert!(!env_present("NIKA_PROVIDERS_DEFINITELY_UNSET_XYZZY"));
    }
}
