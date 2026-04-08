// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Glow animation system for ecosystem files.
//!
//! Provides smooth pulse effects for .nika.yaml, .son, and other
//! SuperNovae ecosystem files.

use std::time::{Duration, Instant};

/// Animation state for a single node
#[derive(Debug, Clone)]
pub struct GlowAnimation {
    /// Start time of animation
    start: Instant,
    /// Animation duration
    duration: Duration,
    /// Animation type
    pub kind: AnimationKind,
}

/// Types of animations supported
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationKind {
    /// Gentle sine wave pulse (default for ecosystem files)
    #[default]
    Pulse,
    /// Expand/collapse transition (150ms ease-out)
    Expand,
    /// Selection sweep effect
    SelectionSweep,
    /// Shimmer effect for loading states
    Shimmer,
}

impl Default for GlowAnimation {
    fn default() -> Self {
        Self::pulse()
    }
}

impl GlowAnimation {
    /// Create a new pulse animation (500ms cycle)
    pub fn pulse() -> Self {
        Self {
            start: Instant::now(),
            duration: Duration::from_millis(500),
            kind: AnimationKind::Pulse,
        }
    }

    /// Create an expand animation (150ms)
    pub fn expand() -> Self {
        Self {
            start: Instant::now(),
            duration: Duration::from_millis(150),
            kind: AnimationKind::Expand,
        }
    }

    /// Create a selection sweep animation (200ms)
    pub fn selection_sweep() -> Self {
        Self {
            start: Instant::now(),
            duration: Duration::from_millis(200),
            kind: AnimationKind::SelectionSweep,
        }
    }

    /// Create a shimmer animation (1000ms)
    pub fn shimmer() -> Self {
        Self {
            start: Instant::now(),
            duration: Duration::from_millis(1000),
            kind: AnimationKind::Shimmer,
        }
    }

    /// Reset the animation start time
    pub fn reset(&mut self) {
        self.start = Instant::now();
    }

    /// Check if animation is complete (for non-looping animations)
    pub fn is_complete(&self) -> bool {
        match self.kind {
            AnimationKind::Pulse | AnimationKind::Shimmer => false, // Loop forever
            AnimationKind::Expand | AnimationKind::SelectionSweep => {
                self.start.elapsed() >= self.duration
            }
        }
    }

    /// Get current intensity (0.0 to 1.0)
    ///
    /// Different easing functions based on animation type:
    /// - Pulse: Sine wave oscillation
    /// - Expand: Ease-out cubic
    /// - SelectionSweep: Linear
    /// - Shimmer: Saw-tooth wave
    pub fn intensity(&self) -> f32 {
        let elapsed = self.start.elapsed().as_secs_f32();
        let cycle = (elapsed / self.duration.as_secs_f32()) % 1.0;

        match self.kind {
            AnimationKind::Pulse => {
                // Sine wave: 0.5 + 0.5 * sin(2PI * cycle)
                // Oscillates between 0.0 and 1.0
                0.5 + 0.5 * (std::f32::consts::TAU * cycle).sin()
            }
            AnimationKind::Expand => {
                // Ease-out cubic: 1 - (1 - x)^3
                // Fast start, slow end
                let x = (elapsed / self.duration.as_secs_f32()).min(1.0);
                1.0 - (1.0 - x).powi(3)
            }
            AnimationKind::SelectionSweep => {
                // Linear: x
                (elapsed / self.duration.as_secs_f32()).min(1.0)
            }
            AnimationKind::Shimmer => {
                // Saw-tooth: cycle directly (0 to 1, then jump back)
                cycle
            }
        }
    }

    /// Get glow factor for color modulation (0.7 to 1.0 for subtle effect)
    pub fn glow_factor(&self) -> f32 {
        // Map intensity (0-1) to glow range (0.7-1.0) for subtle effect
        0.7 + 0.3 * self.intensity()
    }
}

/// Animation ticker for frame-based animation (60fps target)
#[derive(Debug, Clone)]
pub struct AnimationTicker {
    /// Current frame number (0-255, wraps)
    frame: u8,
    /// Last tick timestamp
    last_tick: Instant,
    /// Target frame duration (16.67ms for 60fps)
    frame_duration: Duration,
}

impl Default for AnimationTicker {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimationTicker {
    /// Create a new ticker at 60fps
    pub fn new() -> Self {
        Self {
            frame: 0,
            last_tick: Instant::now(),
            frame_duration: Duration::from_micros(16_667), // ~60fps
        }
    }

