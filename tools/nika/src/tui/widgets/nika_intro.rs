//! Nika Intro Widget - ASCII art logo that explodes into matrix rain
//!
//! A startup animation showing "NIKA" in ASCII art, then exploding
//! into matrix rain drops for a dramatic effect.
//!
//! ```text
//! Phase 1: ASCII art appears (fade in)
//!    ███╗   ██╗██╗██╗  ██╗ █████╗
//!    ████╗  ██║██║██║ ██╔╝██╔══██╗
//!    ██╔██╗ ██║██║█████╔╝ ███████║
//!    ██║╚██╗██║██║██╔═██╗ ██╔══██║
//!    ██║ ╚████║██║██║  ██╗██║  ██║
//!    ╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝  ╚═╝
//!
//! Phase 2: Letters explode into particles
//! Phase 3: Particles become matrix rain
//! ```

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::tui::theme::solarized;

// ═══════════════════════════════════════════════════════════════════════════════
// ASCII ART
// ═══════════════════════════════════════════════════════════════════════════════

/// NIKA ASCII art logo (6 lines, ~40 chars wide)
const NIKA_ASCII: &[&str] = &[
    "███╗   ██╗██╗██╗  ██╗ █████╗ ",
    "████╗  ██║██║██║ ██╔╝██╔══██╗",
    "██╔██╗ ██║██║█████╔╝ ███████║",
    "██║╚██╗██║██║██╔═██╗ ██╔══██║",
    "██║ ╚████║██║██║  ██╗██║  ██║",
    "╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝  ╚═╝",
];

/// Alternative smaller logo for compact terminals
const NIKA_SMALL: &[&str] = &["╔╗╔ ╦ ╦╔═ ╔═╗", "║║║ ║ ╠╩╗ ╠═╣", "╝╚╝ ╩ ╩ ╩ ╩ ╩"];

/// Particle characters for explosion effect
const EXPLOSION_CHARS: &[char] = &[
    '█', '▓', '▒', '░', '╬', '╫', '╪', '┼', '╋', '┃', '━', '┏', '┓', '┗', '┛', '◆', '◇', '○', '●',
    '◐', '◑', '◒', '◓', '★', '☆', '✦', '✧',
];

/// Explosion colors (bright to faded)
const EXPLOSION_COLORS: &[Color] = &[
    solarized::CYAN,
    solarized::GREEN,
    solarized::BLUE,
    solarized::VIOLET,
    solarized::MAGENTA,
    solarized::YELLOW,
    solarized::ORANGE,
];

// ═══════════════════════════════════════════════════════════════════════════════
// PARTICLE
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    char: char,
    color: Color,
    life: f32, // 1.0 = full, 0.0 = dead
}

// ═══════════════════════════════════════════════════════════════════════════════
// INTRO STATE
// ═══════════════════════════════════════════════════════════════════════════════

/// Animation phases
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntroPhase {
    /// Logo fading in
    FadeIn,
    /// Logo visible, waiting
    Hold,
    /// Logo exploding into particles
    Explode,
    /// Particles falling as rain
    Rain,
    /// Animation complete
    Done,
}

/// Nika intro animation state
pub struct NikaIntroState {
    pub phase: IntroPhase,
    pub frame: u16,
    pub opacity: f32,
    particles: Vec<Particle>,
    rng: SmallRng,
}

impl Default for NikaIntroState {
    fn default() -> Self {
        Self::new()
    }
}

impl NikaIntroState {
    pub fn new() -> Self {
        Self {
            phase: IntroPhase::FadeIn,
            frame: 0,
            opacity: 0.0,
            particles: Vec::new(),
            rng: SmallRng::seed_from_u64(42),
        }
    }

    /// Check if intro animation is complete
    pub fn is_done(&self) -> bool {
        self.phase == IntroPhase::Done
    }

