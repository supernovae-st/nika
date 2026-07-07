// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika doctor` — environment diagnosis (spec `nika-cli` §8 · §2 v0.81 floor).
//!
//! Diagnose-ONLY · the human keeps the hand: every problem prints the exact fix
//! COMMAND, never mutates PATH / shell / config (`--fix` is refused v1 · spec
//! §8). Provider keys are checked PRESENT-NOT-PRINTED — only `is_set` is
//! observed, the value is never bound into a variable, so no secret can reach
//! stdout / stderr / a trace (alignment Rule 1). No network BY DEFAULT, no
//! phone-home ever: the base run reports the configured surface and prints
//! the fix. `--ping` (opt-in) TCP-probes the LOCAL provider ports only —
//! loopback defaults or the operator's own `NIKA_*_LOCAL_URL` — never a
//! vendor endpoint, never a request body, 300ms cap per port.
//!
//! Exit · `0` (a diagnosis is informational) · `3` (ENV · spec §4) only when
//! there is NO inference path at all (zero cloud keys present AND zero local
//! providers in the catalog — a broken/empty build). An unset cloud key is a
//! `⚠` (advisory · you may use a different provider or a local server), never a
//! `✖`: with no workflow in hand `doctor` cannot know which provider you need.

use std::fmt::Write as _;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nika_providers::{ProviderRegistry, ProvidersConfig};
use serde_json::Value;

use crate::verbs::{VerbOutput, exit};

/// Severity of one diagnosis line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Level {
    /// `✔` healthy.
    Ok,
    /// `⚠` advisory (the run may still work).
    Warn,
    /// `✖` a hard environment problem (drives `exit 3`).
    Fail,
}

impl Level {
    fn glyph(self) -> char {
        match self {
            Self::Ok => '✔',
            Self::Warn => '⚠',
            Self::Fail => '✖',
        }
    }
}

/// One `--ping` observation · a local port either answered a TCP connect
/// within the cap or it did not (no request is ever sent on the socket).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PingState {
    /// The port accepted the connection (round-trip in ms).
    Reachable(u64),
    /// Nothing listening (or slower than the 300ms cap).
    Unreachable,
}

/// One diagnosis line · a problem carries the exact PRINTED fix (never run).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Finding {
    pub level: Level,
    pub label: String,
    pub detail: String,
    pub fix: Option<String>,
}

/// A provider's environment facts — key PRESENCE only, never the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderProbe {
    pub id: String,
    pub requires_key: bool,
    pub key_present: bool,
    /// The conventional env var to name in the fix (e.g. `ANTHROPIC_API_KEY`).
    pub fix_var: String,
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

/// Pure diagnosis → ordered findings (binary · config · cloud providers ·
/// local summary · the inference-readiness gate).
pub(crate) fn diagnose(probe: &Probe) -> Vec<Finding> {
    let mut out = vec![
        Finding {
            level: Level::Ok,
            label: "binary".to_owned(),
            detail: format!("v{} · self-contained (spec v1 embedded)", probe.version),
            fix: None,
        },
        match &probe.config_path {
            Some(path) => Finding {
                level: Level::Ok,
                label: "config".to_owned(),
                detail: path.clone(),
                fix: None,
            },
            None => Finding {
                level: Level::Warn,
                label: "config".to_owned(),
                detail: "none — built-in defaults".to_owned(),
                fix: None,
            },
        },
    ];

    out.push(Finding {
        level: Level::Ok,
        label: "lsp".to_owned(),
        detail: "available via `nika lsp` (editor language server)".to_owned(),
        fix: None,
    });
    out.push(Finding {
        level: Level::Ok,
        label: "mcp".to_owned(),
        detail: "available via `nika mcp` (8 read-only tools · nika_check through nika_tools)"
            .to_owned(),
        fix: None,
    });

    for client in &probe.clients {
        out.push(client_finding(client));
    }

    // Display order practices the presentation lock (teaching surface ·
    // the first screen after install): the sovereign keyless line leads,
    // then the cloud rows with mistral (EU · open-weight) first. The
    // registry's seed order is functional and stays untouched — this is
    // a render sort only.
    let mut cloud_keys = 0_usize;
    let mut local_ids: Vec<&str> = Vec::new();
    let mut cloud_rows: Vec<&ProviderProbe> = Vec::new();
    for p in &probe.providers {
        if p.requires_key {
            cloud_rows.push(p);
        } else {
            local_ids.push(&p.id);
        }
    }
    cloud_rows.sort_by_key(|p| usize::from(p.id != "mistral"));

    if !local_ids.is_empty() {
        out.push(local_finding(&local_ids, !probe.local_pings.is_empty()));
    }
    out.extend(probe.local_pings.iter().map(ping_finding));

    for p in cloud_rows {
        cloud_keys += usize::from(p.key_present);
        out.push(provider_finding(p));
    }

    out.push(image_finding(&probe.image));
    out.push(tts_finding(&probe.tts));
    out.push(pricing_finding(&probe.pricing));

    // The ONE Fail that drives exit 3 · no inference path AT ALL (a broken or
    // empty catalog). A merely-unset cloud key is a ⚠ above, never fatal.
    if cloud_keys == 0 && local_ids.is_empty() {
        out.push(Finding {
            level: Level::Fail,
            label: "providers".to_owned(),
            detail: "no inference provider available — neither a cloud key nor a local server"
                .to_owned(),
            fix: Some(
                "export <PROVIDER>_API_KEY=…  · or run a local server (ollama · llama.cpp · vLLM)"
                    .to_owned(),
            ),
        });
    }

    out
}

