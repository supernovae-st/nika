// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The environment probe — the ONE detection engine under `nika doctor`
//! and `nika welcome` (one truth, two voices: doctor diagnoses
//! exhaustively, welcome greets and routes).
//!
//! Everything here observes PRESENCE, never values: provider keys are
//! checked `is_set`-only (the value is never bound, so no secret can
//! reach stdout / stderr / a trace — alignment Rule 1). No network BY
//! DEFAULT; `collect(ping: true)` TCP-probes the LOCAL provider ports
//! only (loopback defaults or the operator's own `NIKA_*_LOCAL_URL`),
//! never a vendor endpoint, never a request body, 300ms cap per port.

use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nika_providers::ProviderRegistry;

use crate::verbs::trace::retention::RetentionConfig;
use serde_json::Value;

/// One `--ping` observation · a local port either answered a TCP connect
/// within the cap or it did not (no request is ever sent on the socket).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PingState {
    /// The port accepted the connection (round-trip in ms).
    Reachable(u64),
    /// Nothing listening (or slower than the 300ms cap).
    Unreachable,
}

/// A provider's environment facts — key PRESENCE only, never the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderProbe {
    pub id: String,
    pub requires_key: bool,
    pub key_present: bool,
    /// The conventional env var to name in the fix (e.g. `ANTHROPIC_API_KEY`).
    pub fix_var: String,
    /// Whether structured output rides the provider's NATIVE `json_schema`
    /// mode (`Profile::supports_response_format` — a PROVIDER fact:
    /// deepseek is the one cloud whose schema path is the instruction
    /// fallback + local validation).
    pub structured_native: bool,
}

/// The injected environment facts `diagnose` reasons over — PURE · testable.
/// The CLI fills it from the real env; tests pass synthetic probes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Probe {
    pub version: String,
    /// `Some(path)` when a user config file exists.
    pub config_path: Option<String>,
    pub providers: Vec<ProviderProbe>,
    pub clients: Vec<ClientProbe>,
    /// The `nika:image_generate` plane — key/URL PRESENCE only.
    pub image: ImageProbe,
    /// The `nika:tts_generate` plane — key/URL PRESENCE only.
    pub tts: TtsProbe,
    /// `--ping` observations per local surface (empty = not requested ·
    /// the default run stays fully offline).
    pub local_pings: Vec<(String, String, PingState)>,
    /// The vendored pricing snapshot's identity + age — the staleness
    /// surface no other CLI ships (2026-07 survey).
    pub pricing: PricingProbe,
    /// The active trace-retention knobs (ADR-100 D4 — doctor reports the
    /// values GC actually enforces).
    pub retention: RetentionConfig,
    /// Knob values that would not parse (each fell back LOUDLY to its
    /// default — a typo'd knob silently doing nothing is hidden magic).
    pub retention_notes: Vec<String>,
}

/// The pricing-catalog facts — all derived from the vendored snapshot
/// (zero network · the born-stale law keeps counts read-time-derived).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PricingProbe {
    /// Snapshot date (ISO `YYYY-MM-DD`).
    pub as_of: String,
    /// Upstream sha256 prefix (provenance pin).
    pub sha: String,
    /// Pricing rules vendored.
    pub rules: usize,
    /// Distinct providers covered.
    pub providers: usize,
    /// Whole days between the snapshot date and today (`None` when the
    /// system clock/date could not be compared — never a guess).
    pub age_days: Option<u32>,
}

/// The TTS-plane environment facts (presence only, never values).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TtsProbe {
    pub openai_key: bool,
    pub elevenlabs_key: bool,
    /// `Some(url)` when `NIKA_TTS_LOCAL_URL` is set (config, displayable).
    pub local_url: Option<String>,
}

/// The image-plane environment facts (presence only, never values).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ImageProbe {
    pub openai_key: bool,
    pub gemini_key: bool,
    pub xai_key: bool,
    /// `Some(url)` when `NIKA_IMAGE_LOCAL_URL` is set (a URL is config,
    /// not a credential — displayable).
    pub local_url: Option<String>,
}

/// Agent/editor MCP wiring facts — config presence only, not file contents in
/// the rendered report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientProbe {
    pub id: String,
    pub path: String,
    pub present: bool,
    pub current: bool,
    pub stale: bool,
}

