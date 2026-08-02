// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The MCP spawn confinement — the SAME OS sandbox the `exec` verb rides
//! (spec 01 §permits · ADR-095 Layer 6), never a bespoke second sandbox.
//!
//! The threat model: a configured MCP server IS arbitrary code execution
//! with the operator's full privileges — the same class as the `exec`
//! verb's child, reached by a supply chain the operator vetted only at pin
//! time (the anti-rug-pull pins vet the tool DEFINITIONS, not the server
//! binary's behavior). So the spawn is confined by construction, through
//! the one `CommandSandbox` seam the exec path uses:
//!
//! - **Backend** — [`platform_sandbox`] makes the same selection the exec
//!   composition root makes: Seatbelt on macOS, bwrap on Linux, the
//!   deliberate loud [`NoopSandbox`](nika_kernel::command_sandbox::NoopSandbox)
//!   elsewhere (named by the note on every connect — never silent).
//! - **Filesystem** — [`McpServerConfig::sandbox_spec`] anchors the
//!   boundary at the project dir (the same boundary the exec sandbox
//!   computes for the run root): read + write confined to the project tree,
//!   so the server gets NO write outside it by default.
//! - **Network** — the registry's per-server `network` field maps onto the
//!   kernel's [`NetPolicy`] tri-state verbatim: ABSENT is
//!   [`NetPolicy::Deny`] (a local MCP server needs no network — fail-closed),
//!   `"allow"` is the explicit escape hatch, and `{ "allowlist": [hosts…] }`
//!   reserves the host-granular arm — confining EXACTLY as `"allow"` until
//!   the loopback egress proxy lands (the transitional mapping #658
//!   documented for exec; `proxy_port` stays `None`).
//! - **Environment** — the child gets a SCRUBBED environment: the spawn
//!   site `env_clear`s every ambient value (provider API keys, session
//!   tokens, the dangerous-floor injection vectors) and re-admits
//!   ONLY the curated runner floor. MCP config values pass
//!   explicitly (argv), never via ambient env inheritance.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use nika_kernel::command_sandbox::CommandSandbox;
use nika_kernel::process::{NetPolicy, SandboxSpec};

use crate::client::McpServerConfig;

/// The OS command sandbox for this platform — the same selection the exec
/// composition root makes (ADR-095 Layer 6): macOS rides Seatbelt, Linux
/// rides bubblewrap, anything else is the deliberate
/// [`NoopSandbox`](nika_kernel::command_sandbox::NoopSandbox) — loud via the
/// connect note ([`StdioMcpClient::sandbox_note`](crate::client::StdioMcpClient::sandbox_note)),
/// never the silent default.
#[must_use]
pub fn platform_sandbox() -> Arc<dyn CommandSandbox> {
    #[cfg(target_os = "macos")]
    if nika_sandbox_seatbelt::SeatbeltSandbox::available() {
        return Arc::new(nika_sandbox_seatbelt::SeatbeltSandbox::new());
    }
    #[cfg(target_os = "linux")]
    if nika_sandbox_landlock::LandlockSandbox::available() {
        return Arc::new(nika_sandbox_landlock::LandlockSandbox::new());
    }
    Arc::new(nika_kernel::command_sandbox::NoopSandbox)
}

/// The one-line sandbox mode note printed on every connect/verify —
/// `sandboxed (seatbelt · net deny)` style. The allowlist arm names its
/// transitional mapping (host-granular egress needs the loopback proxy —
/// until it lands, the arm confines exactly as `allow`, loudly). A `noop`
/// backend is the deliberate unconfined choice and the note SCREAMS it —
/// "unconfined" is always a visible decision, never an accident.
pub(crate) fn sandbox_note(backend: &str, net: &NetPolicy) -> String {
    let net_text = match net {
        NetPolicy::Deny => "net deny".to_owned(),
        NetPolicy::Allow => "net allow".to_owned(),
        NetPolicy::Allowlist(list) => format!(
            "net allowlist ({} host(s) · proxyless transitional: confined as allow)",
            list.hosts.len()
        ),
        // The #[non_exhaustive] law: a future arm fails CLOSED — the note
        // names the deny, never claims a grant the backend did not make.
        _ => "net deny (unrecognized arm — fail-closed)".to_owned(),
    };
    if backend == "noop" {
        format!(
            "UNSANDBOXED (no seatbelt/landlock on this host — the server runs unconfined · {net_text})"
        )
    } else {
        format!("sandboxed ({backend} · {net_text})")
    }
}

