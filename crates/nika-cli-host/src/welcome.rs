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
            ("nika examples".to_owned(), "the teaching corpus"),
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
            gate.map_or_else(|| ("nika run".to_owned(), "your workflow, found"), run_line),
            ("nika examples".to_owned(), "the teaching corpus"),
        ],
        // One workflow, fully founded AND clean: run it (bare — the
        // lazy door resolves the only workflow and says so).
        (1, true) => match gate {
            Some(g) if g.proposable => [
                run_line(g),
                ("nika check".to_owned(), "audit before running"),
                ("nika examples".to_owned(), "the teaching corpus"),
            ],
            // Red — or no verdict at all: the exact file is audited
            // FIRST (a run CTA here is precisely what P0-3 forbids).
            _ => [
                gate.map_or_else(
                    || ("nika check".to_owned(), "audit before running"),
                    run_line,
                ),
                ("nika examples".to_owned(), "the teaching corpus"),
                ("nika welcome --deep".to_owned(), "the workspace truth"),
            ],
        },
        // Several workflows, founded: the whole-workspace lens first.
        (_, true) => [
            ("nika welcome --deep".to_owned(), "the workspace truth"),
            ("nika run <file>".to_owned(), "pick one · check twin"),
            ("nika examples".to_owned(), "the teaching corpus"),
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
    match (&envelope.evidence.expanded_from, &envelope.git_root) {
        (Some(_), Some(root)) => root.display().to_string(),
        _ => envelope.display_path.clone(),
    }
}

/// The `nika welcome` verb. `json` emits the versioned machine projection
/// (`welcome_version: 1` · additive-only, like every machine envelope);
/// the human mirror renders through the ONE colour seam (`Theme` ·
/// semantic never decorative — the same law every other surface obeys).
///
/// The CLI's folder evidence is the directory the operator LAUNCHED in —
/// the surface names it explicitly; `resolve` never invents one.
#[must_use]
pub fn run(json: bool, theme: Theme) -> VerbOutput {
    let candidate = std::env::current_dir().ok();
    run_in(json, theme, candidate.as_deref())
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
            return VerbOutput::ok(render_chat_only_json(&probe, counts));
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
        return VerbOutput::ok(render_json(&probe, glance, gate.as_ref(), counts));
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
pub const SAMPLE: &str = r#"nika: v1
workflow:
  id: hello
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

/// The machine half of the mirror — editors · local · keys · the P0-14
/// session row ([`session_row`]).
fn machine_section(s: &mut String, probe: &Probe, glance: Glance, ctx: &ContextView, theme: Theme) {
    let _ = writeln!(s, "{}", theme.paint(Role::Strong, "this machine"));
    let editors: Vec<String> = probe
        .clients
        .iter()
        .map(|c| client_cell(theme, c))
        .collect();
    let unwired = probe.clients.iter().find(|c| !c.current);
    let _ = writeln!(
        s,
        "  editors    {}{}",
        editors.join(" · "),
        if let Some(c) = unwired {
            // H7 (audit UX 2026-07-30): never recommend `wire all` — one
            // named host, one consented mutation at a time. The short
            // form keeps the line under 80 and names the exact command.
            theme.paint(Role::Dim, &format!("   → nika wire {}", c.id))
        } else {
            String::new()
        }
    );
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
        let _ = writeln!(
            s,
            "  kits       {} vs binary {} {}",
            drifted.join(" · "),
            probe.version,
            theme.paint(Role::Dim, "· fixes → nika doctor"),
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
            let _ = write!(s, "  workspace  ");
            if !ctx.root.is_empty() {
                let _ = write!(s, "{} · ", ctx.root);
            }
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
            if let Some(from) = &ctx.expanded_from {
                let _ = write!(
                    s,
                    " {}",
                    theme.paint(Role::Dim, &format!("· from {}", from.display()))
                );
            }
            let _ = writeln!(s);
        }
    }
    let _ = writeln!(s);
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
        ("nika examples".to_owned(), "the teaching corpus"),
    ]
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

/// The versioned machine mirror — additive-only (`welcome_version: 1`).
/// Names and booleans and counts, by construction: nothing in the probe
/// carries a value a secret could ride.
fn render_json(
    probe: &Probe,
    glance: Glance,
    gate: Option<&RunGate>,
    counts: EngineCounts,
) -> String {
    let moves = start_moves(glance, gate);
    let start: Vec<&str> = moves.iter().map(|(cmd, _)| cmd.as_str()).collect();
    let mut machine = probe::environment_json(probe);
    machine["config"] = serde_json::json!(probe.config_path);
    serde_json::json!({
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
    })
    .to_string()
}

/// The chat-only machine mirror — the SAME versioned envelope, minus
/// every workspace claim (there is none to make), plus the mode and the
/// two doors the spec allows. Additive against `welcome_version: 1`: the
/// `context` key is the chat-only signal.
fn render_chat_only_json(probe: &Probe, counts: EngineCounts) -> String {
    let mut machine = probe::environment_json(probe);
    machine["config"] = serde_json::json!(probe.config_path);
    serde_json::json!({
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
        "start": [
            "nika try 01-hello",
            "cd <project> && nika welcome",
            "nika examples",
        ],
    })
    .to_string()
}

#[cfg(test)]
mod tests;