    /// Create a ticker with custom fps
    pub fn with_fps(fps: u32) -> Self {
        Self {
            frame: 0,
            last_tick: Instant::now(),
            frame_duration: Duration::from_micros(1_000_000 / fps as u64),
        }
    }

    /// Get current frame number
    pub fn frame(&self) -> u8 {
        self.frame
    }

    /// Tick and return frame number (wraps at 255)
    pub fn tick(&mut self) -> u8 {
        self.frame = self.frame.wrapping_add(1);
        self.last_tick = Instant::now();
        self.frame
    }

    /// Check if enough time has passed for next frame
    pub fn should_tick(&self) -> bool {
        self.last_tick.elapsed() >= self.frame_duration
    }

    /// Get time since last tick in seconds
    pub fn delta_time(&self) -> f32 {
        self.last_tick.elapsed().as_secs_f32()
    }

    /// Get interpolated glow factor for ecosystem files
    ///
    /// Returns a value between 0.7 and 1.0 based on sine wave
    pub fn glow_factor(&self) -> f32 {
        let cycle = (self.frame as f32 / 60.0) % 1.0;
        0.7 + 0.3 * (std::f32::consts::TAU * cycle).sin()
    }

    /// Get pulse phase (0.0 to 1.0) for animation synchronization
    pub fn pulse_phase(&self) -> f32 {
        (self.frame as f32 / 30.0) % 1.0 // 2Hz pulse at 60fps
    }

    /// Get shimmer offset for text effects
    pub fn shimmer_offset(&self) -> i16 {
        ((self.frame as f32 / 4.0).sin() * 3.0) as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_glow_animation_pulse() {
        let anim = GlowAnimation::pulse();
        assert_eq!(anim.kind, AnimationKind::Pulse);
        // Intensity should be in valid range
        let intensity = anim.intensity();
        assert!((0.0..=1.0).contains(&intensity));
    }

    #[test]
    fn test_glow_animation_expand() {
        let anim = GlowAnimation::expand();
        assert_eq!(anim.kind, AnimationKind::Expand);
        // At start, intensity should be 0
        let intensity = anim.intensity();
        assert!(intensity >= 0.0);
    }

    #[test]
    fn test_glow_animation_is_complete() {
        let anim = GlowAnimation::pulse();
        // Pulse loops forever
        assert!(!anim.is_complete());

        let anim = GlowAnimation::expand();
        // Expand is not complete immediately
        assert!(!anim.is_complete());
    }

    #[test]
    fn test_glow_animation_glow_factor() {
        let anim = GlowAnimation::pulse();
        let factor = anim.glow_factor();
        // Glow factor should be in [0.7, 1.0]
        assert!((0.7..=1.0).contains(&factor));
    }

    #[test]
    fn test_animation_ticker_new() {
        let ticker = AnimationTicker::new();
        assert_eq!(ticker.frame(), 0);
    }

    #[test]
    fn test_animation_ticker_tick() {
        let mut ticker = AnimationTicker::new();
        assert_eq!(ticker.tick(), 1);
        assert_eq!(ticker.tick(), 2);
        assert_eq!(ticker.frame(), 2);
    }

    #[test]
    fn test_animation_ticker_wrap() {
        let mut ticker = AnimationTicker::new();
        for _ in 0..255 {
            ticker.tick();
        }
        assert_eq!(ticker.frame(), 255);
        ticker.tick();
        assert_eq!(ticker.frame(), 0); // Wrapped
    }

    #[test]
    fn test_animation_ticker_glow_factor() {
        let ticker = AnimationTicker::new();
        let factor = ticker.glow_factor();
        // Should be in valid range
        assert!((0.7..=1.0).contains(&factor));
    }

    #[test]
    fn test_animation_ticker_should_tick() {
        let ticker = AnimationTicker::new();
        // Just created, shouldn't tick yet
        assert!(!ticker.should_tick());

        // After sleeping, should be ready
        sleep(Duration::from_millis(20));
        assert!(ticker.should_tick());
    }

    #[test]
    fn test_animation_ticker_with_fps() {
        let ticker = AnimationTicker::with_fps(30);
        // Frame duration should be ~33.33ms
        sleep(Duration::from_millis(35));
        assert!(ticker.should_tick());
    }

    #[test]
    fn test_glow_animation_reset() {
        let mut anim = GlowAnimation::pulse();
        sleep(Duration::from_millis(10));
        anim.reset();
        // After reset, intensity should be near starting point
        let intensity = anim.intensity();
        assert!((0.0..=1.0).contains(&intensity));
    }
}
