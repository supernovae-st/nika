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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

    let mut cloud_keys = 0_usize;
    let mut local_ids: Vec<&str> = Vec::new();
    for p in &probe.providers {
        if !p.requires_key {
            local_ids.push(&p.id);
        } else if p.key_present {
            cloud_keys += 1;
            out.push(Finding {
                level: Level::Ok,
                label: "provider".to_owned(),
                detail: format!("{} — key present", p.id),
                fix: None,
            });
        } else {
            out.push(Finding {
                level: Level::Warn,
                label: "provider".to_owned(),
                detail: format!("{} — {} unset", p.id, p.fix_var),
                fix: Some(format!("export {}=…", p.fix_var)),
            });
        }
    }

    if !local_ids.is_empty() {
        out.push(local_finding(&local_ids, !probe.local_pings.is_empty()));
    }
    out.extend(probe.local_pings.iter().map(ping_finding));

    out.push(image_finding(&probe.image));
    out.push(tts_finding(&probe.tts));

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
fn tcp_ping(addr: &str, timeout: Duration) -> PingState {
    let started = Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    let addr = addr.to_owned();
    // Doctor is a synchronous one-shot diagnostic (no tokio runtime on this
    // path); one plain worker thread is the whole async story here.
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
    match rx.recv_timeout(timeout) {
        Ok(true) => PingState::Reachable(u64::try_from(started.elapsed().as_millis()).unwrap_or(0)),
        _ => PingState::Unreachable,
    }
}

/// `--ping` collection · the LOCAL surfaces only (infer locals + the media
/// planes when their URL is configured) · 300ms cap per port.
fn collect_local_pings(
    registry: &ProviderRegistry,
    image_url: Option<&str>,
    tts_url: Option<&str>,
) -> Vec<(String, String, PingState)> {
    const CAP: Duration = Duration::from_millis(300);
    let mut out = Vec::new();
    for p in registry.profiles() {
        if p.requires_key || p.id == "mock" {
            continue;
        }
        if let Some(addr) = ping_addr(p.base_url) {
            out.push((p.id.to_owned(), addr.clone(), tcp_ping(&addr, CAP)));
        }
    }
    for (id, url) in [("image-local", image_url), ("tts-local", tts_url)] {
        if let Some(addr) = url.and_then(ping_addr) {
            out.push((id.to_owned(), addr.clone(), tcp_ping(&addr, CAP)));
        }
    }
    out
}

/// Build the real probe from the environment (PRESENCE-only key checks · the
/// value is never bound) + the canonical provider catalog, then diagnose.
#[must_use]
pub fn run(ping: bool) -> VerbOutput {
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
        text: render(&findings),
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
        };
        let f = diagnose(&probe);
        let loc = f.iter().find(|f| f.label == "local").expect("local line");
        assert_eq!(loc.level, Level::Ok);
        assert!(loc.detail.contains("ollama") && loc.detail.contains("vllm"));
        assert_eq!(exit_code(&f), exit::OK, "a local path is usable");
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
        let out = run(false);
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

    #[test]
    fn tcp_ping_reachable_and_unreachable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr").to_string();
        assert!(matches!(
            tcp_ping(&addr, Duration::from_millis(300)),
            PingState::Reachable(_)
        ));
        // Bind then drop → the port is free again: nothing listens on it.
        let closed = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").to_string()
        };
        assert_eq!(
            tcp_ping(&closed, Duration::from_millis(300)),
            PingState::Unreachable
        );
        assert_eq!(
            tcp_ping("not-an-addr", Duration::from_millis(300)),
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
