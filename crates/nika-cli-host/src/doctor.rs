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

// The probe layer (structs + collectors) lives in `crate::probe` — the ONE
// detection engine `doctor` and `welcome` share. Re-exported crate-internally
// so this module's tests keep their historical names.
use crate::clients_registry::RegistryCoverage;
use crate::display::theme::{Role, Theme};
use crate::output::{VerbOutput, exit};
pub(crate) use crate::probe::{
    AdoptionState, CapabilityLevel, ClientProbe, HostCapabilityReceipt, ImageProbe, KitProbe,
    ModelsProbe, PingState, PricingProbe, Probe, ProviderProbe, TtsProbe,
};
use nika_providers::probe::{ExecutionLocus, KeyAuth};

/// Severity of one diagnosis line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// `✔` healthy.
    Ok,
    /// `⚠` advisory (the run may still work).
    Warn,
    /// `✖` a hard environment problem (drives `exit 3`).
    Fail,
}

impl Level {
    /// The semantic colour role — the SAME closed vocabulary the run
    /// storyboard speaks (`Role` · theme.rs): green ok · yellow advisory ·
    /// red hard-fail. Never decorative.
    const fn role(self) -> Role {
        match self {
            Self::Ok => Role::Good,
            Self::Warn => Role::Warn,
            Self::Fail => Role::Bad,
        }
    }

    fn glyph(self) -> char {
        match self {
            Self::Ok => '✔',
            Self::Warn => '⚠',
            Self::Fail => '✖',
        }
    }
}

/// One diagnosis line · a problem carries the exact PRINTED fix (never run).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Finding {
    pub level: Level,
    pub label: String,
    pub detail: String,
    pub fix: Option<String>,
}

/// Pure diagnosis → ordered findings (binary · config · cloud providers ·
/// local summary · the inference-readiness gate).
#[must_use]
pub fn diagnose(probe: &Probe) -> Vec<Finding> {
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
    out.extend(retention_findings(&probe.retention, &probe.retention_notes));
    out.extend(tracked_traces_finding(probe.tracked_traces));
    out.push(Finding {
        level: Level::Ok,
        label: "mcp".to_owned(),
        detail: "available via `nika mcp` (9 read-only tools · nika_check through nika_tools)"
            .to_owned(),
        fix: None,
    });
    out.push(access_class_finding());
    // #891 — the sandbox row rides the ONE selection's decision (#888),
    // observed once here (the sidecar precedent · diagnose stays pure).
    out.push(sandbox_finding(&crate::probe::sandbox_probe()));
    out.extend(sidecar_finding());
    out.extend(models_finding(&probe.models));
    if let Ok(cwd) = std::env::current_dir() {
        out.extend(serve_http_door(&cwd));
    }

    for client in &probe.clients {
        out.push(client_finding(client, &probe.kits));
    }

    for kit in &probe.kits {
        out.push(kit_finding(kit, &probe.version));
    }

    // H6 — the matrix coverage row closes the client lane: how much of
    // the ONE registry doctor actually sees, the not-probed NAMED.
    out.push(registry_finding(&probe.clients_registry));

    provider_findings(probe, &mut out);

    out
}

/// `--access` vocabulary (NIKA-1802). Both classes and live harness seat
/// ids are pins; ACP wrapper executable names remain implementation detail.
fn access_class_finding() -> Finding {
    let vocabulary = nika_types::access::AccessClass::ALL
        .map(nika_types::access::AccessClass::as_str)
        .join(" · ");
    Finding {
        level: Level::Ok,
        label: "access".to_owned(),
        detail: format!(
            "--access classes: {vocabulary} · harness seats: {} \
             (ACP wrapper ids are not pins)",
            nika_types::access::HarnessRuntime::vocabulary()
        ),
        fix: None,
    }
}

/// The sandbox row (#891 · #822 P1) — the ONE selection's decision,
/// honestly rendered: a confined backend names its mechanism (and the
/// Linux residual), a `noop` WARNS with the exact per-OS fix — doctor
/// never greenwashes an unconfined spawn as "sandboxed".
fn sandbox_finding(sandbox: &crate::probe::SandboxProbe) -> Finding {
    if sandbox.confined {
        let detail = if sandbox.backend == "landlock" {
            "Linux sandbox (bubblewrap) · backend id: landlock · host-granular net allowlist = \
             follow-on (exec/MCP confine as allow until it lands · #893)"
                .to_owned()
        } else {
            format!(
                "{} · exec and external MCP spawns confined to the declared boundary",
                sandbox.backend
            )
        };
        Finding {
            level: Level::Ok,
            label: "sandbox".to_owned(),
            detail,
            fix: None,
        }
    } else {
        Finding {
            level: Level::Warn,
            label: "sandbox".to_owned(),
            detail: "no OS sandbox backend — exec and external MCP spawns run UNCONFINED (the \
                     declared permits still gate at the builtin/fetch seams)"
                .to_owned(),
            fix: Some(
                if cfg!(target_os = "linux") {
                    "install bubblewrap (apt install bubblewrap) — then re-run `nika doctor`"
                } else {
                    "macOS ships sandbox-exec with the OS — a missing launcher means a broken host"
                }
                .to_owned(),
            ),
        }
    }
}

