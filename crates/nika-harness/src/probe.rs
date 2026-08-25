// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The adapter IDENTITY probe (spec §4 · the promise B3.2 owed) — the
//! binary's version and any registry-declared command shape are judged
//! BEFORE a session, never discovered mid-run.
//!
//! Why before: an adapter outside its pin range speaks a dialect this
//! client did not read. Learning that at `initialize` wastes a spawn
//! and reports a protocol confusion where the truth is a VERSION
//! mismatch. The probe runs `<command> --version`, parses the first
//! semver-ish token out of whatever the CLI prints, and judges it
//! against the adapter's declared range. A row whose binary name can
//! collide then proves its exact subcommand through a bounded public
//! help surface; version equality alone cannot admit it.
//!
//! What the probe is NOT: a credential read, a network call, or a
//! session. It spawns the same confined child shape the session does
//! (composed env · no shell · bounded output). Auth-store probes read
//! path metadata plus a zero-read readability handle, never credential contents.

use nika_kernel::ai::harness::HarnessError;

use crate::registry::{AdapterRow, AuthProbe, DirectoryAuthProbe};
use crate::spawn::{SpawnedHarness, compose_env};

/// One adapter's probe row (P3 B6 · the doctor surface's facts) —
/// presence and exit codes only, never a credential read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AdapterProbeRow {
    /// The adapter id (registry row).
    pub id: String,
    /// The version the probe read, inside the pin (None = binary
    /// absent or outside the pin — `note` names which).
    pub version: Option<(u32, u32)>,
    /// The auth surface's verdict: `Some(true)` a configured auth witness
    /// exists, `Some(false)` the surface says none, `None` the surface itself
    /// is unreadable here. Token validity is judged by the harness session.
    pub authenticated: Option<bool>,
    /// The install pointer (the registry row's package line).
    pub package: String,
    /// What the probe saw when detection failed (the adapter's own
    /// words) — empty on a clean detection.
    pub note: String,
    /// Whether the product CLI (`claude` · `codex` · `gemini`) is on PATH.
    /// Distinct from [`Self::version`]: Claude Code can be installed
    /// while the ACP speaker `claude-agent-acp` is not.
    pub product_present: bool,
}

impl AdapterProbeRow {
    /// Whether the adapter can serve a session NOW (detected AND inside
    /// the pin, with its command-shape proof satisfied when declared).
    /// Auth stays a separate column — a harness without auth is
    /// detected-but-refusing, a truth the row keeps distinct.
    #[must_use]
    pub fn usable(&self) -> bool {
        self.version.is_some()
    }
}

/// Probe every registry row (B6) — version BEFORE dialect (spec §4),
/// then the auth surface. Rows the kill-switch removed never probe.
/// Each probe is deadlined (a hung wrapper is a `None`, never a hang);
/// the rows probe CONCURRENTLY (cold npx resolves must not add).
pub async fn probe_adapters(rows: Vec<AdapterRow>) -> Vec<AdapterProbeRow> {
    let mut set = tokio::task::JoinSet::new();
    for (idx, row) in rows.into_iter().enumerate() {
        set.spawn(async move { (idx, probe_one(row).await) });
    }
    let mut out = Vec::new();
    while let Some(done) = set.join_next().await {
        if let Ok(done) = done {
            out.push(done);
        }
    }
    // The join order is scheduling — the report's order is the
    // REGISTRY's (G-3), so two runs never disagree.
    out.sort_by_key(|(idx, _)| *idx);
    out.into_iter().map(|(_, row)| row).collect()
}

/// Cheap admission facts for `--access` (PATH + [`AuthProbe::HomeFile`]).
/// Never handshake-spawns — compose and `nika check` must not start
/// five ACP speakers. Doctor still uses [`probe_adapters_sync`].
/// Command-auth rows treat ACP-on-PATH as configured; the session
/// is the sign-in witness (NIKA-1805 if the harness refuses).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PresenceFact {
    /// Product token (`claude-code`).
    pub id: String,
    /// Provider ids this row serves.
    pub serves: Vec<String>,
    /// Product CLI on PATH (`claude`).
    pub product_present: bool,
    /// ACP speaker on PATH (`claude-agent-acp`).
    pub acp_present: bool,
    /// [`AuthProbe::HomeFile`] witness, or ACP-on-PATH for Command auth.
    pub configured: bool,
}

