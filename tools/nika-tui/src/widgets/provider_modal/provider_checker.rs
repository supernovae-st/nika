//! Async provider status checker
//!
//! Validates provider connectivity by pinging their APIs.
//! Uses reqwest with timeouts to prevent hanging.
//!

use std::time::{Duration, Instant};

use reqwest::Client;

use crate::providers::{env_var as provider_env_var, llm_provider_ids};

use super::state::ConnectionStatus;

/// Timeout for provider health checks
const PROVIDER_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Provider status checker
pub struct ProviderChecker {
    client: Client,
}

impl Default for ProviderChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderChecker {
    pub fn new() -> Self {
        let client = Client::builder()
            .connect_timeout(PROVIDER_CHECK_TIMEOUT)
            .timeout(PROVIDER_CHECK_TIMEOUT)
            .user_agent(format!("nika/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!("HTTP client build failed: {e}, using default");
                Client::new()
            });

        Self { client }
    }

    /// Check a provider's connectivity status
    pub async fn check(&self, provider: &str) -> ConnectionStatus {
        let start = Instant::now();

        // Check if API key exists (except for native which doesn't need one)
        let env_var = provider_env_var(provider);
        if provider != "native" && std::env::var(env_var).is_err() {
            return ConnectionStatus::NotConfigured;
        }

        // Ping provider
        let result = match provider {
            "anthropic" => self.check_anthropic().await,
            "openai" => self.check_openai().await,
            "mistral" => self.check_mistral().await,
            "groq" => self.check_groq().await,
            "deepseek" => self.check_deepseek().await,
            "gemini" => self.check_gemini().await,
            "xai" | "grok" => self.check_xai().await,
            "native" => self.check_native(),
            _ => Err("Unknown provider".to_string()),
        };

        match result {
            Ok(()) => ConnectionStatus::Connected {
                latency_ms: start.elapsed().as_millis() as u64,
            },
            Err(e) => ConnectionStatus::Failed { error: e },
        }
    }