/// Build the real probe from the environment (PRESENCE-only key checks · the
/// value is never bound) + the canonical provider catalog. The ONE builder
/// both `doctor` and `welcome` consume — a second detector would be a
/// second truth.
#[must_use]
pub(crate) fn collect(ping: bool) -> Probe {
    // ADR-100 D4 — the knobs GC actually enforces, observed once here so
    // `diagnose` stays pure over the Probe.
    let (retention, retention_notes) = RetentionConfig::from_env();
    // The SAME env composition a run uses: the probe observes the world
    // the runtime will see, overrides included (ProvidersConfig::new()
    // here made --ping probe seeds the operator had redirected away).
    let registry = ProviderRegistry::without_http(crate::verbs::run::config_from_env());
    let providers = registry
        .profiles()
        .iter()
        // `mock` is the in-crate test backend, not an operator-facing provider.
        .filter(|p| p.id != "mock")
        .map(|p| {
            let candidates = p.env_candidates();
            ProviderProbe {
                id: p.id.to_owned(),
                requires_key: p.requires_key,
                // PRESENT-NOT-PRINTED: only presence is observed · the value is
                // never bound (alignment Rule 1 · no secret ever surfaces).
                key_present: candidates.iter().any(|v| env_present(v)),
                structured_native: p.supports_response_format(),
                // The conventional var (last candidate · `ANTHROPIC_API_KEY`)
                // is the friendliest fix; the `NIKA_<ID>_API_KEY` form always
                // works too but reads less familiar.
                fix_var: candidates
                    .last()
                    .cloned()
                    .unwrap_or_else(|| format!("NIKA_{}_API_KEY", p.id.to_uppercase())),
            }
        })
        .collect();
    let probe = Probe {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        config_path: config_path(),
        providers,
        clients: client_probes(),
        image: ImageProbe {
            openai_key: env_present("NIKA_OPENAI_API_KEY") || env_present("OPENAI_API_KEY"),
            gemini_key: env_present("NIKA_GEMINI_API_KEY") || env_present("GEMINI_API_KEY"),
            xai_key: env_present("NIKA_XAI_API_KEY") || env_present("XAI_API_KEY"),
            // A URL is connection config, not a credential — displayable.
            #[allow(clippy::disallowed_methods)] // presence+value of a NON-secret config var
            local_url: std::env::var("NIKA_IMAGE_LOCAL_URL")
                .ok()
                .filter(|u| !u.is_empty()),
        },
        tts: TtsProbe {
            openai_key: env_present("NIKA_OPENAI_API_KEY") || env_present("OPENAI_API_KEY"),
            elevenlabs_key: env_present("NIKA_ELEVENLABS_API_KEY")
                || env_present("ELEVENLABS_API_KEY"),
            #[allow(clippy::disallowed_methods)] // presence+value of a NON-secret config var
            local_url: std::env::var("NIKA_TTS_LOCAL_URL").ok().filter(|u| !u.is_empty()),
        },
        local_pings: Vec::new(),
        pricing: pricing_probe(),
        retention,
        retention_notes,
    };
    if ping {
        let local_pings = collect_local_pings(
            &registry,
            probe.image.local_url.as_deref(),
            probe.tts.local_url.as_deref(),
        );
        Probe {
            local_pings,
            ..probe
        }
    } else {
        probe
    }
}

