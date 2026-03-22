//! LimitTracker - Runtime limit tracking for agent execution
//!
//! Tracks resource consumption during agent execution and checks against
//! configured limits to enable cost control and graceful partial completion.
//!
//! ## Features
//!
//! - **Turn tracking**: Counts agentic loop iterations
//! - **Token tracking**: Accumulates input + output tokens
//! - **Cost tracking**: Calculates USD cost based on provider pricing
//! - **Duration tracking**: Wall-clock time from start
//!
//! ## Example
//!
//! ```rust,ignore
//! use nika::runtime::LimitTracker;
//! use nika::ast::LimitsConfig;
//!
//! let config = LimitsConfig {
//!     max_turns: 20,
//!     max_tokens: 50000,
//!     max_cost_usd: 2.00,
//!     max_duration_secs: 300,
//!     ..Default::default()
//! };
//!
//! let mut tracker = LimitTracker::new(config);
//!
//! // After each turn
//! tracker.add_turn();
//! tracker.add_tokens(500, 150);  // input, output
//! tracker.add_cost(0.015);
//!
//! // Check limits
//! if let Some(exceeded) = tracker.check_limits() {
//!     println!("Limit exceeded: {:?}", exceeded);
//! }
//! ```

use std::time::Instant;

use crate::ast::limits::{LimitStatus, LimitType, LimitsConfig};

// ═══════════════════════════════════════════════════════════════════════════
// LimitTracker
// ═══════════════════════════════════════════════════════════════════════════

/// Runtime tracker for agent execution limits.
///
/// Accumulates resource consumption and checks against configured limits.
/// Used by RigAgentLoop to enforce cost control and enable partial completion.
#[derive(Debug, Clone)]
pub struct LimitTracker {
    /// Configuration with limit thresholds
    config: LimitsConfig,

    /// Current turn count
    current_turns: u32,

    /// Accumulated input tokens
    input_tokens: u64,

    /// Accumulated output tokens
    output_tokens: u64,

    /// Accumulated cost in USD
    cost_usd: f64,

    /// Start time for duration tracking
    start_time: Instant,
}

impl LimitTracker {
    /// Create a new tracker with the given configuration.
    pub fn new(config: LimitsConfig) -> Self {
        Self {
            config,
            current_turns: 0,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            start_time: Instant::now(),
        }
    }