impl McpServerConfig {
    /// The OS-confinement boundary for this server, anchored at
    /// `project_dir` — the project boundary the exec sandbox computes for
    /// the run root: read + write confined to the project tree (NO write
    /// outside it by default), network from the registry `network` field
    /// ([`NetPolicy::Deny`] unless the operator opted in — fail-closed).
    #[must_use]
    pub fn sandbox_spec(&self, project_dir: &Path) -> SandboxSpec {
        let mut spec = SandboxSpec::new();
        let root = anchor(project_dir);
        spec.fs_read = vec![format!("{root}/**")];
        spec.fs_write = vec![format!("{root}/**")];
        spec.net = self.network.clone();
        spec
    }
}

/// Anchor the boundary at an ABSOLUTE, canonical project path — the sandbox
/// launchers speak absolute subpaths only (their grant validation is
/// fail-closed on a relative grant). Canonicalization resolves the macOS
/// `/var` → `/private/var` aliasing so the grant matches the profile
/// preamble's view of scratch dirs; a not-yet-existing dir falls back to the
/// lexical absolutize (the same semantics the runtime's permits derivation
/// pins), and a relative dir with a vanished cwd degrades to the relative
/// form the launcher REFUSES — never a widened grant.
fn anchor(dir: &Path) -> String {
    let abs = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(dir),
            Err(_) => return lexically_normalize(dir),
        }
    };
    let canonical = std::fs::canonicalize(&abs).unwrap_or(abs); // seam-bypass-ok: L4 spawn-site path math — canonicalizing the sandbox anchor (the load_server_configs precedent)
    lexically_normalize(&canonical)
}

/// Textual `.`/`..` fold (the `nika-cap` `fit::lexically_normalize`
/// semantics, restated here so this crate stays a leaf consumer — the same
/// restatement the runtime's derivation carries: no symlink walk, the jail's
/// own canonicalization owns the existing part).
fn lexically_normalize(path: &Path) -> String {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_methods
)] // disallowed_methods: tests read HOME/vars to build probe paths — no SecretStore in a test
mod tests {
    use nika_kernel::process::{DANGEROUS_ENV_VARS, RUNNER_FLOOR_ENV_VARS as PASSTHROUGH_ENV_VARS};
    use std::path::PathBuf;
    use std::sync::Mutex;

    use nika_kernel::command_sandbox::CommandSandboxError;
    use nika_kernel::process::{EgressAllowlist, ShellCommand};

    use super::*;
    use crate::client::{StdioMcpClient, ToolsListDyn};
    use crate::pin::PinError;

    /// The shell's own bookkeeping vars — set by `sh` itself at startup,
    /// never inherited (carved out of the scrub assertion).
    const SH_OWN: &[&str] = &["PWD", "SHLVL", "_", "OLDPWD"];

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-mcp-sbx-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp dir");
        dir
    }

    /// The mock MCP server: dumps its env + argv (the scrub/confinement
    /// witnesses), then speaks the handshake + one `tools/list` reply.
    const MOCK_SCRIPT: &str = r#"#!/bin/sh
