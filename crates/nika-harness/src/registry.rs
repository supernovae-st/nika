// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The harness access REGISTRY (P3 B6 · R-5d) — the shipped adapter
//! rows, in the vendor order the operator ratified (G-3: gemini-cli
//! native → qwen-code native → kimi-code native → codex-acp →
//! claude-agent-acp; Anthropic is never the default example).
//!
//! A row is the adapter's pinned IDENTITY: the binary to spawn (never
//! a shell line), the session argv, the version pin + the argv that
//! makes the ADAPTER print its version (the wrapper class's lesson —
//! `npx …` probed bare reports the wrapper, not the adapter), the
//! provider ids it can serve, and the AUTH SURFACE — how `doctor`
//! reads the auth state, which is never a credential read.
//!
//! The kill-switch is the operator's: `NIKA_HARNESS_DISABLE=codex-acp,…`
//! removes rows at load (the vendor-flip law: a flipped adapter is a
//! refusal in plain words, never a silent substitute). Row ids are
//! unique and never equal an access-class token (R-5d) — the second
//! half is unrepresentable ([`HarnessAdapter::new`] refuses at
//! construction), the first half is checked at load.

use nika_kernel::ai::harness::HarnessError;

use crate::probe::VersionPin;
use crate::spawn::HarnessAdapter;

/// How the auth state is read — presence and exit codes only, NEVER a
/// credential read (A-3: the harness owns its auth store; nika never
/// opens it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthProbe {
    /// A status command whose EXIT CODE names the state (0 = an auth
    /// exists). Probed like the version: confined spawn, bounded,
    /// deadlined — its stdout is never treated as a credential and
    /// never leaves the doctor row.
    Command {
        /// The binary (the harness's own CLI, not the adapter wrapper).
        command: &'static str,
        /// The status argv (`login status` · `auth status`).
        args: &'static [&'static str],
    },
    /// A presence check under `$HOME` (the harness's account-selection
    /// file) — existence only; the file is never opened.
    HomeFile(&'static str),
}

/// One shipped adapter row.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AdapterRow {
    /// The pinned adapter identity (id · command · argv · pins).
    pub adapter: HarnessAdapter,
    /// The provider ids this harness can serve (the resolver's bridge).
    pub serves: &'static [&'static str],
    /// The auth surface (never a credential read).
    pub auth: AuthProbe,
    /// The install pointer the fix line names.
    pub package: &'static str,
}

/// The kill-switch env var: a comma-separated list of adapter ids the
/// operator refuses on this machine (read once at registry load).
pub const DISABLE_ENV: &str = "NIKA_HARNESS_DISABLE";

/// The shipped rows, kill-switch applied.
///
/// # Errors
///
/// [`HarnessError::Unavailable`] when the table itself is broken (a
/// duplicate id — the class-token collision is already unrepresentable
/// at construction): a broken registry fails CLOSED, never a partial
/// silent set.
pub fn registry() -> Result<Vec<AdapterRow>, HarnessError> {
    #[allow(clippy::disallowed_methods)] // the sanctioned env boundary (declaration.rs' own)
    registry_with(&|name| std::env::var(name).ok())
}

