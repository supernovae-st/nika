// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika welcome` — the mirror moment (30-seconds surface · first contact).
//!
//! One command, one screen: what Nika IS (the tagline block), what THIS
//! machine already has (the shared [`crate::probe`] engine — the same
//! detection `doctor` diagnoses with · one truth, two voices), what this
//! BINARY carries (counts DERIVED live from the embedded pack/catalog,
//! never hardcoded — the born-stale law), and the three commands to run
//! next (offline first · zero keys).
//!
//! Always offline (no `--ping` here — that stays doctor's opt-in), always
//! exit `0` (a greeting is never a failure — even a bare machine gets
//! routed, not scolded), and PRESENCE-only like everything probe-backed:
//! no secret value exists in this module by construction. The ONE breach
//! of presence-only: with exactly ONE workflow on disk the concierge
//! audits THAT file in-process (parse + check ladder · bounded, never a
//! walk) before any `run` line may be printed (P0-3 · LOI-3). Re-runnable
//! anytime: welcome is a living mirror, not a splash screen.
//!
//! P0-14 rides the binary too (W2): the session context is resolved ONCE
//! through `context_envelope::resolve` at the top of `run` — the surface
//! hands the cwd it can name (never a process-cwd fallback), and a
//! chat-only envelope renders « chat only », never a workspace pretence.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nika_providers::probe::ExecutionLocus;

use crate::context_envelope::{self, ContextEnvelope, ContextMode, EnvFacts, EvidenceSource};
use crate::display::theme::{Role, Theme};
use crate::door::DoorId;
use crate::probe::Probe;
use crate::{output::VerbOutput, probe};

/// The next moves, keyed on where this workspace actually IS — the
/// concierge hands over ONE key, not the keyring (row 0 carries the
/// weight; the others stay dim context). Presence-only inputs — EXCEPT
/// the 1-workflow case, where `gate` carries the exact file's audit
/// verdict (P0-3): a `run` line is only ever printed for a file the
/// ladder just saw clean, and a priced model always carries LOI-3's
/// cap. The multi-workflow case stays generic BY DESIGN — no N-file
/// audit on a greeting (`welcome --deep` owns the full truth).
/// Comments stay ≤26 chars: the widest command pads to 45 and the
/// whole row must live inside 80 columns.
fn start_moves(glance: Glance, gate: Option<&RunGate>) -> [(String, &'static str); 3] {
    match (glance.workflows, glance.agents_md) {
        // P0-4: a TRUNCATED scan that saw zero is unknown, never the
        // stranger's zero — the concierge leads with the full truth, not
        // with founding CTAs that presume an empty workspace.
        (0, _) if !glance.complete => [
            ("nika welcome --deep".to_owned(), "scan partial · the truth"),
            ("nika init".to_owned(), "found this repo (wizard)"),
            DoorId::Discover.row(),
        ],
        // The stranger's moment: nothing here yet — see one run, then found.
        // Only ever reached behind a COMPLETE scan (the arm above).
        (0, _) => [
            ("nika try 01-hello".to_owned(), "offline proof · zero keys"),
            ("nika init".to_owned(), "found this repo (wizard)"),
            ("nika new".to_owned(), "guided first workflow"),
        ],
        // Workflows live here but the agents were never briefed — the
        // founding wizard skips existing files, so it only ADDS. With
        // exactly ONE file the dim run line obeys the same P0-3 gate as
        // the head (the audit is already paid for); with several it
        // stays generic (no N-file audit on a greeting).
        (_, false) => [
            ("nika init".to_owned(), "brief agents · adds only"),
            // A comment is a promise: `nika run` bare resolves the ONE
            // workflow, so « your workflow, found » is only true when
            // there IS one. With several, the bare form exits 3 asking
            // which — the row says so up front (gauntlet 08-01).
            gate.map_or_else(
                || {
                    if glance.workflows > 1 {
                        ("nika run <file>".to_owned(), "pick one · check twin")
                    } else {
                        ("nika run".to_owned(), "your workflow, found")
                    }
                },
                run_line,
            ),
            DoorId::Discover.row(),
        ],
        // One workflow, fully founded AND clean: run it (bare — the
        // lazy door resolves the only workflow and says so).
        (1, true) => match gate {
            Some(g) if g.proposable => [
                run_line(g),
                ("nika check".to_owned(), "audit before running"),
                DoorId::Discover.row(),
            ],
            // Red — or no verdict at all: the exact file is audited
            // FIRST (a run CTA here is precisely what P0-3 forbids).
            _ => [
                gate.map_or_else(
                    || ("nika check".to_owned(), "audit before running"),
                    run_line,
                ),
                DoorId::Discover.row(),
                ("nika welcome --deep".to_owned(), "the workspace truth"),
            ],
        },
        // Several workflows, founded: the whole-workspace lens first.
        (_, true) => [
            ("nika welcome --deep".to_owned(), "the workspace truth"),
            ("nika run <file>".to_owned(), "pick one · check twin"),
            DoorId::Discover.row(),
        ],
    }
}

/// The exact-file run line (P0-3 + LOI-3): a red file gets
/// `check <path>`; a clean priced file gets the cap placeholder on the
/// command, always; a clean unpriced file gets the bare lazy-door run
/// (unpriced is UNKNOWN, never worded « free »).
fn run_line(g: &RunGate) -> (String, &'static str) {
    if !g.proposable {
        (format!("nika check {}", g.path), "red · audit before run")
    } else if g.priced {
        (
            "nika run --max-cost-usd <usd>".to_owned(),
            "priced model · cap it",
        )
    } else {
        ("nika run".to_owned(), "your workflow, found")
    }
}

/// What the current directory already holds — the workspace half of the
/// mirror (the machine half is the probe).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Glance {
    /// Inside a git repository (any ancestor carries `.git`).
    git: bool,
    /// `*.nika.yaml` / `*.nika.yml` files under the directory (bounded walk).
    workflows: usize,
    /// An `AGENTS.md` sits at the root — the repo's agents are briefed.
    agents_md: bool,
    /// The walk finished (P0-4): `false` = the count above is a LOWER
    /// BOUND (budget died · unreadable dir), and zero is UNKNOWN — the
    /// stranger's claims (« no workflows yet » · the sample) are gated
    /// on this flag.
    complete: bool,
}

