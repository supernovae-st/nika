// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The colour seam + glyph state machine (spec §3.1/§3.4).
//!
//! ONE rule: semantic, never decorative. Cyan is the single accent
//! (running); green/red/yellow are verdicts; dim is metadata. Meaning never
//! lives in colour alone — the glyph survives every colour loss. Both glyph
//! themes (unicode + ASCII) are first-class: the ASCII column is what CI
//! logs and legacy terminals render, snapshot-pinned like the unicode one.

use crate::display::state::TaskState;

/// Braille spinner frames (80ms cadence at the call site).
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Token-arrival sparkline ramp (low → high).
pub const SPARK: [char; 7] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇'];

/// Semantic colour roles (the closed set — nothing decorative can exist).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The single accent — the running line.
    Accent,
    /// Success verdict.
    Good,
    /// Failure verdict.
    Bad,
    /// Transient-recovery verdict (retry).
    Warn,
    /// Metadata (notes · meters · paths).
    Dim,
    /// Emphasis (workflow name · failure headline).
    Strong,
}

impl Role {
    const fn sgr(self) -> &'static str {
        match self {
            Self::Accent => "36",
            Self::Good => "32",
            Self::Bad => "31",
            Self::Warn => "33",
            Self::Dim => "2",
            Self::Strong => "1",
        }
    }
}

/// One theme = colour on/off × glyph family × motion.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Emit ANSI colour (resolved from `--color`/`NO_COLOR`/TTY upstream).
    pub color: bool,
    /// Use the ASCII glyph column (CI logs · legacy conhost · `--ascii`).
    pub ascii: bool,
    /// Animate the running glyph (TTY + motion allowed).
    pub animate: bool,
}

impl Theme {
    /// Paint `text` in a semantic role (no-op when colour is off).
    #[must_use]
    pub fn paint(&self, role: Role, text: &str) -> String {
        if self.color {
            format!("\x1b[{}m{text}\x1b[0m", role.sgr())
        } else {
            text.to_owned()
        }
    }

    /// The state glyph, padded to a stable 2-cell column BEFORE painting
    /// (ANSI escapes break width arithmetic after).
    #[must_use]
    pub fn glyph(&self, state: TaskState, tick: usize) -> String {
        if state == TaskState::Running && self.animate && !self.ascii {
            let frame = SPINNER[tick % SPINNER.len()];
            return self.paint(Role::Accent, &format!("{frame} "));
        }
        let raw = if self.ascii {
            match state {
                TaskState::Pending => ". ",
                TaskState::Running => "> ",
                TaskState::Ok => "ok",
                TaskState::Failed => "X ",
                TaskState::Skipped => "- ",
            }
        } else {
            match state {
                TaskState::Pending => "○ ",
                TaskState::Running => "◐ ",
                TaskState::Ok => "✔ ",
                TaskState::Failed => "✖ ",
                TaskState::Skipped => "⊘ ",
            }
        };
        let role = match state {
            TaskState::Running => Role::Accent,
            TaskState::Ok => Role::Good,
            TaskState::Failed => Role::Bad,
            TaskState::Pending | TaskState::Skipped => Role::Dim,
        };
        self.paint(role, raw)
    }

    /// The brand mark for the header line.
    #[must_use]
    pub fn logo(&self) -> &'static str {
        if self.ascii { "[nika]" } else { "🦋" }
    }

    /// Render a 3-sample sparkline from token-arrival counts.
    #[must_use]
    pub fn sparkline(&self, samples: &[u64]) -> String {
        if self.ascii || samples.is_empty() {
            return String::new();
        }
        let tail = &samples[samples.len().saturating_sub(3)..];
        let top = tail.iter().copied().max().unwrap_or(1).max(1);
        let bars: String = tail
            .iter()
            .map(|&v| {
                // v ≤ top by construction, so idx ≤ 6 — try_from is the
                // lint-clean way to say it.
                let idx =
                    usize::try_from(v * (SPARK.len() as u64 - 1) / top).unwrap_or(SPARK.len() - 1);
                SPARK[idx.min(SPARK.len() - 1)]
            })
            .collect();
        self.paint(Role::Accent, &bars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Theme = Theme {
        color: false,
        ascii: false,
        animate: false,
    };
    const ASCII: Theme = Theme {
        color: false,
        ascii: true,
        animate: false,
    };

    #[test]
    fn every_state_has_a_two_cell_glyph_in_both_themes() {
        for state in [
            TaskState::Pending,
            TaskState::Running,
            TaskState::Ok,
            TaskState::Failed,
            TaskState::Skipped,
        ] {
            assert_eq!(ASCII.glyph(state, 0).chars().count(), 2, "{state:?} ascii");
            assert_eq!(
                PLAIN.glyph(state, 0).chars().count(),
                2,
                "{state:?} unicode"
            );
        }
    }

    #[test]
    fn colour_off_means_zero_escapes() {
        let s = PLAIN.paint(Role::Bad, "boom");
        assert_eq!(s, "boom");
        let on = Theme {
            color: true,
            ..PLAIN
        };
        assert!(on.paint(Role::Bad, "boom").contains("\x1b[31m"));
    }

    #[test]
    fn spinner_only_animates_unicode_running() {
        let animated = Theme {
            animate: true,
            ..PLAIN
        };
        let a = animated.glyph(TaskState::Running, 0);
        let b = animated.glyph(TaskState::Running, 1);
        assert_ne!(a, b, "ticks advance the frame");
        // ASCII theme never animates (CI-stable by construction).
        let ascii_anim = Theme {
            animate: true,
            ..ASCII
        };
        assert_eq!(
            ascii_anim.glyph(TaskState::Running, 0),
            ascii_anim.glyph(TaskState::Running, 7)
        );
    }

    #[test]
    fn sparkline_scales_to_its_max_and_is_ascii_silent() {
        assert_eq!(ASCII.sparkline(&[1, 2, 3]), "");
        let bars = PLAIN.sparkline(&[1, 4, 8]);
        assert_eq!(bars.chars().count(), 3);
        assert!(
            bars.ends_with(SPARK[SPARK.len() - 1]),
            "max sample = top bar"
        );
    }
}
