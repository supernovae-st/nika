// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika init [dir]` — the composition-root adapter over
//! [`nika_onboard::founding`] (the unit descended at the 15k prod-LOC
//! wall · 2026-07-12 · the `nika-display` precedent).
//!
//! This file owns exactly what a root owns: the door (bare-TTY → the
//! founding wizard · anything scripted → receipts), the two injected
//! effects (the REAL audit ladder = `check::run_scaffold` · the REAL MCP wiring
//! = `wire::run`), and the `VerbOutput` re-wrap. The conversation, the
//! recipes, the briefs and the report shapes all live in the member.

use std::io::IsTerminal;

use clap::ValueEnum as _;

use crate::display::theme::Theme;
use crate::verbs::VerbOutput;
use crate::verbs::wire::WireTarget;

pub use nika_onboard::founding::agents_md;

/// The `--recipe` and `--theme` vocabularies for clap — the member's
/// registers, re-exported (strings cross the seam; the CLI-framework
/// dependency stays at the root).
pub use nika_onboard::founding::{CANVAS_THEMES, CanvasTheme, RECIPE_NAMES};

/// Scaffold `dir` (default `.`). Bare on a terminal (no `--yes`, no
/// recipe/theme/wire flag) the founding wizard runs; anything scripted
/// keeps file receipts with purposes followed by the next commands.
///
/// `project_file` — `--project-file`: the starter `nika.yaml` is laid by
/// default since #1283 (the member's scripted lane owns the receipt); the
/// flag is kept so older scripts keep working, and routes to the scripted lane.
#[must_use]
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)] // the clap surface, unpacked — one struct-shaped seam
pub fn run(
    dir: &str,
    force: bool,
    yes: bool,
    recipe: Option<&str>,
    example: Option<&str>,
    canvas: Option<&str>,
    wires: &[WireTarget],
    project_file: bool,
    theme: Theme,
) -> VerbOutput {
    // The REAL effects, injected (the member never learns what proving
    // or wiring mean — the root does).
    let audit = move |path: &str| {
        let v = crate::verbs::check::run_scaffold(path, theme);
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
        // Both ends a terminal — the only state any nika surface may prompt in.
        || !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal());
    let out = if scripted {
        let wire_names: Vec<String> = wires.iter().copied().map(wire_name).collect();
        let wire_refs: Vec<&str> = wire_names.iter().map(String::as_str).collect();
        nika_onboard::founding::scripted_run(
            dir,
            force,
            recipe,
            example,
            canvas.and_then(CanvasTheme::parse),
            &wire_refs,
            &audit,
            &wire,
        )
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

/// The member speaks client WORDS; the root resolves them onto the real
/// `WireTarget` register (clap already validated the `--wire` flag —
/// this covers the wizard's free-text lane).
fn wire_target(client: &str) -> Option<WireTarget> {
    WireTarget::from_str(client, true).ok()
}

/// Carry the exact clap target through the member's injected wire seam.
/// A second client list must never turn `detected` into the broader `all`.
fn wire_name(target: WireTarget) -> String {
    target
        .to_possible_value()
        .map(|value| value.get_name().to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verbs::exit;

    const PLAIN: Theme = Theme::new(false, false, false);

    #[test]
    fn every_init_wire_target_preserves_the_requested_scope() {
        use clap::ValueEnum as _;
        for target in WireTarget::value_variants() {
            assert_eq!(wire_target(&wire_name(*target)), Some(*target));
        }
        assert_eq!(wire_name(WireTarget::Detected), "detected");
    }

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
            Some("editor"),
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
            tmp.join("nika.yaml").exists(),
            "the project file is laid by default (#1283)"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// A recipe may deliberately contain author slots. Init accepts that
    /// one expected draft class, keeps the receipt honest, and hands over
    /// to editing before check/run. The public check door still refuses it.
    #[test]
    fn scripted_drafts_hand_over_to_edit_before_run() {
        let tmp = std::env::temp_dir().join(format!("nika-init-draft-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let out = run(
            tmp.to_str().expect("utf8"),
            false,
            true,
            Some("agentic"),
            None,
            None,
            &[],
            false,
            PLAIN,
        );
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("not a workflow yet"), "{}", out.text);
        let editor = out.text.find("$EDITOR").expect("editor handoff");
        let check = out.text.find("nika check").expect("check handoff");
        let run = out.text.find("nika run").expect("run handoff");
        assert!(
            editor < check && check < run,
            "edit → check → run: {}",
            out.text
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// The starter `nika.yaml` is laid by default (#1283) with its
    /// receipt in the file report, and respects the skip law; the
    /// `--project-file` flag of older scripts still routes here.
    #[test]
    fn the_project_file_is_laid_by_default_adds_only() {
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
            false,
            PLAIN,
        );
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(
            out.text
                .lines()
                .any(|l| l.starts_with("✔ created ") && l.contains("nika.yaml — team defaults")),
            "the receipt rides the report (joined path, the scripted register): {}",
            out.text
        );
        let laid = std::fs::read_to_string(tmp.join("nika.yaml")).expect("written");
        assert!(
            laid.contains("# nika.yaml — the project file"),
            "the starter, verbatim: {laid}"
        );
        assert!(
            laid.contains("\nnika: my-project\n"),
            "the starter teaches the NAME — the version rides the $schema line: {laid}"
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