/// The one-file verdict behind the run CTA (P0-3 · LOI-3) — computed
/// ONLY when the workspace carries exactly one workflow (the audit cost
/// is bounded to that file; the multi case keeps a generic CTA).
#[derive(Debug, Clone, PartialEq, Eq)]
struct RunGate {
    /// The root-relative path, for the `check <file>` CTA.
    path: String,
    /// The exact file parses AND the check ladder is clean — the ONLY
    /// condition under which welcome may print a `run` line.
    proposable: bool,
    /// At least one resolved task model carries a catalog price (LOI-3:
    /// a priced run suggestion always bears `--max-cost-usd`).
    priced: bool,
}

/// Counts DERIVED from the embedded surfaces at call time — never typed by
/// hand, so they cannot drift from the binary that prints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EngineCounts {
    builtins: usize,
    locals: usize,
    clouds: usize,
    examples: usize,
    templates: usize,
}

/// The envelope facts the mirror renders (P0-14 · W2) — the envelope
/// itself stays in `context_envelope`; the mirror eats this small owned
/// view. The legacy render ([`ContextView::legacy`]) is Workspace with an
/// EMPTY root: the workspace row then keeps its historical shape, so the
/// pre-envelope render tests stay byte-identical.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ContextView {
    /// Chat-only vs workspace — the one branch (the envelope's own).
    mode: ContextMode,
    /// The RESOLVED root the workspace row names (display form) — empty
    /// in chat-only and in the legacy render.
    root: String,
    /// The subdir the candidate expanded FROM, when it sat below the
    /// root — always displayed, never a silent rewrite.
    expanded_from: Option<PathBuf>,
}

impl ContextView {
    /// The pre-envelope render: workspace mode, no root segment.
    fn legacy() -> Self {
        Self {
            mode: ContextMode::Workspace,
            root: String::new(),
            expanded_from: None,
        }
    }
}

/// The chat-only glance: zero walk ran, so zero claim — every stranger
/// arm stays gated behind a COMPLETE scan this view never fakes.
const CHAT_ONLY_GLANCE: Glance = Glance {
    git: false,
    workflows: 0,
    agents_md: false,
    complete: false,
};

/// The root the workspace row names — the RESOLVED root, never the
/// as-typed candidate alone: a subdir candidate resolves up to the git
/// root (the evidence carries the subdir alongside, so the row shows
/// both).
fn root_label(envelope: &ContextEnvelope) -> String {
    let full = match (&envelope.evidence.expanded_from, &envelope.git_root) {
        (Some(_), Some(root)) => root.display().to_string(),
        _ => envelope.display_path.clone(),
    };
    under_home(&full)
}

/// Abbreviate a path under the home directory to `~/…`.
///
/// A path is the one field on this screen whose width belongs to the
/// person. Truncating it would be a lie about where they are, so the
/// only honest shortening is the one the shell itself understands:
/// `~` is lossless — it pastes back — and it keeps the operator's
/// account name off a screen people paste into issues.
fn under_home(path: &str) -> String {
    let Some(home) = probe::home_dir() else {
        return path.to_owned();
    };
    let home = home.to_string_lossy();
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return path.to_owned();
    }
    match path.strip_prefix(home) {
        Some("") => "~".to_owned(),
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_owned(),
    }
}

/// The `nika welcome` verb. `json` emits the versioned machine projection
/// (`welcome_version: 1` · additive-only, like every machine envelope);
/// the human mirror renders through the ONE colour seam (`Theme` ·
/// semantic never decorative — the same law every other surface obeys).
///
/// Stamp the cascade's `model:` onto a file `nika new` just wrote.
///
/// # Errors
/// Returns when the file cannot be read or rewritten.
pub fn stamp_cascade_model(path: &Path) -> std::io::Result<()> {
    crate::choice::stamp_model_file(path)
}

/// Write the first-wow workflow (`nika new hello`).
#[must_use]
pub fn write_first_wow(dest: &Path, force: bool) -> VerbOutput {
    crate::choice::write_first_wow(dest, force)
}

/// `nika new hello` / `hello.nika.yaml` — the one-shot first file.
#[must_use]
pub fn is_first_wow(from: Option<&str>, dest: Option<&str>) -> bool {
    crate::choice::is_first_wow(from, dest)
}

/// Destination `nika new hello` writes. The slug `hello` still lands a
/// workflow file, not a bare path.
#[must_use]
pub fn first_wow_dest(dest: Option<&str>) -> &str {
    crate::choice::first_wow_dest(dest)
}

