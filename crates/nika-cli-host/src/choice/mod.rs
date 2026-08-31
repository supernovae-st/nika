// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The inference cascade — ONE object, four rungs, the screen is a projection.
//!
//! Source unique for `nika` (TTY and pipe), `welcome`, `doctor --json`,
//! and the `model:` `nika new` writes. Sort: ready first, then scale rank.
//! No editorial. A detected, authenticated harness seat takes the arrow.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::display::theme::{Role, Theme};
use crate::probe::{self, env_present};

const TABLE_YAML: &str = include_str!("models.yaml");
const ALIAS: &str = "nika/gear-one";
const SLOGAN: &str = "Local first. Cloud when you want it.";

/// One barreau of the scale (D-cand-1).
///
/// No `next` here. Every rung used to carry its own copy of the same
/// string, and the JSON mirror served those copies while the TTY
/// derived a fresh one from the directory — so `rungs[].next` kept
/// saying `nika new hello` in a folder that already held the file
/// (#1187). The next step belongs to the SCREEN, not to a rung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Rung {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub ready: bool,
    pub reason: String,
}

/// The cascade — persisté, source unique.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct InferenceChoice {
    pub rungs: Vec<Rung>,
    /// Featured rung id after sort (the arrow).
    pub arrow: String,
    /// What `nika new` writes into `model:`.
    pub chosen_model: String,
    pub slogan: String,
    pub ram_gb: Option<u32>,
    pub local_tier: String,
    pub local_pull: String,
    pub local_download_gb: String,
    /// ACP seats observed (ids only). Doctor --json projects this.
    pub acp_runtimes: Vec<AcpRuntime>,
    /// Env NAMES present, never values.
    pub keys_present: Vec<String>,
    /// Harness seat id when the arrow is ACCESS · what `--access` pins.
    pub chosen_access: Option<String>,
}

