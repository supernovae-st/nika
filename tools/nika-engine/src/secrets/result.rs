// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Secrets loading result types.

/// Result of loading secrets.
#[derive(Debug, Clone, Default)]
pub struct SecretsLoadResult {
    /// Providers loaded from env vars / vault / daemon.
    pub from_env: Vec<String>,
    /// Providers with no key found.
    pub not_found: Vec<String>,
}

impl SecretsLoadResult {
    /// Total number of secrets loaded.
    pub fn total_loaded(&self) -> usize {
        self.from_env.len()
    }

    /// Human-readable summary.
    pub fn summary(&self) -> String {
        format!(
            "{} from env, {} not found",
            self.from_env.len(),
            self.not_found.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secrets_load_result_summary() {
        let result = SecretsLoadResult {
            from_env: vec!["anthropic".into(), "openai".into()],
            not_found: vec!["groq".into()],
        };
        assert_eq!(result.total_loaded(), 2);
        assert!(result.summary().contains("2 from env"));
    }

    #[test]
    fn test_secrets_load_result_empty() {
        let result = SecretsLoadResult {
            from_env: vec![],
            not_found: vec!["anthropic".into()],
        };
        assert_eq!(result.total_loaded(), 0);
        assert!(result.summary().contains("0 from env"));
    }
}