#[must_use]
pub fn run(json: bool, theme: Theme) -> VerbOutput {
    let choice = crate::choice::collect();
    let candidate = std::env::current_dir().ok();
    if json {
        let out = run_in(true, theme, candidate.as_deref());
        return merge_choice_json(out, &choice);
    }
    let facts = EnvFacts {
        evidence: candidate
            .as_deref()
            .map_or(EvidenceSource::None, |_| EvidenceSource::ExplicitCwd),
        ..EnvFacts::detect()
    };
    let envelope = context_envelope::resolve(candidate.as_deref(), &facts);
    crate::metrics::record_if_enabled(
        crate::metrics::EventKind::ContextResolved,
        crate::metrics::Facts {
            session: Some(match envelope.mode {
                ContextMode::ChatOnly => crate::metrics::Session::ChatOnly,
                ContextMode::Workspace => crate::metrics::Session::Workspace,
            }),
            ..crate::metrics::Facts::none()
        },
    );
    crate::metrics::record_if_enabled(
        crate::metrics::EventKind::CtaImpression,
        crate::metrics::Facts {
            cta: Some(crate::metrics::Cta::Create),
            ..crate::metrics::Facts::none()
        },
    );
    VerbOutput::ok(crate::display::vocab::sober(
        theme,
        &choice.render_human(theme),
    ))
}

fn merge_choice_json(mut out: VerbOutput, choice: &crate::choice::InferenceChoice) -> VerbOutput {
    if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&out.text) {
        v["inference_choice"] = choice.welcome_json();
        out.text = v.to_string();
    }
    out
}

/// The envelope-first verb body (P0-14 binary-side): resolve the session
/// context from the candidate the surface named, then branch — chat-only
/// renders the truth and the spec's two doors, workspace renders the
/// mirror rooted at the RESOLVED folder. The process cwd is never
/// consulted here: `None` in, chat-only out.
fn run_in(json: bool, theme: Theme, candidate: Option<&Path>) -> VerbOutput {
    let facts = EnvFacts {
        evidence: candidate.map_or(EvidenceSource::None, |_| EvidenceSource::ExplicitCwd),
        ..EnvFacts::detect()
    };
    let envelope = context_envelope::resolve(candidate, &facts);
    let probe = probe::collect(false);
    let counts = EngineCounts {
        builtins: nika_builtin::tool_defs().len(),
        locals: probe.providers.iter().filter(|p| !p.requires_key).count(),
        clouds: probe.providers.iter().filter(|p| p.requires_key).count(),
        examples: nika_pack::example_slugs().len(),
        templates: nika_pack::template_names().len(),
    };
    if envelope.mode == ContextMode::ChatOnly {
        // No reliable folder evidence: NO walk, NO gate, NO workspace
        // claim — the mirror says « chat only » and routes to the two
        // doors the spec allows (an isolated example · a chosen project).
        let ctx = ContextView {
            mode: ContextMode::ChatOnly,
            ..ContextView::legacy()
        };
        if json {
            let experience = experience_block(&probe, &envelope, CHAT_ONLY_GLANCE, None);
            return VerbOutput::ok(render_chat_only_json(&probe, counts, experience));
        }
        crate::metrics::record_if_enabled(
            crate::metrics::EventKind::ContextResolved,
            crate::metrics::Facts {
                session: Some(crate::metrics::Session::ChatOnly),
                ..crate::metrics::Facts::none()
            },
        );
        let cmds: [String; 3] = chat_only_moves().map(|(cmd, _)| cmd);
        record_impressions(&cmds);
        return VerbOutput::ok(crate::display::vocab::sober(
            theme,
            &render_with_context(&probe, CHAT_ONLY_GLANCE, None, counts, &ctx, theme),
        ));
    }
    // Workspace: the walk + the gate run on the VALIDATED candidate (the
    // envelope's project root), never on a process-cwd guess.
    let root = envelope.project_root.clone();
    let (glance, sole) = glance(&root, 4000);
    // P0-3: the ONE file is audited before any run line may name it.
    let gate = sole.as_deref().map(|rel| run_gate(&root, rel));
    let ctx = ContextView {
        mode: ContextMode::Workspace,
        root: root_label(&envelope),
        expanded_from: envelope.evidence.expanded_from.clone(),
    };
    if json {
        let experience = experience_block(&probe, &envelope, glance, gate.as_ref());
        return VerbOutput::ok(render_json(
            &probe,
            glance,
            gate.as_ref(),
            counts,
            experience,
        ));
    }
    crate::metrics::record_if_enabled(
        crate::metrics::EventKind::ContextResolved,
        crate::metrics::Facts {
            session: Some(crate::metrics::Session::Workspace),
            flag: Some(envelope.evidence.expanded_from.is_some()),
            ..crate::metrics::Facts::none()
        },
    );
    let moves = start_moves(glance, gate.as_ref());
    let cmds: [String; 3] = [0, 1, 2].map(|i| moves[i].0.clone());
    record_impressions(&cmds);
    // The hand-off the audit measures: the concierge LEADS with a run
    // line (only ever on an audited-clean file — P0-3) → the run is
    // back in human hands.
    if cmds.first().is_some_and(|cmd| cmd.starts_with("nika run")) {
        crate::metrics::record_if_enabled(
            crate::metrics::EventKind::HumanRunHandoff,
            crate::metrics::Facts {
                handoff: Some(crate::metrics::Handoff::WelcomeCta),
                ..crate::metrics::Facts::none()
            },
        );
    }
    VerbOutput::ok(crate::display::vocab::sober(
        theme,
        &render_with_context(&probe, glance, gate.as_ref(), counts, &ctx, theme),
    ))
}

/// The concierge's CTA classes (W8 metrics): found (`init` · `new`) ·
/// go see (`examples` · `--deep`) · keep going (`run` · `check`).
fn cta_class(cmd: &str) -> crate::metrics::Cta {
    if cmd.starts_with("nika init") || cmd.starts_with("nika new") {
        crate::metrics::Cta::Create
    } else if cmd.starts_with("nika run") || cmd.starts_with("nika check") {
        crate::metrics::Cta::Continue
    } else {
        crate::metrics::Cta::Discover
    }
}