/// One harness seat as doctor --json names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AcpRuntime {
    pub id: String,
    pub detected: bool,
    pub authenticated: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Table {
    slogan: String,
    gear_one: Vec<TierRow>,
    keys: Vec<KeyRow>,
    harness_names: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TierRow {
    tier: String,
    min_ram_gb: u32,
    download_gb: f64,
    pull: String,
    #[serde(default)]
    #[allow(dead_code, reason = "models.yaml bit, not a runtime gate yet")]
    verified: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct KeyRow {
    env: String,
    name: String,
    model: String,
}

#[derive(Debug, Clone)]
struct Seat {
    id: String,
    name: String,
    detected: bool,
    authenticated: bool,
    /// Whether the ACP ADAPTER — the binary a session actually spawns —
    /// is on PATH. A person can have `claude` the app and not
    /// `claude-agent-acp` the speaker, and the census keeps the two
    /// halves distinct. Without this field the screen read a signed-in
    /// Claude Code as a runnable seat and said `ready · no API key` for
    /// a path that cannot spawn (gauntlet P1/P4, 2026-08-25). Detected
    /// means "you have the app"; ready means "a run can start", and
    /// only the adapter proves the second.
    adapter_present: bool,
}

fn table() -> Table {
    serde_yaml_bw::from_str(TABLE_YAML).unwrap_or_else(|_| Table {
        slogan: SLOGAN.to_owned(),
        gear_one: Vec::new(),
        keys: Vec::new(),
        harness_names: std::collections::BTreeMap::new(),
    })
}

/// The Hub id the current machine would pull for Gear One.
#[must_use]
#[cfg(feature = "local-infer")]
pub(crate) fn featured_pull() -> String {
    let ram = ram_gb();
    resolve_tier(&table(), ram).map_or_else(|| ALIAS.to_owned(), |t| t.pull.clone())
}

/// Observe this machine and persist the result.
#[must_use]
pub(crate) fn collect() -> InferenceChoice {
    let table = table();
    let keys: Vec<String> = table
        .keys
        .iter()
        .filter(|k| env_present(&k.env))
        .map(|k| k.env.clone())
        .collect();
    let ram = ram_gb();
    let pull = resolve_tier(&table, ram).map_or_else(|| ALIAS.to_owned(), |t| t.pull.clone());
    let choice = collect_from(&Machine {
        ram,
        seats: census_seats(&table),
        pulled: featured_is_installed(&pull),
        keys,
        harness_in_binary: cfg!(feature = "access-harness"),
    });
    let _ = persist(&choice);
    choice
}

struct Machine {
    ram: Option<u32>,
    seats: Vec<Seat>,
    pulled: bool,
    keys: Vec<String>,
    harness_in_binary: bool,
}

fn collect_from(machine: &Machine) -> InferenceChoice {
    let table = table();
    let ram = machine.ram;
    let seats = &machine.seats;
    let tier = resolve_tier(&table, ram);
    let (tier_id, pull, download) = match &tier {
        Some(t) => (
            t.tier.clone(),
            t.pull.clone(),
            format!("{} GB", t.download_gb),
        ),
        None => ("lite".to_owned(), ALIAS.to_owned(), "? GB".to_owned()),
    };
    let local_ready = machine.pulled;
    let local = Rung {
        id: "local".to_owned(),
        name: "Nika Gear One".to_owned(),
        available: true,
        ready: local_ready,
        reason: local_reason(local_ready, ram, &download),
    };

    let cloud = Rung {
        id: "cloud".to_owned(),
        name: "Nika Cloud".to_owned(),
        available: false,
        ready: false,
        reason: "our models, our tools, your workflows online".to_owned(),
    };

    // A seat is READY only if a session can actually start on it: the app
    // is here, it is signed in, AND the ACP adapter it spawns is on PATH.
    let ready_seat = seats
        .iter()
        .find(|s| s.detected && s.authenticated && s.adapter_present);
    let harness = harness_rung(seats, ready_seat, machine.harness_in_binary);
    let keys_present = machine.keys.clone();
    let key_row = table
        .keys
        .iter()
        .find(|k| keys_present.iter().any(|p| p == &k.env));
    let key = key_rung(key_row);

    let mut rungs = vec![local, cloud, harness, key];
    rungs.sort_by_key(sort_key);
    let arrow = rungs
        .iter()
        .find(|r| r.id != "cloud")
        .map_or_else(|| "local".to_owned(), |r| r.id.clone());
    let chosen_model = chosen_model_for(&arrow, &pull, ready_seat, key_row);
    let chosen_access = if arrow == "harness" {
        ready_seat
            .or_else(|| seats.iter().find(|s| s.detected))
            .map(|s| s.id.clone())
    } else {
        None
    };
    let acp_runtimes = seats
        .iter()
        .map(|s| AcpRuntime {
            id: s.id.clone(),
            detected: s.detected,
            authenticated: s.authenticated,
        })
        .collect();
    InferenceChoice {
        rungs,
        arrow,
        chosen_model,
        slogan: table.slogan,
        ram_gb: ram,
        local_tier: tier_id,
        local_pull: pull.clone(),
        local_download_gb: download,
        acp_runtimes,
        keys_present,
        chosen_access,
    }
}

fn harness_rung(seats: &[Seat], ready_seat: Option<&Seat>, in_binary: bool) -> Rung {
    let any_detected = seats.iter().any(|s| s.detected);
    // When no seat is ready, name the one the person is CLOSEST to using:
    // the app they are signed into beats one they merely have installed.
    // Registry order otherwise decides, and registry order has nothing to
    // do with this machine — the first fix here showed `Gemini CLI · sign
    // in to the app` to someone signed into Claude Code, which is a true
    // sentence about the wrong seat.
    let seat = ready_seat
        .or_else(|| seats.iter().find(|s| s.detected && s.authenticated))
        .or_else(|| seats.iter().find(|s| s.detected));
    Rung {
        id: "harness".to_owned(),
        name: seat.map_or_else(|| "Claude Code · Codex".to_owned(), |s| s.name.clone()),
        available: any_detected,
        ready: ready_seat.is_some() && in_binary,
        reason: match (ready_seat, in_binary) {
            (Some(_), true) => {
                "here · runs on the plan you already pay for · no API key".to_owned()
            }
            (Some(_), false) => {
                "here · we'd run on the plan you already pay for · this build cannot sit yet"
                    .to_owned()
            }
            // Detected but not runnable. Two different walls, and a
            // person can only act on the one that is actually theirs:
            // the app is here but unsigned, or (the wrapper class) the
            // ACP adapter a session spawns is simply not installed. The
            // adapter gap is keyed on the CLASS, not the sign-in
            // witness: a command-auth seat reads signed-in only with
            // its adapter, so `authenticated && !adapter` cannot see
            // the wrapper wall (R4).
            (None, _) if any_detected => seat.map_or_else(
                || {
                    "here · we'd run on the plan you already pay for · sign in to the app"
                        .to_owned()
                },
                |s| {
                    if !s.adapter_present && wrapper_class(&s.id) {
                        "here · needs its ACP adapter · the install line: nika doctor".to_owned()
                    } else {
                        "here · we'd run on the plan you already pay for · sign in to the app"
                            .to_owned()
                    }
                },
            ),
            _ => "Claude Code · Codex · Kimi · Gemini · already on this computer".to_owned(),
        },
    }
}

fn key_rung(key_row: Option<&KeyRow>) -> Rung {
    Rung {
        id: "key".to_owned(),
        name: key_row.map_or_else(|| "Your API key".to_owned(), |k| k.name.clone()),
        available: key_row.is_some(),
        ready: key_row.is_some(),
        reason: match key_row {
            Some(k) => format!("{} is set · ready now · billed per run", k.env),
            None => "a vendor key on this computer".to_owned(),
        },
    }
}

/// Why the local rung reads the way it does.
///
/// The RAM used to be repeated here (« this machine has 18 GB ») two
/// lines under the hardware row that already states it — the same two
/// words naming three different things on one screen (#1196). The row
/// above owns the hardware facet; this reason owns the price.
fn local_reason(ready: bool, ram: Option<u32>, download: &str) -> String {
    match (ready, ram) {
        (false, Some(_)) => format!("ours · free · {download} to pull"),
        (true, _) | (false, None) => {
            format!("ours · free · runs on this computer · {download}")
        }
    }
}

fn featured_is_installed(pull: &str) -> bool {
    let Some(root) = nika_models::models_probe().root else {
        return false;
    };
    let Some(dir) = pull_repo_dir(Path::new(&root), pull) else {
        return false;
    };
    dir_has_gguf(&dir)
}

/// `~/.nika/models/<owner>/<repo>` for a Hub `owner/repo[:QUANT]` id.
fn pull_repo_dir(root: &Path, pull: &str) -> Option<PathBuf> {
    let (owner, rest) = pull.split_once('/')?;
    if owner.is_empty() {
        return None;
    }
    let repo = rest.split(':').next().unwrap_or(rest);
    if repo.is_empty() {
        return None;
    }
    Some(root.join(owner).join(repo))
}

fn dir_has_gguf(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let path = entry.path();
        let is_gguf = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"));
        is_gguf && entry.metadata().is_ok_and(|meta| meta.len() > 0)
    })
}

