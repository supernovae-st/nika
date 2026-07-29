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

use std::path::{Path, PathBuf};

use nika_providers::ProviderRegistry;
pub(crate) use nika_providers::probe::{PingState, ProviderProbe, env_present};

use crate::verbs::trace::retention::RetentionConfig;
use serde_json::Value;

/// The injected environment facts `diagnose` reasons over — PURE · testable.
/// The CLI fills it from the real env; tests pass synthetic probes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Probe {
    pub version: String,
    /// `Some(path)` when a user config file exists.
    pub config_path: Option<String>,
    pub providers: Vec<ProviderProbe>,
    pub clients: Vec<ClientProbe>,
    /// Installed plugin-kit surfaces (found only — absence is silence,
    /// the kit is optional).
    pub kits: Vec<KitProbe>,
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
    /// The ONE models dir (`~/.nika/models` · issue #146) — presence facts
    /// only, observed once here so `diagnose` stays pure.
    pub models: ModelsProbe,
}

// The models-dir facts live with their store (the descended member) —
// re-exported so `doctor`'s diagnosis keeps one import surface.
pub(crate) use nika_models::ModelsProbe;

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

/// One installed plugin-kit surface (the nika-agents bundle a client
/// cloned): which client landed it and the version its manifest
/// declares. Found kits only — a client without the kit is not a
/// finding (the MCP wire via `nika wire` needs no kit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KitProbe {
    pub client: String,
    pub version: String,
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
    let registry = ProviderRegistry::without_http(nika_runtime::compose::config_from_env());
    let providers = nika_providers::probe::collect_provider_probes(&registry);
    let probe = Probe {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        config_path: config_path(),
        providers,
        clients: client_probes(),
        kits: kit_probes(),
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
        models: nika_models::models_probe(),
    };
    if ping {
        let local_pings = nika_providers::probe::collect_local_pings(
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
        // Zed: ~/.config on EVERY platform (upstream choice) · the MCP
        // entry lives under `context_servers` (zed.dev/docs/ai/mcp). The
        // JSON probe is best-effort on Zed's JSONC settings: a commented
        // file parses as not-wired → the fix line says `nika wire zed`,
        // which itself degrades to the ✋ manual snippet. Honest chain.
        probes.push(client_probe(
            "zed",
            &home.join(".config").join("zed").join("settings.json"),
            &["context_servers", "nika"],
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

/// The known kit landings (`update-mirrors.sh` in nika-agents climbs the
/// same ladder). Each client is probed at the rung its SESSIONS load —
/// the install, not the clone — because that is the drift the operator
/// lives (empirical 2026-07-29: a fresh clone sat next to a 0.105
/// install on all three clients of one machine):
///   cursor · the local drop manifest (marketplace installs self-manage)
///   claude · `installed_plugins.json`, the install rung of record
///            (fallback: the marketplace clone's manifest)
///   codex  · the highest per-version cache dir under `plugins/cache`
///            (fallback: the marketplace clone's manifest)
/// Presence + one version string per client — nothing else is read, an
/// unreadable surface is silence.
fn kit_probes() -> Vec<KitProbe> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let kit = |client: &str, version: String| KitProbe {
        client: client.to_owned(),
        version,
    };
    let mut out = Vec::new();
    if let Some(v) = manifest_version(
        &home
            .join(".cursor")
            .join("plugins")
            .join("local")
            .join("nika")
            .join(".claude-plugin")
            .join("plugin.json"),
    ) {
        out.push(kit("cursor", v));
    }
    let claude_clone = home
        .join(".claude")
        .join("plugins")
        .join("marketplaces")
        .join("nika")
        .join(".agents")
        .join("plugins")
        .join("nika")
        .join(".claude-plugin")
        .join("plugin.json");
    if let Some(v) = claude_installed_version(&home.join(".claude").join("plugins"))
        .or_else(|| manifest_version(&claude_clone))
    {
        out.push(kit("claude", v));
    }
    let codex_clone = home
        .join(".codex")
        .join(".tmp")
        .join("marketplaces")
        .join("nika")
        .join(".agents")
        .join("plugins")
        .join("nika")
        .join(".claude-plugin")
        .join("plugin.json");
    if let Some(v) = codex_cache_version(
        &home
            .join(".codex")
            .join("plugins")
            .join("cache")
            .join("nika")
            .join("nika"),
    )
    .or_else(|| manifest_version(&codex_clone))
    {
        out.push(kit("codex", v));
    }
    out
}

/// The `version` field of one plugin manifest — presence-only read.
fn manifest_version(path: &Path) -> Option<String> {
    read_json(path)?.get("version")?.as_str().map(str::to_owned)
}

/// Claude Code's install rung of record: `installed_plugins.json` maps
/// `nika@nika` to its ACTIVE entries (the cache retains old version
/// dirs, so the JSON — not a dir listing — is the truth).
fn claude_installed_version(plugins_dir: &Path) -> Option<String> {
    let json = read_json(&plugins_dir.join("installed_plugins.json"))?;
    json.get("plugins")?
        .get("nika@nika")?
        .as_array()?
        .iter()
        .find_map(|e| Some(e.get("version")?.as_str()?.to_owned()))
}

/// Codex's install rung: the per-version cache keeps one dir per kit
/// version — the highest semver dirname is what the next session loads.
fn codex_cache_version(cache_dir: &Path) -> Option<String> {
    let entries = std::fs::read_dir(cache_dir).ok()?;
    entries
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            version_key(&name).map(|key| (key, name))
        })
        .max()
        .map(|(_, name)| name)
}

/// Lenient `(major, minor, patch)` ordering key — a dirname that does
/// not start `N.N` is not a version dir (a missing patch reads 0).
fn version_key(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.split('.');
    let maj = parts.next()?.parse().ok()?;
    let min = parts.next()?.parse().ok()?;
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((maj, min, patch))
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

/// The ONE walk the mirror family shares (welcome counts it, context
/// audits it) — descended to the forensics crate
/// (`nika_dap::inventory` · 2026-07-21 · the 15k wall); re-exported at
/// the old path so every `probe::collect_workflow_paths` consumer
/// reads unchanged.
pub use nika_dap::inventory::{SKIP_DIRS, collect_workflow_paths};

/// The environment fragment both machine mirrors emit (welcome ·
/// context): client wiring booleans · local provider ids · cloud key
/// COUNTS. Names and counts by construction — no value exists to leak.
#[must_use]
pub(crate) fn environment_json(probe: &Probe) -> serde_json::Value {
    let clients: Vec<serde_json::Value> = probe
        .clients
        .iter()
        .map(|c| serde_json::json!({ "id": c.id, "wired": c.current }))
        .collect();
    let kits: Vec<serde_json::Value> = probe
        .kits
        .iter()
        .map(|k| serde_json::json!({ "client": k.client, "version": k.version }))
        .collect();
    let locals: Vec<&str> = probe
        .providers
        .iter()
        .filter(|p| !p.requires_key)
        .map(|p| p.id.as_str())
        .collect();
    let present = probe
        .providers
        .iter()
        .filter(|p| p.requires_key && p.key_present)
        .count();
    let total = probe.providers.iter().filter(|p| p.requires_key).count();
    serde_json::json!({
        "clients": clients,
        "kits": kits,
        "local_providers": locals,
        "models_pulled": probe.models.count,
        "models_bytes": probe.models.bytes,
        "cloud_keys_present": present,
        "cloud_keys_total": total,
    })
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
        let dir = base.join(format!("nika-probe-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn manifest_version_reads_the_one_field() {
        let dir = temp_dir("manifest");
        let path = dir.join("plugin.json");
        std::fs::write(&path, r#"{"name":"nika","version":"0.106.0"}"#).expect("fixture");
        assert_eq!(manifest_version(&path).as_deref(), Some("0.106.0"));
        assert_eq!(manifest_version(&dir.join("absent.json")), None);
        std::fs::write(&path, "not json").expect("fixture");
        assert_eq!(manifest_version(&path), None, "unreadable is silence");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn claude_install_rung_is_the_json_of_record() {
        let dir = temp_dir("claude-install");
        std::fs::write(
            dir.join("installed_plugins.json"),
            r#"{"version":2,"plugins":{"nika@nika":[{"scope":"user","version":"0.105.0"}],
                "other@x":[{"version":"9.9.9"}]}}"#,
        )
        .expect("fixture");
        assert_eq!(
            claude_installed_version(&dir).as_deref(),
            Some("0.105.0"),
            "the ACTIVE entry, never another plugin's"
        );
        assert_eq!(claude_installed_version(&temp_dir("claude-empty")), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn codex_cache_picks_the_highest_version_numerically() {
        let dir = temp_dir("codex-cache");
        for d in ["0.9.0", "0.105.0", "0.104.2", "tmp", "0.105"] {
            std::fs::create_dir_all(dir.join(d)).expect("fixture dir");
        }
        // 0.9.0 > 0.105.0 lexically — numeric ordering is the assertion.
        assert_eq!(codex_cache_version(&dir).as_deref(), Some("0.105.0"));
        assert_eq!(codex_cache_version(&dir.join("absent")), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn version_key_is_numeric_and_rejects_non_versions() {
        assert!(version_key("0.105.0") > version_key("0.9.0"));
        assert_eq!(
            version_key("0.105"),
            Some((0, 105, 0)),
            "missing patch reads 0"
        );
        assert_eq!(version_key("tmp"), None);
        assert_eq!(version_key("1"), None, "a version needs major.minor");
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
