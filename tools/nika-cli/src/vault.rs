//! Vault management subcommand handler.
//!
//! `nika vault` provides credential management: multi-field credential storage,
//! import from env/doppler, and export.

use clap::Subcommand;
use colored::Colorize;

use nika_engine::display::{hint, separator, status_line, tree_connector, StatusIcon};
use nika_engine::error::NikaError;
use nika_vault::NikaVault;

/// Vault management actions.
#[derive(Subcommand, Debug)]
pub enum VaultAction {
    /// Store a multi-field credential for a service
    Set {
        /// Service name (e.g. stripe, twilio, elevenlabs)
        service: String,
        /// Field key=value pair (repeatable: --field secret=sk_xxx --field webhook=whsec_xxx)
        #[arg(long, value_parser = parse_field)]
        field: Vec<(String, String)>,
    },

    /// List all stored services and field counts
    List,

    /// Check if stored credentials are accessible
    Check,

    /// Export credentials (redacted by default)
    Export {
        /// Output format: env, json
        #[arg(long, default_value = "env")]
        format: String,
        /// Reveal actual values (dangerous — visible in terminal history)
        #[arg(long)]
        reveal: bool,
    },

    /// Import credentials from an external source
    Import {
        /// Source: "env" (scan environment) or "doppler" (doppler secrets download)
        #[arg(long)]
        from: String,
    },
}

/// Parse a key=value field argument.
fn parse_field(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid field format '{s}', expected KEY=VALUE"))?;
    let key = s[..pos].to_string();
    let value = s[pos + 1..].to_string();
    if key.is_empty() {
        return Err("field key cannot be empty".to_string());
    }
    Ok((key, value))
}

/// Vault key for a service field: `vault:<service>:<field>`.
fn vault_key(service: &str, field: &str) -> String {
    format!("vault:{service}:{field}")
}

/// Extract (service, field) from a vault key, if it matches the pattern.
fn parse_vault_key(key: &str) -> Option<(String, String)> {
    let rest = key.strip_prefix("vault:")?;
    let colon = rest.find(':')?;
    let service = rest[..colon].to_string();
    let field = rest[colon + 1..].to_string();
    if service.is_empty() || field.is_empty() {
        return None;
    }
    Some((service, field))
}

/// Get vault instance.
fn get_vault() -> NikaVault {
    let nika_home = dirs::home_dir().unwrap_or_default().join(".nika");
    NikaVault::new(&nika_home.join("secrets"))
}

/// Env var patterns that suggest credentials.
const CREDENTIAL_SUFFIXES: &[&str] = &["_API_KEY", "_TOKEN", "_SECRET", "_KEY"];