fn sort_key(rung: &Rung) -> (u8, u8) {
    let ready = u8::from(!rung.ready);
    let rank = match rung.id.as_str() {
        "local" => 0,
        "cloud" => 1,
        "harness" => 2,
        "key" => 3,
        _ => 4,
    };
    (ready, rank)
}

fn chosen_model_for(
    arrow: &str,
    local_pull: &str,
    seat: Option<&Seat>,
    key: Option<&KeyRow>,
) -> String {
    match arrow {
        "harness" => seat.map_or_else(|| ALIAS.to_owned(), |s| format!("harness/{}", s.id)),
        "key" => key.map_or_else(|| ALIAS.to_owned(), |k| k.model.clone()),
        _ => local_pull.to_owned(),
    }
}

fn resolve_tier(table: &Table, ram: Option<u32>) -> Option<&TierRow> {
    let ram = ram.unwrap_or(16);
    table
        .gear_one
        .iter()
        .filter(|t| ram >= t.min_ram_gb)
        .max_by_key(|t| t.min_ram_gb)
}

fn ram_gb() -> Option<u32> {
    ram_gb_proc().or_else(ram_gb_sysconf)
}

fn ram_gb_proc() -> Option<u32> {
    parse_memtotal(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

fn parse_memtotal(text: &str) -> Option<u32> {
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("MemTotal:") else {
            continue;
        };
        let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
        return bytes_to_gb(kb.saturating_mul(1024));
    }
    None
}

const GIB: u64 = 1024 * 1024 * 1024;

/// Round-nearest GiB. A 16 GB box whose kernel reports 15.6 must still
/// land on the standard rung — truncating would pick lite.
fn bytes_to_gb(bytes: u64) -> Option<u32> {
    if bytes == 0 {
        return None;
    }
    u32::try_from(bytes.saturating_add(GIB / 2) / GIB)
        .ok()
        .filter(|&gb| gb > 0)
}

fn ram_gb_sysconf() -> Option<u32> {
    ram_gb_phys_pages().or_else(ram_gb_hw_memsize)
}

fn ram_gb_phys_pages() -> Option<u32> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let pages = nix::unistd::sysconf(nix::unistd::SysconfVar::_PHYS_PAGES).ok()??;
        let page = nix::unistd::sysconf(nix::unistd::SysconfVar::PAGE_SIZE).ok()??;
        let bytes = u64::try_from(pages)
            .ok()?
            .saturating_mul(u64::try_from(page).ok()?);
        bytes_to_gb(bytes)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        None
    }
}

