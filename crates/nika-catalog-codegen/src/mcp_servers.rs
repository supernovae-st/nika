// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP server catalog codegen — TOML schema, validation, Rust emission.
//!
//! Lifted from `nika-catalog/build.rs` Sessions 2a-3. Public TOML schema
//! types stay `pub` for re-use in tests and `nika-catalog-verify` ;
//! validation helpers stay `pub(crate)`.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use serde::Deserialize;

use crate::emit::{opt_plain, opt_rstr, rstr, str_slice_expr};
use crate::error::CodegenError;
use crate::schema::{MCP_SERVERS_SCHEMA, assert_schema};
use crate::tags::{tags_slice_expr, validate_tags};

// ─── TOML schema types (public for nika-catalog-verify) ─────────────────

/// Top-level MCP servers TOML file shape.
#[derive(Deserialize, Debug)]
#[non_exhaustive]
pub struct McpServersFile {
    pub schema: String,
    pub servers: Vec<McpServerEntry>,
}

/// A single MCP server entry. Field set matches `data/mcp-servers.toml`.
#[derive(Deserialize, Debug)]
#[non_exhaustive]
pub struct McpServerEntry {
    pub id: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub title: String,
    pub description: String,
    pub category: String,
    pub pricing: String,
    #[serde(default)]
    pub homepage: Option<String>,
    pub last_verified: String,
    #[serde(default)]
    pub packages: Vec<PackageEntry>,
    #[serde(default)]
    pub remotes: Vec<RemoteEntry>,
    #[serde(default)]
    pub env_vars: Vec<EnvVarEntry>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub extra_tags: Vec<String>,
    /// Per-tool effect hints (schema `@1.1`), keyed by tool name —
    /// the MCP `readOnlyHint` / `destructiveHint` manifest annotations.
    /// Empty = no manifest vendored. A `BTreeMap` so emission order is
    /// the sorted tool name, deterministically.
    #[serde(default)]
    pub effect_hints: BTreeMap<String, EffectHintEntry>,
}

/// Effect hints for one tool (schema `@1.1`). Each hint is `Option` —
/// a manifest that only annotates one of the two must not force an
/// invented value for the other. An entry with BOTH absent is refused
/// at validation: it claims nothing, and absence over empty claim.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct EffectHintEntry {
    /// MCP `readOnlyHint` — the tool claims to mutate nothing.
    #[serde(default)]
    pub read_only: Option<bool>,
    /// MCP `destructiveHint` — the tool may destroy or irreversibly
    /// change data.
    #[serde(default)]
    pub destructive: Option<bool>,
}

#[derive(Deserialize, Debug)]
#[non_exhaustive]
pub struct PackageEntry {
    pub registry_type: String,
    pub identifier: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default = "default_transport_stdio")]
    pub transport: String,
    #[serde(default)]
    pub runner: Option<String>,
}

#[derive(Deserialize, Debug)]
#[non_exhaustive]
pub struct RemoteEntry {
    pub transport: String,
    pub url: String,
    #[serde(default = "default_auth_none")]
    pub auth: String,
}

#[derive(Deserialize, Debug)]
#[non_exhaustive]
pub struct EnvVarEntry {
    pub name: String,
    #[serde(default)]
    pub key_prefixes: Vec<String>,
    pub required: bool,
    #[serde(default)]
    pub is_secret: bool,
    #[serde(default)]
    pub description: String,
}

fn default_transport_stdio() -> String {
    "stdio".to_string()
}

fn default_auth_none() -> String {
    "none".to_string()
}

// ─── Public codegen entry point ─────────────────────────────────────────

/// Parse + validate `mcp-servers.toml` bytes, returning the list of
/// validated server entries. Used by [`codegen_mcp_servers`] internally
/// and by the FS-reading orchestrator in `lib.rs`.
pub fn parse_mcp_servers_bytes(
    raw: &[u8],
    path: &Path,
) -> Result<Vec<McpServerEntry>, CodegenError> {
    let raw_str = std::str::from_utf8(raw).map_err(|e| CodegenError::SchemaValidation {
        context: path.display().to_string(),
        reason: format!("file is not valid UTF-8: {e}"),
    })?;
    let file: McpServersFile =
        toml::from_str(raw_str).map_err(|source| CodegenError::TomlParse {
            path: path.to_path_buf(),
            source,
        })?;

    assert_schema(MCP_SERVERS_SCHEMA, &file.schema, path)?;

    validate_servers(&file.servers, path)?;

    Ok(file.servers)
}