    /// Tick animation (call at 10Hz)
    pub fn tick(&mut self, area: Rect) {
        self.frame = self.frame.wrapping_add(1);

        match self.phase {
            IntroPhase::FadeIn => {
                self.opacity += 0.08; // ~1.2s fade in
                if self.opacity >= 1.0 {
                    self.opacity = 1.0;
                    self.phase = IntroPhase::Hold;
                    self.frame = 0; // Reset for hold timing
                }
            }
            IntroPhase::Hold => {
                // Hold for ~0.5s (5 frames at 10Hz)
                if self.frame > 5 {
                    self.phase = IntroPhase::Explode;
                    self.frame = 0; // Reset for explode timing
                    self.spawn_particles(area);
                }
            }
            IntroPhase::Explode => {
                self.update_particles();
                // Transition to rain after particles settle (~1.5s)
                if self.frame > 15 {
                    self.phase = IntroPhase::Rain;
                    self.frame = 0; // Reset for rain timing
                }
            }
            IntroPhase::Rain => {
                self.update_particles();
                self.opacity -= 0.04; // Fade out particles
                if self.opacity <= 0.0 || self.frame > 50 {
                    self.phase = IntroPhase::Done;
                }
            }
            IntroPhase::Done => {}
        }
    }

    /// Spawn explosion particles from logo position
    fn spawn_particles(&mut self, area: Rect) {
        let logo = NIKA_ASCII;
        let logo_width = logo[0].chars().count() as u16;
        let logo_height = logo.len() as u16;

        // Center position
        let start_x = (area.width.saturating_sub(logo_width)) / 2;
        let start_y = (area.height.saturating_sub(logo_height)) / 2;

        // Create particles from each character position
        for (row, line) in logo.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                if ch != ' ' && ch != '╝' && ch != '╚' && ch != '═' {
                    let x = start_x as f32 + col as f32;
                    let y = start_y as f32 + row as f32;

                    // Random explosion velocity
                    let angle: f32 = self.rng.gen_range(0.0..std::f32::consts::TAU);
                    let speed: f32 = self.rng.gen_range(0.5..2.5);
                    let vx = angle.cos() * speed;
                    let vy = angle.sin() * speed + 0.5; // Slight downward bias

                    let color_idx = self.rng.gen_range(0..EXPLOSION_COLORS.len());
                    let char_idx = self.rng.gen_range(0..EXPLOSION_CHARS.len());

                    self.particles.push(Particle {
                        x,
                        y,
                        vx,
                        vy,
                        char: EXPLOSION_CHARS[char_idx],
                        color: EXPLOSION_COLORS[color_idx],
                        life: 1.0,
                    });
                }
            }
        }
    }

    /// Update particle physics
    fn update_particles(&mut self) {
        for p in &mut self.particles {
            p.x += p.vx;
            p.y += p.vy;
            p.vy += 0.15; // Gravity
            p.vx *= 0.98; // Air resistance
            p.life -= 0.02;

            // Randomly change character for glitchy effect
            if self.rng.gen::<f32>() < 0.1 {
                let idx = self.rng.gen_range(0..EXPLOSION_CHARS.len());
                p.char = EXPLOSION_CHARS[idx];
            }
        }

        // Remove dead particles
        self.particles.retain(|p| p.life > 0.0);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// WIDGET
// ═══════════════════════════════════════════════════════════════════════════════

/// Nika intro animation widget
pub struct NikaIntro<'a> {
    state: &'a NikaIntroState,
}

impl<'a> NikaIntro<'a> {
    pub fn new(state: &'a NikaIntroState) -> Self {
        Self { state }
    }
}

