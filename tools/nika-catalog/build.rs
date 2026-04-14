// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

// Build script: runs at build time, never shipped to users. Workspace clippy
// rules designed for runtime src/ are relaxed here with explicit rationale:
//   * `println!("cargo:...")` is the cargo build-script protocol
//   * `eprintln!` + `process::exit(1)` is the standard failure path
//   * `env::var` is how cargo exposes OUT_DIR / CARGO_MANIFEST_DIR
//   * `HashSet` is fine for build-time data (no runtime cost)
//   * `.unwrap()` on `writeln!` to `String` is provably infallible
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::disallowed_types,
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::format_push_string
)]

//! Compile-time code generation for the Nika catalog.
//!
//! Parses `data/*.toml` at build time, validates schema invariants, and
//! emits Rust source to `$OUT_DIR/` for `include!()` from `src/data/`.
//!
//! Runtime dependencies: `phf` + `unicase` (O(1) case-insensitive lookup).
//! Build dependencies: `toml`, `phf_codegen`, `serde`, `unicase`.
//!
//! Schema invariants (fail-fast on violation — build error, not runtime):
//!
//! * unique `id` across all servers
//! * `packages.len() + remotes.len() >= 1` (at least one install path)
//! * `category` resolves via `Category::parse` (known variant)
//! * `pricing` is one of `"free" | "freemium" | "paid"`
//! * `registry_type == "pypi"` packages warn-if-missing `runner`
//!
//! Outputs (written to `$OUT_DIR`):
//!
//! * `mcp_servers.rs` — `static ALL_MCP_SERVERS: &[McpServer]` +
//!   `static MCP_INDEX: phf::Map<UniCase<&'static str>, usize>`

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

// ─── TOML schema (build-time only) ───────────────────────────────────────

#[derive(Deserialize)]
struct McpServersFile {
    servers: Vec<McpServerEntry>,
}

#[derive(Deserialize)]
struct McpServerEntry {
    id: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    title: String,
    description: String,
    category: String,
    pricing: String,
    #[serde(default)]
    homepage: Option<String>,
    last_verified: String,
    #[serde(default)]
    packages: Vec<PackageEntry>,
    #[serde(default)]
    remotes: Vec<RemoteEntry>,
    #[serde(default)]
    env_vars: Vec<EnvVarEntry>,
}

#[derive(Deserialize)]
struct PackageEntry {
    registry_type: String,
    identifier: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default = "default_transport_stdio")]
    transport: String,
    #[serde(default)]
    runner: Option<String>,
}

#[derive(Deserialize)]
struct RemoteEntry {
    transport: String,
    url: String,
    #[serde(default = "default_auth_none")]
    auth: String,
}

#[derive(Deserialize)]
struct EnvVarEntry {
    name: String,
    #[serde(default)]
    key_prefix: Option<String>,
    required: bool,
    #[serde(default)]
    is_secret: bool,
    #[serde(default)]
    description: String,
}

fn default_transport_stdio() -> String {
    "stdio".to_string()
}

fn default_auth_none() -> String {
    "none".to_string()
}

// ─── LLM providers TOML schema ───────────────────────────────────────────

#[derive(Deserialize)]
struct LlmProvidersFile {
    providers: Vec<ProviderEntry>,
}

#[derive(Deserialize)]
struct ProviderEntry {
    id: String,
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    env_var: String,
    #[serde(default)]
    key_prefix: Option<String>,
    default_model: String,
    cheap_model: String,
    requires_key: bool,
    description: String,
    #[serde(default)]
    models: Vec<ProviderModelEntry>,
}

#[derive(Deserialize)]
struct ProviderModelEntry {
    id: String,
    model: String,
    #[serde(default = "default_ctx_window")]
    context_window_tokens: u32,
    #[serde(default = "default_max_output")]
    max_output_tokens: u32,
}

fn default_ctx_window() -> u32 {
    128_000
}