/// Codegen entry — parse TOML bytes + emit Rust source. Equivalent to the
/// legacy `parse_mcp_servers` + `generate_mcp_servers_rs` pair.
pub fn codegen_mcp_servers(toml_bytes: &[u8]) -> Result<String, CodegenError> {
    // Synthetic path used in error messages when the caller passes raw
    // bytes (no FS). Real FS callers go through `parse_mcp_servers_bytes`
    // with the actual path.
    let synthetic = Path::new("<bytes>:mcp-servers.toml");
    let servers = parse_mcp_servers_bytes(toml_bytes, synthetic)?;
    Ok(generate_mcp_servers_rs(&servers))
}

// ─── Validation ──────────────────────────────────────────────────────────

fn validate_servers(servers: &[McpServerEntry], path: &Path) -> Result<(), CodegenError> {
    // Unified bucket: every lookup key (id + aliases) routes through the
    // same `UniCase::ascii` normalisation at runtime, so build.rs uniqueness
    // checks must use the *same* lowering to stay consistent with the phf
    // map that gets emitted.
    let mut seen_keys: HashSet<String> = HashSet::new();

    for s in servers {
        let ctx = path.display().to_string();
        assert_ascii_key("server id", &s.id)
            .map_err(|r| CodegenError::schema_validation(&ctx, r))?;
        let id_key = s.id.to_ascii_lowercase();
        if !seen_keys.insert(id_key) {
            return Err(CodegenError::schema_validation(
                ctx,
                format!("duplicate server id (case-insensitive): {:?}", s.id),
            ));
        }
        if s.packages.is_empty() && s.remotes.is_empty() {
            return Err(CodegenError::schema_validation(
                ctx,
                format!(
                    "server {:?} has no packages and no remotes (need at least one)",
                    s.id
                ),
            ));
        }
        validate_category(&s.category, &s.id)
            .map_err(|r| CodegenError::schema_validation(&ctx, r))?;
        validate_pricing(&s.pricing, &s.id)
            .map_err(|r| CodegenError::schema_validation(&ctx, r))?;
        validate_tags(&s.tags, &s.id).map_err(|r| CodegenError::schema_validation(&ctx, r))?;
        validate_mcp_safety_tag(&s.tags, &s.id)
            .map_err(|r| CodegenError::schema_validation(&ctx, r))?;
        validate_effect_hints(&s.effect_hints, &s.id)
            .map_err(|r| CodegenError::schema_validation(&ctx, r))?;

        for pkg in &s.packages {
            validate_registry_type(&pkg.registry_type, &s.id)
                .map_err(|r| CodegenError::schema_validation(&ctx, r))?;
            validate_transport(&pkg.transport, &s.id)
                .map_err(|r| CodegenError::schema_validation(&ctx, r))?;
            match (pkg.registry_type.as_str(), pkg.runner.as_deref()) {
                ("pypi", None) => {
                    return Err(CodegenError::schema_validation(
                        ctx,
                        format!(
                            "server {:?}: pypi package {:?} must set `runner` (uvx or pipx)",
                            s.id, pkg.identifier
                        ),
                    ));
                }
                ("pypi", Some(r)) => validate_py_runner(r, &s.id)
                    .map_err(|r| CodegenError::schema_validation(&ctx, r))?,
                (_, Some(r)) => {
                    return Err(CodegenError::schema_validation(
                        ctx,
                        format!(
                            "server {:?}: package {:?} is {:?} but sets runner={r:?} (runner is pypi-only)",
                            s.id, pkg.identifier, pkg.registry_type
                        ),
                    ));
                }
                (_, None) => {}
            }
        }
        for r in &s.remotes {
            validate_transport(&r.transport, &s.id)
                .map_err(|reason| CodegenError::schema_validation(&ctx, reason))?;
            validate_auth(&r.auth, &s.id)
                .map_err(|reason| CodegenError::schema_validation(&ctx, reason))?;
        }

        for a in &s.aliases {
            assert_ascii_key("alias", a)
                .map_err(|reason| CodegenError::schema_validation(&ctx, reason))?;
            let key = a.to_ascii_lowercase();
            if !seen_keys.insert(key) {
                return Err(CodegenError::schema_validation(
                    ctx,
                    format!(
                        "alias {a:?} on server {:?} collides with an existing id or alias (case-insensitive)",
                        s.id
                    ),
                ));
            }
        }
    }

    Ok(())
}

