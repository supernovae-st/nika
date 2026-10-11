// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider environment probing — key PRESENCE, endpoint CLASSIFICATION
//! (the readiness ladder · the execution locus) and local-port
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
//! - `probe_model_listing` is a separate opt-in bodyless GET: compatible
//!   schema and advertised model ids, never an inference capability claim.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::ProviderRegistry;

mod model_list;
pub use model_list::{probe_cloud_listing, probe_model_listing};
// The probe's facts live in the size-cap member (D-2026-07-09-N1); their paths stay here.
pub use nika_providers_access::probe::{
    AccessClass, ExecutionLocus, KeyAuth, ModelListing, PingState, ProviderProbe,
    ProviderReadiness, classify_http_status, classify_key_value,
};

/// One derivation for a shipped agentic CLI row (CLI doctor + compose).
/// `key_present` is the ACP speaker; `configured` is the harness login;
/// `fix_var` is the dummy-readable install / sign-in line.
#[must_use]
pub fn harness_access_probe(
    id: impl Into<String>,
    serves: Vec<String>,
    acp_present: bool,
    configured: bool,
    product_present: bool,
) -> ProviderProbe {
    let id = id.into();
    let rt = nika_types::access::HarnessRuntime::lookup(&id);
    let fix = match (product_present, acp_present, configured) {
        (false, false, _) => rt.map_or_else(
            || format!("{id} is not installed. Install it or pick Nika local / Nika Cloud."),
            |r| r.not_installed.to_owned(),
        ),
        (true, false, _) => rt.map_or_else(
            || {
                format!(
                    "{id} is installed, but Nika talks to it through ACP. \
                     Install the ACP speaker or pick Nika local / Nika Cloud."
                )
            },
            |rt| rt.acp_missing(),
        ),
        (_, true, false) => rt.map_or_else(
            || format!("sign in to `{id}` itself"),
            |rt| rt.not_signed_in(),
        ),
        (_, true, true) => String::new(),
    };
    let readiness = ProviderReadiness::new(
        true,
        configured,
        None,
        None,
        false,
        ExecutionLocus::Loopback,
        nika_types::access::AccessClass::Harness,
    );
    ProviderProbe::new(id, false, acp_present, fix, false, readiness, "")
        .with_serves(serves)
        .with_product_present(product_present)
}

/// HTTP provider rows plus PATH-only harness rows when `access-harness` is on.
/// Compose and CLI admission share this so `--access claude-code` cannot 1802.
#[must_use]
pub fn collect_access_probes(registry: &ProviderRegistry) -> Vec<ProviderProbe> {
    #[cfg(not(feature = "access-harness"))]
    {
        collect_provider_probes(registry)
    }
    #[cfg(feature = "access-harness")]
    {
        let mut probes = collect_provider_probes(registry);
        if let Ok(rows) = nika_harness::registry() {
            probes.extend(nika_harness::presence_facts(rows).into_iter().map(|f| {
                harness_access_probe(
                    f.id,
                    f.serves,
                    f.acp_present,
                    f.configured,
                    f.product_present,
                )
            }));
        }
        probes
    }
}