/// Presence-only probe for every registry row — no tokio, no spawn.
#[must_use]
pub fn presence_facts(rows: Vec<AdapterRow>) -> Vec<PresenceFact> {
    rows.into_iter()
        .map(|row| {
            let rt = nika_types::access::HarnessRuntime::lookup(&row.adapter.id);
            let detect = rt.map_or(row.adapter.command.as_str(), |r| r.detect_bin);
            let acp = rt.map_or(row.adapter.command.as_str(), |r| r.acp_bin);
            let product_present = binary_on_path(detect);
            let acp_present = binary_on_path(acp);
            let configured = match row.auth {
                AuthProbe::HomeFile(_) => {
                    probe_auth_home_sync(&row.auth, row.directory_auth).unwrap_or(false)
                }
                AuthProbe::Command { .. } => {
                    command_access_present(&row.adapter.id, product_present, acp_present)
                }
            };
            PresenceFact {
                id: row.adapter.id.clone(),
                serves: row.serves.iter().map(|s| (*s).to_owned()).collect(),
                product_present,
                acp_present,
                configured,
            }
        })
        .collect()
}

fn command_access_present(id: &str, product_present: bool, acp_present: bool) -> bool {
    acp_present || (id == "codex" && product_present)
}

fn probe_auth_home_sync(
    surface: &AuthProbe,
    directory_auth: Option<DirectoryAuthProbe>,
) -> Option<bool> {
    #[allow(clippy::disallowed_methods)] // sanctioned env boundary ($HOME presence)
    let home = std::env::var_os("HOME");
    #[allow(clippy::disallowed_methods)] // sanctioned adapter-home boundary
    let override_home = directory_auth.and_then(|probe| std::env::var_os(probe.override_env));
    match surface {
        AuthProbe::Command { .. } => None,
        AuthProbe::HomeFile(rel) => {
            if let Some(probe) = directory_auth {
                let path = match override_home.as_deref() {
                    Some(root) => std::path::Path::new(root).join(probe.override_relative),
                    None => match home.as_deref() {
                        Some(root) => std::path::Path::new(root).join(rel),
                        None => return Some(false),
                    },
                };
                return Some(provider_credential_directory_ready(
                    &path,
                    probe.credential_files,
                ));
            }
            Some(std::path::Path::new(home.as_deref()?).join(rel).exists())
        }
    }
}

/// The sync façade (the doctor surface is sync by design) — the
/// probes' async lives inside nika-harness, behind a one-shot runtime.
#[must_use]
pub fn probe_adapters_sync(rows: Vec<AdapterRow>) -> Vec<AdapterProbeRow> {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return rows
            .into_iter()
            .map(|row| AdapterProbeRow {
                id: row.adapter.id.clone(),
                version: None,
                authenticated: None,
                package: row.package.to_owned(),
                note: "could not start the probe runtime".to_owned(),
                product_present: false,
            })
            .collect();
    };
    rt.block_on(probe_adapters(rows))
}

async fn probe_one(row: AdapterRow) -> AdapterProbeRow {
    let harness = SpawnedHarness::new(row.adapter.clone());
    let (version, note) = match harness.probe_version().await {
        Ok(seen) => (seen, String::new()),
        Err(e) => (None, e.to_string()),
    };
    let authenticated = probe_auth(&row.auth, row.directory_auth).await;
    let detect = nika_types::access::HarnessRuntime::lookup(&row.adapter.id)
        .map_or(row.adapter.command.as_str(), |rt| rt.detect_bin);
    AdapterProbeRow {
        id: row.adapter.id.clone(),
        version,
        authenticated,
        package: row.package.to_owned(),
        note,
        product_present: binary_on_path(detect),
    }
}