/// MCP registry keys are ASCII by convention. Reject non-ASCII to avoid
/// the `UniCase::new` (unicode folding) vs `UniCase::ascii` (ASCII folding)
/// hash mismatch that would silently drop lookups.
fn assert_ascii_key(kind: &str, s: &str) -> Result<(), String> {
    if !s.is_ascii() {
        return Err(format!(
            "{kind} {s:?} contains non-ASCII characters (ASCII-only ids/aliases required for case-insensitive phf lookup)"
        ));
    }
    if s.is_empty() {
        return Err(format!("{kind} is empty"));
    }
    Ok(())
}

fn validate_category(cat: &str, server: &str) -> Result<(), String> {
    const KNOWN: &[&str] = &[
        "anthropic",
        "databases",
        "search",
        "developer",
        "productivity",
        "ai",
        "image",
        "audio",
        "communication",
        "vectordb",
        "analytics",
        "ecommerce",
        "cms",
        "devops",
        "social",
        "lifestyle",
        "marketing",
        "maps",
    ];
    if !KNOWN.contains(&cat) {
        return Err(format!("server {server:?}: unknown category {cat:?}"));
    }
    Ok(())
}

fn validate_pricing(p: &str, server: &str) -> Result<(), String> {
    match p {
        "free" | "freemium" | "paid" => Ok(()),
        _ => Err(format!("server {server:?}: unknown pricing {p:?}")),
    }
}

fn validate_registry_type(r: &str, server: &str) -> Result<(), String> {
    match r {
        "npm" | "pypi" | "oci" | "cargo" | "mcpb" => Ok(()),
        _ => Err(format!("server {server:?}: unknown registry_type {r:?}")),
    }
}

fn validate_transport(t: &str, server: &str) -> Result<(), String> {
    match t {
        "stdio" | "streamable-http" | "sse" => Ok(()),
        _ => Err(format!("server {server:?}: unknown transport {t:?}")),
    }
}

fn validate_auth(a: &str, server: &str) -> Result<(), String> {
    match a {
        "none" | "bearer" | "oauth" => Ok(()),
        _ => Err(format!("server {server:?}: unknown auth mode {a:?}")),
    }
}

fn validate_py_runner(r: &str, server: &str) -> Result<(), String> {
    match r {
        "uvx" | "pipx" => Ok(()),
        _ => Err(format!("server {server:?}: unknown py runner {r:?}")),
    }
}

/// Per-tool effect hints (schema `@1.1`) must each say SOMETHING coherent.
///
/// - a tool with both hints absent claims nothing — omit it (absence over
///   empty claim);
/// - `read_only = true` + `destructive = true` is incoherent — the MCP
///   spec makes `destructiveHint` meaningful only when `readOnlyHint` is
///   false, and vendoring a contradiction would poison the check's
///   irreversibility reasoning.
///
/// The server-LEVEL `read-only` XOR `destructive` tag invariant
/// ([`validate_mcp_safety_tag`]) is deliberately NOT cross-checked here:
/// a destructive server legitimately exposes read-only tools — that
/// per-tool grain is the whole point of the slot.
fn validate_effect_hints(
    hints: &BTreeMap<String, EffectHintEntry>,
    server_id: &str,
) -> Result<(), String> {
    for (tool, h) in hints {
        assert_ascii_key("effect_hints tool", tool)
            .map_err(|r| format!("server {server_id:?}: {r}"))?;
        if h.read_only.is_none() && h.destructive.is_none() {
            return Err(format!(
                "server {server_id:?}: effect_hints tool {tool:?} carries no hints — \
                 omit the tool instead (absence over empty claim)"
            ));
        }
        if h.read_only == Some(true) && h.destructive == Some(true) {
            return Err(format!(
                "server {server_id:?}: effect_hints tool {tool:?} claims BOTH read_only \
                 and destructive — a tool cannot be both (MCP: destructiveHint is \
                 meaningful only when readOnlyHint is false)"
            ));
        }
    }
    Ok(())
}

/// Enforce the security-filter invariant on MCP server tags: every entry MUST
/// carry **exactly one** of `read-only` or `destructive`. The Shield subsystem
/// uses these tags to allow/deny tools by capability class.
fn validate_mcp_safety_tag(tags: &[String], server_id: &str) -> Result<(), String> {
    let has_ro = tags.iter().any(|t| t == "read-only");
    let has_destructive = tags.iter().any(|t| t == "destructive");
    match (has_ro, has_destructive) {
        (true, true) => Err(format!(
            "MCP server {server_id:?}: cannot carry BOTH `read-only` and `destructive` — \
             pick the most-specific (a server with any write tool is destructive)"
        )),
        (false, false) => Err(format!(
            "MCP server {server_id:?}: must carry exactly one of `read-only` or `destructive` \
             (security-filter invariant — Shield uses this to gate untrusted-data workflows)"
        )),
        _ => Ok(()),
    }
}

