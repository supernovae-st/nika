// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika welcome` — the mirror moment (30-seconds surface · first contact).
//!
//! One command, one screen: what Nika IS (the tagline block), what THIS
//! machine already has (the shared `verbs::probe` engine — the same
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

use std::fmt::Write as _;
use std::path::Path;

use nika_providers::probe::ExecutionLocus;

use crate::display::theme::{Role, Theme};
use crate::verbs::probe::Probe;
use crate::verbs::{VerbOutput, probe};

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
            (
                "nika examples run 01-hello --model mock/echo".to_owned(),
                "offline proof · zero keys",
            ),
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

/// The `nika welcome` verb. `json` emits the versioned machine projection
/// (`welcome_version: 1` · additive-only, like every machine envelope);
/// the human mirror renders through the ONE colour seam (`Theme` ·
/// semantic never decorative — the same law every other surface obeys).
#[must_use]
pub fn run(json: bool, theme: Theme) -> VerbOutput {
    let probe = probe::collect(false);
    let root = Path::new(".");
    let (glance, sole) = glance(root, 4000);
    // P0-3: the ONE file is audited before any run line may name it.
    let gate = sole.as_deref().map(|rel| run_gate(root, rel));
    let counts = EngineCounts {
        builtins: nika_builtin::tool_defs().len(),
        locals: probe.providers.iter().filter(|p| !p.requires_key).count(),
        clouds: probe.providers.iter().filter(|p| p.requires_key).count(),
        examples: nika_pack::example_slugs().len(),
        templates: nika_pack::template_names().len(),
    };
    if json {
        return VerbOutput::ok(render_json(&probe, glance, gate.as_ref(), counts));
    }
    VerbOutput::ok(render_human(&probe, glance, gate.as_ref(), counts, theme))
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
fn client_cell(theme: Theme, c: &crate::verbs::probe::ClientProbe) -> String {
    format!("{} {}", c.id, mark(theme, c.current))
}

/// The six-line taste of the language — shown ONLY when the workspace has
/// zero workflows (the stranger's moment; a workspace with files already
/// knows). The SAME shape as the embedded `01-hello` example, so the
/// START block's `nika examples run 01-hello` runs exactly what the eye
/// just read — a test pins that the sample checks clean for real.
pub(crate) const SAMPLE: &str = r#"nika: v1
workflow:
  id: hello
model: mock/echo
tasks:
  greet:
    infer: { prompt: "say hello to the operator", max_tokens: 50 }"#;

/// The human mirror — sections: identity · this machine · this binary ·
/// (the language, first time only) · start here · learn. One helper per
/// beat (the 100-line fn cap forced this shape here too, and the shape
/// is better); pure over its inputs (tests pass synthetic probes + a
/// plain theme).
fn render_human(
    probe: &Probe,
    glance: Glance,
    gate: Option<&RunGate>,
    counts: EngineCounts,
    theme: Theme,
) -> String {
    let mut s = String::new();
    identity_section(&mut s, probe, theme);
    machine_section(&mut s, probe, glance, theme);
    binary_section(&mut s, counts, glance, theme);
    start_section(&mut s, theme, glance, gate);
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

/// The machine half of the mirror — editors · local · keys · workspace.
fn machine_section(s: &mut String, probe: &Probe, glance: Glance, theme: Theme) {
    let _ = writeln!(s, "{}", theme.paint(Role::Strong, "this machine"));
    let editors: Vec<String> = probe
        .clients
        .iter()
        .map(|c| client_cell(theme, c))
        .collect();
    let unwired = probe.clients.iter().any(|c| !c.current);
    let _ = writeln!(
        s,
        "  editors    {}{}",
        editors.join(" · "),
        if unwired {
            // Four unwired ids + a long hint broke 80 — the short form
            // still names the exact command.
            theme.paint(Role::Dim, "   → nika wire all")
        } else {
            String::new()
        }
    );
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
    // P0-20 · an endpoint override (NIKA_<ID>_BASE_URL · OLLAMA_HOST)
    // moves « local » off this box: the engine is NAMED with endpoint +
    // locus, never laundered under « no key needed ». Loopback stays
    // silent — the default render keeps its exact bytes.
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
                crate::verbs::doctor::redact_userinfo(&p.endpoint),
                p.readiness.execution_locus.label()
            );
        }
    }
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
        .filter(|k| crate::verbs::probe::train_differs(&k.version, &probe.version))
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
    let _ = writeln!(
        s,
        "  workspace  git {} · {} · agents {}",
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
    let _ = writeln!(s);
}

