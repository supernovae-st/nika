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

pub use crate::harness::access_probes_with_harness;
use nika_providers::ProviderRegistry;
use nika_providers::census::AccessCensus;
use nika_providers::probe::ExecutionLocus;
pub use nika_providers::probe::{KeyAuth, PingState, ProviderProbe, env_present};

use crate::clients_registry::{self, RegistryCoverage};
use crate::retention::RetentionConfig;
use serde_json::Value;

/// The injected environment facts `diagnose` reasons over — PURE · testable.
/// The CLI fills it from the real env; tests pass synthetic probes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub version: String,
    /// `Some(path)` when a user config file exists.
    pub config_path: Option<String>,
    pub providers: Vec<ProviderProbe>,
    /// The access census (R4 · one census, one truth): the provider rows
    /// PLUS the harness seats joined in, so no surface ever again reads
    /// provider rows alone and calls a signed-in seat « no inference
    /// path ». Derived ONCE here from the same rows; `doctor`, `welcome`
    /// and the refusal tail READ it, none recomputes.
    pub census: AccessCensus,
    pub clients: Vec<ClientProbe>,
    /// Installed plugin-kit surfaces (found only — absence is silence,
    /// the kit is optional).
    pub kits: Vec<KitProbe>,
    /// The client-matrix coverage (H6 · Q1 2026-07-31): derived from
    /// the vendored nika-plugins registry — the counts doctor renders
    /// are read-time derivations of the ONE SSOT, never hand counts.
    pub clients_registry: RegistryCoverage,
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
    /// Run journals under `.nika/traces` (CWD-relative — the dir the
    /// trace writer appends to), counted by dir-listing ONLY (a greeting
    /// stays instant: no parsing, and a torn journal still proves a run
    /// happened). The adoption ladder's record rung (P0-21).
    pub recorded_runs: usize,
    /// Journals the CWD's git repo ALREADY tracks under the trace dir —
    /// the leak `init`'s `.gitignore` cover now prevents, counted so
    /// doctor can print the untrack remedy for repos founded before it
    /// did. Index-only via `git ls-files` (`crate::git`); `None` when
    /// the observation was impossible (no git · not a repo) — an
    /// unobserved surface is no finding.
    pub tracked_traces: Option<usize>,
}

// The models-dir facts live with their store (the descended member) —
// re-exported so `doctor`'s diagnosis keeps one import surface.
pub use nika_models::ModelsProbe;

/// The pricing-catalog facts — all derived from the vendored snapshot
/// (zero network · the born-stale law keeps counts read-time-derived).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PricingProbe {
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

/// The OS-sandbox decision the doctor reports (#891 · #822 P1) — read off
/// the ONE selection (#888), presence facts only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxProbe {
    /// The stable backend id (`seatbelt` · `landlock` · `noop`).
    pub backend: &'static str,
    /// True when the selection confines (anything but the deliberate noop).
    pub confined: bool,
}

/// The one sandbox observation (the sidecar precedent): the decision is
/// read here once so `diagnose` stays pure over its inputs.
#[must_use]
pub fn sandbox_probe() -> SandboxProbe {
    let decision = nika_runtime::sandbox_select::select_command_sandbox();
    SandboxProbe {
        backend: decision.backend(),
        confined: decision.is_confined(),
    }
}

/// The TTS-plane environment facts (presence only, never values).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TtsProbe {
    pub openai_key: bool,
    pub elevenlabs_key: bool,
    /// `Some(url)` when `NIKA_TTS_LOCAL_URL` is set (config, displayable).
    pub local_url: Option<String>,
}

/// The image-plane environment facts (presence only, never values).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageProbe {
    pub openai_key: bool,
    pub gemini_key: bool,
    pub xai_key: bool,
    /// `Some(url)` when `NIKA_IMAGE_LOCAL_URL` is set (a URL is config,
    /// not a credential — displayable).
    pub local_url: Option<String>,
}