/// `host:port` extracted from a base URL, for a connect-only probe.
/// No URL crate: scheme-strip, authority up to the first `/`, default
/// port per scheme. `None` = unparseable (probed as unreachable).
pub(crate) fn ping_addr(url: &str) -> Option<String> {
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
/// The whole probe — DNS resolution INCLUDED — honours the cap.
/// `to_socket_addrs` is a synchronous OS resolver call with no timeout of
/// its own; on a dead resolver it can stall for seconds, and the review
/// caught the first cut capping only the connect. Resolution + connect
/// run on a worker thread and the caller waits `recv_timeout(cap)`; on
/// expiry the port is reported unreachable and the worker is left to
/// finish in the background (a one-shot diagnostic can afford one
/// parked thread; it exits with the process).
/// Launch one probe worker; the caller awaits the returned channel —
/// [`collect_local_pings`] starts EVERY probe first and then collects
/// against one shared deadline, so seven surfaces answer in ~one cap
/// total instead of seven caps end to end.
pub(crate) fn spawn_ping(addr: &str, timeout: Duration) -> std::sync::mpsc::Receiver<Option<u64>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let addr = addr.to_owned();
    // Doctor is a synchronous one-shot diagnostic (no tokio runtime on this
    // path); plain worker threads are the whole async story here.
    #[allow(clippy::disallowed_methods)]
    std::thread::spawn(move || {
        // The round-trip is measured HERE, in the worker: collection-side
        // elapsed was inflated by every earlier slot's wait (a live 3ms
        // ollama reported "(300ms)" behind a dead port — a 100× lie on a
        // diagnostic surface). DNS included: resolution is part of what
        // the operator's run will pay.
        let started = Instant::now();
        // A dropped receiver (cap expired) makes every send a no-op.
        let rtt = addr
            .to_socket_addrs()
            .ok()
            .and_then(|mut candidates| candidates.next())
            .filter(|sock| TcpStream::connect_timeout(sock, timeout).is_ok())
            .map(|_| u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        let _ = tx.send(rtt);
    });
    rx
}

/// `--ping` collection · the LOCAL surfaces only (infer locals + the media
/// planes when their URL is configured) · 300ms cap per port.
fn collect_local_pings(
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

    let deadline = Instant::now() + CAP;
    pending
        .into_iter()
        .map(|(id, addr, rx)| {
            let budget = deadline.saturating_duration_since(Instant::now());
            let state = match rx.recv_timeout(budget) {
                Ok(Some(ms)) => PingState::Reachable(ms),
                _ => PingState::Unreachable,
            };
            (id, addr, state)
        })
        .collect()
}

fn client_probes() -> Vec<ClientProbe> {
    let mut probes = Vec::new();
    let cursor_workspace_path = PathBuf::from(".").join(".cursor").join("mcp.json");
    if let Some(home) = home_dir() {
        let cursor_paths = vec![home.join(".cursor").join("mcp.json"), cursor_workspace_path];
        probes.push(client_probe_any(
            "cursor",
            &cursor_paths,
            &["mcpServers", "nika"],
        ));
        probes.push(client_probe(
            "windsurf",
            &home
                .join(".codeium")
                .join("windsurf")
                .join("mcp_config.json"),
            &["mcpServers", "nika"],
        ));
        probes.push(client_probe(
            "claude",
            &home.join(".claude.json"),
            &["mcpServers", "nika"],
        ));
    } else {
        probes.push(client_probe(
            "cursor",
            &cursor_workspace_path,
            &["mcpServers", "nika"],
        ));
    }
    probes.push(client_probe(
        "vscode",
        &PathBuf::from(".").join(".vscode").join("mcp.json"),
        &["servers", "nika"],
    ));
    probes
}

pub(crate) fn client_probe_any(
    id: &str,
    paths: &[PathBuf],
    server_path: &[&str; 2],
) -> ClientProbe {
    let mut probes: Vec<ClientProbe> = paths
        .iter()
        .map(|path| client_probe(id, path, server_path))
        .collect();
    if let Some(current) = probes.iter().find(|probe| probe.current).cloned() {
        return current;
    }
    if let Some(stale) = probes.iter().find(|probe| probe.stale).cloned() {
        return stale;
    }
    if let Some(present) = probes.iter().find(|probe| probe.present).cloned() {
        return present;
    }
    probes.remove(0)
}

fn client_probe(id: &str, path: &Path, server_path: &[&str; 2]) -> ClientProbe {
    let present = path.exists();
    let server = if present {
        read_json(path).and_then(|json| server_at(&json, server_path).cloned())
    } else {
        None
    };
    let current = server.as_ref().is_some_and(is_current_mcp_server);
    let stale = server.as_ref().is_some_and(is_stale_mcp_server);
    ClientProbe {
        id: id.to_owned(),
        path: path.display().to_string(),
        present,
        current,
        stale,
    }
}

fn server_at<'a>(value: &'a Value, path: &[&str; 2]) -> Option<&'a Value> {
    value.get(path[0])?.get(path[1])
}

fn is_current_mcp_server(value: &Value) -> bool {
    let Some(args) = value.get("args").and_then(Value::as_array) else {
        return false;
    };
    args.len() == 1 && args[0].as_str() == Some("mcp")
}

fn is_stale_mcp_server(value: &Value) -> bool {
    let Some(args) = value.get("args").and_then(Value::as_array) else {
        return false;
    };
    args.len() == 3
        && args[0].as_str() == Some("mcp")
        && args[1].as_str() == Some("serve")
        && args[2].as_str() == Some("--stdio")
}

fn read_json(path: &Path) -> Option<Value> {
    let body = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&body).ok()
}

#[allow(clippy::disallowed_methods)]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Fill the pricing probe from the vendored snapshot — zero network,
/// counts DERIVED at read time (the born-stale law).
fn pricing_probe() -> PricingProbe {
    let snap = nika_catalog::pricing_snapshot();
    let rules = nika_catalog::all_pricing();
    let providers: std::collections::BTreeSet<&str> = rules.iter().map(|p| p.provider).collect();
    PricingProbe {
        as_of: snap.as_of.to_owned(),
        sha: snap.source_sha256_16.to_owned(),
        rules: rules.len(),
        providers: providers.len(),
        age_days: snapshot_age_days(snap.as_of, epoch_days_now()),
    }
}

