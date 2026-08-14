// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika init [dir]` — the composition-root adapter over
//! [`nika_onboard::founding`] (the unit descended at the 15k prod-LOC
//! wall · 2026-07-12 · the `nika-display` precedent).
//!
//! This file owns exactly what a root owns: the door (bare-TTY → the
//! founding wizard · anything scripted → receipts), the two injected
//! effects (the REAL audit ladder = `check::run` · the REAL MCP wiring
//! = `wire::run`), and the `VerbOutput` re-wrap. The conversation, the
//! recipes, the briefs and the report shapes all live in the member.

use std::io::IsTerminal;

use crate::display::theme::Theme;
use crate::verbs::VerbOutput;
use crate::verbs::wire::WireTarget;

pub use nika_onboard::founding::agents_md;

/// The `--recipe` vocabulary for clap — the member's register, re-exported.
pub use nika_onboard::founding::RECIPE_NAMES;

/// The `--theme` vocabulary — the member's plain enum mirrored as a clap
/// `ValueEnum` (the CLI-framework dependency stays at the root).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CanvasTheme {
    /// The brand skin — engineered black · verb hues.
    Nika,
    /// Adaptive — follows the editor's colors.
    Editor,
    /// Terminal green.
    Phosphor,
    /// Let the extension decide.
    Auto,
}

impl From<CanvasTheme> for nika_onboard::founding::CanvasTheme {
    fn from(c: CanvasTheme) -> Self {
        match c {
            CanvasTheme::Nika => Self::Nika,
            CanvasTheme::Editor => Self::Editor,
            CanvasTheme::Phosphor => Self::Phosphor,
            CanvasTheme::Auto => Self::Auto,
        }
    }
}

/// Scaffold `dir` (default `.`). Bare on a terminal (no `--yes`, no
/// recipe/theme/wire flag) the founding wizard runs; anything scripted
/// keeps receipts-plus-hand-off — and with ZERO new flags the output is
/// the historical bytes exactly.
///
/// `project_file` — `--project-file` (D-2026-08-11-N5): the scripted
/// twin of the wizard's offer question. It routes to the scripted lane
/// (a flag is a script), and lays the starter AFTER the scaffold with
/// its receipt appended — never silently, never without the flag.
#[must_use]
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)] // the clap surface, unpacked — one struct-shaped seam
pub fn run(
    dir: &str,
    force: bool,
    yes: bool,
    recipe: Option<&str>,
    example: Option<&str>,
    canvas: Option<CanvasTheme>,
    wires: &[WireTarget],
    project_file: bool,
    theme: Theme,
) -> VerbOutput {
    // The REAL effects, injected (the member never learns what proving
    // or wiring mean — the root does).
    let audit = move |path: &str| {
        let v = crate::verbs::check::run(path, false, false, None, theme);
        nika_onboard::Outcome {
            text: v.text,
            code: v.code,
        }
    };
    let wire = |client: &str, dir: &str| match wire_target(client) {
        Some(target) => {
            let v = crate::verbs::wire::run(target, dir);
            nika_onboard::Outcome {
                text: v.text,
                code: v.code,
            }
        }
        None => nika_onboard::Outcome::env(format!("unknown wire client `{client}`")),
    };

    let scripted = yes
        || recipe.is_some()
        || example.is_some()
        || canvas.is_some()
        || !wires.is_empty()
        || project_file
        || !interactive();
    let out = if scripted {
        let wire_names: Vec<&str> = wires.iter().copied().map(wire_name).collect();
        let out = nika_onboard::founding::scripted_run(
            dir,
            force,
            recipe,
            example,
            canvas.map(Into::into),
            &wire_names,
            &audit,
            &wire,
        );
        // The scripted twin of the wizard's offer — the flag is the
        // ONLY scripted door (never laid silently), and the receipt
        // appends to the same report (the adds-only law · one law,
        // two doors).
        if project_file {
            let (path, outcome) = nika_onboard::project_file::ensure(dir, force);
            let failed = matches!(outcome, nika_onboard::project_file::Outcome::Failed(_));
            let (_mark, msg) = nika_onboard::project_file::report(&path, &outcome);
            nika_onboard::Outcome {
                text: format!("{}\n{msg}", out.text),
                code: if failed {
                    nika_onboard::codes::ENV
                } else {
                    out.code
                },
            }
        } else {
            out
        }
    } else {
        let stdin = std::io::stdin();
        nika_onboard::wizard::wizard_io(
            dir,
            force,
            theme,
            &mut stdin.lock(),
            &mut std::io::stdout(),
            &audit,
            &wire,
        )
    };
    VerbOutput {
        text: out.text,
        code: out.code,
    }
}

