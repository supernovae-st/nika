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
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

// ─── Schema versioning ──────────────────────────────────────────────────
//
// Every data file carries a top-level `schema = "nika/<name>@<version>"`
// string. build.rs hard-fails on mismatch. Bumping the version is how we
// signal a breaking change in the on-disk format (fields removed, semantics
// changed). Additive changes do not need a bump.

const MCP_SERVERS_SCHEMA: &str = "nika/mcp-servers@1.0";
const LLM_PROVIDERS_SCHEMA: &str = "nika/llm-providers@1.0";
const EMBEDDINGS_SCHEMA: &str = "nika/embeddings@1.0";

// ─── TOML schema (build-time only) ───────────────────────────────────────

#[derive(Deserialize)]
struct McpServersFile {
    schema: String,
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
    key_prefixes: Vec<String>,
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
    schema: String,
    providers: Vec<ProviderEntry>,
}

// ─── Embeddings TOML schema ──────────────────────────────────────────────

#[derive(Deserialize)]
struct EmbeddingsFile {
    schema: String,
    embeddings: Vec<EmbeddingEntry>,
}

#[derive(Deserialize)]
struct EmbeddingEntry {
    id: String,
    provider: String,
    model: String,
    dimensions: u32,
    max_input_tokens: u32,
    normalized_by_default: bool,
    similarity: String,
    input_per_million: f64,
    description: String,
}

#[derive(Deserialize)]
struct ProviderEntry {
    id: String,
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    env_var: String,
    #[serde(default)]
    key_prefixes: Vec<String>,
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
    // Both token-limit fields are REQUIRED. An unset field is a data
    // bug (silent undercount would trash budget enforcement), not a
    // "use sane default" situation.
    context_window_tokens: u32,
    max_output_tokens: u32,
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

    // Cargo's `rerun-if-changed` takes individual paths; walking the data
    // directory at build time registers every `.toml` file plus the dir
    // itself. Dir-level watching catches new-file additions; per-file
    // watching catches content edits on filesystems where inode mtime on
    // the dir lags.
    println!("cargo:rerun-if-changed={}", data_dir.display());
    println!("cargo:rerun-if-changed=build.rs");
    for entry in fs::read_dir(&data_dir)
        .map_err(|e| format!("reading data dir {}: {e}", data_dir.display()))?
    {
        let entry = entry.map_err(|e| format!("iterating data dir: {e}"))?;
        if entry.path().extension() == Some(OsStr::new("toml")) {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }

    let mcp_path = data_dir.join("mcp-servers.toml");
    let providers_path = data_dir.join("llm-providers.toml");

    let servers = parse_mcp_servers(&mcp_path)?;
    let generated = generate_mcp_servers_rs(&servers);
    fs::write(out_dir.join("mcp_servers.rs"), generated)
        .map_err(|e| format!("writing mcp_servers.rs: {e}"))?;

    let providers = parse_llm_providers(&providers_path)?;
    let generated = generate_providers_rs(&providers);
    fs::write(out_dir.join("providers.rs"), generated)
        .map_err(|e| format!("writing providers.rs: {e}"))?;

    let embeddings_path = data_dir.join("embeddings.toml");
    let embeddings = parse_embeddings(&embeddings_path, &providers)?;
    let generated = generate_embeddings_rs(&embeddings);
    fs::write(out_dir.join("embeddings.rs"), generated)
        .map_err(|e| format!("writing embeddings.rs: {e}"))?;

    Ok(())
}

fn assert_schema(expected: &str, got: &str, path: &Path) -> Result<(), String> {
    if got != expected {
        return Err(format!(
            "{}: schema mismatch — file declares {got:?}, build expects {expected:?}",
            path.display()
        ));
    }
    Ok(())
}

// ─── Parsing + validation ────────────────────────────────────────────────

fn parse_mcp_servers(path: &Path) -> Result<Vec<McpServerEntry>, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let file: McpServersFile =
        toml::from_str(&raw).map_err(|e| format!("parsing {}: {e}", path.display()))?;

