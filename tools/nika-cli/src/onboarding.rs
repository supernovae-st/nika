//! First-run onboarding wizard for API key setup.

use colored::Colorize;
use std::io::IsTerminal;

use nika_engine::display::{hint, StatusIcon};
use nika_engine::error::NikaError;

const ONBOARDING_PROVIDERS: &[(&str, &str)] = &[
    ("anthropic", "Claude — best for reasoning, code, analysis"),
    ("openai", "GPT-4o, o3 — versatile, large ecosystem"),
    ("groq", "Llama 4 — free tier, ultra-fast inference"),
    ("mistral", "Mistral — European, strong multilingual"),
    ("deepseek", "DeepSeek — budget-friendly, good reasoning"),
    ("gemini", "Gemini 2.5 — large context, multimodal"),
    ("xai", "Grok 3 — real-time knowledge"),
    ("native", "Local GGUF models — no API key needed"),
];

pub fn has_any_provider_key() -> bool {
    use nika_engine::core::{ProviderCategory, KNOWN_PROVIDERS};

    // Check file flag first (most reliable — no daemon, no vault crypto)
    if onboarding_done_flag_exists() {
        return true;
    }

    // Check env vars (fast path)
    let has_env = KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == ProviderCategory::Llm)
        .any(|p| {
            std::env::var(p.env_var)
                .map(|v| !v.is_empty())
                .unwrap_or(false)
        });
    if has_env {
        return true;
    }

    // Check NikaVault (keys stored via `nika keys set`)
    // Uses canonical get_vault() to match the same path as keys module.
    // If vault fails (machine-id issue, permissions), don't trigger wizard.
    let vault = super::keys::get_vault();
    let vault_providers = [
        "anthropic",
        "openai",
        "xai",
        "gemini",
        "mistral",
        "groq",
        "deepseek",
    ];
    use secrecy::ExposeSecret;
    vault_providers
        .iter()
        .any(|p| matches!(vault.get(p), Ok(Some(k)) if !k.expose_secret().is_empty()))
}

/// Resolve ~/.nika home directory.
fn nika_home_dir() -> std::path::PathBuf {
    std::env::var("NIKA_HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".nika"))
}

/// Check if the `.onboarding_done` flag file exists.
fn onboarding_done_flag_exists() -> bool {
    nika_home_dir().join(".onboarding_done").exists()
}

/// Write the `.onboarding_done` flag file — the most reliable signal that
/// a provider was configured at some point (no daemon, no vault crypto needed).
pub fn mark_onboarding_done() {
    let flag = nika_home_dir().join(".onboarding_done");
    if let Some(parent) = flag.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&flag, "1");
}

/// In-process flag set by `--no-interactive` (avoids unsafe set_var).
static NO_ONBOARDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Mark onboarding as skipped (safe alternative to `set_var("NIKA_NO_ONBOARDING", "1")`).
pub fn set_no_onboarding() {
    NO_ONBOARDING.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Check if onboarding should be skipped entirely (VPS/Docker/CI).
pub fn skip_onboarding() -> bool {
    NO_ONBOARDING.load(std::sync::atomic::Ordering::Relaxed)
        || std::env::var("NIKA_NO_ONBOARDING")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

pub async fn run_onboarding_wizard() -> Result<bool, NikaError> {
    if !std::io::stdin().is_terminal() {
        println!();
        println!("  {} No API keys configured", StatusIcon::Warn);
        println!("{}", hint("Set an API key: nika keys set <provider> <key>"));
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

    // Native provider: no API key needed, just show model pull hint
    if provider == "native" {
        mark_onboarding_done();
        cliclack::outro(format!(
            "{} Native provider selected! Pull a model to get started:\n\n  nika model pull <model-name>\n\n  Example: nika model pull mistral-7b-instruct",
            StatusIcon::Ok
        ))
        .ok();
        return Ok(true);
    }

    // Delegate to keys module — single code path for key management
    // (handles password prompt, validation, vault, env injection, connection test)
    // quiet=true suppresses the keys intro/outro (onboarding has its own)
    super::keys::handle_keys_set(Some(provider.clone()), false, false, true).await?;

    // Mark onboarding as done — prevents the wizard from looping
    mark_onboarding_done();

    Ok(true)
}

pub async fn handle_setup_command(_quiet: bool) -> Result<(), NikaError> {
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
    fn onboarding_providers_has_eight() {
        assert_eq!(ONBOARDING_PROVIDERS.len(), 8);
    }

    #[test]
    fn onboarding_providers_starts_with_anthropic() {
        assert_eq!(ONBOARDING_PROVIDERS[0].0, "anthropic");
    }

    #[test]
    fn skip_onboarding_respects_atomic_flag() {
        NO_ONBOARDING.store(false, std::sync::atomic::Ordering::Relaxed);
        set_no_onboarding();
        assert!(skip_onboarding());
        NO_ONBOARDING.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    #[test]
    fn mark_onboarding_done_creates_flag() {
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: test isolation — no concurrent readers of NIKA_HOME
        unsafe { std::env::set_var("NIKA_HOME", tmp.path()) };
        mark_onboarding_done();
        assert!(tmp.path().join(".onboarding_done").exists());
        assert!(onboarding_done_flag_exists());
        unsafe { std::env::remove_var("NIKA_HOME") };
    }
}
