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
//! the fix. `--ping` opts in to local port probes and bounded model-list
//! GET requests at effective local-protocol endpoints. No inference, model
//! download, auth header, request body or cloud-profile request is sent.
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
use crate::display::theme::Theme;
use crate::output::{VerbOutput, exit};
pub(crate) use crate::probe::{
    AdoptionState, CapabilityLevel, ClientProbe, HostCapabilityReceipt, ImageProbe, KitProbe,
    ModelsProbe, PingState, PricingProbe, Probe, ProviderProbe, TtsProbe,
};
use nika_providers::probe::{ExecutionLocus, KeyAuth};

mod local_models;

#[cfg(test)]
use doctor_view::LABEL_COL;
use nika_display::front_door::doctor as doctor_view;
pub use nika_display::front_door::doctor::{Finding, Level};

/// Pure diagnosis → ordered findings (binary · config · cloud providers ·
/// local summary · the inference-readiness gate).
#[must_use]
pub fn diagnose(probe: &Probe) -> Vec<Finding> {
    // Where the run-key comes from, observed once here and decided
    // without opening the keychain (a custody question never costs a password).
    diagnose_with(probe, &nika_dap::seal::custody())
}

/// [`diagnose`] judging a given run-key custody: the doctor's tests pass a
/// fixed one, so no test reads the keys of the machine it runs on.
pub(crate) fn diagnose_with(probe: &Probe, custody: &nika_dap::seal::Custody) -> Vec<Finding> {
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
    out.push(run_key_finding(custody));
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
    out.extend(probe.providers.iter().filter_map(local_models::finding));

    for p in cloud_rows {
        cloud_keys += usize::from(p.key_present);
        out.push(provider_finding(p));
        out.extend(key_conflict_finding(p));
    }

    out.push(image_finding(&probe.image));
    out.push(tts_finding(&probe.tts));
    out.push(pricing_finding(&probe.pricing));

    // The ONE Fail that drives exit 3 · no inference path AT ALL (a broken or
    // empty catalog). A merely-unset cloud key is a ⚠ above, never fatal —
    // and a signed-in harness seat IS an inference path (R4 · the census
    // joins what the provider rows alone never saw; `seats_ready` carries
    // the configured keys too since #1581, so an empty list is zero paths).
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

/// The run-key custody in one line: where the key this process
/// would sign with comes from, its fingerprint, and why the keychain is or
/// is not part of it. When a signed release and the other builds on this
/// machine sign with different keys, or may, the line says so and names the
/// one gesture that makes them one (nika key align). A secret key file other
/// users can read fails, as ssh refuses one; nothing here ever asks for a
/// password.
fn run_key_finding(custody: &nika_dap::seal::Custody) -> Finding {
    use nika_dap::seal::{KeyFile, KeychainUse};
    if let Some(explicit) = &custody.explicit {
        return explicit_key_finding(explicit, custody.signer.as_deref());
    }
    // A loose fallback fails first: it is refused whenever custody reaches it.
    if let Some((path, KeyFile::Exposed(mode))) = &custody.fallback {
        return exposed_key_finding(path, *mode);
    }
    let keychain = keychain_words(custody.keychain);
    let Some(signer) = custody.signer.as_deref() else {
        return unsigned_key_finding(custody, keychain);
    };
    if custody.keychain_fingerprint.as_deref() == Some(signer) {
        return release_key_finding(signer, custody.file_fingerprint.as_deref(), keychain);
    }
    let place = custody.fallback.as_ref().map_or_else(
        || "~/.nika/keys (0600)".to_owned(),
        |(path, _)| format!("{} (0600)", path.display()),
    );
    match (custody.keychain_key, custody.keychain) {
        (Some(true), KeychainUse::Opened) => run_key_row(
            Level::Warn,
            format!(
                "{place} · signs with {signer} · the keychain holds a run-key entry this build could not read (refused, or not a usable pair)"
            ),
            None,
        ),
        // The keychain holds a key this build never reads: the one a signed
        // release signs with, known here only through a fresh record.
        (Some(true), _) => {
            kept_build_key_finding(&place, signer, custody.recorded.as_deref(), keychain)
        }
        _ => run_key_row(
            Level::Ok,
            format!("{place} · signs with {signer} · {keychain}"),
            None,
        ),
    }
}

/// A build kept from the keychain signs with the 0600 files while the keychain
/// holds the key a signed release signs with. It compares the two only
/// through the record a release wrote for the item as it still is; a stale
/// or absent record leaves it unknown, never one identity.
fn kept_build_key_finding(
    place: &str,
    signer: &str,
    recorded: Option<&str>,
    keychain: &str,
) -> Finding {
    match recorded {
        Some(release) if release == signer => run_key_row(
            Level::Ok,
            format!(
                "{place} · signs with {signer} · the keychain holds the same key, as a signed release recorded it: one identity · {keychain}"
            ),
            None,
        ),
        Some(release) => run_key_row(
            Level::Warn,
            format!(
                "{place} · signs with {signer} · a signed release signs with {release}, the keychain key: two identities · {keychain}"
            ),
            Some(format!(
                "nika key align from a signed release (~/.nika/keys takes {release}; {signer} is retired, the journals it sealed stay verifiable)"
            )),
        ),
        None => run_key_row(
            Level::Warn,
            format!(
                "{place} · signs with {signer} · the keychain also holds a run-key, which a signed release signs with: one key or two is unknown here · {keychain}"
            ),
            Some(
                "run nika doctor from a signed release to compare them; nika key align there makes them one"
                    .to_owned(),
            ),
        ),
    }
}

/// One `run-key` row.
fn run_key_row(level: Level, detail: String, fix: Option<String>) -> Finding {
    Finding {
        level,
        label: "run-key".to_owned(),
        detail,
        fix,
    }
}

/// A secret key file other users can read: refused, as ssh refuses one.
fn exposed_key_finding(path: &std::path::Path, mode: u32) -> Finding {
    run_key_row(
        Level::Fail,
        format!(
            "{} is readable by others ({mode:o}): refused, as ssh refuses one · runs stay unsigned",
            path.display()
        ),
        Some(format!("chmod 600 {}", path.display())),
    )
}

/// The explicit pair is the whole custody: it signs, or runs stay unsigned
/// (it never falls through to another key).
fn explicit_key_finding(
    explicit: &Result<(std::path::PathBuf, nika_dap::seal::KeyFile), String>,
    signer: Option<&str>,
) -> Finding {
    use nika_dap::seal::KeyFile;
    match (explicit, signer) {
        (Err(why), _) => run_key_row(Level::Fail, why.clone(), None),
        (Ok((path, KeyFile::Private)), Some(signer)) => run_key_row(
            Level::Ok,
            format!(
                "{} (0600) · NIKA_RUN_KEY_FILE · signs with {signer}",
                path.display()
            ),
            None,
        ),
        (Ok((path, KeyFile::Private)), None) => run_key_row(
            Level::Fail,
            format!(
                "NIKA_RUN_KEY_FILE names {}, but its pair with NIKA_RUN_PUB_FILE is not usable · runs stay unsigned",
                path.display()
            ),
            None,
        ),
        (Ok((path, KeyFile::Exposed(mode))), _) => exposed_key_finding(path, *mode),
        (Ok((path, _)), _) => run_key_row(
            Level::Fail,
            format!(
                "NIKA_RUN_KEY_FILE names {}, which cannot be read · runs stay unsigned",
                path.display()
            ),
            None,
        ),
    }
}

/// No tier this process reads holds a usable pair: runs stay unsigned.
/// Beside a keychain key, or where that is unknown, a build kept from the
/// keychain is never told `nika key init`: it would mint a second identity.
fn unsigned_key_finding(custody: &nika_dap::seal::Custody, keychain: &str) -> Finding {
    use nika_dap::seal::KeychainUse;
    match (custody.keychain, custody.keychain_key) {
        (KeychainUse::AdHoc | KeychainUse::NoTerminal, Some(true) | None) => run_key_row(
            Level::Warn,
            format!("none for this build · runs stay unsigned · {keychain}"),
            Some(
                "a signed release signs with its keychain key: nika key align from it gives this build the same key; or name a 0600 pair with NIKA_RUN_KEY_FILE and NIKA_RUN_PUB_FILE"
                    .to_owned(),
            ),
        ),
        _ => run_key_row(
            Level::Warn,
            format!("none · runs stay unsigned · {keychain}"),
            Some("nika key init".to_owned()),
        ),
    }
}

/// A signed release signs with the keychain key; dev builds and `nika serve`
/// sign with the 0600 files: one identity when they hold the same key.
fn release_key_finding(signer: &str, files: Option<&str>, keychain: &str) -> Finding {
    match files {
        Some(file) if file != signer => run_key_row(
            Level::Warn,
            format!(
                "{keychain} · signs with {signer} · dev builds and nika serve sign with {file} (~/.nika/keys): two identities"
            ),
            Some(format!(
                "nika key align (~/.nika/keys takes {signer}; {file} is retired, the journals it sealed stay verifiable)"
            )),
        ),
        Some(_) => run_key_row(
            Level::Ok,
            format!(
                "{keychain} · signs with {signer} · ~/.nika/keys holds the same key: one identity for releases, dev builds and nika serve"
            ),
            None,
        ),
        None => run_key_row(
            Level::Ok,
            format!(
                "{keychain} · signs with {signer} · dev builds and nika serve hold no key (nika key align gives them this one)"
            ),
            None,
        ),
    }
}

/// Why this process does or does not open the keychain, in words.
fn keychain_words(keychain: nika_dap::seal::KeychainUse) -> &'static str {
    use nika_dap::seal::KeychainUse;
    match keychain {
        KeychainUse::Opened => {
            "the OS keychain first (this build's signature keeps its grant: no password on later runs)"
        }
        KeychainUse::Off => "the keychain is off (NIKA_KEYCHAIN)",
        KeychainUse::TestBinary => "the keychain is never opened by a test binary",
        KeychainUse::Resident => "the keychain is never opened by a resident (nika serve)",
        KeychainUse::NoTerminal => "the keychain is not opened without a terminal",
        KeychainUse::AdHoc => {
            "the keychain is not opened by this ad-hoc build (each build is a new identity to it, so it would ask for the password on every run)"
        }
        _ => "the keychain is not opened",
    }
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

/// The local-backend clause the image and tts rows share (#1581):
/// doctor never probes the media planes, so an unset URL is named as
/// UNSET — the engine's silent fallback disclosed, never printed as a
/// listener — and a set one is « configured », never « reachable ».
fn media_local_part(url: Option<&str>, var: &str) -> String {
    url.map_or_else(
        || {
            format!(
                "local backend unset · {var} (engine fallback http://localhost:8080 · unprobed)"
            )
        },
        |u| format!("local → {} (configured · unprobed)", redact_userinfo(u)),
    )
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
    let local_part = media_local_part(img.local_url.as_deref(), "NIKA_IMAGE_LOCAL_URL");
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
/// this binary's cost reports, and HOW OLD it is · #1581: a 47-day
/// snapshot read as current when only the stale branch spoke its
/// age), a staleness ⚠ past the threshold. The age warning is the gap
/// no surveyed tool closes (2026-07): a stale vendored price table
/// silently mis-prices every report.
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
    let age = p.age_days.map_or_else(
        || "age unknown".to_owned(),
        |d| {
            format!(
                "{} old",
                crate::text::count(usize::try_from(d).unwrap_or(usize::MAX), "day")
            )
        },
    );
    let identity = format!(
        "{} price rules · {} providers priced · snapshot {} · {age} · {} · list rates (public catalog)",
        p.rules, p.providers, p.as_of, p.sha
    );
    match p.age_days {
        Some(age) if age > PRICING_STALE_DAYS => Finding {
            level: Level::Warn,
            label: "pricing".to_owned(),
            detail: format!("{identity} — past {PRICING_STALE_DAYS} days · cost reports may drift"),
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
    let local_part = media_local_part(tts.local_url.as_deref(), "NIKA_TTS_LOCAL_URL");
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

/// Project the host-owned adoption, receipt and access values for machine rendering.
/// The passive renderer neither recollects facts nor grants authority from them.
#[must_use]
pub fn render_json(
    findings: &[Finding],
    state: AdoptionState,
    receipts: &[HostCapabilityReceipt],
    census: &nika_providers::census::AccessCensus,
) -> String {
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
    doctor_view::render_json(
        findings,
        state.as_str(),
        serde_json::json!(receipts),
        serde_json::json!({
            "paths": paths,
            "seats_ready": census.seats_ready,
            "best": census.best.as_ref().map(|p| p.id.clone()),
        }),
    )
}

fn with_cascade(raw: String) -> String {
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return raw;
    };
    v["cascade"] = crate::choice::doctor_cascade_json(&crate::choice::collect());
    format!("{v:#}")
}

/// Render the human report after the host's existing visibility and link policy.
/// Link only the rows the old renderer would show, in the same detail/fix order.
#[must_use]
pub fn render(findings: &[Finding], verbose: bool, theme: Theme) -> String {
    let rows: Vec<doctor_view::PreparedRow<'_>> = findings
        .iter()
        .filter(|finding| verbose || !calm_foldable(finding))
        .map(|finding| doctor_view::PreparedRow {
            finding,
            detail: link_targets(theme, &finding.detail),
            fix: finding.fix.as_deref().map(|fix| link_targets(theme, fix)),
        })
        .collect();
    let folded = |label: &str| {
        if verbose {
            0
        } else {
            findings
                .iter()
                .filter(|f| calm_foldable(f) && f.label == label)
                .count()
        }
    };
    doctor_view::render(
        findings,
        &rows,
        (folded("agent"), folded("provider"), folded("config") > 0),
        theme,
    )
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

/// Two set key variables of one provider hold different values (NIK-153): named, never their
/// values, with the one the run uses. The composition root compared them in memory.
fn key_conflict_finding(p: &ProviderProbe) -> Option<Finding> {
    if !p.key_conflict {
        return None;
    }
    let used = p.key_vars.first()?;
    let others = p.key_vars.get(1..).unwrap_or_default().join(" and ");
    Some(Finding {
        level: Level::Warn,
        label: "provider".to_owned(),
        detail: format!(
            "{} — {used} and {others} are both set with different values · Nika uses {used}",
            p.id
        ),
        fix: Some(format!(
            "unset {others}, or give it the same key (another tool may read {others})"
        )),
    })
}

/// The variable that holds the key a run sends (the ladder's first set one), else the
/// conventional one the row names: a bad key is replaced where it is read (NIK-153).
fn key_holder(p: &ProviderProbe) -> &str {
    p.key_vars.first().unwrap_or(&p.fix_var)
}

/// A 403: the provider refused this key for the route (never ready), with its fix.
fn key_refused(
    p: &ProviderProbe,
    access: nika_types::access::AccessClass,
    locus_note: &str,
) -> Finding {
    Finding {
        level: Level::Warn,
        label: "provider".to_owned(),
        detail: format!(
            "{} — {access} · recognized · 403 · key refused for this route{locus_note}",
            p.id
        ),
        fix: Some(format!(
            "export {}=…  # a key this route accepts",
            key_holder(p)
        )),
    }
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
            // The variable that supplies the key, by name (NIK-153): the ladder's first set one.
            let source = p
                .key_vars
                .first()
                .map_or_else(String::new, |var| format!(" · key from {var}"));
            let detail = if p.structured_native {
                format!(
                    "{} — {access} · recognized · configured (key present · not validated){source}{locus_note}",
                    p.id
                )
            } else {
                format!(
                    "{} — {access} · recognized · configured (key present · not validated){source} · structured output via instruction + local validation \
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
                key_holder(p)
            )),
        },
        KeyAuth::Unauthorized => Finding {
            level: Level::Warn,
            label: "provider".to_owned(),
            detail: format!(
                "{} — {access} · recognized · 401 · key rejected{locus_note}",
                p.id
            ),
            fix: Some(format!("export {}=…  # a live key", key_holder(p))),
        },
        KeyAuth::Forbidden => key_refused(p, access, &locus_note),
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
        fix: (!pinged).then(|| {
            "nika doctor --ping   # probe local ports and model lists (offline otherwise)"
                .to_owned()
        }),
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
    run_with(ping, json, verbose, theme, Vec::new())
}

/// [`run`] with findings the CALLER observed — the resident's line rides
/// here from `nika-cli` (which holds the serve crate this host does not
/// depend on · ADR-132): the host renders, the binary observes.
#[must_use]
pub fn run_with(
    ping: bool,
    json: bool,
    verbose: bool,
    theme: Theme,
    extra: Vec<Finding>,
) -> VerbOutput {
    run_with_custody(
        ping,
        json,
        verbose,
        theme,
        extra,
        &nika_dap::seal::custody(),
    )
}

/// [`run_with`] judging a given run-key custody, as [`diagnose_with`] does.
#[must_use]
pub(crate) fn run_with_custody(
    ping: bool,
    json: bool,
    verbose: bool,
    theme: Theme,
    extra: Vec<Finding>,
    custody: &nika_dap::seal::Custody,
) -> VerbOutput {
    let probe = crate::probe::collect(ping);
    let mut findings = diagnose_with(&probe, custody);
    findings.extend(extra);
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
            local_models::json(
                with_cascade(render_json(
                    &findings,
                    crate::probe::adoption_state(&probe),
                    &crate::probe::capability_receipts(&probe),
                    &probe.census,
                )),
                &probe.providers,
            )
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
mod custody_tests;
#[cfg(test)]
mod json_tests;
#[cfg(test)]
mod media_tests;
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
    let unknown = row.login.and_then(nika_harness::LoginCheck::unknown_words);
    harness_finding_from_parts(
        &row.id,
        row.version,
        row.authenticated,
        &row.package,
        row.product_present,
        unknown.as_deref(),
    )
}

#[cfg(feature = "access-harness")]
fn harness_finding_from_parts(
    id: &str,
    version: Option<(u32, u32)>,
    authenticated: Option<bool>,
    package: &str,
    product_present: bool,
    unknown: Option<&str>,
) -> Finding {
    let display = nika_types::access::HarnessRuntime::lookup(id).map_or(id, |rt| rt.display);
    if id == "codex" && product_present && version.is_none() {
        return Finding {
            level: Level::Warn,
            label: "runtime".to_owned(),
            detail: format!(
                "{id} — {display} · usable for `infer:` (its login command is judged at \
                 admission · a model the seat does not carry is the run's refusal, and a ready \
                 key outranks it unpinned) · `agent:` needs the ACP speaker · `--access {id}`"
            ),
            fix: Some(format!("install: {package} (only required for agent:)")),
        };
    }
    match (product_present, version, authenticated) {
        (_, Some((major, minor)), Some(true)) => Finding {
            level: Level::Ok,
            label: "runtime".to_owned(),
            detail: format!(
                "{id} — {display} · detected (v{major}.{minor}) · its login command reports signed in · `--access {id}`"
            ),
            fix: None,
        },
        (_, Some((major, minor)), _) => Finding {
            // A login check that did not answer observed nothing: unknown,
            // never a warning about a sign-in it could not see.
            level: if unknown.is_some() {
                Level::Unknown
            } else {
                Level::Warn
            },
            label: "runtime".to_owned(),
            detail: format!(
                "{id} — {display} · detected (v{major}.{minor}) · {} · `--access {id}`",
                unknown.map_or_else(
                    || "not signed in".to_owned(),
                    |why| format!("sign-in unknown: {why}")
                )
            ),
            fix: unknown
                .is_none()
                .then(|| format!("sign in to {display} itself · then `--access {id}`")),
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