fn default_max_output() -> u32 {
    4_096
}

// ─── Main ─────────────────────────────────────────────────────────────────

fn main() {
    if let Err(e) = run() {
        eprintln!("nika-catalog build.rs: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").map_err(|e| format!("CARGO_MANIFEST_DIR: {e}"))?;
    let data_dir = PathBuf::from(&manifest_dir).join("data");
    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|e| format!("OUT_DIR: {e}"))?);

    let mcp_path = data_dir.join("mcp-servers.toml");
    let providers_path = data_dir.join("llm-providers.toml");
    println!("cargo:rerun-if-changed={}", mcp_path.display());
    println!("cargo:rerun-if-changed={}", providers_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    let servers = parse_mcp_servers(&mcp_path)?;
    let generated = generate_mcp_servers_rs(&servers);
    fs::write(out_dir.join("mcp_servers.rs"), generated)
        .map_err(|e| format!("writing mcp_servers.rs: {e}"))?;

    let providers = parse_llm_providers(&providers_path)?;
    let generated = generate_providers_rs(&providers);
    fs::write(out_dir.join("providers.rs"), generated)
        .map_err(|e| format!("writing providers.rs: {e}"))?;

    Ok(())
}

// ─── Parsing + validation ────────────────────────────────────────────────

fn parse_mcp_servers(path: &Path) -> Result<Vec<McpServerEntry>, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let file: McpServersFile =
        toml::from_str(&raw).map_err(|e| format!("parsing {}: {e}", path.display()))?;

    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut seen_aliases: HashSet<String> = HashSet::new();

    for s in &file.servers {
        if !seen_ids.insert(s.id.clone()) {
            return Err(format!("duplicate server id: {}", s.id));
        }
        if s.packages.is_empty() && s.remotes.is_empty() {
            return Err(format!(
                "server {:?} has no packages and no remotes (need at least one)",
                s.id
            ));
        }
        validate_category(&s.category, &s.id)?;
        validate_pricing(&s.pricing, &s.id)?;

        for pkg in &s.packages {
            validate_registry_type(&pkg.registry_type, &s.id)?;
            validate_transport(&pkg.transport, &s.id)?;
            if let Some(r) = &pkg.runner {
                validate_py_runner(r, &s.id)?;
            }
        }
        for r in &s.remotes {
            validate_transport(&r.transport, &s.id)?;
            validate_auth(&r.auth, &s.id)?;
        }

        // Aliases must not collide with any other server's id or aliases.
        for a in &s.aliases {
            let key = a.to_ascii_lowercase();
            if seen_ids.contains(a) {
                return Err(format!("alias {a:?} on server {:?} collides with existing id", s.id));
            }
            if !seen_aliases.insert(key) {
                return Err(format!("duplicate alias: {a:?} (on server {:?})", s.id));
            }
        }
    }

    Ok(file.servers)
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

// ─── Rust source emission ────────────────────────────────────────────────