/// The provider rows — display order practices the presentation lock
/// (teaching surface · the first screen after install): the sovereign
/// keyless line leads, then the cloud rows with mistral (EU ·
/// open-weight) first. The registry's seed order is functional and
/// stays untouched — this is a render sort only.
fn provider_findings(probe: &Probe, out: &mut Vec<Finding>) {
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
        // P0-20 · one extra row per override-pointed engine (LAN/remote),
        // loopback stays the one summary line above.
        out.extend(
            probe
                .providers
                .iter()
                .filter(|p| !p.requires_key)
                .filter_map(local_locus_finding),
        );
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
    // empty catalog). A merely-unset cloud key is a ⚠ above, never fatal —
    // and a signed-in harness seat IS an inference path (R4 · the census
    // joins what the provider rows alone never saw; the ladder's SeatReady
    // rung reads the same `seats_ready`).
    if cloud_keys == 0 && local_ids.is_empty() && probe.census.seats_ready.is_empty() {
        out.push(Finding {
            level: Level::Fail,
            label: "providers".to_owned(),
            detail: "no inference path — no cloud key, no local server, no signed-in harness seat"
                .to_owned(),
            fix: Some(
                "export <PROVIDER>_API_KEY=… · run a local server (ollama · llama.cpp · vLLM) · \
                 or sign in to a harness seat (`--access claude-code`)"
                    .to_owned(),
            ),
        });
    }
}

/// Operator-visible mint. Same recipe `nika serve --bind` prints.
const SERVE_TOKEN_MINT: &str =
    "umask 077 && openssl rand -hex 24 > .nika/serve.token && chmod 600 .nika/serve.token";

/// HTTP door readiness. Silent when the cwd has no token file — the
/// door is opt-in. Never claims TLS: this process does not terminate it.
/// Policy matches `nika-serve` `BearerToken::from_file` (symlink, mode,
/// 32–512 graphic ASCII) so doctor cannot green a file serve would refuse.
pub(crate) fn serve_http_door(root: &std::path::Path) -> Vec<Finding> {
    let token = root.join(".nika/serve.token");
    let Ok(meta) = std::fs::symlink_metadata(&token) else {
        return Vec::new();
    };
    if meta.file_type().is_symlink() {
        return vec![serve_token_refused(
            "token file is a symlink · `nika serve` will refuse it",
        )];
    }
    if !meta.is_file() {
        return vec![serve_token_refused(
            "token path is not a regular file · `nika serve` will refuse it",
        )];
    }
    if let Some(finding) = serve_token_group_readable(&token) {
        return vec![finding];
    }
    if let Some(finding) = serve_token_bytes_refused(&token) {
        return vec![finding];
    }
    vec![Finding {
        level: Level::Ok,
        label: "serve".to_owned(),
        detail: "token owner-only · TLS is the reverse proxy's job \
                 (this process does not terminate TLS)"
            .to_owned(),
        fix: None,
    }]
}

fn serve_token_refused(detail: &str) -> Finding {
    Finding {
        level: Level::Fail,
        label: "serve".to_owned(),
        detail: detail.to_owned(),
        fix: Some(SERVE_TOKEN_MINT.to_owned()),
    }
}

fn serve_token_bytes_refused(path: &std::path::Path) -> Option<Finding> {
    let mut bytes = std::fs::read(path).ok()?;
    if bytes.ends_with(b"\r\n") {
        bytes.truncate(bytes.len() - 2);
    } else if matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    if (32..=512).contains(&bytes.len()) && bytes.iter().all(u8::is_ascii_graphic) {
        return None;
    }
    Some(serve_token_refused(
        "token file is not 32–512 visible ASCII bytes · `nika serve` will refuse it",
    ))
}

#[cfg(unix)]
fn serve_token_group_readable(path: &std::path::Path) -> Option<Finding> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = std::fs::metadata(path).ok()?.permissions().mode();
    // Unix other/group bits; same mask as nika-serve BearerToken.
    #[allow(clippy::verbose_bit_mask)]
    if mode & 0o077 == 0 {
        return None;
    }
    Some(Finding {
        level: Level::Fail,
        label: "serve".to_owned(),
        detail: "token file is group/world-readable · `nika serve` will refuse it".to_owned(),
        fix: Some("umask 077 && chmod 600 .nika/serve.token".to_owned()),
    })
}