/// One installed plugin-kit surface (the nika-plugins bundle a client
/// cloned): which client landed it and the version its manifest
/// declares. Found kits only — a client without the kit is not a
/// finding (the MCP wire via `nika wire` needs no kit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitProbe {
    pub client: String,
    pub version: String,
}

/// Agent/editor MCP wiring facts — config presence only, not file contents in
/// the rendered report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientProbe {
    pub id: String,
    pub path: String,
    pub present: bool,
    pub current: bool,
    pub stale: bool,
}

/// How far a wired host actually climbs (P0-9 · audit UX 2026-07-30:
/// doctor rendered « wired » uniformly — a host parity that does not
/// exist). The scale is honest: every rung is EARNED by a detected
/// surface, and MCP alone is never « guarded ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityLevel {
    /// L1 — the oracle MCP only: the host can ASK (`nika_check` …),
    /// nothing guards the edit or the run.
    OracleOnly,
    /// L2 — the kit's skills/commands/rules guide the authoring; no
    /// hook fires (the codex kit ships none today).
    AuthoringEnabled,
    /// L3+L4 — the kit's hooks are on disk with it: check after edit,
    /// guard before run.
    Guarded,
    /// L5 — deep integration (the host drives runs natively). No host
    /// reaches it today: the ceiling is named, never claimed.
    FullyIntegrated,
}

impl CapabilityLevel {
    /// The machine token rendered on the doctor line (kebab-case).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OracleOnly => "oracle-only",
            Self::AuthoringEnabled => "authoring-enabled",
            Self::Guarded => "guarded",
            Self::FullyIntegrated => "fully-integrated",
        }
    }
}

impl ClientProbe {
    /// The host's earned rung — computed from the surfaces the probe
    /// actually DETECTED (this row's wire · the kit rows), never from
    /// the client's name alone. MCP-only ⇒ `OracleOnly`, JAMAIS
    /// `Guarded` (P0-9).
    #[must_use]
    pub fn capability(&self, kits: &[KitProbe]) -> CapabilityLevel {
        if !self.current || !kits.iter().any(|k| k.client == self.id) {
            return CapabilityLevel::OracleOnly;
        }
        if kit_ships_hooks(&self.id) {
            CapabilityLevel::Guarded
        } else {
            CapabilityLevel::AuthoringEnabled
        }
    }
}

/// The nika-plugins kit ships its hook definitions INSIDE the same drop
/// as the manifest (`hooks/<client>-hooks.json` next to
/// `.claude-plugin/plugin.json` — verified against the bundle
/// 2026-07-31): finding the manifest proves the hooks landed with it.
/// claude + cursor ship hooks; the codex kit carries none today.
fn kit_ships_hooks(client: &str) -> bool {
    matches!(client, "claude" | "cursor")
}

/// The per-host runtime receipt (H5 · audit UX 2026-07-30: « aucun
/// shared runtime receipt » — doctor rendered a flat finding pile and
/// nothing said, per host, WHAT was verified versus assumed). One row
/// per probed host: the earned level plus the canaries that PROVE it,
/// the rails still missing for the rung above, and the exact repair
/// command (printed, never run — the doctor law). The honesty law
/// applies here too: `level_assumed` flags every level above the
/// oracle floor, because the hooks half of the climb rests on the
/// STATIC `kit_ships_hooks` table — a manifest find is observed,
/// « the hooks landed with it » (or did not) is a lookup, and the
/// receipt says so.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HostCapabilityReceipt {
    /// The probed host id (`claude` · `cursor` · `hermes` …).
    pub host: String,
    /// The installed kit's declared version — `None` when no kit was
    /// found (the binary's own version is the doctor line's, never the
    /// host's).
    pub version: Option<String>,
    /// The deepest detected surface: `mcp` · `kit` · `hooks`
    /// (`none` when the host is not wired at all).
    pub surface: String,
    /// The config root the probe inspected for this host.
    pub root: Option<String>,
    /// Every detected surface, in climb order (mcp → kit → hooks).
    pub components: Vec<String>,
    /// The earned capability rung, kebab-case — `unwired` below the
    /// oracle floor (an unwired host never borrows a rung).
    pub capability: String,
    /// What was VERIFIED versus assumed: `config-parsed` is an
    /// observation, `hooks-assumed-from-kit` is the static table
    /// talking — the canary NAMES the difference.
    pub canaries: Vec<String>,
    /// The surfaces missing for the rung above (`mcp` · `kit` ·
    /// `hooks` — empty at the guarded ceiling today).
    pub missing_rails: Vec<String>,
    /// The exact repair command (printed, never run) — `None` when no
    /// rail is missing.
    pub repair: Option<String>,
    /// `true` when part of the level rests on the static
    /// `kit_ships_hooks` table rather than a direct observation.
    pub level_assumed: bool,
    /// The guard's evidence rung, when a guard exists at all —
    /// `declared` (the static table says the kit ships hooks) ·
    /// `loaded` (the host observably loaded the hook this session) ·
    /// `proven` (an allow canary AND a deny canary passed on this
    /// host version and surface, dated). Today every guard rung is
    /// `declared`: no probe loads or canaries a hook live yet, and a
    /// declared guard NEVER borrows the proven word or colour
    /// (UX107-04 — `guarded` rendered green while `level_assumed`
    /// said the receipt was a table lookup).
    pub guard_evidence: Option<String>,
}

