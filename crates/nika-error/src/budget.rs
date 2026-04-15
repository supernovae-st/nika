// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Budget directives for resource-bounded operations.

use serde::{Deserialize, Serialize};

/// Budget directive controlling resource limits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
#[non_exhaustive]
pub enum BudgetDirective {
    /// Limit by token count.
    Tokens {
        /// Maximum tokens allowed.
        limit: u64,
    },
    /// Limit by cost in nano-USD.
    Cost {
        /// Maximum cost in nano-USD.
        limit_nano_usd: i128,
    },
    /// Limit by wall-clock time.
    Time {
        /// Maximum time in milliseconds.
        limit_ms: u64,
    },
    /// No limit.
    #[default]
    Unlimited,
}

impl BudgetDirective {
    /// Create a token budget.
    #[must_use]
    pub fn tokens(limit: u64) -> Self {
        Self::Tokens { limit }
    }

    /// Create a cost budget in nano-USD.
    #[must_use]
    pub fn cost(limit_nano_usd: i128) -> Self {
        Self::Cost { limit_nano_usd }
    }

    /// Create a time budget in milliseconds.
    #[must_use]
    pub fn time_ms(limit_ms: u64) -> Self {
        Self::Time { limit_ms }
    }

    /// Whether this is an unlimited budget.
    #[must_use]
    pub fn is_unlimited(&self) -> bool {
        matches!(self, Self::Unlimited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_budget() {
        let b = BudgetDirective::tokens(10_000);
        assert!(!b.is_unlimited());
        assert!(matches!(b, BudgetDirective::Tokens { limit: 10_000 }));
    }

    #[test]
    fn cost_budget() {
        let b = BudgetDirective::cost(500_000_000);
        assert!(matches!(
            b,
            BudgetDirective::Cost {
                limit_nano_usd: 500_000_000
            }
        ));
    }

    #[test]
    fn time_budget() {
        let b = BudgetDirective::time_ms(30_000);
        assert!(matches!(b, BudgetDirective::Time { limit_ms: 30_000 }));
    }

    #[test]
    fn unlimited_is_default() {
        assert!(BudgetDirective::default().is_unlimited());
    }

    #[test]
    fn serde_roundtrip() {
        let b = BudgetDirective::tokens(5000);
        let json = serde_json::to_string(&b).expect("serialize");
        let back: BudgetDirective = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(b, back);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn budget_is_send_sync() {
        _assert_send_sync::<BudgetDirective>();
    }
}