/// One `cta_impression` per move the mirror shows — the content-free
/// half of the click-through the W8 audit wants measured.
fn record_impressions<S: AsRef<str>>(moves: &[S]) {
    for cmd in moves {
        crate::metrics::record_if_enabled(
            crate::metrics::EventKind::CtaImpression,
            crate::metrics::Facts {
                cta: Some(cta_class(cmd.as_ref())),
                ..crate::metrics::Facts::none()
            },
        );
    }
}

/// The workspace glance — a bounded, dot-dir-skipping walk (depth ≤ 4 ·
/// budget-capped, 4000 entries in production): a greeting must stay
/// instant on a monorepo and must never wander into
/// `node_modules`/`target`. Returns the sole file's root-relative path
/// alongside, exactly when `workflows == 1` (the run CTA's audit target).
/// The walk's truncation flag lands in `Glance::complete` (P0-4) — the
/// budget is a parameter so tests can kill it without staging 4000 files.
fn glance(dir: &Path, walk_budget: usize) -> (Glance, Option<std::path::PathBuf>) {
    let git = dir
        .canonicalize()
        .unwrap_or_else(|_| dir.to_path_buf())
        .ancestors()
        .any(|a| a.join(".git").exists());
    let mut budget = walk_budget;
    let mut paths = Vec::new();
    let truncated = probe::collect_workflow_paths(dir, dir, 4, &mut budget, &mut paths);
    paths.sort(); // the walk orders stably; the full-path sort pins it
    let workflows = paths.len();
    let sole = (workflows == 1 && !truncated).then(|| paths.swap_remove(0));
    (
        Glance {
            git,
            workflows,
            agents_md: dir.join("AGENTS.md").exists(),
            complete: !truncated,
        },
        sole,
    )
}

/// Audit ONE file in-process — parse + the check ladder + the catalog
/// price lookup (the same per-file fold `welcome --deep` runs, here
/// bounded to the single workflow a 1-file workspace carries). Anything
/// unreadable or unparseable is RED — never silently runnable.
fn run_gate(root: &Path, rel: &Path) -> RunGate {
    let path = rel.display().to_string();
    let verdict = |proposable, priced| RunGate {
        path: path.clone(),
        proposable,
        priced,
    };
    let Ok(yaml) = std::fs::read_to_string(root.join(rel)) else {
        return verdict(false, false);
    };
    let Ok(wf) = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    ) else {
        return verdict(false, false);
    };
    let report = nika_check::check(&wf);
    let priced = report.cost.tasks.iter().any(|t| {
        t.model
            .as_deref()
            .and_then(nika_catalog::find_pricing_for)
            .is_some()
    });
    verdict(report.is_clean(), priced)
}

/// The wired/unwired glyph pair — ✓/✗ with an ASCII column (`+`/`x`),
/// painted Good/Dim: an unwired editor is an opportunity, never a
/// failure (Bad stays the run-verdict red, nothing here earns it).
fn mark(theme: Theme, on: bool) -> String {
    let raw = match (theme.ascii, on) {
        (false, true) => "✓",
        (false, false) => "✗",
        (true, true) => "+",
        (true, false) => "x",
    };
    theme.paint(if on { Role::Good } else { Role::Dim }, raw)
}

/// One client's cell in the editors row (`cursor ✓` · `vscode ✗`).
fn client_cell(theme: Theme, c: &crate::probe::ClientProbe) -> String {
    format!("{} {}", c.id, mark(theme, c.current))
}

/// The six-line taste of the language — shown ONLY when the workspace has
/// zero workflows (the stranger's moment; a workspace with files already
/// knows). The SAME shape as the embedded `01-hello` example, so the
/// START block's `nika try 01-hello` runs exactly what the eye
/// just read — a test pins that the sample checks clean for real.
pub const SAMPLE: &str = r#"nika: hello
model: mock/echo
tasks:
  greet:
    infer: { prompt: "say hello to the operator", max_tokens: 50 }"#;

/// The human mirror — sections: identity · this machine · this binary ·
/// (the language, first time only) · start here · learn. This legacy
/// entry is the TEST seam: it carries NO envelope view (an empty root),
/// so the pre-envelope render tests stay byte-identical; `run_in`
/// renders through [`render_with_context`] with the resolved envelope.
#[cfg(test)]
fn render_human(
    probe: &Probe,
    glance: Glance,
    gate: Option<&RunGate>,
    counts: EngineCounts,
    theme: Theme,
) -> String {
    render_with_context(probe, glance, gate, counts, &ContextView::legacy(), theme)
}

/// The envelope-aware mirror — the one `run_in` renders with.
fn render_with_context(
    probe: &Probe,
    glance: Glance,
    gate: Option<&RunGate>,
    counts: EngineCounts,
    ctx: &ContextView,
    theme: Theme,
) -> String {
    let mut s = String::new();
    identity_section(&mut s, probe, theme);
    machine_section(&mut s, probe, glance, ctx, theme);
    binary_section(&mut s, counts, glance, ctx, theme);
    start_section(&mut s, theme, glance, gate, ctx);
    s
}

/// Who nika is — logo · version · the three-line identity.
fn identity_section(s: &mut String, probe: &Probe, theme: Theme) {
    let _ = writeln!(
        s,
        "{} {} — Intent as Code. The workflow language for AI.",
        theme.logo(),
        theme.paint(Role::Strong, &format!("nika {}", probe.version)),
    );
    let _ = writeln!(
        s,
        "   one file · 4 verbs · one binary · audited BEFORE it runs"
    );
    let _ = writeln!(
        s,
        "   every run records a tamper-evident, hash-chained trace"
    );
    let _ = writeln!(s);
}

/// The hanging indent under `  editors    ` — continuation rows and the
/// wire handle line both sit under the label, never under the margin.
const EDITOR_HANG: &str = "             ";

