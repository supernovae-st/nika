// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The adapter VERSION probe (spec §4 · the promise B3.2 owed) — the
//! binary's identity is judged BEFORE a session, never discovered
//! mid-run.
//!
//! Why before: an adapter outside its pin range speaks a dialect this
//! client did not read. Learning that at `initialize` wastes a spawn
//! and reports a protocol confusion where the truth is a VERSION
//! mismatch. The probe runs `<command> --version`, parses the first
//! semver-ish token out of whatever the CLI prints (every harness
//! prints its own prose around it), and judges it against the
//! adapter's declared range.
//!
//! What the probe is NOT: a credential read, a network call, or a
//! session. It spawns the same confined child shape the session does
//! (composed env · no shell · bounded output) and reads one line.

use nika_kernel::ai::harness::HarnessError;

use crate::registry::{AdapterRow, AuthProbe};
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
    /// The auth surface's verdict: `Some(true)` an auth exists,
    /// `Some(false)` the surface says none, `None` the surface itself
    /// is unreadable here (absent is honest — never a guess).
    pub authenticated: Option<bool>,
    /// The install pointer (the registry row's package line).
    pub package: String,
    /// What the probe saw when detection failed (the adapter's own
    /// words) — empty on a clean detection.
    pub note: String,
}

impl AdapterProbeRow {
    /// Whether the adapter can serve a session NOW (detected AND inside
    /// the pin). Auth stays a separate column — a harness without auth
    /// is detected-but-refusing, a truth the row keeps distinct.
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
    let authenticated = probe_auth(&row.auth).await;
    AdapterProbeRow {
        id: row.adapter.id.clone(),
        version,
        authenticated,
        package: row.package.to_owned(),
        note,
    }
}

/// The auth surface probe — an exit code or a presence bit, bounded
/// like every spawn (a hung status command is `None`, not a hang).
async fn probe_auth(surface: &AuthProbe) -> Option<bool> {
    #[allow(clippy::disallowed_methods)] // the sanctioned env boundary ($HOME presence)
    let home = std::env::var_os("HOME");
    probe_auth_with(home.as_deref(), surface).await
}

/// [`probe_auth`] with the home dir injected — the pure half tests
/// drive (writing `$HOME` would need `unsafe` under Rust 2024).
async fn probe_auth_with(home: Option<&std::ffi::OsStr>, surface: &AuthProbe) -> Option<bool> {
    match surface {
        AuthProbe::HomeFile(rel) => Some(std::path::Path::new(home?).join(rel).exists()),
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
            package: "test-only",
        };
        let out = probe_adapters_sync(vec![mk("sync-a"), mk("sync-b")]);
        assert_eq!(out.len(), 2, "the façade probes, never an empty vec");
        assert!(out.iter().all(|r| r.version.is_none()));
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
        assert_eq!(probe_auth(&yes).await, Some(true));
        assert_eq!(probe_auth(&no).await, Some(false));
        // An absent binary is unreadable, never a guess.
        let absent = AuthProbe::Command {
            command: "nika-no-such-binary- anywhere",
            args: &[],
        };
        assert_eq!(probe_auth(&absent).await, None);

        // HomeFile: presence against the INJECTED home.
        let dir = std::env::temp_dir().join(format!("nika-auth-probe-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".gemini")).expect("dir");
        std::fs::write(dir.join(".gemini/google_accounts.json"), b"{}").expect("file");
        let home = dir.as_os_str();
        let present = AuthProbe::HomeFile(".gemini/google_accounts.json");
        let missing = AuthProbe::HomeFile(".qwen");
        assert_eq!(probe_auth_with(Some(home), &present).await, Some(true));
        assert_eq!(probe_auth_with(Some(home), &missing).await, Some(false));
        // kimi-code: the official store directory, existence only —
        // no file is written, so no secret is ever opened.
        std::fs::create_dir_all(dir.join(".kimi-code/credentials")).expect("kimi store");
        let kimi = AuthProbe::HomeFile(".kimi-code/credentials");
        assert_eq!(probe_auth_with(Some(home), &kimi).await, Some(true));
        let kimi_absent = AuthProbe::HomeFile(".kimi-code/credentials");
        let empty = std::env::temp_dir().join(format!("nika-auth-empty-{}", std::process::id()));
        std::fs::create_dir_all(&empty).expect("empty home");
        assert_eq!(
            probe_auth_with(Some(empty.as_os_str()), &kimi_absent).await,
            Some(false)
        );
        let _ = std::fs::remove_dir_all(&empty);
        // No home at all: unreadable, never a guess.
        assert_eq!(probe_auth_with(None, &present).await, None);
        let _ = std::fs::remove_dir_all(&dir);
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
