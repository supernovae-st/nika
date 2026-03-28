//! Provider management subcommand handler

use clap::Subcommand;
use colored::Colorize;
use std::io::IsTerminal;

use nika_engine::display::{hint, status_line, tree_connector, StatusIcon};
use nika_engine::error::NikaError;

/// Provider management actions
#[derive(Subcommand)]
pub enum ProviderAction {
    /// List all providers and their status
    List,

    /// Set API key for a provider (stored in system keychain)
    ///
    /// Prefer interactive mode (no key argument) — the key is masked during input.
    /// Passing the key as an argument exposes it in the process list (ps aux).
    Set {
        /// Provider name (anthropic, openai, mistral, groq, deepseek, gemini, xai)
        provider: Option<String>,
        /// API key — prefer interactive mode (omit this to enter with hidden input)
        #[arg(hide = true)]
        key: Option<String>,
        /// Skip connection test after storing
        #[arg(long)]
        no_test: bool,
    },

    /// Get API key for a provider (masked for security)
    Get {
        /// Provider name
        provider: String,
    },

    /// Delete API key for a provider
    Delete {
        /// Provider name
        provider: String,
    },

    /// Migrate API keys from environment variables to keychain
    Migrate,

    /// Test connection to a provider
    Test {
        /// Provider name
        provider: String,
        /// Suppress output — exit code only (for scripts/CI)
        #[arg(short, long)]
        quiet: bool,
    },
}

const KEY_PREFIXES: &[(&str, &str)] = &[
    ("sk-ant-", "anthropic"),
    ("sk-proj-", "openai"),
    ("sk-svcacct-", "openai"),
    ("gsk_", "groq"),
    ("xai-", "xai"),
];

pub fn detect_provider_from_key(key: &str) -> Option<&'static str> {
    KEY_PREFIXES
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

const PROVIDER_DESCRIPTIONS: &[(&str, &str)] = &[
    ("anthropic", "Claude — recommended for reasoning & code"),
    ("openai", "GPT-4o, GPT-4.1, o3, o4-mini"),
    ("mistral", "Mistral Large, Small, Codestral"),
    ("groq", "Llama 4, Mixtral — ultra-fast inference"),
    ("deepseek", "DeepSeek Chat, Reasoner — budget-friendly"),
    ("gemini", "Gemini 2.5 Pro, Flash — large context"),
    ("xai", "Grok 3 — real-time knowledge"),
];

fn provider_description(id: &str) -> &'static str {
    PROVIDER_DESCRIPTIONS
        .iter()
        .find(|(p, _)| *p == id)
        .map(|(_, d)| *d)
        .unwrap_or("")
}