/// Both ends of the conversation are a terminal — the only state in
/// which any nika surface may prompt (clig.dev interactivity rule).
fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// The member speaks client WORDS; the root resolves them onto the real
/// `WireTarget` register (clap already validated the `--wire` flag —
/// this covers the wizard's free-text lane).
fn wire_target(client: &str) -> Option<WireTarget> {
    use clap::ValueEnum as _;
    WireTarget::from_str(client, true).ok()
}

/// A `WireTarget` back to its wire word (for the member's receipts) —
/// interned through the static register so the member sees stable words.
fn wire_name(target: WireTarget) -> &'static str {
    use clap::ValueEnum as _;
    target
        .to_possible_value()
        .map_or("all", |v| wire_static(v.get_name()))
}

/// Intern a clap possible-value name onto the static register.
fn wire_static(name: &str) -> &'static str {
    const NAMES: [&str; 22] = [
        "cursor",
        "vscode",
        "windsurf",
        "claude",
        "claude-desktop",
        "cline",
        "codex",
        "continue",
        "zed",
        "opencode",
        "hermes",
        "gemini",
        "qwen",
        "lmstudio",
        "junie",
        "grok",
        "antigravity",
        "kimi",
        "kiro",
        "copilot",
        "amp",
        "all",
    ];
    NAMES.iter().find(|n| **n == name).copied().unwrap_or("all")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::exit;

    const PLAIN: Theme = Theme::new(false, false, false);

    /// The root adapter keeps the composed behavior the smoke tests pin:
    /// `--yes` + `--recipe` scaffolds, audits through the REAL ladder,
    /// and hands over — the injection seam changes nothing observable.
    #[test]
    fn scripted_recipe_runs_the_real_ladder_through_the_seam() {
        let tmp = std::env::temp_dir().join(format!("nika-init-adapter-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            Some("ship"),
            None,
            Some(CanvasTheme::Editor),
            &[],
            false,
            PLAIN,
        );
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text.matches("audited").count() >= 2,
            "the REAL check ladder spoke through the injected seam: {}",
            out.text
        );
        let settings = std::fs::read_to_string(tmp.join(".vscode/settings.json")).expect("written");
        assert!(settings.contains("\"nika.dag.theme\": \"editor\""));
        assert!(
            !tmp.join("nika.yaml").exists(),
            "no flag, no project file — never laid silently"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// `--project-file` (D-2026-08-11-N5): the ONE scripted door — it
    /// routes to the scripted lane, lays the starter after the
    /// scaffold with its receipt appended, and respects the skip law.
    #[test]
    fn the_project_file_flag_is_the_only_scripted_door() {
        let tmp = std::env::temp_dir().join(format!("nika-init-projfile-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            Some("ship"),
            None,
            None,
            &[],
            true,
            PLAIN,
        );
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text
                .lines()
                .any(|l| l.contains("created") && l.ends_with("nika.yaml")),
            "the receipt rides the report (joined path, the scripted register): {}",
            out.text
        );
        let laid = std::fs::read_to_string(tmp.join("nika.yaml")).expect("written");
        assert!(
            laid.starts_with("# nika.yaml — the project file"),
            "the starter, verbatim: {laid}"
        );

        // A second run: the skip law (adds-only) — the receipt says so.
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            Some("ship"),
            None,
            None,
            &[],
            true,
            PLAIN,
        );
        assert!(
            out.text.contains("skipped") && out.text.contains("(exists · --force)"),
            "existing = skip: {}",
            out.text
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// Every wizard wire word resolves onto the real register (the
    /// member's menu and the root's enum cannot drift apart).
    #[test]
    fn wizard_wire_words_resolve_on_the_register() {
        for word in ["cursor", "vscode", "claude", "codex", "zed", "all"] {
            assert!(wire_target(word).is_some(), "{word} resolves");
        }
        assert!(wire_target("notepad").is_none());
    }
}
