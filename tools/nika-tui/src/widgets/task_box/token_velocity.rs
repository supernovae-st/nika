// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Token Velocity Tracking for Sparkline Display
//!
//! Ring buffer of tokens/sec samples for real-time visualization.

use std::collections::VecDeque;

/// Token velocity tracker with fixed-size ring buffer
#[derive(Debug, Clone)]
pub struct TokenVelocity {
    /// Samples (tokens per second)
    samples: VecDeque<f32>,
    /// Maximum capacity
    capacity: usize,
    /// Cached sparkline string — rebuilt on push, returned by sparkline_chars()
    cached_sparkline: String,
}

impl TokenVelocity {
    /// Create new velocity tracker with given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
            cached_sparkline: String::new(),
        }
    }

    /// Push a new sample (tokens/sec) and rebuild sparkline cache
    pub fn push(&mut self, tokens_per_sec: f32) {
        if self.samples.len() >= self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(tokens_per_sec);
        self.rebuild_sparkline_cache();
    }

    /// Get all samples as slice
    pub fn samples(&self) -> Vec<f32> {
        self.samples.iter().copied().collect()
    }

    /// Number of samples
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Average velocity
    pub fn average(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().sum::<f32>() / self.samples.len() as f32
    }

    /// Peak velocity
    pub fn peak(&self) -> f32 {
        self.samples.iter().copied().fold(0.0, f32::max)
    }

    /// Get data normalized for sparkline widget (0..max_height)
    pub fn sparkline_data(&self, max_height: u64) -> Vec<u64> {
        if self.samples.is_empty() {
            return Vec::new();
        }

        let peak = self.peak().max(1.0); // Avoid division by zero
        self.samples
            .iter()
            .map(|&v| ((v / peak) * max_height as f32) as u64)
            .collect()
    }

    /// Clear all samples
    pub fn clear(&mut self) {
        self.samples.clear();
        self.cached_sparkline.clear();
    }
}

impl Default for TokenVelocity {
    fn default() -> Self {
        Self::new(30) // 30 samples = ~0.5s at 60fps
    }
}

/// Braille sparkline characters ordered by fill level
const SPARKLINE_CHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

impl TokenVelocity {
    /// Return cached sparkline string — O(1), no allocation in hot render path
    pub fn sparkline_chars(&self) -> &str {
        &self.cached_sparkline
    }

    /// Rebuild sparkline cache — called by push() and clear()
    fn rebuild_sparkline_cache(&mut self) {
        self.cached_sparkline.clear();
        if self.samples.is_empty() {
            return;
        }
        let peak = self.peak().max(1.0); // Avoid division by zero
        for &v in &self.samples {
            let idx = ((v / peak) * (SPARKLINE_CHARS.len() - 1) as f32).round() as usize;
            self.cached_sparkline
                .push(SPARKLINE_CHARS[idx.min(SPARKLINE_CHARS.len() - 1)]);
        }
    }

    /// Minimum velocity (0.0 when empty — avoids returning f32::INFINITY)
    pub fn min(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().copied().fold(f32::INFINITY, f32::min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_velocity_sparkline_chars() {
        let mut vel = TokenVelocity::new(8);
        // Push values: 0 (min), 40, 80 (peak)
        vel.push(0.0); // Should be ▁
        vel.push(40.0); // 50% -> ▄
        vel.push(80.0); // Peak -> █

        let sparkline = vel.sparkline_chars();
        assert_eq!(sparkline.chars().count(), 3);
        assert!(sparkline.starts_with('▁')); // Zero value
        assert!(sparkline.ends_with('█')); // Peak value
    }

    #[test]
    fn test_token_velocity_sparkline_ascending() {
        let mut vel = TokenVelocity::new(8);
        // Push ascending values
        for i in 0..8 {
            vel.push(i as f32 * 10.0);
        }
        let sparkline = vel.sparkline_chars();
        assert_eq!(sparkline.chars().count(), 8);
        // Last char should be peak
        assert!(sparkline.ends_with('█'));
    }

    #[test]
    fn test_token_velocity_sparkline_empty() {
        let vel = TokenVelocity::new(8);
        assert_eq!(vel.sparkline_chars(), "");
    }

    #[test]
    fn test_token_velocity_min() {
        let mut vel = TokenVelocity::new(10);
        vel.push(50.0);
        vel.push(10.0);
        vel.push(30.0);

        assert!((vel.min() - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_token_velocity_new() {
        let vel = TokenVelocity::new(20);
        assert_eq!(vel.capacity(), 20);
        assert_eq!(vel.len(), 0);
        assert!(vel.is_empty());
    }

    #[test]
    fn test_token_velocity_push() {
        let mut vel = TokenVelocity::new(5);
        vel.push(10.0);
        vel.push(20.0);
        vel.push(15.0);

        assert_eq!(vel.len(), 3);
        assert_eq!(vel.samples(), &[10.0, 20.0, 15.0]);
    }

    #[test]
    fn test_token_velocity_overflow() {
        let mut vel = TokenVelocity::new(3);
        vel.push(1.0);
        vel.push(2.0);
        vel.push(3.0);
        vel.push(4.0); // Overwrites oldest

        assert_eq!(vel.len(), 3);
        assert_eq!(vel.samples(), &[2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_token_velocity_average() {
        let mut vel = TokenVelocity::new(10);
        vel.push(10.0);
        vel.push(20.0);
        vel.push(30.0);

        assert!((vel.average() - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_token_velocity_peak() {
        let mut vel = TokenVelocity::new(10);
        vel.push(10.0);
        vel.push(50.0);
        vel.push(30.0);

        assert!((vel.peak() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_token_velocity_sparkline_data() {
        let mut vel = TokenVelocity::new(5);
        for i in 1..=5 {
            vel.push(i as f32 * 10.0);
        }

        let data = vel.sparkline_data(8);
        assert_eq!(data.len(), 5); // Only 5 samples
        assert!(data.iter().all(|&v| v <= 8)); // Normalized to max 8
    }
}
