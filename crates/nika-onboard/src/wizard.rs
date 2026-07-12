// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The founding wizard — bare `nika init` on a terminal.
//!
//! The clack-school rail over the SAME injected-io discipline as the
//! `new` wizard (any `BufRead` in · any `Write` out · tests drive it
//! with a cursor, no PTY needed). Every question is asked BEFORE the
//! first byte is written, so cancel is always the honest « nothing
//! written ». Flags are the scriptable twin: any of `--recipe` /
//! `--theme` / `--wire` routes to the non-interactive path instead
//! (the gh-repo-create shape — bare TTY converses, flags script).

use std::io::BufRead;
use std::path::Path;

use nika_display::chrome;
use nika_display::theme::{Role, Theme};

use crate::founding::{BriefOutcome, CanvasTheme, apply_briefs, proof_receipts, wire_receipts};
use crate::guided;
use crate::recipes::{RECIPES, Recipe, ScaffoldStatus, scaffold, takes_model};
use crate::{Audit, Outcome, Wire, codes};

/// The one tagline the banner speaks (the same claim every teaching
/// surface carries).
const TAGLINE: &str = "the workflow language for AI — audited before it runs";

/// The wire menu the agents step offers — the popular five, then `all`
/// (the full 15-client register stays one `nika wire all` away). Plain
/// client words: the injected [`Wire`] resolves them at the root.
const WIRE_MENU: [(&str, &str); 5] = [
    ("cursor", "~/.cursor/mcp.json"),
    ("vscode", ".vscode/mcp.json"),
    ("claude", "~/.claude.json"),
    ("codex", "~/.codex/config.toml"),
    ("zed", "~/.config/zed/settings.json"),
];

/// The canvas menu — `nika.dag.theme`'s own enum, spoken in wizard rows.
const CANVAS_MENU: [(CanvasTheme, &str); 4] = [
    (
        CanvasTheme::Nika,
        "the brand skin — engineered black · verb hues",
    ),
    (CanvasTheme::Editor, "adaptive — follows your editor colors"),
    (CanvasTheme::Phosphor, "terminal green"),
    (CanvasTheme::Auto, "let the extension decide"),
];

/// Everything the conversation harvests before a single write.
struct Choices {
    recipe: &'static Recipe,
    model: Option<String>,
    canvas: Option<CanvasTheme>,
    wires: Vec<&'static str>,
}

/// The wizard over injected io: converse (no writes) · then found the
/// project (briefs → workflows → wires → proof) · then the ready panel.
pub fn wizard_io(
    dir: &str,
    force: bool,
    theme: Theme,
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    audit: &Audit<'_>,
    wire: &Wire<'_>,
) -> Outcome {
    match converse(dir, theme, input, out) {
        Err(e) => Outcome {
            text: format!("wizard i/o failed: {e}"),
            code: codes::ENV,
        },
        Ok(None) => Outcome {
            text: "cancelled — nothing written".to_owned(),
            code: codes::ENV,
        },
        Ok(Some(choices)) => found(dir, force, theme, &choices, input, out, (audit, wire)),
    }
}

/// The question phase — banner, then recipe · model · canvas · agents.
/// `Ok(None)` = EOF anywhere (the human left — cancel, never loop).
fn converse(
    dir: &str,
    theme: Theme,
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
) -> std::io::Result<Option<Choices>> {
    for line in chrome::banner(theme, "nika · init", TAGLINE, env!("CARGO_PKG_VERSION")) {
        writeln!(out, "{line}")?;
    }
    writeln!(
        out,
        "{}",
        chrome::rail_line(
            theme,
            &format!(
                "project {} {}",
                theme.paint(Role::Strong, &project_name(dir)),
                theme.paint(Role::Dim, "— Enter accepts a default · Ctrl-C exits"),
            )
        )
    )?;

    // ── recipe ──
    writeln!(out, "{}", chrome::rail_head(theme, "recipe"))?;
    for (i, r) in RECIPES.iter().enumerate() {
        writeln!(
            out,
            "{}",
            chrome::rail_pick(theme, i + 1, r.name, r.tagline)
        )?;
    }
    let Some(pick) = guided::ask(
        input,
        out,
        theme,
        &format!("a number {}", theme.paint(Role::Dim, "[1 agentic]")),
    )?
    else {
        return Ok(None);
    };
    let recipe = resolve_recipe(&pick);
    writeln!(
        out,
        "{}",
        chrome::rail_line(
            theme,
            &format!(
                "→ {} {}",
                theme.paint(Role::Accent, recipe.name),
                theme.paint(Role::Dim, &format!("— {}", recipe.tagline)),
            )
        )
    )?;

    // ── model (only when a skeleton in the set takes one) ──
    let model = if takes_model(recipe) {
        match guided::ask_model(input, out, theme)? {
            Some(m) => Some(m),
            None => return Ok(None),
        }
    } else {
        None
    };

    let Some(canvas) = ask_canvas(input, out, theme)? else {
        return Ok(None);
    };
    let Some(wires) = ask_wires(input, out, theme)? else {
        return Ok(None);
    };

    Ok(Some(Choices {
        recipe,
        model,
        canvas,
        wires,
    }))
}

