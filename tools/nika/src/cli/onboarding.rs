//! First-run onboarding wizard for API key setup.

use colored::Colorize;
use std::io::IsTerminal;

use nika::display::{hint, StatusIcon};
use nika::error::NikaError;

use super::provider::detect_provider_from_key;

const ONBOARDING_PROVIDERS: &[(&str, &str)] = &[
    ("anthropic", "Claude — best for reasoning, code, analysis"),
    ("openai", "GPT-4o, o3 — versatile, large ecosystem"),
    ("groq", "Llama 4 — free tier, ultra-fast inference"),
    ("mistral", "Mistral — European, strong multilingual"),
    ("deepseek", "DeepSeek — budget-friendly, good reasoning"),
    ("gemini", "Gemini 2.5 — large context, multimodal"),
    ("xai", "Grok 3 — real-time knowledge"),
];

#[allow(dead_code)]
pub fn has_any_provider_key() -> bool {
    use nika::core::{ProviderCategory, KNOWN_PROVIDERS};
    KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == ProviderCategory::Llm)
        .any(|p| {
            std::env::var(p.env_var)
                .map(|v| !v.is_empty())
                .unwrap_or(false)
        })
}

pub async fn run_onboarding_wizard() -> Result<bool, NikaError> {
    if !std::io::stdin().is_terminal() {
        println!();
        println!("  {} No API keys configured", StatusIcon::Warn);
        println!(
            "{}",
            hint("Set an API key: nika provider set <provider> <key>")
        );
        println!("{}", hint("Or run interactively: nika setup"));
        return Ok(false);
    }

    cliclack::intro(
        "Welcome to Nika! Let's set up your first provider."
            .bold()
            .to_string(),
    )
    .map_err(|e| {
        NikaError::IoError(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            format!("Cancelled: {e}"),
        ))
    })?;

    let items: Vec<(String, String, String)> = ONBOARDING_PROVIDERS
        .iter()
        .map(|(id, desc)| (id.to_string(), id.to_string(), desc.to_string()))
        .collect();

    let provider: String = cliclack::select("Which provider would you like to use?")
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
        .to_string();

    let api_key: String = cliclack::password(format!("Paste your {} API key:", provider))
        .mask('•')
        .interact()
        .map_err(|e| {
            NikaError::IoError(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                format!("Cancelled: {e}"),
            ))
        })?;

    if api_key.is_empty() {
        cliclack::outro(
            "No key entered. You can always run: nika setup"
                .dimmed()
                .to_string(),
        )
        .ok();
        return Ok(false);
    }

    if let Some(detected) = detect_provider_from_key(&api_key) {
        if detected != provider {
            let _ = cliclack::note(
                "Key prefix mismatch",
                format!("Key looks like {detected}, but you selected {provider}."),
            );
        }
    }

    use nika::secrets::validate_key_format;
    if let Err(e) = validate_key_format(&provider, &api_key) {
        cliclack::outro(format!("{} Invalid key format: {e}", "✗".red())).ok();
        return Ok(false);
    }

    use nika::secrets::NikaKeyring;
    NikaKeyring::set(&provider, &api_key).map_err(|e| NikaError::ConfigError {
        reason: format!("Failed to store key: {e}"),
    })?;

    use nika::core::provider_to_env_var;
    let env_var = provider_to_env_var(&provider).unwrap_or("UNKNOWN_API_KEY");
    // SAFETY: single-threaded at this point (before async tasks are spawned)
    unsafe { std::env::set_var(env_var, &api_key) };

    let do_test: bool = cliclack::confirm("Test connection?")
        .initial_value(true)
        .interact()
        .unwrap_or(false);

    if do_test {
        let spinner = cliclack::spinner();
        spinner.start(format!("Testing {provider}..."));
        use nika::provider::rig::RigProvider;
        let prov = match provider.as_str() {
            "anthropic" => RigProvider::claude(),
            "openai" => RigProvider::openai(),
            "mistral" => RigProvider::mistral(),
            "groq" => RigProvider::groq(),
            "deepseek" => RigProvider::deepseek(),
            "gemini" => RigProvider::gemini(),
            "xai" => RigProvider::xai(),
            _ => {
                spinner.stop(format!("{} Unknown provider", "✗".red()));
                cliclack::outro("Key stored. Run your command again.").ok();
                return Ok(true);
            }
        };
        match prov.infer("Say 'OK' if you can hear me.", None).await {
            Ok(_) => spinner.stop(format!("{} Connection successful!", "✓".green())),
            Err(e) => spinner.stop(format!("{} Connection failed: {e}", "✗".red())),
        }
    }

    cliclack::outro(format!(
        "{} {} configured! You're ready to go.",
        "✓".green(),
        provider.bold()
    ))
    .ok();
    Ok(true)
}

pub async fn handle_setup_command() -> Result<(), NikaError> {
    run_onboarding_wizard().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_any_provider_key_doesnt_panic() {
        let _ = has_any_provider_key();
    }

    #[test]
    fn onboarding_providers_has_seven() {
        assert_eq!(ONBOARDING_PROVIDERS.len(), 7);
    }

    #[test]
    fn onboarding_providers_starts_with_anthropic() {
        assert_eq!(ONBOARDING_PROVIDERS[0].0, "anthropic");
    }
}