fn generate_mcp_servers_rs(servers: &[McpServerEntry]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(8_192);
    writeln!(out, "// GENERATED by build.rs from data/mcp-servers.toml. DO NOT EDIT.").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "pub static ALL_MCP_SERVERS: &[crate::types::McpServer] = &["
    )
    .unwrap();

    for s in servers {
        emit_server(&mut out, s);
    }

    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    // phf index: id (+ aliases) → array index, case-insensitive via UniCase.
    let mut builder = phf_codegen::Map::<unicase::UniCase<&str>>::new();
    let mut entry_strings: Vec<(String, String)> = Vec::new();
    for (i, s) in servers.iter().enumerate() {
        entry_strings.push((s.id.clone(), i.to_string()));
        for a in &s.aliases {
            entry_strings.push((a.clone(), i.to_string()));
        }
    }
    // Hold leaked 'static references for the builder.
    let leaked: Vec<(unicase::UniCase<&'static str>, String)> = entry_strings
        .into_iter()
        .map(|(k, v)| (unicase::UniCase::new(Box::leak(k.into_boxed_str()) as &'static str), v))
        .collect();
    for (k, v) in &leaked {
        builder.entry(*k, v);
    }
    writeln!(
        out,
        "pub static MCP_INDEX: phf::Map<unicase::UniCase<&'static str>, usize> = {};",
        builder.build()
    )
    .unwrap();

    out
}

fn emit_server(out: &mut String, s: &McpServerEntry) {
    use std::fmt::Write as _;

    writeln!(out, "    crate::types::McpServer {{").unwrap();
    writeln!(out, "        id: {},", rstr(&s.id)).unwrap();
    write_aliases(out, &s.aliases);
    writeln!(out, "        title: {},", rstr(&s.title)).unwrap();
    writeln!(out, "        description: {},", rstr(&s.description)).unwrap();
    write_packages(out, &s.packages);
    write_remotes(out, &s.remotes);
    write_env_vars(out, &s.env_vars);
    writeln!(out, "        homepage: {},", opt_rstr(s.homepage.as_deref())).unwrap();
    writeln!(out, "        category: crate::types::Category::{},", category_variant(&s.category)).unwrap();
    writeln!(out, "        pricing: crate::types::McpPricing::{},", pricing_variant(&s.pricing)).unwrap();
    writeln!(out, "        last_verified: {},", rstr(&s.last_verified)).unwrap();
    writeln!(out, "    }},").unwrap();
}

fn write_aliases(out: &mut String, aliases: &[String]) {
    use std::fmt::Write as _;
    if aliases.is_empty() {
        writeln!(out, "        aliases: &[],").unwrap();
        return;
    }
    out.push_str("        aliases: &[");
    for (i, a) in aliases.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&rstr(a));
    }
    writeln!(out, "],").unwrap();
}

fn write_packages(out: &mut String, pkgs: &[PackageEntry]) {
    use std::fmt::Write as _;
    if pkgs.is_empty() {
        writeln!(out, "        packages: &[],").unwrap();
        return;
    }
    writeln!(out, "        packages: &[").unwrap();
    for p in pkgs {
        writeln!(out, "            crate::types::McpPackage {{").unwrap();
        writeln!(
            out,
            "                registry_type: crate::types::RegistryType::{},",
            registry_variant(&p.registry_type)
        )
        .unwrap();
        writeln!(out, "                identifier: {},", rstr(&p.identifier)).unwrap();
        writeln!(out, "                version: {},", opt_rstr(p.version.as_deref())).unwrap();
        writeln!(
            out,
            "                transport: crate::types::Transport::{},",
            transport_variant(&p.transport)
        )
        .unwrap();
        writeln!(
            out,
            "                runner: {},",
            match &p.runner {
                Some(r) => format!("Some(crate::types::PyRunner::{})", py_runner_variant(r)),
                None => "None".to_string(),
            }
        )
        .unwrap();
        writeln!(out, "            }},").unwrap();
    }
    writeln!(out, "        ],").unwrap();
}

fn write_remotes(out: &mut String, remotes: &[RemoteEntry]) {
    use std::fmt::Write as _;
    if remotes.is_empty() {
        writeln!(out, "        remotes: &[],").unwrap();
        return;
    }
    writeln!(out, "        remotes: &[").unwrap();
    for r in remotes {
        writeln!(out, "            crate::types::McpRemote {{").unwrap();
        writeln!(
            out,
            "                transport: crate::types::Transport::{},",
            transport_variant(&r.transport)
        )
        .unwrap();
        writeln!(out, "                url: {},", rstr(&r.url)).unwrap();
        writeln!(
            out,
            "                auth: crate::types::AuthMode::{},",
            auth_variant(&r.auth)
        )
        .unwrap();
        writeln!(out, "            }},").unwrap();
    }
    writeln!(out, "        ],").unwrap();
}

fn write_env_vars(out: &mut String, env: &[EnvVarEntry]) {
    use std::fmt::Write as _;
    if env.is_empty() {
        writeln!(out, "        env_vars: &[],").unwrap();
        return;
    }
    writeln!(out, "        env_vars: &[").unwrap();
    for e in env {
        writeln!(out, "            crate::types::EnvVarSpec {{").unwrap();
        writeln!(out, "                name: {},", rstr(&e.name)).unwrap();
        writeln!(out, "                key_prefix: {},", opt_rstr(e.key_prefix.as_deref())).unwrap();
        writeln!(out, "                required: {},", e.required).unwrap();
        writeln!(out, "                is_secret: {},", e.is_secret).unwrap();
        writeln!(out, "                description: {},", rstr(&e.description)).unwrap();
        writeln!(out, "            }},").unwrap();
    }
    writeln!(out, "        ],").unwrap();
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
        _ => unreachable!("validated earlier: category={s}"),
    }
}