impl ClientProbe {
    /// The H5 receipt for THIS host — derived from the SAME probe facts
    /// [`ClientProbe::capability`] climbs (one truth, never recomputed),
    /// plus the provenance the flat level token could not carry.
    #[must_use]
    pub fn capability_receipt(&self, kits: &[KitProbe]) -> HostCapabilityReceipt {
        let kit = kits.iter().find(|k| k.client == self.id);
        let mut components: Vec<String> = Vec::new();
        let mut canaries: Vec<String> = Vec::new();
        let mut missing_rails: Vec<String> = Vec::new();
        if self.current {
            components.push("mcp".to_owned());
        } else {
            missing_rails.push("mcp".to_owned());
        }
        canaries.push(
            if self.present {
                "config-parsed"
            } else {
                "config-absent"
            }
            .to_owned(),
        );
        if self.current && kit.is_some() {
            components.push("kit".to_owned());
            canaries.push("kit-manifest-found".to_owned());
            if kit_ships_hooks(&self.id) {
                components.push("hooks".to_owned());
                // The static table speaks — the canary NAMES the
                // assumption instead of laundering it as a detection.
                canaries.push("hooks-assumed-from-kit".to_owned());
            } else {
                missing_rails.push("hooks".to_owned());
            }
        } else {
            missing_rails.push("kit".to_owned());
            missing_rails.push("hooks".to_owned());
        }
        let has_hooks = components.iter().any(|c| c == "hooks");
        HostCapabilityReceipt {
            host: self.id.clone(),
            version: kit.map(|k| k.version.clone()),
            surface: components
                .last()
                .cloned()
                .unwrap_or_else(|| "none".to_owned()),
            root: Some(self.path.clone()),
            components,
            capability: if self.current {
                self.capability(kits).as_str().to_owned()
            } else {
                "unwired".to_owned()
            },
            canaries,
            repair: if missing_rails.is_empty() {
                None
            } else {
                Some(format!("nika wire {}", self.id))
            },
            missing_rails,
            // Any rung above the oracle floor consulted the static hook
            // table — presence OR absence of hooks is a lookup there.
            level_assumed: self.current && kit.is_some(),
            // The only evidence a hook has today IS that table:
            // `declared`, never `loaded`/`proven` until a live probe
            // and its allow+deny canaries exist.
            guard_evidence: has_hooks.then(|| "declared".to_owned()),
        }
    }
}

/// One receipt per probed host — the `receipts` lane of
/// `doctor --json` (H5 · additive against summary/findings/
/// `adoption_state`, in probe order).
#[must_use]
pub fn capability_receipts(probe: &Probe) -> Vec<HostCapabilityReceipt> {
    probe
        .clients
        .iter()
        .map(|client| client.capability_receipt(&probe.kits))
        .collect()
}