    assert_schema(MCP_SERVERS_SCHEMA, &file.schema, path)?;

    // Unified bucket: every lookup key (id + aliases) routes through the
    // same `UniCase::ascii` normalisation at runtime, so build.rs uniqueness
    // checks must use the *same* lowering to stay consistent with the phf
    // map that gets emitted. Mixing raw + lowered caches leaves silent
    // collision holes (same UniCase key → two distinct entries).
    let mut seen_keys: HashSet<String> = HashSet::new();

    for s in &file.servers {
        assert_ascii_key("server id", &s.id)?;
        let id_key = s.id.to_ascii_lowercase();
        if !seen_keys.insert(id_key) {
            return Err(format!("duplicate server id (case-insensitive): {:?}", s.id));
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
            // Cross-constraint: `runner` is meaningful only for pypi packages.
            // `pypi` with no runner is also a data smell — pick one.
            match (pkg.registry_type.as_str(), pkg.runner.as_deref()) {
                ("pypi", None) => {
                    return Err(format!(
                        "server {:?}: pypi package {:?} must set `runner` (uvx or pipx)",
                        s.id, pkg.identifier
                    ));
                }
                ("pypi", Some(r)) => validate_py_runner(r, &s.id)?,
                (_, Some(r)) => {
                    return Err(format!(
                        "server {:?}: package {:?} is {:?} but sets runner={r:?} (runner is pypi-only)",
                        s.id, pkg.identifier, pkg.registry_type
                    ));
                }
                (_, None) => {}
            }
        }
        for r in &s.remotes {
            validate_transport(&r.transport, &s.id)?;
            validate_auth(&r.auth, &s.id)?;
        }

        // Aliases go in the same unified bucket — they can't collide with
        // any id OR any other alias (case-insensitive).
        for a in &s.aliases {
            assert_ascii_key("alias", a)?;
            let key = a.to_ascii_lowercase();
            if !seen_keys.insert(key) {
                return Err(format!(
                    "alias {a:?} on server {:?} collides with an existing id or alias (case-insensitive)",
                    s.id
                ));
            }
        }
    }

    Ok(file.servers)
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
    // Keys are borrowed from `entries` which lives until `builder.build()`
    // serialises them — no Box::leak needed. `UniCase::ascii` matches the
    // runtime probe in `find_mcp_server`.
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
        writeln!(out, "                key_prefixes: {},", str_slice_expr(&e.key_prefixes)).unwrap();
        writeln!(out, "                required: {},", e.required).unwrap();
        writeln!(out, "                is_secret: {},", e.is_secret).unwrap();
        writeln!(out, "                description: {},", rstr(&e.description)).unwrap();
        writeln!(out, "            }},").unwrap();
    }
    writeln!(out, "        ],").unwrap();
}

