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

/// Where an EFFECTIVE endpoint executes the inference — the P0-20 fact:
/// « local » is a PROTOCOL (no key · an OpenAI-compat wire), never a
/// topology. Derived from the URL a run would ACTUALLY hit (the operator
/// override when present, else the profile seed) — never from
/// `requires_key`, so a LAN-pointed ollama can no longer render as
/// « zero network ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionLocus {
    /// A loopback literal (`localhost` · 127/8 · `::1`) — zero network.
    Loopback,
    /// RFC1918 / link-local / a local hostname (`gpu.lan` · a bare
    /// name) — the operator's own network, off-box.
    Lan,
    /// An off-box endpoint that is NOT the provider's declared default
    /// (an override pointing at a public address).
    Remote,
    /// The provider's declared vendor endpoint (the catalog default).
    Cloud,
    /// Unparseable — never guessed.
    Unknown,
}

impl ExecutionLocus {
    /// The render token every surface shares (`lan` · `remote` …).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::Lan => "lan",
            Self::Remote => "remote",
            Self::Cloud => "cloud",
            Self::Unknown => "unknown",
        }
    }

    /// Classify the EFFECTIVE endpoint against the provider's declared
    /// default. `None` (no override) classifies the default itself — a
    /// local seed reads `Loopback`, a vendor seed reads `Cloud`.
    #[must_use]
    pub fn classify(effective: Option<&str>, default: &str) -> Self {
        let url = effective.unwrap_or(default);
        let Some(host) = endpoint_host(url) else {
            return Self::Unknown;
        };
        if nika_types::net::is_exact_loopback_literal(host) {
            return Self::Loopback;
        }
        if is_lan_host(host) {
            return Self::Lan;
        }
        // The vendor's own declared endpoint is the cloud; any OTHER
        // off-box address is an operator-pointed remote.
        match endpoint_host(default) {
            Some(declared) if declared.eq_ignore_ascii_case(host) => Self::Cloud,
            _ => Self::Remote,
        }
    }
}

/// The HOST of a base URL (no port · v6 keeps its brackets) — the same
/// scheme-strip / authority / userinfo grammar as [`ping_addr`], minus
/// the port. `None` = unparseable (classifies [`ExecutionLocus::Unknown`]).
fn endpoint_host(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split('/').next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if authority.is_empty() {
        return None;
    }
    if authority.starts_with('[') {
        let end = authority.find(']')?;
        return authority.get(..=end);
    }
    authority.split(':').next().filter(|h| !h.is_empty())
}

/// RFC1918 / link-local / ULA literal, or a hostname that can only name
/// the operator's own network (a bare name · `.lan` · `.local` ·
/// `.internal`). Public DNS names are NOT Lan — the loopback call is
/// [`nika_types::net::is_exact_loopback_literal`]'s, upstream of this.
fn is_lan_host(host: &str) -> bool {
    // seam-bypass-ok: pure parse + range math on a held string, zero OS reach
    use std::net::IpAddr;

    let literal = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = literal.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
            IpAddr::V6(v6) => {
                let s = v6.segments();
                // fc00::/7 (unique local) · fe80::/10 (link-local).
                (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
            }
        };
    }
    let name = host.trim_end_matches('.');
    !name.contains('.')
        || [".lan", ".local", ".internal"]
            .iter()
            .any(|tld| name.ends_with(tld))
}

/// The access-class vocabulary rides with the ladder (probe consumers
/// read both from here · L0 re-export, strictly downward).
pub use nika_types::access::AccessClass;

/// Readiness as a LADDER, never a boolean — the P0-5 audit: « modèle
/// reconnu » was rendered as « prêt ». Every rung is its own fact, so no
/// surface ever has to conflate them again; the rungs a probe did not
/// measure stay `None`, never a hopeful default.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProviderReadiness {
    /// The id resolves in the canonical registry (a NAME fact).
    pub recognized: bool,
    /// Key present when the provider requires one (keyless: always) —
    /// CONFIGURED, never « reachable », never « the model is pulled ».
    pub configured: bool,
    /// Filled ONLY by an explicit opt-in probe (`doctor --ping`) —
    /// `None` without one: no implicit network, ever.
    pub reachable: Option<bool>,
    /// A model-listing probe against the live endpoint — NOT WIRED yet;
    /// the rung exists so « configured » is never sold as « the model is
    /// there ».
    pub model_available: Option<bool>,
    /// The vendored pricing snapshot carries list rates for this provider.
    pub priced: bool,
    /// Where the EFFECTIVE endpoint executes (override-aware · P0-20).
    pub execution_locus: ExecutionLocus,
    /// HOW this provider is reached (D-2026-08-04-N1) — the profile's
    /// access class (`local` · `api` · `mock`). A `harness` row arrives
    /// as a probe row of its OWN (the adapter id · `serves` set · R-5c),
    /// still never from a profile; `oauth` lands with its adapter.
    pub access: nika_types::access::AccessClass,
}