fn binary_on_path(name: &str) -> bool {
    #[allow(clippy::disallowed_methods)] // PATH walk · presence only
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

/// The auth surface probe — an exit code or a presence bit, bounded
/// like every spawn (a hung status command is `None`, not a hang).
async fn probe_auth(
    surface: &AuthProbe,
    directory_auth: Option<DirectoryAuthProbe>,
) -> Option<bool> {
    #[allow(clippy::disallowed_methods)] // the sanctioned env boundary ($HOME presence)
    let home = std::env::var_os("HOME");
    #[allow(clippy::disallowed_methods)] // sanctioned adapter-home boundary
    let override_home = directory_auth.and_then(|probe| std::env::var_os(probe.override_env));
    probe_auth_with(
        home.as_deref(),
        override_home.as_deref(),
        surface,
        directory_auth,
    )
    .await
}

/// [`probe_auth`] with the home dir injected — the pure half tests
/// drive (writing `$HOME` would need `unsafe` under Rust 2024).
async fn probe_auth_with(
    home: Option<&std::ffi::OsStr>,
    override_home: Option<&std::ffi::OsStr>,
    surface: &AuthProbe,
    directory_auth: Option<DirectoryAuthProbe>,
) -> Option<bool> {
    match surface {
        AuthProbe::HomeFile(rel) => {
            if let Some(probe) = directory_auth {
                let path = match override_home {
                    Some(root) => std::path::Path::new(root).join(probe.override_relative),
                    None => match home {
                        Some(root) => std::path::Path::new(root).join(rel),
                        None => return Some(false),
                    },
                };
                return Some(provider_credential_directory_ready(
                    &path,
                    probe.credential_files,
                ));
            }
            Some(std::path::Path::new(home?).join(rel).exists())
        }
        AuthProbe::Command { command, args } => {
            let parent: std::collections::BTreeMap<String, String> = std::env::vars().collect();
            let env = compose_env(&parent, &[]);
            let child = tokio::process::Command::new(command)
                .args(*args)
                .env_clear()
                .envs(&env)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .status();
            let status = tokio::time::timeout(std::time::Duration::from_secs(10), child)
                .await
                .ok()?
                .ok()?;
            Some(status.success())
        }
    }
}

/// Metadata-only proof that a provider credential is present. Kimi Code stores
/// provider seats as top-level `credentials/<name>.json`; the `mcp/` subtree is
/// a distinct authority and must not make the model seat look authenticated.
/// Every ambiguity fails closed without reading a credential byte.
fn provider_credential_directory_ready(path: &std::path::Path, credential_files: &[&str]) -> bool {
    provider_credential_directory_ready_with(path, credential_files, |_| {})
}

fn provider_credential_directory_ready_with(
    path: &std::path::Path,
    credential_files: &[&str],
    mut after_open: impl FnMut(&std::path::Path),
) -> bool {
    let Some(path_witnesses) = path_component_witnesses(path) else {
        return false;
    };
    for file_name in credential_files {
        let entry_path = path.join(file_name);
        let metadata = match std::fs::symlink_metadata(&entry_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return false,
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
            continue;
        }
        let Some(opened) = open_witnessed_file(&entry_path, &metadata) else {
            continue;
        };
        if !pathname_names_open_file(&entry_path, &opened) {
            continue;
        }
        after_open(&entry_path);
        if !path_witnesses_unchanged(&path_witnesses)
            || !pathname_names_open_file(&entry_path, &opened)
        {
            continue;
        }
        return true;
    }
    false
}

fn path_component_witnesses(
    path: &std::path::Path,
) -> Option<Vec<(std::path::PathBuf, std::fs::Metadata)>> {
    if !path.is_absolute() {
        return None;
    }
    let mut cursor = std::path::PathBuf::new();
    let mut witnesses = Vec::new();
    for component in path.components() {
        if matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        ) {
            return None;
        }
        cursor.push(component.as_os_str());
        let Ok(metadata) = std::fs::symlink_metadata(&cursor) else {
            return None;
        };
        if metadata.file_type().is_symlink() {
            return None;
        }
        witnesses.push((cursor.clone(), metadata));
    }
    Some(witnesses)
}

fn path_witnesses_unchanged(witnesses: &[(std::path::PathBuf, std::fs::Metadata)]) -> bool {
    witnesses.iter().all(|(path, witnessed)| {
        let Ok(current) = std::fs::symlink_metadata(path) else {
            return false;
        };
        !current.file_type().is_symlink() && same_file_identity(witnessed, &current)
    })
}

/// Open only to let the OS apply UID/ACL readability, then retain the handle
/// through every pathname witness. No credential byte is ever read.
fn open_witnessed_file(
    path: &std::path::Path,
    witnessed: &std::fs::Metadata,
) -> Option<std::fs::File> {
    let Ok(file) = std::fs::File::open(path) else {
        return None;
    };
    let Ok(opened) = file.metadata() else {
        return None;
    };
    (opened.is_file() && opened.len() > 0 && same_file_identity(witnessed, &opened)).then_some(file)
}

/// Re-lstat the live pathname and prove that it still names the held handle.
/// This runs once immediately after open and again after directory witnesses,
/// closing the entry-swap seam without reading credential contents.
fn pathname_names_open_file(path: &std::path::Path, opened: &std::fs::File) -> bool {
    let Ok(handle_metadata) = opened.metadata() else {
        return false;
    };
    let Ok(current) = std::fs::symlink_metadata(path) else {
        return false;
    };
    !current.file_type().is_symlink()
        && current.is_file()
        && current.len() > 0
        && same_file_identity(&handle_metadata, &current)
}

