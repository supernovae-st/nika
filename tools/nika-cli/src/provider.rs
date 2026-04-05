//! Provider management subcommand handler

use clap::Subcommand;
use colored::Colorize;
use std::io::IsTerminal;

use nika_engine::display::{hint, status_line, tree_connector, StatusIcon};
use nika_engine::error::NikaError;

/// Provider management actions (read-only catalog).
///
/// Key management moved to `nika keys`. Use:
///   nika keys set <provider>    — store an API key
///   nika keys remove <provider> — delete a key
///   nika keys check             — test all keys
#[derive(Subcommand)]
pub enum ProviderAction {
    /// List all providers, models, and pricing
    List,

    /// Test connection to a provider
    Test {
        /// Provider name
        provider: String,
        /// Suppress output — exit code only (for scripts/CI)
        #[arg(short, long)]
        quiet: bool,
    },

    // ── v0: did-you-mean redirects for old commands ────────────────────
    /// Moved to `nika keys set`
    #[command(hide = true)]
    Set {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        _args: Vec<String>,
    },
    /// Moved to `nika keys remove`
    #[command(hide = true)]
    Delete {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        _args: Vec<String>,
    },
    /// Moved to `nika keys`
    #[command(hide = true)]
    Get {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        _args: Vec<String>,
    },
    /// Moved to `nika keys`
    #[command(hide = true)]
    Migrate,
    /// Moved to `nika keys`
    #[command(hide = true, name = "vault-reset")]
    VaultReset,
}

/// Detect provider from a pasted API key prefix.
/// Delegates to the canonical KEY_PREFIXES in keys.rs.
pub fn detect_provider_from_key(key: &str) -> Option<&'static str> {
    crate::keys::KEY_PREFIXES
        .iter()
        .find(|(prefix, _)| key.starts_with(prefix))
        .map(|(_, provider)| *provider)
}

fn llm_provider_ids() -> Vec<&'static str> {
    use nika_engine::core::{ProviderCategory, KNOWN_PROVIDERS};
    KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == ProviderCategory::Llm)
        .map(|p| p.id)
        .collect()
}

/// Top models per provider — delegates to keys::top_models().
fn top_models_for_provider(provider: &str) -> String {
    crate::keys::top_models(provider).join(", ")
}

/// Get vault instance — delegates to the canonical keys::get_vault().
fn get_vault() -> nika_vault::NikaVault {
    crate::keys::get_vault()
}

/// Check if a provider has a key in env or vault.
fn has_key_env_or_vault(provider: &str, env_var: &str) -> bool {
    // Check env var first
    if std::env::var(env_var)
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    // Check vault
    get_vault().get(provider).ok().flatten().is_some()
}

