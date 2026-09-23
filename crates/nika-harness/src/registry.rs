// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The harness access REGISTRY (P3 B6 · R-5d) — the shipped adapter
//! rows, in the vendor order the operator ratified (G-3: gemini-cli
//! native → qwen-code native → kimi-code native → codex →
//! claude-code; Anthropic is never the default example).
//!
//! A row is the adapter's pinned IDENTITY: the binary to spawn (never
//! a shell line), the session argv, the version pin + the argv that
//! makes the ADAPTER print its version (the wrapper class's lesson —
//! `npx …` probed bare reports the wrapper, not the adapter), the
//! optional command-shape evidence needed when binary names collide,
//! the provider ids it can serve, and the AUTH SURFACE — how `doctor`
//! reads the auth state, which is never a credential read.
//!
//! The kill-switch is the operator's: `NIKA_HARNESS_DISABLE=codex,…`
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

/// Stronger metadata-only policy for an auth store directory whose
/// application home can be relocated independently of `$HOME`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DirectoryAuthProbe {
    /// The variable that overrides the application's default home.
    pub(crate) override_env: &'static str,
    /// The directory below the overridden application home.
    pub(crate) override_relative: &'static str,
    /// Exact top-level credential witnesses accepted for this seat.
    pub(crate) credential_files: &'static [&'static str],
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
    /// Stronger directory semantics for auth stores that need them.
    pub(crate) directory_auth: Option<DirectoryAuthProbe>,
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

