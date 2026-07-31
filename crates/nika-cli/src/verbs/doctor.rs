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

// The probe layer (structs + collectors) lives in `verbs::probe` — the ONE
// detection engine `doctor` and `welcome` share. Re-exported `pub(crate)` so
// this module's tests (and historical importers) keep their names.
use crate::display::theme::{Role, Theme};
pub(crate) use crate::verbs::probe::{
    AdoptionState, CapabilityLevel, ClientProbe, ImageProbe, KitProbe, ModelsProbe, PingState,
    PricingProbe, Probe, ProviderProbe, TtsProbe,
};
use crate::verbs::{VerbOutput, exit};
use nika_providers::probe::ExecutionLocus;

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
pub(crate) struct Finding {
    pub level: Level,
    pub label: String,
    pub detail: String,
    pub fix: Option<String>,
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
    out.extend(retention_findings(&probe.retention, &probe.retention_notes));
    out.push(Finding {
        level: Level::Ok,
        label: "mcp".to_owned(),
        detail: "available via `nika mcp` (9 read-only tools · nika_check through nika_tools)"
            .to_owned(),
        fix: None,
    });
    out.extend(sidecar_finding());
    out.extend(models_finding(&probe.models));

    for client in &probe.clients {
        out.push(client_finding(client, &probe.kits));
    }