impl ProviderReadiness {
    /// Construct (INV-019) — every rung named, in ladder order.
    #[must_use]
    pub fn new(
        recognized: bool,
        configured: bool,
        reachable: Option<bool>,
        model_available: Option<bool>,
        priced: bool,
        execution_locus: ExecutionLocus,
        access: nika_types::access::AccessClass,
    ) -> Self {
        Self {
            recognized,
            configured,
            reachable,
            model_available,
            priced,
            execution_locus,
            access,
        }
    }
}

/// A provider's environment facts — key PRESENCE only, never the value.
/// A harness adapter rides the SAME row (R-5c · one channel, one
/// derivation): the adapter id in `id`, the providers it fronts in
/// `serves`, `readiness.access == AccessClass::Harness`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    /// The readiness ladder (P0-5) — presence/classification facts only;
    /// `reachable` / `model_available` stay `None` until an opt-in probe.
    pub readiness: ProviderReadiness,
    /// The EFFECTIVE base URL a run would hit (override-aware) — config,
    /// not a credential; renderers redact userinfo before printing.
    pub endpoint: String,
    /// The provider ids a HARNESS-class row serves (R-5c) — `["openai"]`
    /// for `codex`. EMPTY on a provider row: a profile serves
    /// exactly its own id, and the resolver reads that from `id`.
    pub serves: Vec<String>,
}

impl ProviderProbe {
    /// Construct (INV-019) — a provider row (`serves` stays empty; a
    /// harness row names them via [`with_serves`](Self::with_serves)).
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        requires_key: bool,
        key_present: bool,
        fix_var: impl Into<String>,
        structured_native: bool,
        readiness: ProviderReadiness,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            requires_key,
            key_present,
            fix_var: fix_var.into(),
            structured_native,
            readiness,
            endpoint: endpoint.into(),
            serves: Vec::new(),
        }
    }

    /// Name the provider ids a harness-class row serves (R-5c).
    #[must_use]
    pub fn with_serves(mut self, serves: Vec<String>) -> Self {
        self.serves = serves;
        self
    }
}

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
    ProviderProbe::new(id, false, acp_present, fix, false, readiness, "").with_serves(serves)
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

/// What doctor (and any other surface) must distinguish for a cloud key
/// (B19 / B26 / B27 / I11 / issue 1273): present is not ready, 401 is
/// not 402, `sk-invalid` is not configured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyAuth {
    /// No env var is set.
    Absent,
    /// A value is set; format was not judged / HTTP was not dialed.
    Present,
    /// Set, but the value fails the catalog prefix or a minimum length
    /// (`sk-invalid` is this class — never « ready »).
    Implausible,
    /// Live probe: HTTP 401.
    Unauthorized,
    /// Live probe: HTTP 402 (quota / billing).
    PaymentRequired,
    /// Live probe: the endpoint accepted the key (2xx).
    Ok,
}

impl KeyAuth {
    /// The doctor/JSON token (`present` · `401` · `402` · …).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Present => "present",
            Self::Implausible => "implausible",
            Self::Unauthorized => "401",
            Self::PaymentRequired => "402",
            Self::Ok => "ok",
        }
    }

    /// Whether doctor may render this as `configured` / `ready`.
    #[must_use]
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ok | Self::Present)
    }
}

/// Classify a held key value against the catalog prefixes. The value is
/// the caller's to drop — this function never prints it.
#[must_use]
pub fn classify_key_value(prefixes: &[&str], value: &str) -> KeyAuth {
    if value.is_empty() {
        return KeyAuth::Absent;
    }
    let prefix_ok = prefixes.is_empty() || prefixes.iter().any(|p| value.starts_with(p));
    // OpenAI `sk-` / Anthropic `sk-ant-` live keys are far longer than
    // 16; `sk-invalid` (10) is the gauntlet's present-but-fake specimen.
    if !prefix_ok || value.len() < 16 {
        return KeyAuth::Implausible;
    }
    KeyAuth::Present
}

/// Classify a live auth probe's HTTP status. 401 and 402 are first-class
/// rungs; anything else stays `Present` (the value existed, the probe
/// did not prove Ok).
#[must_use]
pub fn classify_http_status(status: u16) -> KeyAuth {
    match status {
        200..=299 => KeyAuth::Ok,
        401 => KeyAuth::Unauthorized,
        402 => KeyAuth::PaymentRequired,
        _ => KeyAuth::Present,
    }
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
        // SILENT — the port answers nothing.
        let dead = crate::ProvidersConfig::new()
            .with_base_url("ollama", format!("http://127.0.0.1:{}", dead_port()));
        let reg = ProviderRegistry::without_http(dead);
        assert!(
            matches!(
                local_run_gate(&reg, "ollama/qwen3.5:4b"),
                Some(LocalLiveness::Silent(_))
            ),
            "a dead port reads Silent"
        );
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
        assert_eq!(classify_http_status(200), KeyAuth::Ok);
        assert_eq!(KeyAuth::Unauthorized.as_str(), "401");
        assert_eq!(KeyAuth::PaymentRequired.as_str(), "402");
        assert_eq!(KeyAuth::Implausible.as_str(), "implausible");
        assert!(!KeyAuth::Unauthorized.is_ready());
        assert!(!KeyAuth::PaymentRequired.is_ready());
        assert!(KeyAuth::Ok.is_ready());
    }
}
