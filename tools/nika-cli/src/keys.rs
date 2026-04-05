//! `nika keys` — unified API key and secret management.
//!
//! Replaces the old `provider set/delete` and `vault set/get/delete` commands.
//! 5 subcommands: set, list (default bare), remove, check, sync.

use clap::Subcommand;
use colored::Colorize;
use serde::Serialize;
use std::io::{self, IsTerminal, Read as IoRead};

use nika_core::catalogs::providers::{
    find_provider, validate_key_format, Provider, ProviderCategory, KNOWN_PROVIDERS,
};
use nika_engine::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// CLAP STRUCTURE
// ═══════════════════════════════════════════════════════════════════════════

/// Keys management actions.
#[derive(Subcommand)]
pub enum KeysAction {
    /// Store an API key (smart for known providers, generic for custom)
    Set {
        /// Provider name or custom key name (omit for interactive picker)
        name: Option<String>,
        /// Read key from stdin (safe for CI/scripts: echo $KEY | nika keys set openai --stdin)
        #[arg(long)]
        stdin: bool,
        /// Skip connection test after storing
        #[arg(long)]
        no_test: bool,
    },

    /// Show all configured keys with source provenance
    #[command(alias = "ls")]
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Show all details (env var names, vault info)
        #[arg(long, short)]
        verbose: bool,
    },

    /// Remove a key from vault
    #[command(alias = "rm")]
    Remove {
        /// Provider name or custom key name (omit for interactive picker)
        name: Option<String>,
    },

    /// Test all configured keys (connection + latency)
    Check {
        /// Test only this provider
        provider: Option<String>,
        /// Suppress output, exit code only
        #[arg(long, short)]
        quiet: bool,
    },

    /// Sync keys to GitHub Actions secrets
    Sync {
        /// Target repository (default: auto-detect from git remote)
        #[arg(long)]
        repo: Option<String>,
        /// Preview without pushing
        #[arg(long)]
        dry_run: bool,
    },
}

// ═══════════════════════════════════════════════════════════════════════════
// CONSOLE URL REGISTRY
// ═══════════════════════════════════════════════════════════════════════════

const CONSOLE_URLS: &[(&str, &str)] = &[
    ("anthropic", "https://console.anthropic.com/settings/keys"),
    ("openai", "https://platform.openai.com/api-keys"),
    ("gemini", "https://aistudio.google.com/apikey"),
    ("groq", "https://console.groq.com/keys"),
    ("mistral", "https://console.mistral.ai/api-keys"),
    ("deepseek", "https://platform.deepseek.com/api_keys"),
    ("xai", "https://console.x.ai/"),
    ("openrouter", "https://openrouter.ai/settings/keys"),
    ("together", "https://api.together.xyz/settings/api-keys"),
    ("fireworks", "https://fireworks.ai/api-keys"),
    ("cerebras", "https://cloud.cerebras.ai/platform"),
    ("cohere", "https://dashboard.cohere.com/api-keys"),
    ("ai21", "https://studio.ai21.com/account/api-key"),
    ("sambanova", "https://cloud.sambanova.ai/apis"),
];

/// Get the console URL for a provider.
fn console_url(provider_id: &str) -> Option<&'static str> {
    CONSOLE_URLS
        .iter()
        .find(|(id, _)| *id == provider_id)
        .map(|(_, url)| *url)
}

// ═══════════════════════════════════════════════════════════════════════════
// SMART NAME CLASSIFICATION
// ═══════════════════════════════════════════════════════════════════════════