/// Build the real probe from the environment (PRESENCE-only key checks · the
/// value is never bound) + the canonical provider catalog. The ONE builder
/// both `doctor` and `welcome` consume — a second detector would be a
/// second truth.
#[must_use]
pub fn collect(ping: bool) -> Probe {
    // ADR-100 D4 — the knobs GC actually enforces, observed once here so
    // `diagnose` stays pure over the Probe.
    let (retention, retention_notes) = RetentionConfig::from_env();
    // The SAME env composition a run uses: the probe observes the world
    // the runtime will see, overrides included (ProvidersConfig::new()
    // here made --ping probe seeds the operator had redirected away).
    let registry = ProviderRegistry::without_http(nika_runtime::compose::config_from_env());
    let providers = nika_providers::probe::collect_provider_probes(&registry);
    // R4 — the census joins the harness seats the provider rows never
    // carried: SAME rows, ONE derivation (`from_parts` is the pure fold;
    // `collect_seats` is the PATH/auth-surface presence pass).
    let census = AccessCensus::from_parts(&providers, nika_providers::census::collect_seats());
    let probe = Probe {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        config_path: config_path(),
        providers,
        census,
        clients: client_probes(),
        kits: kit_probes(),
        clients_registry: clients_registry::vendored()
            .map(clients_registry::coverage)
            .unwrap_or_default(),
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
        recorded_runs: recorded_run_count(),
        tracked_traces: crate::git::tracked_trace_journals(Path::new(".")),
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

/// The probe LIST derives from the vendored client registry (H6): a
/// client is probed only while the matrix claims its wire target — the
/// hard-coded set is gone, and the per-target MECHANISM (which file,
/// which key) rides the shared [`crate::detect`] table `wire` derives
/// from too (one truth). Row order is the historical one
/// ([`clients_registry::PROBE_MECHANISMS`] mirrors it): findings and
/// receipts ride probe order.
fn client_probes() -> Vec<ClientProbe> {
    crate::detect::sights(home_dir().as_deref(), Path::new("."))
        .iter()
        .filter_map(|sight| match sight.kind {
            crate::detect::ConfigKind::Json => {
                Some(client_probe_any(sight.id, &sight.paths, &sight.server_path))
            }
            // Hermes is YAML — the JSON probe cannot see it (H2): the
            // substring predicate carries recognition.
            crate::detect::ConfigKind::Yaml => sight.paths.first().map(|path| hermes_probe(path)),
        })
        .collect()
}

/// The known kit landings (`update-mirrors.sh` in nika-plugins climbs the
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
/// unreadable surface is silence. The LANDINGS derive from the vendored
/// registry (H6): a kit is probed only while the matrix keeps the
/// client's class-A wire row (the `clients_registry::KIT_MECHANISMS` table).
fn kit_probes() -> Vec<KitProbe> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let kit = |client: &str, version: String| KitProbe {
        client: client.to_owned(),
        version,
    };
    let mut out = Vec::new();
    if clients_registry::registry_ships_kit("cursor")
        && let Some(v) = manifest_version(
            &home
                .join(".cursor")
                .join("plugins")
                .join("local")
                .join("nika")
                .join(".claude-plugin")
                .join("plugin.json"),
        )
    {
        out.push(kit("cursor", v));
    }
    if clients_registry::registry_ships_kit("claude") {
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
    }
    if clients_registry::registry_ships_kit("codex") {
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

/// Two version strings ride different release trains (major.minor) —
/// `false` when either side does not parse (never guess a train). The
/// welcome mirror keys its kit lane on this; doctor's direction-aware
/// diagnosis keeps its own ordering.
#[must_use]
pub fn train_differs(a: &str, b: &str) -> bool {
    match (version_key(a), version_key(b)) {
        (Some((am, an, _)), Some((bm, bn, _))) => (am, an) != (bm, bn),
        _ => false,
    }
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

/// Probe one client at every path its config may live (home · workspace):
/// the most-wired sighting wins (current > stale > present > first).
#[must_use]
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
    let stale = server
        .as_ref()
        .is_some_and(crate::detect::is_stale_mcp_server);
    ClientProbe {
        id: id.to_owned(),
        path: path.display().to_string(),
        present,
        current,
        stale,
    }
}

/// Hermes reads `~/.hermes/config.yaml` — YAML, so the JSON probe above
/// cannot see it. Recognition IS [`crate::detect::hermes_recognized`] —
/// the ONE predicate `wire`'s `patch_hermes` Current arm reads too.
/// There is no stale arg form to detect: Hermes has only ever taken
/// `args: [mcp]`.
fn hermes_probe(path: &Path) -> ClientProbe {
    let present = path.exists();
    let current = present
        && std::fs::read_to_string(path).is_ok_and(|body| crate::detect::hermes_recognized(&body));
    ClientProbe {
        id: "hermes".to_owned(),
        path: path.display().to_string(),
        present,
        current,
        stale: false,
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

fn read_json(path: &Path) -> Option<Value> {
    let body = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&body).ok()
}

/// The home directory, from the environment. The ONE sanctioned reader
/// (the raw `var_os` is lint-denied crate-wide) — every surface that
/// needs `~` comes through here.
#[allow(clippy::disallowed_methods)]
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
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
pub use nika_dap::inventory::collect_workflow_paths;

/// The environment fragment both machine mirrors emit (welcome ·
/// context): client wiring booleans · local provider ids · cloud key
/// COUNTS. Names and counts by construction — no value exists to leak.
#[must_use]
pub fn environment_json(probe: &Probe) -> serde_json::Value {
    let clients: Vec<serde_json::Value> = probe
        .clients
        .iter()
        .map(|c| {
            // UX107-04 — the same honesty companions the doctor
            // receipt carries ride HERE too (additive): a bare
            // `capability: "guarded"` in welcome's JSON was the exact
            // laundering the receipt refuses (a table-declared hook is
            // never a proven one, and every projection must say so).
            let receipt = c.capability_receipt(&probe.kits);
            serde_json::json!({
                "id": c.id,
                "wired": c.current,
                // H5 — the earned rung rides next to the wire boolean
                // (additive): « wired » alone rendered a host parity
                // that does not exist.
                "capability": c.capability(&probe.kits).as_str(),
                "level_assumed": receipt.level_assumed,
                "guard_evidence": receipt.guard_evidence,
            })
        })
        .collect();
    let kits: Vec<serde_json::Value> = probe
        .kits
        .iter()
        .map(|k| serde_json::json!({ "client": k.client, "version": k.version }))
        .collect();
    // H6 — the matrix coverage rides next to the per-host rows
    // (additive): « 6 probed » alone rendered a completeness that does
    // not exist; the not-probed are NAMED.
    let registry = serde_json::json!({
        "declared": probe.clients_registry.declared,
        "wireable": probe.clients_registry.wireable,
        "probed": probe.clients_registry.probed,
        "wire_pending": probe.clients_registry.wire_pending,
        "declared_not_probed": probe.clients_registry.declared_not_probed,
    });
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
        "clients_registry": registry,
        "local_providers": locals,
        "models_pulled": probe.models.count,
        "models_bytes": probe.models.bytes,
        "cloud_keys_present": present,
        "cloud_keys_total": total,
        // R4 — the seats a run can start on ride the mirrors too
        // (additive): a signed-in seat is an inference path, and the
        // machine lanes must say so.
        "seats_ready": probe.census.seats_ready,
    })
}

/// The adoption ladder's journal fact: `.ndjson` files under
/// `.nika/traces`, dir-listing only (no parsing — a torn journal still
/// proves a run happened, and welcome stays instant on a huge store).
fn recorded_run_count() -> usize {
    std::fs::read_dir(Path::new(nika_dap::store::TRACE_DIR)).map_or(0, |entries| {
        entries
            .flatten()
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("ndjson"))
            .count()
    })
}

/// Where this machine sits between « installed » and « running for
/// real » (P0-21 · audit UX 2026-07-30: installed, simulated and
/// real-ready were three flat signals — welcome rendered a raw key
/// ratio, doctor a finding pile, and nothing said WHICH rung you are
/// on). ONE classifier computes it from the probe facts; welcome greets
/// with it, `doctor --json` serializes it — no surface recomputes its
/// own truth.
///
/// AMENDED R4 (2026-08-25): the ladder read the PROVIDER rows only, so
/// a signed-in harness seat — inference that runs on the plan the
/// operator already pays for — was invisible to it, and the first
/// screen said « installed · no inference path » while a seat sat
/// ready. The `SeatReady` rung (between `KeyPresent` and `RealReady`)
/// is earned by the CENSUS (`probe.census.seats_ready`: signed in AND
/// the ACP adapter a session spawns on PATH), never re-derived here.
///
/// Two rungs of the audited scale are deliberately ABSENT, and the
/// absence is the honesty law applied:
/// - `NotInstalled` — unreachable: the observing surface IS the running
///   binary; `welcome`/`doctor` existing proves the install.
/// - `MockProven` — unknowable: the trace journal records the workflow
///   and its verdict but NEVER the serving model (verified against
///   `.nika/traces/*.ndjson`, 2026-07-31), so « a simulated run
///   happened » cannot be told from a real one. The rung is not
///   invented; runs without a live path today read `Installed`.
///   (R4: still absent — the journal still can't name the model, and
///   welcome never pings.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdoptionState {
    /// The floor: no cloud key, no local engagement, no seat, no
    /// recorded run. The keyless engines in the catalog are SEED facts
    /// — present on every machine, so they can never count as
    /// detection.
    Installed,
    /// The local lane shows an operator signal — an endpoint override
    /// (locus `lan`/`remote`), pulled GGUF bytes, or an explicit ping —
    /// but reachability is UNPROVEN (welcome never pings).
    LocalDetected,
    /// An explicit `doctor --ping` measured a local port answering.
    /// Only the opt-in probe earns « reachable » — no ping, no claim.
    LocalReachable,
    /// At least one cloud provider is CONFIGURED (key present). Never
    /// « verified »: cloud endpoints are never pinged, by design.
    KeyPresent,
    /// A signed-in harness seat can serve a run TODAY (R4 · the census
    /// `seats_ready` lane: the sign-in witness AND the ACP adapter on
    /// PATH). Outranks a bare metered key — the operator's own plan is
    /// the sovereign path. Never « verified »: the session judges the
    /// seat's own auth at run (NIKA-1805).
    SeatReady,
    /// A live path today (verified local OR configured cloud OR a ready
    /// seat) AND ≥1 run journal on record. The claim is « path live +
    /// runs on record » — the journal proves the engine ran, never
    /// which model answered, so this rung never says « a real model
    /// answered ».
    RealReady,
}

