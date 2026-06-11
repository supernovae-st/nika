// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The colour seam — ONE per binary, ONE source of truth for the theme.
//!
//! This module is the **canonical semantic taxonomy** every coloured byte
//! of the `nika` CLI routes through (CLI presentation canon · the
//! nika-cli display contract §3.4 · clig.dev · the Clack log-level model).
//! It is the reference seam `nika-cli` (L4) and `nika-vscode` derive from;
//! a brand/theme change touches ONLY [`Role::sgr`].
//!
//! ## Two orthogonal semantic axes (never decoration)
//!
//! - **STATUS** — how a thing turned out / its emphasis. The canon-locked
//!   roles: green=ok · red=err · yellow=warn · cyan=THE-accent · dim=muted
//!   · bold=strong.
//! - **VERB** — what KIND of work a task does, encoded so the DAG is
//!   readable at a glance AND ties to the report sections that scrutinize
//!   it. The colour **family is the governing gate** ·
//!   - **magenta family = COST-bearing** (`infer`/`agent` spend tokens —
//!     the COST section governs them)
//!   - **blue family = PERMITS-bearing** (`exec`/`invoke` touch the world —
//!     the PERMITS section governs them)
//!   - **brightness = blast radius** within a family (`agent` > `infer`,
//!     `exec` > `invoke`).
//!
//! Meaning never lives in colour ALONE — every state also carries a glyph
//! and a word, so the output survives colour loss (a11y · `NO_COLOR` · pipes).
//!
//! Determinism: [`Role::sgr`] is a pure total function pinned by a test,
//! so the canonical palette cannot drift under a refactor. Zero raw ANSI
//! outside this file.

use std::io::IsTerminal;

/// The glyph grammar (nika-cli display contract §3.1) — a glyph is part
/// of the TEXT, carrying the state when colour is off. Each has BOTH a
/// unicode and an ASCII rendering: per the contract, ASCII is a
/// **first-class theme** (CI logs · `TERM=dumb` · screen readers), not a
/// degraded mode. The active set is chosen once on [`Theme`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Glyph {
    Ok,
    Err,
    Warn,
    Gated,
    Pending,
    Hint,
    Dep,
    Fix,
    Banner,
    Retry,
}

impl Glyph {
    /// The unicode rendering (aligned to the contract's unicode column).
    const fn unicode(self) -> &'static str {
        match self {
            Self::Ok => "✔",
            Self::Err => "✖",
            Self::Warn => "⚠",
            Self::Gated => "⊘",
            Self::Pending => "○",
            Self::Hint => "➜",
            Self::Dep => "←",
            Self::Fix => "↳",
            Self::Banner => "◆",
            Self::Retry => "↻",
        }
    }

    /// The ASCII rendering (the first-class fallback theme).
    const fn ascii(self) -> &'static str {
        match self {
            Self::Ok => "+",
            Self::Err => "x",
            Self::Warn => "!",
            Self::Gated => "-",
            Self::Pending => ".",
            Self::Hint => ">",
            Self::Dep => "<-",
            Self::Fix => "->",
            Self::Banner => "#",
            Self::Retry => "r",
        }
    }
}

/// The canonical semantic role — the SINGLE SOURCE OF TRUTH for colour.
///
/// A role is a MEANING; [`Role::sgr`] is the ONE table mapping it to an
/// ANSI SGR parameter. Adding a colour = adding a documented role here,
/// never a raw code at a call site.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Role {
    // ── status axis (canon-locked) ──────────────────────────────────
    /// Green — a section/verdict that holds.
    Ok,
    /// Yellow — caution (unbounded cost · retries · warning surface).
    Warn,
    /// Red — a finding that fails the check.
    Err,
    /// Cyan — THE single accent (banner · the actionable machine fix).
    Accent,
    /// Dim — secondary detail (deps · models · annotations).
    Muted,
    /// Bold — structural emphasis (section labels · totals).
    Strong,
    /// Bold green — the clean verdict line.
    VerdictOk,
    /// Bold red — the findings verdict line.
    VerdictErr,
    // ── verb axis · family = governing gate · brightness = blast ─────
    /// Magenta — `infer` spends tokens (COST-governed).
    Infer,
    /// Bold magenta — `agent` is an autonomous token-spend loop.
    Agent,
    /// Blue — `invoke` calls a tool (PERMITS.tools-governed).
    Invoke,
    /// Bold blue — `exec` runs a host process (PERMITS.exec-governed).
    Exec,
}