/// [`registry`] over an injected env lookup — the pure half tests
/// drive (the `seat_from_lookup` precedent: Rust 2024 needs unsafe to
/// write process env, and the workspace forbids it).
///
/// # Errors
///
/// Same as [`registry`].
pub fn registry_with(
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<AdapterRow>, HarnessError> {
    let disabled: Vec<String> = env(DISABLE_ENV)
        .map(|raw| raw.split(',').map(|s| s.trim().to_owned()).collect())
        .unwrap_or_default();
    let mut rows = rows()?;
    rows.retain(|row| !disabled.iter().any(|d| d == &row.adapter.id));
    let mut seen = std::collections::BTreeSet::new();
    for row in &rows {
        if !seen.insert(row.adapter.id.as_str()) {
            return Err(HarnessError::Unavailable {
                reason: format!(
                    "adapter id `{}` is declared twice — the registry's ids are unique (R-5d)",
                    row.adapter.id
                ),
            });
        }
    }
    Ok(rows)
}

/// The static table (G-3 order). Every npm-spawned row pins its package
/// spec exactly (`@<version>` in the argv) AND pins the printed version
/// range — the spawn pin and the probe pin are the same fact.
fn rows() -> Result<Vec<AdapterRow>, HarnessError> {
    Ok(vec![
        AdapterRow {
            adapter: HarnessAdapter::new("gemini-cli", "gemini")?
                .with_args(vec!["--experimental-acp".to_owned()])
                .with_version_pin(VersionPin::new((0, 37), 0)),
            serves: &["gemini"],
            auth: AuthProbe::HomeFile(".gemini/google_accounts.json"),
            package: "@google/gemini-cli (brew install gemini-cli)",
        },
        AdapterRow {
            adapter: HarnessAdapter::new("qwen-code", "qwen")?
                .with_args(vec!["--acp".to_owned(), "--experimental-skills".to_owned()])
                .with_version_pin(VersionPin::new((0, 21), 0)),
            serves: &["qwen"],
            auth: AuthProbe::HomeFile(".qwen"),
            package: "@qwen-code/qwen-code (npm i -g @qwen-code/qwen-code)",
        },
        AdapterRow {
            // Measured 2026-08-22 against the installed Kimi Code CLI
            // (`kimi --version` → `0.37.2` · `kimi acp --help` → "Run
            // kimi-code as an Agent Client Protocol (ACP) server over
            // stdio."). Official Zed args are `["acp"]`. `--login` is
            // a separate option (device-code then exit) and is NOT
            // the session argv. Native CLI: `--version` works, so the
            // probe is the flag, never a handshake.
            adapter: HarnessAdapter::new("kimi-code", "kimi")?
                .with_args(vec!["acp".to_owned()])
                .with_version_pin(VersionPin::new((0, 37), 0))
                .with_passthrough_env(vec!["KIMI_CODE_HOME".to_owned()]),
            serves: &["moonshot"],
            // Official store (data-locations.md): `$KIMI_CODE_HOME/credentials/`
            // (default `~/.kimi-code/credentials/`). Existence only —
            // the files are never opened. No `login status` command
            // exists (`kimi login status` is `unknown command`).
            auth: AuthProbe::HomeFile(".kimi-code/credentials"),
            package: "kimi-code (kimi upgrade · https://moonshotai.github.io/kimi-code/)",
        },
        AdapterRow {
            adapter: HarnessAdapter::new("codex-acp", "codex-acp")?
                // The npm class, honestly: `npm i -g` puts the bin on
                // PATH (the npx-on-the-spec form does NOT link these
                // packages' bins — measured 2026-08-07). No working
                // version flag — the probe is the initialize self-report.
                // The curated model override: 0.16.0's built-in default
                // (gpt-5.6-sol) predates the backend's support floor —
                // the backend refuses it with "requires a newer version"
                // (measured 2026-08-07); the override rides the row and
                // moves with each re-pin.
                .with_args(vec!["-c".to_owned(), "model=gpt-5.5".to_owned()])
                .with_handshake_probe()
                .with_version_pin(VersionPin::new((0, 16), 0)),
            serves: &["openai"],
            auth: AuthProbe::Command {
                command: "codex",
                args: &["login", "status"],
            },
            package: "@zed-industries/codex-acp@0.16.0 (npm i -g · wraps the codex CLI's own auth)",
        },
        AdapterRow {
            adapter: HarnessAdapter::new("claude-agent-acp", "claude-agent-acp")?
                .with_handshake_probe()
                .with_version_pin(VersionPin::new((0, 23), 0)),
            serves: &["anthropic"],
            auth: AuthProbe::Command {
                command: "claude",
                args: &["auth", "status"],
            },
            package: "@zed-industries/claude-agent-acp@0.23.1 (npm i -g · wraps the claude CLI's own auth)",
        },
    ])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn no_env(name: &str) -> Option<String> {
        let _ = name;
        None
    }

    #[test]
    fn the_table_ships_the_five_rows_in_the_ratified_order() {
        let rows = registry_with(&no_env).expect("the static table loads");
        let ids: Vec<&str> = rows.iter().map(|r| r.adapter.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "gemini-cli",
                "qwen-code",
                "kimi-code",
                "codex-acp",
                "claude-agent-acp"
            ],
            "G-3 · native first · Anthropic is never the default example"
        );
    }

    #[test]
    fn kimi_code_is_the_native_acp_subcommand_not_a_wrapper() {
        let rows = registry_with(&no_env).expect("loads");
        let row = rows
            .iter()
            .find(|r| r.adapter.id == "kimi-code")
            .expect("kimi-code ships");
        assert_eq!(row.adapter.command, "kimi");
        assert_eq!(row.adapter.args, vec!["acp".to_owned()]);
        assert!(
            !row.adapter.probe_via_handshake,
            "kimi --version works; handshake is the wrapper class"
        );
        assert_eq!(row.adapter.version_args, vec!["--version".to_owned()]);
        let pin = row.adapter.version_pin.as_ref().expect("pinned");
        assert_eq!(pin.min, (0, 37), "floor is the measured ACP-native release");
        assert_eq!(pin.max_major, 0);
        assert!(pin.accepts(0, 37));
        assert!(pin.accepts(0, 99));
        assert!(!pin.accepts(0, 36), "below the measured floor");
        assert!(!pin.accepts(1, 0), "a new major is a new dialect");
        assert_eq!(row.serves, &["moonshot"]);
        assert_eq!(row.auth, AuthProbe::HomeFile(".kimi-code/credentials"));
        assert_eq!(
            row.adapter.passthrough_env,
            vec!["KIMI_CODE_HOME".to_owned()],
            "relocated home must reach the child; it is a path, not a secret"
        );
        assert!(
            !row.adapter.args.iter().any(|a| a == "--login"),
            "`acp --login` is device-code-then-exit, never the session argv"
        );
    }

    #[test]
    fn every_row_is_pinned_and_its_id_is_never_a_class_token() {
        let rows = registry_with(&no_env).expect("loads");
        for row in &rows {
            assert!(
                row.adapter.version_pin.is_some(),
                "{}: a shipped row is pinned (spec §4)",
                row.adapter.id
            );
            assert!(
                !nika_types::access::AccessClass::ALL
                    .iter()
                    .any(|c| c.as_str() == row.adapter.id),
                "{}: never a class token (R-5d)",
                row.adapter.id
            );
            assert!(
                !row.serves.is_empty(),
                "{}: serves at least one provider",
                row.adapter.id
            );
        }
    }

    #[test]
    fn the_wrapper_rows_probe_by_handshake_never_a_wrapper_flag() {
        let rows = registry_with(&no_env).expect("loads");
        for id in ["codex-acp", "claude-agent-acp"] {
            let row = rows.iter().find(|r| r.adapter.id == id).expect("row");
            // The bin on PATH (npm i -g) — the npx-on-the-spec form does
            // NOT link these packages' bins (measured 2026-08-07).
            assert_eq!(row.adapter.command, id, "{id}: spawned directly");
            assert!(
                row.adapter.probe_via_handshake,
                "{id}: no working version flag exists — the probe is the initialize self-report"
            );
            assert!(
                row.package.contains(id),
                "{id}: the install pointer names the pinned package"
            );
        }
    }

    #[test]
    fn the_kill_switch_removes_rows_at_load() {
        let env = |name: &str| (name == DISABLE_ENV).then(|| "codex-acp, qwen-code ".to_owned());
        let rows = registry_with(&env).expect("loads");
        let ids: Vec<&str> = rows.iter().map(|r| r.adapter.id.as_str()).collect();
        assert_eq!(
            ids,
            ["gemini-cli", "kimi-code", "claude-agent-acp"],
            "whitespace-tolerant removal"
        );
    }

    #[test]
    fn an_unknown_kill_switch_entry_changes_nothing() {
        let env = |name: &str| (name == DISABLE_ENV).then(|| "not-an-adapter".to_owned());
        let rows = registry_with(&env).expect("loads");
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn auth_surfaces_are_presence_or_exit_code_never_a_credential() {
        let rows = registry_with(&no_env).expect("loads");
        for row in &rows {
            match row.auth {
                AuthProbe::Command { command, args } => {
                    assert!(!command.is_empty() && !args.is_empty());
                    // The status surface is the harness's OWN CLI, never
                    // the adapter wrapper (the adapter owns no auth).
                    assert_ne!(
                        command, "npx",
                        "{}: auth probes the real CLI",
                        row.adapter.id
                    );
                }
                AuthProbe::HomeFile(rel) => {
                    assert!(
                        rel.starts_with('.'),
                        "{rel} lives under the harness's own dir"
                    );
                    // Existence only — the path is never opened. KEY /
                    // TOKEN / SECRET name a secret FILE. A vendor's
                    // official store DIRECTORY may contain the word
                    // `credentials` (kimi-code data-locations:
                    // `~/.kimi-code/credentials/`) and that is the
                    // presence bit, not a read.
                    assert!(
                        !rel.to_ascii_uppercase().contains("KEY")
                            && !rel.to_ascii_uppercase().contains("TOKEN")
                            && !rel.to_ascii_uppercase().contains("SECRET"),
                        "{rel} must never name a secret file"
                    );
                }
            }
        }
    }
}