/// Pack the client cells into rows that stay inside 80 display columns.
///
/// The roster is NOT a constant. Six hosts ship today, the registry
/// already names a seventh, and every addition silently widened this
/// row: on a real machine it measured **112 columns** under a published
/// 0.107.0 — a third of the line hanging off an 80-column terminal. The
/// ratchet that should have caught it was measuring a four-host fixture
/// the binary stopped matching two hosts ago (the same drift its own
/// doc comment records having learned once already, for providers).
///
/// So this measures instead of assuming, and it measures the PLAIN
/// width (`id` + space + one mark glyph) — the painted cell carries
/// zero-width escapes that would make a colour terminal wrap early.
fn editor_rows(probe: &Probe, theme: Theme) -> Vec<String> {
    let cells: Vec<(usize, String)> = probe
        .clients
        .iter()
        .map(|c| (c.id.chars().count() + 2, client_cell(theme, c)))
        .collect();
    pack_cells(&cells)
}

/// The shared column budget. Eighty is the one terminal width nobody
/// configures, so it is the one every row has to survive.
const LIMIT: usize = 80;

/// Pack `(plain_width, painted)` cells into rows that fit under a
/// 13-column label with a matching hanging indent.
///
/// Every caller here renders a list whose length is DATA, not a
/// constant — the host roster, the drifted kits — and a row that
/// assumes its data is short is a row that breaks on somebody else's
/// machine. The width is taken from the plain text because the painted
/// cell carries zero-width escapes.
fn pack_cells(cells: &[(usize, String)]) -> Vec<String> {
    let hang = EDITOR_HANG.chars().count();
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    let mut used = hang;
    for (plain, painted) in cells {
        if !row.is_empty() && used + 3 + plain > LIMIT {
            rows.push(std::mem::take(&mut row));
            used = hang;
        }
        if !row.is_empty() {
            row.push_str(" · ");
            used += 3;
        }
        row.push_str(painted);
        used += plain;
    }
    // An empty list still owns its label row — the mirror shows an
    // empty cell, it never lets the label vanish.
    if rows.is_empty() || !row.is_empty() {
        rows.push(row);
    }
    rows
}

/// The machine half of the mirror — editors · local · keys · the P0-14
/// session row ([`session_row`]).
fn machine_section(s: &mut String, probe: &Probe, glance: Glance, ctx: &ContextView, theme: Theme) {
    let _ = writeln!(s, "{}", theme.paint(Role::Strong, "this machine"));
    for (i, row) in editor_rows(probe, theme).iter().enumerate() {
        let _ = writeln!(
            s,
            "{}{row}",
            if i == 0 { "  editors    " } else { EDITOR_HANG }
        );
    }
    let unwired: Vec<&str> = probe
        .clients
        .iter()
        .filter(|c| !c.current)
        .map(|c| c.id.as_str())
        .collect();
    // H7 (audit UX 2026-07-30): never recommend `wire all` — one named
    // host, one consented mutation at a time. The handle takes its own
    // line unconditionally: hanging it off the cell list is what made
    // the row 112 columns wide, and one handle standing behind three
    // gaps reads as one gap, so the count is spoken (gauntlet 08-01).
    if let Some((first, rest)) = unwired.split_first() {
        let line = if rest.is_empty() {
            format!("→ nika wire {first}")
        } else {
            format!(
                "{} unwired · one command each → nika wire {first}",
                unwired.len()
            )
        };
        let _ = writeln!(s, "{EDITOR_HANG}{}", theme.paint(Role::Dim, &line));
    }
    local_provider_lines(s, probe, theme);
    sovereign_and_keys_lines(s, probe, glance, ctx, theme);
}

/// The keyless rows: the local-provider summary line + the P0-20
/// endpoint rows (an override moves « local » off the box — the engine
/// is NAMED with endpoint + locus, never laundered under « no key
/// needed »; loopback stays silent, the default render keeps its bytes).
fn local_provider_lines(s: &mut String, probe: &Probe, theme: Theme) {
    let locals: Vec<&str> = probe
        .providers
        .iter()
        .filter(|p| !p.requires_key)
        .map(|p| p.id.as_str())
        .collect();
    if locals.is_empty() {
        let _ = writeln!(s, "  local      no local providers in this build");
    } else {
        // The five ids alone take 47 columns — the dim tail must stay
        // ≤15 for the row to live inside 80 (doctor --ping teaches the
        // port probe; this row only says « keyless exists »).
        let _ = writeln!(
            s,
            "  local      {} {}",
            locals.join(" · "),
            theme.paint(Role::Dim, "· no key needed"),
        );
    }
    for p in &probe.providers {
        if !p.requires_key
            && matches!(
                p.readiness.execution_locus,
                ExecutionLocus::Lan | ExecutionLocus::Remote
            )
        {
            let _ = writeln!(
                s,
                "  endpoint   {} → {} ({})",
                p.id,
                crate::doctor::redact_userinfo(&p.endpoint),
                p.readiness.execution_locus.label()
            );
        }
    }
}

