// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The MODELS rung of the `nika check` render (#320) — the finding
//! types and their row builder, split from `check_render.rs` at the
//! 1,500-line file wall (the same wall that moved the render itself
//! out of nika-cli · 2026-07-29). The public path stays
//! `check_render::{ModelFinding, ModelsAudit}` through the re-export:
//! the types live beside their renderer either way.

use std::fmt::Write as _;

use nika_check::CheckReport;

use crate::check_render::mark;
use crate::theme::{Role, Theme};

/// One MODELS-rung finding — a `model:` the binary cannot run (#320).
/// The gather (resolver-side) stays with the caller; the render takes
/// the rows (the `PricingPin` precedent: the judge is pure, the identity
/// is injected).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModelFinding {
    /// The unresolvable model string.
    pub model: String,
    /// The tasks carrying it.
    pub tasks: Vec<String>,
    /// The resolver's own refusal reason.
    pub why: String,
}

impl ModelFinding {
    /// Construct a finding (INV-019 · `new()` on every `#[non_exhaustive]`
    /// struct).
    #[must_use]
    pub fn new(model: String, tasks: Vec<String>, why: String) -> Self {
        Self { model, tasks, why }
    }
}

/// The MODELS-rung audit: the findings PLUS the count of entries the
/// rung made no claim about — so the green line can never cover a model
/// it never judged (the old headline counted a skipped templated model
/// as « resolves in this binary »). Lives beside its renderer like
/// [`ModelFinding`]: the gather stays with the caller, the render takes
/// the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModelsAudit {
    /// Models this binary cannot run (literal, or via declared default).
    pub findings: Vec<ModelFinding>,
    /// Templated `model:` entries with NO static declared default — the
    /// value arrives at run time (`--var`); no claim was made.
    pub unjudged: usize,
    /// Models judged THROUGH a declared default (`${{ const.model }}`
    /// whose declaration carries a literal string) that resolve — named
    /// on the green line because a `--var` can swap the value at run.
    pub via_default: usize,
}

impl ModelsAudit {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(findings: Vec<ModelFinding>, unjudged: usize, via_default: usize) -> Self {
        Self {
            findings,
            unjudged,
            via_default,
        }
    }
}

/// The MODELS rung (#320): every `model:` must resolve in THIS binary —
/// green means runnable, never merely cataloged. Renders between PLAN
/// and COST (resolvability before price).
///
/// The green line counts what was JUDGED, never what was skipped. The
/// old headline said « N models resolve in this binary » over the whole
/// requirements list — measured 2026-07-30: a workflow whose only model
/// is `${{ inputs.model }}` (required · no default) printed
/// `✔ 1 model resolves in this binary` while the rung had skipped it
/// wholesale. Nothing resolved; nobody looked. An all-unjudged list now
/// renders `○` (no claim), and a mixed list names its unjudged rest.
pub(crate) fn models(out: &mut String, report: &CheckReport, audit: &ModelsAudit, t: Theme) {
    if report.requirements.models.is_empty() {
        return; // no inference tasks — the ladder says so at COST already
    }
    if !audit.findings.is_empty() {
        for f in &audit.findings {
            let _ = writeln!(
                out,
                " {} {}   `{}` (task{} {}) — {}",
                mark(t, false),
                t.paint(Role::Strong, "MODELS"),
                f.model,
                if f.tasks.len() == 1 { "" } else { "s" },
                f.tasks.join(", "),
                f.why
            );
        }
        return;
    }
    let n = report.requirements.models.len();
    let judged = n - audit.unjudged;
    if judged == 0 {
        // Every model is a run-time value — this rung made NO claim,
        // and a ✔ would be one (the same ○ posture as an undeclared
        // PERMITS block).
        let _ = writeln!(
            out,
            " {} {}   {}",
            t.paint(Role::Dim, "○"),
            t.paint(Role::Strong, "MODELS"),
            t.paint(
                Role::Dim,
                &format!(
                    "{} — value arrives at run (--var) · unjudged",
                    crate::vocab::count(n, "run-time model")
                )
            )
        );
        return;
    }
    let via = if audit.via_default > 0 {
        format!(" ({} via declared default)", audit.via_default)
    } else {
        String::new()
    };
    let line = if audit.unjudged > 0 {
        format!(
            "{judged} of {n} models resolve in this binary{via} · {} run-time · unjudged",
            audit.unjudged
        )
    } else {
        let noun = if n == 1 {
            "model resolves"
        } else {
            "models resolve"
        };
        format!("{n} {noun} in this binary{via}")
    };
    let _ = writeln!(
        out,
        " {} {}   {}",
        mark(t, true),
        t.paint(Role::Strong, "MODELS"),
        t.paint(Role::Dim, &line)
    );
}