/// The static table (G-3 order · native ACP first · Anthropic last). Every
/// row pins the version range its probe reads; every identity is a measured
/// `agentInfo.name`, never a guess (acp_measured_2026-09-22 in the campaign
/// notes). Eight rows: gemini-cli · qwen-code · kimi-code · opencode · codex ·
/// copilot · grok-build · claude-code.
fn rows() -> Result<Vec<AdapterRow>, HarnessError> {
    Ok(vec![
        AdapterRow {
            // Measured 2026-09-22 (gemini-cli 0.37.2): `--acp` and `--experimental-acp` both
            // start the agent; `agentInfo.name` is `gemini-cli`. The consumer OAuth tier was
            // retired on 2026-06-18 (Code Assist for individuals): a paid API key still serves,
            // read by the CLI from ITS OWN home (`GEMINI_CLI_HOME/.gemini/.env`, settings
            // `security.auth.selectedType = gemini-api-key`) — the operator points the seat at
            // that home; no key ever crosses the engine's environment boundary.
            adapter: HarnessAdapter::new("gemini-cli", "gemini")?
                .with_args(vec!["--acp".to_owned()])
                .with_identities(vec!["Gemini CLI".to_owned()])
                .with_version_pin(VersionPin::new((0, 37), 0))
                .with_passthrough_env(vec!["GEMINI_CLI_HOME".to_owned()]),
            serves: &["gemini"],
            auth: AuthProbe::HomeFile(".gemini/google_accounts.json"),
            directory_auth: None,
            package: "@google/gemini-cli (brew install gemini-cli · a paid API key: settings selectedType gemini-api-key in a GEMINI_CLI_HOME of its own)",
        },
        AdapterRow {
            adapter: HarnessAdapter::new("qwen-code", "qwen")?
                .with_args(vec!["--acp".to_owned(), "--experimental-skills".to_owned()])
                .with_identities(vec!["Qwen Code".to_owned()])
                .with_version_pin(VersionPin::new((0, 21), 0)),
            serves: &["qwen"],
            auth: AuthProbe::HomeFile(".qwen"),
            directory_auth: None,
            package: "@qwen-code/qwen-code (npm i -g @qwen-code/qwen-code)",
        },
        AdapterRow {
            // Measured 2026-09-22 against Kimi Code CLI 2.0.2 (the Node rewrite;
            // `kimi --version` → `2.0.2` · `kimi acp --help` → "Run kimi-code as an Agent
            // Client Protocol (ACP) server over stdio." · `agentInfo.name` → `Kimi Code CLI`).
            // The Python `kimi-cli` (0.37, 2026-08-22) is archived upstream. Official Zed
            // args are `["acp"]`; `--login` is device-code-then-exit, never the session argv.
            adapter: HarnessAdapter::new("kimi-code", "kimi")?
                .with_args(vec!["acp".to_owned()])
                .with_identities(vec!["Kimi Code CLI".to_owned(), "kimi-cli".to_owned()])
                .with_version_pin(VersionPin::new((2, 0), 2))
                .with_command_shape_probe(
                    vec!["acp".to_owned(), "--help".to_owned()],
                    "Agent Client Protocol",
                )
                .with_passthrough_env(vec!["KIMI_CODE_HOME".to_owned()]),
            serves: &["moonshot"],
            // Official store (data-locations.md): `$KIMI_CODE_HOME/credentials/`
            // (default `~/.kimi-code/credentials/`). The probe reads
            // path metadata plus a zero-read readability handle; it
            // never reads a credential. No `login status` command exists.
            auth: AuthProbe::HomeFile(".kimi-code/credentials"),
            directory_auth: Some(DirectoryAuthProbe {
                override_env: "KIMI_CODE_HOME",
                override_relative: "credentials",
                credential_files: &["kimi-code.json"],
            }),
            package: "@moonshot-ai/kimi-code (npm i -g @moonshot-ai/kimi-code · https://moonshotai.github.io/kimi-code/)",
        },
        AdapterRow {
            // Measured 2026-09-22 (OpenCode 1.18.15): `opencode acp` speaks ACP over stdio,
            // `agentInfo.name` → `OpenCode`, a `model` config option over every provider it
            // holds. Its auth store is its own (`opencode auth login`); the engine's env floor
            // passes it no key, so a seat runs on what the CLI itself holds.
            adapter: HarnessAdapter::new("opencode", "opencode")?
                .with_args(vec!["acp".to_owned()])
                .with_identities(vec!["OpenCode".to_owned()])
                .with_handshake_probe()
                .with_version_pin(VersionPin::new((1, 18), 1)),
            serves: &["opencode"],
            auth: AuthProbe::HomeFile(".local/share/opencode/auth.json"),
            directory_auth: None,
            package: "opencode-ai (brew install opencode · npm i -g opencode-ai · https://opencode.ai/docs/acp)",
        },
        AdapterRow {
            // The maintained adapter is `@agentclientprotocol/codex-acp` (1.13.0 measured
            // 2026-09-22: `agentInfo.name` → `@agentclientprotocol/codex-acp`, auth methods
            // api-key · chat-gpt, models gpt-6-astra[…]); the `@zed-industries/codex-acp`
            // package (0.16.0, same bin name, answers `codex-acp`) is deprecated on npm and
            // still accepted by the pin. `npm i -g` puts the bin on PATH (the npx-on-the-spec
            // form does NOT link these packages' bins — measured 2026-08-07). No working
            // version flag — the probe is the initialize self-report. The 0.16.0 model
            // override (`-c model=gpt-5.5`, 2026-08-07) is retired: the adapter's own default
            // is the vendor's current model, and a run names its model through the session.
            adapter: HarnessAdapter::new("codex", "codex-acp")?
                .with_identities(vec!["Codex".to_owned()])
                .with_handshake_probe()
                .with_version_pin(VersionPin::new((0, 16), 1)),
            serves: &["openai"],
            auth: AuthProbe::Command {
                command: "codex",
                args: &["login", "status"],
            },
            directory_auth: None,
            package: "@agentclientprotocol/codex-acp (npm i -g · wraps the codex CLI's own auth · @zed-industries/codex-acp is deprecated)",
        },
        AdapterRow {
            // Measured 2026-09-22 (GitHub Copilot CLI 1.0.77, ACP public preview): `copilot
            // --acp` over stdio, `agentInfo.name` → `Copilot`, modes agent · plan · autopilot
            // as URI ids, a `mode` config option. Auth is the CLI's own (`copilot login`).
            adapter: HarnessAdapter::new("copilot", "copilot")?
                .with_args(vec!["--acp".to_owned()])
                .with_identities(vec!["GitHub Copilot".to_owned()])
                .with_handshake_probe()
                .with_version_pin(VersionPin::new((1, 0), 1)),
            serves: &["github"],
            auth: AuthProbe::HomeFile(".copilot/config.json"),
            directory_auth: None,
            package: "@github/copilot (npm i -g @github/copilot · https://docs.github.com/copilot/reference/copilot-cli-reference/acp-server)",
        },
        AdapterRow {
            // Measured 2026-09-22 (Grok Build 1.0.40): `grok agent stdio` speaks ACP (auth
            // methods xai.api_key · cached_token · grok.com; models grok-4.7 · 4.7-fast · 4.6 ·
            // 4.5 with a reasoning-effort option) but its initialize answer carries NO
            // `agentInfo` — so the identity rides `grok --version` (`grok 1.0.40 (…) [stable]`),
            // never a handshake. Auth is the CLI's own store (`grok login` → `~/.grok/auth.json`).
            adapter: HarnessAdapter::new("grok-build", "grok")?
                .with_args(vec!["agent".to_owned(), "stdio".to_owned()])
                .with_identities(vec!["Grok Build".to_owned(), "grok".to_owned()])
                .with_version_pin(VersionPin::new((1, 0), 1)),
            serves: &["xai"],
            auth: AuthProbe::HomeFile(".grok/auth.json"),
            directory_auth: None,
            package: "@xai-official/grok (npm i -g @xai-official/grok · https://docs.x.ai/build)",
        },
        AdapterRow {
            // The maintained adapter is `@agentclientprotocol/claude-agent-acp` (0.81.0
            // measured 2026-09-22: `agentInfo.name` → `@agentclientprotocol/claude-agent-acp`,
            // title `Claude Agent`; it opens a session under Claude Code's `auto` permission
            // mode, which the deprecated `@zed-industries/claude-agent-acp` 0.23.1 — same bin
            // name, answers its own scoped name — refuses with « Invalid
            // permissions.defaultMode: auto. »; the pin accepts both, the refusal rides
            // verbatim). Auth is Claude Code's own login: a credential-shaped variable never
            // crosses, so a seat runs on the subscription, never on an ambient API key.
            adapter: HarnessAdapter::new("claude-code", "claude-agent-acp")?
                .with_identities(vec!["Claude Agent".to_owned()])
                .with_handshake_probe()
                .with_version_pin(VersionPin::new((0, 23), 0)),
            serves: &["anthropic"],
            auth: AuthProbe::Command {
                command: "claude",
                args: &["auth", "status"],
            },
            directory_auth: None,
            package: "@agentclientprotocol/claude-agent-acp (npm i -g · wraps the claude CLI's own auth · @zed-industries/claude-agent-acp is deprecated)",
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
    fn the_table_ships_the_eight_rows_in_the_ratified_order() {
        let rows = registry_with(&no_env).expect("the static table loads");
        let ids: Vec<&str> = rows.iter().map(|r| r.adapter.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "gemini-cli",
                "qwen-code",
                "kimi-code",
                "opencode",
                "codex",
                "copilot",
                "grok-build",
                "claude-code"
            ],
            "G-3 · native first · Anthropic is never the default example"
        );
        let vocab: Vec<&str> = nika_types::access::HarnessRuntime::ALL
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vocab, "registry ids are the product tokens");
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
            row.adapter.answers_as("Kimi Code CLI"),
            "the measured `agentInfo.name` (2.0.2) is a declared identity"
        );
        assert_eq!(
            row.adapter
                .version_pin
                .as_ref()
                .map(|p| (p.min, p.max_major)),
            Some(((2, 0), 2)),
            "the pin reads the Node rewrite (2.0.2 measured 2026-09-22), not the archived 0.37 CLI"
        );
        assert!(
            !row.adapter.probe_via_handshake,
            "kimi --version works; handshake is the wrapper class"
        );
        assert!(
            row.adapter.command_shape_probe.is_some(),
            "the shared binary name needs evidence beyond its version"
        );
        assert_eq!(row.adapter.version_args, vec!["--version".to_owned()]);
        assert_eq!(
            row.directory_auth,
            Some(DirectoryAuthProbe {
                override_env: "KIMI_CODE_HOME",
                override_relative: "credentials",
                credential_files: &["kimi-code.json"],
            })
        );
        let pin = row.adapter.version_pin.as_ref().expect("pinned");
        assert_eq!(
            pin.min,
            (2, 0),
            "floor is the measured Node rewrite (2.0.2, 2026-09-22)"
        );
        assert_eq!(pin.max_major, 2);
        assert!(pin.accepts(2, 0));
        assert!(
            !pin.accepts(0, 37),
            "the archived Python CLI is below the floor"
        );
        assert!(pin.accepts(2, 99));
        assert!(!pin.accepts(1, 99), "below the measured floor");
        assert!(!pin.accepts(3, 0), "a new major is a new dialect");
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
        for (id, command) in [
            ("codex", "codex-acp"),
            ("claude-code", "claude-agent-acp"),
            ("opencode", "opencode"),
            ("copilot", "copilot"),
        ] {
            let row = rows.iter().find(|r| r.adapter.id == id).expect("row");
            // The bin on PATH (npm i -g) — the npx-on-the-spec form does
            // NOT link these packages' bins (measured 2026-08-07).
            assert_eq!(
                row.adapter.command, command,
                "{id}: spawned via ACP speaker"
            );
            assert!(
                row.adapter.probe_via_handshake,
                "{id}: no working version flag exists — the probe is the initialize self-report"
            );
            assert!(
                row.package.contains(command),
                "{id}: the install pointer names the pinned package"
            );
            assert!(
                row.adapter
                    .answers_as(&format!("@agentclientprotocol/{command}")),
                "{id}: the maintained scoped package name is the adapter"
            );
        }
        let grok = rows
            .iter()
            .find(|r| r.adapter.id == "grok-build")
            .expect("grok-build ships");
        assert!(
            !grok.adapter.probe_via_handshake,
            "grok answers initialize without agentInfo: its identity rides `grok --version`"
        );
        assert_eq!(
            grok.adapter.args,
            vec!["agent".to_owned(), "stdio".to_owned()]
        );
    }

    #[test]
    fn the_kill_switch_removes_rows_at_load() {
        let env = |name: &str| (name == DISABLE_ENV).then(|| "codex, qwen-code ".to_owned());
        let rows = registry_with(&env).expect("loads");
        let ids: Vec<&str> = rows.iter().map(|r| r.adapter.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "gemini-cli",
                "kimi-code",
                "opencode",
                "copilot",
                "grok-build",
                "claude-code"
            ],
            "whitespace-tolerant removal"
        );
    }

    #[test]
    fn an_unknown_kill_switch_entry_changes_nothing() {
        let env = |name: &str| (name == DISABLE_ENV).then(|| "not-an-adapter".to_owned());
        let rows = registry_with(&env).expect("loads");
        assert_eq!(rows.len(), 8);
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
