//! `nika keys` — unified API key management.
//!
//! Phase 3: keys list — the wow display with source provenance.
//! Categorized view of all configured, env-only, and missing keys.

use colored::Colorize;
use serde::Serialize;

use nika_core::catalogs::providers::{Provider, ProviderCategory, KNOWN_PROVIDERS};

// ═══════════════════════════════════════════════════════════════════════════
// DATA TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// User-friendly category for key grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyCategory {
    /// LLM providers (Anthropic, OpenAI, etc.)
    Inference,
    /// Web search & crawling (Perplexity, Firecrawl, Ahrefs, DataForSEO)
    Search,
    /// Custom vault secrets
    Custom,
    /// Local providers (mock, native) - always available
    Local,
}

impl KeyCategory {
    /// Icon for this category.
    fn icon(self) -> &'static str {
        match self {
            Self::Inference => "\u{1F9E0}", // brain
            Self::Search => "\u{1F50D}",    // magnifying glass
            Self::Custom => "\u{1F527}",    // wrench
            Self::Local => "\u{25CE}",      // bullseye (◎)
        }
    }

    /// Display label (UPPERCASE).
    fn label(self) -> &'static str {
        match self {
            Self::Inference => "INFERENCE",
            Self::Search => "SEARCH",
            Self::Custom => "CUSTOM",
            Self::Local => "LOCAL",
        }
    }

    /// Right-side description.
    fn description(self) -> &'static str {
        match self {
            Self::Inference => "LLM providers",
            Self::Search => "web discovery",
            Self::Custom => "your secrets",
            Self::Local => "always available",
        }
    }

    /// Sort order for display.
    fn order(self) -> u8 {
        match self {
            Self::Inference => 0,
            Self::Search => 1,
            Self::Custom => 2,
            Self::Local => 3,
        }
    }
}

/// How the key was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeySource {
    /// Encrypted NikaVault (~/.nika/secrets/vault.enc)
    Vault,
    /// Environment variable
    Env,
    /// Daemon IPC (Unix)
    Daemon,
    /// Built-in (mock, filesystem, memory — no key needed)
    Builtin,
    /// Not found anywhere
    None,
}

impl KeySource {
    fn label(self) -> &'static str {
        match self {
            Self::Vault => "vault",
            Self::Env => "env",
            Self::Daemon => "daemon",
            Self::Builtin => "builtin",
            Self::None => "",
        }
    }
}

/// Current status of a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyStatus {
    /// Key found and usable.
    Configured,
    /// Key not found.
    NotConfigured,
    /// Key only in env var — lost on reboot.
    EnvOnly,
    /// System-level (mock, filesystem) — no key needed.
    System,
    /// Local provider but no model loaded.
    Offline,
}

impl KeyStatus {
    pub fn is_configured(self) -> bool {
        matches!(self, Self::Configured | Self::EnvOnly | Self::System)
    }
}

/// A fully resolved key with all metadata.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedKey {
    pub name: String,
    pub category: KeyCategory,
    pub status: KeyStatus,
    pub source: KeySource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_value: Option<String>,
    pub models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_var: Option<String>,
    pub description: String,
}

/// Summary statistics for JSON output.
#[derive(Debug, Clone, Serialize)]
pub struct KeysSummary {
    pub configured: usize,
    pub total: usize,
    pub env_only: usize,
    pub not_configured: usize,
}

/// Complete JSON output structure.
#[derive(Debug, Clone, Serialize)]
pub struct KeysJsonOutput {
    pub keys: Vec<ResolvedKey>,
    pub summary: KeysSummary,
}

// ═══════════════════════════════════════════════════════════════════════════
// CATEGORY MAPPING
// ═══════════════════════════════════════════════════════════════════════════

/// Map a provider to a user-friendly category.
pub fn categorize_provider(provider: &Provider) -> KeyCategory {
    match provider.category {
        ProviderCategory::Llm => KeyCategory::Inference,
        ProviderCategory::Mcp => {
            match provider.id {
                "perplexity" | "firecrawl" | "ahrefs" | "dataforseo" | "supadata" => {
                    KeyCategory::Search
                }
                _ => KeyCategory::Custom,
            }
        }
        ProviderCategory::Local => KeyCategory::Local,
    }
}