/// What this binary carries — derived counts, and (first contact only)
/// the six-line taste of the language itself.
fn binary_section(s: &mut String, counts: EngineCounts, glance: Glance, theme: Theme) {
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
    // partial scan cannot know the workspace is empty.
    if glance.workflows == 0 && glance.complete {
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

/// The hand-off — the state's own three moves, then where to learn more.
fn start_section(s: &mut String, theme: Theme, glance: Glance, gate: Option<&RunGate>) {
    let _ = writeln!(
        s,
        "{}",
        theme.paint(Role::Strong, "start here (offline · zero keys)")
    );
    let moves = start_moves(glance, gate);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::exit;
    use crate::verbs::probe::{
        ClientProbe, ImageProbe, PingState, PricingProbe, ProviderProbe, TtsProbe,
    };
    use nika_providers::probe::{ExecutionLocus, ProviderReadiness};

    /// The default synthetic readiness — recognized · loopback · the
    /// opt-in rungs unmeasured (mirrors `collect_provider_probes`).
    fn readiness(configured: bool, locus: ExecutionLocus) -> ProviderReadiness {
        ProviderReadiness {
            recognized: true,
            configured,
            reachable: None,
            model_available: None,
            priced: false,
            execution_locus: locus,
        }
    }

    fn synthetic_probe() -> Probe {
        Probe {
            models: crate::verbs::probe::ModelsProbe::default(),
            version: "0.0.0-test".to_owned(),
            config_path: None,
            providers: vec![
                ProviderProbe {
                    id: "ollama".to_owned(),
                    requires_key: false,
                    key_present: false,
                    fix_var: "NIKA_OLLAMA_API_KEY".to_owned(),
                    structured_native: true,
                    readiness: readiness(true, ExecutionLocus::Loopback),
                    endpoint: "http://127.0.0.1:11434".to_owned(),
                },
                ProviderProbe {
                    id: "mistral".to_owned(),
                    requires_key: true,
                    key_present: false,
                    fix_var: "MISTRAL_API_KEY".to_owned(),
                    structured_native: true,
                    readiness: readiness(false, ExecutionLocus::Cloud),
                    endpoint: "https://api.mistral.ai/v1/chat/completions".to_owned(),
                },
                ProviderProbe {
                    id: "anthropic".to_owned(),
                    requires_key: true,
                    key_present: true,
                    fix_var: "ANTHROPIC_API_KEY".to_owned(),
                    structured_native: true,
                    readiness: readiness(true, ExecutionLocus::Cloud),
                    endpoint: "https://api.anthropic.com/v1/messages".to_owned(),
                },
            ],
            clients: vec![
                ClientProbe {
                    id: "cursor".to_owned(),
                    path: "~/.cursor/mcp.json".to_owned(),
                    present: true,
                    current: true,
                    stale: false,
                },
                ClientProbe {
                    id: "vscode".to_owned(),
                    path: "./.vscode/mcp.json".to_owned(),
                    present: false,
                    current: false,
                    stale: false,
                },
            ],
            kits: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
        }
    }

    fn counts() -> EngineCounts {
        EngineCounts {
            builtins: 7,
            locals: 1,
            clouds: 2,
            examples: 3,
            templates: 2,
        }
    }

    fn plain() -> Theme {
        Theme::new(false, false, false)
    }

    /// The concierge hands over ONE key per state — the four states each
    /// lead with their own move. The 1-workflow-founded state is keyed on
    /// the file's VERDICT too (P0-3): the clean gate below is what keeps
    /// `run` eligible at all.
    #[test]
    fn start_moves_key_on_the_workspace_state() {
        let g = |workflows, agents_md| Glance {
            git: true,
            workflows,
            agents_md,
            complete: true,
        };
        let clean = RunGate {
            path: "a.nika.yaml".to_owned(),
            proposable: true,
            priced: false,
        };
        assert!(
            start_moves(g(0, false), None)[0]
                .0
                .contains("examples run 01-hello")
        );
        assert_eq!(start_moves(g(2, false), None)[0].0, "nika init");
        // FLIP (P0-3 · audit UX 2026-07-30): this line pinned « 1 workflow
        // + AGENTS.md → nika run head » with NO verdict — the finding's
        // exact reproduction. `run` now leads ONLY behind a clean gate;
        // the red and priced arms are pinned by the scratch-dir tests
        // (one_red_workflow_gets_check_never_run · the LOI-3 twins).
        assert_eq!(start_moves(g(1, true), Some(&clean))[0].0, "nika run");
        assert_eq!(start_moves(g(5, true), None)[0].0, "nika welcome --deep");
    }

    /// A scratch workspace on disk (auto-cleaned) — the run-CTA tests
    /// audit REAL files, never synthetic flags.
    fn scratch(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("scratch dir");
        for (name, body) in files {
            std::fs::write(dir.path().join(name), body).expect("write");
        }
        dir
    }

    /// The context.rs red fixture: `when:` as a bare string is a
    /// conformance finding — the file parses, the ladder refuses it.
    const RED_WORKFLOW: &str = "nika: v1\nworkflow:\n  id: bad\ntasks:\n  a:\n    exec: { command: [\"echo\", \"x\"] }\n  b:\n    after:\n      a: success\n    when: maybe\n    exec: { command: [\"echo\", \"y\"] }\n";

    /// P0-3 (audit UX 2026-07-30) — one RED workflow, agents briefed:
    /// the concierge must NEVER carry a `nika run` CTA (head or dim)
    /// for a file the ladder has not seen clean; the head is
    /// `nika check <the exact file>`. Human and JSON both eat from
    /// `start_moves`, and the JSON projection is asserted too.
    #[test]
    fn one_red_workflow_gets_check_never_run() {
        let dir = scratch(&[("AGENTS.md", "x"), ("bad.nika.yaml", RED_WORKFLOW)]);
        let (g, sole) = glance(dir.path(), 4000);
        assert_eq!(g.workflows, 1, "the scratch holds exactly one file");
        assert!(g.agents_md);
        let gate = sole.as_deref().map(|rel| run_gate(dir.path(), rel));
        assert!(
            !gate.as_ref().expect("a gate for the sole file").proposable,
            "the `when: maybe` fixture is RED for real"
        );
        let moves = start_moves(g, gate.as_ref());
        assert_eq!(
            moves[0].0, "nika check bad.nika.yaml",
            "a red file is audited, never run: {moves:?}"
        );
        for (cmd, _) in &moves {
            assert!(
                !cmd.starts_with("nika run"),
                "no run CTA while the exact file is red: {moves:?}"
            );
        }
        let raw = render_json(&synthetic_probe(), g, gate.as_ref(), counts());
        let v: serde_json::Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(v["start"][0], "nika check bad.nika.yaml", "{raw}");
        assert!(
            !raw.contains("nika run"),
            "the JSON mirror carries no run CTA on a red file: {raw}"
        );
    }

    /// The twin that must NOT regress: one CLEAN unpriced workflow
    /// (mock/echo) keeps the bare `nika run` head — and the wording
    /// never calls an unpriced model « free » (LOI-3: unknown stays
    /// unknown, it is merely uncapped by law's absence of a price).
    #[test]
    fn one_clean_mock_workflow_keeps_the_run_cta_uncapped() {
        let dir = scratch(&[
            ("AGENTS.md", "x"),
            (
                "good.nika.yaml",
                "nika: v1\nworkflow:\n  id: good\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n",
            ),
        ]);
        let (g, sole) = glance(dir.path(), 4000);
        assert_eq!(g.workflows, 1);
        let gate = sole.as_deref().map(|rel| run_gate(dir.path(), rel));
        let gate = gate.expect("a gate for the sole file");
        assert!(
            gate.proposable && !gate.priced,
            "mock/echo: clean, unpriced"
        );
        let moves = start_moves(g, Some(&gate));
        assert_eq!(
            moves[0].0, "nika run",
            "clean + unpriced → run leads: {moves:?}"
        );
        for (cmd, _) in &moves {
            assert!(
                !cmd.contains("--max-cost-usd"),
                "an unpriced model carries no cap placeholder: {moves:?}"
            );
        }
        let text = render_human(&synthetic_probe(), g, Some(&gate), counts(), plain());
        assert!(
            !text.contains("free"),
            "unpriced is never « free »:\n{text}"
        );
    }

    /// LOI-3 — one CLEAN workflow on a PRICED model (openai/*): the run
    /// line the concierge prints always carries the spend cap, with an
    /// explicit placeholder the operator fills.
    #[test]
    fn one_clean_priced_workflow_carries_the_loi3_cap() {
        let dir = scratch(&[
            ("AGENTS.md", "x"),
            (
                "priced.nika.yaml",
                "nika: v1\nworkflow:\n  id: priced\nmodel: openai/gpt-4o-mini\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n",
            ),
        ]);
        let (g, sole) = glance(dir.path(), 4000);
        assert_eq!(g.workflows, 1);
        let gate = sole.as_deref().map(|rel| run_gate(dir.path(), rel));
        let gate = gate.expect("a gate for the sole file");
        assert!(gate.proposable && gate.priced, "openai/*: clean, priced");
        let moves = start_moves(g, Some(&gate));
        assert!(
            moves[0].0.starts_with("nika run"),
            "a clean priced file still runs head-first: {moves:?}"
        );
        assert!(
            moves[0].0.contains("--max-cost-usd"),
            "LOI-3: a priced run CTA carries the cap: {moves:?}"
        );
    }

    /// The mirror speaks the sovereign lane ONLY when bytes are on disk
    /// (a pulled model must be visible — the machine section IS "what
    /// this machine already has"; zero models = zero line, never a
    /// lecture).
    #[test]
    fn mirror_shows_pulled_models_and_stays_silent_at_zero() {
        let glance = Glance {
            git: true,
            workflows: 0,
            agents_md: false,
            complete: true,
        };
        let silent = render_human(&synthetic_probe(), glance, None, counts(), plain());
        assert!(
            !silent.contains("  models"),
            "zero models = zero line:\n{silent}"
        );

        let mut probe = synthetic_probe();
        probe.models.count = 2;
        probe.models.bytes = 211 * 1024 * 1024;
        let shown = render_human(&probe, glance, None, counts(), plain());
        assert!(
            shown.contains("models     2 pulled · 211.0 MiB on disk"),
            "the sovereign lane is in the mirror:\n{shown}"
        );
        assert!(shown.contains("nika model list"), "{shown}");
    }

    /// The endpoint lane obeys the same law as the kit lane: loopback
    /// everywhere is silence (the byte-identical default) — only an
    /// override-pointed engine (`NIKA_OLLAMA_BASE_URL` · `OLLAMA_HOST`) earns
    /// a line, NAMED with its endpoint and locus (P0-20: « local » is a
    /// protocol, never a topology).
    #[test]
    fn mirror_names_an_endpoint_override_and_stays_silent_on_loopback() {
        let glance = Glance {
            git: true,
            workflows: 0,
            agents_md: false,
            complete: true,
        };
        let silent = render_human(&synthetic_probe(), glance, None, counts(), plain());
        assert!(
            !silent.contains("  endpoint"),
            "loopback = no endpoint line:\n{silent}"
        );

        let mut probe = synthetic_probe();
        probe.providers[0].endpoint = "http://gpu.lan:11434".to_owned();
        probe.providers[0].readiness.execution_locus = ExecutionLocus::Lan;
        let shown = render_human(&probe, glance, None, counts(), plain());
        assert!(
            shown.contains("ollama → http://gpu.lan:11434 (lan)"),
            "the override is NAMED next to the local row:\n{shown}"
        );
        assert!(
            shown.contains("· no key needed"),
            "the keyless truth stays — only the topology claim is fixed:\n{shown}"
        );
    }

    /// The kit lane obeys the same law: an aligned (or absent) plugin
    /// kit is silence — only TRAIN drift earns a line, and the line
    /// routes to doctor (the per-client fix lives there, not here).
    #[test]
    fn mirror_names_kit_drift_and_stays_silent_when_aligned() {
        let glance = Glance {
            git: true,
            workflows: 0,
            agents_md: false,
            complete: true,
        };
        let silent = render_human(&synthetic_probe(), glance, None, counts(), plain());
        assert!(!silent.contains("  kits"), "no kits = no line:\n{silent}");

        let mut probe = synthetic_probe();
        probe.kits = vec![
            crate::verbs::probe::KitProbe {
                client: "codex".to_owned(),
                version: "0.0.7".to_owned(), // same 0.0 train as the binary
            },
            crate::verbs::probe::KitProbe {
                client: "claude".to_owned(),
                version: "0.105.0".to_owned(), // another train — drift
            },
        ];
        let shown = render_human(&probe, glance, None, counts(), plain());
        assert!(
            shown.contains("kits       claude 0.105.0 vs binary 0.0.0-test"),
            "only the DRIFTED kit is named, the aligned one is silent:\n{shown}"
        );
        assert!(!shown.contains("codex"), "aligned kit stays out:\n{shown}");
        assert!(shown.contains("fixes → nika doctor"), "{shown}");
    }

    /// P0-21 (audit UX 2026-07-30) — the mirror greets with the adoption
    /// rung: each state carries its OWN metric and its OWN CTA, and the
    /// line never claims more than the probe measured.
    #[test]
    fn the_mirror_greets_each_adoption_rung_with_its_own_cta() {
        let glance = Glance {
            git: true,
            workflows: 2,
            agents_md: true,
            complete: true,
        };
        // KeyPresent — the stock synthetic machine (anthropic keyed).
        let keyed = render_human(&synthetic_probe(), glance, None, counts(), plain());
        assert!(
            keyed.contains(
                "state      key present · 1 of 2 clouds configured — ready for a real run"
            ),
            "KeyPresent renders its own line:\n{keyed}"
        );

        // Installed — strip every engagement fact: no key, no ping, no
        // pulled model, no journal. The catalog's keyless seeds do NOT
        // count as detection.
        let mut bare = synthetic_probe();
        bare.providers[2].key_present = false;
        bare.providers[2].readiness.configured = false;
        let installed = render_human(&bare, glance, None, counts(), plain());
        assert!(
            installed.contains(
                "state      installed · no inference path — proof → nika examples run 01-hello"
            ),
            "Installed routes to the offline proof:\n{installed}"
        );

        // LocalDetected — an override moves ollama off its loopback seed.
        let mut lan = bare.clone();
        lan.providers[0].endpoint = "http://gpu.lan:11434".to_owned();
        lan.providers[0].readiness.execution_locus = ExecutionLocus::Lan;
        let detected = render_human(&lan, glance, None, counts(), plain());
        assert!(
            detected.contains(
                "state      ollama detected · unproven — start it, then nika doctor --ping"
            ),
            "LocalDetected names the engine and the ping hand-off:\n{detected}"
        );

        // LocalReachable — ONLY a --ping measurement earns « reachable ».
        let mut pinged = bare.clone();
        pinged.local_pings = vec![(
            "ollama".to_owned(),
            "127.0.0.1:11434".to_owned(),
            PingState::Reachable(3),
        )];
        let reachable = render_human(&pinged, glance, None, counts(), plain());
        assert!(
            reachable.contains("state      local reachable · ollama (3ms) — point a run at it"),
            "LocalReachable carries the measured round-trip:\n{reachable}"
        );

        // RealReady — a live path AND runs on record. The wording says
        // « path configured »: the journal proves runs, never the model.
        let mut real = synthetic_probe();
        real.recorded_runs = 2;
        let ready = render_human(&real, glance, None, counts(), plain());
        assert!(
            ready.contains("state      real-ready · 2 runs on record · path configured — nika run"),
            "RealReady claims the record, never « a real model answered »:\n{ready}"
        );
        // Every rung's line is its own — the five renders differ.
        let lines: std::collections::BTreeSet<String> =
            [keyed, installed, detected, reachable, ready]
                .iter()
                .map(|t| {
                    t.lines()
                        .find(|l| l.contains("  state"))
                        .expect("a state line")
                        .to_owned()
                })
                .collect();
        assert_eq!(lines.len(), 5, "five rungs, five distinct lines");
    }

    #[test]
    fn human_mirror_carries_the_four_sections_and_no_key_names() {
        let text = render_human(
            &synthetic_probe(),
            Glance {
                git: true,
                workflows: 2,
                agents_md: false,
                complete: true,
            },
            None,
            counts(),
            plain(),
        );
        for needle in [
            "Intent as Code",
            "this machine",
            "this binary",
            "start here",
            "hash-chained",
            "cursor ✓",
            "vscode ✗",
            "nika wire",
            // P0-21: the raw key ratio is REPLACED by the adoption rung —
            // this synthetic machine sits at KeyPresent (anthropic keyed,
            // 1 of 2 clouds), with that rung's own metric and CTA.
            "state      key present · 1 of 2 clouds configured",
            "ready for a real run",
            "not briefed → nika init",
            // 2 workflows + unbriefed → the ONE key is the founding wizard
            // (adds only); the stranger's mock/echo line belongs to the
            // 0-workflow state (pinned in start_moves' own test below).
            "brief agents · adds only",
            "learn: nika.sh",
            "github.com/supernovae-st/nika",
        ] {
            assert!(text.contains(needle), "missing `{needle}`:\n{text}");
        }
        // PRESENT-NOT-PRINTED, one step further: welcome never even names
        // the env VARS — that is doctor's fix surface, not the mirror's.
        assert!(
            !text.contains("API_KEY"),
            "welcome must not name key variables:\n{text}"
        );
        // P0-21: the raw ratio row is gone — the state line carries the
        // counts inside its own metric now.
        assert!(
            !text.contains("cloud keys present"),
            "the raw key ratio is replaced by the adoption rung:\n{text}"
        );
        // A workspace that already HAS workflows skips the language taste
        // (progressive disclosure — the sample is the stranger's moment).
        assert!(
            !text.contains("a whole workflow is one file"),
            "2 workflows → no sample:\n{text}"
        );
    }

    /// A probe shaped like the REAL shipped catalog (5 locals · 10
    /// clouds · 4 clients) — the 80-column gate must hold on the widest
    /// TRUE rows, not on a slim synthetic (the first cut passed on a
    /// 1-local probe while the real machine rendered 102 columns).
    fn shipped_shape_probe() -> Probe {
        let local = |id: &str| ProviderProbe {
            id: id.to_owned(),
            requires_key: false,
            key_present: false,
            fix_var: String::new(),
            structured_native: true,
            readiness: readiness(true, ExecutionLocus::Loopback),
            endpoint: "http://127.0.0.1:1".to_owned(),
        };
        let cloud = |id: &str| ProviderProbe {
            id: id.to_owned(),
            requires_key: true,
            key_present: false,
            fix_var: String::new(),
            structured_native: true,
            readiness: readiness(false, ExecutionLocus::Cloud),
            endpoint: format!("https://api.{id}.example/v1"),
        };
        let client = |id: &str| ClientProbe {
            id: id.to_owned(),
            path: String::new(),
            present: false,
            current: false,
            stale: false,
        };
        Probe {
            models: crate::verbs::probe::ModelsProbe::default(),
            version: "0.98.0".to_owned(),
            config_path: None,
            providers: ["ollama", "lmstudio", "llamacpp", "localai", "vllm"]
                .into_iter()
                .map(local)
                .chain(
                    [
                        "mistral",
                        "anthropic",
                        "openai",
                        "gemini",
                        "deepseek",
                        "xai",
                        "groq",
                        "openrouter",
                        "huggingface",
                        "nvidia",
                    ]
                    .into_iter()
                    .map(cloud),
                )
                .collect(),
            clients: ["cursor", "windsurf", "claude", "vscode"]
                .into_iter()
                .map(client)
                .collect(),
            kits: vec![],
            image: ImageProbe::default(),
            tts: TtsProbe::default(),
            local_pings: Vec::new(),
            pricing: PricingProbe::default(),
            retention: crate::verbs::trace::retention::RetentionConfig::default(),
            retention_notes: vec![],
            recorded_runs: 0,
        }
    }

    #[test]
    fn the_stranger_sees_the_language_and_it_fits_eighty_columns() {
        // Zero workflows = the first-contact moment: the mirror SHOWS a
        // whole workflow (the abstract tagline made concrete) — and every
        // line of the whole render stays ≤80 display columns (the one
        // terminal width nobody configures), measured on the REAL
        // catalog shape.
        let text = render_human(
            &shipped_shape_probe(),
            Glance {
                git: false,
                workflows: 0,
                agents_md: false,
                complete: true,
            },
            None,
            EngineCounts {
                builtins: 25,
                locals: 5,
                clouds: 10,
                examples: 28,
                templates: 9,
            },
            plain(),
        );
        assert!(
            text.contains("a whole workflow is one file"),
            "0 workflows → the sample shows:\n{text}"
        );
        assert!(text.contains("nika: v1"), "{text}");
        assert!(text.contains("infer:"), "{text}");
        for line in text.lines() {
            assert!(
                line.chars().count() <= 80,
                "line exceeds 80 cols ({}): `{line}`",
                line.chars().count()
            );
        }
    }

    #[test]
    fn ascii_theme_swaps_every_glyph() {
        // CI logs and legacy terminals get a first-class column: no 🦋,
        // no ✓/✗ — the [nika] mark and +/x, same meaning (colour law:
        // meaning never lives in glyph loss either).
        let text = render_human(
            &synthetic_probe(),
            Glance {
                git: true,
                workflows: 1,
                agents_md: true,
                complete: true,
            },
            None,
            counts(),
            Theme::new(false, true, false),
        );
        assert!(text.contains("[nika]"), "{text}");
        assert!(text.contains("cursor +"), "{text}");
        assert!(text.contains("vscode x"), "{text}");
        for glyph in ['🦋', '✓', '✗'] {
            assert!(
                !text.contains(glyph),
                "unicode {glyph} leaked into --ascii:\n{text}"
            );
        }
    }

    #[test]
    fn the_sample_is_a_real_workflow_that_checks_clean() {
        // The honesty law, applied to marketing: the six lines the
        // stranger reads must BE a checkable workflow, not pseudo-yaml.
        let path = std::env::temp_dir().join(format!(
            "nika-welcome-sample-{}.nika.yaml",
            std::process::id()
        ));
        std::fs::write(&path, format!("{SAMPLE}\n")).expect("sample written");
        let out = crate::verbs::check::run(
            path.to_str().expect("utf8"),
            false,
            false,
            None,
            Theme::new(false, false, false),
        );
        std::fs::remove_file(&path).ok();
        assert_eq!(
            out.code,
            exit::OK,
            "the welcome sample must check clean:\n{}",
            out.text
        );
    }

    #[test]
    fn json_mirror_is_versioned_additive_and_value_free() {
        let raw = render_json(
            &synthetic_probe(),
            Glance {
                git: false,
                workflows: 0,
                agents_md: true,
                complete: true,
            },
            None,
            counts(),
        );
        let v: serde_json::Value = serde_json::from_str(&raw).expect("welcome --json parses");
        assert_eq!(v["welcome_version"], 1);
        assert_eq!(v["machine"]["cloud_keys_present"], 1);
        assert_eq!(v["machine"]["cloud_keys_total"], 2);
        assert_eq!(v["machine"]["clients"][0]["wired"], true);
        assert_eq!(v["workspace"]["workflows"], 0);
        assert_eq!(v["workspace"]["inventory_complete"], true);
        assert_eq!(v["engine"]["verbs"], 4);
        assert_eq!(v["start"].as_array().map(Vec::len), Some(3));
        assert!(
            !raw.contains("API_KEY") && !raw.contains("key_present"),
            "the JSON mirror carries counts, never per-key facts: {raw}"
        );
    }

    /// P0-4 (audit UX 2026-07-30): a TRUNCATED inventory that found zero
    /// files is an UNKNOWN, never « no workflows yet » — the stranger's
    /// claims (the zero line · the language sample · the JSON mirror) are
    /// all gated on a COMPLETE scan, and a partial scan says so instead.
    #[test]
    fn a_partial_scan_never_renders_the_strangers_zero() {
        let g = Glance {
            git: true,
            workflows: 0,
            agents_md: false,
            complete: false,
        };
        let text = render_human(&synthetic_probe(), g, None, counts(), plain());
        assert!(
            !text.contains("no workflows yet"),
            "a partial scan is unknown, never « zero »:\n{text}"
        );
        assert!(
            text.contains("scan partial"),
            "the partial scan says so:\n{text}"
        );
        assert!(
            !text.contains("a whole workflow is one file"),
            "the sample is the COMPLETE stranger's moment only:\n{text}"
        );
        let raw = render_json(&synthetic_probe(), g, None, counts());
        let v: serde_json::Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(
            v["workspace"]["inventory_complete"], false,
            "the machine mirror carries the scan's completeness: {raw}"
        );
    }

    /// The glance itself propagates the walk's truncation: an injected
    /// tiny budget that dies before the workflow's directory yields
    /// `workflows: 0` WITH `complete: false` — the finding's exact
    /// reproduction, now indistinguishable-proof.
    #[test]
    fn glance_marks_a_budget_killed_scan_incomplete() {
        let dir = tempfile::tempdir().expect("scratch");
        for i in 0..8 {
            std::fs::write(dir.path().join(format!("noise-{i}.txt")), "x").expect("write");
        }
        std::fs::create_dir(dir.path().join("z")).expect("mkdir");
        std::fs::write(dir.path().join("z/flow.nika.yaml"), "x").expect("write");
        let (g, sole) = glance(dir.path(), 3); // dies in the noise
        assert_eq!(g.workflows, 0, "the workflow was never reached");
        assert!(!g.complete, "a killed scan is partial, never « zero »");
        assert!(sole.is_none());
        let (g, _) = glance(dir.path(), 4000);
        assert_eq!(g.workflows, 1);
        assert!(g.complete, "a full scan is complete");
    }

    #[test]
    fn glance_counts_workflows_skips_heavy_dirs_and_sees_git() {
        let tmp = std::env::temp_dir().join(format!("nika-welcome-glance-{}", std::process::id()));
        let nested = tmp.join("flows");
        let heavy = tmp.join("node_modules");
        std::fs::create_dir_all(&nested).expect("mkdir");
        std::fs::create_dir_all(&heavy).expect("mkdir");
        std::fs::create_dir_all(tmp.join(".git")).expect("mkdir");
        std::fs::write(tmp.join("a.nika.yaml"), "x").expect("write");
        std::fs::write(nested.join("b.nika.yml"), "x").expect("write");
        std::fs::write(heavy.join("c.nika.yaml"), "x").expect("write");
        std::fs::write(tmp.join("AGENTS.md"), "x").expect("write");
        let (g, sole) = glance(&tmp, 4000);
        std::fs::remove_dir_all(&tmp).ok();
        assert!(g.git, "sees the .git ancestor");
        assert_eq!(g.workflows, 2, "counts a.nika.yaml + flows/b.nika.yml only");
        assert!(g.agents_md);
        assert!(sole.is_none(), "two files → no sole audit target");
    }

    #[test]
    fn welcome_is_always_a_success() {
        // A greeting is never a failure — even on a bare machine the verb
        // routes (doctor owns the gate semantics, welcome never gates).
        let out = run(false, plain());
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("start here"), "{}", out.text);
        let json = run(true, plain());
        assert_eq!(json.code, exit::OK);
        assert!(json.text.contains("\"welcome_version\":1"), "{}", json.text);
    }
}
