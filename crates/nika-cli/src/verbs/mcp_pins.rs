// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika mcp approve <server>` — the operator surface of the MCP
//! tool-pinning defence: after a drift refusal (or a deliberate server
//! upgrade), re-pin the server's CURRENT tool definitions and print the
//! new pin set as the receipt. The server resolves from
//! `.nika/mcp_servers.json` against the workspace CWD (the `.nika/`
//! convention); every decision is the library's ([`nika_mcp::client`] ·
//! [`nika_mcp::pin`]) — this module maps the outcome to text + the spec §4
//! exit codes. The connect flow (`connect_verified`: TOFU enroll · quiet
//! match · fail-closed drift diff) is library-side in `nika-mcp`, awaiting
//! the runtime MCP dispatch; this verb drives the same seam.

/// Operator-facing honesty card for `nika mcp --help` (C02 · issue 1303).
/// The server is a read-only authoring oracle; running a workflow is
/// `nika run`. Tool names here must stay ⊆ the nine the oracle serves.
pub const OPERATOR_HELP: &str = "\
This MCP server is a read-only authoring oracle.
Tools: nika_check · nika_inspect · nika_explain · nika_schema · nika_examples · nika_template · nika_canon · nika_catalog · nika_tools
It never writes a file and never runs a workflow. To execute: nika run.";

/// The MCP subcommand surface — `approve` (the tool-pinning
/// remediation) or serve (stdio · streamable HTTP). Descended from the
/// bin's dispatcher 2026-07-21 (the 1500-line file cap — the bin
/// composes, the verbs own their routing).
#[derive(Clone, clap::Subcommand)]
pub enum McpAction {
    /// Re-pin the server's CURRENT tool definitions after human review
    /// (the remediation a drift refusal names), printing the new pin set.
    Approve {
        /// The server name from `.nika/mcp_servers.json`.
        server: String,
    },
}

/// The MCP wire (the clap arm of `--transport`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum McpTransportArg {
    /// Newline-delimited JSON-RPC over stdin/stdout.
    Stdio,
    /// Streamable HTTP (POST JSON-RPC · origin-gated · loopback default).
    Http,
}

/// The `nika mcp` routing: the client subcommand dispatches to the pins
/// layer; the server surfaces bind + serve forever (the composition the
/// bin handed down). The banner + errors ride stderr verbatim (the
/// `resolve_trace` precedent — no re-prefixing).
///
/// # Errors
///
/// `verbs::exit::OK` on a clean serve; `1` when the server itself fails
/// (the MCP surface's own failure voice, not the spec §4 taxonomy).
#[allow(clippy::disallowed_macros, clippy::print_stderr)]
#[must_use]
pub fn mcp_verb(
    action: Option<McpAction>,
    transport: McpTransportArg,
    port: u16,
    bind: &str,
) -> u8 {
    match action {
        Some(McpAction::Approve { server }) => return emit(&approve(&server)),
        None => {}
    }
    let served = match transport {
        McpTransportArg::Stdio => nika_mcp::run_stdio(),
        McpTransportArg::Http => match nika_mcp::HttpServer::bind(bind, port) {
            Ok(server) => {
                // The sanctioned env boundary (same seam as config_from_env):
                // the token is operator config crossing into a server hold.
                #[allow(clippy::disallowed_methods)]
                let token = std::env::var("NIKA_MCP_TOKEN").ok();
                // #890: a public bind with no bearer refuses to START — the
                // refusal names both fixes (set the token · bind loopback).
                if let Err(err) = server.guard_bind_auth(token.as_deref()) {
                    eprintln!("nika mcp: {err}");
                    return 1;
                }
                let addr = server
                    .addr()
                    .map_or_else(|_| format!("{bind}:{port}"), |a| a.to_string());
                eprintln!(
                    "nika mcp · http://{addr}/mcp · POST JSON-RPC (MCP 2025-11-25) · origin-gated · {} · production TLS belongs to a reverse proxy",
                    if token.is_some() {
                        "bearer auth ON (NIKA_MCP_TOKEN)"
                    } else {
                        "no auth (set NIKA_MCP_TOKEN to require a bearer)"
                    }
                );
                server.serve(token.as_deref())
            }
            Err(err) => Err(err),
        },
    };
    match served {
        Ok(()) => crate::verbs::exit::OK,
        Err(err) => {
            eprintln!("nika mcp: {err}");
            1
        }
    }
}

/// Print a verb's text on the right stream and return its exit code
/// (the bin's `emit` — findings and successes go to stdout, only
/// environment errors to stderr).
// The MCP surface owns its streams (the resolve_trace precedent).
#[allow(clippy::disallowed_macros, clippy::print_stdout, clippy::print_stderr)]
fn emit(out: &VerbOutput) -> u8 {
    if out.code == crate::verbs::exit::ENV {
        eprintln!("nika: {}", out.text);
    } else if !out.text.is_empty() {
        println!("{}", out.text.trim_end());
    }
    out.code
}
use std::path::Path;

use nika_mcp::client::{
    McpServerConfig, StdioMcpClient, ToolsListDyn, approve_server, load_server_configs,
};
use nika_mcp::pin::{PINS_PATH, PinError};

use super::VerbOutput;

/// `nika mcp approve <server>` — re-pin after human review, production client.
#[must_use]
pub fn approve(server: &str) -> VerbOutput {
    let dir = Path::new(".");
    match resolve_config(server, dir) {
        Ok(config) => approve_with(&config, &StdioMcpClient::new(&config), dir, now_epoch()),
        Err(out) => out,
    }
}