#[cfg(unix)]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(windows)]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    match (
        left.volume_serial_number(),
        left.file_index(),
        right.volume_serial_number(),
        right.file_index(),
    ) {
        (Some(left_volume), Some(left_index), Some(right_volume), Some(right_index)) => {
            left_volume == right_volume && left_index == right_index
        }
        _ => false,
    }
}

#[cfg(not(any(unix, windows)))]
fn same_file_identity(_left: &std::fs::Metadata, _right: &std::fs::Metadata) -> bool {
    false
}

/// The inclusive version range an adapter is pinned to — the
/// schema-diff gate's binary-side twin (spec §2).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct VersionPin {
    /// Lowest accepted `(major, minor)`.
    pub min: (u32, u32),
    /// Highest accepted MAJOR — a new major means a new dialect until
    /// this client is taught otherwise.
    pub max_major: u32,
}

impl VersionPin {
    /// Construct (INV-019).
    #[must_use]
    pub const fn new(min: (u32, u32), max_major: u32) -> Self {
        Self { min, max_major }
    }

    /// Whether `(major, minor)` sits inside the pin.
    #[must_use]
    pub const fn accepts(&self, major: u32, minor: u32) -> bool {
        if major > self.max_major {
            return false;
        }
        if major < self.min.0 {
            return false;
        }
        !(major == self.min.0 && minor < self.min.1)
    }
}

/// Parse the first `MAJOR.MINOR[.PATCH]` token out of a `--version`
/// line. Harness CLIs print prose around it (`codex-acp 1.4.0
/// (build …)` · `gemini-cli version 0.9.1`), so the parse is
/// scan-for-the-first-triple, never a whole-line format assumption.
#[must_use]
pub fn parse_version(line: &str) -> Option<(u32, u32)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        // A digit run must not begin mid-token (`v1.2` is fine, `x1.2`
        // is a name, `1.2` after a dot is a continuation).
        if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'v') {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let major: u32 = line.get(start..i)?.parse().ok()?;
        if bytes.get(i) != Some(&b'.') {
            continue;
        }
        i += 1;
        let mstart = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == mstart {
            continue;
        }
        let minor: u32 = line.get(mstart..i)?.parse().ok()?;
        return Some((major, minor));
    }
    None
}