/// The image plane (`nika:image_generate`) — mock always works; this
/// names what ELSE is wired. Informational, never fatal (media is a
/// builtin, not the inference path).
fn image_finding(img: &ImageProbe) -> Finding {
    let mut wired: Vec<&str> = Vec::new();
    if img.openai_key {
        wired.push("openai");
    }
    if img.gemini_key {
        wired.push("gemini");
    }
    if img.xai_key {
        wired.push("xai");
    }
    let local_part = img.local_url.as_deref().map_or_else(
        || {
            "local → http://localhost:8080 default (set NIKA_IMAGE_LOCAL_URL to point elsewhere)"
                .to_owned()
        },
        |url| format!("local → {url}"),
    );
    Finding {
        level: Level::Ok,
        label: "image".to_owned(),
        detail: if wired.is_empty() {
            format!("mock ready · {local_part} · no cloud image key set")
        } else {
            format!(
                "mock ready · {} key(s) present · {local_part}",
                wired.join(" · ")
            )
        },
        fix: None,
    }
}

/// How old the vendored pricing snapshot may grow before the doctor
/// flags it — prices move monthly-ish upstream; 120 days of drift is
/// where a cost report stops being trustworthy without saying so.
const PRICING_STALE_DAYS: u32 = 120;

/// The pricing-catalog line — identity always (which snapshot prices
/// this binary's cost reports), a staleness ⚠ past the threshold. The
/// age warning is the gap no surveyed tool closes (2026-07): a stale
/// vendored price table silently mis-prices every report.
fn pricing_finding(p: &PricingProbe) -> Finding {
    let identity = format!(
        "{} models · {} providers · snapshot {} · models.dev {}",
        p.rules, p.providers, p.as_of, p.sha
    );
    match p.age_days {
        Some(age) if age > PRICING_STALE_DAYS => Finding {
            level: Level::Warn,
            label: "pricing".to_owned(),
            detail: format!("{identity} — {age} days old · cost reports may drift"),
            fix: Some(
                "upgrade nika — the pricing snapshot ships with releases \
                 (from source: bash scripts/refresh-pricing.sh)"
                    .to_owned(),
            ),
        },
        _ => Finding {
            level: Level::Ok,
            label: "pricing".to_owned(),
            detail: identity,
            fix: None,
        },
    }
}

/// The TTS plane (`nika:tts_generate`) — the image line's exact sibling.
fn tts_finding(tts: &TtsProbe) -> Finding {
    let mut wired: Vec<&str> = Vec::new();
    if tts.openai_key {
        wired.push("openai");
    }
    if tts.elevenlabs_key {
        wired.push("elevenlabs");
    }
    let local_part = tts.local_url.as_deref().map_or_else(
        || {
            "local → http://localhost:8080 default (set NIKA_TTS_LOCAL_URL to point elsewhere)"
                .to_owned()
        },
        |url| format!("local → {url}"),
    );
    Finding {
        level: Level::Ok,
        label: "tts".to_owned(),
        detail: if wired.is_empty() {
            format!("mock ready · {local_part} · no cloud speech key set")
        } else {
            format!(
                "mock ready · {} key(s) present · {local_part}",
                wired.join(" · ")
            )
        },
        fix: None,
    }
}

