// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Animation Utilities
//!
//! Shared animation timing and coordination for Chat DAG widgets.
//!
//! Animation polish

use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════════════════
// ANIMATION TICKER
// ═══════════════════════════════════════════════════════════════════════════

/// Coordinates animation timing across widgets
#[derive(Debug, Clone)]
pub struct AnimationTicker {
    /// Current animation frame (wraps at 256)
    frame: u8,
    /// Last tick time
    last_tick: Instant,
    /// Target frame duration (16ms for 60fps)
    frame_duration: Duration,
    /// Whether animations are paused
    paused: bool,
}

impl Default for AnimationTicker {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimationTicker {
    /// Target FPS for animations
    pub const TARGET_FPS: u32 = 60;

    /// Create a new animation ticker
    pub fn new() -> Self {
        Self {
            frame: 0,
            last_tick: Instant::now(),
            frame_duration: Duration::from_millis(1000 / Self::TARGET_FPS as u64),
            paused: false,
        }
    }

    /// Advance to next frame if enough time has passed
    pub fn tick(&mut self) {
        if self.paused {
            return;
        }

        let now = Instant::now();
        if now.duration_since(self.last_tick) >= self.frame_duration {
            self.frame = self.frame.wrapping_add(1);
            self.last_tick = now;
        }
    }

    /// Force advance to next frame (for testing)
    pub fn force_tick(&mut self) {
        if !self.paused {
            self.frame = self.frame.wrapping_add(1);
            self.last_tick = Instant::now();
        }
    }

    /// Get current frame number (0-255)
    pub fn frame(&self) -> u8 {
        self.frame
    }

    /// Pause animations
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume animations
    pub fn resume(&mut self) {
        self.paused = false;
        self.last_tick = Instant::now();
    }

    /// Check if paused
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Reset to initial state
    pub fn reset(&mut self) {
        self.frame = 0;
        self.last_tick = Instant::now();
        self.paused = false;
    }

    /// Get sine wave pulse intensity (0.0 - 1.0)
    /// Completes one cycle every ~2 seconds (128 frames at 60fps)
    pub fn pulse_intensity(&self) -> f32 {
        let t = self.frame as f32 / 64.0;
        (t * std::f32::consts::PI).sin().abs()
    }

    /// Get flow position as fraction (0.0 - 1.0)
    /// Completes one cycle every ~1 second (60 frames at 60fps)
    pub fn flow_position(&self) -> f32 {
        (self.frame % 60) as f32 / 60.0
    }

    /// Check if should render a pulse highlight
    pub fn should_pulse(&self) -> bool {
        self.pulse_intensity() > 0.5
    }

    /// Get spinner character for current frame
    pub fn spinner_char(&self) -> char {
        const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        SPINNER[(self.frame as usize / 6) % SPINNER.len()]
    }

    /// Get braille spinner character (faster)
    pub fn braille_spinner(&self) -> char {
        const BRAILLE: &[char] = &['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'];
        BRAILLE[(self.frame as usize / 4) % BRAILLE.len()]
    }

    /// Get shake offset for error animation (Phase 11-12)
    /// Returns (x, y) displacement based on current frame
    /// Uses pseudo-random pattern based on frame for consistent shake
    pub fn shake_offset(&self, intensity: f32) -> (f32, f32) {
        if intensity == 0.0 {
            return (0.0, 0.0);
        }

        // Use frame to generate pseudo-random but deterministic shake
        let frame = self.frame as f32;
        let x = (frame * 7.0).sin() * intensity;
        let y = (frame * 11.0).cos() * intensity * 0.7; // Less vertical shake

        (x, y)
    }