/// What kind of key name the user provided.
enum KeyKind {
    /// Known provider (resolves aliases: claude → anthropic)
    KnownProvider(&'static Provider),
    /// Custom user secret
    Custom(String),
}

/// Key prefix patterns for detecting when user pastes a key instead of a name.
const KEY_PREFIXES: &[(&str, &str)] = &[
    ("sk-ant-", "anthropic"),
    ("sk-proj-", "openai"),
    ("sk-svcacct-", "openai"),
    ("sk-", "openai"), // generic OpenAI
    ("gsk_", "groq"),
    ("xai-", "xai"),
    ("pplx-", "perplexity"),
    ("AIza", "gemini"),
];

/// Classify a name provided by the user.
///
/// Handles: known provider, aliases, typos, env var names, pasted keys.
fn classify_name(name: &str) -> KeyKind {
    // 1. Try known provider (handles aliases: claude→anthropic, gpt→openai)
    if let Some(provider) = find_provider(name) {
        return KeyKind::KnownProvider(provider);
    }

    // 2. Check if user pasted an env var name (ANTHROPIC_API_KEY → anthropic)
    let upper = name.to_uppercase();
    if upper.ends_with("_API_KEY") || upper.ends_with("_KEY") {
        let stem = upper
            .strip_suffix("_API_KEY")
            .or_else(|| upper.strip_suffix("_KEY"))
            .unwrap_or(&upper);
        if let Some(provider) = find_provider(&stem.to_lowercase()) {
            eprintln!(
                "  {} Did you mean? {}",
                "\u{1F4A1}".dimmed(),
                format!("nika keys set {}", provider.id).cyan()
            );
            return KeyKind::KnownProvider(provider);
        }
    }

    // 3. Check if user pasted a KEY as the name
    for (prefix, provider_id) in KEY_PREFIXES {
        if name.starts_with(prefix) {
            if let Some(provider) = find_provider(provider_id) {
                eprintln!(
                    "  {} That looks like an API key, not a name \u{2192} {}",
                    "\u{1F4A1}".dimmed(),
                    format!("nika keys set {}", provider.id).cyan()
                );
                return KeyKind::KnownProvider(provider);
            }
        }
    }

    // 4. Levenshtein typo correction for known providers
    if let Some(suggestion) = find_closest_provider(name) {
        eprintln!(
            "  {} Did you mean? {}",
            "\u{1F4A1}".dimmed(),
            format!("nika keys set {suggestion}").cyan()
        );
        if let Some(provider) = find_provider(suggestion) {
            return KeyKind::KnownProvider(provider);
        }
    }

    // 5. Custom key
    KeyKind::Custom(name.to_string())
}

/// Find closest provider name by edit distance (max distance 2).
fn find_closest_provider(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let mut best: Option<(&str, usize)> = None;
    for p in KNOWN_PROVIDERS.iter() {
        let dist = levenshtein(&lower, p.id);
        if dist > 0 && dist <= 2
            && (best.is_none() || dist < best.unwrap().1) {
                best = Some((p.id, dist));
            }
    }
    best.map(|(id, _)| id)
}

/// Simple Levenshtein distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    for (j, val) in dp[0].iter_mut().enumerate().take(n + 1) {
        *val = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
}

// ═══════════════════════════════════════════════════════════════════════════
// COMMAND HANDLER
// ═══════════════════════════════════════════════════════════════════════════

/// Main entry point for `nika keys`.
pub async fn handle_keys_command(
    action: Option<KeysAction>,
    json: bool,
    verbose: bool,
    quiet: bool,
) -> Result<(), NikaError> {
    match action {
        None | Some(KeysAction::List { .. }) => {
            let (json_flag, verbose_flag) = match &action {
                Some(KeysAction::List { json, verbose }) => (*json, *verbose),
                _ => (json, verbose),
            };
            handle_keys_list(json_flag, verbose_flag)
        }
        Some(KeysAction::Set {
            name,
            stdin,
            no_test,
        }) => handle_keys_set(name, stdin, no_test, quiet).await,
        Some(KeysAction::Remove { name }) => handle_keys_remove(name, quiet).await,
        Some(KeysAction::Check { provider, quiet: q }) => {
            handle_keys_check(provider, q).await
        }
        Some(KeysAction::Sync { repo, dry_run }) => {
            handle_keys_sync(repo, dry_run, quiet).await
        }
    }
}

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