/// Judge a probe's raw output against a pin — the pure half (the
/// spawn half lives in [`crate::spawn`], so THIS is what tests drive).
///
/// # Errors
///
/// [`HarnessError::Unavailable`] when nothing parses (the binary is
/// not the adapter we think) or when the version sits outside the pin.
pub fn judge_version(
    adapter_id: &str,
    output: &str,
    pin: &VersionPin,
) -> Result<(u32, u32), HarnessError> {
    let Some((major, minor)) = output.lines().find_map(parse_version) else {
        return Err(HarnessError::Unavailable {
            reason: format!(
                "adapter `{adapter_id}`: `--version` printed no version \
                 ({:?}) — this binary is not the adapter it claims to be",
                output.lines().next().unwrap_or("").trim()
            ),
        });
    };
    if pin.accepts(major, minor) {
        return Ok((major, minor));
    }
    Err(HarnessError::Unavailable {
        reason: format!(
            "adapter `{adapter_id}`: version {major}.{minor} is outside the \
             pin (>= {}.{} · major <= {}) — a version this client has not \
             read speaks a dialect it cannot judge",
            pin.min.0, pin.min.1, pin.max_major
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_codex_counts_for_infer_without_claiming_other_acp_seats() {
        assert!(command_access_present("codex", true, false));
        assert!(!command_access_present("codex", false, false));
        assert!(!command_access_present("claude-code", true, false));
        assert!(command_access_present("claude-code", true, true));
    }

    #[test]
    fn the_parse_finds_the_triple_inside_real_cli_prose() {
        assert_eq!(
            parse_version("codex-acp 1.4.0 (build 2026-07)"),
            Some((1, 4))
        );
        assert_eq!(parse_version("gemini-cli version 0.9.1"), Some((0, 9)));
        assert_eq!(parse_version("v2.11.3"), Some((2, 11)));
        assert_eq!(parse_version("qwen-code 12.0"), Some((12, 0)));
        // Measured 2026-08-22: `kimi --version` / `kimi -V` print a
        // bare triple, no prose.
        assert_eq!(parse_version("0.37.2"), Some((0, 37)));
        // Prose without a version parses to nothing — never a guess.
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version(""), None);
        // A bare integer is not a version (no minor).
        assert_eq!(parse_version("build 12345"), None);
    }

    #[test]
    fn the_parse_never_starts_a_run_mid_token() {
        // The scan's boundary law (mutation-killers for the guard +
        // increments): a digit run glued to a LETTER is a name and is
        // skipped — but a DOT is a clean separator, so the run right
        // after it is a fair candidate.
        assert_eq!(parse_version("x1.2"), None, "mid-token, no continuation");
        assert_eq!(
            parse_version("abc9.9.9"),
            Some((9, 9)),
            "the first run is glued (skipped) · the tail after the dot parses"
        );
        assert_eq!(
            parse_version("v1.2"),
            Some((1, 2)),
            "the v prefix is conventional"
        );
        // The FIRST parseable triple wins, later ones never consulted.
        assert_eq!(parse_version("junk 1.2 then 9.9"), Some((1, 2)));
        // A dot with no minor digits is not a version — scan continues.
        assert_eq!(parse_version("v1. then 2.3"), Some((2, 3)));
        // A triple glued to prose after the minor still parses at its start.
        assert_eq!(parse_version("tool 3.7-beta"), Some((3, 7)));
    }

    #[test]
    fn the_probe_row_usable_flag_reads_the_version() {
        let row = |version: Option<(u32, u32)>| AdapterProbeRow {
            id: "x".to_owned(),
            version,
            authenticated: None,
            package: "p".to_owned(),
            note: String::new(),
            product_present: false,
        };
        assert!(row(Some((1, 0))).usable());
        assert!(
            !row(None).usable(),
            "no version = not usable (never an assumption)"
        );
    }

    #[test]
    fn the_env_registry_wrapper_delegates_and_reads_the_switch() {
        // NIKA_HARNESS_DISABLE is unset in this process → the full table.
        let rows = crate::registry().expect("the live env loads");
        assert_eq!(rows.len(), 5, "registry() reads the real env boundary");
    }

    /// The sync façade is FOR sync callers (doctor) — so the test is
    /// sync too (a `block_on` inside a tokio test's runtime is the
    /// "runtime within a runtime" panic by design).
    #[test]
    fn the_sync_facade_probes_for_real() {
        let mk = |id: &str| crate::registry::AdapterRow {
            adapter: crate::HarnessAdapter::new(id, "false")
                .expect("fine")
                .with_version_pin(VersionPin::new((1, 0), 1)),
            serves: &["mock"],
            auth: crate::registry::AuthProbe::HomeFile(".definitely-absent-nika-test"),
            directory_auth: None,
            package: "test-only",
        };
        let out = probe_adapters_sync(vec![mk("sync-a"), mk("sync-b")]);
        assert_eq!(out.len(), 2, "the façade probes, never an empty vec");
        assert!(out.iter().all(|r| r.version.is_none()));
    }

    #[test]
    fn presence_facts_emit_every_shipped_row_without_a_runtime() {
        let rows = crate::registry_with(&|_| None).expect("static table");
        let facts = presence_facts(rows);
        let ids: Vec<&str> = facts.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "gemini-cli",
                "qwen-code",
                "kimi-code",
                "codex",
                "claude-code"
            ]
        );
        for fact in &facts {
            assert!(
                nika_types::access::HarnessRuntime::lookup(&fact.id).is_some(),
                "{}",
                fact.id
            );
        }
    }

    #[test]
    fn the_pin_is_inclusive_at_the_floor_and_caps_the_major() {
        let pin = VersionPin::new((1, 2), 2);
        assert!(pin.accepts(1, 2), "the floor itself is accepted");
        assert!(pin.accepts(1, 9));
        assert!(pin.accepts(2, 0), "a same-major-cap version rides");
        assert!(!pin.accepts(1, 1), "below the floor minor");
        assert!(!pin.accepts(0, 9), "below the floor major");
        assert!(!pin.accepts(3, 0), "past the major cap — a new dialect");
    }

    #[test]
    fn an_unparseable_probe_names_the_adapter_and_what_it_saw() {
        let err = judge_version(
            "codex-acp",
            "bash: codex-acp: not found\n",
            &VersionPin::new((1, 0), 1),
        )
        .expect_err("no version parses");
        let msg = err.to_string();
        assert!(msg.contains("codex-acp"), "{msg}");
        assert!(msg.contains("not the adapter it claims"), "{msg}");
    }

    #[test]
    fn an_out_of_pin_version_refuses_with_both_sides_named() {
        let err = judge_version("codex-acp", "codex-acp 3.0.0", &VersionPin::new((1, 0), 2))
            .expect_err("major past the cap");
        let msg = err.to_string();
        assert!(msg.contains("3.0"), "{msg}");
        assert!(msg.contains("major <= 2"), "{msg}");
    }

    #[test]
    fn a_probe_inside_the_pin_returns_what_it_read() {
        let seen = judge_version(
            "gemini-cli",
            "gemini-cli version 0.9.1\nextra prose\n",
            &VersionPin::new((0, 8), 0),
        )
        .expect("inside the pin");
        assert_eq!(seen, (0, 9));
    }

    #[test]
    fn kimi_code_pin_accepts_the_measured_line_and_refuses_below_the_floor() {
        let pin = VersionPin::new((0, 37), 0);
        let seen = judge_version("kimi-code", "0.37.2\n", &pin).expect("measured line");
        assert_eq!(seen, (0, 37));
        let err = judge_version("kimi-code", "0.36.9\n", &pin).expect_err("below the floor");
        let msg = err.to_string();
        assert!(msg.contains("kimi-code"), "{msg}");
        assert!(msg.contains("0.36"), "{msg}");
        let major = judge_version("kimi-code", "1.0.0\n", &pin).expect_err("new major");
        assert!(major.to_string().contains("major <= 0"), "{major}");
    }

    #[test]
    fn the_version_rides_the_first_line_that_carries_one() {
        // A CLI that greets before versioning: the probe scans lines,
        // it does not assume line 1 (a real-world shape).
        let seen = judge_version(
            "noisy",
            "warning: config file missing\nnoisy 4.2.0\n",
            &VersionPin::new((4, 0), 4),
        )
        .expect("second line carries it");
        assert_eq!(seen, (4, 2));
    }

    #[tokio::test]
    async fn the_auth_probe_reads_exit_codes_and_presence_bits() {
        // Command: the exit code IS the verdict.
        let yes = AuthProbe::Command {
            command: "true",
            args: &[],
        };
        let no = AuthProbe::Command {
            command: "false",
            args: &[],
        };
        assert_eq!(probe_auth(&yes, None).await, Some(true));
        assert_eq!(probe_auth(&no, None).await, Some(false));
        // An absent binary is unreadable, never a guess.
        let absent = AuthProbe::Command {
            command: "nika-no-such-binary- anywhere",
            args: &[],
        };
        assert_eq!(probe_auth(&absent, None).await, None);

        // HomeFile: presence against the INJECTED home.
        let dir = std::env::temp_dir().join(format!("nika-auth-probe-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".gemini")).expect("dir");
        std::fs::write(dir.join(".gemini/google_accounts.json"), b"{}").expect("file");
        let home = dir.as_os_str();
        let present = AuthProbe::HomeFile(".gemini/google_accounts.json");
        let missing = AuthProbe::HomeFile(".qwen");
        assert_eq!(
            probe_auth_with(Some(home), None, &present, None).await,
            Some(true)
        );
        assert_eq!(
            probe_auth_with(Some(home), None, &missing, None).await,
            Some(false)
        );
        // No home at all: unreadable, never a guess.
        assert_eq!(probe_auth_with(None, None, &present, None).await, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn kimi_auth_uses_its_override_and_detects_provider() {
        let root = std::fs::canonicalize(std::env::temp_dir())
            .expect("the test temp root canonicalizes")
            .join(format!("nika-kimi-auth-{}", std::process::id()));
        let home = root.join("home");
        let default_credentials = home.join(".kimi-code/credentials");
        std::fs::create_dir_all(&default_credentials).expect("default credentials dir");

        let rows = crate::registry_with(&|_| None).expect("registry loads");
        let row = rows
            .iter()
            .find(|row| row.adapter.id == "kimi-code")
            .expect("kimi row");
        let policy = row.directory_auth.expect("secure directory policy");

        assert_eq!(
            probe_auth_with(Some(home.as_os_str()), None, &row.auth, Some(policy)).await,
            Some(false),
            "an empty default store is not authentication"
        );
        std::fs::write(
            default_credentials.join("kimi-code.json"),
            b"opaque-fixture",
        )
        .expect("opaque fixture");
        assert_eq!(
            probe_auth_with(Some(home.as_os_str()), None, &row.auth, Some(policy)).await,
            Some(true),
            "a non-empty top-level provider JSON is the metadata-only proxy"
        );

        let relocated = root.join("relocated");
        std::fs::create_dir_all(&relocated).expect("relocated root");
        assert_eq!(
            probe_auth_with(
                Some(home.as_os_str()),
                Some(relocated.as_os_str()),
                &row.auth,
                Some(policy),
            )
            .await,
            Some(false),
            "KIMI_CODE_HOME wins even when the default store is populated"
        );
        let relocated_credentials = relocated.join("credentials");
        std::fs::create_dir_all(&relocated_credentials).expect("relocated credentials");
        assert_eq!(
            probe_auth_with(
                Some(home.as_os_str()),
                Some(relocated.as_os_str()),
                &row.auth,
                Some(policy),
            )
            .await,
            Some(false),
            "an empty relocated store is not authentication"
        );
        std::fs::write(
            relocated_credentials.join("kimi-code.json"),
            b"opaque-fixture",
        )
        .expect("opaque relocated fixture");
        assert_eq!(
            probe_auth_with(
                Some(home.as_os_str()),
                Some(relocated.as_os_str()),
                &row.auth,
                Some(policy),
            )
            .await,
            Some(true)
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn kimi_auth_refuses_relative_or_symlinked_override() {
        let root = std::fs::canonicalize(std::env::temp_dir())
            .expect("the test temp root canonicalizes")
            .join(format!("nika-kimi-auth-roots-{}", std::process::id()));
        let home = root.join("home");
        let relocated = root.join("relocated");
        std::fs::create_dir_all(relocated.join("credentials")).expect("relocated credentials");
        let rows = crate::registry_with(&|_| None).expect("registry loads");
        let row = rows
            .iter()
            .find(|row| row.adapter.id == "kimi-code")
            .expect("kimi row");
        let policy = row.directory_auth.expect("secure directory policy");

        assert_eq!(
            probe_auth_with(
                Some(home.as_os_str()),
                Some(std::ffi::OsStr::new("relative-kimi-home")),
                &row.auth,
                Some(policy),
            )
            .await,
            Some(false),
            "a relative override is ambiguous"
        );

        #[cfg(unix)]
        {
            let linked = root.join("linked");
            std::os::unix::fs::symlink(&relocated, &linked).expect("symlink fixture");
            assert_eq!(
                probe_auth_with(
                    Some(home.as_os_str()),
                    Some(linked.as_os_str()),
                    &row.auth,
                    Some(policy),
                )
                .await,
                Some(false),
                "a symlinked credentials root fails closed"
            );
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn kimi_auth_refuses_noise_mcp_only_and_unreadable_credentials() {
        let root = std::fs::canonicalize(std::env::temp_dir())
            .expect("the test temp root canonicalizes")
            .join(format!("nika-kimi-auth-shape-{}", std::process::id()));
        let credentials = root.join("credentials");
        std::fs::create_dir_all(credentials.join("mcp")).expect("credential directories");
        std::fs::write(credentials.join(".DS_Store"), b"finder metadata").expect("metadata");
        std::fs::write(credentials.join("README"), b"not a credential").expect("prose");
        std::fs::write(credentials.join("mcp/server.json"), b"opaque-mcp-fixture")
            .expect("mcp fixture");
        std::fs::write(
            credentials.join("unrelated-provider.json"),
            b"opaque-unrelated-fixture",
        )
        .expect("unrelated provider fixture");

        let rows = crate::registry_with(&|_| None).expect("registry loads");
        let row = rows
            .iter()
            .find(|row| row.adapter.id == "kimi-code")
            .expect("kimi row");
        let policy = row.directory_auth.expect("secure directory policy");
        let probe = || probe_auth_with(None, Some(root.as_os_str()), &row.auth, Some(policy));
        assert_eq!(
            probe().await,
            Some(false),
            "filesystem noise, MCP-only, and unrelated providers do not witness the managed seat"
        );

        let credential = credentials.join("kimi-code.json");
        std::fs::write(&credential, b"opaque-provider-fixture").expect("provider fixture");
        assert_eq!(probe().await, Some(true));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o000))
                .expect("make credential unreadable");
            assert_eq!(
                probe().await,
                Some(false),
                "an unreadable provider credential fails closed"
            );
            std::fs::set_permissions(&credential, std::fs::Permissions::from_mode(0o600))
                .expect("restore fixture permissions");
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn kimi_auth_readability_refuses_a_path_swap_to_a_symlink() {
        let root = std::fs::canonicalize(std::env::temp_dir())
            .expect("the test temp root canonicalizes")
            .join(format!("nika-kimi-auth-race-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("fixture directory");
        let candidate = root.join("account.json");
        let replacement = root.join("other.json");
        std::fs::write(&candidate, b"original-provider-fixture").expect("candidate fixture");
        std::fs::write(&replacement, b"replacement-fixture").expect("replacement fixture");
        let witnessed = std::fs::symlink_metadata(&candidate).expect("lstat witness");

        std::fs::remove_file(&candidate).expect("swap candidate");
        std::os::unix::fs::symlink(&replacement, &candidate).expect("symlink replacement");
        assert!(
            open_witnessed_file(&candidate, &witnessed).is_none(),
            "opening a swapped pathname must not validate a different inode"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn kimi_auth_refuses_an_entry_swap_after_open_before_return() {
        let root = std::fs::canonicalize(std::env::temp_dir())
            .expect("the test temp root canonicalizes")
            .join(format!(
                "nika-kimi-auth-post-open-race-{}",
                std::process::id()
            ));
        let credentials = root.join("credentials");
        let replacement = root.join("replacement.json");
        std::fs::create_dir_all(&credentials).expect("credential directory");
        std::fs::write(
            credentials.join("kimi-code.json"),
            b"original-provider-fixture",
        )
        .expect("provider fixture");
        std::fs::write(&replacement, b"replacement-provider-fixture").expect("replacement fixture");

        let ready =
            provider_credential_directory_ready_with(&credentials, &["kimi-code.json"], |entry| {
                std::fs::remove_file(entry).expect("remove opened pathname");
                std::os::unix::fs::symlink(&replacement, entry)
                    .expect("replace opened pathname with symlink");
            });
        assert!(
            !ready,
            "the final pathname witness must reject a swap after File::open"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn kimi_auth_refuses_a_credentials_directory_swap() {
        let root = std::fs::canonicalize(std::env::temp_dir())
            .expect("the test temp root canonicalizes")
            .join(format!("nika-kimi-auth-dir-race-{}", std::process::id()));
        let credentials = root.join("credentials");
        let original = root.join("credentials-original");
        let replacement = root.join("credentials-replacement");
        std::fs::create_dir_all(&credentials).expect("credential directory");
        std::fs::create_dir_all(&replacement).expect("replacement directory");
        std::fs::write(replacement.join("kimi-code.json"), b"replacement-fixture")
            .expect("replacement fixture");
        let witnesses = path_component_witnesses(&credentials).expect("component witnesses");

        std::fs::rename(&credentials, &original).expect("move witnessed directory");
        std::os::unix::fs::symlink(&replacement, &credentials).expect("replace with symlink");
        assert!(
            !path_witnesses_unchanged(&witnesses),
            "a replaced credentials component must not survive its identity witness"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn probe_one_reports_the_version_and_the_auth_together() {
        let dir = std::env::temp_dir().join(format!("nika-probe-one-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let script = dir.join("fake.py");
        std::fs::write(&script, "print('fake-harness 1.4.0')\n").expect("script");
        let path = script.to_string_lossy().into_owned();
        let row = crate::registry::AdapterRow {
            adapter: crate::HarnessAdapter::new("fake-harness", "python3")
                .expect("id is no class token")
                .with_args(vec![path.clone()])
                .with_version_args(vec![path])
                .with_version_pin(VersionPin::new((1, 0), 1)),
            serves: &["mock"],
            auth: AuthProbe::Command {
                command: "true",
                args: &[],
            },
            directory_auth: None,
            package: "test-only",
        };
        let probed = probe_one(row).await;
        assert_eq!(probed.version, Some((1, 4)));
        assert_eq!(probed.authenticated, Some(true));
        assert!(probed.note.is_empty(), "{:?}", probed.note);
        assert!(probed.usable());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn probe_adapters_keeps_the_registry_order_not_the_join_order() {
        let mk = |id: &str| crate::registry::AdapterRow {
            adapter: crate::HarnessAdapter::new(id, "false")
                .expect("fine")
                .with_version_pin(VersionPin::new((1, 0), 1)),
            serves: &["mock"],
            auth: AuthProbe::HomeFile(".definitely-absent-nika-test"),
            directory_auth: None,
            package: "test-only",
        };
        let rows = vec![mk("zzz-first"), mk("aaa-second")];
        let out = probe_adapters(rows).await;
        let ids: Vec<&str> = out.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            ["zzz-first", "aaa-second"],
            "registry order, not alphabetical"
        );
        assert!(
            out.iter().all(|r| r.version.is_none()),
            "`false` prints no version"
        );
        assert_eq!(
            out.iter().map(|r| r.authenticated).collect::<Vec<_>>(),
            vec![Some(false), Some(false)]
        );
    }
}