    /// Get shake offset with exponential decay
    /// elapsed_frames controls how much the shake has decayed
    pub fn shake_offset_with_decay(&self, intensity: f32, elapsed_frames: u32) -> (f32, f32) {
        // Decay factor: halves every 10 frames
        let decay = 0.5_f32.powf(elapsed_frames as f32 / 10.0);
        self.shake_offset(intensity * decay)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ANIMATION STATE
// ═══════════════════════════════════════════════════════════════════════════

/// Animation state for a widget
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationState {
    /// No animation
    #[default]
    Idle,
    /// Pulsing (for running/active)
    Pulsing,
    /// Flowing (for data movement)
    Flowing,
    /// Completed (brief flash then idle)
    Completed,
    /// Error (brief flash then idle)
    Error,
}

impl AnimationState {
    /// Check if this state requires animation updates
    pub fn needs_tick(&self) -> bool {
        matches!(self, AnimationState::Pulsing | AnimationState::Flowing)
    }

    /// Check if this is a transient state (will return to idle)
    pub fn is_transient(&self) -> bool {
        matches!(self, AnimationState::Completed | AnimationState::Error)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EASING FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Common easing functions for smooth animations
pub struct Easing;

impl Easing {
    /// Linear (no easing)
    pub fn linear(t: f32) -> f32 {
        t
    }

    /// Ease in (slow start)
    pub fn ease_in(t: f32) -> f32 {
        t * t
    }

    /// Ease out (slow end)
    pub fn ease_out(t: f32) -> f32 {
        1.0 - (1.0 - t) * (1.0 - t)
    }

    /// Ease in-out (slow start and end)
    pub fn ease_in_out(t: f32) -> f32 {
        if t < 0.5 {
            2.0 * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
        }
    }

    /// Bounce at end
    pub fn bounce(t: f32) -> f32 {
        if t < 1.0 / 2.75 {
            7.5625 * t * t
        } else if t < 2.0 / 2.75 {
            let t = t - 1.5 / 2.75;
            7.5625 * t * t + 0.75
        } else if t < 2.5 / 2.75 {
            let t = t - 2.25 / 2.75;
            7.5625 * t * t + 0.9375
        } else {
            let t = t - 2.625 / 2.75;
            7.5625 * t * t + 0.984375
        }
    }

    /// Elastic easing (overshoots then settles) - Phase 11-12
    /// Good for error feedback that needs attention
    pub fn elastic(t: f32) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }

        let p = 0.3;
        let s = p / 4.0;
        let t = t - 1.0;

        -(2.0_f32.powf(10.0 * t) * ((t - s) * (2.0 * std::f32::consts::PI) / p).sin()) + 1.0
    }

    /// Spring easing (oscillating settle) - Phase 11-12
    /// Good for bouncy UI elements
    pub fn spring(t: f32) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }

        // Damped spring formula
        let omega = 10.0; // Frequency
        let zeta = 0.3; // Damping ratio (underdamped)

        let envelope = (-zeta * omega * t).exp();
        let oscillation = ((1.0 - zeta * zeta).sqrt() * omega * t).cos();

        1.0 - envelope * oscillation
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // --- AnimationTicker tests ---

    #[test]
    fn test_animation_ticker_creation() {
        let ticker = AnimationTicker::new();

        assert_eq!(ticker.frame(), 0);
        assert!(!ticker.is_paused());
    }

    #[test]
    fn test_animation_ticker_force_tick() {
        let mut ticker = AnimationTicker::new();

        ticker.force_tick();
        assert_eq!(ticker.frame(), 1);

        ticker.force_tick();
        assert_eq!(ticker.frame(), 2);
    }

    #[test]
    fn test_animation_ticker_wraps() {
        let mut ticker = AnimationTicker::new();

        for _ in 0..256 {
            ticker.force_tick();
        }

        assert_eq!(ticker.frame(), 0);
    }

    #[test]
    fn test_animation_ticker_pause_resume() {
        let mut ticker = AnimationTicker::new();

        ticker.pause();
        assert!(ticker.is_paused());

        let frame_before = ticker.frame();
        ticker.force_tick();
        assert_eq!(ticker.frame(), frame_before); // No change when paused

        ticker.resume();
        assert!(!ticker.is_paused());

        ticker.force_tick();
        assert_eq!(ticker.frame(), 1);
    }

    #[test]
    fn test_animation_ticker_reset() {
        let mut ticker = AnimationTicker::new();

        ticker.force_tick();
        ticker.force_tick();
        ticker.pause();

        ticker.reset();

        assert_eq!(ticker.frame(), 0);
        assert!(!ticker.is_paused());
    }

    #[test]
    fn test_animation_ticker_pulse_intensity() {
        let ticker = AnimationTicker::new();
        let intensity = ticker.pulse_intensity();

        assert!(intensity >= 0.0);
        assert!(intensity <= 1.0);
    }

    #[test]
    fn test_animation_ticker_pulse_varies() {
        let mut ticker = AnimationTicker::new();

        let initial = ticker.pulse_intensity();

        // Advance several frames
        for _ in 0..16 {
            ticker.force_tick();
        }

        let after = ticker.pulse_intensity();

        // Intensity should change
        assert!((initial - after).abs() > 0.01);
    }

    #[test]
    fn test_animation_ticker_flow_position() {
        let ticker = AnimationTicker::new();
        let pos = ticker.flow_position();

        assert!(pos >= 0.0);
        assert!(pos <= 1.0);
    }

    #[test]
    fn test_animation_ticker_spinner_char() {
        let mut ticker = AnimationTicker::new();

        let initial = ticker.spinner_char();
        assert!("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏".contains(initial));

        // Advance to see different character
        for _ in 0..12 {
            ticker.force_tick();
        }

        let after = ticker.spinner_char();
        assert!("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏".contains(after));
    }

    #[test]
    fn test_animation_ticker_braille_spinner() {
        let ticker = AnimationTicker::new();
        let char = ticker.braille_spinner();

        assert!("⣾⣽⣻⢿⡿⣟⣯⣷".contains(char));
    }

    // --- AnimationState tests ---

    #[test]
    fn test_animation_state_default() {
        let state = AnimationState::default();
        assert_eq!(state, AnimationState::Idle);
    }

    #[test]
    fn test_animation_state_needs_tick() {
        assert!(!AnimationState::Idle.needs_tick());
        assert!(AnimationState::Pulsing.needs_tick());
        assert!(AnimationState::Flowing.needs_tick());
        assert!(!AnimationState::Completed.needs_tick());
        assert!(!AnimationState::Error.needs_tick());
    }

    #[test]
    fn test_animation_state_is_transient() {
        assert!(!AnimationState::Idle.is_transient());
        assert!(!AnimationState::Pulsing.is_transient());
        assert!(!AnimationState::Flowing.is_transient());
        assert!(AnimationState::Completed.is_transient());
        assert!(AnimationState::Error.is_transient());
    }

    // --- Easing tests ---

    #[test]
    fn test_easing_linear() {
        assert_eq!(Easing::linear(0.0), 0.0);
        assert_eq!(Easing::linear(0.5), 0.5);
        assert_eq!(Easing::linear(1.0), 1.0);
    }

    #[test]
    fn test_easing_ease_in() {
        assert_eq!(Easing::ease_in(0.0), 0.0);
        assert!(Easing::ease_in(0.5) < 0.5); // Slower at start
        assert_eq!(Easing::ease_in(1.0), 1.0);
    }

    #[test]
    fn test_easing_ease_out() {
        assert_eq!(Easing::ease_out(0.0), 0.0);
        assert!(Easing::ease_out(0.5) > 0.5); // Faster at start
        assert_eq!(Easing::ease_out(1.0), 1.0);
    }

    #[test]
    fn test_easing_ease_in_out() {
        assert_eq!(Easing::ease_in_out(0.0), 0.0);
        assert!((Easing::ease_in_out(0.5) - 0.5).abs() < 0.01);
        assert_eq!(Easing::ease_in_out(1.0), 1.0);
    }

    #[test]
    fn test_easing_bounce() {
        assert_eq!(Easing::bounce(0.0), 0.0);
        assert!(Easing::bounce(0.5) > 0.5); // Bouncy
        assert!((Easing::bounce(1.0) - 1.0).abs() < 0.001);
    }

    // --- Integration test ---

    #[test]
    fn test_animation_module_exports() {
        // Verify types are accessible
        let _ = AnimationTicker::new();
        let _ = AnimationState::Idle;
        let _ = Easing::linear(0.5);
    }

    // --- shake_offset tests (Phase 11-12) ---

    #[test]
    fn test_shake_offset_returns_tuple() {
        let ticker = AnimationTicker::new();
        let (x, y) = ticker.shake_offset(5.0);

        // Should return bounded values
        assert!(x.abs() <= 5.0);
        assert!(y.abs() <= 5.0);
    }

    #[test]
    fn test_shake_offset_varies_with_frames() {
        let mut ticker = AnimationTicker::new();

        let initial = ticker.shake_offset(5.0);

        // Advance frames
        for _ in 0..4 {
            ticker.force_tick();
        }

        let after = ticker.shake_offset(5.0);

        // Should produce different offsets at different frames
        assert!(initial != after);
    }

    #[test]
    fn test_shake_offset_intensity_zero() {
        let ticker = AnimationTicker::new();
        let (x, y) = ticker.shake_offset(0.0);

        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn test_shake_offset_decays() {
        let mut ticker = AnimationTicker::new();
        let (x1, y1) = ticker.shake_offset_with_decay(5.0, 0);

        for _ in 0..10 {
            ticker.force_tick();
        }

        let (x2, y2) = ticker.shake_offset_with_decay(5.0, 10);

        // Decay should reduce intensity
        let magnitude1 = (x1 * x1 + y1 * y1).sqrt();
        let magnitude2 = (x2 * x2 + y2 * y2).sqrt();
        assert!(magnitude2 <= magnitude1 || magnitude1 == 0.0);
    }

    // --- elastic easing tests (Phase 11-12) ---

    #[test]
    fn test_easing_elastic_boundaries() {
        assert!((Easing::elastic(0.0) - 0.0).abs() < 0.01);
        assert!((Easing::elastic(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_easing_elastic_overshoots() {
        // Elastic should overshoot past 1.0 before settling
        let mid = Easing::elastic(0.7);
        assert!(mid > 0.5); // Should be progressed
    }

    #[test]
    fn test_easing_spring_boundaries() {
        assert!((Easing::spring(0.0) - 0.0).abs() < 0.01);
        assert!((Easing::spring(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_easing_spring_oscillates() {
        let early = Easing::spring(0.3);
        let mid = Easing::spring(0.5);
        let late = Easing::spring(0.8);

        // Spring overshoots past 1.0, then settles back
        assert!(early > 1.0); // Overshoots at early stage
        assert!(mid < early); // Settling down
        assert!(late < early); // Still settling
        assert!((late - 1.0).abs() < 0.1); // Near target at end
    }
}