impl AdoptionState {
    /// The machine token (`doctor --json` · `snake_case`, additive).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::LocalDetected => "local_detected",
            Self::LocalReachable => "local_reachable",
            Self::KeyPresent => "key_present",
            Self::SeatReady => "seat_ready",
            Self::RealReady => "real_ready",
        }
    }

    /// The rung's OWN metric, derived from the probe at render time
    /// (the born-stale law — counts are never typed by hand). Budget:
    /// `metric + " — " + cta` must fit the 67 columns welcome's
    /// `  state      ` row leaves inside 80.
    #[must_use]
    pub fn metric(self, probe: &Probe) -> String {
        match self {
            Self::Installed => "installed · no inference path".to_owned(),
            Self::LocalDetected => {
                let overridden = probe.providers.iter().find(|p| {
                    !p.requires_key
                        && matches!(
                            p.readiness.execution_locus,
                            ExecutionLocus::Lan | ExecutionLocus::Remote
                        )
                });
                if let Some(p) = overridden {
                    format!("{} detected · unproven", p.id)
                } else if probe.models.count > 0 {
                    format!(
                        "{} pulled · unproven",
                        crate::text::count(probe.models.count, "model")
                    )
                } else {
                    // The only remaining signal: a ping that found nothing.
                    "local probed · nothing listening".to_owned()
                }
            }
            Self::LocalReachable => {
                let hit = probe
                    .local_pings
                    .iter()
                    .find_map(|(id, _, state)| match state {
                        PingState::Reachable(ms) => {
                            Some(format!("local reachable · {id} ({ms}ms)"))
                        }
                        PingState::Unreachable => None,
                    });
                hit.unwrap_or_else(|| "local reachable".to_owned())
            }
            Self::KeyPresent => {
                let total = probe.providers.iter().filter(|p| p.requires_key).count();
                let present = probe
                    .providers
                    .iter()
                    .filter(|p| p.requires_key && p.key_present)
                    .count();
                format!("key present · {present} of {total} clouds configured")
            }
            // R4 — the seat rung names the seat (the census owns the
            // list; the rung READS it). The promise is the plan the
            // operator already pays for, never « verified ».
            Self::SeatReady => {
                let seat = probe
                    .census
                    .seats_ready
                    .first()
                    .map_or("a seat", String::as_str);
                format!("seat ready · {seat} signed in")
            }
            Self::RealReady => {
                let path = if probe
                    .local_pings
                    .iter()
                    .any(|(_, _, s)| matches!(s, PingState::Reachable(_)))
                {
                    "endpoint verified"
                } else if probe.census.seats_ready.is_empty() {
                    "path configured"
                } else {
                    "seat ready"
                };
                format!(
                    "real-ready · {} on record · {path}",
                    crate::text::count(probe.recorded_runs, "run")
                )
            }
        }
    }

    /// The rung's OWN next move — one CTA per state, never a shared
    /// « see doctor » (the audit's closure proof: distinct CTA per
    /// distinct state).
    #[must_use]
    pub const fn cta(self) -> &'static str {
        match self {
            Self::Installed => "proof → nika try 01-hello",
            Self::LocalDetected => "start it, then nika doctor --ping",
            Self::LocalReachable => "point a run at it",
            Self::KeyPresent => "ready for a real run",
            Self::SeatReady => "runs on the plan you pay for",
            Self::RealReady => "nika run",
        }
    }
}