/// The canvas beat — the VS Code DAG skin, a real persisted knob.
/// Outer `None` = EOF (cancelled) · inner `None` = skipped (an offer,
/// never a toll).
#[allow(clippy::option_option)] // cancel ≠ skip — both honest states
fn ask_canvas(
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    theme: Theme,
) -> std::io::Result<Option<Option<CanvasTheme>>> {
    writeln!(
        out,
        "{}",
        chrome::rail_head(
            theme,
            "canvas — the VS Code DAG theme (.vscode/settings.json)"
        )
    )?;
    for (i, (c, note)) in CANVAS_MENU.iter().enumerate() {
        writeln!(out, "{}", chrome::rail_pick(theme, i + 1, c.as_str(), note))?;
    }
    let Some(pick) = guided::ask(
        input,
        out,
        theme,
        &format!(
            "a number, or Enter to skip {}",
            theme.paint(Role::Dim, "[skip]")
        ),
    )?
    else {
        return Ok(None);
    };
    Ok(Some(resolve_canvas(&pick)))
}

/// The agents beat — wire the MCP oracle into the picked clients.
fn ask_wires(
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    theme: Theme,
) -> std::io::Result<Option<Vec<&'static str>>> {
    writeln!(
        out,
        "{}",
        chrome::rail_head(theme, "agents — wire the MCP oracle now?")
    )?;
    for (i, (label, path)) in WIRE_MENU.iter().enumerate() {
        writeln!(out, "{}", chrome::rail_pick(theme, i + 1, label, path))?;
    }
    writeln!(
        out,
        "{}",
        chrome::rail_pick(theme, WIRE_MENU.len() + 1, "all", "every supported client")
    )?;
    let Some(pick) = guided::ask(
        input,
        out,
        theme,
        &format!(
            "numbers (`1 3`), `all`, or Enter to skip {}",
            theme.paint(Role::Dim, "[skip]")
        ),
    )?
    else {
        return Ok(None);
    };
    Ok(Some(resolve_wires(&pick)))
}

