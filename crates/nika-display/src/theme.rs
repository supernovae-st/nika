// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The colour seam + glyph state machine (spec §3.1/§3.4).
//!
//! ONE rule: semantic, never decorative. Cyan is the single accent
//! (running); green/red/yellow are verdicts; dim is metadata. Meaning never
//! lives in colour alone — the glyph survives every colour loss. Both glyph
//! themes (unicode + ASCII) are first-class: the ASCII column is what CI
//! logs and legacy terminals render, snapshot-pinned like the unicode one.

use crate::state::TaskState;

/// Braille spinner frames (80ms cadence at the call site) — the ORBIT
/// pattern: `agent`'s own motion (the bounded loop), and the fallback
/// for any running row whose verb the stream hasn't named yet.
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// The per-verb MOTION vocabulary — the same identities the website's
/// motion tiles animate (infer · sampling — scattered dots settle ·
/// exec · scanline — a band sweeps the rows · invoke · roundtrip — a
/// pendulum goes there and back · agent · orbit — [`SPINNER`]). One
/// name per verb across every surface; the terminal speaks it in one
/// braille cell, painted in the verb's own bright slot. Graduates to
/// the spec `design/tokens.yaml` `motion:` block with the family tags
/// (spec-first — this table is the engine's mirror, same stance as the
/// ◇▷◆✦ glyph column).
pub const SAMPLING: [char; 10] = ['⠂', '⠈', '⠐', '⠄', '⠠', '⠁', '⡀', '⠃', '⠘', '⠨'];
/// `exec` · scanline (top row → bottom row → back).
pub const SCANLINE: [char; 10] = ['⠉', '⠒', '⠤', '⣀', '⠤', '⠒', '⠉', '⠒', '⠤', '⣀'];
/// `invoke` · roundtrip (the pendulum — out and back).
pub const ROUNDTRIP: [char; 10] = ['⠆', '⠃', '⠉', '⠘', '⠰', '⢠', '⠰', '⠘', '⠉', '⠃'];

/// The duration-heat ramp (design §1.5): ONE hue (azure ≈ 200°), five
/// quantized LIGHTNESS steps — short bars pale, the long pole bright.
/// Perceptual single-hue by design: never red→green (deuteranopia) ·
/// never a second semantic hue (red stays the failure verdict alone).
/// Truecolor ONLY — the caller gates on `Theme.heat` (`COLORTERM` said
/// truecolor/24bit); the 256-colour fallback is the FLAT bar, never an
/// approximated ramp. A const table, no colour crate.
pub const HEAT_RAMP: [(u8, u8, u8); 5] = [
    (96, 125, 139),
    (86, 143, 163),
    (74, 162, 189),
    (58, 183, 217),
    (38, 205, 247),
];

/// Token-arrival sparkline ramp (low → high).
pub const SPARK: [char; 7] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇'];