/// Top models per provider for display.
fn top_models(provider_id: &str) -> Vec<String> {
    match provider_id {
        "anthropic" => vec!["claude-sonnet-4-6".into(), "claude-haiku-4-5".into()],
        "openai" => vec!["gpt-4.1".into(), "gpt-4o".into(), "o4-mini".into()],
        "mistral" => vec!["mistral-large".into(), "mistral-small".into()],
        "groq" => vec!["llama-3.3-70b".into(), "mixtral-8x7b".into()],
        "deepseek" => vec!["deepseek-chat".into(), "deepseek-reasoner".into()],
        "gemini" => vec!["gemini-2.5-pro".into(), "gemini-2.5-flash".into()],
        "xai" => vec!["grok-3".into(), "grok-3-mini".into()],
        "openrouter" => vec!["200+ models".into()],
        "together" => vec!["llama".into(), "qwen".into()],
        "fireworks" => vec!["open-source models".into()],
        "cerebras" => vec!["2000+ tok/sec".into()],
        "sambanova" => vec!["rdu-inference".into()],
        "cohere" => vec!["command-r+".into()],
        "ai21" => vec!["jamba".into()],
        _ => vec![],
    }
}

/// Mask a key for display: show prefix + dots + last 4 chars.
///
/// e.g. "sk-ant-api03-abcdefgh12345678xyz" -> "sk-ant-••••8xyz"
pub fn mask_key_pretty(key: &str) -> String {
    if key.len() <= 8 {
        return "••••".to_string();
    }
    let last4 = &key[key.len().saturating_sub(4)..];
    // Find a prefix boundary (up to 8 chars before the dots)
    let prefix_len = key.len().min(6);
    let prefix = &key[..prefix_len];
    format!("{prefix}-\u{2022}\u{2022}\u{2022}\u{2022}{last4}")
}

// ═══════════════════════════════════════════════════════════════════════════
// KEY RESOLUTION
// ═══════════════════════════════════════════════════════════════════════════

/// Resolve the status of a single provider key.
///
/// Checks (in order): env var -> daemon -> vault -> not found.
pub fn resolve_provider_key(provider: &Provider, vault: &nika_vault::NikaVault) -> ResolvedKey {
    let env_var = provider.env_var;

    // Local providers that don't need keys
    if !provider.requires_key {
        return if provider.id == "mock" {
            ResolvedKey {
                name: provider.id.to_string(),
                category: categorize_provider(provider),
                status: KeyStatus::System,
                source: KeySource::Builtin,
                masked_value: None,
                models: vec![],
                env_var: None,
                description: provider.description.to_string(),
            }
        } else if provider.id == "native" {
            // Check if a model is loaded
            let has_model = std::env::var("NIKA_NATIVE_MODEL_PATH")
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            ResolvedKey {
                name: provider.id.to_string(),
                category: KeyCategory::Local,
                status: if has_model {
                    KeyStatus::Configured
                } else {
                    KeyStatus::Offline
                },
                source: if has_model {
                    KeySource::Env
                } else {
                    KeySource::None
                },
                masked_value: None,
                models: vec![],
                env_var: Some(env_var.to_string()),
                description: provider.description.to_string(),
            }
        } else {
            // Other no-key providers (filesystem, memory)
            ResolvedKey {
                name: provider.id.to_string(),
                category: categorize_provider(provider),
                status: KeyStatus::System,
                source: KeySource::Builtin,
                masked_value: None,
                models: vec![],
                env_var: Some(env_var.to_string()),
                description: provider.description.to_string(),
            }
        };
    }

    let category = categorize_provider(provider);
    let models = top_models(provider.id);

    // 1. Check env var
    if let Ok(val) = std::env::var(env_var) {
        if !val.is_empty() {
            // Check if also in vault (env + vault = configured; env only = warning)
            let in_vault = vault.get(provider.id).ok().flatten().is_some();
            return ResolvedKey {
                name: provider.id.to_string(),
                category,
                status: if in_vault {
                    KeyStatus::Configured
                } else {
                    KeyStatus::EnvOnly
                },
                source: KeySource::Env,
                masked_value: Some(mask_key_pretty(&val)),
                models,
                env_var: Some(env_var.to_string()),
                description: provider.description.to_string(),
            };
        }
    }

    // 2. Check vault
    if let Ok(Some(secret)) = vault.get(provider.id) {
        use secrecy::ExposeSecret;
        return ResolvedKey {
            name: provider.id.to_string(),
            category,
            status: KeyStatus::Configured,
            source: KeySource::Vault,
            masked_value: Some(mask_key_pretty(secret.expose_secret())),
            models,
            env_var: Some(env_var.to_string()),
            description: provider.description.to_string(),
        };
    }

    // 3. Not configured
    ResolvedKey {
        name: provider.id.to_string(),
        category,
        status: KeyStatus::NotConfigured,
        source: KeySource::None,
        masked_value: None,
        models,
        env_var: Some(env_var.to_string()),
        description: provider.description.to_string(),
    }
}