// ─── Rust source emission ────────────────────────────────────────────────

#[must_use]
pub(crate) fn generate_mcp_servers_rs(servers: &[McpServerEntry]) -> String {
    let mut out = String::with_capacity(8_192);
    let _ = writeln!(
        out,
        "// GENERATED by build.rs from data/mcp-servers.toml. DO NOT EDIT."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "pub static ALL_MCP_SERVERS: &[crate::types::McpServer] = &["
    );

    for s in servers {
        emit_server(&mut out, s);
    }

    let _ = writeln!(out, "];");
    let _ = writeln!(out);

    // phf index: id (+ aliases) → array index, case-insensitive via UniCase.
    let mut builder = phf_codegen::Map::<unicase::UniCase<&str>>::new();
    let entries: Vec<(String, String)> = {
        let mut v = Vec::new();
        for (i, s) in servers.iter().enumerate() {
            v.push((s.id.clone(), i.to_string()));
            for a in &s.aliases {
                v.push((a.clone(), i.to_string()));
            }
        }
        v
    };
    for (k, v) in &entries {
        builder.entry(unicase::UniCase::ascii(k.as_str()), v.as_str());
    }
    let _ = writeln!(
        out,
        "pub static MCP_INDEX: phf::Map<unicase::UniCase<&'static str>, usize> = {};",
        builder.build()
    );

    out
}

fn emit_server(out: &mut String, s: &McpServerEntry) {
    let _ = writeln!(out, "    crate::types::McpServer {{");
    let _ = writeln!(out, "        id: {},", rstr(&s.id));
    write_aliases(out, &s.aliases);
    let _ = writeln!(out, "        title: {},", rstr(&s.title));
    let _ = writeln!(out, "        description: {},", rstr(&s.description));
    write_packages(out, &s.packages);
    write_remotes(out, &s.remotes);
    write_env_vars(out, &s.env_vars);
    let _ = writeln!(
        out,
        "        homepage: {},",
        opt_rstr(s.homepage.as_deref())
    );
    let _ = writeln!(
        out,
        "        category: crate::types::Category::{},",
        category_variant(&s.category)
    );
    let _ = writeln!(
        out,
        "        pricing: crate::types::McpPricing::{},",
        pricing_variant(&s.pricing)
    );
    let _ = writeln!(out, "        last_verified: {},", rstr(&s.last_verified));
    let _ = writeln!(out, "        tags: {},", tags_slice_expr(&s.tags));
    let _ = writeln!(
        out,
        "        extra_tags: {},",
        str_slice_expr(&s.extra_tags)
    );
    write_effect_hints(out, &s.effect_hints);
    let _ = writeln!(out, "    }},");
}

/// Per-tool hints in `BTreeMap` order (sorted by tool name) so emission
/// is deterministic. Absent hints stay `None` in the emitted source — an
/// unannotated tool must never reach the binary as `Some(false)`.
fn write_effect_hints(out: &mut String, hints: &BTreeMap<String, EffectHintEntry>) {
    if hints.is_empty() {
        let _ = writeln!(out, "        effect_hints: &[],");
        return;
    }
    let _ = writeln!(out, "        effect_hints: &[");
    for (tool, h) in hints {
        let _ = writeln!(
            out,
            "            crate::types::ToolEffectHints {{ tool: {}, read_only: {}, destructive: {} }},",
            rstr(tool),
            opt_plain(h.read_only),
            opt_plain(h.destructive),
        );
    }
    let _ = writeln!(out, "        ],");
}

fn write_aliases(out: &mut String, aliases: &[String]) {
    if aliases.is_empty() {
        let _ = writeln!(out, "        aliases: &[],");
        return;
    }
    out.push_str("        aliases: &[");
    for (i, a) in aliases.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&rstr(a));
    }
    let _ = writeln!(out, "],");
}

fn write_packages(out: &mut String, pkgs: &[PackageEntry]) {
    if pkgs.is_empty() {
        let _ = writeln!(out, "        packages: &[],");
        return;
    }
    let _ = writeln!(out, "        packages: &[");
    for p in pkgs {
        let _ = writeln!(out, "            crate::types::McpPackage {{");
        let _ = writeln!(
            out,
            "                registry_type: crate::types::RegistryType::{},",
            registry_variant(&p.registry_type)
        );
        let _ = writeln!(out, "                identifier: {},", rstr(&p.identifier));
        let _ = writeln!(
            out,
            "                version: {},",
            opt_rstr(p.version.as_deref())
        );
        let _ = writeln!(
            out,
            "                transport: crate::types::Transport::{},",
            transport_variant(&p.transport)
        );
        let _ = writeln!(
            out,
            "                runner: {},",
            match &p.runner {
                Some(r) => format!("Some(crate::types::PyRunner::{})", py_runner_variant(r)),
                None => "None".to_string(),
            }
        );
        let _ = writeln!(out, "            }},");
    }
    let _ = writeln!(out, "        ],");
}

