//! Secrets loading result types.

/// Result of loading secrets.
#[derive(Debug, Clone, Default)]
pub struct SecretsLoadResult {
    /// Providers loaded from spn daemon.
    pub from_daemon: Vec<String>,
    /// Providers loaded from fallback (keyring/env).
    pub from_fallback: Vec<String>,
    /// Providers with no key found.
    pub not_found: Vec<String>,
    /// Whether daemon was available.
    pub daemon_available: bool,
}

impl SecretsLoadResult {
    /// Total number of secrets loaded.
    pub fn total_loaded(&self) -> usize {
        self.from_daemon.len() + self.from_fallback.len()
    }

    /// Human-readable summary.
    pub fn summary(&self) -> String {
        if self.daemon_available {
            format!(
                "{} from daemon, {} fallback, {} not found",
                self.from_daemon.len(),
                self.from_fallback.len(),
                self.not_found.len()
            )
        } else {
            format!(
                "daemon unavailable, {} from fallback, {} not found",
                self.from_fallback.len(),
                self.not_found.len()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secrets_load_result_summary() {
        let result = SecretsLoadResult {
            from_daemon: vec!["anthropic".into()],
            from_fallback: vec!["openai".into()],
            not_found: vec!["groq".into()],
            daemon_available: true,
        };
        assert_eq!(result.total_loaded(), 2);
        assert!(result.summary().contains("1 from daemon"));
    }

    #[test]
    fn test_secrets_load_result_no_daemon() {
        let result = SecretsLoadResult {
            from_daemon: vec![],
            from_fallback: vec!["anthropic".into()],
            not_found: vec![],
            daemon_available: false,
        };
        assert!(result.summary().contains("daemon unavailable"));
    }
}
