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
use crate::gitignore;
use crate::guided;
use crate::project_file;
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
    /// `Some(slug)` = the example lane — ONE embedded example founds the
    /// project (verbatim · no model question: a lesson carries its own).
    example: Option<String>,
    model: Option<String>,
    canvas: Option<CanvasTheme>,
    wires: Vec<&'static str>,
    /// `true` = the human took the project-file offer (D-2026-08-11-N5)
    /// — lay the starter `nika.yaml` at founding.
    project_file: bool,
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

    // ── what are you building? (recipes + the example lane) ──
    writeln!(out, "{}", chrome::rail_head(theme, "recipe"))?;
    for (i, r) in RECIPES.iter().enumerate() {
        writeln!(
            out,
            "{}",
            chrome::rail_pick(theme, i + 1, r.name, r.tagline)
        )?;
    }
    writeln!(
        out,
        "{}",
        chrome::rail_pick(
            theme,
            RECIPES.len() + 1,
            "example",
            "start from one example — any slug from bare `nika try`",
        )
    )?;
    let Some((recipe, example)) = ask_blueprint(input, out, theme)? else {
        return Ok(None);
    };

    // ── model (only when a recipe skeleton takes one — an example never
    // asks: the lesson already carries its own `model:` line) ──
    let model = if example.is_none() && takes_model(recipe) {
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
    let Some(project_file) = ask_project_file(input, out, theme)? else {
        return Ok(None);
    };

    Ok(Some(Choices {
        recipe,
        example,
        model,
        canvas,
        wires,
        project_file,
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
    let Some(canvas) = guided::ask_validated(
        input,
        out,
        theme,
        &format!(
            "a number, or Enter to skip {}",
            theme.paint(Role::Dim, "[skip]")
        ),
        &format!("choose 1-{}", CANVAS_MENU.len()),
        |raw| resolve_canvas(raw).map(Some),
        || None,
    )?
    else {
        return Ok(None);
    };
    Ok(Some(canvas))
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
    let Some(wires) = guided::ask_validated(
        input,
        out,
        theme,
        &format!(
            "numbers (`1 3`), `all`, or Enter to skip {}",
            theme.paint(Role::Dim, "[skip]")
        ),
        &format!("choose numbers in 1-{} or `all`", WIRE_MENU.len() + 1),
        |raw| {
            let targets = resolve_wires(raw);
            if targets.is_empty() {
                None
            } else {
                Some(targets)
            }
        },
        Vec::new,
    )?
    else {
        return Ok(None);
    };
    Ok(Some(wires))
}

/// The project-file beat (D-2026-08-11-N5) — the ONE offer question:
/// `y` lays a starter `nika.yaml`, Enter skips (the file is optional;
/// absence IS the defaults — an offer, never a toll). A prose answer
/// is said and re-asked (the P0-8 loop discipline, every menu beat).
fn ask_project_file(
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    theme: Theme,
) -> std::io::Result<Option<bool>> {
    writeln!(
        out,
        "{}",
        chrome::rail_head(theme, "project file — lay a starter nika.yaml?")
    )?;
    guided::ask_validated(
        input,
        out,
        theme,
        &format!(
            "`y` to lay it, or Enter to skip {}",
            theme.paint(Role::Dim, "[skip]")
        ),
        "`y` lays it · Enter skips",
        |raw| {
            matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "y" | "yes" | "o" | "oui"
            )
            .then_some(true)
        },
        || false,
    )
}

/// The briefs rows of the write phase — the adds-only report, and
/// `true` when any write FAILED (the caller folds it into its early
/// exit). Extracted under the 100-line fn law (ADR-023).
fn report_briefs(
    dir: &str,
    theme: Theme,
    briefs: &[(String, BriefOutcome)],
    out: &mut dyn std::io::Write,
) -> bool {
    let mut failed = false;
    for (path, outcome) in briefs {
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
    failed
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
    let mut failed = report_briefs(dir, theme, &briefs, out);
    // The trace cover — the same adds-only guarantee the scripted twin
    // gives (one law, two doors · `gitignore.rs`).
    let (git_path, git_outcome) = gitignore::ensure(dir);
    failed |= matches!(git_outcome, gitignore::Outcome::Failed(_));
    let (mark, msg) = gitignore::report(&relative(dir, &git_path), &git_outcome);
    writeln!(out, "{}", brief_line(theme, mark, &msg)).ok();
    // The starter `nika.yaml` — ONLY when the offer was taken, riding
    // the same adds-only report (D-2026-08-11-N5 · the gitignore row's
    // sibling: existing = skip · --force overrides).
    if choices.project_file {
        failed |= lay_project_file(dir, force, theme, out);
    }
    if failed {
        return Outcome::env("scaffold failed — see the report above".to_owned());
    }

    let scaffolded = match &choices.example {
        Some(slug) => super::recipes::scaffold_example(dir, slug, force),
        None => scaffold(dir, choices.recipe, choices.model.as_deref(), force),
    };
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
    if choices.example.is_none() && choices.recipe.name == "starter" {
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

/// The starter `nika.yaml` row — laid ONLY when the offer was taken,
/// riding the same adds-only report as the gitignore cover
/// (D-2026-08-11-N5). Returns `true` when the write FAILED (init's
/// one environment error), so `found` folds it into its early exit
/// (extracted under the 100-line fn law).
fn lay_project_file(dir: &str, force: bool, theme: Theme, out: &mut dyn std::io::Write) -> bool {
    let (pf_path, pf_outcome) = project_file::ensure(dir, force);
    let failed = matches!(pf_outcome, project_file::Outcome::Failed(_));
    let (mark, msg) = project_file::report(&relative(dir, &pf_path), &pf_outcome);
    writeln!(out, "{}", brief_line(theme, mark, &msg)).ok();
    failed
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

/// What the blueprint answer resolves to — a recipe, or the example
/// lane (whose slug follow-up runs after the pick).
enum BlueprintPick {
    Recipe(&'static Recipe),
    ExampleLane,
}

/// The blueprint beat — the pick answer, the example-slug follow-up
/// when lane 6 is chosen, and the confirmation rail line. `None` = EOF.
/// Enter = the agentic curriculum (the announced default) · a NON-EMPTY
/// answer that is neither a menu number nor a recipe name is SAID and
/// re-asked (P0-8 — « débutant » or « 99 » must never become agentic
/// silently).
#[allow(clippy::type_complexity)] // (recipe, example) IS the harvest shape
fn ask_blueprint(
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    theme: Theme,
) -> std::io::Result<Option<(&'static Recipe, Option<String>)>> {
    let Some(pick) = guided::ask_validated(
        input,
        out,
        theme,
        &format!("a number {}", theme.paint(Role::Dim, "[1 agentic]")),
        &format!("choose 1-{} or a recipe name", RECIPES.len() + 1),
        |raw| {
            if is_example_pick(raw) {
                Some(BlueprintPick::ExampleLane)
            } else {
                resolve_recipe(raw).map(BlueprintPick::Recipe)
            }
        },
        || BlueprintPick::Recipe(&RECIPES[0]),
    )?
    else {
        return Ok(None);
    };
    let (recipe, example) = match pick {
        BlueprintPick::Recipe(recipe) => (recipe, None),
        BlueprintPick::ExampleLane => {
            let Some(slug) = ask_example_slug(input, out, theme)? else {
                return Ok(None);
            };
            (&RECIPES[0], Some(slug))
        }
    };
    let picked_line = example.as_ref().map_or_else(
        || {
            format!(
                "→ {} {}",
                theme.paint(Role::Accent, recipe.name),
                theme.paint(Role::Dim, &format!("— {}", recipe.tagline)),
            )
        },
        |slug| {
            format!(
                "→ {} {}",
                theme.paint(Role::Accent, &format!("example `{slug}`")),
                theme.paint(Role::Dim, "— verbatim · a lesson carries its own model"),
            )
        },
    );
    writeln!(out, "{}", chrome::rail_line(theme, &picked_line))?;
    Ok(Some((recipe, example)))
}

/// Did the answer pick the example lane? (its number · or the word).
fn is_example_pick(pick: &str) -> bool {
    let n = RECIPES.len() + 1;
    pick.trim() == n.to_string() || pick.trim().eq_ignore_ascii_case("example")
}

/// The example-slug beat — free entry, `01-hello` as the Enter default,
/// validated against the embedded set (P0-8: an unknown slug is SAID and
/// re-asked — it used to fall back to 01-hello after one note, still
/// overriding the human's choice).
fn ask_example_slug(
    input: &mut dyn BufRead,
    out: &mut dyn std::io::Write,
    theme: Theme,
) -> std::io::Result<Option<String>> {
    guided::ask_validated(
        input,
        out,
        theme,
        &format!("example slug {}", theme.paint(Role::Dim, "[01-hello]")),
        "any slug from bare `nika try`",
        |raw| {
            let slug = raw.trim();
            if nika_pack::example(slug).is_some() {
                Some(slug.to_owned())
            } else {
                None
            }
        },
        || "01-hello".to_owned(),
    )
}

/// A menu answer → the recipe. `None` = neither a menu number nor a
/// known recipe name — the ask loop says so and re-asks (Enter never
/// reaches here: the loop maps it to the announced agentic default).
fn resolve_recipe(pick: &str) -> Option<&'static Recipe> {
    if let Ok(n) = pick.parse::<usize>() {
        return n.checked_sub(1).and_then(|i| RECIPES.get(i));
    }
    super::recipes::recipe(pick)
}

/// A menu answer → the canvas theme. `None` = unrecognized (the ask
/// loop says so and re-asks · Enter = skip, mapped by the loop — a
/// theme is an offer, never a toll).
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

/// A menu answer → wire targets (`1 3` · `all`). An EMPTY result on a
/// non-empty answer = unrecognized (the ask loop says so and re-asks ·
/// Enter = skip, mapped by the loop).
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

    /// Two Enters + skip + skip + skip = the golden path: agentic
    /// curriculum · offline mock · no canvas stamp · no wires · no
    /// project file — briefs + 4 workflows on disk, every one audited,
    /// the panel handed back.
    #[test]
    fn golden_path_founds_the_agentic_project() {
        let dir = fresh_dir("golden");
        let d = dir.to_str().expect("utf8");
        // recipe Enter (agentic) · model Enter (mock) · canvas Enter
        // (skip) · agents Enter (skip) · project file Enter (skip)
        let mut input = std::io::Cursor::new(b"\n\n\n\n\n".to_vec());
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

    /// P0-8 · a NON-EMPTY answer the menu doesn't carry is SAID and
    /// RE-ASKED — never a silent default. « débutant » must not found an
    /// agentic project by accident: the wizard says « unrecognized »,
    /// poses the recipe question again, and only the explicit `1` lands
    /// the agentic pick.
    #[test]
    fn an_invalid_recipe_answer_is_said_and_reasked() {
        let dir = fresh_dir("reask");
        let d = dir.to_str().expect("utf8");
        // recipe « débutant » (invalid → re-asked) · 1 (agentic,
        // explicit) · model Enter · canvas Enter · agents Enter ·
        // project file Enter (skip)
        let mut input = std::io::Cursor::new("débutant\n1\n\n\n\n\n".as_bytes().to_vec());
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
        assert!(shown.contains("unrecognized"), "the typo is said: {shown}");
        let after = &shown[shown.find("unrecognized").expect("said")..];
        assert!(
            after.contains("[1 agentic]"),
            "the recipe question is re-posed: {shown}"
        );
        let reask = &after[after.find("[1 agentic]").expect("re-posed")..];
        assert!(
            reask.contains("→ agentic"),
            "agentic lands only AFTER the explicit pick: {shown}"
        );
        assert!(
            dir.join("workflows/01-hello-chain.nika.yaml").exists(),
            "the explicit agentic pick founded the project"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// P0-8 · the loop discipline covers EVERY menu beat: a canvas prose
    /// answer and a wires prose answer are each said + re-asked, Enter
    /// still skips.
    #[test]
    fn invalid_canvas_and_wires_answers_are_said_and_reasked() {
        let dir = fresh_dir("reask2");
        let d = dir.to_str().expect("utf8");
        // recipe 5 (minimal · no model question) · canvas « thème sombre »
        // (invalid → re-asked) · 2 (editor) · agents « aucun » (invalid →
        // re-asked) · Enter (skip) · project file Enter (skip)
        let mut input = std::io::Cursor::new("5\nthème sombre\n2\naucun\n\n\n".as_bytes().to_vec());
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
        assert_eq!(
            shown.matches("unrecognized").count(),
            2,
            "both typos are said: {shown}"
        );
        let settings = std::fs::read_to_string(dir.join(".vscode/settings.json")).expect("written");
        let parsed: serde_json::Value = serde_json::from_str(&settings).expect("valid json");
        assert_eq!(
            parsed.get("nika.dag.theme").and_then(|v| v.as_str()),
            Some("editor"),
            "the re-asked canvas pick lands"
        );
        assert!(
            !v.text.contains("wired to the oracle"),
            "the skipped wires stay skipped: {}",
            v.text
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// P0-8 · an unknown example slug is said + re-asked too (it used to
    /// fall back to 01-hello after one note — the human's choice was
    /// still overridden).
    #[test]
    fn an_unknown_example_slug_is_said_and_reasked() {
        let dir = fresh_dir("reask3");
        let d = dir.to_str().expect("utf8");
        // recipe lane « example » · slug « hello » (unknown → re-asked) ·
        // « 01-hello » · canvas Enter · agents Enter · project file Enter
        let mut input = std::io::Cursor::new(b"example\nhello\n01-hello\n\n\n\n".to_vec());
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
        assert!(shown.contains("unrecognized"), "the typo is said: {shown}");
        assert!(
            shown.contains("→ example `01-hello`"),
            "the re-asked slug lands: {shown}"
        );
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
        // recipe 5 (minimal) · canvas 3 (phosphor) · agents skip ·
        // project file Enter (skip)
        let mut input = std::io::Cursor::new(b"5\n3\n\n\n".to_vec());
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

    /// The wizard lane lays the same trace cover (T1 · one law, two
    /// doors): the row rides the scaffold block, project-relative.
    #[test]
    fn the_wizard_lays_the_traces_cover_too() {
        let dir = fresh_dir("gitignore");
        let d = dir.to_str().expect("utf8");
        // recipe 5 (minimal) · canvas Enter (skip) · agents Enter (skip)
        // · project file Enter (skip)
        let mut input = std::io::Cursor::new(b"5\n\n\n\n".to_vec());
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
            shown.contains("created .gitignore"),
            "the cover row, project-relative: {shown}"
        );
        let body = std::fs::read_to_string(dir.join(".gitignore")).expect("written");
        assert!(body.contains(".nika/traces/"), "the cover entry: {body}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The project-file offer (D-2026-08-11-N5): an explicit `y` lays
    /// the starter and the row rides the founding report — created,
    /// project-relative, next to the gitignore cover.
    #[test]
    fn the_offer_lays_the_starter_on_an_explicit_yes() {
        let dir = fresh_dir("offer-yes");
        let d = dir.to_str().expect("utf8");
        // recipe 5 (minimal) · canvas Enter (skip) · agents Enter (skip)
        // · project file « y » (the offer, taken)
        let mut input = std::io::Cursor::new(b"5\n\n\ny\n".to_vec());
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
            shown.contains("lay a starter nika.yaml"),
            "the question is posed: {shown}"
        );
        assert!(
            shown.contains("created nika.yaml"),
            "the row rides the report: {shown}"
        );
        let laid = std::fs::read_to_string(dir.join("nika.yaml")).expect("written");
        assert!(
            laid.starts_with("# nika.yaml — the project file"),
            "the grammar's starter, verbatim: {laid}"
        );
        // …and what was laid parses back to the defaults (never a
        // silently-governing file).
        let parsed = nika_vocab::project::parse(&laid).expect("the laid starter parses");
        assert_eq!(parsed, nika_vocab::project::Project::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Enter SKIPS the offer — never laid silently, nothing on disk.
    #[test]
    fn enter_skips_the_offer_and_nothing_is_laid() {
        let dir = fresh_dir("offer-skip");
        let d = dir.to_str().expect("utf8");
        // recipe 5 · canvas Enter · agents Enter · project file Enter
        let mut input = std::io::Cursor::new(b"5\n\n\n\n".to_vec());
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
        assert!(shown.contains("lay a starter nika.yaml"), "{shown}");
        assert!(
            !shown.contains("created nika.yaml"),
            "no lay row on a skip: {shown}"
        );
        assert!(!dir.join("nika.yaml").exists(), "never laid silently");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The skip/overwrite law at the wizard: an existing `nika.yaml`
    /// is SKIPPED (its bytes untouched) even when the human says yes;
    /// `--force` is the one override.
    #[test]
    fn an_existing_file_is_skipped_and_force_overrides() {
        // Skip — the human's bytes win over the offer.
        let dir = fresh_dir("offer-exists");
        let d = dir.to_str().expect("utf8");
        std::fs::write(dir.join("nika.yaml"), "nika: v1\nceiling: 9.99\n").expect("seed");
        let mut input = std::io::Cursor::new(b"5\n\n\ny\n".to_vec());
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
            shown.contains("skipped nika.yaml (exists · --force)"),
            "the skip row, project-relative: {shown}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("nika.yaml")).expect("read"),
            "nika: v1\nceiling: 9.99\n",
            "the human's bytes survive the offer"
        );
        std::fs::remove_dir_all(&dir).ok();

        // Force — the founding law's one override.
        let dir = fresh_dir("offer-force");
        let d = dir.to_str().expect("utf8");
        std::fs::write(dir.join("nika.yaml"), "nika: v1\nceiling: 9.99\n").expect("seed");
        let mut input = std::io::Cursor::new(b"5\n\n\ny\n".to_vec());
        let mut out = Vec::new();
        let v = wizard_io(
            d,
            true,
            PLAIN,
            &mut input,
            &mut out,
            &stub_audit,
            &stub_wire,
        );
        assert_eq!(v.code, codes::OK, "{}", v.text);
        let shown = String::from_utf8(out).expect("utf8");
        assert!(shown.contains("created nika.yaml"), "{shown}");
        let laid = std::fs::read_to_string(dir.join("nika.yaml")).expect("read");
        assert!(
            laid.starts_with("# nika.yaml — the project file"),
            "the starter replaced the seed: {laid}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// P0-8 covers the new beat too: a prose answer (« peut-être ») is
    /// SAID and re-asked — it never becomes a silent skip, let alone a
    /// silent lay.
    #[test]
    fn a_prose_answer_to_the_offer_is_said_and_reasked() {
        let dir = fresh_dir("offer-reask");
        let d = dir.to_str().expect("utf8");
        // recipe 5 · canvas Enter · agents Enter · project file
        // « peut-être » (invalid → re-asked) · Enter (skip)
        let mut input = std::io::Cursor::new("5\n\n\npeut-être\n\n".as_bytes().to_vec());
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
        assert!(shown.contains("unrecognized"), "the prose is said: {shown}");
        assert!(
            !dir.join("nika.yaml").exists(),
            "a re-asked offer lays nothing"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolvers_cover_the_menus() {
        // FLIP (P0-8 · 2026-07-31): resolve_recipe is now honest — it
        // returns Option and the ASK LOOP owns the Enter default and the
        // re-ask. The « 99 » line used to PIN the silent agentic
        // fallback (a typo founding the wrong project without a word).
        assert_eq!(resolve_recipe("3").map(|r| r.name), Some("ship"));
        assert_eq!(
            resolve_recipe("ship").map(|r| r.name),
            Some("ship"),
            "names work too"
        );
        assert!(resolve_recipe("").is_none(), "Enter is the loop's default");
        assert!(
            resolve_recipe("99").is_none(),
            "off-menu is said + re-asked, never a silent default"
        );
        assert!(
            resolve_recipe("débutant").is_none(),
            "prose is not a recipe"
        );

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
        // recipe 2 (starter) · canvas skip · agents skip · project file
        // skip · then the new wizard: intent Enter (chain) · file Enter ·
        // model Enter (mock)
        let mut input = std::io::Cursor::new(b"2\n\n\n\n\n\n\n".to_vec());
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