/// THE one classifier (P0-21 · amended R4 2026-08-25 — the seat rung
/// reads the census) — pure over the probe facts, so welcome, doctor
/// and the tests all climb the SAME ladder. Highest earned rung wins;
/// a rung is only ever earned by a fact the probe actually measured.
#[must_use]
pub fn adoption_state(probe: &Probe) -> AdoptionState {
    let cloud_configured = probe
        .providers
        .iter()
        .any(|p| p.requires_key && p.key_present && key_auth_of(p).is_ready());
    let local_reachable = probe
        .local_pings
        .iter()
        .any(|(_, _, state)| matches!(state, PingState::Reachable(_)));
    // R4 — a ready seat is a live path today: it earns RealReady beside
    // a key or a verified endpoint, and outranks a bare metered key
    // (the operator's own plan is the sovereign path).
    let seat_ready = !probe.census.seats_ready.is_empty();
    if (cloud_configured || local_reachable || seat_ready) && probe.recorded_runs > 0 {
        return AdoptionState::RealReady;
    }
    if local_reachable {
        return AdoptionState::LocalReachable;
    }
    if seat_ready {
        return AdoptionState::SeatReady;
    }
    if cloud_configured {
        return AdoptionState::KeyPresent;
    }
    let local_engaged = !probe.local_pings.is_empty()
        || probe.models.count > 0
        || probe.providers.iter().any(|p| {
            !p.requires_key
                && matches!(
                    p.readiness.execution_locus,
                    ExecutionLocus::Lan | ExecutionLocus::Remote
                )
        });
    if local_engaged {
        return AdoptionState::LocalDetected;
    }
    AdoptionState::Installed
}