/// The tail of the machine section (post-provider rows).
fn sovereign_and_keys_lines(
    s: &mut String,
    probe: &Probe,
    glance: Glance,
    ctx: &ContextView,
    theme: Theme,
) {
    // The sovereign lane — ONLY when bytes are on disk (a mirror line
    // must carry information, never a lecture; zero models = silence).
    if probe.models.count > 0 {
        let _ = writeln!(
            s,
            "  models     {} pulled · {} on disk {}",
            probe.models.count,
            nika_models::store::human_size(probe.models.bytes),
            theme.paint(Role::Dim, "· nika model list"),
        );
    }
    // P0-21 — the adoption rung replaces the raw key ratio: ONE state,
    // its own metric, its own CTA (the same classifier doctor --json
    // serializes — one truth, two voices).
    let state = probe::adoption_state(probe);
    let _ = writeln!(
        s,
        "  state      {} {}",
        state.metric(probe),
        theme.paint(Role::Dim, &format!("— {}", state.cta())),
    );
    // The plugin-kit lane — ONLY on train drift (an aligned or absent
    // kit is silence; the same carry-information-never-lecture law as
    // the models row · the per-client fix lives in doctor).
    let drifted: Vec<String> = probe
        .kits
        .iter()
        .filter(|k| crate::probe::train_differs(&k.version, &probe.version))
        .map(|k| format!("{} {}", k.client, k.version))
        .collect();
    if !drifted.is_empty() {
        // Same width law as the editors roster: the drifted list is
        // data, not a constant. Three drifted kits plus the binary
        // version plus the handle measured 100 columns on a normal
        // machine — the verdict and the handle take the hanging line.
        let cells: Vec<(usize, String)> = drifted
            .iter()
            .map(|d| (d.chars().count(), d.clone()))
            .collect();
        for (i, row) in pack_cells(&cells).iter().enumerate() {
            let _ = writeln!(
                s,
                "{}{row}",
                if i == 0 { "  kits       " } else { EDITOR_HANG }
            );
        }
        let _ = writeln!(
            s,
            "{EDITOR_HANG}{}",
            theme.paint(
                Role::Dim,
                &format!("vs binary {} · fixes → nika doctor", probe.version),
            ),
        );
    }
    session_row(s, glance, ctx, theme);
    let _ = writeln!(s);
}

/// The P0-14 row closing the machine section: chat-only SAYS « chat
/// only » and claims nothing (no git bit, no count, no agents row —
/// there is no workspace here); workspace names the RESOLVED root and
/// traces the subdir expansion (displayed, never silent).
fn session_row(s: &mut String, glance: Glance, ctx: &ContextView, theme: Theme) {
    match ctx.mode {
        ContextMode::ChatOnly => {
            let _ = writeln!(s, "  session    chat only — no reliable project detected");
        }
        ContextMode::Workspace => {
            // The root is a PATH — the one field in this whole screen
            // whose width belongs to the person, not to us. A normal
            // checkout rendered this row at 150 columns. So the facts
            // move under the label when the two cannot share a line;
            // the path is never truncated (a half-path is a lie about
            // where you are).
            let facts = workspace_facts(glance, theme);
            let plain = workspace_facts(glance, Theme::new(false, false, false));
            let root_w = ctx.root.chars().count();
            if ctx.root.is_empty() {
                let _ = writeln!(s, "  workspace  {facts}");
            } else if EDITOR_HANG.chars().count() + root_w + 3 + plain.chars().count() <= LIMIT {
                let _ = writeln!(s, "  workspace  {} · {facts}", ctx.root);
            } else {
                let _ = writeln!(s, "  workspace  {}", ctx.root);
                let _ = writeln!(s, "{EDITOR_HANG}{facts}");
            }
            // The expansion trace carries a SECOND path — the subdir
            // the person actually stood in. Two unbounded paths never
            // share a line (this one rendered at 161 columns beside the
            // facts); it earns its own, and its own `~`.
            if let Some(from) = &ctx.expanded_from {
                let _ = writeln!(
                    s,
                    "{EDITOR_HANG}{}",
                    theme.paint(
                        Role::Dim,
                        &format!("from {}", under_home(&from.display().to_string())),
                    )
                );
            }
        }
    }
    let _ = writeln!(s);
}

/// The workspace row's facts — git bit · workflow count · agents brief
/// · the subdir expansion when there was one. Built twice per render
/// (painted for the eye, plain to MEASURE), because a painted string
/// carries escapes that lie about its width.
fn workspace_facts(glance: Glance, theme: Theme) -> String {
    let mut s = String::new();
    {
        let s = &mut s;
        let _ = write!(
            s,
            "git {} · {} · agents {}",
            mark(theme, glance.git),
            match (glance.workflows, glance.complete) {
                // P0-4: « no workflows yet » is a claim only a COMPLETE scan
                // may make; a truncated walk renders the honest lower bound.
                (0, true) => "no workflows yet".to_owned(),
                (0, false) => "0 found · scan partial".to_owned(),
                (1, true) => "1 workflow".to_owned(),
                (n, true) => format!("{n} workflows"),
                (n, false) => format!("{n}+ found · scan partial"),
            },
            if glance.agents_md {
                format!("briefed {} (AGENTS.md)", mark(theme, true))
            } else {
                format!("not briefed {}", theme.paint(Role::Dim, "→ nika init"))
            }
        );
    }
    s
}

/// What this binary carries — derived counts, and the six-line taste of
/// the language itself (first contact only — chat-only included: the
/// isolated example IS the sample's run).
fn binary_section(
    s: &mut String,
    counts: EngineCounts,
    glance: Glance,
    ctx: &ContextView,
    theme: Theme,
) {
    let _ = writeln!(s, "{}", theme.paint(Role::Strong, "this binary"));
    let _ = writeln!(
        s,
        "  4 verbs · {} builtins · {} providers · {} examples · {} templates",
        counts.builtins,
        counts.locals + counts.clouds,
        counts.examples,
        counts.templates
    );
    let _ = writeln!(s);
    // The stranger's moment is gated on a COMPLETE zero (P0-4) — a
    // partial scan cannot know the workspace is empty. Chat-only holds
    // no scan at all: the sample rides as the isolated example instead.
    if ctx.mode == ContextMode::ChatOnly || (glance.workflows == 0 && glance.complete) {
        let _ = writeln!(
            s,
            "{}",
            theme.paint(Role::Strong, "a whole workflow is one file")
        );
        for line in SAMPLE.lines() {
            let _ = writeln!(s, "  {line}");
        }
        let _ = writeln!(s);
    }
}