fn client_finding(client: &ClientProbe) -> Finding {
    if client.current {
        return Finding {
            level: Level::Ok,
            label: "agent".to_owned(),
            detail: format!("{} wired at {}", client.id, client.path),
            fix: None,
        };
    }
    if client.stale {
        return Finding {
            level: Level::Warn,
            label: "agent".to_owned(),
            detail: format!(
                "{} has stale MCP args at {} (`mcp serve --stdio`)",
                client.id, client.path
            ),
            fix: Some(format!("nika wire {}", client.id)),
        };
    }
    if client.present {
        return Finding {
            level: Level::Warn,
            label: "agent".to_owned(),
            detail: format!("{} config exists but Nika MCP is not wired", client.id),
            fix: Some(format!("nika wire {}", client.id)),
        };
    }
    Finding {
        level: Level::Warn,
        label: "agent".to_owned(),
        detail: format!("{} not wired", client.id),
        fix: Some(format!("nika wire {}", client.id)),
    }
}

/// Exit code · `ENV(3)` when any `Fail`, else `OK(0)`.
pub(crate) fn exit_code(findings: &[Finding]) -> u8 {
    if findings.iter().any(|f| f.level == Level::Fail) {
        exit::ENV
    } else {
        exit::OK
    }
}

/// Render findings as the `nika doctor` report (spec §8 layout · glyph · label
/// padded · detail · an indented `fix:` line under a problem) — opened by the
/// ONE verdict line (`✔ 6 ok · 4 warn · 0 fail`) so the state of the
/// environment reads before the sections do. Sections stay unchanged.
/// The machine lane (Q7): findings verbatim + a computed summary —
/// agents/CI branch on `summary.fail` instead of parsing glyphs.
pub(crate) fn render_json(findings: &[Finding]) -> String {
    let count = |lvl: Level| findings.iter().filter(|f| f.level == lvl).count();
    let payload = serde_json::json!({
        "summary": {
            "ok": count(Level::Ok),
            "warn": count(Level::Warn),
            "fail": count(Level::Fail),
        },
        "findings": findings,
    });
    format!("{payload:#}")
}

pub(crate) fn render(findings: &[Finding]) -> String {
    let mut s = String::new();
    let count = |level: Level| findings.iter().filter(|f| f.level == level).count();
    let (ok, warn, fail) = (count(Level::Ok), count(Level::Warn), count(Level::Fail));
    let verdict = if fail > 0 { Level::Fail } else { Level::Ok };
    let _ = writeln!(s, "{} {ok} ok · {warn} warn · {fail} fail", verdict.glyph());
    for f in findings {
        let _ = writeln!(s, "{} {:<10} {}", f.level.glyph(), f.label, f.detail);
        if let Some(fix) = &f.fix {
            let _ = writeln!(s, "  fix: {fix}");
        }
    }
    s
}

/// One cloud-provider row (✔ key present · ⚠ unset, with the export fix).
fn provider_finding(p: &ProviderProbe) -> Finding {
    if p.key_present {
        Finding {
            level: Level::Ok,
            label: "provider".to_owned(),
            detail: format!("{} — key present", p.id),
            fix: None,
        }
    } else {
        Finding {
            level: Level::Warn,
            label: "provider".to_owned(),
            detail: format!("{} — {} unset", p.id, p.fix_var),
            fix: Some(format!("export {}=…", p.fix_var)),
        }
    }
}

/// The local-providers summary line · unpinged runs hand off to `--ping`.
fn local_finding(local_ids: &[&str], pinged: bool) -> Finding {
    Finding {
        level: Level::Ok,
        label: "local".to_owned(),
        detail: format!(
            "{} provider(s) ({}) — no key · needs a running server",
            local_ids.len(),
            local_ids.join(" · ")
        ),
        fix: (!pinged)
            .then(|| "nika doctor --ping   # probe the local ports (offline otherwise)".to_owned()),
    }
}

