// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The doctor's passive report cells. Diagnosis, visibility, links and exit stay host-owned.

use crate::theme::{Role, Theme};
use std::fmt::Write as _;

/// Severity of one diagnosis line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// `✔` healthy.
    Ok,
    /// `⚠` advisory (the run may still work).
    Warn,
    /// `✖` a hard environment problem (drives `exit 3`).
    Fail,
}

impl Level {
    /// The semantic colour role — the SAME closed vocabulary the run
    /// storyboard speaks (`Role` · theme.rs): green ok · yellow advisory ·
    /// red hard-fail. Never decorative.
    const fn role(self) -> Role {
        match self {
            Self::Ok => Role::Good,
            Self::Warn => Role::Warn,
            Self::Fail => Role::Bad,
        }
    }

    fn glyph(self) -> char {
        match self {
            Self::Ok => '✔',
            Self::Warn => '⚠',
            Self::Fail => '✖',
        }
    }
}

/// One diagnosis line · a problem carries the exact PRINTED fix (never run).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Finding {
    pub level: Level,
    pub label: String,
    pub detail: String,
    pub fix: Option<String>,
}

/// One visible row, with only the linkable cells prepared by its host.
/// Preparation must preserve the printed text and may add admitted terminal links.
/// No callback, path lookup or visibility decision is deferred to rendering.
pub struct PreparedRow<'a> {
    pub finding: &'a Finding,
    pub detail: String,
    pub fix: Option<String>,
}

/// The fixed label column (nextest school: one grid, computed on RAW text).
/// Every label `diagnose` can emit fits STRICTLY inside it (pinned by
/// `every_label_fits_the_fixed_column`) so the detail column never shears.
pub const LABEL_COL: usize = 10;

/// Render the findings through the ONE colour seam (`Theme` · semantic
/// never decorative — the same law welcome/run obey). Doctor rows carry NO
/// durations, so the nextest discipline reduces to the status/label
/// columns — a fixed 1-cell status glyph + the fixed `LABEL_COL` label
/// cell, both laid out on RAW text and painted AFTER (ANSI escapes never
/// enter width arithmetic — the same law as `Theme::glyph`). The sober
/// register (colour off · links off · every pipe) is byte-identical to the
/// themeless render it replaces.
///
/// B-8b (the 2026-07-31 gauntlet): a healthy keyless machine printed 13+
/// ⚠ rows — every unwired agent, every unconfigured provider, the
/// config-less default — and the alarm glyph taught the user to ignore
/// it. `verbose: false` folds those three advisory classes into ONE calm
/// line (`--verbose` unfolds each); the verdict line keeps counting the
/// truth, the machine lane (`render_json`) always carries every finding.
/// The host already selected visible rows and prepared their link cells, preserving
/// its verbose/fold policy and filesystem observations. This function does no I/O.
#[must_use]
pub fn render(
    findings: &[Finding],
    rows: &[PreparedRow<'_>],
    advisory: (usize, usize, bool),
    theme: Theme,
) -> String {
    let mut s = String::new();
    let count = |level: Level| findings.iter().filter(|f| f.level == level).count();
    let (ok, warn, fail) = (count(Level::Ok), count(Level::Warn), count(Level::Fail));
    let verdict = if fail > 0 { Level::Fail } else { Level::Ok };
    let glyph = |level: Level| theme.paint(level.role(), &level.glyph().to_string());
    let _ = writeln!(s, "{} {ok} ok · {warn} warn · {fail} fail", glyph(verdict));
    for row in rows {
        let f = row.finding;
        let _ = writeln!(
            s,
            "{} {:<LABEL_COL$} {}",
            glyph(f.level),
            f.label,
            row.detail
        );
        if let Some(fix) = &row.fix {
            let _ = writeln!(s, "  fix: {fix}");
        }
    }
    let (agents, providers, config) = advisory;
    let mut classes = Vec::new();
    if agents > 0 {
        classes.push(format!("{agents} agents unwired"));
    }
    if providers > 0 {
        classes.push(format!("{providers} providers unconfigured"));
    }
    if config {
        classes.push("config defaults".to_owned());
    }
    if !classes.is_empty() {
        let _ = writeln!(
            s,
            "{} {:<LABEL_COL$} a healthy machine's notes — {} · nika doctor --verbose unfolds each",
            theme.paint(Role::Dim, "·"),
            theme.paint(Role::Dim, "advisory"),
            theme.paint(Role::Dim, &classes.join(" · "))
        );
    }
    s
}

/// Render findings as the `nika doctor` report (spec §8 layout · glyph · label
/// padded · detail · an indented `fix:` line under a problem) — opened by the
/// ONE verdict line (`✔ 6 ok · 4 warn · 0 fail`) so the state of the
/// environment reads before the sections do. Sections stay unchanged.
/// The machine lane (Q7): findings verbatim + a computed summary —
/// agents/CI branch on `summary.fail` instead of parsing glyphs. P0-21:
/// the adoption rung rides alongside (additive) — ONE state token the
/// flat findings could never express. H5: the per-host runtime receipts
/// ride alongside too (additive) — what each host earned, what was
/// verified versus assumed, and the repair, per host. R4: the access
/// census rides alongside (additive) — every path with its custody and
/// fix, the ready seats, the best path; one read, never recomputed.
/// The host owns projection, redaction and disclosure decisions for these values.
/// This presentation function is not a policy or privacy validator; it does no I/O.
#[must_use]
pub fn render_json(
    findings: &[Finding],
    adoption_state: &str,
    receipts: serde_json::Value,
    access: serde_json::Value,
) -> String {
    let count = |lvl: Level| findings.iter().filter(|f| f.level == lvl).count();
    let mut payload = serde_json::json!({
        "summary": {
            "ok": count(Level::Ok),
            "warn": count(Level::Warn),
            "fail": count(Level::Fail),
        },
        "adoption_state": adoption_state,
        "findings": findings,
    });
    payload["receipts"] = receipts;
    payload["access"] = access;
    format!("{payload:#}")
}

#[cfg(test)]
mod tests {
    use super::Level;

    #[test]
    fn level_glyphs_are_distinct() {
        assert_eq!(Level::Ok.glyph(), '✔');
        assert_eq!(Level::Warn.glyph(), '⚠');
        assert_eq!(Level::Fail.glyph(), '✖');
    }
}
