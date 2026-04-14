// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

// The body of this module is produced by `build.rs` — it contains a
// `phf::Map` literal whose internal hash-key is necessarily a 20-digit u64.
// Workspace clippy rules (readable literals, pedantic) can't apply to code
// we do not own, so they are relaxed here. Anything authored by hand stays
// under the normal workspace lints.
#![allow(clippy::unreadable_literal, clippy::too_many_lines)]

//! Build-time generated catalogs.
//!
//! The body of this module is produced by `build.rs` from `data/*.toml`
//! and materialised at `$OUT_DIR/mcp_servers.rs`. Phase C step 3
//! establishes the infrastructure; Steps 4 + 5 fill in the full data.
//!
//! Consumers should prefer the shape exposed here over the legacy
//! `ALL_MCP_ALIASES` static. During Phase C transition both coexist.

// `UniCase::ascii(...)` expressions appear in the phf_codegen output.
use unicase::UniCase;

include!(concat!(env!("OUT_DIR"), "/mcp_servers.rs"));

/// Case-insensitive lookup by id or alias. Returns `None` when unknown.
#[must_use]
pub fn find_mcp_server(name: &str) -> Option<&'static crate::types::McpServer> {
    let idx = *MCP_INDEX.get(&UniCase::ascii(name))?;
    ALL_MCP_SERVERS.get(idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_servers_non_empty() {
        assert!(!ALL_MCP_SERVERS.is_empty());
    }

    #[test]
    fn seed_contains_filesystem() {
        let s = find_mcp_server("filesystem").expect("filesystem seeded");
        assert_eq!(s.id, "filesystem");
        assert_eq!(s.category, crate::types::Category::Anthropic);
        assert_eq!(s.packages.len(), 1);
        assert_eq!(s.packages[0].registry_type, crate::types::RegistryType::Npm);
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert!(find_mcp_server("FILESYSTEM").is_some());
        assert!(find_mcp_server("FileSystem").is_some());
        assert!(find_mcp_server("filesystem").is_some());
    }

    #[test]
    fn unknown_server_returns_none() {
        assert!(find_mcp_server("does-not-exist").is_none());
    }

    #[test]
    fn pypi_server_has_runner() {
        let s = find_mcp_server("qdrant").expect("qdrant seeded");
        assert_eq!(s.packages[0].registry_type, crate::types::RegistryType::Pypi);
        assert_eq!(s.packages[0].runner, Some(crate::types::PyRunner::Uvx));
    }

    #[test]
    fn remote_server_has_no_packages() {
        let s = find_mcp_server("intercom").expect("intercom seeded");
        assert!(s.packages.is_empty());
        assert_eq!(s.remotes.len(), 1);
        assert_eq!(s.remotes[0].auth, crate::types::AuthMode::OAuth);
    }

    #[test]
    fn every_server_has_at_least_one_install_path() {
        for s in ALL_MCP_SERVERS {
            assert!(
                !s.packages.is_empty() || !s.remotes.is_empty(),
                "server {:?} has no packages AND no remotes",
                s.id,
            );
        }
    }

    #[test]
    fn every_server_id_roundtrips_through_index() {
        for s in ALL_MCP_SERVERS {
            assert_eq!(
                find_mcp_server(s.id).map(|x| x.id),
                Some(s.id),
                "server {:?} is not resolvable via MCP_INDEX",
                s.id,
            );
        }
    }
}