/// Resolve custom vault keys (custom:* prefix).
pub fn resolve_custom_keys(vault: &nika_vault::NikaVault) -> Vec<ResolvedKey> {
    let Ok(all_keys) = vault.list() else {
        return vec![];
    };

    all_keys
        .iter()
        .filter(|k| k.starts_with("custom:"))
        .map(|k| {
            let name = k.strip_prefix("custom:").unwrap_or(k);
            use secrecy::ExposeSecret;
            let masked = vault
                .get(k)
                .ok()
                .flatten()
                .map(|s| mask_key_pretty(s.expose_secret()));
            ResolvedKey {
                name: name.to_string(),
                category: KeyCategory::Custom,
                status: KeyStatus::Configured,
                source: KeySource::Vault,
                masked_value: masked,
                models: vec![],
                env_var: None,
                description: "Custom secret".to_string(),
            }
        })
        .collect()
}

/// Gather all resolved keys from all sources.
pub fn gather_all_keys(vault: &nika_vault::NikaVault) -> Vec<ResolvedKey> {
    let mut keys: Vec<ResolvedKey> = KNOWN_PROVIDERS
        .iter()
        // Skip providers that don't need keys and aren't interesting for display
        // (filesystem, memory — keep mock + native)
        .filter(|p| {
            p.requires_key || p.id == "mock" || p.id == "native"
        })
        .map(|p| resolve_provider_key(p, vault))
        .collect();

    // Add custom vault keys
    keys.extend(resolve_custom_keys(vault));

    keys
}

// ═══════════════════════════════════════════════════════════════════════════
// DISPLAY — THE WOW
// ═══════════════════════════════════════════════════════════════════════════

/// Dashed separator using ┈ (box drawing light horizontal).
fn dashed_separator(width: usize) -> String {
    "\u{2508}".repeat(width)
}

