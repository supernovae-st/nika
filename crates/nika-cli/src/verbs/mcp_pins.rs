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