pub async fn handle_provider_command(
    action: ProviderAction,
    _quiet: bool,
) -> Result<(), NikaError> {
    use nika_engine::core::provider_to_env_var;
    use nika_engine::secrets::{
        mask_api_key, migrate_env_to_keyring, validate_key_format, NikaKeyring,
    };

    let all_providers = llm_provider_ids();

    match action {
        ProviderAction::List => {
            let mut configured = 0usize;
            let total = all_providers.len();
            for provider in &all_providers {
                let env_var = provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY");
                // Check env var FIRST to avoid unnecessary keychain access
                // (keychain triggers macOS security prompts for unconfigured providers)
                let has_env = std::env::var(env_var)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);
                if has_env || NikaKeyring::exists(provider) {
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
                // Check env var FIRST to minimize keychain access (avoids macOS popups)
                let has_env = std::env::var(env_var)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);
                // Only query keychain if env var is not set
                let has_keychain = if has_env { false } else { NikaKeyring::exists(provider) };
                let is_last = i == all_providers.len() - 1;
                let connector = tree_connector(is_last).dimmed();
                let (icon, source, masked) = match (has_env, has_keychain) {
                    (true, _) => {
                        let m = std::env::var(env_var)
                            .ok()
                            .map(|k| mask_api_key(&k))
                            .unwrap_or_default();
                        (StatusIcon::Ok, "env", m)
                    }
                    (false, true) => {
                        let m = NikaKeyring::get_masked(provider).unwrap_or_default();
                        (StatusIcon::Ok, "keychain", m)
                    }
                    (false, false) => (StatusIcon::Fail, "", String::new()),
                };
                if masked.is_empty() {
                    println!(
                        "  {} {} {:12} {}",
                        connector,
                        icon,
                        provider,
                        format!("→ nika provider set {provider}").dimmed()
                    );
                } else {
                    println!(
                        "  {} {} {:12} {} {}",
                        connector,
                        icon,
                        provider,
                        format!("[{masked}]").dimmed(),
                        format!("({source})").dimmed()
                    );
                }
            }
            println!();
            println!(
                "{}",
                hint("nika provider set <name>  Add or update an API key")
            );
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

        ProviderAction::Set {
            provider,
            key,
            no_test,
        } => {
            let is_tty = std::io::stdin().is_terminal();
            let provider = match provider {
                Some(p) => {
                    if !all_providers.contains(&p.as_str()) {
                        return Err(NikaError::ValidationError {
                            reason: format!(
                                "Unknown provider '{}'. Valid: {}",
                                p,
                                all_providers.join(", ")
                            ),
                        });
                    }
                    p
                }
                None if is_tty => {
                    let items: Vec<(String, String, String)> = all_providers
                        .iter()
                        .map(|&p| {
                            (
                                p.to_string(),
                                p.to_string(),
                                provider_description(p).to_string(),
                            )
                        })
                        .collect();
                    cliclack::select("Which provider?")
                        .items(
                            &items
                                .iter()
                                .map(|(v, l, h)| (v.as_str(), l.as_str(), h.as_str()))
                                .collect::<Vec<_>>(),
                        )
                        .interact()
                        .map_err(|e| {
                            NikaError::IoError(std::io::Error::new(
                                std::io::ErrorKind::Interrupted,
                                format!("Cancelled: {e}"),
                            ))
                        })?
                        .to_string()
                }
                None => {
                    return Err(NikaError::ValidationError {
                        reason: "Provider name required in non-interactive mode".into(),
                    })
                }
            };
            let api_key = match key {
                Some(k) => {
                    eprintln!(
                        "  {} API key passed as argument — visible in process list (ps aux)",
                        StatusIcon::Warn
                    );
                    eprintln!(
                        "{}",
                        hint("Prefer: nika provider set (interactive, masked input)")
                    );
                    k
                }
                None if is_tty => cliclack::password(format!("Paste your {provider} API key:"))
                    .mask('•')
                    .interact()
                    .map_err(|e| {
                        NikaError::IoError(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            format!("Cancelled: {e}"),
                        ))
                    })?,
                None => {
                    return Err(NikaError::ValidationError {
                        reason: "API key required. Usage: nika provider set <provider> <key>"
                            .into(),
                    })
                }
            };
            if let Some(detected) = detect_provider_from_key(&api_key) {
                if detected != provider {
                    println!(
                        "  {} Key prefix suggests {} but storing as {}",
                        StatusIcon::Warn,
                        detected.cyan(),
                        provider.cyan()
                    );
                }
            }
            if let Err(e) = validate_key_format(&provider, &api_key) {
                return Err(NikaError::ValidationError { reason: e });
            }
            // Try daemon IPC first (faster, avoids keychain popup)
            #[cfg(unix)]
            {
                let sock = nika_daemon::daemon_socket_path();
                if sock.exists() {
                    let client = nika_daemon::DaemonClient::new(&sock);
                    if client.set_secret(&provider, &api_key).await.is_ok() {
                        let env_var_name =
                            provider_to_env_var(&provider).unwrap_or("UNKNOWN_API_KEY");
                        // SAFETY: no concurrent tasks reading env vars at this point
                        unsafe { std::env::set_var(env_var_name, &api_key) };
                        println!(
                            "  {} API key for {} stored via daemon",
                            StatusIcon::Ok,
                            provider.bold()
                        );
                        if !no_test
                            && is_tty
                            && cliclack::confirm("Test connection now?")
                                .initial_value(true)
                                .interact()
                                .unwrap_or(false)
                        {
                            test_provider_connection(&provider).await;
                        }
                        return Ok(());
                    }
                    // Fall through to direct keyring
                }
            }
            NikaKeyring::set(&provider, &api_key).map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to store key: {e}"),
            })?;
            println!(
                "  {} API key for {} stored in system keychain",
                StatusIcon::Ok,
                provider.bold()
            );
            if !no_test
                && is_tty
                && cliclack::confirm("Test connection now?")
                    .initial_value(true)
                    .interact()
                    .unwrap_or(false)
            {
                let env_var = provider_to_env_var(&provider).unwrap_or("UNKNOWN_API_KEY");
                // SAFETY: no concurrent tasks reading env vars at this point
                unsafe { std::env::set_var(env_var, &api_key) };
                test_provider_connection(&provider).await;
            }
            Ok(())
        }

        ProviderAction::Get { provider } => {
            let env_var = provider_to_env_var(&provider).unwrap_or("UNKNOWN_API_KEY");
            match NikaKeyring::get_masked(&provider) {
                Some(masked) => println!(
                    "  {} {}: {} {}",
                    StatusIcon::Ok,
                    provider.bold(),
                    masked,
                    "(keychain)".dimmed()
                ),
                None => match std::env::var(env_var) {
                    Ok(key) if !key.is_empty() => println!(
                        "  {} {}: {} {}",
                        StatusIcon::Ok,
                        provider.bold(),
                        mask_api_key(&key),
                        "(env)".dimmed()
                    ),
                    _ => {
                        println!(
                            "{}",
                            status_line(StatusIcon::Fail, &format!("{provider}: not configured"))
                        );
                        println!("{}", hint(&format!("nika provider set {provider}")));
                    }
                },
            }
            Ok(())
        }

        ProviderAction::Delete { provider } => {
            // Try daemon IPC first
            #[cfg(unix)]
            {
                let sock = nika_daemon::daemon_socket_path();
                if sock.exists() {
                    let client = nika_daemon::DaemonClient::new(&sock);
                    if client.delete_secret(&provider).await.is_ok() {
                        println!(
                            "  {} API key for {} deleted via daemon",
                            StatusIcon::Ok,
                            provider.bold()
                        );
                        return Ok(());
                    }
                    // Fall through to direct keyring
                }
            }
            match NikaKeyring::delete(&provider) {
                Ok(()) => println!(
                    "  {} API key for {} deleted from keychain",
                    StatusIcon::Ok,
                    provider.bold()
                ),
                Err(e) => {
                    return Err(NikaError::ConfigError {
                        reason: format!("Failed to delete key: {e}"),
                    })
                }
            }
            Ok(())
        }

        ProviderAction::Migrate => {
            println!(
                "{}",
                "Migrating API keys from environment variables...".cyan()
            );
            let report = migrate_env_to_keyring();
            println!();
            println!("{}", report.summary());
            Ok(())
        }

        ProviderAction::Test { provider, quiet } => {
            if quiet {
                // Quiet mode: no output, exit code only
                use nika_engine::core::provider_to_env_var;
                use nika_engine::secrets::NikaKeyring;
                let env_var = provider_to_env_var(&provider).unwrap_or("UNKNOWN_API_KEY");
                let has_key = NikaKeyring::exists(&provider)
                    || std::env::var(env_var).is_ok_and(|v| !v.is_empty());
                if !has_key && provider != "native" {
                    std::process::exit(1);
                }
                if run_provider_test(&provider).await.is_err() {
                    std::process::exit(1);
                }
            } else {
                test_provider_connection(&provider).await;
            }
            Ok(())
        }
    }
}