/// Render the keys list to a String (testable, no side effects).
pub fn render_keys_list(keys: &[ResolvedKey], verbose: bool) -> String {
    let mut out = String::new();

    let configured_count = keys.iter().filter(|k| k.status.is_configured()).count();
    let env_only_count = keys
        .iter()
        .filter(|k| k.status == KeyStatus::EnvOnly)
        .count();
    let not_configured: Vec<&ResolvedKey> = keys
        .iter()
        .filter(|k| k.status == KeyStatus::NotConfigured)
        .collect();

    // Empty state — welcome box
    if configured_count == 0 && keys.iter().all(|k| !k.status.is_configured()) {
        out.push_str(&render_empty_state());
        return out;
    }

    // Header
    let count_text = format!("{configured_count} configured");
    out.push_str(&format!(
        "\n  \u{1F511} {}{}",
        "Keys".bold(),
        format!("  {count_text}").dimmed()
    ));
    out.push('\n');

    // Group by category
    let mut categories: Vec<KeyCategory> = vec![
        KeyCategory::Inference,
        KeyCategory::Search,
        KeyCategory::Custom,
        KeyCategory::Local,
    ];
    categories.sort_by_key(|c| c.order());

    for cat in &categories {
        let cat_keys: Vec<&ResolvedKey> = keys.iter().filter(|k| k.category == *cat).collect();
        if cat_keys.is_empty() {
            continue;
        }

        out.push('\n');
        // Category header
        let header = format!(
            "  {} {}",
            cat.icon(),
            cat.label().bold().cyan(),
        );
        let desc = format!("\u{2014} {}", cat.description());
        // Right-align description
        out.push_str(&format!("{header:<44}{}\n", desc.dimmed()));
        out.push_str(&format!("  {}\n", dashed_separator(54).dimmed()));

        for key in &cat_keys {
            out.push_str(&render_key_line(key, *cat, verbose));
            out.push('\n');
        }
    }

    // Problem summary at bottom
    let problems = render_problem_summary(&not_configured, env_only_count);
    if !problems.is_empty() {
        out.push('\n');
        out.push_str(&problems);
        out.push('\n');
    }

    // Hint line
    out.push('\n');
    out.push_str(&format!(
        "  {}",
        "nika keys set \u{2039}name\u{203A}  \u{00B7}  nika keys check  \u{00B7}  nika keys sync"
            .dimmed()
    ));
    out.push('\n');

    out
}

/// Render a single key line.
fn render_key_line(key: &ResolvedKey, category: KeyCategory, verbose: bool) -> String {
    match key.status {
        KeyStatus::Configured | KeyStatus::EnvOnly => render_configured_line(key, verbose),
        KeyStatus::System => render_system_line(key),
        KeyStatus::Offline => render_offline_line(key),
        KeyStatus::NotConfigured => render_unconfigured_line(key, category),
    }
}

/// Render a configured key: `  ● name           sk-ant-••••7f2k     vault    Model1, Model2`
fn render_configured_line(key: &ResolvedKey, verbose: bool) -> String {
    let icon = if key.status == KeyStatus::EnvOnly {
        format!("{}", "\u{26A0}".yellow()) // warning sign for env-only
    } else {
        format!("{}", "\u{25CF}".green()) // filled circle
    };

    let name = format!("{:<16}", key.name).bold().to_string();
    let masked = key
        .masked_value
        .as_deref()
        .unwrap_or("")
        .dimmed()
        .to_string();
    let masked_padded = format!("{:<20}", masked);

    let source_colored = match key.source {
        KeySource::Vault => format!("{}", key.source.label().green()),
        KeySource::Env => format!("{}", key.source.label().yellow()),
        KeySource::Daemon => format!("{}", key.source.label().cyan()),
        _ => key.source.label().to_string(),
    };
    let source_padded = format!("{:<8}", source_colored);

    let models_str = if key.models.is_empty() {
        String::new()
    } else {
        key.models.join(", ").dimmed().to_string()
    };

    let mut line = format!("  {icon} {name} {masked_padded} {source_padded} {models_str}");

    // Env-only warning suffix
    if key.status == KeyStatus::EnvOnly {
        line.push_str(&format!("  {}", "lost on reboot".yellow()));
    }

    if verbose {
        if let Some(ref env_var) = key.env_var {
            line.push_str(&format!("\n    {}", format!("env: {env_var}").dimmed()));
        }
    }

    line
}

/// Render a system/builtin key line: `  ◎ mock           no key needed                deterministic`
fn render_system_line(key: &ResolvedKey) -> String {
    let icon = format!("{}", "\u{25CE}".green()); // ◎
    let name = format!("{:<16}", key.name).bold().to_string();
    let info = "no key needed".dimmed().to_string();
    let desc = key.description.dimmed().to_string();
    format!("  {icon} {name} {info:<20} {desc}")
}

/// Render an offline key line: `  ○ native         no model loaded              nika model pull`
fn render_offline_line(key: &ResolvedKey) -> String {
    let icon = format!("{}", "\u{25CB}".dimmed()); // ○
    let name = format!("{:<16}", key.name).bold().to_string();
    let info = "no model loaded".dimmed().to_string();
    let hint_text = "nika model pull".dimmed().to_string();
    format!("  {icon} {name} {info:<20} {hint_text}")
}