    /// Create a tracker with no limits (unlimited execution).
    pub fn unlimited() -> Self {
        Self::new(LimitsConfig::default())
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Increment methods
    // ═══════════════════════════════════════════════════════════════════════

    /// Increment turn counter.
    pub fn add_turn(&mut self) {
        self.current_turns += 1;
    }

    /// Add tokens from a turn.
    pub fn add_tokens(&mut self, input: u64, output: u64) {
        self.input_tokens += input;
        self.output_tokens += output;
    }

    /// Add cost from a turn.
    pub fn add_cost(&mut self, cost: f64) {
        self.cost_usd += cost;
    }

    /// Record a complete turn with all metrics.
    pub fn record_turn(&mut self, input_tokens: u64, output_tokens: u64, cost: f64) {
        self.add_turn();
        self.add_tokens(input_tokens, output_tokens);
        self.add_cost(cost);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Getters
    // ═══════════════════════════════════════════════════════════════════════

    /// Get current turn count.
    pub fn turns(&self) -> u32 {
        self.current_turns
    }

    /// Get total tokens (input + output).
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    /// Get input tokens.
    pub fn input_tokens(&self) -> u64 {
        self.input_tokens
    }

    /// Get output tokens.
    pub fn output_tokens(&self) -> u64 {
        self.output_tokens
    }

    /// Get accumulated cost in USD.
    pub fn cost_usd(&self) -> f64 {
        self.cost_usd
    }

    /// Get elapsed duration in seconds.
    pub fn duration_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get the configuration.
    pub fn config(&self) -> &LimitsConfig {
        &self.config
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Limit checking
    // ═══════════════════════════════════════════════════════════════════════

    /// Check all limits and return the first exceeded limit, if any.
    ///
    /// Returns `Some(LimitStatus)` if a limit is exceeded, `None` otherwise.
    pub fn check_limits(&self) -> Option<LimitStatus> {
        // Check in order of typical impact: turns, tokens, cost, duration
        if let Some(status) = self.check_turns() {
            if status.exceeded {
                return Some(status);
            }
        }

        if let Some(status) = self.check_tokens() {
            if status.exceeded {
                return Some(status);
            }
        }

        if let Some(status) = self.check_cost() {
            if status.exceeded {
                return Some(status);
            }
        }

        if let Some(status) = self.check_duration() {
            if status.exceeded {
                return Some(status);
            }
        }

        None
    }

    /// Check turns limit.
    pub fn check_turns(&self) -> Option<LimitStatus> {
        if self.config.has_turns_limit() {
            Some(LimitStatus::new(
                LimitType::Turns,
                self.current_turns as f64,
                self.config.max_turns as f64,
            ))
        } else {
            None
        }
    }

    /// Check tokens limit.
    pub fn check_tokens(&self) -> Option<LimitStatus> {
        if self.config.has_tokens_limit() {
            Some(LimitStatus::new(
                LimitType::Tokens,
                self.total_tokens() as f64,
                self.config.max_tokens as f64,
            ))
        } else {
            None
        }
    }

    /// Check cost limit.
    pub fn check_cost(&self) -> Option<LimitStatus> {
        if self.config.has_cost_limit() {
            Some(LimitStatus::new(
                LimitType::Cost,
                self.cost_usd,
                self.config.max_cost_usd,
            ))
        } else {
            None
        }
    }

    /// Check duration limit.
    pub fn check_duration(&self) -> Option<LimitStatus> {
        if self.config.has_duration_limit() {
            Some(LimitStatus::new(
                LimitType::Duration,
                self.duration_secs() as f64,
                self.config.max_duration_secs as f64,
            ))
        } else {
            None
        }
    }

    /// Get status for all configured limits.
    pub fn all_statuses(&self) -> Vec<LimitStatus> {
        let mut statuses = Vec::new();

        if let Some(s) = self.check_turns() {
            statuses.push(s);
        }
        if let Some(s) = self.check_tokens() {
            statuses.push(s);
        }
        if let Some(s) = self.check_cost() {
            statuses.push(s);
        }
        if let Some(s) = self.check_duration() {
            statuses.push(s);
        }

        statuses
    }

    /// Calculate progress as a percentage (0.0 - 1.0).
    ///
    /// Returns the maximum progress across all configured limits.
    /// If no limits are configured, returns 0.0.
    pub fn progress(&self) -> f64 {
        self.all_statuses()
            .iter()
            .map(|s| s.usage_pct)
            .fold(0.0, f64::max)
    }

    /// Check if any limit has been exceeded.
    pub fn any_exceeded(&self) -> bool {
        self.check_limits().is_some()
    }

    /// Check if we're approaching a limit (>80% usage).
    pub fn approaching_limit(&self) -> bool {
        self.all_statuses().iter().any(|s| s.usage_pct > 0.8)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Reset
    // ═══════════════════════════════════════════════════════════════════════

    /// Reset all counters (keeps configuration).
    pub fn reset(&mut self) {
        self.current_turns = 0;
        self.input_tokens = 0;
        self.output_tokens = 0;
        self.cost_usd = 0.0;
        self.start_time = Instant::now();
    }
}

impl Default for LimitTracker {
    fn default() -> Self {
        Self::unlimited()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::limits::OnLimitReachedConfig;

    fn config_with_turns(max: u32) -> LimitsConfig {
        LimitsConfig {
            max_turns: max,
            ..Default::default()
        }
    }

    fn config_with_tokens(max: u64) -> LimitsConfig {
        LimitsConfig {
            max_tokens: max,
            ..Default::default()
        }
    }

    fn config_with_cost(max: f64) -> LimitsConfig {
        LimitsConfig {
            max_cost_usd: max,
            ..Default::default()
        }
    }

    fn full_config() -> LimitsConfig {
        LimitsConfig {
            max_turns: 10,
            max_tokens: 5000,
            max_cost_usd: 1.00,
            max_duration_secs: 60,
            on_limit_reached: OnLimitReachedConfig::default(),
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // Construction tests
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn new_tracker_starts_at_zero() {
        let tracker = LimitTracker::new(full_config());

        assert_eq!(tracker.turns(), 0);
        assert_eq!(tracker.total_tokens(), 0);
        assert_eq!(tracker.input_tokens(), 0);
        assert_eq!(tracker.output_tokens(), 0);
        assert!((tracker.cost_usd() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn unlimited_tracker_has_no_limits() {
        let tracker = LimitTracker::unlimited();

        assert!(!tracker.config().has_limits());
        assert!(tracker.check_limits().is_none());
    }

    #[test]
    fn default_is_unlimited() {
        let tracker = LimitTracker::default();
        assert!(!tracker.config().has_limits());
    }

    // ════════════════════════════════════════════════════════════════════
    // Increment tests
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn add_turn_increments() {
        let mut tracker = LimitTracker::new(full_config());

        tracker.add_turn();
        assert_eq!(tracker.turns(), 1);

        tracker.add_turn();
        tracker.add_turn();
        assert_eq!(tracker.turns(), 3);
    }

    #[test]
    fn add_tokens_accumulates() {
        let mut tracker = LimitTracker::new(full_config());

        tracker.add_tokens(100, 50);
        assert_eq!(tracker.input_tokens(), 100);
        assert_eq!(tracker.output_tokens(), 50);
        assert_eq!(tracker.total_tokens(), 150);

        tracker.add_tokens(200, 100);
        assert_eq!(tracker.input_tokens(), 300);
        assert_eq!(tracker.output_tokens(), 150);
        assert_eq!(tracker.total_tokens(), 450);
    }

    #[test]
    fn add_cost_accumulates() {
        let mut tracker = LimitTracker::new(full_config());

        tracker.add_cost(0.10);
        assert!((tracker.cost_usd() - 0.10).abs() < f64::EPSILON);

        tracker.add_cost(0.25);
        assert!((tracker.cost_usd() - 0.35).abs() < f64::EPSILON);
    }

    #[test]
    fn record_turn_does_all() {
        let mut tracker = LimitTracker::new(full_config());

        tracker.record_turn(100, 50, 0.015);

        assert_eq!(tracker.turns(), 1);
        assert_eq!(tracker.input_tokens(), 100);
        assert_eq!(tracker.output_tokens(), 50);
        assert!((tracker.cost_usd() - 0.015).abs() < f64::EPSILON);
    }

    // ════════════════════════════════════════════════════════════════════
    // Limit checking tests
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn check_turns_not_exceeded() {
        let mut tracker = LimitTracker::new(config_with_turns(10));

        tracker.add_turn();
        tracker.add_turn();
        tracker.add_turn();

        let status = tracker.check_turns().unwrap();
        assert!(!status.exceeded);
        assert!((status.current - 3.0).abs() < f64::EPSILON);
        assert!((status.maximum - 10.0).abs() < f64::EPSILON);
        assert!((status.usage_pct - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn check_turns_exceeded() {
        let mut tracker = LimitTracker::new(config_with_turns(5));

        for _ in 0..5 {
            tracker.add_turn();
        }

        let status = tracker.check_turns().unwrap();
        assert!(status.exceeded);
        assert!((status.current - 5.0).abs() < f64::EPSILON);
        assert!((status.usage_pct - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn check_tokens_not_exceeded() {
        let mut tracker = LimitTracker::new(config_with_tokens(10000));

        tracker.add_tokens(2000, 1000);

        let status = tracker.check_tokens().unwrap();
        assert!(!status.exceeded);
        assert!((status.current - 3000.0).abs() < f64::EPSILON);
        assert!((status.usage_pct - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn check_tokens_exceeded() {
        let mut tracker = LimitTracker::new(config_with_tokens(5000));

        tracker.add_tokens(3000, 2500); // 5500 total

        let status = tracker.check_tokens().unwrap();
        assert!(status.exceeded);
    }

    #[test]
    fn check_cost_not_exceeded() {
        let mut tracker = LimitTracker::new(config_with_cost(2.00));

        tracker.add_cost(0.50);
        tracker.add_cost(0.75);

        let status = tracker.check_cost().unwrap();
        assert!(!status.exceeded);
        assert!((status.current - 1.25).abs() < f64::EPSILON);
    }

    #[test]
    fn check_cost_exceeded() {
        let mut tracker = LimitTracker::new(config_with_cost(1.00));

        tracker.add_cost(0.80);
        tracker.add_cost(0.30);

        let status = tracker.check_cost().unwrap();
        assert!(status.exceeded);
    }

    #[test]
    fn check_limits_returns_first_exceeded() {
        let config = LimitsConfig {
            max_turns: 5,
            max_tokens: 10000,
            ..Default::default()
        };
        let mut tracker = LimitTracker::new(config);

        // Exceed turns first
        for _ in 0..6 {
            tracker.add_turn();
        }

        let exceeded = tracker.check_limits().unwrap();
        assert_eq!(exceeded.limit_type, LimitType::Turns);
    }

    #[test]
    fn check_limits_none_when_ok() {
        let mut tracker = LimitTracker::new(full_config());

        tracker.add_turn();
        tracker.add_tokens(100, 50);
        tracker.add_cost(0.01);

        assert!(tracker.check_limits().is_none());
    }

    // ════════════════════════════════════════════════════════════════════
    // Progress and status tests
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn progress_returns_max_usage() {
        let config = LimitsConfig {
            max_turns: 10,
            max_tokens: 1000,
            ..Default::default()
        };
        let mut tracker = LimitTracker::new(config);

        tracker.add_turn(); // 10% turns
        tracker.add_tokens(500, 0); // 50% tokens

        let progress = tracker.progress();
        assert!((progress - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn progress_zero_when_unlimited() {
        let tracker = LimitTracker::unlimited();
        assert!((tracker.progress() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn all_statuses_returns_configured() {
        let tracker = LimitTracker::new(full_config());
        let statuses = tracker.all_statuses();

        assert_eq!(statuses.len(), 4); // turns, tokens, cost, duration
    }

    #[test]
    fn any_exceeded_true_when_over() {
        let mut tracker = LimitTracker::new(config_with_turns(3));

        assert!(!tracker.any_exceeded());

        tracker.add_turn();
        tracker.add_turn();
        tracker.add_turn();

        assert!(tracker.any_exceeded());
    }

    #[test]
    fn approaching_limit_at_80_percent() {
        let mut tracker = LimitTracker::new(config_with_turns(10));

        for _ in 0..7 {
            tracker.add_turn();
        }
        assert!(!tracker.approaching_limit()); // 70%

        tracker.add_turn();
        tracker.add_turn();
        assert!(tracker.approaching_limit()); // 90%
    }

    // ════════════════════════════════════════════════════════════════════
    // Reset tests
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn reset_clears_counters() {
        let mut tracker = LimitTracker::new(full_config());

        tracker.add_turn();
        tracker.add_tokens(100, 50);
        tracker.add_cost(0.10);

        tracker.reset();

        assert_eq!(tracker.turns(), 0);
        assert_eq!(tracker.total_tokens(), 0);
        assert!((tracker.cost_usd() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn reset_keeps_config() {
        let config = config_with_turns(10);
        let mut tracker = LimitTracker::new(config);

        tracker.reset();

        assert!(tracker.config().has_turns_limit());
        assert_eq!(tracker.config().max_turns, 10);
    }

    // ════════════════════════════════════════════════════════════════════
    // Duration tests (basic - timing is hard to test)
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn duration_starts_at_zero() {
        let tracker = LimitTracker::new(full_config());
        // Duration should be 0 or very small (< 1 sec)
        assert!(tracker.duration_secs() < 1);
    }
}