impl Widget for NikaIntro<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // v0.22.1 FIX: Fill background first to ensure clean slate
        // Without this, random terminal artifacts could show through
        let bg_style = Style::default().bg(solarized::BASE03);
        buf.set_style(area, bg_style);

        match self.state.phase {
            IntroPhase::FadeIn | IntroPhase::Hold => {
                // Render ASCII logo centered
                let logo = if area.width >= 40 {
                    NIKA_ASCII
                } else {
                    NIKA_SMALL
                };
                let logo_width = logo[0].chars().count() as u16;
                let logo_height = logo.len() as u16;

                let start_x = area.x + (area.width.saturating_sub(logo_width)) / 2;
                let start_y = area.y + (area.height.saturating_sub(logo_height)) / 2;

                let opacity = self.state.opacity;
                let color = apply_opacity(solarized::CYAN, opacity);

                for (row, line) in logo.iter().enumerate() {
                    let y = start_y + row as u16;
                    if y < area.y + area.height {
                        buf.set_string(start_x, y, line, Style::default().fg(color));
                    }
                }

                // Add 🦋 decorations
                if opacity > 0.5 {
                    let butterfly = "🦋";
                    // Top left
                    if start_x > 4 && start_y > 1 {
                        buf.set_string(
                            start_x - 4,
                            start_y - 1,
                            butterfly,
                            Style::default().fg(solarized::MAGENTA),
                        );
                    }
                    // Top right
                    if start_x + logo_width + 2 < area.x + area.width {
                        buf.set_string(
                            start_x + logo_width + 2,
                            start_y - 1,
                            butterfly,
                            Style::default().fg(solarized::VIOLET),
                        );
                    }
                    // Bottom center
                    if start_y + logo_height < area.y + area.height {
                        buf.set_string(
                            start_x + logo_width / 2,
                            start_y + logo_height,
                            butterfly,
                            Style::default().fg(solarized::CYAN),
                        );
                    }
                }
            }
            IntroPhase::Explode | IntroPhase::Rain => {
                // Render particles
                for p in &self.state.particles {
                    let x = p.x as u16;
                    let y = p.y as u16;

                    if x >= area.x
                        && x < area.x + area.width
                        && y >= area.y
                        && y < area.y + area.height
                    {
                        let color = apply_opacity(p.color, p.life * self.state.opacity);
                        buf.set_string(x, y, p.char.to_string(), Style::default().fg(color));
                    }
                }
            }
            IntroPhase::Done => {}
        }
    }
}

/// Apply opacity to a color
fn apply_opacity(color: Color, opacity: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * opacity) as u8,
            (g as f32 * opacity) as u8,
            (b as f32 * opacity) as u8,
        ),
        c => c,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intro_state_new() {
        let state = NikaIntroState::new();
        assert_eq!(state.phase, IntroPhase::FadeIn);
        assert_eq!(state.frame, 0);
        assert!((state.opacity - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_intro_phase_progression() {
        let mut state = NikaIntroState::new();
        let area = Rect::new(0, 0, 80, 24);

        // Tick through fade in (~13 ticks at 0.08 opacity per tick)
        for _ in 0..13 {
            state.tick(area);
        }
        assert_eq!(state.phase, IntroPhase::Hold);

        // Tick through hold (>5 frames)
        for _ in 0..6 {
            state.tick(area);
        }
        assert_eq!(state.phase, IntroPhase::Explode);
    }

    #[test]
    fn test_intro_is_done() {
        let mut state = NikaIntroState::new();
        let area = Rect::new(0, 0, 80, 24);

        assert!(!state.is_done());

        // Fast forward to done
        for _ in 0..100 {
            state.tick(area);
        }
        assert!(state.is_done());
    }

    #[test]
    fn test_particle_spawning() {
        let mut state = NikaIntroState::new();
        let area = Rect::new(0, 0, 80, 24);

        // Get to explode phase
        state.phase = IntroPhase::Hold;
        state.frame = 13;
        state.tick(area);

        assert!(!state.particles.is_empty());
    }

    #[test]
    fn test_render_empty_area() {
        let state = NikaIntroState::new();
        let intro = NikaIntro::new(&state);
        let area = Rect::new(0, 0, 0, 0);
        let mut buf = Buffer::empty(area);
        intro.render(area, &mut buf);
        // Should not panic
    }
}