/// The write phase — briefs · workflows · wires · proof · panel.
fn found(
    dir: &str,
    force: bool,
    theme: Theme,
    choices: &Choices,
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    (audit, wire): (&Audit<'_>, &Wire<'_>),
) -> Outcome {
    writeln!(out, "{}", chrome::rail_head(theme, "scaffold")).ok();
    let briefs = apply_briefs(dir, force, choices.canvas);
    let mut failed = false;
    for (path, outcome) in &briefs {
        let rel = relative(dir, path);
        let line = match outcome {
            BriefOutcome::Created => brief_line(theme, '✔', &format!("created {rel}")),
            BriefOutcome::Skipped => {
                brief_line(theme, '·', &format!("skipped {rel} (exists · --force)"))
            }
            BriefOutcome::Failed(e) => {
                failed = true;
                brief_line(theme, '✖', &format!("{rel}: {e}"))
            }
        };
        writeln!(out, "{line}").ok();
    }
    if failed {
        return Outcome::env("scaffold failed — see the report above".to_owned());
    }

    let scaffolded = scaffold(dir, choices.recipe, choices.model.as_deref(), force);
    for (path, status) in &scaffolded {
        let rel = relative(dir, path);
        let line = match status {
            ScaffoldStatus::Created => brief_line(theme, '✔', &format!("created {rel}")),
            ScaffoldStatus::Skipped => {
                brief_line(theme, '·', &format!("skipped {rel} (exists · --force)"))
            }
            ScaffoldStatus::Failed(e) => brief_line(theme, '✖', &format!("{rel}: {e}")),
        };
        writeln!(out, "{line}").ok();
    }
    if scaffolded
        .iter()
        .any(|(_, s)| matches!(s, ScaffoldStatus::Failed(_)))
    {
        return Outcome::env("scaffold failed — see the report above".to_owned());
    }

    for line in wire_receipts(dir, &choices.wires, wire) {
        writeln!(out, "{line}").ok();
    }

    // starter hands over to the guided three-question flow — its
    // materialize step already audits and teaches; the panel would
    // double the hand-off.
    if choices.recipe.name == "starter" {
        writeln!(out).ok();
        return guided::wizard_io(dir, None, force, theme, input, out, audit);
    }

    // ── proof: the audit ladder INSIDE the first minute ──
    let created: Vec<&str> = scaffolded
        .iter()
        .filter(|(p, s)| *s == ScaffoldStatus::Created && p.ends_with(".nika.yaml"))
        .map(|(p, _)| p.as_str())
        .collect();
    let mut worst = codes::OK;
    if !created.is_empty() {
        writeln!(
            out,
            "{}",
            chrome::rail_head(theme, "proof — audit before a single token")
        )
        .ok();
        for (line, code) in proof_receipts(dir, &created, audit) {
            worst = worst.max(code);
            writeln!(out, "{line}").ok();
        }
    }

    Outcome {
        text: ready_panel(dir, theme, choices, &created),
        code: worst,
    }
}

/// One themed report line — the wizard register only (the `--yes` /
/// piped report keeps its exact historical bytes in `init::run`).
fn brief_line(theme: Theme, mark: char, text: &str) -> String {
    let role = match mark {
        '✔' => Role::Good,
        '✖' => Role::Bad,
        _ => Role::Dim,
    };
    let glyph = if theme.ascii {
        match mark {
            '✔' => "ok",
            '✖' => "X ",
            _ => ". ",
        }
        .to_owned()
    } else {
        format!("{mark} ")
    };
    chrome::rail_line(theme, &format!("{}{}", theme.paint(role, &glyph), text))
}

/// The ready panel — created set · next moves · the docs pointer.
fn ready_panel(dir: &str, theme: Theme, choices: &Choices, created: &[&str]) -> String {
    let first = created
        .first()
        .map_or_else(|| "my-first.nika.yaml".to_owned(), |p| relative(dir, p));
    let run_hint = choices.model.as_deref().map_or_else(
        || format!("nika run {first} --model mock/echo"),
        |m| format!("nika run {first} --model {m}"),
    );
    let mut lines: Vec<(String, Role)> = vec![
        (
            format!(
                "{} workflows · briefs for every agent client{}",
                created.len(),
                choices
                    .canvas
                    .map_or_else(String::new, |c| format!(" · canvas {}", c.as_str())),
            ),
            Role::Dim,
        ),
        (String::new(), Role::Dim),
        (run_hint, Role::Strong),
        (format!("nika check {first}"), Role::Strong),
        (
            "nika explain <NIKA-XXXX>   # every finding teaches".to_owned(),
            Role::Dim,
        ),
        (String::new(), Role::Dim),
        (
            format!(
                "docs · {}",
                theme.link("https://docs.nika.sh", "docs.nika.sh")
            ),
            Role::Dim,
        ),
    ];
    if !choices.wires.is_empty() {
        lines.insert(
            1,
            (
                format!(
                    "{} agent client(s) wired to the oracle",
                    choices.wires.len()
                ),
                Role::Dim,
            ),
        );
    }
    let title = format!("{} ready — {} is agentic", theme.logo(), project_name(dir));
    chrome::panel(theme, &title, &lines).join("\n")
}

