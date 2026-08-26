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
use nika_schema::raw::{RawAction, RawWorkflow};

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
/// The row for infer/agent tasks NO `model:` reaches (#1178).
///
/// This is the rung's finding before any resolver runs: there is no
/// string to resolve. It rendered nothing at all — `requirements.models`
/// is empty in that case, which is the same shape as « this workflow has
/// no inference tasks », and [`models`]'s early return reads it as
/// exactly that.
///
/// So `check` went green on a file whose run cannot start, and the run
/// died naming a PROVIDER error — the class `nika explain` teaches as an
/// HTTP failure. Nothing dialed. The author was sent to look for a
/// network problem that a missing line in their own file had caused. The
/// rung that owns « can this run reach a model » has to answer when the
/// answer is no.
///
/// ⚠, not ✖ — the same posture `inputs_required` earns, and for the same
/// reason: `nika run --model <provider>/<name>` supplies one, so the file
/// is INCOMPLETE, not refused. A ✖ would also have to move the verdict,
/// and a red row over a green `audited` card is the
/// three-surfaces-two-answers defect this render already carries a note
/// about (P0-11).
fn absent_model_tasks(wf: &RawWorkflow) -> Vec<&str> {
    let envelope_model = wf.model.as_ref();
    wf.tasks
        .iter()
        .filter_map(|task| {
            let task_model = match &task.value.action {
                RawAction::Infer(action) => action.model.as_ref(),
                RawAction::Agent(action) => action.model.as_ref(),
                RawAction::Exec(_) | RawAction::Invoke(_) => return None,
                #[allow(
                    clippy::unreachable,
                    reason = "non_exhaustive future variant — schema and renderer ship together; fail loud beats silently-wrong output"
                )]
                other => unreachable!("unknown action: {other:?}"),
            };
            (task_model.is_none() && envelope_model.is_none()).then_some(task.value.id.value.as_str())
        })
        .collect()
}

fn absent_model_row(out: &mut String, wf: &RawWorkflow, t: Theme) {
    let tasks = absent_model_tasks(wf);
    if tasks.is_empty() {
        return;
    }
    let named = tasks
        .iter()
        .map(|id| format!("`{id}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        " {} {}   no `model:` reaches {} {named} — declare one on the task, or as the \
         envelope default `model: <provider>/<name>`, or pass `--model` at run \
         (`--model mock/echo` previews offline)",
        t.paint(Role::Warn, if t.ascii { "!" } else { "⚠" }),
        t.paint(Role::Strong, "MODELS"),
        if tasks.len() == 1 { "task" } else { "tasks" },
    );
}

/// The refusal rows, out of `models` under the 100-line fn cap: one row
/// per finding, the error code prefixed when the refusal carries one.
fn findings_rows(out: &mut String, findings: &[ModelFinding], t: Theme) {
    for f in findings {
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
}

pub(crate) fn models(
    out: &mut String,
    report: &CheckReport,
    wf: &RawWorkflow,
    audit: &ModelsAudit,
    t: Theme,
) {
    absent_model_row(out, wf, t);
    if report.requirements.models.is_empty() {
        return; // no inference tasks — the ladder says so at COST already
    }
    if !audit.findings.is_empty() {
        findings_rows(out, &audit.findings, t);
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
    // P02's narrowing (the same gauntlet shape, one layer down): the
    // green line read as « this run will reach the model », then the run
    // refused for a missing key. This rung judges RESOLUTION in the
    // binary — never key presence on this machine, which stays the
    // advisory surface's truth (`access_plan`), with the refusal
    // authority at the run gate. A ✔ that claims exactly what it
    // covered, and names what defers.
    let key_presence = " · key presence on this machine not judged (advisory: check --json access_plan · \
         the run gate refuses NIKA-INFER-001)";
    let line = if audit.unjudged > 0 {
        format!(
            "{judged} of {n} models resolve in this binary{via} · {} run-time · unjudged{liveness}{key_presence}",
            audit.unjudged
        )
    } else {
        let noun = if n == 1 {
            "model resolves"
        } else {
            "models resolve"
        };
        format!("{n} {noun} in this binary{via}{liveness}{key_presence}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn workflow(yaml: &str) -> RawWorkflow {
        parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses")
    }

    /// The claim-narrowing pin (the gauntlet read `✔ MODELS 1 model
    /// resolves in this binary` as « the run will reach it », then the
    /// run refused for a missing key): the green line names what it did
    /// NOT judge — key presence on this machine — and where that truth
    /// lives (the advisory `access_plan` · the run gate's NIKA-INFER-001).
    /// Dropping the clause turns this red; the clause is unconditional
    /// because the rung never judges key presence, on any workflow.
    #[test]
    fn the_green_line_names_the_key_presence_deferral() {
        let mut out = String::new();
        let full = workflow(
            "nika: m\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n",
        );
        let report = nika_check::check(&full);
        models(
            &mut out,
            &report,
            &full,
            &ModelsAudit::new(Vec::new(), 0, 0),
            Theme::new(false, true, false),
        );
        assert!(out.contains("1 model resolves in this binary"), "{out}");
        for clause in [
            "key presence on this machine not judged",
            "check --json access_plan",
            "NIKA-INFER-001",
        ] {
            assert!(out.contains(clause), "the deferral names `{clause}`: {out}");
        }

        // The mixed list (one judged, one run-time) carries the same
        // narrowing — the deferral is a property of the RUNG, not of the
        // all-static happy path.
        let mixed = workflow(
            "nika: m\ninputs:\n  seat: { type: string, required: true }\ntasks:\n  a:\n    \
             infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n  b:\n    \
             infer: { prompt: hi, max_tokens: 10, model: \"${{ inputs.seat }}\" }\n",
        );
        let report = nika_check::check(&mixed);
        let mut out = String::new();
        models(
            &mut out,
            &report,
            &mixed,
            &ModelsAudit::new(Vec::new(), 1, 0),
            Theme::new(false, true, false),
        );
        assert!(
            out.contains("1 of 2 models resolve in this binary"),
            "{out}"
        );
        assert!(
            out.contains("key presence on this machine not judged"),
            "the mixed line narrows too: {out}"
        );
    }

    #[test]
    fn absent_model_tasks_are_named_only_when_no_model_reaches_them() {
        let bare = workflow("nika: bare\ntasks:\n  bot:\n    agent: { prompt: \"say hi\" }\n");
        assert_eq!(absent_model_tasks(&bare), vec!["bot"]);

        let invoke = workflow("nika: invoke\ntasks:\n  call:\n    invoke: { tool: nika:uuid }\n");
        assert!(absent_model_tasks(&invoke).is_empty());

        let covered = workflow(
            "nika: covered\nmodel: mock/echo\ntasks:\n  bot:\n    agent: { prompt: \"say hi\" }\n",
        );
        assert!(absent_model_tasks(&covered).is_empty());

        let overridden = workflow(
            "nika: overridden\ntasks:\n  bot:\n    agent: { prompt: \"hi\", model: mock/echo }\n",
        );
        assert!(absent_model_tasks(&overridden).is_empty());

        let mut rendered = String::new();
        absent_model_row(&mut rendered, &bare, Theme::new(false, false, false));
        assert!(rendered.contains("no `model:` reaches task `bot`"));
        assert!(rendered.contains("declare one on the task"));
        assert!(rendered.contains("envelope default"));
        assert!(rendered.contains("pass `--model` at run"));
    }
}
