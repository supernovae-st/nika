// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE client-detection table `probe` and `wire` share (ADR-110
//! split-residue cleanup · 2026-07-31). Before this module, `probe`'s
//! `client_probes()` and `wire`'s `detected_targets_in()` each carried
//! their own per-client path list — and only probe's was gated on the
//! vendored matrix, so a client dropped from the matrix stayed
//! wire-visible but doctor-invisible (two truths). Now both derive from
//! [`sights`]: one table, one registry gate, one stale predicate, one
//! Hermes recognition predicate.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::clients_registry;

/// How a client's config is read: JSON (the `nika` server entry lives at
/// `server_path`) or YAML (Hermes — no parser round-trips user YAML, the
/// [`hermes_recognized`] substring predicate carries recognition).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigKind {
    Json,
    Yaml,
}

/// One client the detection engine knows how to SEE: its id, the config
/// paths this machine may show (home-anchored first, then
/// workspace-anchored), and the JSON path of the `nika` server entry.
#[derive(Debug, Clone)]
pub(crate) struct ClientSight {
    /// The registry / probe id (`cursor` · `claude` · `hermes` …).
    pub(crate) id: &'static str,
    /// The config file's format — the probe path differs per kind.
    pub(crate) kind: ConfigKind,
    /// Every path the config may live at, most-wired first.
    pub(crate) paths: Vec<PathBuf>,
    /// The JSON path of the `nika` server entry (`["mcpServers", "nika"]`
    /// for the cursor-like family).
    pub(crate) server_path: [&'static str; 2],
}

/// The static half of one table row: home-relative and
/// workspace-relative path segments (empty = never anchored there).
struct SightSpec {
    id: &'static str,
    kind: ConfigKind,
    home: &'static [&'static [&'static str]],
    work: &'static [&'static [&'static str]],
    server_path: [&'static str; 2],
}

/// The per-client table, in the historical probe order (findings and
/// receipts ride it — the order is load-bearing, and
/// [`clients_registry::PROBE_MECHANISMS`] mirrors it).
const SIGHT_SPECS: &[SightSpec] = &[
    SightSpec {
        id: "cursor",
        kind: ConfigKind::Json,
        home: &[&[".cursor", "mcp.json"]],
        work: &[&[".cursor", "mcp.json"]],
        server_path: ["mcpServers", "nika"],
    },
    SightSpec {
        id: "windsurf",
        kind: ConfigKind::Json,
        home: &[&[".codeium", "windsurf", "mcp_config.json"]],
        work: &[],
        server_path: ["mcpServers", "nika"],
    },
    SightSpec {
        id: "claude",
        kind: ConfigKind::Json,
        home: &[&[".claude.json"]],
        work: &[],
        server_path: ["mcpServers", "nika"],
    },
    // Zed: ~/.config on EVERY platform (upstream choice) · the MCP entry
    // lives under `context_servers` (zed.dev/docs/ai/mcp). The JSON
    // probe is best-effort on Zed's JSONC settings: a commented file
    // parses as not-wired → the fix line says `nika wire zed`, which
    // itself degrades to the ✋ manual snippet. Honest chain.
    SightSpec {
        id: "zed",
        kind: ConfigKind::Json,
        home: &[&[".config", "zed", "settings.json"]],
        work: &[],
        server_path: ["context_servers", "nika"],
    },
    // Hermes: YAML, the JSON probe cannot see it (H2) — recognition is
    // the `hermes_recognized` substring predicate, never a parse.
    SightSpec {
        id: "hermes",
        kind: ConfigKind::Yaml,
        home: &[&[".hermes", "config.yaml"]],
        work: &[],
        server_path: ["mcp_servers", "nika"],
    },
    SightSpec {
        id: "vscode",
        kind: ConfigKind::Json,
        home: &[],
        work: &[&[".vscode", "mcp.json"]],
        server_path: ["servers", "nika"],
    },
];

/// Resolve the sight rows for THIS machine: every row gated on the
/// vendored matrix (a client is SEEN only while the matrix claims its
/// wire target — the H6 law `probe` and `wire` now share), home-anchored
/// paths first (dropped when `home` is unknown), workspace-anchored
/// paths resolved against `work`. A row with no resolvable path is no
/// sight at all.
pub(crate) fn sights(home: Option<&Path>, work: &Path) -> Vec<ClientSight> {
    let join = |base: &Path, segs: &[&str]| segs.iter().fold(base.to_path_buf(), |p, s| p.join(s));
    SIGHT_SPECS
        .iter()
        .filter(|spec| clients_registry::registry_wires(spec.id))
        .filter_map(|spec| {
            let mut paths: Vec<PathBuf> = home
                .map(|base| spec.home.iter().map(|segs| join(base, segs)).collect())
                .unwrap_or_default();
            paths.extend(spec.work.iter().map(|segs| join(work, segs)));
            (!paths.is_empty()).then_some(ClientSight {
                id: spec.id,
                kind: spec.kind,
                paths,
                server_path: spec.server_path,
            })
        })
        .collect()
}

/// The stale argv form (`mcp serve --stdio` — the pre-current wire) —
/// THE one stale predicate: `probe` flags the drift, `wire` migrates it.
pub(crate) fn is_stale_mcp_server(value: &Value) -> bool {
    let Some(args) = value.get("args").and_then(Value::as_array) else {
        return false;
    };
    args.len() == 3
        && args[0].as_str() == Some("mcp")
        && args[1].as_str() == Some("serve")
        && args[2].as_str() == Some("--stdio")
}

/// The Hermes recognition predicate — a `nika:` server whose command
/// line names the binary. Substring match BY DESIGN: user YAML carries
/// comments and anchors no parser round-trips; presence-only, never a
/// guess. `probe` reads it for detection, `wire` for the Current arm.
pub(crate) fn hermes_recognized(body: &str) -> bool {
    body.contains("nika:") && body.contains("command: nika")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table mirrors the registry's probe mechanisms exactly — one
    /// list, one order (findings and receipts ride probe order).
    #[test]
    fn the_table_mirrors_the_registry_probe_mechanisms() {
        let ids: Vec<&str> = SIGHT_SPECS.iter().map(|spec| spec.id).collect();
        assert_eq!(
            ids.as_slice(),
            clients_registry::PROBE_MECHANISMS,
            "the detect table drifted from the registry's probe mechanisms"
        );
    }

    /// Every sight row answers a wire target the matrix claims today
    /// (the degrade-open law keeps the rows when the vendored snapshot
    /// cannot parse — never a silent coverage loss).
    #[test]
    fn every_row_is_matrix_claimed() {
        for spec in SIGHT_SPECS {
            assert!(
                clients_registry::registry_wires(spec.id),
                "{} has no matrix wire row",
                spec.id
            );
        }
    }

    /// A row whose paths all need a HOME the machine does not have is no
    /// sight at all — the workspace-anchored rows survive.
    #[test]
    fn no_home_keeps_only_worktree_anchored_rows() {
        let work = Path::new("/tmp/nika-detect-test-work");
        let rows = sights(None, work);
        let ids: Vec<&str> = rows.iter().map(|row| row.id).collect();
        assert_eq!(ids, ["cursor", "vscode"], "got {ids:?}");
        for row in &rows {
            assert!(
                row.paths.iter().all(|p| p.starts_with(work)),
                "{} anchored outside the workspace: {:?}",
                row.id,
                row.paths
            );
        }
    }

    /// The stale predicate pins the exact `mcp serve --stdio` argv and
    /// nothing else.
    #[test]
    fn stale_predicate_matches_only_the_stale_argv() {
        let stale = serde_json::json!({"command": "nika", "args": ["mcp", "serve", "--stdio"]});
        let current = serde_json::json!({"command": "nika", "args": ["mcp"]});
        assert!(is_stale_mcp_server(&stale));
        assert!(!is_stale_mcp_server(&current));
        assert!(!is_stale_mcp_server(&serde_json::json!({})));
        assert!(!is_stale_mcp_server(
            &serde_json::json!({"args": ["mcp", "serve"]})
        ));
    }

    /// The Hermes predicate needs both halves — a `nika:` server AND the
    /// binary's command line (presence-only, never a guess).
    #[test]
    fn hermes_predicate_needs_both_halves() {
        assert!(hermes_recognized(
            "mcp_servers:\n  nika:\n    command: nika\n    args: [mcp]\n"
        ));
        assert!(!hermes_recognized(
            "mcp_servers:\n  other:\n    command: nika\n"
        ));
        assert!(!hermes_recognized(
            "mcp_servers:\n  nika:\n    command: other\n"
        ));
    }
}