/// [`collect_access_probes`] over a no-http registry view of `config`.
#[must_use]
pub fn collect_access_probes_env(config: crate::ProvidersConfig) -> Vec<ProviderProbe> {
    collect_access_probes(&crate::ProviderRegistry::without_http(config))
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
            // PRESENT-NOT-PRINTED: only presence is observed · the
            // value is never bound.
            let key_present = candidates.iter().any(|v| env_present(v));
            // The URL a run would ACTUALLY hit — the operator override
            // when present, else the seed (the registry already folds).
            let endpoint = registry.effective_base_url(p.id).unwrap_or(p.base_url);
            ProviderProbe::new(
                p.id.to_owned(),
                p.requires_key,
                key_present,
                // The conventional var (last candidate) is the
                // friendliest fix; `NIKA_<ID>_API_KEY` always works too
                // but reads less familiar.
                candidates
                    .last()
                    .cloned()
                    .unwrap_or_else(|| format!("NIKA_{}_API_KEY", p.id.to_uppercase())),
                p.supports_response_format(),
                ProviderReadiness::new(
                    true,
                    !p.requires_key || key_present,
                    None,
                    None,
                    nika_catalog::all_pricing()
                        .iter()
                        .any(|r| r.provider.eq_ignore_ascii_case(p.id)),
                    ExecutionLocus::classify(Some(endpoint), p.base_url),
                    p.access_class(),
                ),
                endpoint.to_owned(),
            )
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

/// `host:port` for a connect-only probe, from the WHATWG parse the
/// transport connects with (`url::Url`), so the probe dials what the run
/// would hit: a validated host (IPv6 in brackets) and the explicit or
/// default `http`/`https` port. Path, query, fragment and userinfo never
/// reach the dial or the text that prints it. `None` = another scheme or
/// an unparseable URL: nothing is dialed (the doctor lists no ping and the
/// run gate does not apply).
#[must_use]
pub fn ping_addr(url: &str) -> Option<String> {
    let url = url::Url::parse(url).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let port = url.port_or_known_default()?;
    Some(format!("{}:{port}", url.host_str()?))
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

fn receive_ping_state(rx: &std::sync::mpsc::Receiver<Option<u64>>, budget: Duration) -> PingState {
    match rx.recv_timeout(budget) {
        Ok(Some(ms)) => PingState::Reachable(ms),
        _ => PingState::Unreachable,
    }
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
            let state = receive_ping_state(&rx, budget);
            (id, addr, state)
        })
        .collect()
}

/// The B-5 liveness verdict (the 2026-07-31 gauntlet hang · Marta 2
/// min, Dmitri 120s, Zoe at cutoff): the two ways a local endpoint
/// hangs a run, told apart. `Silent` — nothing answered the connect
/// (dead · blackholed · DNS stalled); `Mute` — the port ACCEPTED but
/// never spoke HTTP (a stuck listener: connect-only probes pass it,
/// the request then hangs forever); `Live` — the port answered AND
/// spoke inside the caps (round-trip ms). A 404 speaks as loud as a
/// 200: liveness is « the server answers », never « the route exists ».
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalLiveness {
    /// Connect + a `GET /` byte, both inside the caps (RTT ms).
    Live(u64),
    /// The port accepted but never spoke (the mute-server hang).
    Mute(String),
    /// Nothing answered the connect (dead · blackholed · slow DNS).
    Silent(String),
}

/// Connect-only? Not enough for the RUN gate: the gauntlet's mute
/// server accepts and says nothing, and the request then hangs with
/// zero timeout (`infer` defaults to `timeout: None`). This probe goes
/// ONE step past [`spawn_ping`]'s sovereignty invariant — a bare
/// `GET /` with no headers, no body, no path — against the endpoint the
/// run is about to POST a whole prompt to anyway. Connect cap 300ms
/// (the doctor sweep's own), speak cap 1s (a loaded local engine still
/// answers its HTTP loop in ms — inference runs off the accept loop).
/// DNS included, on the worker, exactly like [`spawn_ping`].
#[must_use]
pub fn spawn_local_liveness(addr: &str) -> std::sync::mpsc::Receiver<LocalLiveness> {
    use std::io::{Read, Write};

    const CONNECT_CAP: Duration = Duration::from_millis(300);
    const SPEAK_CAP: Duration = Duration::from_millis(1000);

    let (tx, rx) = std::sync::mpsc::channel();
    let addr = addr.to_owned();
    #[allow(clippy::disallowed_methods)]
    std::thread::spawn(move || {
        let started = Instant::now(); // seam-bypass-ok: diagnostic RTT · one-shot probe, not workflow time
        let verdict = addr
            .to_socket_addrs()
            .ok()
            .and_then(|mut candidates| candidates.next())
            .map_or_else(
                || LocalLiveness::Silent(addr.clone()),
                |sock| {
                    let dial = TcpStream::connect_timeout(&sock, CONNECT_CAP); // seam-bypass-ok: run-gate dial · the endpoint the run is about to call
                    match dial {
                        Err(_) => LocalLiveness::Silent(addr.clone()),
                        Ok(mut stream) => {
                            let rtt =
                                u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                            let _ = stream.set_write_timeout(Some(SPEAK_CAP));
                            let _ = stream.set_read_timeout(Some(SPEAK_CAP));
                            let spoke = stream
                                .write_all(b"GET / HTTP/1.0\r\n\r\n")
                                .and_then(|()| stream.read(&mut [0_u8; 1]));
                            match spoke {
                                Ok(_) => LocalLiveness::Live(rtt),
                                Err(_) => LocalLiveness::Mute(addr.clone()),
                            }
                        }
                    }
                },
            );
        let _ = tx.send(verdict);
    });
    rx
}