/// Render an unconfigured key line: `  · unconfigured                                 nika keys set name`
fn render_unconfigured_line(key: &ResolvedKey, _category: KeyCategory) -> String {
    let icon = format!("{}", "\u{00B7}".dimmed()); // ·
    let name = format!("{:<16}", key.name).dimmed().to_string();
    let set_hint = format!("nika keys set {}", key.name).dimmed().to_string();
    format!("  {icon} {name} {:<20} {set_hint}", "")
}

/// Render the empty state (zero configured keys).
fn render_empty_state() -> String {
    let width: usize = 52;
    let lines = vec![
        String::new(),
        "No API keys configured yet.".to_string(),
        String::new(),
        "Get started:".to_string(),
        "  nika keys set anthropic".to_string(),
        "  nika keys set openai".to_string(),
        String::new(),
        "Or import from environment:".to_string(),
        "  nika keys import --from env".to_string(),
    ];
    // Use panel_with_content style manually with rounded corners
    let inner = width.saturating_sub(4);
    let bar = "\u{2500}".repeat(inner + 2);
    let mut out = format!(
        "\n  \u{256D}{bar}\u{256E}\n  \u{2502} {:<inner$} \u{2502}\n",
        "\u{1F511} Keys".bold(),
    );
    out.push_str(&format!("  \u{251C}{bar}\u{2524}\n"));
    for line in &lines {
        out.push_str(&format!("  \u{2502} {:<inner$} \u{2502}\n", line));
    }
    out.push_str(&format!("  \u{2570}{bar}\u{256F}"));
    out.push('\n');
    out
}

/// Render the problem summary line.
///
/// `  · N not set: name1, name2  ⚠ N env-only: name3`
fn render_problem_summary(not_configured: &[&ResolvedKey], env_only_count: usize) -> String {
    let mut parts = Vec::new();

    if !not_configured.is_empty() {
        // Only show inference providers as "not set" (skip MCP/search — user may not need them)
        let inference_missing: Vec<&str> = not_configured
            .iter()
            .filter(|k| k.category == KeyCategory::Inference)
            .map(|k| k.name.as_str())
            .collect();
        if !inference_missing.is_empty() {
            let names = inference_missing.join(", ");
            parts.push(format!(
                "{} {} not set: {}",
                "\u{00B7}".dimmed(),
                inference_missing.len(),
                names.dimmed()
            ));
        }
    }

    if env_only_count > 0 {
        parts.push(format!(
            "{} {} env-only",
            "\u{26A0}".yellow(),
            env_only_count
        ));
    }

    if parts.is_empty() {
        return String::new();
    }

    format!("  {}", parts.join("  "))
}