fn write_remotes(out: &mut String, remotes: &[RemoteEntry]) {
    if remotes.is_empty() {
        let _ = writeln!(out, "        remotes: &[],");
        return;
    }
    let _ = writeln!(out, "        remotes: &[");
    for r in remotes {
        let _ = writeln!(out, "            crate::types::McpRemote {{");
        let _ = writeln!(
            out,
            "                transport: crate::types::Transport::{},",
            transport_variant(&r.transport)
        );
        let _ = writeln!(out, "                url: {},", rstr(&r.url));
        let _ = writeln!(
            out,
            "                auth: crate::types::AuthMode::{},",
            auth_variant(&r.auth)
        );
        let _ = writeln!(out, "            }},");
    }
    let _ = writeln!(out, "        ],");
}

fn write_env_vars(out: &mut String, env: &[EnvVarEntry]) {
    if env.is_empty() {
        let _ = writeln!(out, "        env_vars: &[],");
        return;
    }
    let _ = writeln!(out, "        env_vars: &[");
    for e in env {
        let _ = writeln!(out, "            crate::types::EnvVarSpec {{");
        let _ = writeln!(out, "                name: {},", rstr(&e.name));
        let _ = writeln!(
            out,
            "                key_prefixes: {},",
            str_slice_expr(&e.key_prefixes)
        );
        let _ = writeln!(out, "                required: {},", e.required);
        let _ = writeln!(out, "                is_secret: {},", e.is_secret);
        let _ = writeln!(
            out,
            "                description: {},",
            rstr(&e.description)
        );
        let _ = writeln!(out, "            }},");
    }
    let _ = writeln!(out, "        ],");
}

// ─── Variant mapping ─────────────────────────────────────────────────────

fn category_variant(s: &str) -> &'static str {
    match s {
        "anthropic" => "Anthropic",
        "databases" => "Databases",
        "search" => "Search",
        "developer" => "Developer",
        "productivity" => "Productivity",
        "ai" => "Ai",
        "image" => "Image",
        "audio" => "Audio",
        "communication" => "Communication",
        "vectordb" => "Vectordb",
        "analytics" => "Analytics",
        "ecommerce" => "Ecommerce",
        "cms" => "Cms",
        "devops" => "Devops",
        "social" => "Social",
        "lifestyle" => "Lifestyle",
        "marketing" => "Marketing",
        "maps" => "Maps",
        _ => panic_validated("category", s),
    }
}

fn pricing_variant(s: &str) -> &'static str {
    match s {
        "free" => "Free",
        "freemium" => "Freemium",
        "paid" => "Paid",
        _ => panic_validated("pricing", s),
    }
}

fn registry_variant(s: &str) -> &'static str {
    match s {
        "npm" => "Npm",
        "pypi" => "Pypi",
        "oci" => "Oci",
        "cargo" => "Cargo",
        "mcpb" => "Mcpb",
        _ => panic_validated("registry_type", s),
    }
}

fn transport_variant(s: &str) -> &'static str {
    match s {
        "stdio" => "Stdio",
        "streamable-http" => "StreamableHttp",
        "sse" => "Sse",
        _ => panic_validated("transport", s),
    }
}

fn auth_variant(s: &str) -> &'static str {
    match s {
        "none" => "None",
        "bearer" => "Bearer",
        "oauth" => "OAuth",
        _ => panic_validated("auth", s),
    }
}

fn py_runner_variant(s: &str) -> &'static str {
    match s {
        "uvx" => "Uvx",
        "pipx" => "Pipx",
        _ => panic_validated("runner", s),
    }
}