pub async fn handle_provider_command(
    action: ProviderAction,
    _quiet: bool,
) -> Result<(), NikaError> {
    use nika_engine::core::provider_to_env_var;
    use nika_engine::secrets::mask_api_key;
    use secrecy::ExposeSecret;

    let all_providers = llm_provider_ids();

    match action {
        ProviderAction::List => {
            let mut configured = 0usize;
            let total = all_providers.len();

            // Pre-check daemon availability once (avoid per-provider reconnect)
            #[cfg(unix)]
            let daemon_client = {
                let sock = nika_daemon::daemon_socket_path();
                if sock.exists() {
                    Some(nika_daemon::DaemonClient::new(&sock))
                } else {
                    None
                }
            };
            #[cfg(not(unix))]
            let _daemon_client: Option<()> = None;

            let vault = get_vault();

            for provider in &all_providers {
                let env_var = provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY");
                // Check env var FIRST
                let has_env = std::env::var(env_var)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);
                if has_env {
                    configured += 1;
                    continue;
                }
                // Try daemon IPC
                #[cfg(unix)]
                {
                    if let Some(ref client) = daemon_client {
                        if let Ok(exists) = client.has_secret(provider).await {
                            if exists {
                                configured += 1;
                                continue;
                            }
                        }
                        continue;
                    }
                }
                // Try vault directly
                if vault.get(provider).ok().flatten().is_some() {
                    configured += 1;
                }
            }
            let count_color = if configured == total {
                format!("{configured}/{total} all configured")
                    .green()
                    .to_string()
            } else if configured > 0 {
                format!("{configured}/{total} configured")
                    .yellow()
                    .to_string()
            } else {
                format!("0/{total} configured").red().to_string()
            };
            println!("\n  {} ({})", "LLM Providers".bold(), count_color);
            println!("{}", nika_engine::display::separator(50));
            println!();
            for (i, provider) in all_providers.iter().enumerate() {
                let env_var = provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY");
                // Check env var FIRST
                let has_env = std::env::var(env_var)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);

                // Determine source: env > daemon > vault
                let (has_daemon, has_vault) = if has_env {
                    (false, false)
                } else {
                    #[cfg(unix)]
                    {
                        if let Some(ref client) = daemon_client {
                            if let Ok(exists) = client.has_secret(provider).await {
                                if exists {
                                    (true, false)
                                } else {
                                    (false, vault.get(provider).ok().flatten().is_some())
                                }
                            } else {
                                (false, vault.get(provider).ok().flatten().is_some())
                            }
                        } else {
                            (false, vault.get(provider).ok().flatten().is_some())
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        (false, vault.get(provider).ok().flatten().is_some())
                    }
                };

                let is_last = i == all_providers.len() - 1;
                let connector = tree_connector(is_last).dimmed();
                let (icon, source, masked) = match (has_env, has_daemon, has_vault) {
                    (true, _, _) => {
                        let m = std::env::var(env_var)
                            .ok()
                            .map(|k| mask_api_key(&k))
                            .unwrap_or_default();
                        (StatusIcon::Ok, "env", m)
                    }
                    (_, true, _) => {
                        // Daemon-sourced: get masked key via daemon
                        #[cfg(unix)]
                        let m = {
                            if let Some(ref client) = daemon_client {
                                client
                                    .get_secret(provider)
                                    .await
                                    .ok()
                                    .flatten()
                                    .map(|k| mask_api_key(&k))
                                    .unwrap_or_default()
                            } else {
                                String::new()
                            }
                        };
                        #[cfg(not(unix))]
                        let m = String::new();
                        (StatusIcon::Ok, "daemon", m)
                    }
                    (_, _, true) => {
                        let m = vault
                            .get(provider)
                            .ok()
                            .flatten()
                            .map(|s| mask_api_key(s.expose_secret()))
                            .unwrap_or_default();
                        (StatusIcon::Ok, "vault", m)
                    }
                    _ => (StatusIcon::Fail, "", String::new()),
                };
                if masked.is_empty() {
                    println!(
                        "  {} {} {:12} {}",
                        connector,
                        icon,
                        provider,
                        format!("→ nika keys set {provider}").dimmed()
                    );
                } else {
                    let source_label = match source {
                        "env" => format!("({source}) ⚠ lost on reboot"),
                        _ => format!("({source})"),
                    };
                    let models = top_models_for_provider(provider);
                    let model_hint = if models.is_empty() {
                        String::new()
                    } else {
                        format!("  {}", models.dimmed())
                    };
                    println!(
                        "  {} {} {:12} {} {}{}",
                        connector,
                        icon,
                        provider,
                        format!("[{masked}]").dimmed(),
                        source_label.dimmed(),
                        model_hint,
                    );
                }
            }
            // Always show mock + native
            println!();
            println!("  {} ({})", "Other".bold(), "always available".dimmed());
            println!("{}", nika_engine::display::separator(50));
            println!();
            println!(
                "  {} {} {:12} {}",
                tree_connector(false).dimmed(),
                StatusIcon::Ok,
                "mock",
                "deterministic test responses, no API key".dimmed()
            );
            println!(
                "  {} {} {:12} {}",
                tree_connector(true).dimmed(),
                StatusIcon::Fail,
                "native",
                "local GGUF models → nika model pull <name>".dimmed()
            );
            println!();
            println!("{}", hint("nika keys set <name>  Add or update an API key"));
            println!(
                "{}",
                hint("nika provider test <name> Test provider connection")
            );

            // Show custom endpoints from config
            let config = nika_engine::config::NikaConfig::load()
                .unwrap_or_default()
                .with_env();
            if !config.endpoints.is_empty() {
                println!();
                println!(
                    "  {} ({})",
                    "Custom Endpoints".bold(),
                    format!("{} configured", config.endpoints.len()).cyan()
                );
                println!("{}", nika_engine::display::separator(50));
                println!();
                for (name, ep) in &config.endpoints {
                    let model_info = ep
                        .model
                        .as_deref()
                        .map(|m| format!(" model={}", m))
                        .unwrap_or_default();
                    let key_info = if ep.api_key.is_some() {
                        "[key set]"
                    } else {
                        "[no auth]"
                    };
                    println!(
                        "  {} {} {:12} {} {}{}",
                        tree_connector(false).dimmed(),
                        StatusIcon::Ok,
                        name,
                        ep.base_url.dimmed(),
                        key_info.dimmed(),
                        model_info.dimmed(),
                    );
                }
                println!();
                println!(
                    "{}",
                    hint("Add endpoints in ~/.config/nika/config.toml under [endpoints.<name>]")
                );
            }

            Ok(())
        }

        ProviderAction::Test { provider, quiet } => {
            if quiet {
                // Quiet mode: no output, exit code only
                use nika_engine::core::provider_to_env_var;
                let env_var = provider_to_env_var(&provider).unwrap_or("UNKNOWN_API_KEY");
                let has_key = has_key_env_or_vault(&provider, env_var);
                if !has_key && provider != "native" {
                    return Err(NikaError::ProviderNotConfigured {
                        provider: provider.clone(),
                    });
                }
                if let Err(msg) = run_provider_test(&provider).await {
                    return Err(NikaError::ProviderApiError { message: msg });
                }
            } else {
                test_provider_connection(&provider).await?;
            }
            Ok(())
        }

        // v0: redirect old commands to nika keys
        ProviderAction::Set { .. } | ProviderAction::Delete { .. } => {
            eprintln!(
                "  {} Did you mean? {}",
                "\u{2717}".red().bold(),
                "nika keys set <provider>".cyan()
            );
            eprintln!("  Key management moved to: {}", "nika keys".bold());
            Err(NikaError::ConfigError {
                reason: "Command moved to: nika keys".to_string(),
            })
        }
        ProviderAction::Get { .. } => {
            eprintln!(
                "  {} Did you mean? {}",
                "\u{2717}".red().bold(),
                "nika keys".cyan()
            );
            eprintln!("  Key management moved to: {}", "nika keys".bold());
            Err(NikaError::ConfigError {
                reason: "Command moved to: nika keys".to_string(),
            })
        }
        ProviderAction::Migrate | ProviderAction::VaultReset => {
            eprintln!(
                "  {} This command was removed. Use: {}",
                "\u{2717}".red().bold(),
                "nika keys".cyan()
            );
            Err(NikaError::ConfigError {
                reason: "Command moved to: nika keys".to_string(),
            })
        }
    }
}