fn pricing_variant(s: &str) -> &'static str {
    match s {
        "free" => "Free",
        "freemium" => "Freemium",
        "paid" => "Paid",
        _ => unreachable!("validated earlier: pricing={s}"),
    }
}

fn registry_variant(s: &str) -> &'static str {
    match s {
        "npm" => "Npm",
        "pypi" => "Pypi",
        "oci" => "Oci",
        "cargo" => "Cargo",
        "mcpb" => "Mcpb",
        _ => unreachable!("validated earlier: registry_type={s}"),
    }
}

fn transport_variant(s: &str) -> &'static str {
    match s {
        "stdio" => "Stdio",
        "streamable-http" => "StreamableHttp",
        "sse" => "Sse",
        _ => unreachable!("validated earlier: transport={s}"),
    }
}

fn auth_variant(s: &str) -> &'static str {
    match s {
        "none" => "None",
        "bearer" => "Bearer",
        "oauth" => "OAuth",
        _ => unreachable!("validated earlier: auth={s}"),
    }
}

fn py_runner_variant(s: &str) -> &'static str {
    match s {
        "uvx" => "Uvx",
        "pipx" => "Pipx",
        _ => unreachable!("validated earlier: runner={s}"),
    }
}

// ─── LLM providers parse + emit ──────────────────────────────────────────

fn parse_llm_providers(path: &Path) -> Result<Vec<ProviderEntry>, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let file: LlmProvidersFile =
        toml::from_str(&raw).map_err(|e| format!("parsing {}: {e}", path.display()))?;

    // Uniqueness: provider ids + their aliases must not collide with anything.
    let mut seen_names: HashSet<String> = HashSet::new();
    for p in &file.providers {
        if !seen_names.insert(p.id.to_ascii_lowercase()) {
            return Err(format!("duplicate provider name: {:?}", p.id));
        }
        for a in &p.aliases {
            if !seen_names.insert(a.to_ascii_lowercase()) {
                return Err(format!(
                    "provider {:?}: alias {a:?} collides with another provider id/alias",
                    p.id
                ));
            }
        }
        if p.requires_key && p.env_var.is_empty() {
            return Err(format!(
                "provider {:?}: requires_key=true but env_var is empty",
                p.id
            ));
        }

        // Every model must have non-empty wire identifier + sane token limits.
        let mut seen_nicks: HashSet<String> = HashSet::new();
        for m in &p.models {
            if !seen_nicks.insert(m.id.clone()) {
                return Err(format!(
                    "provider {:?}: duplicate model nickname {:?}",
                    p.id, m.id
                ));
            }
            if m.model.is_empty() {
                return Err(format!(
                    "provider {:?}: model {:?} has empty wire identifier",
                    p.id, m.id
                ));
            }
            if m.context_window_tokens == 0 || m.max_output_tokens == 0 {
                return Err(format!(
                    "provider {:?}: model {:?} has zero token limit",
                    p.id, m.id
                ));
            }
        }
    }

    Ok(file.providers)
}