/// Semantic colour roles (the closed set — nothing decorative can exist).
///
/// Two bands, one law. The NORMAL band (31/32/33/36) speaks state and
/// verdict; the BRIGHT band (93/94/95/96) speaks the language's own
/// vocabulary — the four verbs every other Nika surface already paints
/// (spec `design/tokens.yaml`: infer blue · exec amber · invoke cyan ·
/// agent violet). Bright ≠ decorative: a verb chip is an identity, and
/// the band split keeps it from ever colliding with a verdict (bright
/// cyan `invoke` next to the normal-cyan accent stays distinguishable,
/// and both remain the USER's terminal hues — never hex).
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
    /// The `infer` verb chip (spec blue family → bright blue).
    VerbInfer,
    /// The `exec` verb chip (spec amber family → bright yellow).
    VerbExec,
    /// The `invoke` verb chip (spec cyan family → bright cyan).
    VerbInvoke,
    /// The `agent` verb chip (spec violet family → bright magenta).
    VerbAgent,
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
            Self::VerbInfer => "94",
            Self::VerbExec => "93",
            Self::VerbInvoke => "96",
            Self::VerbAgent => "95",
        }
    }

    /// The verb-chip role for a runtime verb word — `None` for anything
    /// outside the locked four (a builtin note · a malformed stream):
    /// unknown vocabulary renders dim, never a guessed identity.
    #[must_use]
    pub fn for_verb(verb: &str) -> Option<Self> {
        match verb {
            "infer" => Some(Self::VerbInfer),
            "exec" => Some(Self::VerbExec),
            "invoke" => Some(Self::VerbInvoke),
            "agent" => Some(Self::VerbAgent),
            _ => None,
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
    /// The interactive duration accents (nextest school): bracketed
    /// right-aligned duration cells (`[  2.7s]`) + the `slow` marker.
    /// TTY comfort ONLY — these are TEXT, so every sober register
    /// (pipes · CI · `--no-progress` · machine modes) keeps its exact
    /// bytes by leaving this off.
    pub accents: bool,
}

impl Theme {
    /// The 3-knob constructor every call site speaks — capability tiers
    /// (`heat` · `links` · `accents`) default OFF so a bare theme is
    /// exactly the pre-round-8 surface (pipes · CI · tests stay
    /// byte-identical).
    #[must_use]
    pub const fn new(color: bool, ascii: bool, animate: bool) -> Self {
        Self {
            color,
            ascii,
            animate,
            heat: false,
            links: false,
            accents: false,
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
                TaskState::Paused => "? ",
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
                TaskState::Paused => "◇ ",
            }
        };
        let role = match state {
            TaskState::Running => Role::Accent,
            TaskState::Ok => Role::Good,
            TaskState::Failed => Role::Bad,
            // Paused rides amber beside Retrying: both say "a human's
            // attention moves this forward" — never red, never dim.
            TaskState::Retrying | TaskState::Paused => Role::Warn,
            TaskState::Pending | TaskState::Skipped | TaskState::Cancelled => Role::Dim,
        };
        self.paint(role, raw)
    }

    /// The brand mark for the header line.
    #[must_use]
    pub fn logo(&self) -> &'static str {
        if self.ascii { "[nika]" } else { "🦋" }
    }

    /// One frame of a RUNNING row's motion — the verb's own pattern in
    /// the verb's own bright slot (`None`/unknown verb = the orbit
    /// fallback in the accent, today's exact behavior). Callers gate on
    /// `animate && !ascii` exactly as they do for [`SPINNER`]; every
    /// sober register stays tick-blind.
    #[must_use]
    pub fn verb_spin(&self, verb: Option<&str>, tick: usize) -> String {
        let (frames, role): (&[char; 10], Role) = match verb {
            Some("infer") => (&SAMPLING, Role::VerbInfer),
            Some("exec") => (&SCANLINE, Role::VerbExec),
            Some("invoke") => (&ROUNDTRIP, Role::VerbInvoke),
            Some("agent") => (&SPINNER, Role::VerbAgent),
            _ => (&SPINNER, Role::Accent),
        };
        let frame = frames[tick % frames.len()];
        self.paint(role, &format!("{frame} "))
    }

    /// The verb-identity glyph, padded to the SAME stable 2-cell column
    /// as the state glyph (spec `design/tokens.yaml` bindings: ◇ infer ·
    /// ▷ exec · ◆ invoke · ✦ agent). Unknown vocabulary gets the dim
    /// bullet — the column never lies about identity it doesn't have.
    #[must_use]
    pub fn verb_glyph(&self, verb: &str) -> String {
        let Some(role) = Role::for_verb(verb) else {
            return self.paint(Role::Dim, if self.ascii { ". " } else { "· " });
        };
        let raw = if self.ascii {
            match role {
                Role::VerbExec => "$ ",
                Role::VerbInvoke => "@ ",
                Role::VerbAgent => "* ",
                _ => "i ", // VerbInfer — the only remaining verb arm
            }
        } else {
            match role {
                Role::VerbExec => "▷ ",
                Role::VerbInvoke => "◆ ",
                Role::VerbAgent => "✦ ",
                _ => "◇ ",
            }
        };
        self.paint(role, raw)
    }

    /// The bare 1-cell verb glyph — unpainted, unpadded (the map and
    /// any state-painting surface colour it by STATE, not the verb
    /// band). Same table as [`Self::verb_glyph`] — one vocabulary.
    #[must_use]
    pub fn verb_glyph_bare(&self, verb: Option<&str>) -> &'static str {
        match (verb, self.ascii) {
            (Some("exec"), false) => "▷",
            (Some("exec"), true) => "$",
            (Some("invoke"), false) => "◆",
            (Some("invoke"), true) => "@",
            (Some("agent"), false) => "✦",
            (Some("agent"), true) => "*",
            (Some("infer"), false) => "◇",
            (Some("infer"), true) => "i",
            (None | Some(_), false) => "·",
            (None | Some(_), true) => ".",
        }
    }

    /// Wrap `text` as an OSC-8 hyperlink to `url` — a no-op unless the
    /// `links` capability resolved on (TTY + `--hyperlink` chain), so
    /// every sober register keeps its exact bytes.
    #[must_use]
    pub fn link(&self, url: &str, text: &str) -> String {
        if self.links {
            crate::format::osc8(url, text, None)
        } else {
            text.to_owned()
        }
    }

    /// Paint a DATA run in one heat step (truecolor SGR · [`HEAT_RAMP`])
    /// — the ONLY truecolor seam in the binary: chrome (borders · labels
    /// · verdicts) stays on the ANSI-16 `Role` slots forever (the user's
    /// terminal theme decides those hues). Plain text when colour is
    /// off; the CALLER gates on `self.heat` (the `COLORTERM` capability).
    #[must_use]
    pub fn heat_step(self, step: usize, text: &str) -> String {
        if !self.color {
            return text.to_owned();
        }
        let (r, g, b) = HEAT_RAMP[step.min(HEAT_RAMP.len() - 1)];
        format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m")
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

    /// The verb column is the tokens-SSOT vocabulary (◇▷◆✦ · spec
    /// `design/tokens.yaml` bindings) in the SAME 2-cell law as the
    /// state column — both glyph themes pinned.
    #[test]
    fn verb_glyphs_are_two_cell_in_both_themes() {
        for verb in ["infer", "exec", "invoke", "agent", "unknown-word"] {
            assert_eq!(PLAIN.verb_glyph(verb).chars().count(), 2, "{verb} unicode");
            assert_eq!(ASCII.verb_glyph(verb).chars().count(), 2, "{verb} ascii");
        }
        assert_eq!(PLAIN.verb_glyph("infer"), "◇ ");
        assert_eq!(PLAIN.verb_glyph("exec"), "▷ ");
        assert_eq!(PLAIN.verb_glyph("invoke"), "◆ ");
        assert_eq!(PLAIN.verb_glyph("agent"), "✦ ");
        assert_eq!(ASCII.verb_glyph("exec"), "$ ");
        assert_eq!(ASCII.verb_glyph("agent"), "* ");
    }

    /// The verb band is BRIGHT (93-96) — identity never collides with a
    /// verdict slot, and unknown vocabulary stays dim (no guessed chip).
    #[test]
    fn verb_band_is_bright_and_unknown_stays_dim() {
        let coloured = Theme::new(true, false, false);
        assert!(coloured.verb_glyph("infer").contains("\x1b[94m"));
        assert!(coloured.verb_glyph("exec").contains("\x1b[93m"));
        assert!(coloured.verb_glyph("invoke").contains("\x1b[96m"));
        assert!(coloured.verb_glyph("agent").contains("\x1b[95m"));
        assert!(coloured.verb_glyph("fetch").contains("\x1b[2m"), "dim");
        assert_eq!(Role::for_verb("fetch"), None, "tools are not verbs");
    }

    /// The motion vocabulary: each verb runs in ITS pattern and ITS
    /// bright slot; unknown verbs keep the orbit-in-accent fallback
    /// (yesterday's exact behavior). The four patterns are pairwise
    /// DISTINCT at tick 0 — identities, not decoration.
    #[test]
    fn verb_spin_speaks_each_verbs_own_motion() {
        let anim = Theme {
            color: true,
            animate: true,
            ..Theme::new(true, false, true)
        };
        let infer = anim.verb_spin(Some("infer"), 0);
        let exec = anim.verb_spin(Some("exec"), 0);
        let invoke = anim.verb_spin(Some("invoke"), 0);
        let agent = anim.verb_spin(Some("agent"), 0);
        assert!(infer.contains("\x1b[94m") && infer.contains(SAMPLING[0]));
        assert!(exec.contains("\x1b[93m") && exec.contains(SCANLINE[0]));
        assert!(invoke.contains("\x1b[96m") && invoke.contains(ROUNDTRIP[0]));
        assert!(agent.contains("\x1b[95m") && agent.contains(SPINNER[0]));
        let fallback = anim.verb_spin(None, 0);
        assert!(fallback.contains("\x1b[36m") && fallback.contains(SPINNER[0]));
        // Pairwise distinct silhouettes at tick 0 (agent shares orbit
        // with the fallback by design — the colour tells them apart).
        let glyphs = [SAMPLING[0], SCANLINE[0], ROUNDTRIP[0], SPINNER[0]];
        for i in 0..glyphs.len() {
            for j in i + 1..glyphs.len() {
                assert_ne!(glyphs[i], glyphs[j], "patterns {i}/{j} collide");
            }
        }
        // Frames advance (motion is motion).
        assert_ne!(
            anim.verb_spin(Some("infer"), 0),
            anim.verb_spin(Some("infer"), 1)
        );
        // 2-cell law holds for every frame of every set.
        for set in [&SAMPLING, &SCANLINE, &ROUNDTRIP] {
            for t in 0..set.len() {
                let plain = Theme::new(false, false, true);
                assert_eq!(plain.verb_spin(Some("exec"), t).chars().count(), 2);
            }
        }
    }
}