fn ram_gb_hw_memsize() -> Option<u32> {
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    {
        use sysctl::Sysctl as _;
        let ctl = sysctl::Ctl::new("hw.memsize").ok()?;
        match ctl.value().ok()? {
            sysctl::CtlValue::U64(n) => bytes_to_gb(n),
            sysctl::CtlValue::U32(n) | sysctl::CtlValue::Uint(n) => bytes_to_gb(u64::from(n)),
            sysctl::CtlValue::S64(n) => bytes_to_gb(u64::try_from(n).ok()?),
            sysctl::CtlValue::Int(n) => bytes_to_gb(u64::try_from(n).ok()?),
            _ => None,
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "freebsd")))]
    {
        None
    }
}

/// The seats the census measured (R4 · one census, one truth). The
/// cascade used to walk PATH on its own — and name the seat by the
/// retired ACP wrapper id (`claude-agent-acp`, NIKA-1802 as a pin),
/// which `chosen_access` then taught to `doctor --json`. The seat id is
/// now the LIVE pin token (`claude-code`) straight from the registry's
/// presence pass (PATH + auth surface, never a spawn, never a
/// credential read).
fn census_seats(table: &Table) -> Vec<Seat> {
    nika_providers::census::collect_seats()
        .into_iter()
        .map(|fact| {
            let name = table
                .harness_names
                .get(&fact.id)
                .cloned()
                .unwrap_or_else(|| fact.id.clone());
            Seat {
                detected: fact.product_present || fact.adapter_present || fact.signed_in,
                authenticated: fact.signed_in,
                adapter_present: fact.adapter_present,
                id: fact.id,
                name,
            }
        })
        .collect()
}

/// The wrapper class: the ACP speaker is a DIFFERENT binary from the
/// product CLI (`claude-agent-acp` ≠ `claude`). For those seats the
/// adapter gap is the actionable one — and the install line lives in
/// `nika doctor`, never teaching the wrapper id as if it were the pin
/// (R4 · it is NIKA-1802 « retired »).
fn wrapper_class(seat_id: &str) -> bool {
    nika_types::access::HarnessRuntime::lookup(seat_id)
        .is_some_and(|rt| rt.detect_bin != rt.acp_bin)
}