async fn test_provider_connection(provider: &str) -> Result<(), NikaError> {
    use nika_engine::core::provider_to_env_var;
    let env_var = provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY");
    let has_key = has_key_env_or_vault(provider, env_var);
    if !has_key && provider != "native" {
        println!(
            "{}",
            status_line(StatusIcon::Fail, &format!("No API key for {provider}"))
        );
        println!("{}", hint(&format!("nika keys set {provider}")));
        return Err(NikaError::ProviderNotConfigured {
            provider: provider.to_string(),
        });
    }
    let use_spinner = std::io::stderr().is_terminal();
    let result = if use_spinner {
        let spinner = cliclack::spinner();
        spinner.start(format!("Testing {provider}..."));
        let result = run_provider_test(provider).await;
        match &result {
            Ok(msg) => spinner.stop(format!("{} {msg}", StatusIcon::Ok)),
            Err(msg) => spinner.stop(format!("{} {msg}", StatusIcon::Fail)),
        }
        result
    } else {
        eprintln!("Testing {provider}...");
        let result = run_provider_test(provider).await;
        match &result {
            Ok(msg) => eprintln!("  {} {msg}", StatusIcon::Ok),
            Err(msg) => eprintln!("  {} {msg}", StatusIcon::Fail),
        }
        result
    };
    result
        .map(|_| ())
        .map_err(|msg| NikaError::ProviderApiError { message: msg })
}

pub async fn run_provider_test(provider: &str) -> Result<String, String> {
    use nika_engine::provider::rig::RigProvider;
    let prov = match provider {
        "anthropic" => RigProvider::claude(),
        "openai" => RigProvider::openai(),
        "mistral" => RigProvider::mistral(),
        "groq" => RigProvider::groq(),
        "deepseek" => RigProvider::deepseek(),
        "gemini" => RigProvider::gemini(),
        "xai" => RigProvider::xai(),
        "native" => {
            #[cfg(feature = "native-inference")]
            {
                RigProvider::native()
            }
            #[cfg(not(feature = "native-inference"))]
            {
                return Err("Native inference not available".into());
            }
        }
        _ => return Err(format!("Unknown provider: {provider}")),
    };
    match prov.infer("Say 'OK' if you can hear me.", None, None).await {
        Ok(response) => {
            let truncated: String = response.chars().take(80).collect();
            Ok(format!("Connection OK — {truncated}"))
        }
        Err(e) => Err(format!("Connection failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_anthropic_key() {
        assert_eq!(
            detect_provider_from_key("sk-ant-api03-test"),
            Some("anthropic")
        );
    }
    #[test]
    fn detect_openai_key() {
        assert_eq!(detect_provider_from_key("sk-proj-abc123"), Some("openai"));
        assert_eq!(
            detect_provider_from_key("sk-svcacct-abc123"),
            Some("openai")
        );
    }
    #[test]
    fn detect_groq_key() {
        assert_eq!(detect_provider_from_key("gsk_abc123"), Some("groq"));
    }
    #[test]
    fn detect_xai_key() {
        assert_eq!(detect_provider_from_key("xai-abc123"), Some("xai"));
    }
    #[test]
    fn detect_unknown_key() {
        assert_eq!(detect_provider_from_key("unknown-key-format"), None);
    }
    #[test]
    fn detect_empty_key() {
        assert_eq!(detect_provider_from_key(""), None);
    }
    #[test]
    fn llm_provider_ids_includes_all_seven() {
        let ids = llm_provider_ids();
        assert!(ids.len() >= 7);
        assert!(ids.contains(&"anthropic"));
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"xai"));
    }
}