/// Today as whole days since the Unix epoch (UTC) — `None` if the
/// system clock reads before 1970 (never a guess).
fn epoch_days_now() -> Option<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs() / 86_400).ok())
}

/// Whole days between an ISO `YYYY-MM-DD` and `today` (epoch days) —
/// `None` on unparseable input or a snapshot "from the future" (clock
/// skew reads as fresh, never as stale).
fn snapshot_age_days(as_of: &str, today: Option<i64>) -> Option<u32> {
    let snapshot = iso_to_epoch_days(as_of)?;
    u32::try_from(today?.checked_sub(snapshot)?).ok()
}

/// `YYYY-MM-DD` → days since the Unix epoch (Howard Hinnant's civil
/// algorithm — no chrono dep for one subtraction).
fn iso_to_epoch_days(iso: &str) -> Option<i64> {
    let bytes = iso.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = iso.get(0..4)?.parse().ok()?;
    let month: u32 = iso.get(5..7)?.parse().ok()?;
    let day: u32 = iso.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = if shifted_year >= 0 {
        shifted_year
    } else {
        shifted_year - 399
    } / 400;
    let year_of_era = shifted_year - era * 400;
    let shifted_month = (i64::from(month) + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

/// Whether an env var is SET and non-empty — PRESENT-NOT-PRINTED. The value is
/// never bound (`var_os(...).is_some`), so no secret can surface (alignment
/// Rule 1). This is the probe layer's sanctioned `std::env` boundary — the same
/// justification the compose root's key-read carries: checking key PRESENCE is
/// not a secret READ, and routing a presence-bool through the kernel vault seam
/// would be ceremony with no payoff.
#[allow(clippy::disallowed_methods)]
pub(crate) fn env_present(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// `~/.nika/config.toml` when it exists (presence only · never read). `HOME` is
/// a path, not a secret — the scoped allow mirrors `env_present`'s.
#[allow(clippy::disallowed_methods)]
fn config_path() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let path = std::path::Path::new(&home)
        .join(".nika")
        .join("config.toml");
    path.exists().then(|| path.display().to_string())
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
    fn iso_to_epoch_days_civil_math() {
        assert_eq!(iso_to_epoch_days("1970-01-01"), Some(0));
        assert_eq!(iso_to_epoch_days("1970-01-02"), Some(1));
        // The leap day: 2000-02-29 exists, 03-01 is exactly one later.
        let feb29 = iso_to_epoch_days("2000-02-29").expect("leap day");
        let mar01 = iso_to_epoch_days("2000-03-01").expect("march 1");
        assert_eq!(mar01 - feb29, 1);
        assert!(iso_to_epoch_days("2026-07-07").expect("modern date") > 20_000);
        for bad in ["2026-13-01", "2026-7-07", "garbage", ""] {
            assert_eq!(iso_to_epoch_days(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn snapshot_age_never_reads_the_future_as_stale() {
        let today = iso_to_epoch_days("2026-07-08");
        assert_eq!(snapshot_age_days("2026-07-07", today), Some(1));
        assert_eq!(snapshot_age_days("2026-07-08", today), Some(0));
        // Clock skew: a snapshot "from tomorrow" reads unknowable, never stale.
        assert_eq!(snapshot_age_days("2026-07-09", today), None);
        assert_eq!(snapshot_age_days("garbage", today), None);
        assert_eq!(snapshot_age_days("2026-07-07", None), None);
    }

    #[test]
    fn the_real_probe_fills_pricing_from_the_vendored_snapshot() {
        // Derived counts (born-stale law): non-empty · providers ≥ the
        // cloud set · identity mirrors the catalog accessor.
        let p = pricing_probe();
        assert!(p.rules > 100, "got {}", p.rules);
        assert!(p.providers >= 5, "got {}", p.providers);
        assert_eq!(p.as_of, nika_catalog::pricing_snapshot().as_of);
        assert_eq!(p.sha, nika_catalog::pricing_snapshot().source_sha256_16);
    }

    #[test]
    fn collect_offline_never_pings_and_never_binds_a_value() {
        // The default (welcome + doctor sans --ping) is fully offline:
        // zero ping observations, and the probe carries booleans/names
        // only — no field can hold a key VALUE by construction.
        let probe = collect(false);
        assert!(probe.local_pings.is_empty(), "offline = no ping rows");
        assert!(!probe.version.is_empty());
        assert!(
            probe.providers.iter().all(|p| !p.id.is_empty()),
            "provider rows are ids, not values"
        );
    }
}