/// Check if an env var name looks like a credential.
fn is_credential_env_var(name: &str) -> bool {
    CREDENTIAL_SUFFIXES
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

pub async fn handle_vault_command(action: VaultAction, _quiet: bool) -> Result<(), NikaError> {
    match action {
        VaultAction::Set { service, field } => {
            if field.is_empty() {
                return Err(NikaError::ValidationError {
                    reason: "At least one --field KEY=VALUE is required".into(),
                });
            }

            let vault = get_vault();
            for (key, value) in &field {
                let vk = vault_key(&service, key);
                vault.set(&vk, value).map_err(|e| NikaError::ConfigError {
                    reason: format!("Failed to store {service}.{key}: {e}"),
                })?;
            }

            println!(
                "  {} {} fields stored for service {}",
                StatusIcon::Ok,
                field.len(),
                service.bold()
            );
            for (i, (key, _)) in field.iter().enumerate() {
                let is_last = i == field.len() - 1;
                let connector = tree_connector(is_last).dimmed();
                println!("  {} {}", connector, key);
            }
            Ok(())
        }

        VaultAction::List => {
            let vault = get_vault();
            let all_keys = vault.list().map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to list vault: {e}"),
            })?;

            // Group by service
            let mut services: std::collections::BTreeMap<String, Vec<String>> =
                std::collections::BTreeMap::new();
            for key in &all_keys {
                if let Some((service, field)) = parse_vault_key(key) {
                    services.entry(service).or_default().push(field);
                }
                // Skip non-vault keys (provider keys like "anthropic")
            }

            // Also show legacy provider keys
            let provider_keys: Vec<&String> = all_keys
                .iter()
                .filter(|k| !k.starts_with("vault:"))
                .collect();

            if services.is_empty() && provider_keys.is_empty() {
                println!(
                    "{}",
                    status_line(StatusIcon::Info, "Vault is empty — no credentials stored")
                );
                println!("{}", hint("nika vault set <service> --field key=value"));
                return Ok(());
            }

            if !services.is_empty() {
                println!(
                    "\n  {} ({})",
                    "Vault Services".bold(),
                    format!("{} services", services.len()).cyan()
                );
                println!("{}", separator(50));
                println!();
                let service_list: Vec<_> = services.iter().collect();
                for (i, (service, fields)) in service_list.iter().enumerate() {
                    let is_last = i == service_list.len() - 1 && provider_keys.is_empty();
                    let connector = tree_connector(is_last).dimmed();
                    println!(
                        "  {} {} {:20} {}",
                        connector,
                        StatusIcon::Ok,
                        service,
                        format!("{} fields", fields.len()).dimmed()
                    );
                }
            }

            if !provider_keys.is_empty() {
                println!(
                    "\n  {} ({})",
                    "Provider Keys".bold(),
                    format!("{} keys", provider_keys.len()).cyan()
                );
                println!("{}", separator(50));
                println!();
                for (i, key) in provider_keys.iter().enumerate() {
                    let is_last = i == provider_keys.len() - 1;
                    let connector = tree_connector(is_last).dimmed();
                    println!("  {} {} {}", connector, StatusIcon::Ok, key);
                }
            }

            println!();
            Ok(())
        }

        VaultAction::Check => {
            let vault = get_vault();
            let all_keys = vault.list().map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to list vault: {e}"),
            })?;

            let mut services: std::collections::BTreeMap<String, Vec<String>> =
                std::collections::BTreeMap::new();
            for key in &all_keys {
                if let Some((service, field)) = parse_vault_key(key) {
                    services.entry(service).or_default().push(field);
                }
            }

            if services.is_empty() {
                println!(
                    "{}",
                    status_line(StatusIcon::Info, "No vault credentials to check")
                );
                return Ok(());
            }

            println!(
                "\n  {} Checking {} services...\n",
                StatusIcon::Info,
                services.len()
            );

            let mut all_ok = true;
            for (service, fields) in &services {
                let mut service_ok = true;
                for field in fields {
                    let vk = vault_key(service, field);
                    match vault.get(&vk) {
                        Ok(Some(_)) => {} // accessible
                        _ => {
                            println!(
                                "  {} {}.{}: {}",
                                StatusIcon::Fail,
                                service,
                                field,
                                "cannot decrypt".red()
                            );
                            service_ok = false;
                            all_ok = false;
                        }
                    }
                }
                if service_ok {
                    println!(
                        "  {} {}: {} fields accessible",
                        StatusIcon::Ok,
                        service.bold(),
                        fields.len()
                    );
                }
            }

            if all_ok {
                println!(
                    "\n{}",
                    status_line(StatusIcon::Ok, "All credentials accessible")
                );
            }
            Ok(())
        }

        VaultAction::Export { format, reveal } => {
            let vault = get_vault();
            let all_keys = vault.list().map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to list vault: {e}"),
            })?;

            match format.as_str() {
                "env" => {
                    for key in &all_keys {
                        if let Some((service, field)) = parse_vault_key(key) {
                            let env_name =
                                format!("{}_{}", service.to_uppercase(), field.to_uppercase());
                            if reveal {
                                if let Ok(Some(secret)) = vault.get(key) {
                                    use secrecy::ExposeSecret;
                                    // SAFETY: user explicitly requested --reveal flag to export
                                    // cleartext secrets for shell eval. This is intentional.
                                    println!("{}={}", env_name, secret.expose_secret());
                                }
                            } else {
                                println!("{}=****", env_name);
                            }
                        }
                    }
                }
                "json" => {
                    let mut map = serde_json::Map::new();
                    for key in &all_keys {
                        if let Some((service, field)) = parse_vault_key(key) {
                            let service_map = map.entry(service).or_insert_with(|| {
                                serde_json::Value::Object(serde_json::Map::new())
                            });
                            if let serde_json::Value::Object(ref mut m) = service_map {
                                if reveal {
                                    if let Ok(Some(secret)) = vault.get(key) {
                                        use secrecy::ExposeSecret;
                                        m.insert(
                                            field,
                                            serde_json::Value::String(
                                                secret.expose_secret().to_string(),
                                            ),
                                        );
                                    }
                                } else {
                                    m.insert(field, serde_json::Value::String("****".to_string()));
                                }
                            }
                        }
                    }
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::Value::Object(map))
                            .unwrap_or_else(|_| "{}".to_string())
                    );
                }
                other => {
                    return Err(NikaError::ValidationError {
                        reason: format!("Unknown export format '{other}'. Valid: env, json"),
                    });
                }
            }
            Ok(())
        }

        VaultAction::Import { from } => match from.as_str() {
            "env" => import_from_env().await,
            "doppler" => import_from_doppler().await,
            other => Err(NikaError::ValidationError {
                reason: format!("Unknown import source '{other}'. Valid: env, doppler"),
            }),
        },
    }
}