async fn test_provider_connection(provider: &str) {
    use nika_engine::core::provider_to_env_var;
    use nika_engine::secrets::NikaKeyring;
    let env_var = provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY");
    let has_key =
        NikaKeyring::exists(provider) || std::env::var(env_var).is_ok_and(|v| !v.is_empty());
    if !has_key && provider != "native" {
        println!(
            "{}",
            status_line(StatusIcon::Fail, &format!("No API key for {provider}"))
        );
        println!("{}", hint(&format!("nika provider set {provider}")));
        return;
    }
    let use_spinner = std::io::stderr().is_terminal();
    if use_spinner {
        let spinner = cliclack::spinner();
        spinner.start(format!("Testing {provider}..."));
        let result = run_provider_test(provider).await;
        match result {
            Ok(msg) => spinner.stop(format!("{} {msg}", StatusIcon::Ok)),
            Err(msg) => spinner.stop(format!("{} {msg}", StatusIcon::Fail)),
        }
    } else {
        eprintln!("Testing {provider}...");
        match run_provider_test(provider).await {
            Ok(msg) => eprintln!("  {} {msg}", StatusIcon::Ok),
            Err(msg) => eprintln!("  {} {msg}", StatusIcon::Fail),
        }
    }
}

async fn run_provider_test(provider: &str) -> Result<String, String> {
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
    match prov.infer("Say 'OK' if you can hear me.", None).await {
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
    fn provider_description_known() {
        assert!(provider_description("anthropic").contains("Claude"));
    }
    #[test]
    fn provider_description_unknown() {
        assert!(provider_description("nonexistent").is_empty());
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