/// The project's display name — the dir's basename (`.` resolves to the
/// current directory's).
fn project_name(dir: &str) -> String {
    let path = Path::new(dir);
    let resolved = if dir == "." {
        std::env::current_dir().unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    resolved
        .file_name()
        .map_or_else(|| dir.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// A path relative to the project dir when it nests there (receipts
/// speak project-relative · absolute stays honest for elsewhere).
fn relative(dir: &str, path: &str) -> String {
    Path::new(path)
        .strip_prefix(dir)
        .map_or_else(|_| path.to_owned(), |p| p.to_string_lossy().into_owned())
}

/// A menu answer → the recipe (Enter = the agentic curriculum · a name
/// works too · anything else falls back to the default, never a dead
/// end after the human answered).
fn resolve_recipe(pick: &str) -> &'static Recipe {
    if let Ok(n) = pick.parse::<usize>()
        && let Some(r) = n.checked_sub(1).and_then(|i| RECIPES.get(i))
    {
        return r;
    }
    super::recipes::recipe(pick).unwrap_or(&RECIPES[0])
}

/// A menu answer → the canvas theme (Enter or anything unrecognized =
/// skip — a theme is an offer, never a toll).
fn resolve_canvas(pick: &str) -> Option<CanvasTheme> {
    if let Ok(n) = pick.parse::<usize>() {
        return n
            .checked_sub(1)
            .and_then(|i| CANVAS_MENU.get(i))
            .map(|(c, _)| *c);
    }
    CANVAS_MENU
        .iter()
        .find(|(c, _)| c.as_str() == pick)
        .map(|(c, _)| *c)
}

/// A menu answer → wire targets (`1 3` · `all` · Enter/other = none).
fn resolve_wires(pick: &str) -> Vec<&'static str> {
    if pick.eq_ignore_ascii_case("all") {
        return vec!["all"];
    }
    let mut targets: Vec<&'static str> = pick
        .split_whitespace()
        .filter_map(|w| w.parse::<usize>().ok())
        .filter_map(|n| {
            if n == WIRE_MENU.len() + 1 {
                return Some("all");
            }
            n.checked_sub(1)
                .and_then(|i| WIRE_MENU.get(i))
                .map(|(name, _)| *name)
        })
        .collect();
    targets.dedup();
    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Theme = Theme::new(false, false, false);

    /// Test stubs for the injected effects — the SHAPES, not the
    /// ladders (the real check/wire integrations live at the
    /// composition root and in the recipes own-corpus ratchet).
    fn stub_audit(path: &str) -> Outcome {
        Outcome::ok(format!("  ✔ audited (stub) ← {path}"))
    }
    fn stub_wire(client: &str, _dir: &str) -> Outcome {
        Outcome::ok(format!("{client}: wired (stub)"))
    }

    fn fresh_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-initwiz-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    /// Two Enters + skip + skip = the golden path: agentic curriculum ·
    /// offline mock · no canvas stamp · no wires — briefs + 4 workflows
    /// on disk, every one audited, the panel handed back.
    #[test]
    fn golden_path_founds_the_agentic_project() {
        let dir = fresh_dir("golden");
        let d = dir.to_str().expect("utf8");
        // recipe Enter (agentic) · model Enter (mock) · canvas Enter
        // (skip) · agents Enter (skip)
        let mut input = std::io::Cursor::new(b"\n\n\n\n".to_vec());
        let mut out = Vec::new();
        let v = wizard_io(
            d,
            false,
            PLAIN,
            &mut input,
            &mut out,
            &stub_audit,
            &stub_wire,
        );
        assert_eq!(v.code, codes::OK, "{}", v.text);
        let shown = String::from_utf8(out).expect("utf8");
        assert!(shown.contains("recipe"), "{shown}");
        assert!(shown.contains("agentic"), "{shown}");
        assert!(shown.contains("created AGENTS.md"), "{shown}");
        assert!(
            shown.contains("created workflows/01-hello-chain.nika.yaml"),
            "{shown}"
        );
        assert!(shown.contains("proof"), "the audit runs: {shown}");
        assert!(v.text.contains("ready"), "the panel: {}", v.text);
        assert!(
            v.text
                .contains("nika run workflows/01-hello-chain.nika.yaml")
        );
        for rel in [
            "AGENTS.md",
            "workflows/01-hello-chain.nika.yaml",
            "workflows/04-agent-loop.nika.yaml",
        ] {
            assert!(dir.join(rel).exists(), "{rel} written");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// EOF before the first write = nothing on disk, the honest cancel.
    #[test]
    fn cancel_before_write_leaves_nothing() {
        let dir = fresh_dir("cancel");
        let d = dir.to_str().expect("utf8");
        let mut input = std::io::Cursor::new(Vec::new());
        let mut out = Vec::new();
        let v = wizard_io(
            d,
            false,
            PLAIN,
            &mut input,
            &mut out,
            &stub_audit,
            &stub_wire,
        );
        assert_eq!(v.code, codes::ENV);
        assert!(v.text.contains("nothing written"), "{}", v.text);
        assert!(!dir.join("AGENTS.md").exists(), "no partial scaffold");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Picking the canvas stamps `nika.dag.theme` into the settings the
    /// SAME write creates — one file, one truth.
    #[test]
    fn canvas_pick_stamps_the_dag_theme() {
        let dir = fresh_dir("canvas");
        let d = dir.to_str().expect("utf8");
        // recipe 5 (minimal) · canvas 3 (phosphor) · agents skip
        let mut input = std::io::Cursor::new(b"5\n3\n\n".to_vec());
        let mut out = Vec::new();
        let v = wizard_io(
            d,
            false,
            PLAIN,
            &mut input,
            &mut out,
            &stub_audit,
            &stub_wire,
        );
        assert_eq!(v.code, codes::OK, "{}", v.text);
        let settings = std::fs::read_to_string(dir.join(".vscode/settings.json")).expect("written");
        let parsed: serde_json::Value = serde_json::from_str(&settings).expect("valid json");
        assert_eq!(
            parsed.get("nika.dag.theme").and_then(|v| v.as_str()),
            Some("phosphor")
        );
        assert!(
            parsed.get("yaml.schemas").is_some(),
            "schema wiring survives the stamp"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolvers_cover_the_menus() {
        assert_eq!(resolve_recipe("").name, "agentic", "Enter = the default");
        assert_eq!(resolve_recipe("3").name, "ship");
        assert_eq!(resolve_recipe("ship").name, "ship", "names work too");
        assert_eq!(resolve_recipe("99").name, "agentic", "off-menu falls back");

        assert_eq!(resolve_canvas(""), None, "Enter skips");
        assert_eq!(resolve_canvas("1"), Some(CanvasTheme::Nika));
        assert_eq!(resolve_canvas("editor"), Some(CanvasTheme::Editor));
        assert_eq!(resolve_canvas("7"), None);

        assert!(resolve_wires("").is_empty());
        assert_eq!(resolve_wires("all"), vec!["all"]);
        assert_eq!(resolve_wires("1 3"), vec!["cursor", "claude"]);
        assert_eq!(resolve_wires("6"), vec!["all"]);
    }

    /// The starter recipe hands over to the shared three-question flow —
    /// one io script drives both wizards end-to-end.
    #[test]
    fn starter_hands_over_to_the_guided_flow() {
        let dir = fresh_dir("starter");
        let d = dir.to_str().expect("utf8");
        // recipe 2 (starter) · canvas skip · agents skip · then the new
        // wizard: intent Enter (chain) · file Enter · model Enter (mock)
        let mut input = std::io::Cursor::new(b"2\n\n\n\n\n\n".to_vec());
        let mut out = Vec::new();
        let v = wizard_io(
            d,
            false,
            PLAIN,
            &mut input,
            &mut out,
            &stub_audit,
            &stub_wire,
        );
        assert_eq!(v.code, codes::OK, "{}", v.text);
        let shown = String::from_utf8(out).expect("utf8");
        assert!(
            shown.contains("your first workflow"),
            "the guided flow ran: {shown}"
        );
        assert!(
            dir.join("my-first.nika.yaml").exists(),
            "the guided file landed"
        );
        assert!(v.text.contains("audited"), "its audit spoke: {}", v.text);
        std::fs::remove_dir_all(&dir).ok();
    }
}