/// Emit a `&[&str]` expression suitable for a `const`/`static` context.
fn str_slice_expr(xs: &[String]) -> String {
    if xs.is_empty() {
        return "&[]".to_string();
    }
    let mut out = String::from("&[");
    for (i, s) in xs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&rstr(s));
    }
    out.push(']');
    out
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

    assert_schema(LLM_PROVIDERS_SCHEMA, &file.schema, path)?;

    // Unified bucket: ids + aliases all resolve via `UniCase::ascii`, so
    // uniqueness must match the same lowering to stay consistent with the
    // emitted phf map.
    let mut seen_keys: HashSet<String> = HashSet::new();
    for p in &file.providers {
        assert_ascii_key("provider id", &p.id)?;
        if !seen_keys.insert(p.id.to_ascii_lowercase()) {
            return Err(format!("duplicate provider id (case-insensitive): {:?}", p.id));
        }
        for a in &p.aliases {
            assert_ascii_key("provider alias", a)?;
            if !seen_keys.insert(a.to_ascii_lowercase()) {
                return Err(format!(
                    "provider {:?}: alias {a:?} collides with an existing id or alias (case-insensitive)",
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

        // Every provider ships at least one model; otherwise `default_model`
        // and `cheap_model` can't be self-consistent against the models list.
        if p.models.is_empty() {
            return Err(format!(
                "provider {:?}: models list is empty (at least one model required)",
                p.id
            ));
        }

        // Model invariants.
        let mut seen_nicks: HashSet<String> = HashSet::new();
        let mut seen_wires: HashSet<String> = HashSet::new();
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
            if !seen_wires.insert(m.model.clone()) {
                return Err(format!(
                    "provider {:?}: duplicate model wire {:?}",
                    p.id, m.model
                ));
            }
            if m.context_window_tokens == 0 || m.max_output_tokens == 0 {
                return Err(format!(
                    "provider {:?}: model {:?} has zero token limit",
                    p.id, m.id
                ));
            }
        }

        // Self-consistency: default_model + cheap_model must appear in the
        // models list (as wire identifiers). Fails the build, not a test.
        if !seen_wires.contains(&p.default_model) {
            return Err(format!(
                "provider {:?}: default_model {:?} not in models list",
                p.id, p.default_model
            ));
        }
        if !seen_wires.contains(&p.cheap_model) {
            return Err(format!(
                "provider {:?}: cheap_model {:?} not in models list",
                p.id, p.cheap_model
            ));
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
        "pub static ALL_PROVIDERS: &[crate::types::Provider] = &["
    )
    .unwrap();

    for p in providers {
        emit_provider(&mut out, p);
    }

    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    // phf index: id + aliases → array index. Same scoped-borrow pattern
    // as the MCP side — no Box::leak.
    let mut builder = phf_codegen::Map::<unicase::UniCase<&str>>::new();
    let entries: Vec<(String, String)> = {
        let mut v = Vec::new();
        for (i, p) in providers.iter().enumerate() {
            v.push((p.id.clone(), i.to_string()));
            for a in &p.aliases {
                v.push((a.clone(), i.to_string()));
            }
        }
        v
    };
    for (k, v) in &entries {
        builder.entry(unicase::UniCase::ascii(k.as_str()), v.as_str());
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

    writeln!(out, "    crate::types::Provider {{").unwrap();
    writeln!(out, "        id: {},", rstr(&p.id)).unwrap();
    writeln!(out, "        name: {},", rstr(&p.name)).unwrap();
    write_str_slice(out, "aliases", &p.aliases, 8);
    writeln!(out, "        env_var: {},", rstr(&p.env_var)).unwrap();
    writeln!(out, "        key_prefixes: {},", str_slice_expr(&p.key_prefixes)).unwrap();
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

// ─── Embeddings parse + emit ─────────────────────────────────────────────

fn parse_embeddings(
    path: &Path,
    providers: &[ProviderEntry],
) -> Result<Vec<EmbeddingEntry>, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let file: EmbeddingsFile =
        toml::from_str(&raw).map_err(|e| format!("parsing {}: {e}", path.display()))?;

    assert_schema(EMBEDDINGS_SCHEMA, &file.schema, path)?;

    // Build provider id set (case-insensitive lowercased) for FK-style check.
    let known_providers: HashSet<String> = providers
        .iter()
        .map(|p| p.id.to_ascii_lowercase())
        .collect();

    let mut seen_ids: HashSet<String> = HashSet::new();
    for e in &file.embeddings {
        assert_ascii_key("embedding id", &e.id)?;
        if !seen_ids.insert(e.id.to_ascii_lowercase()) {
            return Err(format!("duplicate embedding id: {:?}", e.id));
        }
        if e.dimensions == 0 {
            return Err(format!("embedding {:?}: dimensions must be > 0", e.id));
        }
        if e.max_input_tokens == 0 {
            return Err(format!("embedding {:?}: max_input_tokens must be > 0", e.id));
        }
        if e.input_per_million.is_nan() || e.input_per_million < 0.0 {
            return Err(format!(
                "embedding {:?}: input_per_million must be a finite non-negative number",
                e.id
            ));
        }
        if e.model.is_empty() {
            return Err(format!("embedding {:?}: model wire id is empty", e.id));
        }
        if e.description.is_empty() {
            return Err(format!("embedding {:?}: description is empty", e.id));
        }
        validate_similarity(&e.similarity, &e.id)?;
        if !known_providers.contains(&e.provider.to_ascii_lowercase()) {
            return Err(format!(
                "embedding {:?}: provider {:?} is not declared in llm-providers.toml",
                e.id, e.provider
            ));
        }
    }

    Ok(file.embeddings)
}

fn validate_similarity(s: &str, id: &str) -> Result<(), String> {
    match s {
        "cosine" | "dot-product" | "l2" => Ok(()),
        _ => Err(format!("embedding {id:?}: unknown similarity {s:?}")),
    }
}

fn similarity_variant(s: &str) -> &'static str {
    match s {
        "cosine" => "Cosine",
        "dot-product" => "DotProduct",
        "l2" => "L2",
        _ => unreachable!("validated earlier: similarity={s}"),
    }
}

fn generate_embeddings_rs(embeddings: &[EmbeddingEntry]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(4_096);
    writeln!(out, "// GENERATED by build.rs from data/embeddings.toml. DO NOT EDIT.").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "pub static ALL_EMBEDDINGS: &[crate::types::Embedding] = &["
    )
    .unwrap();

    for e in embeddings {
        writeln!(out, "    crate::types::Embedding {{").unwrap();
        writeln!(out, "        id: {},", rstr(&e.id)).unwrap();
        writeln!(out, "        provider: {},", rstr(&e.provider)).unwrap();
        writeln!(out, "        model: {},", rstr(&e.model)).unwrap();
        writeln!(out, "        dimensions: {},", e.dimensions).unwrap();
        writeln!(out, "        max_input_tokens: {},", e.max_input_tokens).unwrap();
        writeln!(
            out,
            "        normalized_by_default: {},",
            e.normalized_by_default
        )
        .unwrap();
        writeln!(
            out,
            "        similarity: crate::types::Similarity::{},",
            similarity_variant(&e.similarity)
        )
        .unwrap();
        writeln!(
            out,
            "        input_per_million: {:?},",
            e.input_per_million
        )
        .unwrap();
        writeln!(out, "        description: {},", rstr(&e.description)).unwrap();
        writeln!(out, "    }},").unwrap();
    }

    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    // phf index: id → array index. Same UniCase::ascii pattern as the others.
    let mut builder = phf_codegen::Map::<unicase::UniCase<&str>>::new();
    let entries: Vec<(String, String)> = embeddings
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.clone(), i.to_string()))
        .collect();
    for (k, v) in &entries {
        builder.entry(unicase::UniCase::ascii(k.as_str()), v.as_str());
    }
    writeln!(
        out,
        "pub static EMBEDDING_INDEX: phf::Map<unicase::UniCase<&'static str>, usize> = {};",
        builder.build()
    )
    .unwrap();

    out
}

// ─── String helpers ──────────────────────────────────────────────────────

/// Rust-escape a string into a double-quoted source literal.
///
/// Uses `char::escape_debug` for coverage of C0 controls, DEL (0x7F),
/// C1 controls (0x80..=0x9F), and Unicode scalars in general — wider
/// than a hand-rolled match ladder. Double-quote is the only char
/// `escape_debug` leaves unescaped that Rust string syntax requires
/// escaped, so we handle it explicitly.
fn rstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' {
            out.push_str("\\\"");
        } else {
            // escape_debug handles: \n, \r, \t, \\, \u{xx} for controls,
            // and passes readable UTF-8 through unchanged.
            for ec in c.escape_debug() {
                out.push(ec);
            }
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