/// Helper for the post-validation `unreachable!` arms — the legacy code
/// used `unreachable!`, but workspace lints `panic = "deny"` make that
/// awkward in src/. Centralising here lets the few unreachable branches
/// share a single allow scope and surface a clear failure message if the
/// invariant is ever violated.
#[allow(
    clippy::panic,
    reason = "validated upstream — branch is unreachable in valid input"
)]
fn panic_validated(field: &str, value: &str) -> ! {
    panic!(
        "internal invariant violated: {field}={value:?} reached variant mapping without prior validation"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_one_server() -> McpServerEntry {
        McpServerEntry {
            id: "demo".to_string(),
            aliases: vec!["d".to_string()],
            title: "Demo".to_string(),
            description: "demo server".to_string(),
            category: "ai".to_string(),
            pricing: "free".to_string(),
            homepage: None,
            last_verified: "2026-01-01".to_string(),
            packages: vec![PackageEntry {
                registry_type: "npm".to_string(),
                identifier: "demo-pkg".to_string(),
                version: Some("1.0.0".to_string()),
                transport: "stdio".to_string(),
                runner: None,
            }],
            remotes: vec![],
            env_vars: vec![],
            tags: vec!["read-only".to_string()],
            extra_tags: vec![],
            effect_hints: BTreeMap::new(),
        }
    }

    #[test]
    fn validate_mcp_safety_tag_requires_exactly_one() {
        assert!(validate_mcp_safety_tag(&["read-only".to_string()], "x").is_ok());
        assert!(validate_mcp_safety_tag(&["destructive".to_string()], "x").is_ok());
        assert!(validate_mcp_safety_tag(&[], "x").is_err());
        assert!(
            validate_mcp_safety_tag(&["read-only".to_string(), "destructive".to_string()], "x",)
                .is_err()
        );
    }

    #[test]
    fn validate_category_rejects_unknown() {
        assert!(validate_category("nope", "s").is_err());
        assert!(validate_category("ai", "s").is_ok());
    }

    #[test]
    fn validate_pricing_closed_set() {
        assert!(validate_pricing("free", "s").is_ok());
        assert!(validate_pricing("nope", "s").is_err());
    }

    #[test]
    fn validate_registry_type_closed_set() {
        for ok in ["npm", "pypi", "oci", "cargo", "mcpb"] {
            assert!(validate_registry_type(ok, "s").is_ok());
        }
        assert!(validate_registry_type("nope", "s").is_err());
    }

    #[test]
    fn validate_transport_closed_set() {
        for ok in ["stdio", "streamable-http", "sse"] {
            assert!(validate_transport(ok, "s").is_ok());
        }
        assert!(validate_transport("nope", "s").is_err());
    }

    #[test]
    fn validate_auth_closed_set() {
        for ok in ["none", "bearer", "oauth"] {
            assert!(validate_auth(ok, "s").is_ok());
        }
        assert!(validate_auth("nope", "s").is_err());
    }

    #[test]
    fn validate_py_runner_closed_set() {
        for ok in ["uvx", "pipx"] {
            assert!(validate_py_runner(ok, "s").is_ok());
        }
        assert!(validate_py_runner("nope", "s").is_err());
    }

    #[test]
    fn assert_ascii_key_rejects_non_ascii_and_empty() {
        assert!(assert_ascii_key("id", "ascii").is_ok());
        assert!(assert_ascii_key("id", "café").is_err());
        assert!(assert_ascii_key("id", "").is_err());
    }

    #[test]
    fn generate_emits_header_and_static() {
        let s = generate_mcp_servers_rs(&[fixture_one_server()]);
        assert!(s.contains("// GENERATED by build.rs from data/mcp-servers.toml. DO NOT EDIT."));
        assert!(s.contains("pub static ALL_MCP_SERVERS"));
        assert!(s.contains("pub static MCP_INDEX"));
        assert!(s.contains("crate::types::McpServer"));
        assert!(s.contains("crate::types::Category::Ai"));
        assert!(s.contains("crate::types::McpPricing::Free"));
    }

    #[test]
    fn idempotent_emission() {
        let server = fixture_one_server();
        let a = generate_mcp_servers_rs(&[fixture_one_server()]);
        let b = generate_mcp_servers_rs(&[server]);
        assert_eq!(a, b);
    }

    #[test]
    fn validate_servers_rejects_duplicate_ids() {
        let s1 = fixture_one_server();
        let mut s2 = fixture_one_server();
        s2.id = "DEMO".to_string(); // case-insensitive collision
        let err = validate_servers(&[s1, s2], Path::new("/x")).unwrap_err();
        assert!(matches!(err, CodegenError::SchemaValidation { .. }));
    }

    #[test]
    fn validate_servers_rejects_no_install_path() {
        let mut s = fixture_one_server();
        s.packages.clear();
        s.remotes.clear();
        let err = validate_servers(&[s], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("no packages and no remotes"), "got: {msg}");
    }

    #[test]
    fn validate_servers_pypi_requires_runner() {
        let mut s = fixture_one_server();
        s.packages[0].registry_type = "pypi".to_string();
        s.packages[0].runner = None;
        let err = validate_servers(&[s], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("must set `runner`"), "got: {msg}");
    }

    #[test]
    fn validate_servers_runner_pypi_only() {
        let mut s = fixture_one_server();
        s.packages[0].registry_type = "npm".to_string();
        s.packages[0].runner = Some("uvx".to_string());
        let err = validate_servers(&[s], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("runner is pypi-only"), "got: {msg}");
    }

    #[test]
    fn parse_mcp_servers_bytes_rejects_bad_schema() {
        let toml = b"schema = \"nika/mcp-servers@9.9\"\nservers = []\n";
        let err = parse_mcp_servers_bytes(toml, Path::new("/x")).unwrap_err();
        assert!(matches!(err, CodegenError::SchemaMismatch { .. }));
    }

    #[test]
    fn parse_mcp_servers_bytes_accepts_canonical_schema() {
        let toml = b"schema = \"nika/mcp-servers@1.1\"\nservers = []\n";
        let servers = parse_mcp_servers_bytes(toml, Path::new("/x")).unwrap();
        assert!(servers.is_empty());
    }

    #[test]
    fn parse_mcp_servers_bytes_rejects_superseded_1_0() {
        // Under @1.0 an absent hint meant "the schema never carried
        // hints"; under @1.1 it means "the manifest did not annotate".
        // A stale file must not be read with the new absence semantics.
        let toml = b"schema = \"nika/mcp-servers@1.0\"\nservers = []\n";
        let err = parse_mcp_servers_bytes(toml, Path::new("/x")).unwrap_err();
        assert!(format!("{err}").contains("1.1"), "got: {err}");
    }

    // ─── Mutation-kill arc 2026-06-11 (vector 39 · 47-survivor sweep) ────────

    #[test]
    fn serde_defaults_transport_stdio_and_auth_none() {
        // Kills the default_transport_stdio / default_auth_none replacements:
        // omitted fields must deserialize to the canonical defaults.
        let toml_src = r#"
            schema = "mcp-servers/v1"
            [[servers]]
            id = "demo"
            description = "d"
            category = "ai"
            pricing = "free"
            last_verified = "2026-01-01"
            [[servers.packages]]
            registry_type = "npm"
            identifier = "pkg"
            [[servers.remotes]]
            transport = "streamable-http"
            url = "https://example.org/mcp"
        "#;
        let parsed: McpServersFile = toml::from_str(toml_src).expect("parses");
        assert_eq!(parsed.servers[0].packages[0].transport, "stdio");
        assert_eq!(parsed.servers[0].remotes[0].auth, "none");
    }

    // ── @1.1 effect_hints ──────────────────────────────────────────

    fn hint(read_only: Option<bool>, destructive: Option<bool>) -> EffectHintEntry {
        EffectHintEntry {
            read_only,
            destructive,
        }
    }

    #[test]
    fn parse_accepts_effect_hints_keyed_by_tool() {
        let toml_src = r#"
            schema = "nika/mcp-servers@1.1"
            [[servers]]
            id = "demo"
            description = "d"
            category = "ai"
            pricing = "free"
            last_verified = "2026-01-01"
            tags = ["read-only"]
            [[servers.packages]]
            registry_type = "npm"
            identifier = "pkg"
            [servers.effect_hints]
            query = { read_only = true }
            drop_table = { destructive = true }
        "#;
        let servers = parse_mcp_servers_bytes(toml_src.as_bytes(), Path::new("/x")).unwrap();
        let h = &servers[0].effect_hints;
        assert_eq!(h.len(), 2);
        assert_eq!(h["query"].read_only, Some(true));
        assert_eq!(h["query"].destructive, None, "unannotated stays None");
        assert_eq!(h["drop_table"].destructive, Some(true));
    }

    #[test]
    fn generate_emits_effect_hints_sorted_and_empty_slice() {
        let mut s = fixture_one_server();
        s.effect_hints
            .insert("write_row".to_string(), hint(Some(false), Some(true)));
        s.effect_hints
            .insert("query".to_string(), hint(Some(true), None));
        let out = generate_mcp_servers_rs(&[s]);
        let q = out
            .find("tool: \"query\", read_only: Some(true), destructive: None")
            .expect("query hint emitted");
        let w = out
            .find("tool: \"write_row\", read_only: Some(false), destructive: Some(true)")
            .expect("write_row hint emitted");
        assert!(q < w, "BTreeMap order: sorted by tool name");
        // The empty path: no manifest vendored emits the empty slice.
        let out = generate_mcp_servers_rs(&[fixture_one_server()]);
        assert!(out.contains("effect_hints: &[],"), "got: {out}");
    }

    #[test]
    fn validate_effect_hints_rejects_empty_claim_and_contradiction() {
        // Both hints absent = an entry that says nothing.
        let mut s = fixture_one_server();
        s.effect_hints.insert("noop".to_string(), hint(None, None));
        let err = validate_servers(&[s], Path::new("/x")).unwrap_err();
        assert!(format!("{err}").contains("carries no hints"), "got: {err}");
        // read_only + destructive both true = incoherent manifest.
        let mut s = fixture_one_server();
        s.effect_hints
            .insert("weird".to_string(), hint(Some(true), Some(true)));
        let err = validate_servers(&[s], Path::new("/x")).unwrap_err();
        assert!(format!("{err}").contains("BOTH read_only"), "got: {err}");
    }

    #[test]
    fn validate_effect_hints_accepts_coherent_grains() {
        // A destructive SERVER exposing a read-only TOOL is the point of
        // the per-tool grain — must pass alongside the server-level tag.
        let mut s = fixture_one_server();
        s.tags = vec!["destructive".to_string()];
        s.effect_hints
            .insert("read_file".to_string(), hint(Some(true), None));
        s.effect_hints
            .insert("delete_file".to_string(), hint(Some(false), Some(true)));
        validate_servers(&[s], Path::new("/x")).expect("coherent per-tool hints must pass");
    }

    #[test]
    fn validate_servers_accepts_unique_alias_rejects_collision() {
        // Kills `delete !` on seen_keys.insert(alias): a VALID config with a
        // unique alias must pass (the mutant errors on every first insert).
        let ok = fixture_one_server();
        validate_servers(std::slice::from_ref(&ok), Path::new("<test>"))
            .expect("unique alias must validate");
        // Alias colliding with an existing id (case-insensitive) must fail.
        let mut imposter = fixture_one_server();
        imposter.id = "other".to_string();
        imposter.aliases = vec!["DEMO".to_string()];
        let err = validate_servers(&[ok, imposter], Path::new("<test>"))
            .expect_err("alias colliding with an id must be rejected");
        assert!(err.to_string().contains("collides"), "{err}");
    }

    #[test]
    fn write_aliases_separator_placement_exact() {
        // Kills the `i > 0` comparator mutants (< / == / >=): the separator
        // goes BETWEEN items only — never before the first.
        let mut out = String::new();
        write_aliases(&mut out, &["a".to_string(), "b".to_string()]);
        assert!(out.contains(r#"&["a", "b"]"#), "got: {out}");
        let mut empty = String::new();
        write_aliases(&mut empty, &[]);
        assert!(empty.contains("&[],"), "got: {empty}");
    }

    #[test]
    fn category_variant_exhaustive() {
        let map = [
            ("anthropic", "Anthropic"),
            ("databases", "Databases"),
            ("search", "Search"),
            ("developer", "Developer"),
            ("productivity", "Productivity"),
            ("ai", "Ai"),
            ("image", "Image"),
            ("audio", "Audio"),
            ("communication", "Communication"),
            ("vectordb", "Vectordb"),
            ("analytics", "Analytics"),
            ("ecommerce", "Ecommerce"),
            ("cms", "Cms"),
            ("devops", "Devops"),
            ("social", "Social"),
            ("lifestyle", "Lifestyle"),
            ("marketing", "Marketing"),
            ("maps", "Maps"),
        ];
        for (s, want) in map {
            assert_eq!(category_variant(s), want, "{s}");
        }
    }

    #[test]
    fn registry_transport_auth_runner_variants_exhaustive() {
        for (s, want) in [
            ("npm", "Npm"),
            ("pypi", "Pypi"),
            ("oci", "Oci"),
            ("cargo", "Cargo"),
            ("mcpb", "Mcpb"),
        ] {
            assert_eq!(registry_variant(s), want, "{s}");
        }
        for (s, want) in [
            ("stdio", "Stdio"),
            ("streamable-http", "StreamableHttp"),
            ("sse", "Sse"),
        ] {
            assert_eq!(transport_variant(s), want, "{s}");
        }
        for (s, want) in [("none", "None"), ("bearer", "Bearer"), ("oauth", "OAuth")] {
            assert_eq!(auth_variant(s), want, "{s}");
        }
        for (s, want) in [("uvx", "Uvx"), ("pipx", "Pipx")] {
            assert_eq!(py_runner_variant(s), want, "{s}");
        }
    }
}