/// Classify a cloud key for doctor (B19 / issue 1273): `sk-invalid` is
/// present-but-implausible, never `configured` / `ready`. The value is
/// classified then dropped — it never reaches a finding string.
#[must_use]
pub(crate) fn key_auth_of(p: &ProviderProbe) -> KeyAuth {
    if !p.requires_key {
        return KeyAuth::Present;
    }
    if !p.key_present {
        return KeyAuth::Absent;
    }
    key_auth_from_value(catalog_prefixes(&p.id), held_key_value(p).as_deref())
}

/// Pure classification: a held value is judged; missing value with
/// `key_present` (a test fixture · a race) stays `Present`.
#[must_use]
pub(crate) fn key_auth_from_value(prefixes: &[&str], value: Option<&str>) -> KeyAuth {
    match value {
        Some(v) => nika_providers::classify_key_value(prefixes, v),
        None => KeyAuth::Present,
    }
}

fn catalog_prefixes(id: &str) -> &'static [&'static str] {
    nika_catalog::find_provider(id).map_or(&[], |p| p.key_prefixes)
}

fn held_key_value(p: &ProviderProbe) -> Option<String> {
    let mut names = vec![format!("NIKA_{}_API_KEY", p.id.to_uppercase())];
    if !p.fix_var.is_empty() && !names.iter().any(|n| n == &p.fix_var) {
        names.push(p.fix_var.clone());
    }
    if let Some(row) = nika_catalog::find_provider(&p.id)
        && !row.env_var.is_empty()
        && !names.iter().any(|n| n == row.env_var)
    {
        names.push(row.env_var.to_owned());
    }
    first_nonempty_env(&names)
}

