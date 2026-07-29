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
//! no secret value exists in this module by construction. Re-runnable
//! anytime: welcome is a living mirror, not a splash screen.

use std::fmt::Write as _;
use std::path::Path;

use crate::display::theme::{Role, Theme};
use crate::verbs::probe::{Probe, ProviderProbe};
use crate::verbs::{VerbOutput, probe};

/// The next moves, keyed on where this workspace actually IS — the
/// concierge hands over ONE key, not the keyring (row 0 carries the
/// weight; the others stay dim context). Presence-only inputs — the
/// mirror never audits. Comments stay ≤26 chars: the widest command
/// pads to 45 and the whole row must live inside 80 columns.
fn start_moves(glance: Glance) -> [(&'static str, &'static str); 3] {
    match (glance.workflows, glance.agents_md) {
        // The stranger's moment: nothing here yet — see one run, then found.
        (0, _) => [
            (
                "nika examples run 01-hello --model mock/echo",
                "offline proof · zero keys",
            ),
            ("nika init", "found this repo (wizard)"),
            ("nika new", "guided first workflow"),
        ],
        // Workflows live here but the agents were never briefed — the
        // founding wizard skips existing files, so it only ADDS.
        (_, false) => [
            ("nika init", "brief agents · adds only"),
            ("nika run", "your workflow, found"),
            ("nika examples", "the teaching corpus"),
        ],
        // One workflow, fully founded: run it (bare — the lazy door
        // resolves the only workflow and says so).
        (1, true) => [
            ("nika run", "your workflow, found"),
            ("nika check", "audit before running"),
            ("nika examples", "the teaching corpus"),
        ],
        // Several workflows, founded: the whole-workspace lens first.
        (_, true) => [
            ("nika welcome --deep", "the workspace truth"),
            ("nika run <file>", "pick one · check twin"),
            ("nika examples", "the teaching corpus"),
        ],
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
    let glance = glance(Path::new("."));
    let counts = EngineCounts {
        builtins: nika_builtin::tool_defs().len(),
        locals: probe.providers.iter().filter(|p| !p.requires_key).count(),
        clouds: probe.providers.iter().filter(|p| p.requires_key).count(),
        examples: nika_pack::example_slugs().len(),
        templates: nika_pack::template_names().len(),
    };
    if json {
        return VerbOutput::ok(render_json(&probe, glance, counts));
    }
    VerbOutput::ok(render_human(&probe, glance, counts, theme))
}

/// The workspace glance — a bounded, dot-dir-skipping walk (depth ≤ 4 ·
/// ≤ 4000 entries): a greeting must stay instant on a monorepo and must
/// never wander into `node_modules`/`target`.
fn glance(dir: &Path) -> Glance {
    let git = dir
        .canonicalize()
        .unwrap_or_else(|_| dir.to_path_buf())
        .ancestors()
        .any(|a| a.join(".git").exists());
    let mut budget = 4000usize;
    let mut paths = Vec::new();
    probe::collect_workflow_paths(dir, dir, 4, &mut budget, &mut paths);
    let workflows = paths.len();
    Glance {
        git,
        workflows,
        agents_md: dir.join("AGENTS.md").exists(),
    }
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
fn render_human(probe: &Probe, glance: Glance, counts: EngineCounts, theme: Theme) -> String {
    let mut s = String::new();
    identity_section(&mut s, probe, theme);
    machine_section(&mut s, probe, glance, theme);
    binary_section(&mut s, counts, glance, theme);
    start_section(&mut s, theme, glance);
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
    let (present, total) = cloud_key_counts(&probe.providers);
    let _ = writeln!(
        s,
        "  keys       {present}/{total} cloud keys present {}",
        theme.paint(Role::Dim, "· details + fixes → nika doctor"),
    );
    let _ = writeln!(
        s,
        "  workspace  git {} · {} · agents {}",
        mark(theme, glance.git),
        match glance.workflows {
            0 => "no workflows yet".to_owned(),
            1 => "1 workflow".to_owned(),
            n => format!("{n} workflows"),
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
    if glance.workflows == 0 {
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
fn start_section(s: &mut String, theme: Theme, glance: Glance) {
    let _ = writeln!(
        s,
        "{}",
        theme.paint(Role::Strong, "start here (offline · zero keys)")
    );
    let moves = start_moves(glance);
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

/// Cloud keys: how many of the key-requiring providers have one PRESENT
/// (presence is a bool the probe observed — no value exists to leak).
fn cloud_key_counts(providers: &[ProviderProbe]) -> (usize, usize) {
    let clouds: Vec<&ProviderProbe> = providers.iter().filter(|p| p.requires_key).collect();
    let present = clouds.iter().filter(|p| p.key_present).count();
    (present, clouds.len())
}

/// The versioned machine mirror — additive-only (`welcome_version: 1`).
/// Names and booleans and counts, by construction: nothing in the probe
/// carries a value a secret could ride.
fn render_json(probe: &Probe, glance: Glance, counts: EngineCounts) -> String {
    let start: Vec<&str> = start_moves(glance).iter().map(|(cmd, _)| *cmd).collect();
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
    use crate::verbs::probe::{ClientProbe, ImageProbe, PricingProbe, TtsProbe};

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
                },
                ProviderProbe {
                    id: "mistral".to_owned(),
                    requires_key: true,
                    key_present: false,
                    fix_var: "MISTRAL_API_KEY".to_owned(),
                    structured_native: true,
                },
                ProviderProbe {
                    id: "anthropic".to_owned(),
                    requires_key: true,
                    key_present: true,
                    fix_var: "ANTHROPIC_API_KEY".to_owned(),
                    structured_native: true,
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
    /// lead with their own move (presence-only inputs · never an audit).
    #[test]
    fn start_moves_key_on_the_workspace_state() {
        let g = |workflows, agents_md| Glance {
            git: true,
            workflows,
            agents_md,
        };
        assert!(
            start_moves(g(0, false))[0]
                .0
                .contains("examples run 01-hello")
        );
        assert_eq!(start_moves(g(2, false))[0].0, "nika init");
        assert_eq!(start_moves(g(1, true))[0].0, "nika run");
        assert_eq!(start_moves(g(5, true))[0].0, "nika welcome --deep");
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
        };
        let silent = render_human(&synthetic_probe(), glance, counts(), plain());
        assert!(
            !silent.contains("  models"),
            "zero models = zero line:\n{silent}"
        );

        let mut probe = synthetic_probe();
        probe.models.count = 2;
        probe.models.bytes = 211 * 1024 * 1024;
        let shown = render_human(&probe, glance, counts(), plain());
        assert!(
            shown.contains("models     2 pulled · 211.0 MiB on disk"),
            "the sovereign lane is in the mirror:\n{shown}"
        );
        assert!(shown.contains("nika model list"), "{shown}");
    }

    #[test]
    fn human_mirror_carries_the_four_sections_and_no_key_names() {
        let text = render_human(
            &synthetic_probe(),
            Glance {
                git: true,
                workflows: 2,
                agents_md: false,
            },
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
            "1/2 cloud keys present",
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
        };
        let cloud = |id: &str| ProviderProbe {
            id: id.to_owned(),
            requires_key: true,
            key_present: false,
            fix_var: String::new(),
            structured_native: true,
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
            },
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
            },
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
            },
            counts(),
        );
        let v: serde_json::Value = serde_json::from_str(&raw).expect("welcome --json parses");
        assert_eq!(v["welcome_version"], 1);
        assert_eq!(v["machine"]["cloud_keys_present"], 1);
        assert_eq!(v["machine"]["cloud_keys_total"], 2);
        assert_eq!(v["machine"]["clients"][0]["wired"], true);
        assert_eq!(v["workspace"]["workflows"], 0);
        assert_eq!(v["engine"]["verbs"], 4);
        assert_eq!(v["start"].as_array().map(Vec::len), Some(3));
        assert!(
            !raw.contains("API_KEY") && !raw.contains("key_present"),
            "the JSON mirror carries counts, never per-key facts: {raw}"
        );
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
        let g = glance(&tmp);
        std::fs::remove_dir_all(&tmp).ok();
        assert!(g.git, "sees the .git ancestor");
        assert_eq!(g.workflows, 2, "counts a.nika.yaml + flows/b.nika.yml only");
        assert!(g.agents_md);
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