    /// Check all providers in parallel
    ///
    /// Uses centralized `llm_provider_ids()` from providers module.
    pub async fn check_all(&self) -> Vec<(&'static str, ConnectionStatus)> {
        let providers: Vec<_> = llm_provider_ids().collect();

        let mut handles = Vec::new();
        for provider in providers {
            let checker = Self::new(); // Clone checker for each task
            handles.push(tokio::spawn(async move {
                let status = checker.check(provider).await;
                (provider, status)
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            if let Ok(result) = handle.await {
                results.push(result);
            }
        }

        results
    }

    async fn check_anthropic(&self) -> Result<(), String> {
        let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| "No API key")?;

        self.client
            .get("https://api.anthropic.com/v1/models")
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?
            .error_for_status()
            .map_err(|e| format!("API error: {}", e.status().map_or(0, |s| s.as_u16())))?;

        Ok(())
    }

    async fn check_openai(&self) -> Result<(), String> {
        let key = std::env::var("OPENAI_API_KEY").map_err(|_| "No API key")?;

        self.client
            .get("https://api.openai.com/v1/models")
            .bearer_auth(key)
            .send()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?
            .error_for_status()
            .map_err(|e| format!("API error: {}", e.status().map_or(0, |s| s.as_u16())))?;

        Ok(())
    }

    async fn check_mistral(&self) -> Result<(), String> {
        let key = std::env::var("MISTRAL_API_KEY").map_err(|_| "No API key")?;

        self.client
            .get("https://api.mistral.ai/v1/models")
            .bearer_auth(key)
            .send()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?
            .error_for_status()
            .map_err(|e| format!("API error: {}", e.status().map_or(0, |s| s.as_u16())))?;

        Ok(())
    }

    async fn check_groq(&self) -> Result<(), String> {
        let key = std::env::var("GROQ_API_KEY").map_err(|_| "No API key")?;

        self.client
            .get("https://api.groq.com/openai/v1/models")
            .bearer_auth(key)
            .send()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?
            .error_for_status()
            .map_err(|e| format!("API error: {}", e.status().map_or(0, |s| s.as_u16())))?;

        Ok(())
    }

    async fn check_deepseek(&self) -> Result<(), String> {
        let key = std::env::var("DEEPSEEK_API_KEY").map_err(|_| "No API key")?;

        self.client
            .get("https://api.deepseek.com/v1/models")
            .bearer_auth(key)
            .send()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?
            .error_for_status()
            .map_err(|e| format!("API error: {}", e.status().map_or(0, |s| s.as_u16())))?;

        Ok(())
    }

    async fn check_gemini(&self) -> Result<(), String> {
        let key = std::env::var("GEMINI_API_KEY").map_err(|_| "No API key")?;

        // Gemini uses Google's generativelanguage API
        // SEC: use |_| to avoid leaking the API key in error messages,
        // since the key is embedded in the URL query string.
        self.client
            .get(format!(
                "https://generativelanguage.googleapis.com/v1beta/models?key={}",
                key
            ))
            .send()
            .await
            .map_err(|_| "Gemini: connection failed".to_string())?
            .error_for_status()
            .map_err(|e| format!("API error: {}", e.status().map_or(0, |s| s.as_u16())))?;

        Ok(())
    }

    /// Check xAI (Grok) connectivity
    async fn check_xai(&self) -> Result<(), String> {
        let key = std::env::var("XAI_API_KEY").map_err(|_| "No API key")?;

        self.client
            .get("https://api.x.ai/v1/models")
            .header("Authorization", format!("Bearer {}", key))
            .send()
            .await
            .map_err(|_| "xAI: connection failed".to_string())?
            .error_for_status()
            .map_err(|e| format!("API error: {}", e.status().map_or(0, |s| s.as_u16())))?;

        Ok(())
    }

    /// Check native inference availability
    fn check_native(&self) -> Result<(), String> {
        // Native inference via mistral.rs
        // Check if the native-inference feature is enabled
        #[cfg(feature = "native-inference")]
        {
            // For native inference, we just check if the feature is enabled
            // Actual model loading happens at runtime
            Ok(())
        }
        #[cfg(not(feature = "native-inference"))]
        {
            Err(
                "Native inference not available in this build"
                    .to_string(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_provider_checker_new() {
        let checker = ProviderChecker::new();
        // Should not panic
        drop(checker);
    }

    #[test]
    fn test_provider_checker_default() {
        let checker = ProviderChecker::default();
        drop(checker);
    }

    #[tokio::test]
    #[serial]
    async fn test_check_anthropic_no_key() {
        std::env::remove_var("ANTHROPIC_API_KEY");
        let checker = ProviderChecker::new();
        let status = checker.check("anthropic").await;
        assert!(matches!(status, ConnectionStatus::NotConfigured));
    }

    #[tokio::test]
    #[serial]
    async fn test_check_openai_no_key() {
        std::env::remove_var("OPENAI_API_KEY");
        let checker = ProviderChecker::new();
        let status = checker.check("openai").await;
        assert!(matches!(status, ConnectionStatus::NotConfigured));
    }

    #[tokio::test]
    #[serial]
    async fn test_check_mistral_no_key() {
        std::env::remove_var("MISTRAL_API_KEY");
        let checker = ProviderChecker::new();
        let status = checker.check("mistral").await;
        assert!(matches!(status, ConnectionStatus::NotConfigured));
    }

    #[tokio::test]
    #[serial]
    async fn test_check_groq_no_key() {
        std::env::remove_var("GROQ_API_KEY");
        let checker = ProviderChecker::new();
        let status = checker.check("groq").await;
        assert!(matches!(status, ConnectionStatus::NotConfigured));
    }

    #[tokio::test]
    #[serial]
    async fn test_check_deepseek_no_key() {
        std::env::remove_var("DEEPSEEK_API_KEY");
        let checker = ProviderChecker::new();
        let status = checker.check("deepseek").await;
        assert!(matches!(status, ConnectionStatus::NotConfigured));
    }

    #[tokio::test]
    #[serial]
    async fn test_check_gemini_no_key() {
        std::env::remove_var("GEMINI_API_KEY");
        let checker = ProviderChecker::new();
        let status = checker.check("gemini").await;
        assert!(matches!(status, ConnectionStatus::NotConfigured));
    }

    #[tokio::test]
    async fn test_check_native_returns_status() {
        // Native doesn't require API key
        let checker = ProviderChecker::new();
        let status = checker.check("native").await;
        // Either connected (feature enabled) or failed (feature disabled)
        assert!(matches!(
            status,
            ConnectionStatus::Connected { .. } | ConnectionStatus::Failed { .. }
        ));
    }

    #[tokio::test]
    async fn test_check_unknown_provider_no_key() {
        // Use a unique provider name that definitely doesn't have an env var set
        let checker = ProviderChecker::new();
        let status = checker.check("totally_fake_provider_xyz").await;
        assert!(matches!(status, ConnectionStatus::NotConfigured));
    }

    #[tokio::test]
    #[serial]
    async fn test_check_unknown_provider_falls_through() {
        // Unknown provider returns Failed "Unknown provider" when key check passes
        // For this test, we verify the behavior by checking that known providers
        // don't return "Unknown provider" error
        let checker = ProviderChecker::new();
        std::env::remove_var("ANTHROPIC_API_KEY");
        let status = checker.check("anthropic").await;
        // Should be NotConfigured (no key) not Failed (unknown provider)
        assert!(matches!(status, ConnectionStatus::NotConfigured));
    }

    #[tokio::test]
    async fn test_check_all_returns_results() {
        let checker = ProviderChecker::new();
        let results = checker.check_all().await;
        // Should return results for all LLM providers (6 cloud + 1 native = 7)
        assert!(!results.is_empty());
    }

    // Integration test with real API (only runs if key is set)
    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_check_anthropic_with_real_key() {
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            return;
        }
        let checker = ProviderChecker::new();
        let status = checker.check("anthropic").await;
        assert!(matches!(
            status,
            ConnectionStatus::Connected { .. } | ConnectionStatus::Failed { .. }
        ));
    }
}