/// The chat-only doors (the spec's two: an isolated example · choosing a
/// project) plus the corpus — ONE source: the render and the W8
/// impression journal read the same moves (a drift between what is shown
/// and what is measured would make the metric a lie).
fn chat_only_moves() -> [(String, &'static str); 3] {
    [
        ("nika try 01-hello".to_owned(), "isolated · zero keys"),
        (
            "cd <project> && nika welcome".to_owned(),
            "choose a project first",
        ),
        DoorId::Discover.row(),
    ]
}

/// Every command the concierge can ever teach, across every workspace
/// state — the parse-ratchet surface. The `nika-cli` unit replays each
/// one against the live clap tree, so a door rename (the 0.107
/// `examples` → `try` move that welcome kept teaching) breaks a test
/// before it can break a paste. Placeholders (`<file>` · `<usd>` ·
/// `<project>`) are the operator's slots; the ratchet fills them.
#[must_use]
pub fn taught_start_commands() -> Vec<String> {
    let gates = [
        None,
        Some(RunGate {
            path: "wf.nika.yaml".to_owned(),
            proposable: false,
            priced: false,
        }),
        Some(RunGate {
            path: "wf.nika.yaml".to_owned(),
            proposable: true,
            priced: false,
        }),
        Some(RunGate {
            path: "wf.nika.yaml".to_owned(),
            proposable: true,
            priced: true,
        }),
    ];
    let mut commands: Vec<String> = Vec::new();
    for workflows in [0usize, 1, 3] {
        for agents_md in [false, true] {
            for complete in [false, true] {
                let glance = Glance {
                    git: true,
                    workflows,
                    agents_md,
                    complete,
                };
                for gate in &gates {
                    for (command, _) in start_moves(glance, gate.as_ref()) {
                        if !commands.contains(&command) {
                            commands.push(command);
                        }
                    }
                }
            }
        }
    }
    for (command, _) in chat_only_moves() {
        if !commands.contains(&command) {
            commands.push(command);
        }
    }
    for command in [
        "nika new",
        "nika new hello",
        "nika run",
        "nika check",
        "nika doctor",
    ] {
        if !commands.iter().any(|c| c == command) {
            commands.push((*command).to_owned());
        }
    }
    commands
}

/// The hand-off — the state's own three moves, then where to learn more.
/// Chat-only keys on no workspace at all: the two doors the spec allows
/// (an isolated example · choosing a project) plus the corpus.
fn start_section(
    s: &mut String,
    theme: Theme,
    glance: Glance,
    gate: Option<&RunGate>,
    ctx: &ContextView,
) {
    let _ = writeln!(
        s,
        "{}",
        theme.paint(Role::Strong, "start here (offline · zero keys)")
    );
    let moves = if ctx.mode == ContextMode::ChatOnly {
        chat_only_moves()
    } else {
        start_moves(glance, gate)
    };
    let width = moves
        .iter()
        .map(|(cmd, _)| cmd.chars().count())
        .max()
        .unwrap_or(0);
    // The gh/bun law (2026 survey): exactly ONE next command carries the
    // maximum visual weight — the first row is the thing to run NOW, the
    // other two stay plain (a journey, not a menu of equals).
    for (i, (cmd, why)) in moves.iter().enumerate() {
        let painted = if i == 0 {
            theme.paint(Role::Strong, &format!("{cmd:<width$}"))
        } else {
            format!("{cmd:<width$}")
        };
        let _ = writeln!(
            s,
            "  {painted}   {}",
            theme.paint(Role::Dim, &format!("# {why}"))
        );
    }
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "{}",
        theme.paint(
            Role::Dim,
            &format!(
                "learn: {} · docs: {} · ⭐ {}",
                theme.link("https://nika.sh", "nika.sh"),
                theme.link("https://docs.nika.sh", "docs.nika.sh"),
                theme.link(
                    "https://github.com/supernovae-st/nika",
                    "github.com/supernovae-st/nika"
                ),
            )
        )
    );
}