/// The B-5 run gate: one liveness verdict on a local-protocol model's
/// EFFECTIVE endpoint, consulted BEFORE the first call. A server-backed
/// keyless engine (`ollama/…`) whose port stays silent — or mute —
/// fails FAST here, the caller naming the cause and the exits, instead
/// of heartbeat-spamming « still running » until the user kills the
/// run. `None` = the gate does not apply (a keyed cloud provider owns
/// its transport errors · `mock` is in-process, there is nothing to
/// dial · an unknown provider is the resolver's word, not this gate's).
#[must_use]
pub fn local_run_gate<H>(registry: &ProviderRegistry<H>, model: &str) -> Option<LocalLiveness>
where
    H: nika_kernel::http::HttpPostDyn + Send + Sync + 'static,
{
    let (provider_id, _) = model.split_once('/')?;
    let provider_id = crate::profile::canonical_provider(provider_id);
    let profile = registry.profiles().iter().find(|p| p.id == provider_id)?;
    // The gate mirrors collect_local_pings' own filter: server-backed
    // keyless engines only.
    if profile.requires_key || profile.id == "mock" {
        return None;
    }
    // Probe what the run would ACTUALLY hit (the anti-doctor law above):
    // the operator's override when present, else the seed.
    let url = registry
        .effective_base_url(profile.id)
        .unwrap_or(profile.base_url);
    let addr = ping_addr(url)?;
    // The caller's own cap is the recv_timeout: connect + speak phases
    // inside the worker each carry theirs (300ms · 1s).
    Some(
        spawn_local_liveness(&addr)
            .recv_timeout(Duration::from_millis(1500))
            .unwrap_or(LocalLiveness::Silent(addr)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receive_ping_state_classifies_every_channel_outcome() {
        let (reachable_tx, reachable_rx) = std::sync::mpsc::channel();
        reachable_tx.send(Some(7)).expect("reachable result");
        assert_eq!(
            receive_ping_state(&reachable_rx, Duration::ZERO),
            PingState::Reachable(7)
        );

        let (refused_tx, refused_rx) = std::sync::mpsc::channel();
        refused_tx.send(None).expect("refused result");
        assert_eq!(
            receive_ping_state(&refused_rx, Duration::ZERO),
            PingState::Unreachable
        );

        let (waiting_tx, waiting_rx) = std::sync::mpsc::channel();
        assert_eq!(
            receive_ping_state(&waiting_rx, Duration::ZERO),
            PingState::Unreachable
        );
        drop(waiting_tx);

        let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();
        drop(dropped_tx);
        assert_eq!(
            receive_ping_state(&dropped_rx, Duration::ZERO),
            PingState::Unreachable
        );
    }

    #[test]
    fn spawn_ping_reaches_a_held_loopback_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr").to_string();
        let rx = spawn_ping(&addr, Duration::from_millis(300));
        assert!(matches!(
            receive_ping_state(&rx, Duration::from_millis(300)),
            PingState::Reachable(_)
        ));
    }

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
    fn ping_addr_dials_only_the_host_and_port_the_transport_parses() {
        // A backslash, query or fragment right after the authority used to
        // ride the dial address into the gate's refusal text (E32).
        for (url, dial) in [
            ("http://127.0.0.1:38433\\sentinel\\v1", "127.0.0.1:38433"),
            ("http://127.0.0.1:38433?k=sentinel", "127.0.0.1:38433"),
            ("http://127.0.0.1:38433#sentinel", "127.0.0.1:38433"),
            ("http://127.0.0.1:38433/%73entinel/v1", "127.0.0.1:38433"),
            ("http://[::1]:8080/sentinel", "[::1]:8080"),
            ("https://[::1]/sentinel", "[::1]:443"),
            ("HTTP://LocalHost:11434/sentinel", "localhost:11434"),
            ("http://user:sentinel@host/v1", "host:80"),
        ] {
            assert_eq!(ping_addr(url).as_deref(), Some(dial), "{url}");
        }
        for url in [
            "file:///sentinel",
            "ws://host:1/sentinel",
            "http://:8080/sentinel",
            "http://[::1",
            "sentinel",
            "http://host:99999/sentinel",
        ] {
            assert_eq!(ping_addr(url), None, "{url}");
        }
    }

    #[test]
    fn env_present_observes_without_binding() {
        assert!(env_present("PATH"), "PATH is set + non-empty");
        assert!(!env_present("NIKA_PROVIDERS_DEFINITELY_UNSET_XYZZY"));
    }

    #[test]
    fn locus_classifies_loopback_lan_remote_and_cloud() {
        use ExecutionLocus as L;
        let ollama_seed = "http://127.0.0.1:11434";
        // Loopback literals — the sovereign default (v4 · name · v6).
        assert_eq!(
            L::classify(Some("http://127.0.0.1:11434"), ollama_seed),
            L::Loopback
        );
        assert_eq!(
            L::classify(Some("http://localhost:11434/v1"), ollama_seed),
            L::Loopback
        );
        assert_eq!(
            L::classify(Some("http://[::1]:11434"), ollama_seed),
            L::Loopback
        );
        // RFC1918 literals + local hostnames — the operator's own
        // network, off-box: « local » is a PROTOCOL, not a topology.
        assert_eq!(
            L::classify(Some("http://192.168.1.20:11434"), ollama_seed),
            L::Lan
        );
        assert_eq!(
            L::classify(Some("http://10.0.0.5:11434"), ollama_seed),
            L::Lan
        );
        assert_eq!(
            L::classify(Some("http://172.16.0.9:11434"), ollama_seed),
            L::Lan
        );
        assert_eq!(
            L::classify(Some("http://gpu.lan:11434"), ollama_seed),
            L::Lan
        );
        assert_eq!(
            L::classify(Some("http://gpu-box:11434"), ollama_seed),
            L::Lan,
            "a bare hostname names the operator's own network"
        );
        // A public address off-box is REMOTE (not the vendor default).
        assert_eq!(
            L::classify(Some("http://8.8.8.8:11434"), ollama_seed),
            L::Remote
        );
        // The declared vendor endpoint is the cloud — absent override
        // classifies the DEFAULT.
        let mistral_seed = "https://api.mistral.ai/v1/chat/completions";
        assert_eq!(L::classify(None, mistral_seed), L::Cloud);
        assert_eq!(L::classify(Some(mistral_seed), mistral_seed), L::Cloud);
        // A keyed provider overridden to a proxy is never laundered « cloud ».
        assert_eq!(
            L::classify(Some("https://proxy.corp.example/v1"), mistral_seed),
            L::Remote
        );
        assert_eq!(
            L::classify(Some("http://10.1.2.3:8080/v1"), mistral_seed),
            L::Lan
        );
        // Absent override → the declared default (a local seed is loopback).
        assert_eq!(L::classify(None, ollama_seed), L::Loopback);
        // Unparseable — never guessed.
        assert_eq!(L::classify(Some("nope"), ollama_seed), L::Unknown);
        // Userinfo never reaches the classification.
        assert_eq!(
            L::classify(Some("http://user:pass@192.168.1.20:11434"), ollama_seed),
            L::Lan
        );
    }

    #[test]
    fn readiness_is_a_ladder_not_a_boolean() {
        let reg = ProviderRegistry::without_http(crate::ProvidersConfig::new());
        let probes = collect_provider_probes(&reg);
        let ollama = probes.iter().find(|p| p.id == "ollama").expect("seeded");
        // RECOGNIZED (the name resolves) · CONFIGURED (keyless) — and the
        // endpoint defaults to loopback…
        assert!(ollama.readiness.recognized);
        assert!(ollama.readiness.configured);
        assert_eq!(ollama.readiness.execution_locus, ExecutionLocus::Loopback);
        // …but REACHABILITY and MODEL availability are NOT claimed: no
        // ping ran, no model listing was fetched — `None` is the honest
        // state, « recognized » is never rendered as « ready » (P0-5).
        assert_eq!(ollama.readiness.reachable, None);
        assert_eq!(ollama.readiness.model_available, None);
        assert!(!ollama.readiness.priced, "a local engine has no list rates");
        let anthropic = probes.iter().find(|p| p.id == "anthropic").expect("seeded");
        assert!(anthropic.readiness.recognized);
        assert_eq!(
            anthropic.readiness.configured, anthropic.key_present,
            "a keyed provider is configured exactly when its key is present"
        );
        assert_eq!(anthropic.readiness.execution_locus, ExecutionLocus::Cloud);
        assert!(
            anthropic.readiness.priced,
            "the vendored snapshot prices anthropic"
        );
    }

    #[test]
    fn an_endpoint_override_moves_the_locus_off_loopback() {
        // What `NIKA_OLLAMA_BASE_URL=http://gpu.lan:11434` becomes through
        // the compose ladder — injected as config (no `set_var` race).
        let config = crate::ProvidersConfig::new().with_base_url("ollama", "http://gpu.lan:11434");
        let reg = ProviderRegistry::without_http(config);
        let probes = collect_provider_probes(&reg);
        let ollama = probes.iter().find(|p| p.id == "ollama").expect("seeded");
        assert_eq!(ollama.readiness.execution_locus, ExecutionLocus::Lan);
        assert_eq!(ollama.endpoint, "http://gpu.lan:11434");
        let lmstudio = probes.iter().find(|p| p.id == "lmstudio").expect("seeded");
        assert_eq!(
            lmstudio.readiness.execution_locus,
            ExecutionLocus::Loopback,
            "an override on one engine never moves another"
        );
    }

    /// A port nothing listens on anymore (bound, then dropped — never
    /// dialed, so no `TIME_WAIT` ghost): the deterministic « silent ».
    #[allow(clippy::disallowed_methods)] // test seam — the probe's own worker pattern
    fn dead_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        port
    }

    /// A one-shot loopback server: accepts and behaves per `speak`
    /// (true = answers a bare 404, false = holds the socket mute).
    #[allow(clippy::disallowed_methods)] // test seam — the probe's own worker pattern
    fn spawn_stub_server(speak: bool) -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                std::thread::spawn(move || {
                    if speak {
                        use std::io::Write;
                        let _ = stream.write_all(b"HTTP/1.0 404 Not Found\r\n\r\n");
                    } else {
                        std::thread::sleep(std::time::Duration::from_secs(30));
                        drop(stream);
                    }
                });
            }
        });
        port
    }

    #[test]
    fn local_run_gate_skips_cloud_mock_and_unknown() {
        let reg = ProviderRegistry::without_http(crate::ProvidersConfig::new());
        assert!(
            local_run_gate(&reg, "anthropic/claude-sonnet-4-6").is_none(),
            "a keyed cloud provider owns its transport errors"
        );
        assert!(
            local_run_gate(&reg, "mock/echo").is_none(),
            "in-process — there is nothing to dial"
        );
        assert!(
            local_run_gate(&reg, "ghost/x").is_none(),
            "an unknown provider is the resolver's word"
        );
        assert!(
            local_run_gate(&reg, "bare-id").is_none(),
            "no provider half"
        );
    }

    #[test]
    fn local_run_gate_reads_silence_muteness_and_liveness() {
        // SILENT — the port answers nothing. A dropped ephemeral port can be
        // re-bound by any process in the window before the probe (it was, once,
        // under a full oracle run beside this suite): a fresh port is tried a
        // few times and Silent must be observed at least once.
        let silent = (0..5).any(|_| {
            let dead = crate::ProvidersConfig::new()
                .with_base_url("ollama", format!("http://127.0.0.1:{}", dead_port()));
            let reg = ProviderRegistry::without_http(dead);
            matches!(
                local_run_gate(&reg, "ollama/qwen3.5:4b"),
                Some(LocalLiveness::Silent(_))
            )
        });
        assert!(silent, "a dead port reads Silent");
        // MUTE — the port accepts and never speaks (the gauntlet hang).
        let mute = crate::ProvidersConfig::new().with_base_url(
            "ollama",
            format!("http://127.0.0.1:{}", spawn_stub_server(false)),
        );
        let reg = ProviderRegistry::without_http(mute);
        assert!(
            matches!(
                local_run_gate(&reg, "ollama/qwen3.5:4b"),
                Some(LocalLiveness::Mute(_))
            ),
            "a stuck listener reads Mute"
        );
        // LIVE — the port speaks (a 404 is as alive as a 200).
        let live = crate::ProvidersConfig::new().with_base_url(
            "ollama",
            format!("http://127.0.0.1:{}", spawn_stub_server(true)),
        );
        let reg = ProviderRegistry::without_http(live);
        assert!(
            matches!(
                local_run_gate(&reg, "ollama/qwen3.5:4b"),
                Some(LocalLiveness::Live(_))
            ),
            "a speaking server reads Live"
        );
    }

    #[test]
    fn a_harness_access_probe_encodes_install_vs_acp_vs_signin() {
        let missing =
            harness_access_probe("claude-code", vec!["anthropic".into()], false, false, false);
        assert_eq!(
            missing.readiness.access,
            nika_types::access::AccessClass::Harness
        );
        assert!(!missing.key_present);
        assert!(missing.fix_var.contains("Claude Code is not installed"));

        let no_acp =
            harness_access_probe("claude-code", vec!["anthropic".into()], false, false, true);
        assert!(
            no_acp.fix_var.contains("claude-agent-acp"),
            "{}",
            no_acp.fix_var
        );

        let ready = harness_access_probe("claude-code", vec!["anthropic".into()], true, true, true);
        assert!(ready.key_present);
        assert!(ready.readiness.configured);
        assert!(ready.fix_var.is_empty());
    }

    /// B19 / issue 1273: `sk-invalid` is present, not ready/configured.
    /// 401 and 402 are distinct rungs.
    #[test]
    fn doctor_distinguishes_present_401_and_402() {
        assert_eq!(classify_key_value(&["sk-"], ""), KeyAuth::Absent);
        assert_eq!(
            classify_key_value(&["sk-"], "sk-invalid"),
            KeyAuth::Implausible
        );
        assert!(!classify_key_value(&["sk-"], "sk-invalid").is_ready());
        assert_eq!(
            classify_key_value(&["sk-"], "sk-abcdefghijklmnopqrstuvwxyz"),
            KeyAuth::Present
        );
        assert_eq!(
            classify_key_value(&["sk-ant-"], "sk-abcdefghijklmnopqrstuvwxyz"),
            KeyAuth::Implausible
        );
        assert_eq!(classify_http_status(401), KeyAuth::Unauthorized);
        assert_eq!(classify_http_status(402), KeyAuth::PaymentRequired);
        assert_eq!(classify_http_status(403), KeyAuth::Forbidden);
        assert_eq!(KeyAuth::Forbidden.as_str(), "403");
        assert!(!KeyAuth::Forbidden.is_ready());
        // Only a listing's 401 or 403 refuses a key; presence alone, or no answer, never does.
        let mut readiness = ProviderReadiness::new(
            true,
            true,
            None,
            None,
            true,
            ExecutionLocus::Cloud,
            AccessClass::Api,
        );
        assert!(!readiness.key_refused(), "configured, not validated");
        for (rung, refused) in [
            (KeyAuth::Ok, false),
            (KeyAuth::Present, false),
            (KeyAuth::Unauthorized, true),
            (KeyAuth::Forbidden, true),
        ] {
            readiness.key_auth = Some(rung);
            assert!(readiness.key_refused() == refused, "{rung:?}");
        }
        assert_eq!(classify_http_status(200), KeyAuth::Ok);
        assert_eq!(KeyAuth::Unauthorized.as_str(), "401");
        assert_eq!(KeyAuth::PaymentRequired.as_str(), "402");
        assert_eq!(KeyAuth::Implausible.as_str(), "implausible");
        assert!(!KeyAuth::Unauthorized.is_ready());
        assert!(!KeyAuth::PaymentRequired.is_ready());
        assert!(KeyAuth::Ok.is_ready());
    }

    // The access candidates read the probe rows this crate collects (moved with the probes
    // when resolve_access became the size-cap member's).
    #[test]
    fn candidates_bridge_reads_the_probe_truth() {
        use crate::registry::{ProviderRegistry, ProvidersConfig};
        use crate::resolve_access::candidates_for;
        let registry = ProviderRegistry::without_http(ProvidersConfig::new());
        let probes = crate::probe::collect_provider_probes(&registry);
        let candidates = candidates_for(&probes, "ollama");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].access, "ollama");
        assert_eq!(candidates[0].class, AccessClass::Local);
        // Keyless local: no fix var to teach.
        assert!(candidates[0].fix_var.is_none());
        let candidates = candidates_for(&probes, "mistral");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].class, AccessClass::Api);
        assert!(candidates[0].fix_var.is_some());
    }
}