fn generate_providers_rs(providers: &[ProviderEntry]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(8_192);
    writeln!(out, "// GENERATED by build.rs from data/llm-providers.toml. DO NOT EDIT.").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "pub static ALL_PROVIDERS_V3: &[crate::types::ProviderDef] = &["
    )
    .unwrap();

    for p in providers {
        emit_provider(&mut out, p);
    }

    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    // phf index: id + aliases → array index.
    let mut builder = phf_codegen::Map::<unicase::UniCase<&str>>::new();
    let mut entries: Vec<(String, String)> = Vec::new();
    for (i, p) in providers.iter().enumerate() {
        entries.push((p.id.clone(), i.to_string()));
        for a in &p.aliases {
            entries.push((a.clone(), i.to_string()));
        }
    }
    let leaked: Vec<(unicase::UniCase<&'static str>, String)> = entries
        .into_iter()
        .map(|(k, v)| (unicase::UniCase::new(Box::leak(k.into_boxed_str()) as &'static str), v))
        .collect();
    for (k, v) in &leaked {
        builder.entry(*k, v);
    }
    writeln!(
        out,
        "pub static PROVIDER_INDEX: phf::Map<unicase::UniCase<&'static str>, usize> = {};",
        builder.build()
    )
    .unwrap();

    out
}

fn emit_provider(out: &mut String, p: &ProviderEntry) {
    use std::fmt::Write as _;

    writeln!(out, "    crate::types::ProviderDef {{").unwrap();
    writeln!(out, "        id: {},", rstr(&p.id)).unwrap();
    writeln!(out, "        name: {},", rstr(&p.name)).unwrap();
    write_str_slice(out, "aliases", &p.aliases, 8);
    writeln!(out, "        env_var: {},", rstr(&p.env_var)).unwrap();
    writeln!(out, "        key_prefix: {},", opt_rstr(p.key_prefix.as_deref())).unwrap();
    writeln!(out, "        default_model: {},", rstr(&p.default_model)).unwrap();
    writeln!(out, "        cheap_model: {},", rstr(&p.cheap_model)).unwrap();
    writeln!(out, "        requires_key: {},", p.requires_key).unwrap();
    writeln!(out, "        description: {},", rstr(&p.description)).unwrap();
    write_models(out, &p.models);
    writeln!(out, "    }},").unwrap();
}

fn write_str_slice(out: &mut String, field: &str, xs: &[String], indent: usize) {
    use std::fmt::Write as _;
    let pad = " ".repeat(indent);
    if xs.is_empty() {
        writeln!(out, "{pad}{field}: &[],").unwrap();
        return;
    }
    out.push_str(&format!("{pad}{field}: &["));
    for (i, s) in xs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&rstr(s));
    }
    writeln!(out, "],").unwrap();
}

fn write_models(out: &mut String, models: &[ProviderModelEntry]) {
    use std::fmt::Write as _;
    if models.is_empty() {
        writeln!(out, "        models: &[],").unwrap();
        return;
    }
    writeln!(out, "        models: &[").unwrap();
    for m in models {
        writeln!(out, "            crate::types::ProviderModel {{").unwrap();
        writeln!(out, "                id: {},", rstr(&m.id)).unwrap();
        writeln!(out, "                model: {},", rstr(&m.model)).unwrap();
        writeln!(out, "                context_window_tokens: {},", m.context_window_tokens).unwrap();
        writeln!(out, "                max_output_tokens: {},", m.max_output_tokens).unwrap();
        writeln!(out, "            }},").unwrap();
    }
    writeln!(out, "        ],").unwrap();
}

// ─── String helpers ──────────────────────────────────────────────────────

/// Rust-escape a string into a double-quoted source literal.
fn rstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn opt_rstr(s: Option<&str>) -> String {
    match s {
        Some(v) => format!("Some({})", rstr(v)),
        None => "None".to_string(),
    }
}
