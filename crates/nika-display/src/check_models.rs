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
    /// Spec code when the refusal is a spec claim (`NIKA-PROVIDER` for a
    /// missing or unknown canonical prefix). `None` for engine-local
    /// refusals (cataloged vendor this binary cannot drive) and for
    /// catalog-warning rows.
    pub code: Option<String>,
}

impl ModelFinding {
    /// Construct a finding (INV-019 · `new()` on every `#[non_exhaustive]`
    /// struct).
    #[must_use]
    pub fn new(model: String, tasks: Vec<String>, why: String) -> Self {
        Self {
            model,
            tasks,
            why,
            code: None,
        }
    }

    /// Attach a spec code (consuming builder — `new()` stays frozen,
    /// INV-019).
    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
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
    /// Models that RESOLVE but match nothing the pricing snapshot
    /// carries for their (priced) provider — the two-strike class
    /// (audit UX 2026-07-31: buy the key, then meet the typo). A ⚠,
    /// never a ✖: the snapshot is dated, providers ship models weekly.
    pub catalog_warnings: Vec<ModelFinding>,
    /// Resolvable models on a server-backed KEYLESS engine (the local
    /// five) — the green line must nuance « resolves » for these: the
    /// rung never dialed the server, so it is never « reachable »
    /// (B-5's sibling · the gauntlet read the green line as a promise).
    pub local_server: usize,
}

impl ModelsAudit {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(findings: Vec<ModelFinding>, unjudged: usize, via_default: usize) -> Self {
        Self {
            findings,
            unjudged,
            via_default,
            catalog_warnings: Vec::new(),
            local_server: 0,
        }
    }

    /// Attach the catalog cross-check warnings (consuming builder — the
    /// `new()` signature stays frozen, INV-019).
    #[must_use]
    pub fn with_catalog_warnings(mut self, warnings: Vec<ModelFinding>) -> Self {
        self.catalog_warnings = warnings;
        self
    }

    /// Attach the server-backed-local count (consuming builder — the
    /// `new()` signature stays frozen, INV-019).
    #[must_use]
    pub fn with_local_server(mut self, count: usize) -> Self {
        self.local_server = count;
        self
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
            let detail = match f.code.as_deref() {
                Some(code) => format!("[{code}] {}", f.why),
                None => f.why.clone(),
            };
            let _ = writeln!(
                out,
                " {} {}   `{}` (task{} {}) — {detail}",
                mark(t, false),
                t.paint(Role::Strong, "MODELS"),
                f.model,
                if f.tasks.len() == 1 { "" } else { "s" },
                f.tasks.join(", "),
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
    // B-5's sibling (the gauntlet read the green line as a promise, then
    // met the dead server at run): « resolves » is never « reachable »
    // for a server this rung never dialed — say so, and name the one
    // probe that does.
    let liveness = if audit.local_server > 0 {
        " · local servers not probed (nika doctor --ping)"
    } else {
        ""
    };
    let line = if audit.unjudged > 0 {
        format!(
            "{judged} of {n} models resolve in this binary{via} · {} run-time · unjudged{liveness}",
            audit.unjudged
        )
    } else {
        let noun = if n == 1 {
            "model resolves"
        } else {
            "models resolve"
        };
        format!("{n} {noun} in this binary{via}{liveness}")
    };
    let _ = writeln!(
        out,
        " {} {}   {}",
        mark(t, true),
        t.paint(Role::Strong, "MODELS"),
        t.paint(Role::Dim, &line)
    );
    // The catalog cross-check rides UNDER the green line (the audit
    // stays clean — advisory): the model resolves, the snapshot has
    // never heard of it, and the user deserves to know BEFORE the key.
    for w in &audit.catalog_warnings {
        let _ = writeln!(
            out,
            " {} {}   {}",
            t.paint(Role::Warn, if t.ascii { "!" } else { "⚠" }),
            t.paint(Role::Strong, "MODELS"),
            w.why
        );
    }
}