/// Read the first non-empty env value among `names`, then the caller
/// classifies and drops it. Never printed.
#[allow(clippy::disallowed_methods)] // classify-then-drop · B19 · the value never leaves this fn
fn first_nonempty_env(names: &[String]) -> Option<String> {
    names
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()))
}

/// The R4 seat-escape line an auth-class witness earns when THIS
/// machine has a signed-in harness seat (NIKA-1800 at admission ·
/// NIKA-INFER-001 at the call). Census-derived: printed only when it
/// is TRUE, naming the live pin token — never the retired ACP wrapper
/// id. The collector is presence-only (no ping, no spawn), cheap enough
/// for a refusal path.
#[must_use]
pub fn seat_escape_tail() -> Option<String> {
    AccessCensus::collect(nika_runtime::compose::config_from_env()).seat_escape()
}

/// The auth-class witness predicate (R4): a missing credential
/// (NIKA-INFER-001) or a refused admission (NIKA-1800) earns the seat
/// escape hatch. Pure over the witness text — the journal's own words.
#[must_use]
pub fn auth_class_witness(detail: &str) -> bool {
    detail.contains("NIKA-INFER-001") || detail.contains("NIKA-1800")
}

/// Print the seat escape line when any witness is auth-class AND the
/// census found a ready seat — the ONE seam every run lane calls (the
/// lane picks the stream; `writeln!` to a locked handle, so the print
/// lints stay silent and the bytes stay the lane's choice).
pub fn print_seat_escape<'a, W: std::io::Write>(
    witnesses: impl IntoIterator<Item = &'a str>,
    prefix: &str,
    out: &mut W,
) {
    if !witnesses.into_iter().any(auth_class_witness) {
        return;
    }
    if let Some(tail) = seat_escape_tail() {
        let _ = writeln!(out, "{prefix}{tail}");
    }
}

/// The teaching-side half (R4 · `nika explain`): the auth-class codes
/// (the INFER-001 canon row · the 1800 admission refusal) append the
/// seat escape hatch when THIS machine has a signed-in seat. The tail
/// is census-derived (`Some` only when a seat is ready), so the
/// teaching never promises a path the machine does not have; the low
/// layers keep their own voice (the provider layer cannot see seats —
/// the L4 renderer can).
#[must_use]
pub fn with_seat_tail(code: &str, tail: Option<&str>, text: String) -> String {
    if !matches!(code, "NIKA-INFER-001" | "NIKA-1800") {
        return text;
    }
    match tail {
        Some(tail) => format!("{text}\n  {tail}\n"),
        None => text,
    }
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
mod tests;