/// One rendered `--ping` line (✔ listening · ⚠ nothing there).
fn ping_finding((id, addr, state): &(String, String, PingState)) -> Finding {
    match state {
        PingState::Reachable(ms) => Finding {
            level: Level::Ok,
            label: "ping".to_owned(),
            detail: format!("{id} — listening on {addr} ({ms}ms)"),
            fix: None,
        },
        PingState::Unreachable => Finding {
            level: Level::Warn,
            label: "ping".to_owned(),
            detail: format!("{id} — nothing listening on {addr}"),
            fix: None,
        },
    }
}

/// `host:port` extracted from a base URL, for a connect-only probe.
/// No URL crate: scheme-strip, authority up to the first `/`, default
/// port per scheme. `None` = unparseable (probed as unreachable).
fn ping_addr(url: &str) -> Option<String> {
    let (default_port, rest) = if let Some(r) = url.strip_prefix("https://") {
        ("443", r)
    } else if let Some(r) = url.strip_prefix("http://") {
        ("80", r)
    } else {
        return None;
    };
    let authority = rest.split('/').next().unwrap_or_default();
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
fn spawn_ping(addr: &str, timeout: Duration) -> std::sync::mpsc::Receiver<bool> {
    let (tx, rx) = std::sync::mpsc::channel();
    let addr = addr.to_owned();
    // Doctor is a synchronous one-shot diagnostic (no tokio runtime on this
    // path); plain worker threads are the whole async story here.
    #[allow(clippy::disallowed_methods)]
    std::thread::spawn(move || {
        // A dropped receiver (cap expired) makes every send a no-op.
        let reachable = addr
            .to_socket_addrs()
            .ok()
            .and_then(|mut candidates| candidates.next())
            .is_some_and(|sock| TcpStream::connect_timeout(&sock, timeout).is_ok());
        let _ = tx.send(reachable);
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
        if let Some(addr) = ping_addr(p.base_url) {
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

    let started = Instant::now();
    let deadline = started + CAP;
    pending
        .into_iter()
        .map(|(id, addr, rx)| {
            let budget = deadline.saturating_duration_since(Instant::now());
            let state = match rx.recv_timeout(budget) {
                Ok(true) => {
                    PingState::Reachable(u64::try_from(started.elapsed().as_millis()).unwrap_or(0))
                }
                _ => PingState::Unreachable,
            };
            (id, addr, state)
        })
        .collect()
}

/// Build the real probe from the environment (PRESENCE-only key checks · the
/// value is never bound) + the canonical provider catalog, then diagnose.
#[must_use]
pub fn run(ping: bool, json: bool) -> VerbOutput {
    let registry = ProviderRegistry::without_http(ProvidersConfig::new());
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
    };
    let probe = if ping {
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
    };
    let findings = diagnose(&probe);
    let code = exit_code(&findings);
    VerbOutput {
        text: if json {
            render_json(&findings)
        } else {
            render(&findings)
        },
        code,
    }
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

fn client_probe_any(id: &str, paths: &[PathBuf], server_path: &[&str; 2]) -> ClientProbe {
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
/// Rule 1). This is the doctor verb's sanctioned `std::env` boundary — the same
/// justification the compose root's key-read carries: checking key PRESENCE is
/// not a secret READ, and routing a presence-bool through the kernel vault seam
/// would be ceremony with no payoff.
#[allow(clippy::disallowed_methods)]
fn env_present(name: &str) -> bool {
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

    fn cloud(id: &str, var: &str, present: bool) -> ProviderProbe {
        ProviderProbe {
            id: id.to_owned(),
            requires_key: true,
            key_present: present,
            fix_var: var.to_owned(),
        }
    }
    fn local(id: &str) -> ProviderProbe {
        ProviderProbe {
            id: id.to_owned(),
            requires_key: false,
            key_present: false,
            fix_var: String::new(),
        }
    }

    #[test]
    fn key_present_is_ok_and_exits_zero() {
        let probe = Probe {
            version: "0.81.0".to_owned(),
            config_path: Some("~/.nika/config.toml".to_owned()),
            providers: vec![cloud("anthropic", "ANTHROPIC_API_KEY", true)],
            clients: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
        };
        let f = diagnose(&probe);
        let prov = f
            .iter()
            .find(|f| f.label == "provider")
            .expect("provider line");
        assert_eq!(prov.level, Level::Ok);
        assert!(prov.detail.contains("key present"));
        assert_eq!(exit_code(&f), exit::OK);
    }

    #[test]
    fn unset_key_is_a_warn_with_a_fix_not_a_fail() {
        // An unset cloud key is advisory — exit stays 0 (a local provider is a
        // valid path · doctor cannot know which provider the user needs).
        let probe = Probe {
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![
                cloud("anthropic", "ANTHROPIC_API_KEY", false),
                local("ollama"),
            ],
            clients: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
        };
        let f = diagnose(&probe);
        let prov = f
            .iter()
            .find(|f| f.label == "provider")
            .expect("provider line");
        assert_eq!(prov.level, Level::Warn);
        assert_eq!(
            prov.fix.as_deref(),
            Some("export ANTHROPIC_API_KEY=…"),
            "the exact fix command is printed"
        );
        assert!(
            !f.iter().any(|f| f.level == Level::Fail),
            "a local path exists"
        );
        assert_eq!(exit_code(&f), exit::OK);
    }

    #[test]
    fn never_prints_a_secret_value() {
        // The probe carries only a bool — there is no field a value could ride.
        // Assert the rendered report carries the VAR NAME, never a value.
        let probe = Probe {
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![cloud("openai", "OPENAI_API_KEY", false)],
            clients: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
        };
        let text = render(&diagnose(&probe));
        assert!(text.contains("OPENAI_API_KEY"), "names the var: {text}");
        assert!(
            !text.contains("sk-"),
            "no secret-shaped value leaks: {text}"
        );
    }

    #[test]
    fn no_provider_at_all_fails_with_exit_three() {
        // A broken/empty catalog — no cloud key AND no local server: the real
        // "cannot infer" environment error (spec §4 ENV).
        let probe = Probe {
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![cloud("anthropic", "ANTHROPIC_API_KEY", false)],
            clients: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
        };
        let f = diagnose(&probe);
        assert!(f.iter().any(|f| f.level == Level::Fail));
        assert_eq!(exit_code(&f), exit::ENV);
    }

    #[test]
    fn local_provider_alone_is_a_usable_path_exit_zero() {
        let probe = Probe {
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![local("ollama"), local("vllm")],
            clients: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
        };
        let f = diagnose(&probe);
        let loc = f.iter().find(|f| f.label == "local").expect("local line");
        assert_eq!(loc.level, Level::Ok);
        assert!(loc.detail.contains("ollama") && loc.detail.contains("vllm"));
        assert_eq!(exit_code(&f), exit::OK, "a local path is usable");
    }

    #[test]
    fn json_lane_carries_summary_and_findings_verbatim() {
        let findings = vec![
            Finding {
                level: Level::Ok,
                label: "binary".to_owned(),
                detail: "v0.96.0".to_owned(),
                fix: None,
            },
            Finding {
                level: Level::Fail,
                label: "config".to_owned(),
                detail: "broken".to_owned(),
                fix: Some("nika wire claude".to_owned()),
            },
        ];
        let json: serde_json::Value =
            serde_json::from_str(&render_json(&findings)).expect("valid JSON");
        assert_eq!(json["summary"]["ok"], 1);
        assert_eq!(json["summary"]["fail"], 1);
        assert_eq!(json["findings"][0]["level"], "ok");
        assert_eq!(json["findings"][1]["fix"], "nika wire claude");
    }

    // ── The pricing snapshot line (Cost Intelligence 2026-07-08) ──

    fn pricing(age_days: Option<u32>) -> PricingProbe {
        PricingProbe {
            as_of: "2026-07-07".to_owned(),
            sha: "d31a39603aa5419d".to_owned(),
            rules: 606,
            providers: 10,
            age_days,
        }
    }

    #[test]
    fn pricing_line_names_the_snapshot_identity() {
        let f = pricing_finding(&pricing(Some(3)));
        assert_eq!(f.level, Level::Ok);
        assert!(f.detail.contains("606 models"), "{}", f.detail);
        assert!(f.detail.contains("10 providers"), "{}", f.detail);
        assert!(f.detail.contains("2026-07-07"), "{}", f.detail);
        assert!(f.detail.contains("d31a39603aa5419d"), "{}", f.detail);
        assert!(f.fix.is_none());
    }

    #[test]
    fn stale_pricing_snapshot_warns_with_the_upgrade_fix() {
        // The staleness gap no surveyed tool closes: past the threshold
        // the line flips ⚠ and prints the exact remedy.
        let f = pricing_finding(&pricing(Some(PRICING_STALE_DAYS + 1)));
        assert_eq!(f.level, Level::Warn);
        assert!(f.detail.contains("days old"), "{}", f.detail);
        assert!(
            f.fix.as_deref().is_some_and(|x| x.contains("upgrade nika")),
            "{:?}",
            f.fix
        );
        // AT the threshold stays green (stale means PAST it).
        assert_eq!(
            pricing_finding(&pricing(Some(PRICING_STALE_DAYS))).level,
            Level::Ok
        );
        // An uncomputable age never guesses stale.
        assert_eq!(pricing_finding(&pricing(None)).level, Level::Ok);
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

    /// Each severity prints a DISTINCT glyph — a `Default::default()` mutant
    /// (the null char `'\0'`) would erase the level cue the operator scans for.
    #[test]
    fn level_glyphs_are_distinct() {
        assert_eq!(Level::Ok.glyph(), '✔');
        assert_eq!(Level::Warn.glyph(), '⚠');
        assert_eq!(Level::Fail.glyph(), '✖');
    }

    #[test]
    fn client_probe_reports_stale_wiring_with_a_wire_fix() {
        let probe = Probe {
            version: "0.90.0".to_owned(),
            config_path: None,
            providers: vec![local("ollama")],
            clients: vec![ClientProbe {
                id: "cursor".to_owned(),
                path: "~/.cursor/mcp.json".to_owned(),
                present: true,
                current: false,
                stale: true,
            }],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
        };
        let text = render(&diagnose(&probe));
        assert!(text.contains("stale MCP args"), "{text}");
        assert!(text.contains("fix: nika wire cursor"), "{text}");
    }

    #[test]
    fn cursor_probe_accepts_workspace_config_from_extension() {
        let dir = temp_dir("cursor-workspace");
        let global = dir.join("home").join(".cursor").join("mcp.json");
        let workspace = dir.join("repo").join(".cursor").join("mcp.json");
        std::fs::create_dir_all(workspace.parent().expect("parent")).expect("mkdir");
        std::fs::write(
            &workspace,
            r#"{"mcpServers":{"nika":{"command":"nika","args":["mcp"]}}}"#,
        )
        .expect("fixture");

        let paths = vec![global, workspace.clone()];
        let probe = client_probe_any("cursor", &paths, &["mcpServers", "nika"]);
        assert!(probe.current);
        assert_eq!(probe.path, workspace.display().to_string());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// `env_present` is a PRESENCE check (set + non-empty) — read against real
    /// vars so no racy `set_var` is needed. PATH is always set + non-empty
    /// (kills the `-> false` constant + the `!is_empty` negation); a name
    /// nothing sets is absent (kills the `-> true` constant).
    #[test]
    fn env_present_reflects_the_real_environment() {
        assert!(env_present("PATH"), "PATH is set + non-empty");
        assert!(!env_present("NIKA_CLI_DEFINITELY_UNSET_VARIABLE_XYZZY"));
    }

    #[test]
    fn the_real_catalog_has_no_fail_and_renders() {
        // The wired run() over the canonical catalog: local providers exist, so
        // there is never a hard Fail (exit 0) even with no keys in the test env.
        // ping=false — the default run stays fully offline, in tests too.
        let out = run(false, false);
        assert_eq!(out.code, exit::OK, "the catalog always offers a path");
        assert!(out.text.contains("binary"), "renders the binary line");
        // The LLM test backend stays hidden (no `mock — key` provider row);
        // the IMAGE line's `mock ready` is operator-facing truth (the image
        // mock is a documented, always-available provider).
        assert!(
            !out.text.contains("mock —"),
            "the LLM test backend is hidden"
        );
        assert!(out.text.contains("image"), "the image plane renders");
        assert!(out.text.contains("tts"), "the tts plane renders");
    }

    /// The report opens on the ONE verdict line — level counts first,
    /// sections unchanged below. The glyph tracks the WORST level (✔
    /// with warns · ✖ only on a fail).
    #[test]
    fn render_opens_with_the_verdict_count_line() {
        let ok = Finding {
            level: Level::Ok,
            label: "binary".to_owned(),
            detail: "v0 · self-contained".to_owned(),
            fix: None,
        };
        let warn = Finding {
            level: Level::Warn,
            label: "provider".to_owned(),
            detail: "x — KEY unset".to_owned(),
            fix: Some("export KEY=…".to_owned()),
        };
        let text = render(&[ok.clone(), ok.clone(), warn.clone()]);
        let first = text.lines().next().expect("verdict line");
        assert_eq!(first, "✔ 2 ok · 1 warn · 0 fail");
        assert!(text.contains("binary"), "sections unchanged: {text}");

        let fail = Finding {
            level: Level::Fail,
            label: "providers".to_owned(),
            detail: "no path".to_owned(),
            fix: None,
        };
        let red = render(&[ok, warn, fail]);
        assert!(
            red.starts_with("✖ 1 ok · 1 warn · 1 fail"),
            "a fail flips the verdict glyph: {red}"
        );
    }

    #[allow(clippy::disallowed_methods)]
    fn temp_dir(name: &str) -> PathBuf {
        let base = std::env::var_os("CARGO_TARGET_TMPDIR").map_or_else(
            || {
                std::env::current_dir()
                    .expect("current dir")
                    .join("target")
                    .join("tmp")
            },
            PathBuf::from,
        );
        let dir = base.join(format!("nika-doctor-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn ping_addr_extracts_authority_and_defaults_ports() {
        assert_eq!(
            ping_addr("http://127.0.0.1:11434/v1/chat/completions").as_deref(),
            Some("127.0.0.1:11434")
        );
        assert_eq!(
            ping_addr("https://example.test/v1").as_deref(),
            Some("example.test:443")
        );
        assert_eq!(ping_addr("http://host/v1").as_deref(), Some("host:80"));
        assert_eq!(ping_addr("ftp://x"), None);
        assert_eq!(ping_addr("http://"), None);
    }

    /// The single-probe composition the parallel collector applies per
    /// surface — kept here so the probe contract stays directly tested.
    fn ping_once(addr: &str, timeout: Duration) -> PingState {
        match spawn_ping(addr, timeout).recv_timeout(timeout) {
            Ok(true) => PingState::Reachable(0),
            _ => PingState::Unreachable,
        }
    }

    #[test]
    fn tcp_ping_reachable_and_unreachable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr").to_string();
        assert!(matches!(
            ping_once(&addr, Duration::from_millis(300)),
            PingState::Reachable(_)
        ));
        // Bind then drop → the port is free again: nothing listens on it.
        let closed = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").to_string()
        };
        assert_eq!(
            ping_once(&closed, Duration::from_millis(300)),
            PingState::Unreachable
        );
        assert_eq!(
            ping_once("not-an-addr", Duration::from_millis(300)),
            PingState::Unreachable
        );
    }

    #[test]
    fn local_line_hands_off_to_ping_and_ping_lines_render() {
        let base = Probe {
            version: "0.0.0".to_owned(),
            config_path: None,
            providers: vec![ProviderProbe {
                id: "ollama".to_owned(),
                requires_key: false,
                key_present: false,
                fix_var: String::new(),
            }],
            clients: Vec::new(),
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
        };
        let findings = diagnose(&base);
        let local = findings
            .iter()
            .find(|f| f.label == "local")
            .expect("local line");
        assert!(
            local.fix.as_deref().is_some_and(|f| f.contains("--ping")),
            "unpinged run must hand off to --ping"
        );

        let pinged = Probe {
            local_pings: vec![
                (
                    "ollama".to_owned(),
                    "127.0.0.1:11434".to_owned(),
                    PingState::Reachable(3),
                ),
                (
                    "vllm".to_owned(),
                    "127.0.0.1:8000".to_owned(),
                    PingState::Unreachable,
                ),
            ],
            ..base
        };
        let findings = diagnose(&pinged);
        let pings: Vec<_> = findings.iter().filter(|f| f.label == "ping").collect();
        assert_eq!(pings.len(), 2);
        assert!(pings[0].detail.contains("listening on 127.0.0.1:11434"));
        assert_eq!(pings[0].level, Level::Ok);
        assert!(pings[1].detail.contains("nothing listening"));
        assert_eq!(pings[1].level, Level::Warn);
        let local = findings
            .iter()
            .find(|f| f.label == "local")
            .expect("local line");
        assert!(local.fix.is_none(), "pinged run drops the hand-off");
    }
}