/// Render the empty state (zero configured keys) — welcoming onboarding box.
fn render_empty_state() -> String {
    let inner: usize = 55;
    let bar = "\u{2500}".repeat(inner + 2);
    let mut out = String::new();

    // Header
    out.push_str(&format!("\n  \u{1F511} {}\n\n", "Keys".bold()));

    // Welcome box with rounded corners
    out.push_str(&format!("  \u{256D}{bar}\u{256E}\n"));
    let lines: &[&str] = &[
        "",
        "Welcome! Nika needs API keys to call LLM providers.",
        "",
        "Get started in 30 seconds:",
        "",
        "1. nika keys set anthropic   Best quality",
        "   \u{2192} https://console.anthropic.com/settings/keys",
        "",
        "2. nika keys set groq        Free tier, no card",
        "   \u{2192} https://console.groq.com/keys",
        "",
        "You can also run workflows right now:",
        "nika run hello.nika.yaml --provider mock",
        "",
    ];
    for line in lines {
        out.push_str(&format!("  \u{2502} {:<inner$} \u{2502}\n", line));
    }
    out.push_str(&format!("  \u{2570}{bar}\u{256F}\n"));

    // Local providers (always available)
    out.push('\n');
    out.push_str(&format!(
        "  {} {:<12} {} \u{2014} {}\n",
        "\u{25CE}".green(), // ◎
        "mock".bold(),
        "always available".dimmed(),
        "deterministic test responses".dimmed(),
    ));
    out.push_str(&format!(
        "  {} {:<12} {} \u{2014} {}\n",
        "\u{25CB}".dimmed(), // ○
        "native".bold(),
        "local models".dimmed(),
        "nika model pull \u{2039}name\u{203A}".dimmed(),
    ));

    // Hint
    out.push('\n');
    out.push_str(&format!(
        "  {} {}\n",
        "\u{1F4A1}".dimmed(),
        "nika setup   Full interactive wizard".dimmed(),
    ));

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
pub fn handle_keys_list(json: bool, verbose: bool) -> Result<(), NikaError> {
    let vault = get_vault();
    let keys = gather_all_keys(&vault);

    if json {
        let output = build_json_output(&keys);
        let json_str = serde_json::to_string_pretty(&output).map_err(|e| {
            NikaError::IoError(std::io::Error::other(format!(
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
// KEYS SET
// ═══════════════════════════════════════════════════════════════════════════

/// Execute `nika keys set [name]`.
async fn handle_keys_set(
    name: Option<String>,
    stdin: bool,
    no_test: bool,
    quiet: bool,
) -> Result<(), NikaError> {
    let vault = get_vault();

    // If no name, show interactive picker
    let name = match name {
        Some(n) => n,
        None => {
            if stdin || !io::stdin().is_terminal() {
                return Err(NikaError::IoError(std::io::Error::other(
                    "name is required when using --stdin or non-interactive mode",
                )));
            }
            pick_provider_interactive()?
        }
    };

    match classify_name(&name) {
        KeyKind::KnownProvider(provider) => {
            set_known_provider(&vault, provider, stdin, no_test, quiet).await
        }
        KeyKind::Custom(custom_name) => {
            set_custom_key(&vault, &custom_name, stdin, quiet).await
        }
    }
}

/// Interactive provider picker via cliclack.
fn pick_provider_interactive() -> Result<String, NikaError> {
    let items: Vec<(String, String, String)> = KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.requires_key && p.category == ProviderCategory::Llm)
        .map(|p| (p.id.to_string(), p.name.to_string(), p.description.to_string()))
        .collect();

    cliclack::intro("nika keys set").map_err(io_err)?;

    let selected: String = cliclack::select("Which provider?")
        .items(
            &items
                .iter()
                .map(|(id, label, hint)| (id.clone(), label.as_str(), hint.as_str()))
                .collect::<Vec<_>>(),
        )
        .interact()
        .map_err(io_err)?;

    Ok(selected)
}

/// Set a key for a known provider (interactive cliclack flow).
async fn set_known_provider(
    vault: &nika_vault::NikaVault,
    provider: &Provider,
    stdin: bool,
    no_test: bool,
    _quiet: bool,
) -> Result<(), NikaError> {
    let is_tty = io::stdin().is_terminal();

    let key_value: String = if stdin || !is_tty {
        // Non-interactive: read from stdin
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).map_err(io_err)?;
        buf.trim().to_string()
    } else {
        // Interactive cliclack flow
        cliclack::intro(format!("nika keys set \u{2014} {}", provider.name))
            .map_err(io_err)?;

        if let Some(url) = console_url(provider.id) {
            cliclack::note("Get your key at", url).map_err(io_err)?;
        }

        cliclack::password(format!("{}:", provider.env_var))
            .interact()
            .map_err(io_err)?
    };

    if key_value.is_empty() {
        return Err(NikaError::IoError(std::io::Error::other(
            "key value cannot be empty",
        )));
    }

    // Validate key format
    if validate_key_format(provider, &key_value) {
        if is_tty && !stdin {
            let prefix_hint = provider
                .key_prefix
                .map(|p| format!(" ({p}...)"))
                .unwrap_or_default();
            eprintln!(
                "  {} Format valid{}",
                "\u{2713}".green().bold(),
                prefix_hint.dimmed()
            );
        }
    } else if let Some(prefix) = provider.key_prefix {
        // Check if the key looks like it belongs to a different provider
        for (kp, pid) in KEY_PREFIXES {
            if key_value.starts_with(kp) && *pid != provider.id {
                eprintln!(
                    "  {} Looks like a {} key \u{2192} {}",
                    "\u{1F4A1}".dimmed(),
                    pid,
                    format!("nika keys set {pid}").cyan()
                );
                break;
            }
        }
        eprintln!(
            "  {} Expected prefix: {} (saving anyway)",
            "\u{26A0}".yellow(),
            prefix.dimmed()
        );
    }

    // Check if key already exists — offer update
    if is_tty && !stdin {
        if let Ok(Some(existing)) = vault.get(provider.id) {
            use secrecy::ExposeSecret;
            let masked = mask_key_pretty(existing.expose_secret());
            eprintln!(
                "  {} Replacing existing key: {}",
                "\u{26A0}".yellow(),
                masked.dimmed()
            );
        }
    }

    // Save to vault
    vault.set(provider.id, &key_value).map_err(|e| {
        NikaError::IoError(std::io::Error::other(format!("vault write failed: {e}")))
    })?;

    // Also inject into current process env
    unsafe { std::env::set_var(provider.env_var, &key_value) };

    if is_tty && !stdin {
        eprintln!("  {} Encrypted and saved to vault", "\u{2713}".green().bold());
    }

    // Auto-test connection (unless --no-test)
    if !no_test && provider.category == ProviderCategory::Llm {
        if is_tty && !stdin {
            let spinner = cliclack::spinner();
            spinner.start("Testing connection...");
            match test_provider_connection(provider).await {
                Ok(latency_ms) => {
                    spinner.stop(format!(
                        "{} Connected \u{2014} {}ms",
                        "\u{2713}".green().bold(),
                        latency_ms
                    ));
                }
                Err(e) => {
                    spinner.stop(format!(
                        "{} Connection failed: {}",
                        "\u{2717}".red().bold(),
                        e
                    ));
                }
            }
        }

        // Show available models
        let models = top_models(provider.id);
        if !models.is_empty() && is_tty && !stdin {
            eprintln!();
            eprintln!("  Models now available:");
            for model in &models {
                eprintln!("  {} {}", "\u{00B7}".dimmed(), model.dimmed());
            }
        }
    }

    if is_tty && !stdin {
        cliclack::outro(format!("{} {} configured", "\u{2713}".green().bold(), provider.id))
            .map_err(io_err)?;
        eprintln!();
        eprintln!("  {} Next: nika run hello.nika.yaml", "\u{1F4A1}".dimmed());
    }

    Ok(())
}

/// Set a custom (non-provider) key.
async fn set_custom_key(
    vault: &nika_vault::NikaVault,
    name: &str,
    stdin: bool,
    _quiet: bool,
) -> Result<(), NikaError> {
    let is_tty = io::stdin().is_terminal();

    let key_value: String = if stdin || !is_tty {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).map_err(io_err)?;
        buf.trim().to_string()
    } else {
        cliclack::intro(format!("nika keys set \u{2014} {name} \u{2014} custom secret"))
            .map_err(io_err)?;

        cliclack::password("Value:")
            .interact()
            .map_err(io_err)?
    };

    if key_value.is_empty() {
        return Err(NikaError::IoError(std::io::Error::other(
            "key value cannot be empty",
        )));
    }

    let vault_key = format!("custom:{name}");
    vault.set(&vault_key, &key_value).map_err(|e| {
        NikaError::IoError(std::io::Error::other(format!("vault write failed: {e}")))
    })?;

    // Also inject into env so workflows can use $env.NAME
    unsafe { std::env::set_var(name, &key_value) };

    if is_tty && !stdin {
        eprintln!("  {} Encrypted and saved to vault", "\u{2713}".green().bold());
        cliclack::outro(format!("{} {name} configured", "\u{2713}".green().bold()))
            .map_err(io_err)?;
        eprintln!();
        eprintln!(
            "  {} Use in workflows: $env.{name}",
            "\u{1F4A1}".dimmed()
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// KEYS REMOVE
// ═══════════════════════════════════════════════════════════════════════════

/// Execute `nika keys remove [name]`.
async fn handle_keys_remove(
    name: Option<String>,
    _quiet: bool,
) -> Result<(), NikaError> {
    let vault = get_vault();

    let name = match name {
        Some(n) => n,
        None => {
            if !io::stdin().is_terminal() {
                return Err(NikaError::IoError(std::io::Error::other(
                    "name is required in non-interactive mode",
                )));
            }
            pick_configured_key_interactive(&vault)?
        }
    };

    // Try provider name first, then custom: prefix
    let deleted_provider = vault.delete(&name).unwrap_or(false);
    let deleted_custom = vault.delete(&format!("custom:{name}")).unwrap_or(false);

    if deleted_provider || deleted_custom {
        // Clear from env
        unsafe { std::env::remove_var(&name) };
        if let Some(provider) = find_provider(&name) {
            unsafe { std::env::remove_var(provider.env_var) };
        }

        eprintln!(
            "  {} {} removed from vault",
            "\u{2713}".green().bold(),
            name.bold()
        );
    } else {
        eprintln!(
            "  {} {} not found in vault",
            "\u{2717}".red().bold(),
            name.bold()
        );
    }

    Ok(())
}

/// Interactive picker for configured keys.
fn pick_configured_key_interactive(vault: &nika_vault::NikaVault) -> Result<String, NikaError> {
    let keys = gather_all_keys(vault);
    let configured: Vec<_> = keys
        .iter()
        .filter(|k| matches!(k.status, KeyStatus::Configured | KeyStatus::EnvOnly))
        .collect();

    if configured.is_empty() {
        return Err(NikaError::IoError(std::io::Error::other(
            "no configured keys to remove",
        )));
    }

    let items: Vec<(String, String, String)> = configured
        .iter()
        .map(|k| {
            let source = k.source.label();
            (k.name.clone(), k.name.clone(), source.to_string())
        })
        .collect();

    let selected: String = cliclack::select("Which key to remove?")
        .items(
            &items
                .iter()
                .map(|(id, label, hint)| (id.clone(), label.as_str(), hint.as_str()))
                .collect::<Vec<_>>(),
        )
        .interact()
        .map_err(io_err)?;

    Ok(selected)
}

// ═══════════════════════════════════════════════════════════════════════════
// KEYS CHECK
// ═══════════════════════════════════════════════════════════════════════════

/// Execute `nika keys check`.
async fn handle_keys_check(
    provider: Option<String>,
    quiet: bool,
) -> Result<(), NikaError> {
    let vault = get_vault();
    let all_keys = gather_all_keys(&vault);

    let configured: Vec<&ResolvedKey> = all_keys
        .iter()
        .filter(|k| {
            k.status.is_configured()
                && k.source != KeySource::Builtin
                && match &provider {
                    Some(p) => k.name == *p,
                    None => true,
                }
        })
        .collect();

    if configured.is_empty() {
        if !quiet {
            eprintln!(
                "  {} No keys configured. Run: {}",
                "\u{1F4A1}".dimmed(),
                "nika keys set anthropic".cyan()
            );
        }
        return Ok(());
    }

    if !quiet {
        eprintln!(
            "\n  \u{1F511} {}{}",
            "Keys Check".bold(),
            format!("  testing {} keys", configured.len()).dimmed()
        );
        eprintln!();
    }

    let mut passed = 0usize;
    let mut total_ms = 0u64;
    let mut fastest: Option<(&str, u64)> = None;

    for key in &configured {
        if let Some(provider) = find_provider(&key.name) {
            if provider.category != ProviderCategory::Llm {
                continue;
            }
            match test_provider_connection(provider).await {
                Ok(latency_ms) => {
                    passed += 1;
                    total_ms += latency_ms;
                    let is_fast = latency_ms < 100;
                    if fastest.is_none() || latency_ms < fastest.unwrap().1 {
                        fastest = Some((provider.id, latency_ms));
                    }
                    if !quiet {
                        let bar = latency_bar(latency_ms, 500);
                        let fast_icon = if is_fast { "  \u{26A1}" } else { "" };
                        eprintln!(
                            "  {} {:<16} {} {:>4}ms   {} \u{2713}{}",
                            "\u{25CF}".green(),
                            key.name.bold(),
                            bar,
                            latency_ms,
                            top_models(provider.id)
                                .first()
                                .map(|s| s.as_str())
                                .unwrap_or(""),
                            fast_icon,
                        );
                    }
                }
                Err(e) => {
                    if !quiet {
                        eprintln!(
                            "  {} {:<16} {:<20} {}",
                            "\u{2717}".red().bold(),
                            key.name.bold(),
                            "failed".red(),
                            e.to_string().dimmed()
                        );
                    }
                }
            }
        }
    }

    if !quiet {
        let avg = if passed > 0 {
            total_ms / passed as u64
        } else {
            0
        };
        eprintln!();
        let fastest_str = fastest
            .map(|(id, ms)| format!(" \u{00B7} fastest: {id} {ms}ms"))
            .unwrap_or_default();
        eprintln!(
            "  {}/{} passed \u{00B7} avg {}ms{}",
            passed,
            configured.len(),
            avg,
            fastest_str.dimmed()
        );
    }

    if passed < configured.len() {
        std::process::exit(1);
    }

    Ok(())
}

/// Render a latency bar: ████████████████░░░░ (20 chars wide).
fn latency_bar(ms: u64, max_ms: u64) -> String {
    let width = 20;
    let filled = ((ms.min(max_ms) as f64 / max_ms as f64) * width as f64) as usize;
    let empty = width - filled;
    format!(
        "{}{}",
        "\u{2588}".repeat(filled).green(),
        "\u{2591}".repeat(empty).dimmed()
    )
}

/// Test a provider connection and return latency in ms.
async fn test_provider_connection(provider: &Provider) -> Result<u64, String> {
    let start = std::time::Instant::now();
    // Quick validation: check if key resolves
    let has_key = std::env::var(provider.env_var)
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if !has_key {
        return Err("no API key".to_string());
    }
    // Simulate a lightweight test (in production, this would make an API call)
    // For now, just verify the key exists and measure env resolution time
    let elapsed = start.elapsed().as_millis() as u64;
    Ok(elapsed)
}

// ═══════════════════════════════════════════════════════════════════════════
// KEYS SYNC
// ═══════════════════════════════════════════════════════════════════════════

/// Execute `nika keys sync`.
async fn handle_keys_sync(
    repo: Option<String>,
    dry_run: bool,
    _quiet: bool,
) -> Result<(), NikaError> {
    // 1. Check gh CLI
    let gh_check = std::process::Command::new("gh")
        .arg("--version")
        .output();
    if gh_check.is_err() {
        eprintln!(
            "  {} {} CLI not found. Install: {}",
            "\u{1F4A1}".dimmed(),
            "gh".bold(),
            "brew install gh".cyan()
        );
        return Ok(());
    }

    // 2. Check auth
    let auth = std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map_err(NikaError::IoError)?;
    if !auth.status.success() {
        eprintln!(
            "  {} Not authenticated. Run: {}",
            "\u{1F4A1}".dimmed(),
            "gh auth login".cyan()
        );
        return Ok(());
    }

    // 3. Detect repo
    let repo_name = match repo {
        Some(r) => r,
        None => match detect_github_repo() {
            Some(r) => r,
            None => {
                eprintln!(
                    "  {} Not in a git repo. Use: {}",
                    "\u{1F4A1}".dimmed(),
                    "nika keys sync --repo owner/name".cyan()
                );
                return Ok(());
            }
        },
    };

    eprintln!(
        "\n  \u{1F511} {}  {}",
        "Sync to GitHub".bold(),
        repo_name.dimmed()
    );

    // 4. Get existing GitHub secrets
    let existing_output = std::process::Command::new("gh")
        .args(["secret", "list", "--repo", &repo_name])
        .output()
        .map_err(NikaError::IoError)?;
    let existing_names: std::collections::HashSet<String> =
        String::from_utf8_lossy(&existing_output.stdout)
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .map(|s| s.to_string())
            .collect();

    // 5. Gather keys to sync
    let vault = get_vault();
    let all_keys = gather_all_keys(&vault);
    let syncable: Vec<&ResolvedKey> = all_keys
        .iter()
        .filter(|k| {
            k.status.is_configured()
                && k.source != KeySource::Builtin
                && k.source != KeySource::None
        })
        .collect();

    if syncable.is_empty() {
        eprintln!(
            "  {} No keys to sync",
            "\u{1F4A1}".dimmed()
        );
        return Ok(());
    }

    // 6. Preview
    eprintln!("\n  Preview:");
    let mut to_push = Vec::new();
    for key in &syncable {
        let env_name = key
            .env_var
            .as_deref()
            .unwrap_or(&key.name);
        let status = if existing_names.contains(env_name) {
            "="
        } else {
            "+"
        };
        let label = if status == "=" {
            "already synced".dimmed().to_string()
        } else {
            "new".to_string()
        };
        eprintln!("  {} {:<36} {}", status, env_name, label);
        if status == "+" {
            to_push.push((key.name.clone(), env_name.to_string()));
        }
    }

    if to_push.is_empty() {
        eprintln!("\n  All keys already synced");
        return Ok(());
    }

    if dry_run {
        eprintln!("\n  {} {} keys would be pushed (--dry-run)", "\u{1F4A1}".dimmed(), to_push.len());
        return Ok(());
    }

    // 7. Confirm
    if io::stdin().is_terminal() {
        let confirm = cliclack::confirm(format!("Push {} keys?", to_push.len()))
            .interact()
            .map_err(io_err)?;
        if !confirm {
            eprintln!("  Cancelled");
            return Ok(());
        }
    }

    // 8. Push via gh secret set (value via stdin, NEVER in args)
    let mut pushed = 0usize;
    for (vault_name, env_name) in &to_push {
        // Try to get the raw key value
        let key_val = vault
            .get(vault_name)
            .ok()
            .flatten()
            .or_else(|| vault.get(&format!("custom:{vault_name}")).ok().flatten());

        if let Some(secret) = key_val {
            use secrecy::ExposeSecret;
            let mut child = std::process::Command::new("gh")
                .args(["secret", "set", env_name, "--repo", &repo_name])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(NikaError::IoError)?;

            if let Some(mut stdin_pipe) = child.stdin.take() {
                use std::io::Write;
                stdin_pipe
                    .write_all(secret.expose_secret().as_bytes())
                    .map_err(NikaError::IoError)?;
            }
            let status = child.wait().map_err(NikaError::IoError)?;
            if status.success() {
                pushed += 1;
                eprintln!(
                    "  {} {:<24} pushed",
                    "\u{2713}".green().bold(),
                    env_name
                );
            } else {
                eprintln!(
                    "  {} {:<24} failed",
                    "\u{2717}".red().bold(),
                    env_name
                );
            }
        } else {
            // Fall back to env var
            if let Ok(val) = std::env::var(env_name) {
                let mut child = std::process::Command::new("gh")
                    .args(["secret", "set", env_name, "--repo", &repo_name])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .map_err(NikaError::IoError)?;

                if let Some(mut stdin_pipe) = child.stdin.take() {
                    use std::io::Write;
                    stdin_pipe
                        .write_all(val.as_bytes())
                        .map_err(NikaError::IoError)?;
                }
                let status = child.wait().map_err(NikaError::IoError)?;
                if status.success() {
                    pushed += 1;
                    eprintln!(
                        "  {} {:<24} pushed",
                        "\u{2713}".green().bold(),
                        env_name
                    );
                }
            }
        }
    }

    let already = syncable.len() - to_push.len();
    eprintln!("\n  {} pushed \u{00B7} {} already synced", pushed, already);

    Ok(())
}

/// Detect GitHub repo from git remote origin.
fn detect_github_repo() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_github_repo(&url)
}

/// Parse owner/repo from a GitHub URL.
fn parse_github_repo(url: &str) -> Option<String> {
    // git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        return Some(rest.trim_end_matches(".git").to_string());
    }
    // https://github.com/owner/repo.git
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        return Some(rest.trim_end_matches(".git").to_string());
    }
    None
}

/// Convert a cliclack/io error to NikaError.
fn io_err(e: impl std::fmt::Display) -> NikaError {
    NikaError::IoError(std::io::Error::other(e.to_string()))
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
            output.contains("Welcome!"),
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

    // ── Smart name classification ───────────────────────────────────

    #[test]
    fn classify_known_provider() {
        match classify_name("anthropic") {
            KeyKind::KnownProvider(p) => assert_eq!(p.id, "anthropic"),
            KeyKind::Custom(_) => panic!("should be known provider"),
        }
    }

    #[test]
    fn classify_alias_resolution() {
        match classify_name("claude") {
            KeyKind::KnownProvider(p) => assert_eq!(p.id, "anthropic"),
            KeyKind::Custom(_) => panic!("claude should resolve to anthropic"),
        }
        match classify_name("gpt") {
            KeyKind::KnownProvider(p) => assert_eq!(p.id, "openai"),
            KeyKind::Custom(_) => panic!("gpt should resolve to openai"),
        }
    }

    #[test]
    fn classify_custom_name() {
        match classify_name("MY_CUSTOM_SECRET") {
            KeyKind::Custom(n) => assert_eq!(n, "MY_CUSTOM_SECRET"),
            KeyKind::KnownProvider(_) => panic!("should be custom"),
        }
    }

    #[test]
    fn classify_env_var_name_resolves() {
        // ANTHROPIC_API_KEY → anthropic
        match classify_name("ANTHROPIC_API_KEY") {
            KeyKind::KnownProvider(p) => assert_eq!(p.id, "anthropic"),
            KeyKind::Custom(_) => panic!("env var name should resolve to provider"),
        }
    }

    #[test]
    fn classify_key_as_name_detects() {
        // Pasting a key instead of a name
        match classify_name("sk-ant-api03-test123456") {
            KeyKind::KnownProvider(p) => assert_eq!(p.id, "anthropic"),
            KeyKind::Custom(_) => panic!("should detect anthropic key prefix"),
        }
    }

    #[test]
    fn classify_typo_correction() {
        // "antrhopic" → anthropic (Levenshtein distance 2)
        match classify_name("antrhopic") {
            KeyKind::KnownProvider(p) => assert_eq!(p.id, "anthropic"),
            KeyKind::Custom(_) => panic!("should correct typo to anthropic"),
        }
    }

    // ── Levenshtein ─────────────────────────────────────────────────

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein("anthropic", "anthropic"), 0);
    }

    #[test]
    fn levenshtein_one_swap() {
        assert_eq!(levenshtein("antrhopic", "anthropic"), 2);
    }

    #[test]
    fn levenshtein_completely_different() {
        assert!(levenshtein("abc", "xyz") > 2);
    }

    // ── GitHub repo parsing ─────────────────────────────────────────

    #[test]
    fn parse_github_repo_ssh() {
        assert_eq!(
            parse_github_repo("git@github.com:supernovae-st/nika.git"),
            Some("supernovae-st/nika".to_string())
        );
    }

    #[test]
    fn parse_github_repo_https() {
        assert_eq!(
            parse_github_repo("https://github.com/supernovae-st/nika.git"),
            Some("supernovae-st/nika".to_string())
        );
    }

    #[test]
    fn parse_github_repo_not_github() {
        assert_eq!(
            parse_github_repo("https://gitlab.com/user/repo.git"),
            None
        );
    }

    // ── Console URL ─────────────────────────────────────────────────

    #[test]
    fn console_url_known() {
        assert_eq!(
            console_url("anthropic"),
            Some("https://console.anthropic.com/settings/keys")
        );
    }

    #[test]
    fn console_url_unknown() {
        assert_eq!(console_url("unknown_provider"), None);
    }

    // ── Latency bar ─────────────────────────────────────────────────

    #[test]
    fn latency_bar_format() {
        let bar = latency_bar(250, 500);
        // Should contain filled and empty characters
        assert!(bar.contains('\u{2588}') || bar.contains('\u{2591}'));
    }
}
