//! Routing configuration for fallback chains and smart routing.
//!
//! Allows workflows to define provider fallback strategies:
//!
//! ```yaml
//! routing:
//!   fallback: [anthropic, openai, groq]
//!   strategy: cost  # or: latency, availability
//! ```

use serde::{Deserialize, Serialize};

/// Routing configuration for provider fallback and smart routing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RoutingConfig {
    /// Fallback provider chain (try in order).
    #[serde(default)]
    pub fallback: Vec<String>,

    /// Routing strategy.
    #[serde(default)]
    pub strategy: Option<RoutingStrategy>,
}

/// Strategy for selecting a provider from the fallback chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutingStrategy {
    /// Use the cheapest available provider.
    Cost,
    /// Use the fastest available provider.
    Latency,
    /// Use the first available provider (default).
    Availability,
}

/// Condition for retry behavior within routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RetryCondition {
    /// Retry on any error.
    Always,
    /// Retry only on transient errors (rate limits, timeouts).
    Transient,
    /// Never retry (fail immediately).
    Never,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_config_serde_roundtrip() {
        let config = RoutingConfig {
            fallback: vec!["anthropic".into(), "openai".into()],
            strategy: Some(RoutingStrategy::Cost),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: RoutingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn routing_config_default() {
        let config = RoutingConfig::default();
        assert!(config.fallback.is_empty());
        assert_eq!(config.strategy, None);
    }
}