# $1 = the dump dir (inside the project boundary).
printenv > "$1/env.txt"
printf '%s\n' "$@" > "$1/argv.txt"
read -r _init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"mock","version":"0.0.0"}}}'
read -r _notif
read -r _list
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"ping","description":"Ping","inputSchema":{"type":"object"}}]}}'
"#;

    /// Write the mock server into `dir` and return the config spawning it.
    fn mock_server(dir: &Path) -> McpServerConfig {
        let script = dir.join("mock-mcp.sh");
        std::fs::write(&script, MOCK_SCRIPT).expect("write mock server");
        McpServerConfig::stdio(
            "mock",
            "/bin/sh",
            vec![
                script.to_string_lossy().into_owned(),
                dir.to_string_lossy().into_owned(),
            ],
        )
    }

    /// The confinement seam double: records the spec it was asked to enforce
    /// and marks the confined argv — proving WHICH command the spawn ran.
    #[derive(Default)]
    struct RecordingSandbox {
        spec: Mutex<Option<SandboxSpec>>,
    }

    impl CommandSandbox for RecordingSandbox {
        fn confine(
            &self,
            spec: &SandboxSpec,
            mut command: ShellCommand,
        ) -> Result<ShellCommand, CommandSandboxError> {
            *self.spec.lock().expect("lock") = Some(spec.clone());
            // Trailing position: a leading marker would be parsed as an
            // option by an interpreter command (`sh --marker …`).
            command.args.push("--sbx-confined".to_owned());
            Ok(command)
        }

        fn backend(&self) -> &'static str {
            "recording"
        }
    }

    /// A backend that always refuses — the fail-closed witness.
    struct RefusingSandbox;

    impl CommandSandbox for RefusingSandbox {
        fn confine(
            &self,
            _spec: &SandboxSpec,
            _command: ShellCommand,
        ) -> Result<ShellCommand, CommandSandboxError> {
            Err(CommandSandboxError::Unavailable {
                reason: "test: no mechanism on this host".to_owned(),
            })
        }

        fn backend(&self) -> &'static str {
            "refusing"
        }
    }

    /// The derivation matrix (the mission's spec-derivation test): deny is
    /// the default, `allow` is the opt-in, the allowlist arm carries the
    /// declared set verbatim with the proxy port RESERVED — and every arm
    /// anchors read + write at the project boundary.
    #[test]
    fn the_spec_derivation_matrix_per_server_config() {
        let root = Path::new("/data/proj");
        let cfg = McpServerConfig::stdio("a", "x", Vec::new());
        let spec = cfg.sandbox_spec(root);
        assert_eq!(spec.net, NetPolicy::Deny, "no network field = fail-closed");
        assert_eq!(spec.fs_read, vec!["/data/proj/**".to_owned()]);
        assert_eq!(spec.fs_write, vec!["/data/proj/**".to_owned()]);

        let cfg = cfg.with_network(NetPolicy::Allow);
        assert_eq!(cfg.sandbox_spec(root).net, NetPolicy::Allow, "the opt-in");

        let hosts = vec!["api.example.com".to_owned(), "*.github.com".to_owned()];
        let cfg = cfg.with_network(NetPolicy::Allowlist(EgressAllowlist::new(hosts.clone())));
        let spec = cfg.sandbox_spec(root);
        assert_eq!(
            spec.net,
            NetPolicy::Allowlist(EgressAllowlist::new(hosts)),
            "the declared set, verbatim"
        );
        let NetPolicy::Allowlist(list) = &spec.net else {
            panic!("allowlist arm");
        };
        assert!(
            list.proxy_port.is_none(),
            "the proxy port stays RESERVED until the egress proxy lands"
        );
    }

    #[test]
    fn a_relative_project_dir_anchors_at_the_cwd() {
        let cwd = std::env::current_dir().expect("cwd");
        let spec = McpServerConfig::stdio("a", "x", Vec::new()).sandbox_spec(Path::new("rel/proj"));
        let expected = lexically_normalize(&cwd.join("rel/proj"));
        assert_eq!(spec.fs_read, vec![format!("{expected}/**")]);
        assert_eq!(spec.fs_write, vec![format!("{expected}/**")]);
    }

    /// The spawn goes through the seam with the DERIVED boundary, and the
    /// CONFINED command (not the original) is what actually runs.
    #[cfg(unix)]
    #[test]
    fn the_spawn_rides_the_confined_command_with_the_derived_spec() {
        let dir = tmp("seam");
        let cfg = mock_server(&dir);
        let rec = Arc::new(RecordingSandbox::default());
        let client = StdioMcpClient::new(&cfg)
            .with_project_dir(dir.clone())
            .with_sandbox(rec.clone());
        let tools = client.tools_list().expect("the mock server answers");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "ping");

        let spec = rec
            .spec
            .lock()
            .expect("lock")
            .clone()
            .expect("confine ran before the spawn");
        let root = anchor(&dir);
        assert_eq!(spec.fs_read, vec![format!("{root}/**")]);
        assert_eq!(spec.fs_write, vec![format!("{root}/**")]);
        assert_eq!(spec.net, NetPolicy::Deny, "default server = net deny");

        let argv = std::fs::read_to_string(dir.join("argv.txt")).expect("argv dump");
        assert!(
            argv.contains("--sbx-confined"),
            "the CONFINED argv reached the server (the seam's output spawned, not the raw command): {argv}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Env hygiene (the mission's scrub test): the child inherits ONLY the
    /// curated passthrough — the dangerous-env floor and every ambient
    /// non-curated value (the provider-key class) stay behind. `set_var` is
    /// the forbidden unsafe, so the DANGEROUS entries are proven at the
    /// predicate below and the ambient-leak class is proven here against a
    /// real non-curated parent var.
    #[cfg(unix)]
    #[test]
    fn the_child_env_is_scrubbed_to_the_curated_passthrough() {
        let dir = tmp("scrub");
        let cfg = mock_server(&dir);
        let client = StdioMcpClient::new(&cfg)
            .with_project_dir(dir.clone())
            .with_sandbox(Arc::new(RecordingSandbox::default()));
        client.tools_list().expect("the mock server answers");

        let dump = std::fs::read_to_string(dir.join("env.txt")).expect("env dump");
        assert!(dump.contains("PATH="), "the curated PATH passes: {dump}");
        for line in dump.lines() {
            let key = line.split('=').next().unwrap_or("");
            assert!(
                PASSTHROUGH_ENV_VARS.contains(&key) || SH_OWN.contains(&key),
                "non-curated var `{key}` reached the child: {dump}"
            );
        }
        // A parent var OUTSIDE the curated set (the ambient-secret class)
        // does NOT reach the spawned server.
        let foreign = std::env::vars()
            .map(|(k, _)| k)
            .find(|k| !PASSTHROUGH_ENV_VARS.contains(&k.as_str()) && !SH_OWN.contains(&k.as_str()));
        if let Some(k) = foreign {
            assert!(
                !dump.contains(&format!("{k}=")),
                "ambient `{k}` leaked into the child: {dump}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_dangerous_floor_and_the_secret_class_never_pass() {
        // Asked of the composition PRODUCTION runs, not of a predicate
        // beside it. `apply_env_scrub` calls `compose_child_env` with no
        // grants and no authored map, so this map IS what a spawned
        // server inherits — the crate's own copy of the law was retired
        // the day an adversarial pass noticed the doc promised one
        // function and the code had two (2026-08-02).
        let ambient = |name: &str| Some(format!("ambient-{name}"));
        let composed = nika_kernel::process::compose_child_env(
            ambient,
            &[],
            &std::collections::BTreeMap::new(),
        );
        for name in DANGEROUS_ENV_VARS {
            assert!(
                !composed.contains_key(*name),
                "{name} must never reach a server"
            );
        }
        for provider_key in [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "AWS_SECRET_ACCESS_KEY",
            "GITHUB_TOKEN",
        ] {
            assert!(
                !composed.contains_key(provider_key),
                "{provider_key} is not curated"
            );
        }
        for ok in ["PATH", "HOME", "TMPDIR"] {
            assert!(composed.contains_key(ok), "{ok} is curated");
        }
        // The two lists are structurally disjoint — a curated name can never
        // be an injection vector.
        for name in PASSTHROUGH_ENV_VARS {
            assert!(
                !DANGEROUS_ENV_VARS.contains(name),
                "{name} sits on both lists"
            );
        }
    }

    /// The mission's fail-closed test: a sandbox refusal is the honest
    /// NIKA-MCP-005 — and NO process ever starts (no silent unconfined
    /// fallback).
    #[cfg(unix)]
    #[test]
    fn a_sandbox_refusal_is_nika_mcp_005_and_no_process_starts() {
        let dir = tmp("fail-closed");
        let cfg = mock_server(&dir);
        let client = StdioMcpClient::new(&cfg)
            .with_project_dir(dir.clone())
            .with_sandbox(Arc::new(RefusingSandbox));
        let err = client
            .tools_list()
            .expect_err("a confine refusal is terminal");
        assert!(matches!(err, PinError::Sandbox { .. }), "{err}");
        assert_eq!(err.code(), Some("NIKA-MCP-005"));
        let text = format!("{err}");
        assert!(text.contains("NOT started"), "{text}");
        assert!(
            !dir.join("env.txt").exists() && !dir.join("argv.txt").exists(),
            "the server was NEVER started"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The macOS smoke (the mission's seatbelt test): a REAL spawn through
    /// `sandbox-exec` — the profile wraps the child, proven by a positive
    /// denial (a write OUTSIDE the project boundary, in `$HOME`, is refused)
    /// alongside the working in-boundary protocol exchange.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_real_seatbelt_spawn_wraps_the_child_in_the_profile() {
        if !nika_sandbox_seatbelt::SeatbeltSandbox::available() {
            return; // no launcher on this host — the seam tests carry the logic
        }
        let dir = tmp("seatbelt");
        let home = std::env::var("HOME").expect("HOME set");
        let probe = format!("{home}/.nika-mcp-sbx-probe-{}", std::process::id());
        let script = dir.join("mock-mcp-jail.sh");
        std::fs::write(
            &script,
            r#"#!/bin/sh
# $1 = dump dir (in-boundary) · $2 = an OUT-of-boundary write target.
printenv > "$1/env.txt"
if ( echo x > "$2" ) 2>/dev/null; then
  printf '%s\n' "WRITE-ALLOWED" > "$1/probe.txt"
  rm -f "$2"
else
  printf '%s\n' "WRITE-DENIED" > "$1/probe.txt"
fi
read -r _init
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"mock","version":"0.0.0"}}}'
read -r _notif
read -r _list
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"ping","description":"Ping","inputSchema":{"type":"object"}}]}}'
"#,
        )
        .expect("write jail mock");
        let cfg = McpServerConfig::stdio(
            "mock",
            "/bin/sh",
            vec![
                script.to_string_lossy().into_owned(),
                dir.to_string_lossy().into_owned(),
                probe.clone(),
            ],
        );
        // `new` selects the real Seatbelt backend on this host.
        let client = StdioMcpClient::new(&cfg).with_project_dir(dir.clone());
        assert!(
            client.sandbox_note().starts_with("sandboxed (seatbelt"),
            "{}",
            client.sandbox_note()
        );
        let tools = client
            .tools_list()
            .expect("the confined server still answers");
        assert_eq!(tools.len(), 1);

        let report = std::fs::read_to_string(dir.join("probe.txt")).expect("probe report");
        assert_eq!(
            report.trim(),
            "WRITE-DENIED",
            "the profile wraps the child: a $HOME write is refused"
        );
        assert!(
            !Path::new(&probe).exists(),
            "no probe file escaped into $HOME"
        );
        let dump = std::fs::read_to_string(dir.join("env.txt")).expect("env dump");
        assert!(
            dump.contains("PATH=") && !dump.contains("DYLD_INSERT_LIBRARIES="),
            "scrubbed even inside the jail: {dump}"
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&probe); // belt: if the jail broke, clean up
    }

    #[test]
    fn the_note_names_the_backend_and_the_net_arm() {
        assert_eq!(
            sandbox_note("seatbelt", &NetPolicy::Deny),
            "sandboxed (seatbelt · net deny)"
        );
        assert_eq!(
            sandbox_note("landlock", &NetPolicy::Allow),
            "sandboxed (landlock · net allow)"
        );
        let allow = sandbox_note(
            "seatbelt",
            &NetPolicy::Allowlist(EgressAllowlist::new(vec![
                "a.example".to_owned(),
                "b.example".to_owned(),
            ])),
        );
        assert!(
            allow.contains("allowlist")
                && allow.contains("2 host(s)")
                && allow.contains("confined as allow"),
            "the transitional mapping is named loudly: {allow}"
        );
        let loud = sandbox_note("noop", &NetPolicy::Deny);
        assert!(
            loud.starts_with("UNSANDBOXED"),
            "the deliberate unconfined choice screams: {loud}"
        );
    }

    #[test]
    fn the_platform_selector_matches_the_host() {
        let expected = if cfg!(target_os = "macos")
            && nika_sandbox_seatbelt::SeatbeltSandbox::available()
        {
            "seatbelt"
        } else if cfg!(target_os = "linux") && nika_sandbox_landlock::LandlockSandbox::available() {
            "landlock"
        } else {
            "noop"
        };
        assert_eq!(platform_sandbox().backend(), expected);
    }
}