#[cfg(not(unix))]
fn serve_token_group_readable(_path: &std::path::Path) -> Option<Finding> {
    None
}

/// The sovereign sidecar lane (ADR-091) — a row ONLY when this binary was
/// built with it (a BUILD fact · presence, never a probe): the default
/// build's doctor stays byte-identical, so the row set is per-axis (a
/// `Vec`, not an `Option` — the arity depends on the compile). Its
/// models-dir half lives in [`models_finding`] (issue #146 closed the
/// deferral this note used to carry).
fn sidecar_finding() -> Vec<Finding> {
    #[cfg(feature = "local-infer")]
    {
        vec![Finding {
            level: Level::Ok,
            label: "sidecar".to_owned(),
            detail: "local inference built in — `nika model serve --model <path.gguf>` \
                     (loopback · OpenAI-compatible)"
                .to_owned(),
            fix: None,
        }]
    }
    #[cfg(not(feature = "local-infer"))]
    {
        Vec::new()
    }
}

/// The models-dir half of the sidecar lane (issue #146 — the deferral
/// `sidecar_finding` documented): the dir + count once anything is
/// pulled (ANY build — acquisition is not feature-gated), and a teach
/// row on a sidecar build with nothing to serve. A default build with
/// zero pulls stays byte-identical.
fn models_finding(models: &ModelsProbe) -> Vec<Finding> {
    if models.count > 0 {
        let root = models.root.as_deref().unwrap_or("~/.nika/models");
        return vec![Finding {
            level: Level::Ok,
            label: "models".to_owned(),
            detail: format!(
                "{root} — {} GGUF · {} (`nika model list`)",
                models.count,
                nika_models::store::human_size(models.bytes)
            ),
            fix: None,
        }];
    }
    #[cfg(feature = "local-infer")]
    {
        vec![Finding {
            level: Level::Warn,
            label: "models".to_owned(),
            detail: "none pulled yet — the sidecar has nothing to serve".to_owned(),
            fix: Some(format!(
                "nika model pull {}",
                crate::choice::featured_pull()
            )),
        }]
    }
    #[cfg(not(feature = "local-infer"))]
    {
        Vec::new()
    }
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
        |url| format!("local → {}", redact_userinfo(url)),
    );
    Finding {
        level: Level::Ok,
        label: "image".to_owned(),
        detail: if wired.is_empty() {
            format!("mock ready · {local_part} · no cloud image key set")
        } else {
            format!(
                "mock ready · {} {} present · {local_part}",
                wired.join(" · "),
                if wired.len() == 1 { "key" } else { "keys" }
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
    // LIST RATES said here on purpose: private/proxy/negotiated pricing
    // is not reflected (the override file is the roadmapped answer) —
    // silent wrong-cost is the honesty law's blind spot otherwise.
    //
    // #1179 · `p.rules` counts PRICE ROWS, not models. Calling them
    // « models » put « 633 models » one command away from `nika
    // catalog`'s « 69 models » — two inventories under one word, an
    // order of magnitude apart, both on a first session. Every number
    // NAMES its facet (RAMS-12 · A-06), and the facet here is the
    // snapshot's rate table: several patterns can price one model, and
    // a pattern can price models this catalog never lists.
    let identity = format!(
        "{} price rules · {} providers priced · snapshot {} · {} · list rates (public catalog)",
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
        |url| format!("local → {}", redact_userinfo(url)),
    );
    Finding {
        level: Level::Ok,
        label: "tts".to_owned(),
        detail: if wired.is_empty() {
            format!("mock ready · {local_part} · no cloud speech key set")
        } else {
            format!(
                "mock ready · {} {} present · {local_part}",
                wired.join(" · "),
                if wired.len() == 1 { "key" } else { "keys" }
            )
        },
        fix: None,
    }
}

/// ADR-100 D4 — the trace-retention knobs ride the env config surface;
/// doctor reports the values GC actually enforces, and additionally
/// speaks when a typo'd knob fell back to its default (a knob silently
/// doing nothing would be hidden magic).
fn retention_findings(
    retention: &crate::retention::RetentionConfig,
    notes: &[String],
) -> Vec<Finding> {
    let mut out = vec![Finding {
        level: Level::Ok,
        label: "traces".to_owned(),
        detail: format!(
            "{} (NIKA_TRACE_KEEP · NIKA_TRACE_MAX_AGE_DAYS · NIKA_TRACE_BUDGET_MB)",
            retention.summary()
        ),
        fix: None,
    }];
    for note in notes {
        out.push(Finding {
            level: Level::Warn,
            label: "traces".to_owned(),
            detail: note.clone(),
            fix: Some("set a whole number · or unset to keep the default".to_owned()),
        });
    }
    out
}

/// The trace-leak signal (the other half of init's `.gitignore`
/// guarantee): journals under `.nika/traces` carry model outputs, file
/// contents and tool arguments, so a repo that TRACKS them is one push
/// away from publishing them. Init covers repos founded from now on —
/// this row is for the ones founded before it did. A Warn, never a
/// Fail (the env works; the hygiene debt is the operator's call), and
/// never folded into the calm line (it is none of the healthy
/// machine's three advisory classes). Diagnose-only: the exact remedy
/// is printed, never run — `git rm` needs `-r` to take a directory.
fn tracked_traces_finding(tracked: Option<usize>) -> Vec<Finding> {
    match tracked {
        Some(n) if n > 0 => vec![Finding {
            level: Level::Warn,
            label: "traces".to_owned(),
            detail: format!(
                "{} under .nika/traces tracked by git — journals carry model outputs · file contents · tool arguments",
                crate::text::count(n, "run journal")
            ),
            fix: Some(
                "git rm -r --cached .nika/traces   # files stay on disk · commit the removal"
                    .to_owned(),
            ),
        }],
        _ => Vec::new(),
    }
}

/// `major.minor` from a semver-ish string — the kit handshake compares
/// TRAINS, not patches (the kit is cut per release train; a patch
/// release ships binary-only, so patch drift is not a finding).
fn major_minor(v: &str) -> Option<(u64, u64)> {
    let mut parts = v.split('.');
    let maj = parts.next()?.parse().ok()?;
    let min = parts.next()?.parse().ok()?;
    Some((maj, min))
}

/// One installed plugin-kit row — the kit↔binary handshake nothing
/// client-side used to surface (the drift contract lived only in the
/// nika-plugins CI). Same train → ✔. A lagging kit names the refresh
/// command for ITS client (Claude Code climbs TWO rungs); a kit riding
/// ahead names the binary upgrade. An unparseable manifest version
/// warns and never guesses a train.
fn kit_finding(kit: &KitProbe, bin_version: &str) -> Finding {
    let (Some(k), Some(b)) = (major_minor(&kit.version), major_minor(bin_version)) else {
        return Finding {
            level: Level::Warn,
            label: "kit".to_owned(),
            detail: format!(
                "{} plugin kit — unparseable version ({})",
                kit.client, kit.version
            ),
            fix: None,
        };
    };
    if k == b {
        return Finding {
            level: Level::Ok,
            label: "kit".to_owned(),
            detail: format!(
                "{} plugin kit {} — on the binary's train",
                kit.client, kit.version
            ),
            fix: None,
        };
    }
    if k < b {
        let fix = match kit.client.as_str() {
            "claude" => {
                "claude plugin marketplace update nika, then claude plugin update nika@nika"
            }
            "codex" => "codex plugin marketplace upgrade nika",
            "cursor" => "re-sync the local drop: scripts/update-mirrors.sh (nika-plugins checkout)",
            _ => "refresh the nika plugin from its marketplace",
        };
        return Finding {
            level: Level::Warn,
            label: "kit".to_owned(),
            detail: format!(
                "{} plugin kit {} lags the binary ({bin_version})",
                kit.client, kit.version
            ),
            fix: Some(fix.to_owned()),
        };
    }
    Finding {
        level: Level::Warn,
        label: "kit".to_owned(),
        detail: format!(
            "{} plugin kit {} rides ahead of the binary ({bin_version})",
            kit.client, kit.version
        ),
        fix: Some("brew upgrade nika".to_owned()),
    }
}

fn client_finding(client: &ClientProbe, kits: &[KitProbe]) -> Finding {
    if client.current {
        let level = client.capability(kits);
        // The guard rung above the oracle floor rests on the static
        // hook table today (the receipt's `level_assumed`) — the line
        // must say so in its own words (UX107-04).
        let assumed = kits.iter().any(|k| k.client == client.id);
        let (word, surfaces) = guard_wording(level, assumed);
        return Finding {
            level: Level::Ok,
            label: "agent".to_owned(),
            detail: format!(
                "{} · Nika MCP oracle wired · {} ({}) at {}",
                client.id, word, surfaces, client.path
            ),
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

/// The H6 coverage row — the client matrix counts, DERIVED from the
/// vendored registry (never a hand count). A wireable client without a
/// probe mechanism is NAMED (declared-not-probed): doctor says how much
/// of the matrix it sees, and the unseen is listed, not dropped. An
/// unparsed registry is a loud warning, never a silent zero.
fn registry_finding(cov: &RegistryCoverage) -> Finding {
    if cov.declared == 0 {
        return Finding {
            level: Level::Warn,
            label: "registry".to_owned(),
            detail: "client registry unavailable — the vendored snapshot did not parse".to_owned(),
            fix: None,
        };
    }
    let mut detail = format!(
        "client matrix · {} declared · {} wireable · {} probed",
        cov.declared, cov.wireable, cov.probed
    );
    if !cov.declared_not_probed.is_empty() {
        let _ = write!(
            detail,
            " · {} declared-not-probed ({})",
            cov.declared_not_probed.len(),
            cov.declared_not_probed.join(" · ")
        );
    }
    if cov.wire_pending > 0 {
        let _ = write!(
            detail,
            " · {} wire-pending (next release)",
            cov.wire_pending
        );
    }
    Finding {
        level: Level::Ok,
        label: "registry".to_owned(),
        detail,
        fix: None,
    }
}

/// The level word plus the parenthetical that keeps it auditable at a
/// glance (P0-9: oracle-only ≠ guarded, and the line must SHOW why).
/// The guard rungs speak the evidence ladder (UX107-04): a hook the
/// static table declares is `guard-declared … unproven`, and ONLY a
/// live allow+deny canary on this host version/surface may ever say
/// `guarded … proven live`. A declared guard never borrows the proven
/// word — same glyph, different sentence, no laundering.
const fn guard_wording(level: CapabilityLevel, assumed: bool) -> (&'static str, &'static str) {
    match (level, assumed) {
        (CapabilityLevel::OracleOnly, _) => ("oracle-only", "mcp · no hooks"),
        (CapabilityLevel::AuthoringEnabled, _) => ("authoring-enabled", "mcp + kit · no hooks"),
        (CapabilityLevel::Guarded, true) => {
            ("guard-declared", "kit ships hooks · unproven in session")
        }
        (CapabilityLevel::Guarded, false) => ("guarded", "kit + hooks · proven live"),
        (CapabilityLevel::FullyIntegrated, _) => ("fully-integrated", "kit + hooks + runtime"),
    }
}

/// Exit code · `ENV(3)` when any `Fail`, else `OK(0)`.
#[must_use]
pub fn exit_code(findings: &[Finding]) -> u8 {
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
/// agents/CI branch on `summary.fail` instead of parsing glyphs. P0-21:
/// the adoption rung rides alongside (additive) — ONE state token the
/// flat findings could never express. H5: the per-host runtime receipts
/// ride alongside too (additive) — what each host earned, what was
/// verified versus assumed, and the repair, per host. R4: the access
/// census rides alongside (additive) — every path with its custody and
/// fix, the ready seats, the best path; one read, never recomputed.
#[must_use]
pub fn render_json(
    findings: &[Finding],
    state: AdoptionState,
    receipts: &[HostCapabilityReceipt],
    census: &nika_providers::census::AccessCensus,
) -> String {
    let count = |lvl: Level| findings.iter().filter(|f| f.level == lvl).count();
    let paths: Vec<serde_json::Value> = census
        .paths
        .iter()
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "class": p.class.as_str(),
                "configured": p.configured,
                "custody": p.custody,
                "fix": p.fix_line,
            })
        })
        .collect();
    let payload = serde_json::json!({
        "summary": {
            "ok": count(Level::Ok),
            "warn": count(Level::Warn),
            "fail": count(Level::Fail),
        },
        "adoption_state": state.as_str(),
        "findings": findings,
        "receipts": receipts,
        "access": {
            "paths": paths,
            "seats_ready": census.seats_ready,
            "best": census.best.as_ref().map(|p| p.id.clone()),
        },
    });
    format!("{payload:#}")
}

fn with_cascade(raw: String) -> String {
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return raw;
    };
    v["cascade"] = crate::choice::collect().doctor_cascade_json();
    format!("{v:#}")
}

/// The fixed label column (nextest school: one grid, computed on RAW text).
/// Every label `diagnose` can emit fits STRICTLY inside it (pinned by
/// `every_label_fits_the_fixed_column`) so the detail column never shears.
const LABEL_COL: usize = 10;

/// Render the findings through the ONE colour seam (`Theme` · semantic
/// never decorative — the same law welcome/run obey). Doctor rows carry NO
/// durations, so the nextest discipline reduces to the status/label
/// columns — a fixed 1-cell status glyph + the fixed `LABEL_COL` label
/// cell, both laid out on RAW text and painted AFTER (ANSI escapes never
/// enter width arithmetic — the same law as `Theme::glyph`). The sober
/// register (colour off · links off · every pipe) is byte-identical to the
/// themeless render it replaces.
///
/// B-8b (the 2026-07-31 gauntlet): a healthy keyless machine printed 13+
/// ⚠ rows — every unwired agent, every unconfigured provider, the
/// config-less default — and the alarm glyph taught the user to ignore
/// it. `verbose: false` folds those three advisory classes into ONE calm
/// line (`--verbose` unfolds each); the verdict line keeps counting the
/// truth, the machine lane (`render_json`) always carries every finding.
#[must_use]
pub fn render(findings: &[Finding], verbose: bool, theme: Theme) -> String {
    let mut s = String::new();
    let count = |level: Level| findings.iter().filter(|f| f.level == level).count();
    let (ok, warn, fail) = (count(Level::Ok), count(Level::Warn), count(Level::Fail));
    let verdict = if fail > 0 { Level::Fail } else { Level::Ok };
    let glyph = |level: Level| theme.paint(level.role(), &level.glyph().to_string());
    let _ = writeln!(s, "{} {ok} ok · {warn} warn · {fail} fail", glyph(verdict));
    for f in findings {
        if !verbose && calm_foldable(f) {
            continue;
        }
        let _ = writeln!(
            s,
            "{} {:<LABEL_COL$} {}",
            glyph(f.level),
            f.label,
            link_targets(theme, &f.detail)
        );
        if let Some(fix) = &f.fix {
            let _ = writeln!(s, "  fix: {}", link_targets(theme, fix));
        }
    }
    if !verbose {
        let agents = findings
            .iter()
            .filter(|f| calm_foldable(f) && f.label == "agent")
            .count();
        let providers = findings
            .iter()
            .filter(|f| calm_foldable(f) && f.label == "provider")
            .count();
        let config = findings
            .iter()
            .any(|f| calm_foldable(f) && f.label == "config");
        let mut classes = Vec::new();
        if agents > 0 {
            classes.push(format!("{agents} agents unwired"));
        }
        if providers > 0 {
            classes.push(format!("{providers} providers unconfigured"));
        }
        if config {
            classes.push("config defaults".to_owned());
        }
        if !classes.is_empty() {
            let _ = writeln!(
                s,
                "{} {:<LABEL_COL$} a healthy machine's notes — {} · nika doctor --verbose unfolds each",
                theme.paint(Role::Dim, "·"),
                theme.paint(Role::Dim, "advisory"),
                theme.paint(Role::Dim, &classes.join(" · "))
            );
        }
    }
    s
}

/// The B-8b fold classes — advisory by construction on a healthy
/// machine: an unwired agent (wiring is opt-in), an unconfigured
/// provider (keyless is a valid choice), the config-less default
/// (built-ins are designed). A Warn of any OTHER kind (a stale snapshot
/// · a kit drift · a dead ping) never folds — it earned its row.
fn calm_foldable(f: &Finding) -> bool {
    f.level == Level::Warn
        && match f.label.as_str() {
            "agent" => f.detail.contains("not wired"),
            "provider" => f.detail.contains("not configured"),
            "config" => true,
            _ => false,
        }
}

/// OSC-8-wrap the linkable targets inside ONE printed doctor line — an
/// existing file path (`/…` or `./…` · canonicalize-gated through the
/// [`crate::output::linked_path`] seam: a link that cannot open stays plain)
/// or an `https://` URL. Token-bounded on spaces, and the printed TEXT never
/// changes — with the `links` capability off (every pipe · `--hyperlink
/// never`) the line is returned VERBATIM, so the sober register stays
/// byte-frozen.
fn link_targets(theme: Theme, text: &str) -> String {
    if !theme.links {
        return text.to_owned();
    }
    let linked: Vec<String> = text
        .split(' ')
        .map(|token| {
            if token.starts_with("https://") {
                theme.link(token, token)
            } else if token.starts_with('/') || token.starts_with("./") {
                crate::output::linked_path(theme, token)
            } else {
                token.to_owned()
            }
        })
        .collect();
    linked.join(" ")
}

/// One cloud-provider row — readiness is a LADDER (P0-5): the line
/// separates RECOGNIZED (the name resolves) from CONFIGURED (the key is
/// present) and never claims reachability — only the opt-in `--ping`
/// lane may say that, and cloud endpoints are never pinged.
///
/// B19: `sk-invalid` is `implausible`, 401/402 are their own rungs —
/// none of those print as `configured` / `ready`.
fn provider_finding(p: &ProviderProbe) -> Finding {
    provider_finding_auth(p, crate::probe::key_auth_of(p))
}

fn provider_finding_auth(p: &ProviderProbe, auth: KeyAuth) -> Finding {
    // An override endpoint off the vendor default is the exception that
    // gets the ink (P0-20): a proxy/LAN front is NAMED with its locus,
    // never read as « the vendor's cloud ».
    let locus_note = match p.readiness.execution_locus {
        ExecutionLocus::Lan | ExecutionLocus::Remote => format!(
            " · endpoint {} ({} — not the vendor default)",
            redact_userinfo(&p.endpoint),
            p.readiness.execution_locus.label()
        ),
        ExecutionLocus::Loopback | ExecutionLocus::Cloud | ExecutionLocus::Unknown => String::new(),
    };
    let access = p.readiness.access;
    match auth {
        KeyAuth::Ok | KeyAuth::Present => {
            // The exception gets the ink, not the norm: every provider is
            // schema-native except the instruction-fallback clouds (deepseek —
            // no json_schema in its API), and an operator picking a model for
            // a structured workflow wants that fact on the health surface.
            let detail = if p.structured_native {
                format!(
                    "{} — {access} · recognized · configured (key present){locus_note}",
                    p.id
                )
            } else {
                format!(
                    "{} — {access} · recognized · configured (key present) · structured output via instruction + local validation \
                     (no native json_schema){locus_note}",
                    p.id
                )
            };
            Finding {
                level: Level::Ok,
                label: "provider".to_owned(),
                detail,
                fix: None,
            }
        }
        KeyAuth::Implausible => Finding {
            level: Level::Warn,
            label: "provider".to_owned(),
            detail: format!(
                "{} — {access} · recognized · present · implausible (not a live key){locus_note}",
                p.id
            ),
            fix: Some(format!(
                "export {}=…  # a real key, not a placeholder",
                p.fix_var
            )),
        },
        KeyAuth::Unauthorized => Finding {
            level: Level::Warn,
            label: "provider".to_owned(),
            detail: format!(
                "{} — {access} · recognized · 401 · key rejected{locus_note}",
                p.id
            ),
            fix: Some(format!("export {}=…  # a live key", p.fix_var)),
        },
        KeyAuth::PaymentRequired => Finding {
            level: Level::Warn,
            label: "provider".to_owned(),
            detail: format!(
                "{} — {access} · recognized · 402 · quota / billing{locus_note}",
                p.id
            ),
            fix: Some(format!("settle billing for {}", p.id)),
        },
        KeyAuth::Absent => Finding {
            level: Level::Warn,
            label: "provider".to_owned(),
            detail: format!(
                "{} — {access} · recognized · not configured ({} unset){locus_note}",
                p.id, p.fix_var
            ),
            fix: Some(format!("export {}=…", p.fix_var)),
        },
        other => Finding {
            level: Level::Warn,
            label: "provider".to_owned(),
            detail: format!(
                "{} — {access} · recognized · {}{locus_note}",
                p.id,
                other.as_str()
            ),
            fix: None,
        },
    }
}

/// One row per local engine whose EFFECTIVE endpoint is off-loopback
/// (P0-20): the operator override (`NIKA_<ID>_BASE_URL` · `OLLAMA_HOST`)
/// is NAMED with its locus — a LAN GPU box never launders as « local ».
/// `None` on loopback: the summary line already tells that truth.
fn local_locus_finding(p: &ProviderProbe) -> Option<Finding> {
    match p.readiness.execution_locus {
        ExecutionLocus::Lan | ExecutionLocus::Remote => Some(Finding {
            level: Level::Ok,
            label: "local".to_owned(),
            detail: format!(
                "{} — {} · configured · endpoint {} ({} — not loopback)",
                p.id,
                p.readiness.access,
                redact_userinfo(&p.endpoint),
                p.readiness.execution_locus.label()
            ),
            fix: None,
        }),
        ExecutionLocus::Loopback | ExecutionLocus::Cloud | ExecutionLocus::Unknown => None,
    }
}

/// The local-providers summary line · unpinged runs hand off to `--ping`.
fn local_finding(local_ids: &[&str], pinged: bool) -> Finding {
    Finding {
        level: Level::Ok,
        label: "local".to_owned(),
        detail: format!(
            "{} ({}) — no key · needs a running server",
            crate::text::count(local_ids.len(), "provider"),
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

/// Strip `user:pass@` from a URL for DISPLAY — a local media URL is
/// config, not a credential, but `http://user:s3cret@tts.lan` in a CI
/// log leaks the embedded secret (the rust-pro review's F5). The
/// userinfo is replaced, never echoed. `pub`: the endpoint rows
/// (catalog · welcome) redact through the SAME seam.
#[must_use]
pub fn redact_userinfo(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    match rest[..authority_end].rfind('@') {
        Some(at) => format!("{scheme}://***@{}", &rest[at + 1..]),
        None => url.to_owned(),
    }
}

/// Diagnose the environment: build the real probe (the shared
/// [`crate::probe`] engine · PRESENCE-only · offline unless `ping`), then
/// render the findings. The theme comes from the global
/// `--color`/`--hyperlink` chain (main.rs) — a piped doctor keeps its
/// exact bytes; `--json` never colours. `verbose` unfolds the healthy
/// machine's advisory notes (B-8b — the human lane defaults to calm).
#[must_use]
pub fn run(ping: bool, json: bool, verbose: bool, theme: Theme) -> VerbOutput {
    let probe = crate::probe::collect(ping);
    let findings = diagnose(&probe);
    // P3 B6 · every shipped agentic CLI runtime (always listed · label
    // `runtime`, never the MCP-wire `agent` column).
    #[cfg(feature = "access-harness")]
    let findings = {
        let mut findings = findings;
        findings.extend(harness_findings());
        findings
    };
    let code = exit_code(&findings);
    VerbOutput {
        text: if json {
            with_cascade(render_json(
                &findings,
                crate::probe::adoption_state(&probe),
                &crate::probe::capability_receipts(&probe),
                &probe.census,
            ))
        } else {
            // The same sobriety seam the concierge rides: `--plain`
            // promises ASCII glyph twins, and doctor's ✔/⚠/· column
            // shipped raw Unicode into transcripts for a train
            // (gauntlet G-B, LANG-04). JSON stays byte-exact — the
            // machine lane never needed the fold.
            crate::display::vocab::sober(theme, &render(&findings, verbose, theme))
        },
        code,
    }
}

#[cfg(test)]
mod json_tests;
#[cfg(test)]
mod pricing_tests;
#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;

/// P3 B6 · every shipped agentic CLI runtime, always listed (label
/// `runtime` — never `agent`, which is the MCP-wire column).
#[cfg(feature = "access-harness")]
pub(crate) fn harness_findings() -> Vec<Finding> {
    let rows = match nika_harness::registry() {
        Ok(rows) => rows,
        Err(e) => {
            return vec![Finding {
                level: Level::Warn,
                label: "runtime".to_owned(),
                detail: format!("the adapter registry refused to load: {e}"),
                fix: None,
            }];
        }
    };
    nika_harness::probe_adapters_sync(rows)
        .iter()
        .map(harness_finding)
        .collect()
}

#[cfg(feature = "access-harness")]
fn harness_finding(row: &nika_harness::AdapterProbeRow) -> Finding {
    harness_finding_from_parts(
        &row.id,
        row.version,
        row.authenticated,
        &row.package,
        row.product_present,
    )
}

#[cfg(feature = "access-harness")]
fn harness_finding_from_parts(
    id: &str,
    version: Option<(u32, u32)>,
    authenticated: Option<bool>,
    package: &str,
    product_present: bool,
) -> Finding {
    let display = nika_types::access::HarnessRuntime::lookup(id).map_or(id, |rt| rt.display);
    if id == "codex" && product_present && version.is_none() {
        return Finding {
            level: Level::Warn,
            label: "runtime".to_owned(),
            detail: format!(
                "{id} — {display} · infer-grade direct path detected (login judged at run) · \
                 agent ACP speaker missing · `--access {id}`"
            ),
            fix: Some(format!("install: {package} (only required for agent:)")),
        };
    }
    match (product_present, version, authenticated) {
        (_, Some((major, minor)), Some(true)) => Finding {
            level: Level::Ok,
            label: "runtime".to_owned(),
            detail: format!(
                "{id} — {display} · detected (v{major}.{minor}) · authenticated (its own login) · `--access {id}`"
            ),
            fix: None,
        },
        (_, Some((major, minor)), _) => Finding {
            level: Level::Warn,
            label: "runtime".to_owned(),
            detail: format!(
                "{id} — {display} · detected (v{major}.{minor}) · not signed in · `--access {id}`"
            ),
            fix: Some(format!(
                "sign in to {display} itself · then `--access {id}`"
            )),
        },
        (true, None, _) => Finding {
            level: Level::Warn,
            label: "runtime".to_owned(),
            detail: format!("{id} — {display} · installed, ACP speaker missing · `--access {id}`"),
            fix: Some(format!(
                "install the ACP speaker: {package} · the wrapper is never the pin — `--access {id}`"
            )),
        },
        (false, None, _) => Finding {
            level: Level::Warn,
            label: "runtime".to_owned(),
            detail: format!("{id} — {display} · not installed · `--access {id}`"),
            // The app first — teaching the ACP wrapper package here was
            // the R4 lie: the operator installed the wrapper (never the
            // app), then ate NIKA-1802 trying it as a pin.
            fix: Some(format!("install {display} itself · then `--access {id}`")),
        },
    }
}