/// Scan environment variables matching credential patterns and import them.
async fn import_from_env() -> Result<(), NikaError> {
    let vault = get_vault();
    let mut imported = 0usize;

    println!(
        "\n  {} Scanning environment for credentials...\n",
        StatusIcon::Info
    );

    let mut vars: Vec<(String, String)> = std::env::vars()
        .filter(|(k, v)| is_credential_env_var(k) && !v.is_empty())
        .collect();
    vars.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, value) in &vars {
        // Derive service name from env var: STRIPE_API_KEY → stripe, STRIPE_SECRET → stripe
        let service = derive_service_from_env(name);
        let field = derive_field_from_env(name);
        let vk = vault_key(&service, &field);

        // Check if already in vault
        if vault.get(&vk).ok().flatten().is_some() {
            println!(
                "  {} {}: {} (already stored)",
                tree_connector(false).dimmed(),
                StatusIcon::Skip,
                name.dimmed()
            );
            continue;
        }

        vault.set(&vk, value).map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to import {name}: {e}"),
        })?;
        imported += 1;
        println!(
            "  {} {} {}: {} → {}.{}",
            tree_connector(false).dimmed(),
            StatusIcon::Ok,
            name,
            "imported".green(),
            service,
            field
        );
    }

    println!(
        "\n{}",
        status_line(
            StatusIcon::Ok,
            &format!("{imported} credentials imported from environment")
        )
    );
    Ok(())
}

/// Import from Doppler: `doppler secrets download --format json`.
async fn import_from_doppler() -> Result<(), NikaError> {
    use std::process::Command;

    // Check if doppler CLI is available
    if which::which("doppler").is_err() {
        return Err(NikaError::ConfigError {
            reason: "Doppler CLI not found. Install: https://docs.doppler.com/docs/install-cli"
                .into(),
        });
    }

    println!(
        "\n  {} Fetching secrets from Doppler...\n",
        StatusIcon::Info
    );

    let output = Command::new("doppler")
        .args(["secrets", "download", "--format", "json", "--no-file"])
        .output()
        .map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to run doppler: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NikaError::ConfigError {
            reason: format!("Doppler failed: {stderr}"),
        });
    }

    let secrets: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| NikaError::ConfigError {
            reason: format!("Failed to parse Doppler output: {e}"),
        })?;

    let vault = get_vault();
    let mut imported = 0usize;

    if let serde_json::Value::Object(map) = secrets {
        for (key, value) in &map {
            if let serde_json::Value::String(val) = value {
                if val.is_empty() {
                    continue;
                }
                let service = derive_service_from_env(key);
                let field = derive_field_from_env(key);
                let vk = vault_key(&service, &field);

                vault.set(&vk, val).map_err(|e| NikaError::ConfigError {
                    reason: format!("Failed to import {key}: {e}"),
                })?;
                imported += 1;
                println!(
                    "  {} {} {}: {} → {}.{}",
                    tree_connector(false).dimmed(),
                    StatusIcon::Ok,
                    key,
                    "imported".green(),
                    service,
                    field
                );
            }
        }
    }

    println!(
        "\n{}",
        status_line(
            StatusIcon::Ok,
            &format!("{imported} credentials imported from Doppler")
        )
    );
    Ok(())
}

