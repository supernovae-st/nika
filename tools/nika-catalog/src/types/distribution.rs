// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Multi-distribution model for MCP servers.
//!
//! Aligned with the official MCP Registry schema
//! ([registry.modelcontextprotocol.io](https://registry.modelcontextprotocol.io/v0/servers)
//! schema version 2025-12-11): a server exposes one or more local-install
//! `packages` (npm/pypi/oci/cargo/mcpb) and/or one or more `remotes` (remote
//! HTTP/SSE endpoints). The build-time invariant (enforced by `build.rs`) is
//! `packages.len() + remotes.len() >= 1` — either local install or remote URL.
//!
//! All types are `#[non_exhaustive]` so additive variants (new registries,
//! new transports, new auth modes) are not breaking changes.

/// Package registry that hosts a local-install MCP server.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RegistryType {
    /// npm (JavaScript/TypeScript) — `npx @scope/pkg`.
    Npm,
    /// `PyPI` (Python) — `uvx pkg` or `pipx run pkg`.
    Pypi,
    /// OCI container registry — `docker run ghcr.io/org/img`.
    Oci,
    /// crates.io (Rust) — `cargo install pkg`.
    Cargo,
    /// MCP bundle (`.mcpb` archive).
    Mcpb,
}

/// MCP transport protocol for a package or remote.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Transport {
    /// Standard I/O — the default for local packages.
    Stdio,
    /// Streamable HTTP (current MCP HTTP standard, SSE+POST duplex).
    StreamableHttp,
    /// Server-Sent Events only (one-way, pre-streamable-http transport).
    Sse,
}

/// Authentication mode for a remote MCP endpoint.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AuthMode {
    /// No authentication.
    None,
    /// Bearer token in `Authorization: Bearer <token>` header.
    Bearer,
    /// OAuth 2.0 authorization code flow (dynamic client registration).
    OAuth,
}

/// Python runner for `PyPI` packages.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PyRunner {
    /// `uvx` (Astral's `uv`-based runner).
    Uvx,
    /// `pipx` (the classic pypa runner).
    Pipx,
}

/// A local-install MCP server package.
///
/// Corresponds to a single entry in the registry's `packages[]` array.
/// `runner` is only meaningful when `registry_type == RegistryType::Pypi`
/// (build.rs enforces this invariant).
///
/// `Serialize` is implemented for diagnostics (`serde_json::to_string` on
/// the whole catalog, CLI `nika mcp show --json`). `Deserialize` is
/// intentionally omitted — the type is populated by `build.rs` from TOML,
/// not round-tripped at runtime (`&'static [T]` can't come from a
/// deserializer without leaking).
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct McpPackage {
    /// Which registry hosts this package (npm / pypi / oci / cargo / mcpb).
    pub registry_type: RegistryType,
    /// Registry-specific identifier.
    ///
    /// Examples: `"@modelcontextprotocol/server-filesystem"` (npm),
    /// `"mcp-server-qdrant"` (pypi), `"ghcr.io/grafana/mcp-grafana"` (oci).
    pub identifier: &'static str,
    /// Optional pinned version (`None` means "latest").
    pub version: Option<&'static str>,
    /// Transport protocol for this package.
    pub transport: Transport,
    /// Python runner (only set when `registry_type == Pypi`).
    pub runner: Option<PyRunner>,
}

/// A remote MCP endpoint (no local install).
///
/// Corresponds to a single entry in the registry's `remotes[]` array.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct McpRemote {
    /// Transport protocol (usually `StreamableHttp`, sometimes `Sse`).
    pub transport: Transport,
    /// Fully-qualified URL of the remote endpoint.
    pub url: &'static str,
    /// Authentication mode required by the endpoint.
    pub auth: AuthMode,
}

/// Declarative description of an environment variable consumed by an MCP server.
///
/// Lives at the server level (not inside [`McpPackage`]) because the same set
/// of env vars usually applies to every package/remote of a server.
///
/// `key_prefixes` is a slice so services with disjunctive prefix rules can
/// describe all of them (e.g. `OpenAI` accepts both `sk-proj-` and `sk-`). An
/// empty slice means "no prefix constraint".
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct EnvVarSpec {
    /// Variable name (e.g. `"QDRANT_URL"`).
    pub name: &'static str,
    /// Accepted key prefixes for format validation. Empty = no constraint.
    pub key_prefixes: &'static [&'static str],
    /// Whether the variable must be set for the server to work.
    pub required: bool,
    /// Whether the variable holds a secret (redacted in logs / diagnostics).
    pub is_secret: bool,
    /// Short human-readable description.
    pub description: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time proofs that these types are cheap, thread-safe statics.
    const _: () = {
        const fn assert_copy<T: Copy>() {}
        const fn assert_send_sync<T: Send + Sync>() {}
        assert_copy::<RegistryType>();
        assert_copy::<Transport>();
        assert_copy::<AuthMode>();
        assert_copy::<PyRunner>();
        assert_copy::<McpPackage>();
        assert_copy::<McpRemote>();
        assert_copy::<EnvVarSpec>();
        assert_send_sync::<McpPackage>();
        assert_send_sync::<McpRemote>();
    };

    #[test]
    fn package_construction() {
        let pkg = McpPackage {
            registry_type: RegistryType::Npm,
            identifier: "@modelcontextprotocol/server-filesystem",
            version: None,
            transport: Transport::Stdio,
            runner: None,
        };
        assert_eq!(pkg.registry_type, RegistryType::Npm);
        assert!(pkg.version.is_none());
    }

    #[test]
    fn pypi_package_with_runner() {
        let pkg = McpPackage {
            registry_type: RegistryType::Pypi,
            identifier: "mcp-server-qdrant",
            version: None,
            transport: Transport::Stdio,
            runner: Some(PyRunner::Uvx),
        };
        assert_eq!(pkg.runner, Some(PyRunner::Uvx));
    }

    #[test]
    fn remote_with_oauth() {
        let remote = McpRemote {
            transport: Transport::StreamableHttp,
            url: "https://mcp.intercom.com/sse",
            auth: AuthMode::OAuth,
        };
        assert_eq!(remote.auth, AuthMode::OAuth);
    }

    #[test]
    fn env_var_spec_with_prefixes() {
        const PREFIXES: &[&str] = &["sk-ant-"];
        let spec = EnvVarSpec {
            name: "ANTHROPIC_API_KEY",
            key_prefixes: PREFIXES,
            required: true,
            is_secret: true,
            description: "Anthropic API key",
        };
        assert_eq!(spec.key_prefixes, &["sk-ant-"]);
        assert!(spec.required && spec.is_secret);
    }

    #[test]
    fn env_var_spec_with_disjunctive_prefixes() {
        // OpenAI: historically both sk-proj- (project keys) and sk- (user keys).
        const PREFIXES: &[&str] = &["sk-proj-", "sk-"];
        let spec = EnvVarSpec {
            name: "OPENAI_API_KEY",
            key_prefixes: PREFIXES,
            required: true,
            is_secret: true,
            description: "OpenAI API key",
        };
        assert_eq!(spec.key_prefixes.len(), 2);
    }

    #[test]
    fn env_var_spec_without_prefix_has_empty_slice() {
        const NONE: &[&str] = &[];
        let spec = EnvVarSpec {
            name: "PATH",
            key_prefixes: NONE,
            required: false,
            is_secret: false,
            description: "filesystem path",
        };
        assert!(spec.key_prefixes.is_empty());
    }
}