    for kit in &probe.kits {
        out.push(kit_finding(kit, &probe.version));
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
            fix: Some("nika model pull Qwen/Qwen3-4B-Instruct-2507-GGUF".to_owned()),
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
    let identity = format!(
        "{} models · {} providers · snapshot {} · {} · list rates (public catalog)",
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
    retention: &crate::verbs::trace::retention::RetentionConfig,
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
/// nika-agents CI). Same train → ✔. A lagging kit names the refresh
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
            "cursor" => "re-sync the local drop: scripts/update-mirrors.sh (nika-agents checkout)",
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
        return Finding {
            level: Level::Ok,
            label: "agent".to_owned(),
            detail: format!(
                "{} wired · {} ({}) at {}",
                client.id,
                level.as_str(),
                capability_surfaces(level),
                client.path
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

/// The surfaces that earned a rung — the parenthetical that keeps the
/// level auditable at a glance (P0-9: oracle-only ≠ guarded, and the
/// line must SHOW why).
const fn capability_surfaces(level: CapabilityLevel) -> &'static str {
    match level {
        CapabilityLevel::OracleOnly => "mcp · no hooks",
        CapabilityLevel::AuthoringEnabled => "mcp + kit · no hooks",
        CapabilityLevel::Guarded => "kit + hooks",
        CapabilityLevel::FullyIntegrated => "kit + hooks + runtime",
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
/// agents/CI branch on `summary.fail` instead of parsing glyphs. P0-21:
/// the adoption rung rides alongside (additive) — ONE state token the
/// flat findings could never express.
pub(crate) fn render_json(findings: &[Finding], state: AdoptionState) -> String {
    let count = |lvl: Level| findings.iter().filter(|f| f.level == lvl).count();
    let payload = serde_json::json!({
        "summary": {
            "ok": count(Level::Ok),
            "warn": count(Level::Warn),
            "fail": count(Level::Fail),
        },
        "adoption_state": state.as_str(),
        "findings": findings,
    });
    format!("{payload:#}")
}

/// The fixed label column (nextest school: one grid, computed on RAW text).
/// Every label `diagnose` can emit fits STRICTLY inside it (pinned by
/// `every_label_fits_the_fixed_column`) so the detail column never shears.
const LABEL_COL: usize = 10;

/// Render the findings through the ONE colour seam (`Theme` · semantic
/// never decorative — the same law welcome/run obey). Doctor rows carry NO
/// durations, so the nextest discipline reduces to the status/label
/// columns — a fixed 1-cell status glyph + the fixed [`LABEL_COL`] label
/// cell, both laid out on RAW text and painted AFTER (ANSI escapes never
/// enter width arithmetic — the same law as `Theme::glyph`). The sober
/// register (colour off · links off · every pipe) is byte-identical to the
/// themeless render it replaces.
pub(crate) fn render(findings: &[Finding], theme: Theme) -> String {
    let mut s = String::new();
    let count = |level: Level| findings.iter().filter(|f| f.level == level).count();
    let (ok, warn, fail) = (count(Level::Ok), count(Level::Warn), count(Level::Fail));
    let verdict = if fail > 0 { Level::Fail } else { Level::Ok };
    let glyph = |level: Level| theme.paint(level.role(), &level.glyph().to_string());
    let _ = writeln!(s, "{} {ok} ok · {warn} warn · {fail} fail", glyph(verdict));
    for f in findings {
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
    s
}

/// OSC-8-wrap the linkable targets inside ONE printed doctor line — an
/// existing file path (`/…` or `./…` · canonicalize-gated through the
/// [`crate::verbs::linked_path`] seam: a link that cannot open stays plain)
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
                crate::verbs::linked_path(theme, token)
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
fn provider_finding(p: &ProviderProbe) -> Finding {
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
    if p.readiness.configured {
        // The exception gets the ink, not the norm: every provider is
        // schema-native except the instruction-fallback clouds (deepseek —
        // no json_schema in its API), and an operator picking a model for
        // a structured workflow wants that fact on the health surface.
        let detail = if p.structured_native {
            format!(
                "{} — recognized · configured (key present){locus_note}",
                p.id
            )
        } else {
            format!(
                "{} — recognized · configured (key present) · structured output via instruction + local validation \
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
    } else {
        Finding {
            level: Level::Warn,
            label: "provider".to_owned(),
            detail: format!(
                "{} — recognized · not configured ({} unset){locus_note}",
                p.id, p.fix_var
            ),
            fix: Some(format!("export {}=…", p.fix_var)),
        }
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
                "{} — configured · endpoint {} ({} — not loopback)",
                p.id,
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
/// userinfo is replaced, never echoed. `pub(crate)`: the endpoint rows
/// (catalog · welcome) redact through the SAME seam.
pub(crate) fn redact_userinfo(url: &str) -> String {
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
/// `verbs::probe` engine · PRESENCE-only · offline unless `ping`), then
/// render the findings. The theme comes from the global
/// `--color`/`--hyperlink` chain (main.rs) — a piped doctor keeps its
/// exact bytes; `--json` never colours.
#[must_use]
pub fn run(ping: bool, json: bool, theme: Theme) -> VerbOutput {
    let probe = crate::verbs::probe::collect(ping);
    let findings = diagnose(&probe);
    let code = exit_code(&findings);
    VerbOutput {
        text: if json {
            render_json(&findings, crate::verbs::probe::adoption_state(&probe))
        } else {
            render(&findings, theme)
        },
        code,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use nika_providers::ProviderRegistry;

    use super::*;
    use crate::verbs::probe::client_probe_any;
    use nika_providers::probe::{
        ExecutionLocus, ProviderReadiness, env_present, ping_addr, spawn_ping,
    };

    /// The sober register — the byte-frozen baseline every pipe reads.
    const PLAIN: Theme = Theme::new(false, false, false);

    fn readiness(configured: bool, locus: ExecutionLocus) -> ProviderReadiness {
        ProviderReadiness {
            recognized: true,
            configured,
            reachable: None,
            model_available: None,
            priced: false,
            execution_locus: locus,
        }
    }

    fn cloud(id: &str, var: &str, present: bool) -> ProviderProbe {
        ProviderProbe {
            id: id.to_owned(),
            requires_key: true,
            key_present: present,
            fix_var: var.to_owned(),
            structured_native: id != "deepseek",
            readiness: readiness(present, ExecutionLocus::Cloud),
            endpoint: format!("https://api.{id}.example/v1"),
        }
    }
    fn local(id: &str) -> ProviderProbe {
        ProviderProbe {
            id: id.to_owned(),
            requires_key: false,
            key_present: false,
            fix_var: String::new(),
            structured_native: true,
            readiness: readiness(true, ExecutionLocus::Loopback),
            endpoint: "http://127.0.0.1:11434".to_owned(),
        }
    }

    #[test]
    fn provider_row_separates_recognized_from_configured() {
        // P0-5: « recognized » is never « ready » — the row names the
        // two rungs separately, and reachability stays the opt-in
        // `--ping` lane's word (never implied by a present key).
        let present = provider_finding(&cloud("anthropic", "ANTHROPIC_API_KEY", true));
        assert!(
            present
                .detail
                .contains("recognized · configured (key present)"),
            "{}",
            present.detail
        );
        assert!(
            !present.detail.contains("reachable"),
            "no ping ran — the word must not appear: {}",
            present.detail
        );
        let unset = provider_finding(&cloud("anthropic", "ANTHROPIC_API_KEY", false));
        assert_eq!(unset.level, Level::Warn);
        assert!(
            unset
                .detail
                .contains("recognized · not configured (ANTHROPIC_API_KEY unset)"),
            "{}",
            unset.detail
        );
        assert_eq!(unset.fix.as_deref(), Some("export ANTHROPIC_API_KEY=…"));
    }

    #[test]
    fn a_cloud_proxy_override_is_never_laundered_as_cloud() {
        // P0-20 on the keyed lane: an operator proxy in front of mistral
        // is NAMED with its locus — never read as « the vendor default ».
        let mut p = cloud("mistral", "MISTRAL_API_KEY", true);
        p.endpoint = "https://proxy.corp.example/v1".to_owned();
        p.readiness.execution_locus = ExecutionLocus::Remote;
        let f = provider_finding(&p);
        assert!(
            f.detail.contains("https://proxy.corp.example/v1"),
            "{}",
            f.detail
        );
        assert!(
            f.detail.contains("(remote — not the vendor default)"),
            "{}",
            f.detail
        );
    }

    #[test]
    fn a_lan_override_is_named_on_the_local_lane() {
        // P0-20: ollama pointed at the GPU box must not render « local »
        // — the endpoint and its locus get their own row.
        let mut ollama = local("ollama");
        ollama.endpoint = "http://gpu.lan:11434".to_owned();
        ollama.readiness.execution_locus = ExecutionLocus::Lan;
        let probe = Probe {
            models: ModelsProbe::default(),
            version: "0.0.0".to_owned(),
            config_path: None,
            providers: vec![ollama],
            clients: Vec::new(),
            kits: Vec::new(),
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
        };
        let findings = diagnose(&probe);
        let rows: Vec<_> = findings.iter().filter(|f| f.label == "local").collect();
        assert!(
            rows.iter()
                .any(|f| f.detail.contains("http://gpu.lan:11434")
                    && f.detail.contains("(lan — not loopback)")),
            "the override is named: {rows:?}"
        );
        // A loopback ollama earns NO extra row — the exception gets the ink.
        let quiet = Probe {
            providers: vec![local("ollama")],
            ..probe
        };
        let findings = diagnose(&quiet);
        assert_eq!(
            findings.iter().filter(|f| f.label == "local").count(),
            1,
            "loopback stays the one summary line: {findings:?}"
        );
    }

    #[test]
    fn instruction_fallback_cloud_is_named_on_the_health_surface() {
        // deepseek carries a key but no native json_schema — the doctor
        // row says so; a native provider's row stays unannotated.
        let deepseek = cloud("deepseek", "DEEPSEEK_API_KEY", true);
        let f = provider_finding(&deepseek);
        assert!(
            f.detail.contains("instruction + local validation"),
            "{}",
            f.detail
        );
        let openai = cloud("openai", "OPENAI_API_KEY", true);
        let f = provider_finding(&openai);
        assert!(!f.detail.contains("instruction"), "{}", f.detail);
    }

    #[test]
    fn key_present_is_ok_and_exits_zero() {
        let probe = Probe {
            models: ModelsProbe::default(),
            version: "0.81.0".to_owned(),
            config_path: Some("~/.nika/config.toml".to_owned()),
            providers: vec![cloud("anthropic", "ANTHROPIC_API_KEY", true)],
            clients: vec![],
            kits: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
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
            models: ModelsProbe::default(),
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![
                cloud("anthropic", "ANTHROPIC_API_KEY", false),
                local("ollama"),
            ],
            clients: vec![],
            kits: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
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
            models: ModelsProbe::default(),
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![cloud("openai", "OPENAI_API_KEY", false)],
            clients: vec![],
            kits: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
        };
        let text = render(&diagnose(&probe), PLAIN);
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
            models: ModelsProbe::default(),
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![cloud("anthropic", "ANTHROPIC_API_KEY", false)],
            clients: vec![],
            kits: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
        };
        let f = diagnose(&probe);
        assert!(f.iter().any(|f| f.level == Level::Fail));
        assert_eq!(exit_code(&f), exit::ENV);
    }

    #[test]
    fn local_provider_alone_is_a_usable_path_exit_zero() {
        let probe = Probe {
            models: ModelsProbe::default(),
            version: "0.81.0".to_owned(),
            config_path: None,
            providers: vec![local("ollama"), local("vllm")],
            clients: vec![],
            kits: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
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
            serde_json::from_str(&render_json(&findings, AdoptionState::KeyPresent))
                .expect("valid JSON");
        assert_eq!(json["summary"]["ok"], 1);
        assert_eq!(json["summary"]["fail"], 1);
        assert_eq!(json["findings"][0]["level"], "ok");
        assert_eq!(json["findings"][1]["fix"], "nika wire claude");
    }

    /// P0-21 — the machine lane carries the adoption rung next to the
    /// findings: agents/CI branch on ONE state token, not on a parse of
    /// the flat finding rows.
    #[test]
    fn doctor_json_serializes_the_adoption_state() {
        let findings = vec![Finding {
            level: Level::Ok,
            label: "binary".to_owned(),
            detail: "v0".to_owned(),
            fix: None,
        }];
        for (state, token) in [
            (AdoptionState::Installed, "installed"),
            (AdoptionState::LocalDetected, "local_detected"),
            (AdoptionState::LocalReachable, "local_reachable"),
            (AdoptionState::KeyPresent, "key_present"),
            (AdoptionState::RealReady, "real_ready"),
        ] {
            let json: serde_json::Value =
                serde_json::from_str(&render_json(&findings, state)).expect("valid JSON");
            assert_eq!(json["adoption_state"], token, "{state:?}");
        }
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
        assert!(
            f.detail.contains("list rates"),
            "the public-catalog basis is named — private/proxy deals are \
             not reflected and the line must say so: {}",
            f.detail
        );
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

    /// Each severity prints a DISTINCT glyph — a `Default::default()` mutant
    /// (the null char `'\0'`) would erase the level cue the operator scans for.
    /// The sidecar row is a BUILD fact: present exactly when this binary
    /// carries the `local-infer` feature, absent otherwise — the default
    /// doctor stays byte-identical.
    #[test]
    fn sidecar_row_tracks_the_build_feature() {
        let probe = Probe {
            models: ModelsProbe::default(),
            version: "0.99.0".to_owned(),
            config_path: None,
            providers: vec![local("ollama")],
            clients: vec![],
            kits: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
        };
        let sidecar = diagnose(&probe).into_iter().find(|f| f.label == "sidecar");
        if cfg!(feature = "local-infer") {
            let row = sidecar.expect("built with local-infer — the row must appear");
            assert_eq!(row.level, Level::Ok);
            assert!(row.detail.contains("nika model serve"), "{}", row.detail);
        } else {
            assert!(sidecar.is_none(), "default build carries no sidecar row");
        }
    }

    /// The models row (issue #146 · the sidecar's dir+count half): pulled
    /// models list on ANY build; an empty store rows only on a sidecar
    /// build (teaching pull) — the default doctor stays byte-identical.
    #[test]
    fn models_row_reports_the_dir_and_count_once_pulled() {
        let with_models = ModelsProbe {
            root: Some("/home/x/.nika/models".to_owned()),
            count: 2,
            bytes: 3 * 1024 * 1024 * 1024,
        };
        let rows = models_finding(&with_models);
        assert_eq!(rows.len(), 1, "pulled models row on EVERY build");
        assert_eq!(rows[0].level, Level::Ok);
        assert_eq!(rows[0].label, "models");
        assert!(
            rows[0].detail.contains("/home/x/.nika/models"),
            "{}",
            rows[0].detail
        );
        assert!(rows[0].detail.contains("2 GGUF"), "{}", rows[0].detail);
        assert!(rows[0].detail.contains("3.0 GiB"), "{}", rows[0].detail);
        assert!(
            rows[0].detail.contains("nika model list"),
            "{}",
            rows[0].detail
        );
    }

    #[test]
    fn empty_models_store_teaches_pull_only_on_a_sidecar_build() {
        let rows = models_finding(&ModelsProbe::default());
        if cfg!(feature = "local-infer") {
            assert_eq!(rows.len(), 1, "sidecar build with nothing to serve warns");
            assert_eq!(rows[0].level, Level::Warn);
            assert!(
                rows[0]
                    .fix
                    .as_deref()
                    .is_some_and(|f| f.contains("nika model pull")),
                "{rows:?}"
            );
        } else {
            assert!(rows.is_empty(), "default build + zero pulls = no row");
        }
    }

    #[test]
    fn level_glyphs_are_distinct() {
        assert_eq!(Level::Ok.glyph(), '✔');
        assert_eq!(Level::Warn.glyph(), '⚠');
        assert_eq!(Level::Fail.glyph(), '✖');
    }

    #[test]
    fn doctor_names_each_host_capability_level() {
        // P0-9 — the flat « wired at … » line claimed a host parity
        // that does not exist: an oracle-only host and a guarded one
        // now read as the two DIFFERENT rungs they are.
        let wired = |id: &str| ClientProbe {
            id: id.to_owned(),
            path: format!("~/.{id}/cfg"),
            present: true,
            current: true,
            stale: false,
        };
        let probe = Probe {
            models: ModelsProbe::default(),
            version: "0.106.0".to_owned(),
            config_path: None,
            providers: vec![],
            clients: vec![wired("hermes"), wired("cursor")],
            kits: vec![KitProbe {
                client: "cursor".to_owned(),
                version: "0.106.0".to_owned(),
            }],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
        };
        let text = render(&diagnose(&probe), PLAIN);
        assert!(
            text.contains("hermes wired · oracle-only (mcp · no hooks)"),
            "{text}"
        );
        assert!(
            text.contains("cursor wired · guarded (kit + hooks)"),
            "{text}"
        );
    }

    #[test]
    fn client_probe_reports_stale_wiring_with_a_wire_fix() {
        let probe = Probe {
            models: ModelsProbe::default(),
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
            kits: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
        };
        let text = render(&diagnose(&probe), PLAIN);
        assert!(text.contains("stale MCP args"), "{text}");
        assert!(text.contains("fix: nika wire cursor"), "{text}");
    }

    // ── The kit↔binary handshake (the drift surface CI alone carried) ──

    fn kit(client: &str, version: &str) -> KitProbe {
        KitProbe {
            client: client.to_owned(),
            version: version.to_owned(),
        }
    }

    #[test]
    fn kit_on_the_binary_train_is_ok_and_patch_drift_is_not_a_finding() {
        // 0.106.0 kit vs 0.106.1 binary — same train, patch releases
        // ship binary-only, the row stays green with no fix.
        let f = kit_finding(&kit("codex", "0.106.0"), "0.106.1");
        assert_eq!(f.level, Level::Ok);
        assert_eq!(f.label, "kit");
        assert!(f.detail.contains("on the binary's train"), "{}", f.detail);
        assert!(f.fix.is_none());
    }

    #[test]
    fn lagging_kit_names_the_refresh_for_its_own_client() {
        // Codex climbs ONE rung; Claude Code climbs TWO — the fix is
        // per-client, never generic when the client is known.
        let f = kit_finding(&kit("codex", "0.104.0"), "0.106.1");
        assert_eq!(f.level, Level::Warn);
        assert!(
            f.detail.contains("lags the binary (0.106.1)"),
            "{}",
            f.detail
        );
        assert_eq!(
            f.fix.as_deref(),
            Some("codex plugin marketplace upgrade nika")
        );

        let f = kit_finding(&kit("claude", "0.104.0"), "0.106.1");
        assert_eq!(
            f.fix.as_deref(),
            Some("claude plugin marketplace update nika, then claude plugin update nika@nika"),
            "both rungs are named — the half-climbed ladder is the proven trap"
        );

        let f = kit_finding(&kit("cursor", "0.104.0"), "0.106.1");
        assert!(
            f.fix
                .as_deref()
                .is_some_and(|x| x.contains("update-mirrors.sh")),
            "{:?}",
            f.fix
        );

        let f = kit_finding(&kit("someclient", "0.104.0"), "0.106.1");
        assert!(
            f.fix.as_deref().is_some_and(|x| x.contains("marketplace")),
            "unknown client still gets a generic refresh: {:?}",
            f.fix
        );
    }

    #[test]
    fn kit_ahead_of_the_binary_names_the_binary_upgrade() {
        let f = kit_finding(&kit("claude", "0.108.0"), "0.106.1");
        assert_eq!(f.level, Level::Warn);
        assert!(f.detail.contains("rides ahead"), "{}", f.detail);
        assert_eq!(f.fix.as_deref(), Some("brew upgrade nika"));
    }

    #[test]
    fn unparseable_kit_version_warns_without_guessing_a_train() {
        let f = kit_finding(&kit("codex", "garbage"), "0.106.1");
        assert_eq!(f.level, Level::Warn);
        assert!(f.detail.contains("unparseable"), "{}", f.detail);
        assert!(f.fix.is_none(), "no fix can be honest without a train");
    }

    #[test]
    fn diagnose_carries_one_kit_row_per_found_surface() {
        let probe = Probe {
            models: ModelsProbe::default(),
            version: "0.106.1".to_owned(),
            config_path: None,
            providers: vec![local("ollama")],
            clients: vec![],
            kits: vec![kit("codex", "0.106.0"), kit("claude", "0.104.0")],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
        };
        let findings = diagnose(&probe);
        let kits: Vec<_> = findings.iter().filter(|f| f.label == "kit").collect();
        assert_eq!(kits.len(), 2, "one row per found kit, absence is silence");
        assert_eq!(kits[0].level, Level::Ok);
        assert_eq!(kits[1].level, Level::Warn);
    }

    #[test]
    fn major_minor_parses_trains_and_rejects_junk() {
        assert_eq!(major_minor("0.106.1"), Some((0, 106)));
        assert_eq!(
            major_minor("1.0.0-rc.2"),
            Some((1, 0)),
            "an rc rides its release's train — the tag lives in the patch slot"
        );
        assert_eq!(major_minor("garbage"), None);
        assert_eq!(major_minor(""), None);
        assert_eq!(major_minor("7"), None, "a train needs both components");
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
        let out = run(false, false, PLAIN);
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
        let text = render(&[ok.clone(), ok.clone(), warn.clone()], PLAIN);
        let first = text.lines().next().expect("verdict line");
        assert_eq!(first, "✔ 2 ok · 1 warn · 0 fail");
        assert!(text.contains("binary"), "sections unchanged: {text}");

        let fail = Finding {
            level: Level::Fail,
            label: "providers".to_owned(),
            detail: "no path".to_owned(),
            fix: None,
        };
        let red = render(&[ok, warn, fail], PLAIN);
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
            Ok(Some(ms)) => PingState::Reachable(ms),
            _ => PingState::Unreachable,
        }
    }

    #[test]
    fn userinfo_never_prints_and_never_dials() {
        // F5: an operator embedding basic-auth in a local URL must not
        // see it echoed (CI logs) nor dialed as part of the host.
        assert_eq!(
            redact_userinfo("http://user:s3cret@tts.lan:8080/v1"),
            "http://***@tts.lan:8080/v1"
        );
        assert_eq!(
            redact_userinfo("http://tts.lan:8080"),
            "http://tts.lan:8080"
        );
        assert_eq!(redact_userinfo("no-scheme"), "no-scheme");
        assert_eq!(
            ping_addr("http://user:s3cret@tts.lan:8080/v1").as_deref(),
            Some("tts.lan:8080")
        );
    }

    #[test]
    fn effective_base_url_reaches_the_ping() {
        use nika_providers::ProvidersConfig;
        // The anti-doctor fix: --ping probes the OVERRIDDEN url, not the
        // seed — the registry answers the override when one is present.
        let config = ProvidersConfig::new()
            .with_base_url("ollama", "http://10.9.9.9:7777/v1/chat/completions");
        let reg = ProviderRegistry::without_http(config);
        assert_eq!(
            reg.effective_base_url("ollama"),
            Some("http://10.9.9.9:7777/v1/chat/completions")
        );
        let reg2 = ProviderRegistry::without_http(ProvidersConfig::new());
        assert!(
            reg2.effective_base_url("ollama")
                .is_some_and(|u| u.contains("127.0.0.1:11434"))
        );
        assert_eq!(reg2.effective_base_url("nope"), None);
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
            models: ModelsProbe::default(),
            version: "0.0.0".to_owned(),
            config_path: None,
            providers: vec![local("ollama")],
            clients: Vec::new(),
            kits: Vec::new(),
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
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
            models: ModelsProbe::default(),
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

    /// Every label `diagnose` can emit fits STRICTLY inside the fixed
    /// [`LABEL_COL`] cell — the grid never shears (nextest school).
    #[test]
    fn every_label_fits_the_fixed_column() {
        for label in [
            "binary",
            "config",
            "lsp",
            "mcp",
            "sidecar",
            "traces",
            "agent",
            "kit",
            "provider",
            "providers",
            "local",
            "ping",
            "image",
            "tts",
            "pricing",
        ] {
            assert!(
                label.len() <= LABEL_COL,
                "label `{label}` ({}) exceeds LABEL_COL ({LABEL_COL})",
                label.len()
            );
        }
    }

    /// Item-3 wiring: on the linked register an https target inside a
    /// detail line rides the OSC-8 wrapper (text unchanged); the sober
    /// register — every pipe — keeps its exact bytes, zero escapes.
    #[test]
    fn linked_register_wraps_https_targets_and_sober_stays_frozen() {
        let f = vec![Finding {
            level: Level::Ok,
            label: "pricing".to_owned(),
            detail: "snapshot · see https://docs.nika.sh/errors for codes".to_owned(),
            fix: None,
        }];
        let sober = render(&f, PLAIN);
        assert!(
            !sober.contains('\x1b'),
            "sober register is escape-free: {sober:?}"
        );
        let mut linked = PLAIN;
        linked.links = true;
        let out = render(&f, linked);
        assert!(
            out.contains(
                "\x1b]8;;https://docs.nika.sh/errors\x1b\\https://docs.nika.sh/errors\x1b]8;;\x1b\\"
            ),
            "https target rides OSC-8: {out:?}"
        );
    }
}