/// The experience block riding `welcome --json` (additive against
/// `welcome_version: 1`) — the FIRST consumer of the router: the same
/// facts the concierge already proves (envelope mode · glance · the
/// one-file gate · kit drift) fold into an
/// [`ExperienceStateV1`](crate::experience::ExperienceStateV1), and
/// `route()` answers with the one next action. MCP and the plugin read
/// THIS object next — one truth, many projections, never a second
/// ladder. Honest floors of this projection, stated not hidden:
/// `findings` is 0 (the gate carries a verdict, not a count),
/// paused/failed traces are the deep lens's knowledge (not folded
/// yet), `journey` has no local persistence (always `orientation`),
/// and `intent` is unknowable at a greeting.
fn experience_block(
    probe: &Probe,
    envelope: &ContextEnvelope,
    glance: Glance,
    gate: Option<&RunGate>,
) -> serde_json::Value {
    use crate::experience::{
        ContextEvidenceV1, ContextModeV1, ExperienceStateV1, WorkflowStateV1, route,
    };
    let chat_only = envelope.mode == ContextMode::ChatOnly;
    // The multi-root door. Several roots are open and none was named:
    // the envelope has always known it, the router has always had the
    // `select_root` arm for it — but `context_mode` only ever spoke
    // two words, so the arm was unreachable and the predicate sat
    // behind a dead-code exemption. One wire closes both, and every
    // claim below narrows: an unchosen root is not a root, so nothing
    // is named, scanned or called writable until the person picks.
    let unselected = !chat_only && envelope.requires_explicit_root();
    let evidence = if chat_only {
        ContextEvidenceV1::None
    } else {
        match envelope.evidence.source {
            EvidenceSource::HostSelection => ContextEvidenceV1::HostSelection,
            EvidenceSource::ActiveFile => ContextEvidenceV1::ActiveFile,
            // The CLI's cwd IS the operator's explicit choice (they
            // stood there when they called).
            EvidenceSource::ExplicitCwd => ContextEvidenceV1::ExplicitUser,
            EvidenceSource::None => ContextEvidenceV1::None,
        }
    };
    let workflow = if chat_only || unselected {
        WorkflowStateV1::Unknown
    } else {
        // P0-4 applies to EVERY count, not just zero: a truncated walk
        // that saw one file knows nothing about the rest — and the one
        // it saw was never audited (`glance` only hands a sole path,
        // and therefore a gate, behind a COMPLETE walk). A `Clean` here
        // would have the JSON contract claim an audit the text render
        // refuses in the same breath (« 1+ found · scan partial ») —
        // the two-surfaces-one-evidence law, caught by the refuter pass
        // before it ever shipped.
        match (glance.workflows, gate) {
            (_, _) if !glance.complete => WorkflowStateV1::Unknown,
            (0, _) => WorkflowStateV1::Absent,
            (1, Some(g)) if !g.proposable => WorkflowStateV1::Findings,
            (1, Some(_)) => WorkflowStateV1::Clean,
            // A complete walk that saw one file always carries its
            // gate; without one the state is unknown, never clean.
            (1, None) => WorkflowStateV1::Unknown,
            (_, _) => WorkflowStateV1::Several,
        }
    };
    let root = (!chat_only && !unselected).then(|| envelope.project_root.display().to_string());
    // ONE writability truth: the envelope already probed it (mode bits
    // AND the mirror-safe rules) — recomputing it here would be a
    // second answer to the same question.
    let writable = !chat_only && !unselected && envelope.writable;
    let configured = probe.providers.iter().any(|p| !p.requires_key)
        || probe
            .providers
            .iter()
            .any(|p| p.requires_key && p.key_present);
    let state = ExperienceStateV1 {
        context_mode: if chat_only {
            ContextModeV1::ChatOnly
        } else if unselected {
            ContextModeV1::MultiRootUnselected
        } else {
            ContextModeV1::Workspace
        },
        evidence,
        root,
        writable,
        inventory_complete: !chat_only && !unselected && glance.complete,
        workflow,
        workflow_path: gate.map(|g| g.path.clone()),
        provider: if configured {
            crate::experience::ProviderStateV1::Configured
        } else {
            crate::experience::ProviderStateV1::Unconfigured
        },
        // ONE drift law with the rendered line (probe::train_differs):
        // a PATCH is not a train. String equality here would have made
        // the router cry `align_versions` the day a 0.107.1 binary met
        // a 0.107.0 kit — degrading every richer CTA on the very
        // release this wave is cutting, while the human line stayed
        // silent. Two definitions of drift in one file is one too many.
        versions_coherent: !probe
            .kits
            .iter()
            .any(|k| probe::train_differs(&k.version, &probe.version)),
        ..ExperienceStateV1::chat_only()
    };
    let action = route(&state);
    serde_json::json!({ "state": state, "action": action })
}

/// The versioned machine mirror — additive-only (`welcome_version: 1`).
/// Names and booleans and counts, by construction: nothing in the probe
/// carries a value a secret could ride.
fn render_json(
    probe: &Probe,
    glance: Glance,
    gate: Option<&RunGate>,
    counts: EngineCounts,
    experience: serde_json::Value,
) -> String {
    let moves = start_moves(glance, gate);
    let start: Vec<&str> = moves.iter().map(|(cmd, _)| cmd.as_str()).collect();
    let mut machine = probe::environment_json(probe);
    machine["config"] = serde_json::json!(probe.config_path);
    let mut v = serde_json::json!({
        "welcome_version": 1,
        "version": probe.version,
        "machine": machine,
        "workspace": {
            "git": glance.git,
            "workflows": glance.workflows,
            "agents_md": glance.agents_md,
            "inventory_complete": glance.complete,
        },
        "engine": {
            "verbs": 4,
            "builtins": counts.builtins,
            "local_providers": counts.locals,
            "cloud_providers": counts.clouds,
            "examples": counts.examples,
            "templates": counts.templates,
        },
        "start": start,
    });
    v["experience"] = experience;
    v.to_string()
}

/// The chat-only machine mirror — the SAME versioned envelope, minus
/// every workspace claim (there is none to make), plus the mode and the
/// two doors the spec allows. Additive against `welcome_version: 1`: the
/// `context` key is the chat-only signal.
fn render_chat_only_json(
    probe: &Probe,
    counts: EngineCounts,
    experience: serde_json::Value,
) -> String {
    let mut machine = probe::environment_json(probe);
    machine["config"] = serde_json::json!(probe.config_path);
    let mut v = serde_json::json!({
        "welcome_version": 1,
        "version": probe.version,
        "context": { "mode": "chat_only" },
        "machine": machine,
        "engine": {
            "verbs": 4,
            "builtins": counts.builtins,
            "local_providers": counts.locals,
            "cloud_providers": counts.clouds,
            "examples": counts.examples,
            "templates": counts.templates,
        },
        // ONE source with the rendered text (the W8 law): the JSON
        // mirror derives from `chat_only_moves()`, never a hand-kept
        // twin array — two lists of the same moves drift.
        "start": chat_only_moves()
            .iter()
            .map(|(cmd, _)| cmd.clone())
            .collect::<Vec<String>>(),
    });
    v["experience"] = experience;
    v.to_string()
}

#[cfg(test)]
mod tests;
