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

/// One theme = colour on/off × glyph family × motion × capability tier.
// Five INDEPENDENT render knobs, not a state machine — the same
// flags-are-flags exemption the clap arg structs carry (main.rs).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Emit ANSI colour (resolved through the ONE priority chain —
    /// `display::format::color_enabled` · `--color` > `CLICOLOR_FORCE` >
    /// `NO_COLOR` > `CLICOLOR=0` > TTY+`TERM≠dumb`).
    pub color: bool,
    /// Use the ASCII glyph column (CI logs · legacy conhost · `--ascii`).
    pub ascii: bool,
    /// Animate the running glyph (TTY + motion allowed).
    pub animate: bool,
    /// Truecolor DATA ramps allowed (`COLORTERM` said truecolor/24bit ·
    /// the duration-heat waterfall). Chrome NEVER rides this — the
    /// ANSI-16 `Role` slots stay the chrome vocabulary (format.rs doc).
    pub heat: bool,
    /// Emit OSC-8 hyperlinks (`--hyperlink` · auto = TTY + `TERM≠dumb` ·
    /// never to pipes).
    pub links: bool,
}

impl Theme {
    /// The 3-knob constructor every call site speaks — capability tiers
    /// (`heat` · `links`) default OFF so a bare theme is exactly the
    /// pre-round-8 surface (pipes · CI · tests stay byte-identical).
    #[must_use]
    pub const fn new(color: bool, ascii: bool, animate: bool) -> Self {
        Self {
            color,
            ascii,
            animate,
            heat: false,
            links: false,
        }
    }
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
            // The §3.1 ASCII column (err `X` ≠ blocked `x`). The
            // comprehension pass re-voiced the never-ran pair: a SKIP
            // (a decision — gate false · cache hit) reads `~>` (flows
            // past), a BLOCKED cancellation reads `x` (the path died).
            match state {
                TaskState::Pending => ". ",
                TaskState::Running => "> ",
                TaskState::Ok => "ok",
                TaskState::Failed => "X ",
                TaskState::Retrying => "r ",
                TaskState::Skipped => "~>",
                TaskState::Cancelled => "x ",
            }
        } else {
            match state {
                TaskState::Pending => "○ ",
                TaskState::Running => "◐ ",
                TaskState::Ok => "✔ ",
                TaskState::Failed => "✖ ",
                TaskState::Retrying => "↻ ",
                TaskState::Skipped => "↷ ",
                TaskState::Cancelled => "⊘ ",
            }
        };
        let role = match state {
            TaskState::Running => Role::Accent,
            TaskState::Ok => Role::Good,
            TaskState::Failed => Role::Bad,
            TaskState::Retrying => Role::Warn,
            TaskState::Pending | TaskState::Skipped | TaskState::Cancelled => Role::Dim,
        };
        self.paint(role, raw)
    }

    /// The brand mark for the header line.
    #[must_use]
    pub fn logo(&self) -> &'static str {
        if self.ascii { "[nika]" } else { "🦋" }
    }

    /// Wrap `text` as an OSC-8 hyperlink to `url` — a no-op unless the
    /// `links` capability resolved on (TTY + `--hyperlink` chain), so
    /// every sober register keeps its exact bytes.
    #[must_use]
    pub fn link(&self, url: &str, text: &str) -> String {
        if self.links {
            crate::display::format::osc8(url, text, None)
        } else {
            text.to_owned()
        }
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

    const PLAIN: Theme = Theme::new(false, false, false);
    const ASCII: Theme = Theme::new(false, true, false);

    #[test]
    fn every_state_has_a_two_cell_glyph_in_both_themes() {
        for state in [
            TaskState::Pending,
            TaskState::Running,
            TaskState::Ok,
            TaskState::Failed,
            TaskState::Retrying,
            TaskState::Skipped,
            TaskState::Cancelled,
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
    fn locked_contract_glyphs_for_the_new_states() {
        // §3.1 table (comprehension-pass vocabulary) · retrying ↻/r
        // yellow · skip ↷/~> dim (a decision: gate false · cache hit) ·
        // blocked-cancelled ⊘/x dim · err `X` ≠ blocked `x` (the case
        // IS the distinction).
        assert_eq!(PLAIN.glyph(TaskState::Retrying, 0), "↻ ");
        assert_eq!(ASCII.glyph(TaskState::Retrying, 0), "r ");
        assert_eq!(PLAIN.glyph(TaskState::Skipped, 0), "↷ ");
        assert_eq!(ASCII.glyph(TaskState::Skipped, 0), "~>");
        assert_eq!(PLAIN.glyph(TaskState::Cancelled, 0), "⊘ ");
        assert_eq!(ASCII.glyph(TaskState::Cancelled, 0), "x ");
        assert_ne!(
            ASCII.glyph(TaskState::Failed, 0),
            ASCII.glyph(TaskState::Cancelled, 0)
        );
        let coloured = Theme::new(true, false, false);
        assert!(
            coloured.glyph(TaskState::Retrying, 0).contains("\x1b[33m"),
            "retrying paints yellow (Warn)"
        );
        assert!(
            coloured.glyph(TaskState::Cancelled, 0).contains("\x1b[2m"),
            "cancelled paints dim — a decision, never red"
        );
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

    /// Exact ramp pins — the scaling arithmetic `v·(len−1)/top` is the
    /// semantics (a mutated operator draws a wrong-but-plausible chart,
    /// only exact bars catch it).
    #[test]
    fn sparkline_exact_ramps() {
        // top=8: 1·6/8=0 → ▁ · 4·6/8=3 → ▄ · 8·6/8=6 → ▇
        assert_eq!(PLAIN.sparkline(&[1, 4, 8]), "▁▄▇");
        // top=3: 2·6/3=4 → ▅ · 3·6/3=6 → ▇
        assert_eq!(PLAIN.sparkline(&[2, 3]), "▅▇");
        // Only the LAST 3 samples render: [9,1,2,3] → tail [1,2,3] ·
        // top=3 · 1·6/3=2 → ▃ · 2·6/3=4 → ▅ · 3·6/3=6 → ▇
        assert_eq!(PLAIN.sparkline(&[9, 1, 2, 3]), "▃▅▇");
        // A zero sample floors at the bottom bar (top clamps to ≥1).
        assert_eq!(PLAIN.sparkline(&[0]), "▁");
    }

    /// The scale NUMERATOR is `SPARK.len()-1` (=6), not `SPARK.len()` (=7):
    /// a `/1`-mutated denominator (7 instead of 6) reads the same on the
    /// ramps above but DIVERGES on a near-saturated tail. top=8: 8·6/8=6 →▇,
    /// 7·6/8=5 →▆ — the mutant's 7·7/8=6 would draw ▇ (clamped), so only an
    /// exact near-top sample catches the off-by-one in the scale factor.
    #[test]
    fn sparkline_scale_factor_is_len_minus_one() {
        assert_eq!(PLAIN.sparkline(&[8, 7, 7]), "▇▆▆");
    }
}