/// The injected-seam core of [`approve`] — generic over the client so
/// tests drive it with a mock (no subprocess · no network).
pub(crate) fn approve_with<C: ToolsListDyn + ?Sized>(
    config: &McpServerConfig,
    client: &C,
    dir: &Path,
    now: u64,
) -> VerbOutput {
    match approve_server(config, client, dir, now) {
        Ok(report) => {
            use std::fmt::Write as _;
            let mut text = format!(
                "approved {} tool(s) from MCP server `{}` — the lockfile now pins the CURRENT definitions\n  \
                 lockfile: {PINS_PATH} · pinned_at {}",
                report.pins.len(),
                report.server,
                report.pinned_at
            );
            for (name, pin) in &report.pins {
                let _ = write!(text, "\n  {name}  {pin}");
            }
            if report.pins.is_empty() {
                text.push_str("\n  (the server exposes no tools — nothing to pin)");
            }
            VerbOutput::ok(text)
        }
        Err(err) => pin_error_output(&err),
    }
}

/// Resolve one configured server — the teaching error when the registry
/// does not know the name (NIKA-MCP-001 class · exit 3).
fn resolve_config(server: &str, dir: &Path) -> Result<McpServerConfig, VerbOutput> {
    let configs = load_server_configs(dir).map_err(|e| pin_error_output(&e))?;
    configs
        .into_iter()
        .find(|c| c.name == server)
        .ok_or_else(|| {
            VerbOutput::env(format!(
                "MCP server `{server}` is not configured — declare it in .nika/mcp_servers.json first\n  \
                 form: {{\"mcp_servers_format\": 1, \"servers\": {{\"{server}\": {{\"command\": \"…\", \"args\": […]}}}}}}"
            ))
        })
}

/// Pin failures are environment-class (spec §4 · exit 3); the text is the
/// error's own Display — one voice, authored in `nika_mcp::pin`.
fn pin_error_output(err: &PinError) -> VerbOutput {
    VerbOutput::env(format!("{err}"))
}

/// Wall-clock seconds for `pinned_at`, read at the L4 boundary (INV-027).
fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::path::PathBuf;

    use nika_mcp::pin::McpToolDef;
    use serde_json::json;

    use super::*;
    use crate::verbs::exit;

    struct Mock {
        tools: Vec<McpToolDef>,
    }

    impl ToolsListDyn for Mock {
        fn tools_list(&self) -> Result<Vec<McpToolDef>, PinError> {
            Ok(self.tools.clone())
        }
    }

    fn config() -> McpServerConfig {
        McpServerConfig::stdio("postgres", "honest-srv", Vec::new())
    }

    fn tools() -> Vec<McpToolDef> {
        vec![McpToolDef::new(
            "query",
            "Run a SQL query",
            json!({"type": "object", "properties": {"sql": {"type": "string"}}}),
        )]
    }

    fn tmp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nika-cli-mcppin-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp dir");
        dir
    }

    #[test]
    fn approve_repins_and_prints_the_pin_set() {
        let dir = tmp("approve");
        approve_with(&config(), &Mock { tools: tools() }, &dir, 1);
        let upgraded = Mock {
            tools: vec![
                McpToolDef::new(
                    "query",
                    "Run a SQL query — v2",
                    json!({"type": "object", "properties": {"sql": {"type": "string"}}}),
                ),
                McpToolDef::new("explain", "Plan a query", json!({})),
            ],
        };
        let out = approve_with(&config(), &upgraded, &dir, 1_700_000_000);
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("approved 2 tool(s)"), "{}", out.text);
        assert!(out.text.contains("explain  blake3:"), "{}", out.text);
        assert!(out.text.contains("query  blake3:"), "{}", out.text);
        // The re-pin is real: the lockfile now matches the NEW surface.
        let text = std::fs::read_to_string(dir.join(PINS_PATH)).expect("lockfile");
        assert!(text.contains("Run a SQL query — v2"), "{text}");
        assert!(text.contains("2023-11-14T22:13:20Z"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// C02 · issue 1303 · the mcp verb's honesty card names the read-only
    /// oracle, at least three served tools, and `nika run` as the execute
    /// door — and does not advertise a write/run tool.
    #[test]
    fn operator_help_names_the_read_only_oracle_and_the_run_door() {
        assert!(OPERATOR_HELP.contains("read-only"), "C02: {OPERATOR_HELP}");
        let named = [
            "nika_check",
            "nika_inspect",
            "nika_explain",
            "nika_schema",
            "nika_examples",
            "nika_template",
            "nika_canon",
            "nika_catalog",
            "nika_tools",
        ]
        .iter()
        .filter(|n| OPERATOR_HELP.contains(*n))
        .count();
        assert!(
            named >= 3,
            "C02 must name at least 3 of the 9 tools, named {named}: {OPERATOR_HELP}"
        );
        assert!(OPERATOR_HELP.contains("nika run"), "C02: {OPERATOR_HELP}");
        for forbidden in ["nika_run", "nika_write", "nika_exec"] {
            assert!(
                !OPERATOR_HELP.contains(forbidden),
                "C02 WALL: help must not advertise `{forbidden}`: {OPERATOR_HELP}"
            );
        }
    }

    #[test]
    fn an_unconfigured_server_is_a_teaching_env_error() {
        let dir = tmp("unconfigured");
        let out = resolve_config("ghost", &dir).expect_err("unknown server");
        assert_eq!(out.code, exit::ENV, "{}", out.text);
        assert!(out.text.contains("not configured"), "{}", out.text);
        assert!(out.text.contains(".nika/mcp_servers.json"), "{}", out.text);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_zero_tool_server_says_nothing_to_pin() {
        let dir = tmp("zero");
        let out = approve_with(&config(), &Mock { tools: Vec::new() }, &dir, 1);
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("approved 0 tool(s)"), "{}", out.text);
        assert!(out.text.contains("nothing to pin"), "{}", out.text);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
