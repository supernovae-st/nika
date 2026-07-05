// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE formatting + colour-capability vocabulary (the round-8 seam).
//!
//! ## The cost rule (one formatter · every surface)
//!
//! Money speaks through [`fmt_cost_usd`] everywhere a run's spend renders
//! (the footer meter · the verdict card · per-task cells · the waterfall).
//! The rule: dollars-and-cents baseline (`$1.50` · `$0.01`); a SUB-CENT
//! cost renders at the 6-place cap with trailing zeros stripped
//! (`$0.0003` · `$0.000284` — the honest precision, never the
//! first-rounding lie that would call 0.000284 "`$0.0003`"); zero and
//! sub-floor dust render the flat `$0.00`. Before this seam the meter
//! said `$0.000` while the card said `$0.0003` on the SAME run.
//!
//! ## The colour split (locked)
//!
//! CHROME — borders · labels · state glyphs · verdicts — speaks the
//! ANSI-16 slots via [`crate::display::theme::Role`]: the USER's terminal
//! theme decides the actual hues, we only name semantics. TRUECOLOR is
//! reserved for DATA (the duration-heat ramp) and fires only under an
//! explicit `COLORTERM` capability signal — never for decoration.
//!
//! ## The colour-resolution order (the fd/cargo convention)
//!
//! `--color always|never|auto` > `CLICOLOR_FORCE` (set · non-empty ·
//! not `0`) > `NO_COLOR` (any non-empty value) > `CLICOLOR=0` > the
//! auto floor: stdout is a TTY and `TERM != dumb`. [`color_enabled`] is
//! the pure decision; the binary collects the environment facts (this
//! module does no I/O).

/// The `--color <WHEN>` tri-state (fd · ls · cargo convention).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    /// Force colour on (pagers · `watch` · captured demos).
    Always,
    /// Force colour off (`--no-color` folds here).
    Never,
    /// Resolve from the environment chain + TTY (the default).
    Auto,
}

/// The environment facts the `auto` arm reads — collected once by the
/// binary (the display layer stays I/O-free · pure · testable).
// Four INDEPENDENT environment facts, not a state machine — the same
// flags-are-flags exemption the clap arg structs carry (main.rs).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ColorEnv {
    /// `CLICOLOR_FORCE` set · non-empty · not `"0"`.
    pub force: bool,
    /// `NO_COLOR` set to any non-empty value (<https://no-color.org>).
    pub no_color: bool,
    /// `CLICOLOR` set to exactly `"0"`.
    pub clicolor_zero: bool,
    /// `TERM` is exactly `dumb`.
    pub term_dumb: bool,
}

/// Resolve the colour decision (the locked priority order · module doc).
#[must_use]
pub fn color_enabled(choice: ColorChoice, env: ColorEnv, tty: bool) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            if env.force {
                return true;
            }
            if env.no_color || env.clicolor_zero {
                return false;
            }
            tty && !env.term_dumb
        }
    }
}

/// Widest a sub-cent cost may grow (decimal places) before it honestly
/// rounds to the flat zero form.
const COST_MAX_PLACES: usize = 6;

/// Format a run spend in dollars (the ONE cost formatter — module doc).
///
/// Two regimes: an amount that survives 2-place rounding is plain money
/// (`$1.50` · `$0.01` — the dollars-and-cents baseline); a sub-cent
/// amount renders at the [`COST_MAX_PLACES`] cap with trailing zeros
/// stripped (`$0.002` · `$0.000284` — every digit shown is real).
/// `$0.00` for zero and for dust below the 6-place floor — a cost the
/// engine cannot render honestly is a cost of zero cents, not a lie of
/// trailing zeros. Non-finite input (a corrupt trace float) renders the
/// zero form too — never a panic, never `$NaN`.
#[must_use]
pub fn fmt_cost_usd(usd: f64) -> String {
    if !usd.is_finite() {
        return "$0.00".to_owned();
    }
    let cents = format!("{usd:.2}");
    if has_visible_amount(&cents) {
        return format!("${cents}");
    }
    let full = format!("{usd:.COST_MAX_PLACES$}");
    if !has_visible_amount(&full) {
        return "$0.00".to_owned();
    }
    format!("${}", full.trim_end_matches('0'))
}

