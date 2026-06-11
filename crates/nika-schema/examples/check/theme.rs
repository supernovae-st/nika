// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The colour seam — ONE per binary (CLI presentation canon).
//!
//! Semantic, never decorative: green=ok · red=err · yellow=warn ·
//! cyan=THE accent · dim=secondary · bold=strong. Resolution order per
//! the nika-cli display contract: `--color=never|always` → `NO_COLOR` →
//! `CLICOLOR_FORCE` → TTY. Glyphs are part of the TEXT (meaning never
//! lives in colour alone — they survive colour loss).
//!
//! Zero raw ANSI outside this file.

use std::io::IsTerminal;

// ── The glyph grammar (nika-cli display contract §3.1) ──────────────
pub(crate) const G_OK: &str = "✔";
pub(crate) const G_ERR: &str = "✖";
pub(crate) const G_WARN: &str = "⚠";
pub(crate) const G_GATED: &str = "⊘";
pub(crate) const G_PENDING: &str = "○";
pub(crate) const G_HINT: &str = "➜";
pub(crate) const G_DEP: &str = "←";
pub(crate) const G_FIX: &str = "↳";
pub(crate) const G_BANNER: &str = "◆";
pub(crate) const G_RETRY: &str = "↻";

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
/// reset, and cannot pick a non-semantic colour.
#[derive(Clone, Copy)]
pub(crate) struct Theme {
    on: bool,
}

impl Theme {
    /// Resolve from the environment + the `--color` flag.
    ///
    /// `NO_COLOR`/`CLICOLOR_FORCE` are the cross-tool TERMINAL contract
    /// (no-color.org), not workflow configuration — the only legitimate
    /// direct env reads in a CLI surface (same exemption class as the
    /// print macros above; workflow env goes through the kernel caps).
    #[allow(clippy::disallowed_methods)]
    pub(crate) fn from_env(flag: ColorFlag) -> Self {
        Self {
            on: resolve_colour(
                flag,
                std::env::var_os("NO_COLOR").is_some(),
                std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0"),
                std::io::stdout().is_terminal(),
            ),
        }
    }

    fn paint(self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_owned()
        }
    }

    /// Green — a section/verdict that holds.
    pub(crate) fn ok(self, s: &str) -> String {
        self.paint("32", s)
    }
    /// Red — a finding that fails the check.
    pub(crate) fn err(self, s: &str) -> String {
        self.paint("31", s)
    }
    /// Yellow — a warning (unbounded cost · secret leak surface).
    pub(crate) fn warn(self, s: &str) -> String {
        self.paint("33", s)
    }
    /// Cyan — THE one accent (banner · machine-applicable fixes).
    pub(crate) fn accent(self, s: &str) -> String {
        self.paint("36", s)
    }
    /// Dim — secondary detail (deps · models · annotations).
    pub(crate) fn dim(self, s: &str) -> String {
        self.paint("2", s)
    }
    /// Bold — the verdict line.
    pub(crate) fn bold(self, s: &str) -> String {
        self.paint("1", s)
    }
    /// Bold green / bold red verdict composites.
    pub(crate) fn verdict_ok(self, s: &str) -> String {
        self.paint("1;32", s)
    }
    pub(crate) fn verdict_err(self, s: &str) -> String {
        self.paint("1;31", s)
    }
}