/// Derive a service name from an env var: `STRIPE_API_KEY` → `stripe`.
fn derive_service_from_env(name: &str) -> String {
    // Strip known suffixes and lowercase the rest
    let stripped = CREDENTIAL_SUFFIXES
        .iter()
        .fold(name.to_string(), |acc, suffix| {
            if let Some(prefix) = acc.strip_suffix(suffix) {
                prefix.to_string()
            } else {
                acc
            }
        });
    stripped.to_lowercase()
}

/// Derive a field name from an env var: `STRIPE_API_KEY` → `api_key`.
fn derive_field_from_env(name: &str) -> String {
    for suffix in CREDENTIAL_SUFFIXES {
        if let Some(_prefix) = name.strip_suffix(suffix) {
            // Remove leading underscore from suffix
            return suffix.strip_prefix('_').unwrap_or(suffix).to_lowercase();
        }
    }
    "key".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_field_valid() {
        let (k, v) = parse_field("secret=sk_live_xxx").unwrap();
        assert_eq!(k, "secret");
        assert_eq!(v, "sk_live_xxx");
    }

    #[test]
    fn parse_field_value_with_equals() {
        let (k, v) = parse_field("data=a=b=c").unwrap();
        assert_eq!(k, "data");
        assert_eq!(v, "a=b=c");
    }

    #[test]
    fn parse_field_empty_value() {
        let (k, v) = parse_field("key=").unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, "");
    }

    #[test]
    fn parse_field_no_equals() {
        let err = parse_field("noequals").unwrap_err();
        assert!(err.contains("KEY=VALUE"));
    }

    #[test]
    fn parse_field_empty_key() {
        let err = parse_field("=value").unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn vault_key_format() {
        assert_eq!(vault_key("stripe", "secret"), "vault:stripe:secret");
        assert_eq!(
            vault_key("elevenlabs", "api_key"),
            "vault:elevenlabs:api_key"
        );
    }

    #[test]
    fn parse_vault_key_valid() {
        let (s, f) = parse_vault_key("vault:stripe:secret").unwrap();
        assert_eq!(s, "stripe");
        assert_eq!(f, "secret");
    }

    #[test]
    fn parse_vault_key_with_nested_colons() {
        let (s, f) = parse_vault_key("vault:stripe:webhook:extra").unwrap();
        assert_eq!(s, "stripe");
        assert_eq!(f, "webhook:extra");
    }

    #[test]
    fn parse_vault_key_not_vault_prefix() {
        assert!(parse_vault_key("anthropic").is_none());
        assert!(parse_vault_key("some:other:key").is_none());
    }

    #[test]
    fn parse_vault_key_empty_parts() {
        assert!(parse_vault_key("vault::field").is_none());
        assert!(parse_vault_key("vault:service:").is_none());
    }

    #[test]
    fn is_credential_env_var_matches() {
        assert!(is_credential_env_var("STRIPE_API_KEY"));
        assert!(is_credential_env_var("GITHUB_TOKEN"));
        assert!(is_credential_env_var("MY_SECRET"));
        assert!(is_credential_env_var("WEBHOOK_KEY"));
    }

    #[test]
    fn is_credential_env_var_rejects() {
        assert!(!is_credential_env_var("PATH"));
        assert!(!is_credential_env_var("HOME"));
        assert!(!is_credential_env_var("NIKA_HOME"));
        assert!(!is_credential_env_var("NODE_ENV"));
    }

    #[test]
    fn derive_service_from_env_strips_suffix() {
        assert_eq!(derive_service_from_env("STRIPE_API_KEY"), "stripe");
        assert_eq!(derive_service_from_env("GITHUB_TOKEN"), "github");
        assert_eq!(derive_service_from_env("MY_APP_SECRET"), "my_app");
        assert_eq!(derive_service_from_env("WEBHOOK_KEY"), "webhook");
    }

    #[test]
    fn derive_field_from_env_extracts_suffix() {
        assert_eq!(derive_field_from_env("STRIPE_API_KEY"), "api_key");
        assert_eq!(derive_field_from_env("GITHUB_TOKEN"), "token");
        assert_eq!(derive_field_from_env("MY_APP_SECRET"), "secret");
        assert_eq!(derive_field_from_env("WEBHOOK_KEY"), "key");
    }

    #[test]
    fn derive_field_from_env_no_match() {
        assert_eq!(derive_field_from_env("RANDOM_VAR"), "key");
    }

    // ── Integration tests with temp vault ──────────────────────────────────

    fn test_vault() -> (tempfile::TempDir, NikaVault) {
        let dir = tempfile::TempDir::new().unwrap();
        // SAFETY: test isolation
        unsafe { std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-vault-cli") };
        let vault = NikaVault::new(dir.path());
        (dir, vault)
    }

    #[test]
    fn vault_set_and_list_roundtrip() {
        let (_dir, vault) = test_vault();

        // Store multi-field credential
        vault
            .set(&vault_key("stripe", "secret"), "sk_live_xxx")
            .unwrap();
        vault
            .set(&vault_key("stripe", "webhook"), "whsec_xxx")
            .unwrap();
        vault.set(&vault_key("twilio", "sid"), "AC_xxx").unwrap();

        // List all and group by service
        let all_keys = vault.list().unwrap();
        let vault_keys: Vec<_> = all_keys.iter().filter_map(|k| parse_vault_key(k)).collect();

        // Should have 3 vault entries
        assert_eq!(vault_keys.len(), 3);

        // Group by service
        let mut services: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for (service, field) in &vault_keys {
            services
                .entry(service.clone())
                .or_default()
                .push(field.clone());
        }

        assert_eq!(services.len(), 2); // stripe, twilio
        assert_eq!(services["stripe"].len(), 2);
        assert_eq!(services["twilio"].len(), 1);
    }

    #[test]
    fn vault_get_specific_field() {
        use secrecy::ExposeSecret;
        let (_dir, vault) = test_vault();

        vault
            .set(&vault_key("stripe", "secret"), "sk_live_xxx")
            .unwrap();
        vault
            .set(&vault_key("stripe", "webhook"), "whsec_xxx")
            .unwrap();

        let secret = vault.get(&vault_key("stripe", "secret")).unwrap().unwrap();
        assert_eq!(secret.expose_secret(), "sk_live_xxx");

        let webhook = vault.get(&vault_key("stripe", "webhook")).unwrap().unwrap();
        assert_eq!(webhook.expose_secret(), "whsec_xxx");
    }

    #[test]
    fn vault_coexists_with_provider_keys() {
        let (_dir, vault) = test_vault();

        // Provider key (legacy format)
        vault.set("anthropic", "sk-ant-test").unwrap();

        // Vault credential
        vault
            .set(&vault_key("stripe", "secret"), "sk_live_xxx")
            .unwrap();

        let all = vault.list().unwrap();
        assert_eq!(all.len(), 2);

        // Can distinguish vault from provider keys
        let vault_count = all.iter().filter(|k| k.starts_with("vault:")).count();
        let provider_count = all.iter().filter(|k| !k.starts_with("vault:")).count();
        assert_eq!(vault_count, 1);
        assert_eq!(provider_count, 1);
    }
}