/// Does a formatted decimal show any non-zero digit? (`0.00` no · `0.01`
/// yes — the "visibly non-zero" test both regimes share.)
fn has_visible_amount(text: &str) -> bool {
    text.bytes().any(|b| b.is_ascii_digit() && b != b'0')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cost rule, pinned end-to-end: baseline cents · the sub-cent
    /// 6-place cap with trailing zeros stripped · the flat zero.
    #[test]
    fn cost_formatter_extends_places_until_non_zero() {
        assert_eq!(fmt_cost_usd(0.0), "$0.00");
        assert_eq!(fmt_cost_usd(1.5), "$1.50");
        assert_eq!(fmt_cost_usd(0.04), "$0.04");
        assert_eq!(fmt_cost_usd(0.01), "$0.01");
        // Super-cent: the dollars-and-cents baseline.
        assert_eq!(fmt_cost_usd(0.011), "$0.01");
        // Sub-cent: honest places up to the cap (the field bug: the meter
        // said $0.000 while the card said $0.0003 on the SAME run).
        assert_eq!(fmt_cost_usd(0.002), "$0.002");
        assert_eq!(fmt_cost_usd(0.0003), "$0.0003");
        assert_eq!(fmt_cost_usd(0.000_284_25), "$0.000284", "cap-6 · no lie");
        assert_eq!(fmt_cost_usd(0.000_01), "$0.00001");
        assert_eq!(fmt_cost_usd(0.000_004), "$0.000004");
        // Below the 6-place floor: the honest flat zero, never 0.000000.
        assert_eq!(fmt_cost_usd(0.000_000_4), "$0.00");
    }

    /// The regime boundary is 2-place rounding: `0.005` survives it
    /// (`$0.01` · baseline money) while `0.0049` does not and renders its
    /// REAL sub-cent places (`$0.0049` — never the rounded `$0.005`).
    #[test]
    fn cost_formatter_rejects_zero_looking_roundings() {
        assert_eq!(fmt_cost_usd(0.0049), "$0.0049");
        assert_eq!(fmt_cost_usd(0.005), "$0.01", "rounds up at 2 places");
        assert_eq!(fmt_cost_usd(0.9999), "$1.00");
    }

    /// Corrupt input never panics and never prints `$NaN`.
    #[test]
    fn cost_formatter_survives_non_finite() {
        assert_eq!(fmt_cost_usd(f64::NAN), "$0.00");
        assert_eq!(fmt_cost_usd(f64::INFINITY), "$0.00");
    }

    /// The full priority chain (module doc order) — each rung beats
    /// everything below it.
    #[test]
    fn color_resolution_priority_chain() {
        let plain = ColorEnv::default();
        // The explicit flag beats every environment fact.
        let hostile = ColorEnv {
            force: false,
            no_color: true,
            clicolor_zero: true,
            term_dumb: true,
        };
        assert!(color_enabled(ColorChoice::Always, hostile, false));
        let forcing = ColorEnv {
            force: true,
            ..hostile
        };
        assert!(!color_enabled(ColorChoice::Never, forcing, true));

        // CLICOLOR_FORCE beats NO_COLOR, CLICOLOR=0 and the pipe floor.
        assert!(color_enabled(ColorChoice::Auto, forcing, false));

        // NO_COLOR (non-empty) kills colour even on a TTY.
        let no_color = ColorEnv {
            no_color: true,
            ..plain
        };
        assert!(!color_enabled(ColorChoice::Auto, no_color, true));

        // CLICOLOR=0 likewise.
        let clicolor_zero = ColorEnv {
            clicolor_zero: true,
            ..plain
        };
        assert!(!color_enabled(ColorChoice::Auto, clicolor_zero, true));

        // The auto floor: TTY and TERM≠dumb.
        assert!(color_enabled(ColorChoice::Auto, plain, true));
        assert!(!color_enabled(ColorChoice::Auto, plain, false));
        let dumb = ColorEnv {
            term_dumb: true,
            ..plain
        };
        assert!(!color_enabled(ColorChoice::Auto, dumb, true));
    }
}