impl InferenceChoice {
    /// The cascade rendered against the bare directory door — the
    /// choice-only seam (`Next:` keys on the files in `cwd`, never on
    /// the crate that compiled the binary · gauntlet P15). The screen
    /// itself renders through [`Self::render_human_next`]: `welcome`
    /// knows the sole file's VERDICT, and a verdict outranks a listing.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn render_human_at(&self, theme: Theme, cwd: Option<&Path>) -> String {
        self.render_human_next(theme, &front_door_next(cwd))
    }

    /// Human projection — TTY and pipe render this same product, and
    /// so does `--json`: `next` is computed ONCE by the caller and
    /// handed to every projection (#1187).
    #[must_use]
    pub(crate) fn render_human_next(&self, theme: Theme, next: &str) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "{}", theme.paint(Role::Strong, &self.slogan));
        let _ = writeln!(s);
        let _ = writeln!(s, "Nika runs a plan from a file, with a model you pick.");
        if let Some(gb) = self.ram_gb {
            let _ = writeln!(
                s,
                "{}",
                // « this hardware », not « this machine » — the body's
                // `this machine` section is the ENVIRONMENT one
                // (editors · providers · workspace). One name, one
                // referent: the same two words stood for three things
                // on this screen (#1196 · the A-06 class in prose).
                theme.paint(
                    Role::Dim,
                    &format!(
                        "this hardware · {gb} GB · Gear One {} ({})",
                        self.local_tier, self.local_download_gb
                    )
                )
            );
        }
        let _ = writeln!(s);
        for rung in self.rungs.iter().filter(|r| r.id != "cloud") {
            if !rung.available && !rung.ready && rung.id == "harness" {
                continue;
            }
            if !rung.available && !rung.ready && rung.id == "key" {
                continue;
            }
            let arrow = if rung.id == self.arrow { "▸ " } else { "  " };
            let name = if rung.id == self.arrow {
                theme.paint(Role::Strong, &rung.name)
            } else {
                rung.name.clone()
            };
            let _ = writeln!(s, "{arrow}{name:<22} {}", rung.reason);
        }
        let _ = writeln!(s);
        let _ = writeln!(s, "Next:");
        let _ = writeln!(s, "  {}", theme.paint(Role::Strong, next));
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "Coming: Nika Cloud · our models, our tools, your workflows online."
        );
        s
    }

    /// Machine projection of the cascade (env NAMES only).
    #[must_use]
    pub(crate) fn doctor_cascade_json(&self) -> serde_json::Value {
        let ram = self.ram_gb.unwrap_or(0);
        serde_json::json!({
            "hardware_ok_for_workstation": ram >= 64,
            "hardware_ok_for_default": ram >= 24,
            "hardware_ok_for_standard": ram >= 16,
            "hardware_ok_for_lite": ram >= 8,
            "local_model_ready": self.rungs.iter().any(|r| r.id == "local" && r.ready),
            "local_tier": self.local_tier,
            "acp_runtimes": self.acp_runtimes,
            "keys_present": self.keys_present,
            "nika_cloud_session": serde_json::Value::Null,
            "arrow": self.arrow,
            "chosen_model": self.chosen_model,
            "chosen_access": self.chosen_access,
        })
    }

    /// Versioned welcome envelope fragment.
    ///
    /// `next` is the screen's ONE next step, passed in rather than
    /// derived: an agent reading `rungs[].next` and a human reading
    /// the `Next:` block must be told the same thing (#1187). Cloud is
    /// the one rung with no door — it does not ship yet.
    #[must_use]
    pub(crate) fn welcome_json(&self, next: &str) -> serde_json::Value {
        let rungs: Vec<serde_json::Value> = self
            .rungs
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "name": r.name,
                    "available": r.available,
                    "ready": r.ready,
                    "reason": r.reason,
                    "next": if r.id == "cloud" { "" } else { next },
                })
            })
            .collect();
        serde_json::json!({
            "arrow": self.arrow,
            "next": next,
            "chosen_model": self.chosen_model,
            "chosen_access": self.chosen_access,
            "slogan": self.slogan,
            "rungs": rungs,
        })
    }
}