/// Build JSON output for `--json` flag.
pub fn build_json_output(keys: &[ResolvedKey]) -> KeysJsonOutput {
    let configured = keys.iter().filter(|k| k.status.is_configured()).count();
    let env_only = keys
        .iter()
        .filter(|k| k.status == KeyStatus::EnvOnly)
        .count();
    let not_configured = keys
        .iter()
        .filter(|k| k.status == KeyStatus::NotConfigured)
        .count();

    KeysJsonOutput {
        keys: keys.to_vec(),
        summary: KeysSummary {
            configured,
            total: keys.len(),
            env_only,
            not_configured,
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC ENTRY POINT
// ═══════════════════════════════════════════════════════════════════════════

/// Get a vault instance (same pattern as provider.rs).
pub fn get_vault() -> nika_vault::NikaVault {
    #[cfg(unix)]
    let nika_home = nika_daemon::daemon_dir()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".nika"));
    #[cfg(not(unix))]
    let nika_home = dirs::home_dir().unwrap_or_default().join(".nika");
    nika_vault::NikaVault::new(&nika_home.join("secrets"))
}

/// Execute `nika keys list`.
pub fn handle_keys_list(json: bool, verbose: bool) -> Result<(), nika_engine::error::NikaError> {
    let vault = get_vault();
    let keys = gather_all_keys(&vault);

    if json {
        let output = build_json_output(&keys);
        let json_str = serde_json::to_string_pretty(&output).map_err(|e| {
            nika_engine::error::NikaError::IoError(std::io::Error::other(format!(
                "JSON serialization failed: {e}"
            )))
        })?;
        println!("{json_str}");
    } else {
        print!("{}", render_keys_list(&keys, verbose));
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use nika_core::catalogs::providers::find_provider;

    fn make_key(
        name: &str,
        category: KeyCategory,
        status: KeyStatus,
        source: KeySource,
        masked: Option<&str>,
        models: Vec<&str>,
    ) -> ResolvedKey {
        ResolvedKey {
            name: name.to_string(),
            category,
            status,
            source,
            masked_value: masked.map(|s| s.to_string()),
            models: models.into_iter().map(|s| s.to_string()).collect(),
            env_var: Some(format!("{}_API_KEY", name.to_uppercase())),
            description: format!("{name} provider"),
        }
    }

    // ── Test 1: Display with mix of configured/unconfigured ──────────
    #[test]
    fn display_mixed_configured_unconfigured() {
        let keys = vec![
            make_key(
                "anthropic",
                KeyCategory::Inference,
                KeyStatus::Configured,
                KeySource::Vault,
                Some("sk-ant-\u{2022}\u{2022}\u{2022}\u{2022}7f2k"),
                vec!["claude-sonnet-4-6", "claude-haiku-4-5"],
            ),
            make_key(
                "openai",
                KeyCategory::Inference,
                KeyStatus::NotConfigured,
                KeySource::None,
                None,
                vec!["gpt-4.1", "gpt-4o"],
            ),
            make_key(
                "mock",
                KeyCategory::Local,
                KeyStatus::System,
                KeySource::Builtin,
                None,
                vec![],
            ),
        ];

        let output = render_keys_list(&keys, false);

        // Header present with count
        assert!(output.contains("Keys"), "must contain Keys header");
        assert!(
            output.contains("2 configured"),
            "must show 2 configured (anthropic + mock)"
        );

        // Configured key shows name, masked value, source
        assert!(output.contains("anthropic"), "must show anthropic");
        assert!(output.contains("vault"), "must show vault source");

        // Unconfigured key shows set hint
        assert!(
            output.contains("nika keys set openai"),
            "must show set hint for unconfigured"
        );

        // Local section present
        assert!(output.contains("LOCAL"), "must show LOCAL section");
        assert!(
            output.contains("no key needed"),
            "mock must show no key needed"
        );
    }

    // ── Test 2: Empty state shows welcome box ────────────────────────
    #[test]
    fn empty_state_shows_welcome_box() {
        let keys = vec![
            make_key(
                "anthropic",
                KeyCategory::Inference,
                KeyStatus::NotConfigured,
                KeySource::None,
                None,
                vec![],
            ),
            make_key(
                "openai",
                KeyCategory::Inference,
                KeyStatus::NotConfigured,
                KeySource::None,
                None,
                vec![],
            ),
        ];

        let output = render_keys_list(&keys, false);

        // Welcome box with rounded corners
        assert!(
            output.contains('\u{256D}'),
            "must have rounded top-left corner"
        );
        assert!(
            output.contains('\u{256F}'),
            "must have rounded bottom-right corner"
        );
        assert!(
            output.contains("No API keys configured"),
            "must show welcome message"
        );
        assert!(
            output.contains("nika keys set anthropic"),
            "must show getting started hint"
        );
    }

    // ── Test 3: JSON output structure ────────────────────────────────
    #[test]
    fn json_output_structure() {
        let keys = vec![
            make_key(
                "anthropic",
                KeyCategory::Inference,
                KeyStatus::Configured,
                KeySource::Vault,
                Some("sk-ant-****"),
                vec!["claude-sonnet-4-6"],
            ),
            make_key(
                "openai",
                KeyCategory::Inference,
                KeyStatus::NotConfigured,
                KeySource::None,
                None,
                vec!["gpt-4.1"],
            ),
            make_key(
                "mock",
                KeyCategory::Local,
                KeyStatus::System,
                KeySource::Builtin,
                None,
                vec![],
            ),
        ];

        let json = build_json_output(&keys);

        assert_eq!(json.keys.len(), 3);
        assert_eq!(json.summary.configured, 2); // anthropic + mock
        assert_eq!(json.summary.total, 3);
        assert_eq!(json.summary.not_configured, 1); // openai
        assert_eq!(json.summary.env_only, 0);

        // Verify serialization works
        let serialized = serde_json::to_string(&json).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert!(parsed["summary"]["configured"].is_number());
        assert!(parsed["keys"].is_array());
    }

    // ── Test 4: Env-only warning appears ─────────────────────────────
    #[test]
    fn env_only_warning_appears() {
        let keys = vec![
            make_key(
                "anthropic",
                KeyCategory::Inference,
                KeyStatus::EnvOnly,
                KeySource::Env,
                Some("sk-ant-\u{2022}\u{2022}\u{2022}\u{2022}7f2k"),
                vec!["claude-sonnet-4-6"],
            ),
            make_key(
                "mock",
                KeyCategory::Local,
                KeyStatus::System,
                KeySource::Builtin,
                None,
                vec![],
            ),
        ];

        let output = render_keys_list(&keys, false);

        assert!(
            output.contains("lost on reboot"),
            "must warn about env-only key: got:\n{output}"
        );
        assert!(
            output.contains("env"),
            "must show env source: got:\n{output}"
        );
        // Problem summary mentions env-only
        assert!(
            output.contains("env-only"),
            "problem summary must mention env-only count"
        );
    }

    // ── Test 5: Custom keys in CUSTOM section ────────────────────────
    #[test]
    fn custom_keys_in_custom_section() {
        let keys = vec![
            make_key(
                "anthropic",
                KeyCategory::Inference,
                KeyStatus::Configured,
                KeySource::Vault,
                Some("sk-ant-****"),
                vec![],
            ),
            ResolvedKey {
                name: "ELEVENLABS_API_KEY".to_string(),
                category: KeyCategory::Custom,
                status: KeyStatus::Configured,
                source: KeySource::Vault,
                masked_value: Some("el-\u{2022}\u{2022}\u{2022}\u{2022}abcd".to_string()),
                models: vec![],
                env_var: None,
                description: "Custom secret".to_string(),
            },
        ];

        let output = render_keys_list(&keys, false);

        assert!(output.contains("CUSTOM"), "must show CUSTOM section");
        assert!(
            output.contains("ELEVENLABS_API_KEY"),
            "must show custom key name"
        );
    }

    // ── Test 6: Models shown for configured LLM providers ────────────
    #[test]
    fn models_shown_for_configured_providers() {
        let keys = vec![make_key(
            "anthropic",
            KeyCategory::Inference,
            KeyStatus::Configured,
            KeySource::Vault,
            Some("sk-ant-****"),
            vec!["claude-sonnet-4-6", "claude-haiku-4-5"],
        )];

        let output = render_keys_list(&keys, false);

        assert!(
            output.contains("claude-sonnet-4-6"),
            "must show model name"
        );
        assert!(
            output.contains("claude-haiku-4-5"),
            "must show second model"
        );
    }

    // ── Test 7: Categories hidden when empty ─────────────────────────
    #[test]
    fn categories_hidden_when_empty() {
        // Only inference keys, no search/custom
        let keys = vec![make_key(
            "anthropic",
            KeyCategory::Inference,
            KeyStatus::Configured,
            KeySource::Vault,
            Some("sk-ant-****"),
            vec![],
        )];

        let output = render_keys_list(&keys, false);

        assert!(output.contains("INFERENCE"), "must show INFERENCE section");
        assert!(
            !output.contains("SEARCH"),
            "must NOT show SEARCH section (empty)"
        );
        assert!(
            !output.contains("CUSTOM"),
            "must NOT show CUSTOM section (empty)"
        );
        assert!(
            !output.contains("LOCAL"),
            "must NOT show LOCAL section (empty)"
        );
    }

    // ── Test 8: Problem summary counts correct ───────────────────────
    #[test]
    fn problem_summary_counts_correct() {
        let keys = vec![
            make_key(
                "anthropic",
                KeyCategory::Inference,
                KeyStatus::Configured,
                KeySource::Vault,
                Some("sk-****"),
                vec![],
            ),
            make_key(
                "openai",
                KeyCategory::Inference,
                KeyStatus::NotConfigured,
                KeySource::None,
                None,
                vec![],
            ),
            make_key(
                "mistral",
                KeyCategory::Inference,
                KeyStatus::NotConfigured,
                KeySource::None,
                None,
                vec![],
            ),
            make_key(
                "groq",
                KeyCategory::Inference,
                KeyStatus::EnvOnly,
                KeySource::Env,
                Some("gsk_****"),
                vec![],
            ),
        ];

        let not_configured: Vec<&ResolvedKey> = keys
            .iter()
            .filter(|k| k.status == KeyStatus::NotConfigured)
            .collect();
        let env_only = keys
            .iter()
            .filter(|k| k.status == KeyStatus::EnvOnly)
            .count();

        let summary = render_problem_summary(&not_configured, env_only);

        // 2 inference not set
        assert!(
            summary.contains("2 not set"),
            "must show 2 not set, got: {summary}"
        );
        assert!(
            summary.contains("openai"),
            "must mention openai, got: {summary}"
        );
        assert!(
            summary.contains("mistral"),
            "must mention mistral, got: {summary}"
        );
        // 1 env-only
        assert!(
            summary.contains("1 env-only"),
            "must show 1 env-only, got: {summary}"
        );
    }

    // ── Helper tests ─────────────────────────────────────────────────

    #[test]
    fn mask_key_pretty_standard() {
        let masked = mask_key_pretty("sk-ant-api03-abcdefghijklmnop");
        assert!(masked.contains("sk-ant"), "must preserve prefix");
        assert!(
            masked.contains("\u{2022}\u{2022}\u{2022}\u{2022}"),
            "must have dots"
        );
        // Last 4 chars
        assert!(masked.ends_with("mnop"), "must end with last 4 chars");
    }

    #[test]
    fn mask_key_pretty_short() {
        let masked = mask_key_pretty("short");
        assert_eq!(masked, "\u{2022}\u{2022}\u{2022}\u{2022}");
    }

    #[test]
    fn categorize_llm_provider() {
        let anthropic = find_provider("anthropic").unwrap();
        assert_eq!(categorize_provider(anthropic), KeyCategory::Inference);
    }

    #[test]
    fn categorize_search_provider() {
        let perplexity = find_provider("perplexity").unwrap();
        assert_eq!(categorize_provider(perplexity), KeyCategory::Search);

        let firecrawl = find_provider("firecrawl").unwrap();
        assert_eq!(categorize_provider(firecrawl), KeyCategory::Search);
    }

    #[test]
    fn categorize_mcp_as_custom() {
        let neo4j = find_provider("neo4j").unwrap();
        assert_eq!(categorize_provider(neo4j), KeyCategory::Custom);

        let github = find_provider("github").unwrap();
        assert_eq!(categorize_provider(github), KeyCategory::Custom);
    }

    #[test]
    fn categorize_local_provider() {
        let mock = find_provider("mock").unwrap();
        assert_eq!(categorize_provider(mock), KeyCategory::Local);

        let native = find_provider("native").unwrap();
        assert_eq!(categorize_provider(native), KeyCategory::Local);
    }

    #[test]
    fn key_status_is_configured() {
        assert!(KeyStatus::Configured.is_configured());
        assert!(KeyStatus::EnvOnly.is_configured());
        assert!(KeyStatus::System.is_configured());
        assert!(!KeyStatus::NotConfigured.is_configured());
        assert!(!KeyStatus::Offline.is_configured());
    }
}