impl Role {
    /// The ONE table: role → SGR parameter string. Pure, total, pinned by
    /// `sgr_table_is_canonical` — the palette cannot drift silently.
    pub(crate) const fn sgr(self) -> &'static str {
        match self {
            Self::Ok => "32",
            Self::Warn => "33",
            Self::Err => "31",
            Self::Accent => "36",
            Self::Muted => "2",
            Self::Strong => "1",
            Self::VerdictOk => "1;32",
            Self::VerdictErr => "1;31",
            // magenta family = cost · blue family = permits · bold = blast
            Self::Infer => "35",
            Self::Agent => "1;35",
            Self::Invoke => "34",
            Self::Exec => "1;34",
        }
    }
}

/// A workflow verb — the presentation-side mirror of the four execution
/// models (kept schema-independent so the theme has zero schema deps).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum VerbKind {
    Infer,
    Exec,
    Invoke,
    Agent,
}

impl VerbKind {
    /// The verb's display name (the column text).
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Infer => "infer",
            Self::Exec => "exec",
            Self::Invoke => "invoke",
            Self::Agent => "agent",
        }
    }

    /// The verb's canonical colour role (the governing-gate logic).
    pub(crate) const fn role(self) -> Role {
        match self {
            Self::Infer => Role::Infer,
            Self::Exec => Role::Exec,
            Self::Invoke => Role::Invoke,
            Self::Agent => Role::Agent,
        }
    }
}

/// `--color` flag values (contract resolution order, highest priority).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorFlag {
    Auto,
    Always,
    Never,
}

/// The pure resolution core — unit-testable, no environment reads.
/// Precedence: explicit flag → `NO_COLOR` → `CLICOLOR_FORCE` → TTY.
pub(crate) fn resolve_colour(flag: ColorFlag, no_color: bool, force: bool, tty: bool) -> bool {
    match flag {
        ColorFlag::Always => true,
        ColorFlag::Never => false,
        ColorFlag::Auto => {
            if no_color {
                false
            } else if force {
                true
            } else {
                tty
            }
        }
    }
}

/// Auto-resetting semantic colour API — a call site cannot forget the
/// reset, and cannot pick a non-semantic colour (it picks a [`Role`]).
/// Also the glyph-theme selector (unicode vs the first-class ASCII set).
#[derive(Clone, Copy)]
pub(crate) struct Theme {
    on: bool,
    unicode: bool,
}

impl Theme {
    /// Resolve colour + glyph theme from the environment + flags.
    ///
    /// `NO_COLOR`/`CLICOLOR_FORCE`/`TERM` are the cross-tool TERMINAL
    /// contract (no-color.org), not workflow configuration — the only
    /// legitimate direct env reads in a CLI surface (same exemption class
    /// as the print macros; workflow env goes through the kernel caps).
    /// ASCII is forced by `--ascii` OR a `TERM=dumb`/unset terminal.
    #[allow(clippy::disallowed_methods)]
    pub(crate) fn from_env(flag: ColorFlag, ascii_flag: bool) -> Self {
        let term_dumb = std::env::var_os("TERM").is_none_or(|t| t == "dumb");
        Self {
            on: resolve_colour(
                flag,
                std::env::var_os("NO_COLOR").is_some(),
                std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0"),
                std::io::stdout().is_terminal(),
            ),
            unicode: !ascii_flag && !term_dumb,
        }
    }

    /// Construct explicitly (test-only — the binary always resolves via
    /// [`Theme::from_env`]).
    #[cfg(test)]
    pub(crate) const fn new(on: bool, unicode: bool) -> Self {
        Self { on, unicode }
    }