fn persist(choice: &InferenceChoice) -> std::io::Result<()> {
    let Some(home) = probe::home_dir() else {
        return Ok(());
    };
    let dir = home.join(".nika");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("inference-choice.json");
    let body = serde_json::to_vec_pretty(choice).unwrap_or_default();
    std::fs::write(path, body)
}

/// Replace the top-level `model:` of a scaffold with a model THIS
/// binary can sit on. Hub ids (`unsloth/…`) and harness seat ids are
/// pull / `--access` targets — MODELS refuses them as `model:`.
///
/// The hello lesson is forced onto `mock/echo` (B01 / B17 / I01). Pack
/// `01-hello` may name a local ollama seat; a take must still rehearse
/// keyless. A present `OPENAI_API_KEY` must not rewrite it onto a billed
/// seat.
pub(crate) fn stamp_model_file(path: &Path) -> std::io::Result<()> {
    let body = std::fs::read_to_string(path)?;
    let model = if is_hello_lesson(&body) {
        "mock/echo".to_owned()
    } else {
        runnable_stamp_model(&collect())
    };
    std::fs::write(path, stamp_body(&body, &model))
}

/// The one hello (`nika: hello` · pack `01-hello` / `nika new hello`).
#[must_use]
pub(crate) fn is_hello_lesson(body: &str) -> bool {
    body.lines().any(|line| {
        let t = line.trim();
        t == "nika: hello" || t.starts_with("nika: hello ")
    })
}

/// The `model:` a scaffold may carry. `chosen_model` stays the cascade
/// identity (what we'd pull, which seat we'd pin); the file that runs
/// must resolve in this binary.
#[must_use]
pub(crate) fn runnable_stamp_model(choice: &InferenceChoice) -> String {
    if nika_providers::resolve_refusal(&choice.chosen_model).is_none() {
        choice.chosen_model.clone()
    } else {
        "mock/echo".to_owned()
    }
}

/// Stamp `model:` on a template body. Inserts one if the skeleton has none.
#[must_use]
pub(crate) fn stamp_body(body: &str, model: &str) -> String {
    let scalar = yaml_scalar(model);
    let mut found = false;
    let mut out: Vec<String> = body
        .lines()
        .map(|line| {
            if line.starts_with("model: ") {
                found = true;
                format!("model: {scalar}")
            } else {
                line.to_owned()
            }
        })
        .collect();
    if !found {
        let mut inserted = false;
        let mut with = Vec::new();
        for line in out {
            with.push(line.clone());
            if !inserted && line.starts_with("nika: ") {
                with.push(format!("model: {scalar}"));
                inserted = true;
            }
        }
        out = with;
    }
    let mut text = out.join("\n");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn yaml_scalar(value: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-');
    if !value.is_empty() && value.chars().all(plain) {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "''"))
    }
}

/// The first-wow slug `nika new hello` writes.
pub(crate) const FIRST_WOW_SLUG: &str = "hello";
/// Default dest for the first-wow file.
pub(crate) const FIRST_WOW_DEST: &str = "hello.nika.yaml";

const FIRST_WOW_MODELINE: &str =
    "# yaml-language-server: $schema=https://nika.sh/spec/v1/workflow.schema.json\n";
const FIRST_WOW_PROMPT: &str =
    "Reply with one short sentence confirming you can hear me. No preamble.";

/// `nika new hello` / `nika new hello.nika.yaml` — the one-shot first file.
#[must_use]
pub(crate) fn is_first_wow(from: Option<&str>, dest: Option<&str>) -> bool {
    matches!(from, Some(FIRST_WOW_SLUG | FIRST_WOW_DEST))
        || (from.is_none() && matches!(dest, Some(FIRST_WOW_SLUG | FIRST_WOW_DEST)))
}

/// Where `nika new hello` writes. `hello` as dest still lands a workflow file.
#[must_use]
pub(crate) fn first_wow_dest(dest: Option<&str>) -> &str {
    match dest {
        None | Some(FIRST_WOW_SLUG) => FIRST_WOW_DEST,
        Some(path) => path,
    }
}

/// Write the first-wow workflow. Always the pack `01-hello` body
/// (`mock/echo`) — a present vendor key must not switch the file
/// onto a billed seat (B01 / B17 / I01).
#[must_use]
pub(crate) fn write_first_wow(dest: &Path, force: bool) -> crate::output::VerbOutput {
    write_first_wow_from(dest, force, &collect())
}

#[must_use]
pub(crate) fn write_first_wow_from(
    dest: &Path,
    force: bool,
    choice: &InferenceChoice,
) -> crate::output::VerbOutput {
    if dest.exists() && !force {
        return crate::output::VerbOutput::env(format!(
            "{} exists — pass --force to overwrite",
            dest.display()
        ));
    }
    let dest_s = dest.display().to_string();
    let body = first_wow_yaml(choice).replace("examples/01-hello.nika.yaml", &dest_s);
    match std::fs::write(dest, body) {
        Ok(()) => crate::output::VerbOutput::ok(format!(
            "wrote {} · {}",
            dest.display(),
            first_wow_next(dest)
        )),
        Err(e) => crate::output::VerbOutput::env(format!("cannot write {}: {e}", dest.display())),
    }
}

fn first_wow_next(dest: &Path) -> String {
    // Gauntlet W2 (P01 P02 P04 P05 P07 P12 P15): `--access harness` on
    // an `agent:` file with no `model:` is NIKA-INFER-001. The Next
    // line is a copy-paste that must exit 0 on a keyless machine.
    format!("nika run {}", dest.display())
}

/// Every shape [`front_door_next`] can return, in the placeholder form
/// the concierge's parse ratchet replays against the live clap tree.
///
/// A hand-written mirror of a function is a lie waiting to happen, so
/// `door_shapes_mirror_the_real_door` runs the real function against
/// real directories and proves this list is its exact image.
pub(crate) const DOOR_SHAPES: [&str; 3] = ["nika new hello", "nika run <file>", "nika run"];

/// `Next:` after the user already has a file. Teaching `nika new hello`
/// again is a dead end (`exists — pass --force`) — gulf of execution,
/// and the tool's own advice caused it (gauntlet P15).
pub(crate) fn front_door_next(cwd: Option<&Path>) -> String {
    let Some(dir) = cwd else {
        return "nika new hello".to_owned();
    };
    let files = cwd_workflows(dir);
    match files.as_slice() {
        [] => "nika new hello".to_owned(),
        [one] => first_wow_next(Path::new(one)),
        many if many.iter().any(|n| n == FIRST_WOW_DEST) => {
            first_wow_next(Path::new(FIRST_WOW_DEST))
        }
        _ => "nika run".to_owned(),
    }
}

/// Workflow files sitting in THIS directory. Greeting stays instant —
/// no walk into `node_modules` / a monorepo.
fn cwd_workflows(cwd: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(cwd) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".nika.yaml") || name.ends_with(".nika.yml") {
                Some(name.into_owned())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
}

/// The first-wow body is pack `01-hello` — one hello, always `mock/echo`.
/// The cascade still chooses a billed seat for OTHER scaffolds; hello
/// is the rehearsal that must run on a keyless (and a keyed) machine.
#[must_use]
pub(crate) fn first_wow_yaml(_choice: &InferenceChoice) -> String {
    if let Some(body) = nika_pack::example("01-hello") {
        // Pack 01-hello may name ollama; the first-wow take is always
        // the rehearsal seat (B01). Comments on the model: line go with
        // the stamp.
        stamp_body(body, "mock/echo")
    } else {
        let model = yaml_scalar("mock/echo");
        format!(
            "{FIRST_WOW_MODELINE}nika: hello\nmodel: {model}\npermits: {{}}\ntasks:\n  greet:\n    infer:\n      prompt: \"{FIRST_WOW_PROMPT}\"\n      max_tokens: 64\noutputs:\n  greeting: ${{{{ tasks.greet.output }}}}\n"
        )
    }
}

/// Pack skeletons — the cascade stamps them at `nika new`.
#[must_use]
#[cfg(test)]
pub(crate) fn pack_template_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nika-pack/pack/templates")
}

#[cfg(test)]
mod tests;