    /// The active rendering of `g` (unicode or the first-class ASCII set).
    pub(crate) const fn glyph(self, g: Glyph) -> &'static str {
        if self.unicode { g.unicode() } else { g.ascii() }
    }

    /// Whether the unicode glyph theme is active (for non-[`Glyph`]
    /// typography like the horizontal rule).
    pub(crate) const fn unicode_glyphs(self) -> bool {
        self.unicode
    }

    /// THE painting primitive — wrap `s` in `role`'s SGR + an auto-reset.
    /// Never nest a painted string inside another `paint` (the inner reset
    /// would cancel the outer role mid-string).
    pub(crate) fn paint(self, role: Role, s: &str) -> String {
        if self.on {
            format!("\x1b[{}m{s}\x1b[0m", role.sgr())
        } else {
            s.to_owned()
        }
    }

    // ── semantic sugar over `paint` (the call-site vocabulary) ───────
    pub(crate) fn ok(self, s: &str) -> String {
        self.paint(Role::Ok, s)
    }
    pub(crate) fn err(self, s: &str) -> String {
        self.paint(Role::Err, s)
    }
    pub(crate) fn warn(self, s: &str) -> String {
        self.paint(Role::Warn, s)
    }
    pub(crate) fn accent(self, s: &str) -> String {
        self.paint(Role::Accent, s)
    }
    pub(crate) fn dim(self, s: &str) -> String {
        self.paint(Role::Muted, s)
    }
    pub(crate) fn bold(self, s: &str) -> String {
        self.paint(Role::Strong, s)
    }
    pub(crate) fn verdict_ok(self, s: &str) -> String {
        self.paint(Role::VerdictOk, s)
    }
    pub(crate) fn verdict_err(self, s: &str) -> String {
        self.paint(Role::VerdictErr, s)
    }

    /// Paint a verb name in its governing-gate colour.
    pub(crate) fn verb(self, kind: VerbKind, s: &str) -> String {
        self.paint(kind.role(), s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_table_is_canonical() {
        // Pin EVERY role → SGR. A refactor that changes the palette must
        // change this test deliberately (the determinism ratchet).
        assert_eq!(Role::Ok.sgr(), "32");
        assert_eq!(Role::Warn.sgr(), "33");
        assert_eq!(Role::Err.sgr(), "31");
        assert_eq!(Role::Accent.sgr(), "36");
        assert_eq!(Role::Muted.sgr(), "2");
        assert_eq!(Role::Strong.sgr(), "1");
        assert_eq!(Role::VerdictOk.sgr(), "1;32");
        assert_eq!(Role::VerdictErr.sgr(), "1;31");
        assert_eq!(Role::Infer.sgr(), "35");
        assert_eq!(Role::Agent.sgr(), "1;35");
        assert_eq!(Role::Invoke.sgr(), "34");
        assert_eq!(Role::Exec.sgr(), "1;34");
    }

    #[test]
    fn verb_colour_families_encode_the_governing_gate() {
        // magenta family (35) = COST-bearing verbs.
        assert!(VerbKind::Infer.role().sgr().ends_with("35"));
        assert!(VerbKind::Agent.role().sgr().ends_with("35"));
        // blue family (34) = PERMITS-bearing verbs.
        assert!(VerbKind::Invoke.role().sgr().ends_with("34"));
        assert!(VerbKind::Exec.role().sgr().ends_with("34"));
        // the higher-blast variant is bold within its family.
        assert!(VerbKind::Agent.role().sgr().starts_with("1;"));
        assert!(VerbKind::Exec.role().sgr().starts_with("1;"));
        assert!(!VerbKind::Infer.role().sgr().starts_with("1;"));
        assert!(!VerbKind::Invoke.role().sgr().starts_with("1;"));
    }

    #[test]
    fn verb_names_are_stable() {
        assert_eq!(VerbKind::Infer.name(), "infer");
        assert_eq!(VerbKind::Exec.name(), "exec");
        assert_eq!(VerbKind::Invoke.name(), "invoke");
        assert_eq!(VerbKind::Agent.name(), "agent");
    }

    #[test]
    fn paint_off_is_identity_on_wraps_and_resets() {
        assert_eq!(Theme::new(false, true).paint(Role::Ok, "hi"), "hi");
        assert_eq!(
            Theme::new(true, true).paint(Role::Ok, "hi"),
            "\x1b[32mhi\x1b[0m"
        );
        // every painted span self-closes (no role bleeds past its string)
        assert!(Theme::new(true, true).err("x").ends_with("\x1b[0m"));
    }

    #[test]
    fn glyph_theme_switches_and_both_sets_are_pinned() {
        let uni = Theme::new(false, true);
        let asc = Theme::new(false, false);
        // unicode set
        assert_eq!(uni.glyph(Glyph::Ok), "✔");
        assert_eq!(uni.glyph(Glyph::Gated), "⊘");
        assert_eq!(uni.glyph(Glyph::Retry), "↻");
        // ASCII first-class fallback — pinned, every glyph distinct enough
        assert_eq!(asc.glyph(Glyph::Ok), "+");
        assert_eq!(asc.glyph(Glyph::Err), "x");
        assert_eq!(asc.glyph(Glyph::Gated), "-");
        assert_eq!(asc.glyph(Glyph::Retry), "r");
        assert_eq!(asc.glyph(Glyph::Dep), "<-");
        // ASCII is pure ASCII (the whole point: dumb terminals)
        for g in [
            Glyph::Ok,
            Glyph::Err,
            Glyph::Warn,
            Glyph::Banner,
            Glyph::Fix,
        ] {
            assert!(asc.glyph(g).is_ascii(), "{g:?} ascii must be ASCII");
        }
    }

    #[test]
    fn colour_resolution_precedence() {
        // flag wins over everything
        assert!(resolve_colour(ColorFlag::Always, true, false, false));
        assert!(!resolve_colour(ColorFlag::Never, false, true, true));
        // auto: NO_COLOR beats CLICOLOR_FORCE beats TTY
        assert!(!resolve_colour(ColorFlag::Auto, true, true, true));
        assert!(resolve_colour(ColorFlag::Auto, false, true, false));
        assert!(resolve_colour(ColorFlag::Auto, false, false, true));
        assert!(!resolve_colour(ColorFlag::Auto, false, false, false));
    }
}
